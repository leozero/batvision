//! Accounts, device sessions, 2FA and quick connect.

pub mod crypto;
pub mod quick_connect;
pub mod rate_limit;
pub mod totp;

use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    sync::Mutex,
    time::{Duration, Instant},
};

use axum::{
    extract::{ConnectInfo, FromRequestParts},
    http::{header, request::Parts},
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    AppState,
    db::{Db, now},
    error::{ApiError, ApiResult},
};

pub const DEVICE_TOKEN_PREFIX: &str = "bvd_";
const CHALLENGE_TTL: Duration = Duration::from_secs(5 * 60);
const CHALLENGE_MAX_ATTEMPTS: u32 = 5;

// ---- Models ---------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "TEXT", rename_all = "lowercase")]
pub enum Role {
    Admin,
    User,
}

#[derive(Debug, Clone, Serialize, ToSchema, sqlx::FromRow)]
pub struct User {
    pub id: i64,
    pub username: String,
    pub role: Role,
    /// TMDB language tag for metadata (`fr-FR`, `en-US`…).
    pub language: String,
    pub totp_enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "TEXT", rename_all = "snake_case")]
pub enum Platform {
    Web,
    Desktop,
    Android,
    AndroidTv,
    Other,
}

/// What a client says about itself when signing in.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct DeviceInfo {
    /// Human-readable name shown in the device list ("Pixel 9", "Firefox – Mac").
    pub name: String,
    pub platform: Platform,
}

impl DeviceInfo {
    pub fn validate(&self) -> ApiResult<()> {
        let len = self.name.trim().chars().count();
        if len == 0 || len > 64 {
            return Err(ApiError::bad_request(
                "invalid_device_name",
                "device name must be 1 to 64 characters",
            ));
        }
        Ok(())
    }
}

/// Returned when a device is signed in. The token is shown only once.
#[derive(Debug, Serialize, ToSchema)]
pub struct Session {
    /// Send as `Authorization: Bearer <token>`.
    pub token: String,
    pub device_id: i64,
    pub user: User,
}

// ---- Validation -----------------------------------------------------------------

pub fn validate_username(username: &str) -> ApiResult<()> {
    let ok = (3..=32).contains(&username.len())
        && username
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'));
    if ok {
        Ok(())
    } else {
        Err(ApiError::bad_request(
            "invalid_username",
            "username must be 3 to 32 characters: letters, digits, '_', '-', '.'",
        ))
    }
}

pub fn validate_password(password: &str) -> ApiResult<()> {
    let len = password.chars().count();
    if (10..=256).contains(&len) {
        Ok(())
    } else {
        Err(ApiError::bad_request(
            "weak_password",
            "password must be 10 to 256 characters",
        ))
    }
}

// ---- Shared auth state ------------------------------------------------------------

/// A password-validated login waiting for its TOTP code.
#[derive(Debug)]
pub struct Challenge {
    pub user_id: i64,
    pub device: DeviceInfo,
    pub expires_at: Instant,
    pub attempts: u32,
}

#[derive(Default)]
pub struct AuthState {
    pub login_limiter: rate_limit::RateLimiter,
    /// Keyed by the hash of the challenge token.
    challenges: Mutex<HashMap<String, Challenge>>,
    pub quick_connect: quick_connect::QuickConnect,
    /// One-time token printed in the logs while no account exists.
    pub setup_token: Mutex<Option<String>>,
}

impl AuthState {
    pub fn create_challenge(&self, user_id: i64, device: DeviceInfo) -> String {
        let token = crypto::random_token("bvc_");
        let now = Instant::now();
        let mut challenges = self.challenges.lock().unwrap();
        challenges.retain(|_, c| c.expires_at > now);
        challenges.insert(
            crypto::hash_token(&token),
            Challenge {
                user_id,
                device,
                expires_at: now + CHALLENGE_TTL,
                attempts: 0,
            },
        );
        token
    }

    /// User behind a live challenge.
    pub fn challenge_user(&self, token: &str) -> ApiResult<i64> {
        let key = crypto::hash_token(token);
        let mut challenges = self.challenges.lock().unwrap();
        match challenges.get(&key) {
            Some(c) if c.expires_at > Instant::now() => Ok(c.user_id),
            Some(_) => {
                challenges.remove(&key);
                Err(challenge_expired())
            }
            None => Err(challenge_expired()),
        }
    }

