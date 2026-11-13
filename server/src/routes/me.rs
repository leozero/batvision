//! The signed-in user's own account: profile, password, 2FA, devices.

use std::time::Instant;

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    AppState,
    auth::{self, CurrentDevice, Platform, Role, User, crypto, totp},
    db::now,
    error::{ApiError, ApiResult},
};

#[utoipa::path(get, path = "/api/me", tag = "me", security(("bearer" = [])),
    responses((status = 200, body = User)))]
pub async fn get_me(current: CurrentDevice) -> Json<User> {
    Json(current.user)
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateMeRequest {
    /// TMDB language tag, e.g. `fr-FR`, `en-US`.
    pub language: Option<String>,
}

/// Languages TMDB translates into. A closed list: every distinct language among the
/// users is fetched for every item, so a made-up tag would cost a whole library of
/// requests for nothing.
const LANGUAGES: &[&str] = &[
    "ar-SA", "bg-BG", "ca-ES", "cs-CZ", "da-DK", "de-AT", "de-CH", "de-DE", "el-GR", "en-AU",
    "en-CA", "en-GB", "en-US", "es-ES", "es-MX", "et-EE", "fi-FI", "fr-BE", "fr-CA", "fr-CH",
    "fr-FR", "he-IL", "hi-IN", "hr-HR", "hu-HU", "id-ID", "it-IT", "ja-JP", "ko-KR", "lt-LT",
    "lv-LV", "nb-NO", "nl-BE", "nl-NL", "pl-PL", "pt-BR", "pt-PT", "ro-RO", "ru-RU", "sk-SK",
    "sl-SI", "sr-RS", "sv-SE", "th-TH", "tr-TR", "uk-UA", "vi-VN", "zh-CN", "zh-HK", "zh-TW",
];

pub fn validate_language(language: &str) -> ApiResult<()> {
    if LANGUAGES.contains(&language) {
        Ok(())
    } else {
        Err(ApiError::bad_request(
            "invalid_language",
            "language must look like fr-FR or en-US",
        ))
    }
}

#[utoipa::path(patch, path = "/api/me", tag = "me", security(("bearer" = [])),
    request_body = UpdateMeRequest, responses((status = 200, body = User)))]
pub async fn update_me(
    State(state): State<AppState>,
    current: CurrentDevice,
    Json(req): Json<UpdateMeRequest>,
) -> ApiResult<Json<User>> {
    if let Some(language) = &req.language {
        validate_language(language)?;
        sqlx::query("UPDATE users SET language = ? WHERE id = ?")
            .bind(language)
            .bind(current.user.id)
            .execute(&state.db)
            .await?;
    }
    Ok(Json(auth::load_user(&state.db, current.user.id).await?))
}

/// Re-checks the caller's password for sensitive changes, with brute-force protection.
async fn check_password(state: &AppState, user_id: i64, password: &str) -> ApiResult<()> {
    let key = format!("reauth:{user_id}");
    let limiter = &state.auth.login_limiter;
    if let Err(wait) = limiter.attempt(&[&key], Instant::now()) {
        return Err(ApiError::TooManyAttempts {
            retry_after_secs: wait.as_secs().max(1),
        });
    }
    let hash: String = sqlx::query_scalar("SELECT password_hash FROM users WHERE id = ?")
        .bind(user_id)
        .fetch_one(&state.db)
        .await?;
    if crypto::verify_password(password.to_owned(), hash).await {
        limiter.record_success(&[&key]);
        Ok(())
    } else {
        Err(ApiError::InvalidCredentials)
    }
}

/// Checks a second factor for a signed-in user, under the limiter sign-in uses: a
/// stolen session can't try every code.
async fn check_second_factor(state: &AppState, user_id: i64, code: &str) -> ApiResult<()> {
    let key = format!("totp:{user_id}");
    let limiter = &state.auth.login_limiter;
    if let Err(wait) = limiter.attempt(&[&key], Instant::now()) {
        return Err(ApiError::TooManyAttempts {
            retry_after_secs: wait.as_secs().max(1),
        });
    }
    if auth::verify_second_factor(&state.db, user_id, code).await? {
        limiter.record_success(&[&key]);
        Ok(())
    } else {
        Err(ApiError::InvalidCredentials)
    }
}

/// A TOTP code (not a recovery code), however the app spaced it.
fn is_totp_code(code: &str) -> bool {
    code.chars().filter(|c| !c.is_whitespace()).count() == 6
}

#[derive(Deserialize, ToSchema)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

