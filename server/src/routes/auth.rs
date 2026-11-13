//! Sign-in flows: first-run setup, password (+ TOTP), invitations, quick connect.

use std::time::Instant;

use axum::{
    Json,
    extract::State,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    AppState,
    auth::{
        self, ClientIp, CurrentDevice, DeviceInfo, Role, Session,
        crypto::{self, constant_time_eq},
        quick_connect::{self, Poll},
        session_response,
    },
    db::now,
    error::{ApiError, ApiResult},
};

// ---- First-run setup --------------------------------------------------------------

#[derive(Serialize, ToSchema)]
pub struct SetupStatus {
    /// True while no account exists: the first admin must be created with the setup token.
    pub needed: bool,
}

#[utoipa::path(get, path = "/api/setup", tag = "auth",
    responses((status = 200, body = SetupStatus)))]
pub async fn setup_status(State(state): State<AppState>) -> ApiResult<Json<SetupStatus>> {
    let users: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&state.db)
        .await?;
    Ok(Json(SetupStatus { needed: users == 0 }))
}

#[derive(Deserialize, ToSchema)]
pub struct SetupRequest {
    /// Printed in the server logs at startup while no account exists.
    pub setup_token: String,
    pub username: String,
    pub password: String,
    pub device: DeviceInfo,
}

/// Creates the first administrator. They must then enable 2FA before using admin features.
#[utoipa::path(post, path = "/api/setup", tag = "auth", request_body = SetupRequest,
    responses((status = 201, body = Session), (status = 403, body = crate::error::ErrorBody)))]
pub async fn setup(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    Json(req): Json<SetupRequest>,
) -> ApiResult<Response> {
    req.device.validate()?;
    let key = format!("setup:{ip}");
    let limiter = &state.auth.login_limiter;
    if let Err(wait) = limiter.check(&[&key], Instant::now()) {
        return Err(ApiError::TooManyAttempts {
            retry_after_secs: wait.as_secs().max(1),
        });
    }

    // Take the token out first: two concurrent setups can't both succeed.
    let expected = {
        let mut token = state.auth.setup_token.lock().unwrap();
        match token.as_deref() {
            Some(t) if constant_time_eq(t, req.setup_token.trim()) => token.take(),
            Some(_) => None,
            None => {
                return Err(ApiError::forbidden(
                    "setup_done",
                    "setup is already complete",
                ));
            }
        }
    };
    let Some(expected) = expected else {
        limiter.record_failure(&[&key], Instant::now());
        return Err(ApiError::forbidden(
            "invalid_setup_token",
            "invalid setup token",
        ));
    };

    let result = auth::create_user(
        &state.db,
        &req.username,
        &req.password,
        Role::Admin,
        &state.config.default_language,
    )
    .await;
    let user_id = match result {
        Ok(id) => id,
        Err(err) => {
            // Invalid username/password: give the token back so setup can be retried.
            *state.auth.setup_token.lock().unwrap() = Some(expected);
            return Err(err);
        }
    };
    tracing::info!(username = %req.username, "first administrator created, setup complete");
    let session = auth::issue_session(&state.db, user_id, &req.device).await?;
    Ok(session_response(
        session,
        req.device.platform,
        StatusCode::CREATED,
        state.config.secure_cookies,
        |s| serde_json::to_value(s).expect("serializable"),
    ))
}

// ---- Password login -----------------------------------------------------------------

#[derive(Deserialize, ToSchema)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
    pub device: DeviceInfo,
}

#[derive(Serialize, ToSchema)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum LoginResponse {
    /// Signed in.
    Ok(Session),
    /// Password accepted; send a TOTP or recovery code to `/api/auth/login/totp`.
    TotpRequired { challenge: String },
}

fn too_many(wait: std::time::Duration) -> ApiError {
    ApiError::TooManyAttempts {
        retry_after_secs: wait.as_secs().max(1),
    }
}

#[utoipa::path(post, path = "/api/auth/login", tag = "auth", request_body = LoginRequest,
    responses(
        (status = 200, body = LoginResponse),
        (status = 401, body = crate::error::ErrorBody),
        (status = 429, body = crate::error::ErrorBody)))]