    /// Consumes the challenge after a successful second factor.
    pub fn consume_challenge(&self, token: &str) -> ApiResult<Challenge> {
        self.challenges
            .lock()
            .unwrap()
            .remove(&crypto::hash_token(token))
            .ok_or_else(challenge_expired)
    }

    /// Counts a wrong code; the challenge dies after too many.
    pub fn fail_challenge(&self, token: &str) {
        let key = crypto::hash_token(token);
        let mut challenges = self.challenges.lock().unwrap();
        if let Some(c) = challenges.get_mut(&key) {
            c.attempts += 1;
            if c.attempts >= CHALLENGE_MAX_ATTEMPTS {
                challenges.remove(&key);
            }
        }
    }
}

fn challenge_expired() -> ApiError {
    ApiError::forbidden("challenge_expired", "sign in again")
}

// ---- Device sessions --------------------------------------------------------------

/// Registers a device for `user_id` and returns its session (with the only copy of the token).
pub async fn issue_session(db: &Db, user_id: i64, device: &DeviceInfo) -> ApiResult<Session> {
    let token = crypto::random_token(DEVICE_TOKEN_PREFIX);
    let now = now();
    let device_id: i64 = sqlx::query_scalar(
        "INSERT INTO devices (user_id, name, platform, token_hash, created_at, last_seen_at)
         VALUES (?, ?, ?, ?, ?, ?) RETURNING id",
    )
    .bind(user_id)
    .bind(device.name.trim())
    .bind(device.platform)
    .bind(crypto::hash_token(&token))
    .bind(now)
    .bind(now)
    .fetch_one(db)
    .await?;
    let user = load_user(db, user_id).await?;
    Ok(Session {
        token,
        device_id,
        user,
    })
}

pub async fn load_user(db: &Db, user_id: i64) -> ApiResult<User> {
    Ok(
        sqlx::query_as("SELECT id, username, role, language, totp_enabled FROM users WHERE id = ?")
            .bind(user_id)
            .fetch_one(db)
            .await?,
    )
}

/// The authenticated device making the request.
#[derive(Debug, Clone)]
pub struct CurrentDevice {
    pub device_id: i64,
    pub user: User,
}

impl FromRequestParts<AppState> for CurrentDevice {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> ApiResult<Self> {
        let token = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .ok_or(ApiError::Unauthenticated)?;

        #[derive(sqlx::FromRow)]
        struct Row {
            device_id: i64,
            last_seen_at: i64,
            id: i64,
            username: String,
            role: Role,
            language: String,
            totp_enabled: bool,
        }
        let row: Row = sqlx::query_as(
            "SELECT d.id AS device_id, d.last_seen_at, u.id, u.username, u.role, u.language, u.totp_enabled
             FROM devices d JOIN users u ON u.id = d.user_id
             WHERE d.token_hash = ? AND d.revoked_at IS NULL",
        )
        .bind(crypto::hash_token(token))
        .fetch_optional(&state.db)
        .await?
        .ok_or(ApiError::Unauthenticated)?;

        // Throttled to avoid a write on every request.
        let now = now();
        if now - row.last_seen_at > 60 {
            sqlx::query("UPDATE devices SET last_seen_at = ? WHERE id = ?")
                .bind(now)
                .bind(row.device_id)
                .execute(&state.db)
                .await?;
        }

        Ok(Self {
            device_id: row.device_id,
            user: User {
                id: row.id,
                username: row.username,
                role: row.role,
                language: row.language,
                totp_enabled: row.totp_enabled,
            },
        })
    }
}

/// An admin with 2FA enabled. Admins without 2FA must enroll before anything else.
#[derive(Debug, Clone)]
pub struct Admin(pub CurrentDevice);

impl FromRequestParts<AppState> for Admin {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> ApiResult<Self> {
        let current = CurrentDevice::from_request_parts(parts, state).await?;
        if current.user.role != Role::Admin {
            return Err(ApiError::forbidden(
                "admin_only",
                "administrator access required",
            ));
        }
        if !current.user.totp_enabled {
            return Err(ApiError::forbidden(
                "totp_enrollment_required",
                "administrators must enable two-factor authentication first",
            ));
        }
        Ok(Self(current))
    }
}

