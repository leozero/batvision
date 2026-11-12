mod common;

use axum::http::{Method, StatusCode};
use common::{PASSWORD, TestApp, device, totp_next, totp_now};
use serde_json::json;

// ---- Setup ----------------------------------------------------------------------------

#[tokio::test]
async fn setup_is_needed_until_first_admin_exists() {
    let app = TestApp::new().await;
    let (_, status) = app.get("/api/setup", None).await;
    assert_eq!(status["needed"], true);

    let token = app.setup_admin("admin").await;
    let (_, me) = app.get("/api/me", Some(&token)).await;
    assert_eq!(me["role"], "admin");
    assert_eq!(me["totp_enabled"], false);

    let (_, status) = app.get("/api/setup", None).await;
    assert_eq!(status["needed"], false);
}

#[tokio::test]
async fn setup_rejects_wrong_token_and_cannot_run_twice() {
    let app = TestApp::new().await;
    let real = app.state.init_setup_token().await.unwrap().unwrap();
    let body = |token: &str| {
        json!({ "setup_token": token, "username": "admin", "password": PASSWORD,
                "device": device("web", "web") })
    };
    let (status, err) = app.post("/api/setup", None, body("WRONG")).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(err["code"], "invalid_setup_token");

    let (status, _) = app.post("/api/setup", None, body(&real)).await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, err) = app.post("/api/setup", None, body(&real)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(err["code"], "setup_done");
}