pub async fn login(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    Json(req): Json<LoginRequest>,
) -> ApiResult<Response> {
    req.device.validate()?;
    let ip_key = format!("ip:{ip}");
    let user_key = format!("user:{}", req.username.to_lowercase());
    let limiter = &state.auth.login_limiter;
    // Counted before the password is checked: a burst can't outrun the limiter.
    limiter
        .attempt(&[&ip_key, &user_key], Instant::now())
        .map_err(too_many)?;

    let row: Option<(i64, String, bool)> =
        sqlx::query_as("SELECT id, password_hash, totp_enabled FROM users WHERE username = ?")
            .bind(&req.username)
            .fetch_optional(&state.db)
            .await?;

    let hash = row
        .as_ref()
        .map(|(_, h, _)| h.clone())
        .unwrap_or_else(|| crypto::dummy_hash().to_owned());
    let valid = crypto::verify_password(req.password, hash).await;

    let (user_id, totp_enabled) = match row {
        Some((id, _, totp)) if valid => (id, totp),
        _ => return Err(ApiError::InvalidCredentials),
    };
    // Only the account key is cleared: one valid account must not reset an IP's counter,
    // which only gets this attempt back.
    limiter.record_success(&[&user_key]);
    limiter.forgive(&[&ip_key]);

    if totp_enabled {
        let challenge = state.auth.create_challenge(user_id, req.device);
        return Ok(Json(LoginResponse::TotpRequired { challenge }).into_response());
    }
    let session = auth::issue_session(&state.db, user_id, &req.device).await?;
    Ok(session_response(
        session,
        req.device.platform,
        StatusCode::OK,
        state.config.secure_cookies,
        |s| serde_json::to_value(LoginResponse::Ok(s)).expect("serializable"),
    ))
}

#[derive(Deserialize, ToSchema)]
pub struct TotpLoginRequest {
    pub challenge: String,
    /// 6-digit code from the authenticator app, or a recovery code.
    pub code: String,
}

#[utoipa::path(post, path = "/api/auth/login/totp", tag = "auth", request_body = TotpLoginRequest,
    responses(
        (status = 200, body = Session),
        (status = 401, body = crate::error::ErrorBody),
        (status = 403, body = crate::error::ErrorBody),
        (status = 429, body = crate::error::ErrorBody)))]
pub async fn login_totp(
    State(state): State<AppState>,
    Json(req): Json<TotpLoginRequest>,
) -> ApiResult<Response> {
    // Both counted before the code is checked: a burst can't outrun either cap.
    let user_id = state.auth.challenge_attempt(&req.challenge)?;
    let key = format!("totp:{user_id}");
    let limiter = &state.auth.login_limiter;
    limiter.attempt(&[&key], Instant::now()).map_err(too_many)?;

    if !auth::verify_second_factor(&state.db, user_id, &req.code).await? {
        return Err(ApiError::InvalidCredentials);
    }
    limiter.record_success(&[&key]);
    let challenge = state.auth.consume_challenge(&req.challenge)?;
    let session = auth::issue_session(&state.db, user_id, &challenge.device).await?;
    Ok(session_response(
        session,
        challenge.device.platform,
        StatusCode::OK,
        state.config.secure_cookies,
        |s| serde_json::to_value(s).expect("serializable"),
    ))
}

/// Revokes the calling device's token.
#[utoipa::path(post, path = "/api/auth/logout", tag = "auth",
    security(("bearer" = [])), responses((status = 204)))]
pub async fn logout(State(state): State<AppState>, current: CurrentDevice) -> ApiResult<Response> {
    sqlx::query("UPDATE devices SET revoked_at = ? WHERE id = ?")
        .bind(now())
        .bind(current.device_id)
        .execute(&state.db)
        .await?;
    crate::downloads::service::release_revoked_devices(&state).await?;
    Ok((
        StatusCode::NO_CONTENT,
        [(
            header::SET_COOKIE,
            auth::clear_session_cookie(state.config.secure_cookies),
        )],
    )
        .into_response())
}

// ---- Invitations -------------------------------------------------------------------

#[derive(Deserialize, ToSchema)]
pub struct AcceptInvitationRequest {
    pub token: String,
    pub username: String,
    pub password: String,
    pub device: DeviceInfo,
}

/// Creates an account from an invitation link and signs the device in.
#[utoipa::path(post, path = "/api/auth/invitations/accept", tag = "auth",
    request_body = AcceptInvitationRequest,
    responses(
        (status = 201, body = Session),
        (status = 403, body = crate::error::ErrorBody),
        (status = 409, body = crate::error::ErrorBody)))]