/// Changes the password and signs out every other device.
#[utoipa::path(put, path = "/api/me/password", tag = "me", security(("bearer" = [])),
    request_body = ChangePasswordRequest,
    responses((status = 204), (status = 401, body = crate::error::ErrorBody)))]
pub async fn change_password(
    State(state): State<AppState>,
    current: CurrentDevice,
    Json(req): Json<ChangePasswordRequest>,
) -> ApiResult<StatusCode> {
    check_password(&state, current.user.id, &req.current_password).await?;
    auth::validate_password(&req.new_password)?;
    let hash = crypto::hash_password(req.new_password).await?;
    let mut tx = state.db.begin().await?;
    sqlx::query("UPDATE users SET password_hash = ? WHERE id = ?")
        .bind(hash)
        .bind(current.user.id)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "UPDATE devices SET revoked_at = ? WHERE user_id = ? AND id != ? AND revoked_at IS NULL",
    )
    .bind(now())
    .bind(current.user.id)
    .bind(current.device_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    crate::downloads::service::release_revoked_devices(&state).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---- 2FA -----------------------------------------------------------------------------

#[derive(Deserialize, ToSchema)]
pub struct TotpSetupRequest {
    pub password: String,
}

#[derive(Serialize, ToSchema)]
pub struct TotpSetup {
    /// Base32 secret, for manual entry.
    pub secret: String,
    /// `otpauth://` URL, to display as a QR code.
    pub otpauth_url: String,
}

/// Starts 2FA enrollment. 2FA becomes active only after `/api/me/totp/enable`.
#[utoipa::path(post, path = "/api/me/totp/setup", tag = "me", security(("bearer" = [])),
    request_body = TotpSetupRequest,
    responses(
        (status = 200, body = TotpSetup),
        (status = 401, body = crate::error::ErrorBody),
        (status = 409, body = crate::error::ErrorBody)))]
pub async fn totp_setup(
    State(state): State<AppState>,
    current: CurrentDevice,
    Json(req): Json<TotpSetupRequest>,
) -> ApiResult<Json<TotpSetup>> {
    if current.user.totp_enabled {
        return Err(ApiError::conflict(
            "totp_already_enabled",
            "2FA is already enabled",
        ));
    }
    check_password(&state, current.user.id, &req.password).await?;
    let secret = totp::new_secret();
    sqlx::query("UPDATE users SET totp_secret = ?, totp_last_step = NULL WHERE id = ?")
        .bind(&secret)
        .bind(current.user.id)
        .execute(&state.db)
        .await?;
    let otpauth_url = totp::otpauth_url(&secret, &current.user.username)?;
    Ok(Json(TotpSetup {
        secret,
        otpauth_url,
    }))
}

#[derive(Deserialize, ToSchema)]
pub struct TotpEnableRequest {
    /// Current code from the authenticator app, proving it was set up correctly.
    pub code: String,
}

#[derive(Serialize, ToSchema)]
pub struct RecoveryCodes {
    /// Shown only once. Each code can be used once instead of a TOTP code.
    pub codes: Vec<String>,
}

#[utoipa::path(post, path = "/api/me/totp/enable", tag = "me", security(("bearer" = [])),
    request_body = TotpEnableRequest,
    responses(
        (status = 200, body = RecoveryCodes),
        (status = 401, body = crate::error::ErrorBody),
        (status = 409, body = crate::error::ErrorBody)))]
pub async fn totp_enable(
    State(state): State<AppState>,
    current: CurrentDevice,
    Json(req): Json<TotpEnableRequest>,
) -> ApiResult<Json<RecoveryCodes>> {
    if current.user.totp_enabled {
        return Err(ApiError::conflict(
            "totp_already_enabled",
            "2FA is already enabled",
        ));
    }
    let has_secret: Option<String> =
        sqlx::query_scalar("SELECT totp_secret FROM users WHERE id = ?")
            .bind(current.user.id)
            .fetch_one(&state.db)
            .await?;
    if has_secret.is_none() {
        return Err(ApiError::conflict(
            "totp_not_set_up",
            "call /api/me/totp/setup first",
        ));
    }
    // Only TOTP codes here: no recovery codes exist yet.
    if !is_totp_code(&req.code) {
        return Err(ApiError::InvalidCredentials);
    }
    check_second_factor(&state, current.user.id, &req.code).await?;
    sqlx::query("UPDATE users SET totp_enabled = 1 WHERE id = ?")
        .bind(current.user.id)
        .execute(&state.db)
        .await?;
    let codes = auth::regenerate_recovery_codes(&state.db, current.user.id).await?;
    Ok(Json(RecoveryCodes { codes }))
}