#[tokio::test]
async fn setup_with_weak_password_keeps_token_usable() {
    let app = TestApp::new().await;
    let token = app.state.init_setup_token().await.unwrap().unwrap();
    let (status, err) = app
        .post(
            "/api/setup",
            None,
            json!({ "setup_token": token, "username": "admin", "password": "short",
                    "device": device("web", "web") }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(err["code"], "weak_password");
    let (status, _) = app
        .post(
            "/api/setup",
            None,
            json!({ "setup_token": token, "username": "admin", "password": PASSWORD,
                    "device": device("web", "web") }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
}

// ---- Admin 2FA enforcement -------------------------------------------------------------

#[tokio::test]
async fn admin_routes_require_2fa_enrollment() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin").await;
    let (status, err) = app.get("/api/admin/users", Some(&token)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(err["code"], "totp_enrollment_required");

    app.enable_totp(&token).await;
    let (status, users) = app.get("/api/admin/users", Some(&token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(users.as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn regular_users_cannot_use_admin_routes() {
    let app = TestApp::new().await;
    let (admin, _) = app.ready_admin().await;
    let user = app.create_user(&admin, "alice").await;
    let (status, err) = app.get("/api/admin/users", Some(&user)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(err["code"], "admin_only");
}

#[tokio::test]
async fn admins_cannot_disable_2fa() {
    let app = TestApp::new().await;
    let (admin, secret) = app.ready_admin().await;
    let (status, err) = app
        .post(
            "/api/me/totp/disable",
            Some(&admin),
            json!({ "password": PASSWORD, "code": totp_next(&secret) }),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(err["code"], "totp_mandatory");
}

// ---- Login ------------------------------------------------------------------------------

#[tokio::test]
async fn login_without_2fa_returns_a_session() {
    let app = TestApp::new().await;
    let (admin, _) = app.ready_admin().await;
    let token = app.create_user(&admin, "alice").await;
    assert!(token.starts_with("bvd_"));
    let (status, me) = app.get("/api/me", Some(&token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(me["username"], "alice");
}

#[tokio::test]
async fn username_is_case_insensitive() {
    let app = TestApp::new().await;
    let (admin, _) = app.ready_admin().await;
    app.create_user(&admin, "Alice").await;
    app.login_token("alice").await;
    let (status, err) = app
        .post(
            "/api/admin/users",
            Some(&admin),
            json!({ "username": "ALICE", "password": PASSWORD, "role": "user" }),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(err["code"], "username_taken");
}

#[tokio::test]
async fn wrong_password_and_unknown_user_look_the_same() {
    let app = TestApp::new().await;
    app.setup_admin("admin").await;
    let login = |username: &str, password: &str| json!({ "username": username, "password": password, "device": device("x", "web") });
    let (s1, e1) = app
        .post("/api/auth/login", None, login("admin", "wrong password!"))
        .await;
    let (s2, e2) = app
        .post("/api/auth/login", None, login("nobody", "wrong password!"))
        .await;
    assert_eq!(s1, StatusCode::UNAUTHORIZED);
    assert_eq!((s1, &e1), (s2, &e2));
    assert_eq!(e1["code"], "invalid_credentials");
}

#[tokio::test]
async fn repeated_failures_lock_the_account() {
    let app = TestApp::new().await;
    app.setup_admin("admin").await;
    let bad =
        json!({ "username": "admin", "password": "wrong password!", "device": device("x", "web") });
    for i in 0..5 {
        // Different IPs: the per-account counter is what locks.
        let (status, _) = app
            .request_from(
                Method::POST,
                "/api/auth/login",
                None,
                Some(bad.clone()),
                &format!("198.51.100.{i}"),
            )
            .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }
    let good = json!({ "username": "admin", "password": PASSWORD, "device": device("x", "web") });
    let (status, err) = app
        .request_from(
            Method::POST,
            "/api/auth/login",
            None,
            Some(good),
            "198.51.100.200",
        )
        .await;
    assert_eq!(
        status,
        StatusCode::TOO_MANY_REQUESTS,
        "even the right password waits"
    );
    assert_eq!(err["code"], "too_many_attempts");
}

#[tokio::test]
async fn repeated_failures_lock_the_ip() {
    let app = TestApp::new().await;
    app.setup_admin("admin").await;
    for i in 0..5 {
        let bad = json!({ "username": format!("user{i}"), "password": "wrong password!",
                          "device": device("x", "web") });
        app.post("/api/auth/login", None, bad).await;
    }
    let good = json!({ "username": "admin", "password": PASSWORD, "device": device("x", "web") });
    let (status, _) = app.post("/api/auth/login", None, good.clone()).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    // Another IP is not affected.
    let (status, _) = app
        .request_from(
            Method::POST,
            "/api/auth/login",
            None,
            Some(good),
            "192.0.2.50",
        )
        .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn login_with_2fa_requires_a_code() {
    let app = TestApp::new().await;
    let (_, secret) = app.ready_admin().await;
    let (status, body) = app
        .post(
            "/api/auth/login",
            None,
            json!({ "username": "admin", "password": PASSWORD, "device": device("TV", "android_tv") }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "totp_required");
    let challenge = body["challenge"].as_str().unwrap();

    let (status, _) = app
        .post(
            "/api/auth/login/totp",
            None,
            json!({ "challenge": challenge, "code": "000000" }),
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, session) = app
        .post(
            "/api/auth/login/totp",
            None,
            json!({ "challenge": challenge, "code": totp_next(&secret) }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{session}");
    assert_eq!(session["user"]["username"], "admin");

    // The challenge is single-use.
    let (status, err) = app
        .post(
            "/api/auth/login/totp",
            None,
            json!({ "challenge": challenge, "code": totp_next(&secret) }),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(err["code"], "challenge_expired");
}

#[tokio::test]
async fn a_totp_code_cannot_be_replayed() {
    let app = TestApp::new().await;
    let (_, secret) = app.ready_admin().await;
    let code = totp_next(&secret);
    let start =
        || json!({ "username": "admin", "password": PASSWORD, "device": device("x", "web") });

    let (_, first) = app.post("/api/auth/login", None, start()).await;
    let (status, _) = app
        .post(
            "/api/auth/login/totp",
            None,
            json!({ "challenge": first["challenge"], "code": code }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    let (_, second) = app.post("/api/auth/login", None, start()).await;
    let (status, _) = app
        .post(
            "/api/auth/login/totp",
            None,
            json!({ "challenge": second["challenge"], "code": code }),
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn recovery_codes_work_once() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin").await;
    let (_, codes) = app.enable_totp(&token).await;
    assert_eq!(codes.len(), 10);
    let start =
        || json!({ "username": "admin", "password": PASSWORD, "device": device("x", "web") });

    let (_, login) = app.post("/api/auth/login", None, start()).await;
    let (status, _) = app
        .post(
            "/api/auth/login/totp",
            None,
            json!({ "challenge": login["challenge"], "code": codes[0].to_uppercase() }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    let (_, login) = app.post("/api/auth/login", None, start()).await;
    let (status, _) = app
        .post(
            "/api/auth/login/totp",
            None,
            json!({ "challenge": login["challenge"], "code": codes[0] }),
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn challenge_dies_after_too_many_wrong_codes() {
    let app = TestApp::new().await;
    let (_, secret) = app.ready_admin().await;
    let (_, login) = app
        .post(
            "/api/auth/login",
            None,
            json!({ "username": "admin", "password": PASSWORD, "device": device("x", "web") }),
        )
        .await;
    for _ in 0..5 {
        app.post(
            "/api/auth/login/totp",
            None,
            json!({ "challenge": login["challenge"], "code": "111111" }),
        )
        .await;
    }
    let (status, _) = app
        .post(
            "/api/auth/login/totp",
            None,
            json!({ "challenge": login["challenge"], "code": totp_next(&secret) }),
        )
        .await;
    assert_ne!(status, StatusCode::OK);
}

// ---- Tokens and devices -------------------------------------------------------------------

#[tokio::test]
async fn invalid_or_missing_tokens_are_rejected() {
    let app = TestApp::new().await;
    let (status, err) = app.get("/api/me", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(err["code"], "unauthenticated");
    let (status, _) = app.get("/api/me", Some("bvd_forged")).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn logout_and_revocation_kill_tokens() {
    let app = TestApp::new().await;
    let (admin, _) = app.ready_admin().await;
    let phone = app.create_user(&admin, "alice").await;
    let laptop = app.login_token("alice").await;

    let (_, devices) = app.get("/api/me/devices", Some(&laptop)).await;
    let devices = devices.as_array().unwrap();
    assert_eq!(devices.len(), 2);
    let phone_id = devices.iter().find(|d| d["current"] == false).unwrap()["id"].clone();

    let (status, _) = app
        .request(
            Method::DELETE,
            &format!("/api/me/devices/{phone_id}"),
            Some(&laptop),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = app.get("/api/me", Some(&phone)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, _) = app.post("/api/auth/logout", Some(&laptop), json!({})).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = app.get("/api/me", Some(&laptop)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn users_cannot_revoke_other_users_devices() {
    let app = TestApp::new().await;
    let (admin, _) = app.ready_admin().await;
    let alice = app.create_user(&admin, "alice").await;
    let (_, admin_devices) = app.get("/api/me/devices", Some(&admin)).await;
    let admin_device = admin_devices[0]["id"].clone();
    let (status, _) = app
        .request(
            Method::DELETE,
            &format!("/api/me/devices/{admin_device}"),
            Some(&alice),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn changing_password_signs_out_other_devices() {
    let app = TestApp::new().await;
    let (admin, _) = app.ready_admin().await;
    let phone = app.create_user(&admin, "alice").await;
    let laptop = app.login_token("alice").await;
    let (status, _) = app
        .request(
            Method::PUT,
            "/api/me/password",
            Some(&laptop),
            Some(json!({ "current_password": PASSWORD, "new_password": "another long password" })),
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(app.get("/api/me", Some(&laptop)).await.0, StatusCode::OK);
    assert_eq!(
        app.get("/api/me", Some(&phone)).await.0,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn user_can_change_language() {
    let app = TestApp::new().await;
    let (admin, _) = app.ready_admin().await;
    let alice = app.create_user(&admin, "alice").await;
    let (status, me) = app
        .request(
            Method::PATCH,
            "/api/me",
            Some(&alice),
            Some(json!({ "language": "en-US" })),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(me["language"], "en-US");
    let (status, _) = app
        .request(
            Method::PATCH,
            "/api/me",
            Some(&alice),
            Some(json!({ "language": "english" })),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn user_can_disable_2fa_with_password_and_code() {
    let app = TestApp::new().await;
    let (admin, _) = app.ready_admin().await;
    let alice = app.create_user(&admin, "alice").await;
    let (secret, _) = app.enable_totp(&alice).await;
    let (status, _) = app
        .post(
            "/api/me/totp/disable",
            Some(&alice),
            json!({ "password": PASSWORD, "code": totp_next(&secret) }),
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    app.login_token("alice").await; // no challenge anymore
}

#[tokio::test]
async fn enabling_2fa_needs_a_valid_code() {
    let app = TestApp::new().await;
    let token = app.setup_admin("admin").await;
    let (_, setup) = app
        .post(
            "/api/me/totp/setup",
            Some(&token),
            json!({ "password": PASSWORD }),
        )
        .await;
    assert!(
        setup["otpauth_url"]
            .as_str()
            .unwrap()
            .starts_with("otpauth://totp/")
    );
    let (status, _) = app
        .post(
            "/api/me/totp/enable",
            Some(&token),
            json!({ "code": "123456" }),
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let secret = setup["secret"].as_str().unwrap();
    let (status, _) = app
        .post(
            "/api/me/totp/enable",
            Some(&token),
            json!({ "code": totp_now(secret) }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
}

// ---- Invitations ------------------------------------------------------------------------

#[tokio::test]
async fn invitation_creates_an_account_once() {
    let app = TestApp::new().await;
    let (admin, _) = app.ready_admin().await;
    let (status, invitation) = app
        .post(
            "/api/admin/invitations",
            Some(&admin),
            json!({ "role": "user" }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let token = invitation["token"].as_str().unwrap();
    let accept = |username: &str| {
        json!({ "token": token, "username": username, "password": PASSWORD,
                "device": device("Pixel", "android") })
    };

    let (status, session) = app
        .post("/api/auth/invitations/accept", None, accept("bob"))
        .await;
    assert_eq!(status, StatusCode::CREATED, "{session}");
    assert_eq!(session["user"]["role"], "user");

    let (status, err) = app
        .post("/api/auth/invitations/accept", None, accept("carol"))
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(err["code"], "invalid_invitation");

    let (_, list) = app.get("/api/admin/invitations", Some(&admin)).await;
    assert!(list[0]["used_by"].is_number());
}

#[tokio::test]
async fn invitation_survives_a_taken_username() {
    let app = TestApp::new().await;
    let (admin, _) = app.ready_admin().await;
    let (_, invitation) = app
        .post(
            "/api/admin/invitations",
            Some(&admin),
            json!({ "role": "user" }),
        )
        .await;
    let accept = |username: &str| {
        json!({ "token": invitation["token"], "username": username, "password": PASSWORD,
                "device": device("Pixel", "android") })
    };
    let (status, _) = app
        .post("/api/auth/invitations/accept", None, accept("admin"))
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, _) = app
        .post("/api/auth/invitations/accept", None, accept("dave"))
        .await;
    assert_eq!(status, StatusCode::CREATED);
}

#[tokio::test]
async fn deleted_invitation_cannot_be_used() {
    let app = TestApp::new().await;
    let (admin, _) = app.ready_admin().await;
    let (_, invitation) = app
        .post(
            "/api/admin/invitations",
            Some(&admin),
            json!({ "role": "user" }),
        )
        .await;
    let (status, _) = app
        .request(
            Method::DELETE,
            &format!("/api/admin/invitations/{}", invitation["id"]),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = app
        .post(
            "/api/auth/invitations/accept",
            None,
            json!({ "token": invitation["token"], "username": "eve", "password": PASSWORD,
                    "device": device("x", "web") }),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn admin_cannot_delete_self_but_can_delete_others() {
    let app = TestApp::new().await;
    let (admin, _) = app.ready_admin().await;
    let alice = app.create_user(&admin, "alice").await;
    let (_, me) = app.get("/api/me", Some(&admin)).await;
    let (_, alice_me) = app.get("/api/me", Some(&alice)).await;

    let (status, _) = app
        .request(
            Method::DELETE,
            &format!("/api/admin/users/{}", me["id"]),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, _) = app
        .request(
            Method::DELETE,
            &format!("/api/admin/users/{}", alice_me["id"]),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(
        app.get("/api/me", Some(&alice)).await.0,
        StatusCode::UNAUTHORIZED
    );
}

// ---- Quick connect ----------------------------------------------------------------------

#[tokio::test]
async fn quick_connect_signs_in_the_tv_as_the_approver() {
    let app = TestApp::new().await;
    let (admin, _) = app.ready_admin().await;
    let alice = app.create_user(&admin, "alice").await;

    let (status, started) = app
        .post(
            "/api/auth/quick-connect/start",
            None,
            json!({ "device": device("Salon", "android_tv") }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let secret = started["poll_secret"].clone();

    let (_, poll) = app
        .post(
            "/api/auth/quick-connect/poll",
            None,
            json!({ "poll_secret": secret }),
        )
        .await;
    assert_eq!(poll["status"], "pending");

    let (status, _) = app
        .post(
            "/api/auth/quick-connect/approve",
            Some(&alice),
            json!({ "code": started["code"] }),
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (_, poll) = app
        .post(
            "/api/auth/quick-connect/poll",
            None,
            json!({ "poll_secret": secret }),
        )
        .await;
    assert_eq!(poll["status"], "ok", "{poll}");
    assert_eq!(poll["user"]["username"], "alice");
    let tv_token = poll["token"].as_str().unwrap();
    let (_, devices) = app.get("/api/me/devices", Some(tv_token)).await;
    assert!(
        devices
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["platform"] == "android_tv" && d["current"] == true)
    );
}

#[tokio::test]
async fn quick_connect_approval_needs_auth_and_a_valid_code() {
    let app = TestApp::new().await;
    let (admin, _) = app.ready_admin().await;
    let (status, _) = app
        .post(
            "/api/auth/quick-connect/approve",
            None,
            json!({ "code": "ABCDEF" }),
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = app
        .post(
            "/api/auth/quick-connect/approve",
            Some(&admin),
            json!({ "code": "ABCDEF" }),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ---- Web sessions (cookie) --------------------------------------------------------------

#[tokio::test]
async fn web_clients_get_an_httponly_cookie_instead_of_a_token() {
    let app = TestApp::new().await;
    let (admin, _) = app.ready_admin().await;
    app.create_user(&admin, "alice").await;

    let (status, headers, body) = app
        .request_with_headers(
            Method::POST,
            "/api/auth/login",
            &[],
            Some(json!({ "username": "alice", "password": PASSWORD, "device": device("Firefox", "web") })),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ok");
    assert!(body["token"].is_null(), "token never reaches page scripts");
    let cookie = headers[axum::http::header::SET_COOKIE]
        .to_str()
        .unwrap()
        .to_owned();
    assert!(cookie.starts_with("bv_session=bvd_"));
    for flag in ["HttpOnly", "SameSite=Strict", "Path=/", "Secure"] {
        assert!(cookie.contains(flag), "{flag} missing in {cookie}");
    }
    let pair = cookie.split(';').next().unwrap().to_owned();

    let (status, _, me) = app
        .request_with_headers(Method::GET, "/api/me", &[("cookie", &pair)], None)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(me["username"], "alice");

    let (status, headers, _) = app
        .request_with_headers(
            Method::POST,
            "/api/auth/logout",
            &[("cookie", &pair)],
            Some(json!({})),
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(
        headers[axum::http::header::SET_COOKIE]
            .to_str()
            .unwrap()
            .contains("Max-Age=0")
    );
    let (status, _, _) = app
        .request_with_headers(Method::GET, "/api/me", &[("cookie", &pair)], None)
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn non_web_clients_get_the_token_in_the_body() {
    let app = TestApp::new().await;
    let (admin, _) = app.ready_admin().await;
    app.create_user(&admin, "alice").await;
    let (_, headers, body) = app
        .request_with_headers(
            Method::POST,
            "/api/auth/login",
            &[],
            Some(json!({ "username": "alice", "password": PASSWORD, "device": device("Pixel", "android") })),
        )
        .await;
    assert!(body["token"].as_str().unwrap().starts_with("bvd_"));
    assert!(headers.get(axum::http::header::SET_COOKIE).is_none());
}