pub async fn accept_invitation(
    State(state): State<AppState>,
    Json(req): Json<AcceptInvitationRequest>,
) -> ApiResult<Response> {
    req.device.validate()?;
    let token_hash = crypto::hash_token(req.token.trim());
    let invalid = || ApiError::forbidden("invalid_invitation", "invitation invalid or expired");

    let (invitation_id, role): (i64, Role) = sqlx::query_as(
        "SELECT id, role FROM invitations
         WHERE token_hash = ? AND used_at IS NULL AND expires_at > ?",
    )
    .bind(&token_hash)
    .bind(now())
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(invalid)?;

    // Claim the invitation first so it can't be used twice concurrently.
    let claimed =
        sqlx::query("UPDATE invitations SET used_at = ? WHERE id = ? AND used_at IS NULL")
            .bind(now())
            .bind(invitation_id)
            .execute(&state.db)
            .await?;
    if claimed.rows_affected() != 1 {
        return Err(invalid());
    }

    let created = auth::create_user(
        &state.db,
        &req.username,
        &req.password,
        role,
        &state.config.default_language,
    )
    .await;
    let user_id = match created {
        Ok(id) => id,
        Err(err) => {
            // Release the invitation: the user can retry with another username/password.
            sqlx::query("UPDATE invitations SET used_at = NULL WHERE id = ?")
                .bind(invitation_id)
                .execute(&state.db)
                .await?;
            return Err(err);
        }
    };
    sqlx::query("UPDATE invitations SET used_by = ? WHERE id = ?")
        .bind(user_id)
        .bind(invitation_id)
        .execute(&state.db)
        .await?;

    let session = auth::issue_session(&state.db, user_id, &req.device).await?;
    Ok(session_response(
        session,
        req.device.platform,
        StatusCode::CREATED,
        state.config.secure_cookies,
        |s| serde_json::to_value(s).expect("serializable"),
    ))
}

// ---- Quick connect -----------------------------------------------------------------

#[derive(Deserialize, ToSchema)]
pub struct QuickConnectStartRequest {
    pub device: DeviceInfo,
}

#[derive(Serialize, ToSchema)]
pub struct QuickConnectStarted {
    /// Show this on screen; the user types it on a signed-in device.
    pub code: String,
    /// Keep secret; send it to `/api/auth/quick-connect/poll`.
    pub poll_secret: String,
    pub expires_in_secs: u64,
}

#[utoipa::path(post, path = "/api/auth/quick-connect/start", tag = "auth",
    request_body = QuickConnectStartRequest,
    responses((status = 200, body = QuickConnectStarted), (status = 429, body = crate::error::ErrorBody)))]
pub async fn quick_connect_start(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    Json(req): Json<QuickConnectStartRequest>,
) -> ApiResult<Json<QuickConnectStarted>> {
    req.device.validate()?;
    let started = state
        .auth
        .quick_connect
        .start(req.device, &ip, Instant::now())
        .ok_or(ApiError::TooManyAttempts {
            retry_after_secs: 60,
        })?;
    Ok(Json(QuickConnectStarted {
        code: started.code,
        poll_secret: started.poll_secret,
        expires_in_secs: quick_connect::CODE_TTL.as_secs(),
    }))
}

#[derive(Deserialize, ToSchema)]
pub struct QuickConnectPollRequest {
    pub poll_secret: String,
}

#[derive(Serialize, ToSchema)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum QuickConnectPoll {
    /// Not approved yet: poll again in a few seconds.
    Pending,
    /// Expired or unknown: start over.
    Expired,
    Ok(Session),
}

#[utoipa::path(post, path = "/api/auth/quick-connect/poll", tag = "auth",
    request_body = QuickConnectPollRequest,
    responses((status = 200, body = QuickConnectPoll)))]
pub async fn quick_connect_poll(
    State(state): State<AppState>,
    Json(req): Json<QuickConnectPollRequest>,
) -> ApiResult<Json<QuickConnectPoll>> {
    Ok(Json(
        match state
            .auth
            .quick_connect
            .poll(&req.poll_secret, Instant::now())
        {
            Poll::Pending => QuickConnectPoll::Pending,
            Poll::Expired => QuickConnectPoll::Expired,
            Poll::Approved { user_id, device } => {
                QuickConnectPoll::Ok(auth::issue_session(&state.db, user_id, &device).await?)
            }
        },
    ))
}

#[derive(Deserialize, ToSchema)]
pub struct QuickConnectApproveRequest {
    pub code: String,
}

/// Approves the code shown on another device, signing it in as the caller.
#[utoipa::path(post, path = "/api/auth/quick-connect/approve", tag = "auth",
    security(("bearer" = [])), request_body = QuickConnectApproveRequest,
    responses(
        (status = 204),
        (status = 404, body = crate::error::ErrorBody),
        (status = 429, body = crate::error::ErrorBody)))]
pub async fn quick_connect_approve(
    State(state): State<AppState>,
    current: CurrentDevice,
    Json(req): Json<QuickConnectApproveRequest>,
) -> ApiResult<StatusCode> {
    let key = format!("qc:{}", current.user.id);
    let limiter = &state.auth.login_limiter;
    limiter.check(&[&key], Instant::now()).map_err(too_many)?;
    if state
        .auth
        .quick_connect
        .approve(&req.code, current.user.id, Instant::now())
    {
        limiter.record_success(&[&key]);
        Ok(StatusCode::NO_CONTENT)
    } else {
        limiter.record_failure(&[&key], Instant::now());
        Err(ApiError::NotFound)
    }
}
