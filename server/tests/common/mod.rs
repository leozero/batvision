//! Shared harness: a real router on an in-memory database, driven without a network.
#![allow(dead_code)]

use axum::{
    Router,
    body::Body,
    http::{Method, Request, StatusCode, header},
};
use batvision_server::{AppState, app, config::Config, db};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

pub const PASSWORD: &str = "correct horse battery";

pub struct TestApp {
    pub state: AppState,
    pub router: Router,
}

pub fn test_config() -> Config {
    let mut config = Config::from_lookup(|_| None).unwrap();
    config.trust_proxy = true;
    config
}

impl TestApp {
    pub async fn new() -> Self {
        Self::with_config(test_config()).await
    }

    pub async fn with_config(config: Config) -> Self {
        let db = db::open_in_memory().await.unwrap();
        let state = AppState::new(config, db);
        let router = app(state.clone());
        Self { state, router }
    }

    pub async fn request(
        &self,
        method: Method,
        uri: &str,
        token: Option<&str>,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        self.request_from(method, uri, token, body, "203.0.113.1")
            .await
    }

    /// Same as `request`, from a given client IP (via X-Forwarded-For).
    pub async fn request_from(
        &self,
        method: Method,
        uri: &str,
        token: Option<&str>,
        body: Option<Value>,
        ip: &str,
    ) -> (StatusCode, Value) {
        let mut req = Request::builder()
            .method(method)
            .uri(uri)
            .header("x-forwarded-for", ip);
        if let Some(token) = token {
            req = req.header(header::AUTHORIZATION, format!("Bearer {token}"));
        }
        let req = match body {
            Some(body) => req
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string())),
            None => req.body(Body::empty()),
        }
        .unwrap();
        let res = self.router.clone().oneshot(req).await.unwrap();
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let value = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes)
                .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into()))
        };
        (status, value)
    }

    pub async fn get(&self, uri: &str, token: Option<&str>) -> (StatusCode, Value) {
        self.request(Method::GET, uri, token, None).await
    }

    pub async fn post(&self, uri: &str, token: Option<&str>, body: Value) -> (StatusCode, Value) {
        self.request(Method::POST, uri, token, Some(body)).await
    }

    /// Runs first-run setup and returns the admin's token (2FA not enabled yet).
    pub async fn setup_admin(&self, username: &str) -> String {
        let setup_token = self.state.init_setup_token().await.unwrap().unwrap();
        let (status, body) = self
            .post(
                "/api/setup",
                None,
                json!({
                    "setup_token": setup_token,
                    "username": username,
                    "password": PASSWORD,
                    "device": device("Admin browser", "web"),
                }),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        body["token"].as_str().unwrap().to_owned()
    }

    /// Enables 2FA for the token's user; returns (secret, recovery codes).
    pub async fn enable_totp(&self, token: &str) -> (String, Vec<String>) {
        let (status, setup) = self
            .post(
                "/api/me/totp/setup",
                Some(token),
                json!({ "password": PASSWORD }),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{setup}");
        let secret = setup["secret"].as_str().unwrap().to_owned();
        let (status, codes) = self
            .post(
                "/api/me/totp/enable",
                Some(token),
                json!({ "code": totp_now(&secret) }),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{codes}");
        let codes = codes["codes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c.as_str().unwrap().to_owned())
            .collect();
        (secret, codes)
    }

    /// An admin with 2FA, ready for admin routes. Returns (token, totp secret).
    pub async fn ready_admin(&self) -> (String, String) {
        let token = self.setup_admin("admin").await;
        let (secret, _) = self.enable_totp(&token).await;
        // The enrollment code burned the current step; tests needing another code
        // use `totp_next` to stay valid.
        (token, secret)
    }

    pub async fn create_user(&self, admin_token: &str, username: &str) -> String {
        let (status, body) = self
            .post(
                "/api/admin/users",
                Some(admin_token),
                json!({ "username": username, "password": PASSWORD, "role": "user" }),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        self.login_token(username).await
    }

    /// Password login for a user without 2FA.
    pub async fn login_token(&self, username: &str) -> String {
        let (status, body) = self
            .post(
                "/api/auth/login",
                None,
                json!({ "username": username, "password": PASSWORD, "device": device("Phone", "android") }),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["status"], "ok", "{body}");
        body["token"].as_str().unwrap().to_owned()
    }
}

pub fn device(name: &str, platform: &str) -> Value {
    json!({ "name": name, "platform": platform })
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// Generates a TOTP code with an independent implementation-free path: we reuse
/// the server's secret format but compute the code through `totp-rs` directly.
pub fn totp_at(secret: &str, unix_time: u64) -> String {
    let secret = totp_rs::Secret::try_from_base32(secret).unwrap();
    totp_rs::Builder::new()
        .with_secret(secret)
        .build()
        .unwrap()
        .generate(unix_time)
        .to_string()
}

pub fn totp_now(secret: &str) -> String {
    totp_at(secret, unix_now())
}

/// Code for the next 30 s step: accepted thanks to the ±1 step skew, and newer
/// than a code already burned during the current step.
pub fn totp_next(secret: &str) -> String {
    totp_at(secret, unix_now() + 30)
}