/// Client IP, from the socket or, behind a trusted reverse proxy, from the
/// rightmost `X-Forwarded-For` entry (the one the proxy itself appended).
#[derive(Debug, Clone)]
pub struct ClientIp(pub String);

impl FromRequestParts<AppState> for ClientIp {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        if state.config.trust_proxy
            && let Some(ip) = parts
                .headers
                .get("x-forwarded-for")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.rsplit(',').next())
                .and_then(|v| v.trim().parse::<IpAddr>().ok())
        {
            return Ok(Self(ip.to_string()));
        }
        let ip = parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .map(|ConnectInfo(addr)| addr.ip().to_string())
            .unwrap_or_else(|| "unknown".to_owned());
        Ok(Self(ip))
    }
}

// ---- Second factor ------------------------------------------------------------------

/// Checks a TOTP code (6 digits) or an unused recovery code for `user_id`, and
/// burns it: a TOTP step or recovery code is accepted only once.
pub async fn verify_second_factor(db: &Db, user_id: i64, code: &str) -> ApiResult<bool> {
    let digits: String = code.chars().filter(|c| !c.is_whitespace()).collect();
    if digits.len() == 6 && digits.chars().all(|c| c.is_ascii_digit()) {
        let row: Option<(Option<String>, Option<i64>)> =
            sqlx::query_as("SELECT totp_secret, totp_last_step FROM users WHERE id = ?")
                .bind(user_id)
                .fetch_optional(db)
                .await?;
        let Some((Some(secret), last_step)) = row else {
            return Ok(false);
        };
        let Some(step) = totp::verify(&secret, &digits, now() as u64, last_step) else {
            return Ok(false);
        };
        // Conditional update: two concurrent requests with the same code can't both win.
        let updated = sqlx::query(
            "UPDATE users SET totp_last_step = ?
             WHERE id = ? AND (totp_last_step IS NULL OR totp_last_step < ?)",
        )
        .bind(step)
        .bind(user_id)
        .bind(step)
        .execute(db)
        .await?;
        return Ok(updated.rows_affected() == 1);
    }

    let normalized = totp::normalize_recovery_code(code);
    if normalized.len() != 12 {
        return Ok(false);
    }
    let updated = sqlx::query(
        "UPDATE recovery_codes SET used_at = ?
         WHERE user_id = ? AND code_hash = ? AND used_at IS NULL",
    )
    .bind(now())
    .bind(user_id)
    .bind(crypto::hash_token(&normalized))
    .execute(db)
    .await?;
    Ok(updated.rows_affected() == 1)
}

/// Replaces the user's recovery codes and returns the new ones (shown once).
pub async fn regenerate_recovery_codes(db: &Db, user_id: i64) -> ApiResult<Vec<String>> {
    let codes = totp::new_recovery_codes(10);
    let mut tx = db.begin().await?;
    sqlx::query("DELETE FROM recovery_codes WHERE user_id = ?")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    for code in &codes {
        sqlx::query("INSERT INTO recovery_codes (user_id, code_hash) VALUES (?, ?)")
            .bind(user_id)
            .bind(crypto::hash_token(&totp::normalize_recovery_code(code)))
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(codes)
}

/// Creates an account. Fails with 409 if the username is taken.
pub async fn create_user(
    db: &Db,
    username: &str,
    password: &str,
    role: Role,
    language: &str,
) -> ApiResult<i64> {
    validate_username(username)?;
    validate_password(password)?;
    let hash = crypto::hash_password(password.to_owned()).await?;
    let result = sqlx::query_scalar(
        "INSERT INTO users (username, password_hash, role, language, created_at)
         VALUES (?, ?, ?, ?, ?) RETURNING id",
    )
    .bind(username)
    .bind(hash)
    .bind(role)
    .bind(language)
    .bind(now())
    .fetch_one(db)
    .await;
    match result {
        Ok(id) => Ok(id),
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => Err(ApiError::conflict(
            "username_taken",
            "this username is already taken",
        )),
        Err(e) => Err(e.into()),
    }
}
