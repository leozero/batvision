//! Account administration. Every route requires an admin with 2FA enabled.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    AppState,
    auth::{self, Admin, Role, User, crypto},
    db::now,
    error::{ApiError, ApiResult},
};

#[utoipa::path(get, path = "/api/admin/users", tag = "admin", security(("bearer" = [])),
    responses((status = 200, body = Vec<User>)))]
pub async fn list_users(State(state): State<AppState>, _: Admin) -> ApiResult<Json<Vec<User>>> {
    Ok(Json(
        sqlx::query_as(
            "SELECT id, username, role, language, totp_enabled FROM users ORDER BY username",
        )
        .fetch_all(&state.db)
        .await?,
    ))
}

#[derive(Deserialize, ToSchema)]
pub struct CreateUserRequest {
    pub username: String,
    pub password: String,
    pub role: Role,
}

#[utoipa::path(post, path = "/api/admin/users", tag = "admin", security(("bearer" = [])),
    request_body = CreateUserRequest,
    responses((status = 201, body = User), (status = 409, body = crate::error::ErrorBody)))]
pub async fn create_user(
    State(state): State<AppState>,
    _: Admin,
    Json(req): Json<CreateUserRequest>,
) -> ApiResult<(StatusCode, Json<User>)> {
    let id = auth::create_user(
        &state.db,
        &req.username,
        &req.password,
        req.role,
        &state.config.default_language,
    )
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(auth::load_user(&state.db, id).await?),
    ))
}

#[utoipa::path(delete, path = "/api/admin/users/{id}", tag = "admin", security(("bearer" = [])),
    params(("id" = i64, Path)),
    responses(
        (status = 204),
        (status = 404, body = crate::error::ErrorBody),
        (status = 409, body = crate::error::ErrorBody)))]
pub async fn delete_user(
    State(state): State<AppState>,
    Admin(admin): Admin,
    Path(id): Path<i64>,
) -> ApiResult<StatusCode> {
    if id == admin.user.id {
        return Err(ApiError::conflict(
            "cannot_delete_self",
            "you cannot delete your own account",
        ));
    }
    let result = sqlx::query("DELETE FROM users WHERE id = ?")
        .bind(id)
        .execute(&state.db)
        .await?;
    if result.rows_affected() == 1 {
        // Its devices' requests went with it: their encodes have nobody left.
        crate::downloads::service::release_revoked_devices(&state).await?;
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound)
    }
}

/// Another account, checked to exist: administrators change their own from their
/// account page, where their password is asked.
async fn other_user(state: &AppState, admin: &auth::CurrentDevice, id: i64) -> ApiResult<()> {
    if id == admin.user.id {
        return Err(ApiError::conflict(
            "cannot_reset_self",
            "change your own password and 2FA from your account",
        ));
    }
    let exists: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM users WHERE id = ?)")
        .bind(id)
        .fetch_one(&state.db)
        .await?;
    if exists {
        Ok(())
    } else {
        Err(ApiError::NotFound)
    }
}

#[derive(Serialize, ToSchema)]
pub struct TemporaryPassword {
    /// Shown once: give it to the user, who then chooses their own.
    pub password: String,
}

/// Gives a locked-out user a temporary password and signs out their devices.
#[utoipa::path(post, path = "/api/admin/users/{id}/password", tag = "admin",
    security(("bearer" = [])), params(("id" = i64, Path)),
    responses(
        (status = 200, body = TemporaryPassword),
        (status = 404, body = crate::error::ErrorBody),
        (status = 409, body = crate::error::ErrorBody)))]
pub async fn reset_user_password(
    State(state): State<AppState>,
    Admin(admin): Admin,
    Path(id): Path<i64>,
) -> ApiResult<Json<TemporaryPassword>> {
    other_user(&state, &admin, id).await?;
    let password = auth::reset_password(&state.db, id).await?;
    crate::downloads::service::release_revoked_devices(&state).await?;
    Ok(Json(TemporaryPassword { password }))
}