#[derive(Deserialize, ToSchema)]
pub struct TotpDisableRequest {
    pub password: String,
    pub code: String,
}

/// Disables 2FA. Not allowed for administrators, for whom it is mandatory.
#[utoipa::path(post, path = "/api/me/totp/disable", tag = "me", security(("bearer" = [])),
    request_body = TotpDisableRequest,
    responses(
        (status = 204),
        (status = 401, body = crate::error::ErrorBody),
        (status = 403, body = crate::error::ErrorBody)))]
pub async fn totp_disable(
    State(state): State<AppState>,
    current: CurrentDevice,
    Json(req): Json<TotpDisableRequest>,
) -> ApiResult<StatusCode> {
    if current.user.role == Role::Admin {
        return Err(ApiError::forbidden(
            "totp_mandatory",
            "two-factor authentication is mandatory for administrators",
        ));
    }
    if !current.user.totp_enabled {
        return Ok(StatusCode::NO_CONTENT);
    }
    check_password(&state, current.user.id, &req.password).await?;
    check_second_factor(&state, current.user.id, &req.code).await?;
    let mut tx = state.db.begin().await?;
    sqlx::query(
        "UPDATE users SET totp_enabled = 0, totp_secret = NULL, totp_last_step = NULL WHERE id = ?",
    )
    .bind(current.user.id)
    .execute(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM recovery_codes WHERE user_id = ?")
        .bind(current.user.id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize, ToSchema)]
pub struct RegenerateRecoveryCodesRequest {
    /// Current TOTP code.
    pub code: String,
}

/// Replaces all recovery codes (for example after using several of them).
#[utoipa::path(post, path = "/api/me/totp/recovery-codes", tag = "me",
    security(("bearer" = [])), request_body = RegenerateRecoveryCodesRequest,
    responses((status = 200, body = RecoveryCodes), (status = 401, body = crate::error::ErrorBody)))]
pub async fn regenerate_recovery_codes(
    State(state): State<AppState>,
    current: CurrentDevice,
    Json(req): Json<RegenerateRecoveryCodesRequest>,
) -> ApiResult<Json<RecoveryCodes>> {
    if !current.user.totp_enabled || !is_totp_code(&req.code) {
        return Err(ApiError::InvalidCredentials);
    }
    check_second_factor(&state, current.user.id, &req.code).await?;
    let codes = auth::regenerate_recovery_codes(&state.db, current.user.id).await?;
    Ok(Json(RecoveryCodes { codes }))
}

// ---- Devices -------------------------------------------------------------------------

#[derive(Serialize, ToSchema, sqlx::FromRow)]
pub struct DeviceSummary {
    pub id: i64,
    pub name: String,
    pub platform: Platform,
    pub created_at: i64,
    pub last_seen_at: i64,
    /// The device making this request.
    #[sqlx(skip)]
    pub current: bool,
}

#[utoipa::path(get, path = "/api/me/devices", tag = "me", security(("bearer" = [])),
    responses((status = 200, body = Vec<DeviceSummary>)))]
pub async fn list_devices(
    State(state): State<AppState>,
    current: CurrentDevice,
) -> ApiResult<Json<Vec<DeviceSummary>>> {
    let mut devices: Vec<DeviceSummary> = sqlx::query_as(
        "SELECT id, name, platform, created_at, last_seen_at FROM devices
         WHERE user_id = ? AND revoked_at IS NULL ORDER BY last_seen_at DESC",
    )
    .bind(current.user.id)
    .fetch_all(&state.db)
    .await?;
    for device in &mut devices {
        device.current = device.id == current.device_id;
    }
    Ok(Json(devices))
}

/// Signs a device out. Its token stops working immediately.
#[utoipa::path(delete, path = "/api/me/devices/{id}", tag = "me", security(("bearer" = [])),
    params(("id" = i64, Path)),
    responses((status = 204), (status = 404, body = crate::error::ErrorBody)))]
pub async fn revoke_device(
    State(state): State<AppState>,
    current: CurrentDevice,
    Path(id): Path<i64>,
) -> ApiResult<StatusCode> {
    let result = sqlx::query(
        "UPDATE devices SET revoked_at = ? WHERE id = ? AND user_id = ? AND revoked_at IS NULL",
    )
    .bind(now())
    .bind(id)
    .bind(current.user.id)
    .execute(&state.db)
    .await?;
    if result.rows_affected() == 1 {
        crate::downloads::service::release_revoked_devices(&state).await?;
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound)
    }
}
