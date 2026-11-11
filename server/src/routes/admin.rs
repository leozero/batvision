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
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound)
    }
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