/// Removes a user's second factor: for a lost phone with no recovery code left.
#[utoipa::path(delete, path = "/api/admin/users/{id}/totp", tag = "admin",
    security(("bearer" = [])), params(("id" = i64, Path)),
    responses(
        (status = 204),
        (status = 404, body = crate::error::ErrorBody),
        (status = 409, body = crate::error::ErrorBody)))]
pub async fn reset_user_totp(
    State(state): State<AppState>,
    Admin(admin): Admin,
    Path(id): Path<i64>,
) -> ApiResult<StatusCode> {
    other_user(&state, &admin, id).await?;
    auth::reset_totp(&state.db, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize, ToSchema)]
pub struct CreateInvitationRequest {
    pub role: Role,
    /// Validity in hours (default 72, max 720).
    pub valid_hours: Option<u32>,
}

#[derive(Serialize, ToSchema)]
pub struct InvitationCreated {
    pub id: i64,
    /// Shown only once. The client builds the invitation link from it.
    pub token: String,
    pub expires_at: i64,
}

#[utoipa::path(post, path = "/api/admin/invitations", tag = "admin", security(("bearer" = [])),
    request_body = CreateInvitationRequest,
    responses((status = 201, body = InvitationCreated)))]
pub async fn create_invitation(
    State(state): State<AppState>,
    Admin(admin): Admin,
    Json(req): Json<CreateInvitationRequest>,
) -> ApiResult<(StatusCode, Json<InvitationCreated>)> {
    let hours = req.valid_hours.unwrap_or(72);
    if !(1..=720).contains(&hours) {
        return Err(ApiError::bad_request(
            "invalid_validity",
            "valid_hours must be between 1 and 720",
        ));
    }
    let token = crypto::random_token("bvi_");
    let created_at = now();
    let expires_at = created_at + i64::from(hours) * 3600;
    let id = sqlx::query_scalar(
        "INSERT INTO invitations (token_hash, role, created_by, created_at, expires_at)
         VALUES (?, ?, ?, ?, ?) RETURNING id",
    )
    .bind(crypto::hash_token(&token))
    .bind(req.role)
    .bind(admin.user.id)
    .bind(created_at)
    .bind(expires_at)
    .fetch_one(&state.db)
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(InvitationCreated {
            id,
            token,
            expires_at,
        }),
    ))
}

#[derive(Serialize, ToSchema, sqlx::FromRow)]
pub struct Invitation {
    pub id: i64,
    pub role: Role,
    pub created_at: i64,
    pub expires_at: i64,
    pub used_at: Option<i64>,
    pub used_by: Option<i64>,
}

#[utoipa::path(get, path = "/api/admin/invitations", tag = "admin", security(("bearer" = [])),
    responses((status = 200, body = Vec<Invitation>)))]
pub async fn list_invitations(
    State(state): State<AppState>,
    _: Admin,
) -> ApiResult<Json<Vec<Invitation>>> {
    Ok(Json(
        sqlx::query_as(
            "SELECT id, role, created_at, expires_at, used_at, used_by FROM invitations
             ORDER BY created_at DESC",
        )
        .fetch_all(&state.db)
        .await?,
    ))
}

#[utoipa::path(delete, path = "/api/admin/invitations/{id}", tag = "admin",
    security(("bearer" = [])), params(("id" = i64, Path)),
    responses((status = 204), (status = 404, body = crate::error::ErrorBody)))]
pub async fn delete_invitation(
    State(state): State<AppState>,
    _: Admin,
    Path(id): Path<i64>,
) -> ApiResult<StatusCode> {
    let result = sqlx::query("DELETE FROM invitations WHERE id = ?")
        .bind(id)
        .execute(&state.db)
        .await?;
    if result.rows_affected() == 1 {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound)
    }
}
