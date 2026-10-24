//! Signing in from the app: password (uniform failures, lockout), the
//! refresh rotation, logout and the sessions list, ending in first-party
//! bearer tokens.

use crate::common;

use axum::http::StatusCode;
use common::*;
use flash_core::UserId;
use flash_server::password::hash_password;
use flash_store::Store;
use serde_json::json;

fn with_password(store: &Store, name: &str, email: &str, password: &str) -> UserId {
    let user = member(store, name, email);
    store
        .set_password_hash(user, Some(&hash_password(password).unwrap()))
        .unwrap();
    user
}

async fn login(t: &TestApp, email: &str, password: &str, label: &str) -> Reply {
    send(
        &t.app,
        json_post(
            "/api/v1/auth/password",
            None,
            &json!({ "email": email, "password": password, "device_label": label }),
        ),
    )
    .await
}

#[tokio::test]
async fn password_login_mints_tokens_and_me() {
    let t = AppBuilder::new("api-pw").build();
    let user = with_password(
        &t.store,
        "FlashTester",
        "flashtester@example.com",
        "hunter42hunter42",
    );

    let reply = login(&t, "FlashTester@Example.com ", "hunter42hunter42", "iPhone").await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.text());
    let json = reply.json();
    assert_eq!(json["token_type"], "Bearer");
    assert_eq!(json["expires_in"], 3600);
    assert_eq!(json["user"]["id"], user.raw());
    assert_eq!(json["user"]["display_name"], "FlashTester");
    assert_eq!(json["user"]["theme"], "dark");
    assert_eq!(json["user"]["login_methods"]["password"], true);
    assert_eq!(reply.header("cache-control"), Some("no-store"));

    let access = json["access_token"].as_str().unwrap();
    let me = send(&t.app, json_get("/api/v1/me", Some(access))).await;
    assert_eq!(me.status, StatusCode::OK);
    assert_eq!(me.json()["email"], "flashtester@example.com");
}

/// Sign-in, refresh and `/me` answer with one `Me`: the app checks each
/// against the same schema, and a field missing from one of them once
/// signed it out on every cold start.
#[tokio::test]
async fn sign_in_refresh_and_me_answer_the_same_user() {
    let t = AppBuilder::new("api-me-shape").build();
    with_password(
        &t.store,
        "FlashTester",
        "flashtester@example.com",
        "hunter42hunter42",
    );
    let signed_in = login(&t, "flashtester@example.com", "hunter42hunter42", "iPhone")
        .await
        .json();
    let refreshed = send(
        &t.app,
        json_post(
            "/api/v1/auth/refresh",
            None,
            &json!({ "refresh_token": signed_in["refresh_token"] }),
        ),
    )
    .await
    .json();
    let me = send(
        &t.app,
        json_get("/api/v1/me", refreshed["access_token"].as_str()),
    )
    .await
    .json();

    assert_eq!(refreshed["user"], signed_in["user"]);
    assert_eq!(me, signed_in["user"]);
}

#[tokio::test]
async fn wrong_password_unknown_email_and_lockout_are_uniform() {
    let t = AppBuilder::new("api-pw-fail").build();
    with_password(
        &t.store,
        "FlashTester",
        "flashtester@example.com",
        "hunter42hunter42",
    );

    let wrong = login(&t, "flashtester@example.com", "nope-nope-nope", "x").await;
    assert_eq!(wrong.status, StatusCode::UNAUTHORIZED);
    assert_eq!(wrong.json()["error"]["code"], "invalid_credentials");
    let unknown = login(&t, "nobody@example.com", "hunter42hunter42", "x").await;
    assert_eq!(unknown.status, StatusCode::UNAUTHORIZED);
    assert_eq!(unknown.json(), wrong.json(), "no account-existence oracle");

    // Four more failures reach the lockout; the right password is then
    // refused with the very same answer.
    for _ in 0..4 {
        login(&t, "flashtester@example.com", "still-wrong-pw", "x").await;
    }
    let locked = login(&t, "flashtester@example.com", "hunter42hunter42", "x").await;
    assert_eq!(locked.status, StatusCode::UNAUTHORIZED);
    assert_eq!(locked.json(), wrong.json());
}

#[tokio::test]
async fn refresh_rotates_and_the_old_token_dies() {
    let t = AppBuilder::new("api-refresh").build();
    with_password(
        &t.store,
        "FlashTester",
        "flashtester@example.com",
        "hunter42hunter42",
    );
    let first = login(&t, "flashtester@example.com", "hunter42hunter42", "iPhone")
        .await
        .json();
    let old_refresh = first["refresh_token"].as_str().unwrap();

    let reply = send(
        &t.app,
        json_post(
            "/api/v1/auth/refresh",
            None,
            &json!({ "refresh_token": old_refresh }),
        ),
    )
    .await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.text());
    let second = reply.json();
    assert_ne!(second["access_token"], first["access_token"]);
    assert_eq!(second["user"]["display_name"], "FlashTester");

    // The new access token lives; the rotated-away one is dead. What the
    // rotated-away refresh token still does is the two tests below.
    let me = send(
        &t.app,
        json_get("/api/v1/me", second["access_token"].as_str()),
    )
    .await;
    assert_eq!(me.status, StatusCode::OK);
    let old = send(
        &t.app,
        json_get("/api/v1/me", first["access_token"].as_str()),
    )
    .await;
    assert_eq!(old.status, StatusCode::UNAUTHORIZED);
}

async fn refresh(t: &TestApp, token: &serde_json::Value) -> Reply {
    send(
        &t.app,
        json_post(
            "/api/v1/auth/refresh",
            None,
            &json!({ "refresh_token": token }),
        ),
    )
    .await
}

/// A phone whose refresh reply was lost still holds the token it sent;
/// sending it again at once brings back the pair that refresh made, not
/// a sign-out and not a third pair.
#[tokio::test]
async fn a_refresh_whose_reply_was_lost_gets_the_same_pair_again() {
    let t = AppBuilder::new("api-refresh-again").build();
    with_password(
        &t.store,
        "FlashTester",
        "flashtester@example.com",
        "hunter42hunter42",
    );
    let first = login(&t, "flashtester@example.com", "hunter42hunter42", "iPhone")
        .await
        .json();
    let lost = refresh(&t, &first["refresh_token"]).await.json();

    let again = refresh(&t, &first["refresh_token"]).await;
    assert_eq!(again.status, StatusCode::OK, "{}", again.text());
    let again = again.json();
    assert_eq!(again["access_token"], lost["access_token"]);
    assert_eq!(again["refresh_token"], lost["refresh_token"]);
    assert_eq!(again["user"]["display_name"], "FlashTester");
    let expires_in = again["expires_in"].as_i64().unwrap();
    assert!((3540..=3600).contains(&expires_in), "{expires_in}");
    let me = send(
        &t.app,
        json_get("/api/v1/me", again["access_token"].as_str()),
    )
    .await;
    assert_eq!(me.status, StatusCode::OK);
    // The pair carries on as if the first reply had arrived.
    let next = refresh(&t, &again["refresh_token"]).await;
    assert_eq!(next.status, StatusCode::OK, "{}", next.text());
}

/// Reuse detection: a refresh token presented after the one it was
/// rotated into has been used means two parties hold the grant, so the
/// whole grant goes, the newest pair included.
#[tokio::test]
async fn a_rotated_away_refresh_token_used_again_revokes_the_grant() {
    let t = AppBuilder::new("api-refresh-reuse").build();
    with_password(
        &t.store,
        "FlashTester",
        "flashtester@example.com",
        "hunter42hunter42",
    );
    let first = login(&t, "flashtester@example.com", "hunter42hunter42", "iPhone")
        .await
        .json();
    let second = refresh(&t, &first["refresh_token"]).await.json();
    let third = refresh(&t, &second["refresh_token"]).await;
    assert_eq!(third.status, StatusCode::OK, "{}", third.text());
    let third = third.json();

    let replay = refresh(&t, &first["refresh_token"]).await;
    assert_eq!(replay.status, StatusCode::UNAUTHORIZED);
    assert_eq!(replay.json()["error"]["code"], "invalid_grant");
    let me = send(
        &t.app,
        json_get("/api/v1/me", third["access_token"].as_str()),
    )
    .await;
    assert_eq!(
        me.status,
        StatusCode::UNAUTHORIZED,
        "the newest access token died"
    );
    let next = refresh(&t, &third["refresh_token"]).await;
    assert_eq!(
        next.status,
        StatusCode::UNAUTHORIZED,
        "the newest refresh token died"
    );
}

/// A connector's refresh token sent to the app's refresh endpoint is
/// refused before anything changes, so the connector keeps working.
#[tokio::test]
async fn a_connector_refresh_token_sent_to_the_app_is_refused_untouched() {
    let t = AppBuilder::new("api-refresh-client").build();
    let user = member(&t.store, "FlashTester", "flashtester@example.com");
    let now = flash_server::service::now_ms();
    let refresh_token = flash_server::auth::new_token();
    t.store
        .insert_oauth_token(
            &flash_server::auth::hash_token(&flash_server::auth::new_token()),
            &flash_server::auth::hash_token(&refresh_token),
            "test-client",
            user,
            "flashcards",
            "",
            None,
            now + 3_600_000,
            now + 86_400_000,
            now,
        )
        .unwrap();

    let refused = refresh(&t, &json!(refresh_token)).await;
    assert_eq!(refused.status, StatusCode::UNAUTHORIZED);
    assert_eq!(refused.json()["error"]["code"], "invalid_grant");

    let connector = send(
        &t.app,
        form_post(
            "/oauth/token",
            None,
            &format!(
                "grant_type=refresh_token&refresh_token={refresh_token}&client_id=test-client"
            ),
        ),
    )
    .await;
    assert_eq!(connector.status, StatusCode::OK, "{}", connector.text());
}

#[tokio::test]
async fn logout_revokes_the_grant() {
    let t = AppBuilder::new("api-logout").build();
    with_password(
        &t.store,
        "FlashTester",
        "flashtester@example.com",
        "hunter42hunter42",
    );
    let pair = login(&t, "flashtester@example.com", "hunter42hunter42", "iPhone")
        .await
        .json();
    let access = pair["access_token"].as_str().unwrap();

    let reply = send(
        &t.app,
        json_post("/api/v1/auth/logout", Some(access), &json!({})),
    )
    .await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT);
    let me = send(&t.app, json_get("/api/v1/me", Some(access))).await;
    assert_eq!(me.status, StatusCode::UNAUTHORIZED);
    let refresh = send(
        &t.app,
        json_post(
            "/api/v1/auth/refresh",
            None,
            &json!({ "refresh_token": pair["refresh_token"] }),
        ),
    )
    .await;
    assert_eq!(refresh.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn sessions_list_revoke_others_and_revoke_one() {
    let t = AppBuilder::new("api-sessions").build();
    with_password(
        &t.store,
        "FlashTester",
        "flashtester@example.com",
        "hunter42hunter42",
    );
    let phone = login(&t, "flashtester@example.com", "hunter42hunter42", "iPhone")
        .await
        .json();
    let ipad = login(&t, "flashtester@example.com", "hunter42hunter42", "iPad")
        .await
        .json();
    let phone_access = phone["access_token"].as_str().unwrap();
    let ipad_access = ipad["access_token"].as_str().unwrap();

    let list = send(&t.app, json_get("/api/v1/auth/sessions", Some(ipad_access))).await;
    assert_eq!(list.status, StatusCode::OK);
    let items = list.json()["items"].as_array().unwrap().clone();
    assert_eq!(items.len(), 2);
    let current: Vec<&serde_json::Value> = items.iter().filter(|i| i["current"] == true).collect();
    assert_eq!(current.len(), 1);
    assert_eq!(current[0]["label"], "iPad");
    assert_eq!(current[0]["kind"], "app");

    // Sign out other devices keeps the caller.
    let reply = send(
        &t.app,
        json_post(
            "/api/v1/auth/sessions/revoke_others",
            Some(ipad_access),
            &json!({}),
        ),
    )
    .await;
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(reply.json()["revoked"], 1);
    let phone_me = send(&t.app, json_get("/api/v1/me", Some(phone_access))).await;
    assert_eq!(phone_me.status, StatusCode::UNAUTHORIZED);
    let ipad_me = send(&t.app, json_get("/api/v1/me", Some(ipad_access))).await;
    assert_eq!(ipad_me.status, StatusCode::OK);

    // Revoking one grant by id, then it's gone and a repeat is a 404.
    let id = current[0]["id"].as_i64().unwrap();
    let reply = send(
        &t.app,
        json_delete(
            &format!("/api/v1/auth/sessions/app/{id}"),
            Some(ipad_access),
        ),
    )
    .await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT);
    let again = send(&t.app, json_get("/api/v1/me", Some(ipad_access))).await;
    assert_eq!(again.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn malformed_json_speaks_the_envelope() {
    let t = AppBuilder::new("api-badjson").build();
    let reply = send(
        &t.app,
        json_post("/api/v1/auth/password", None, &json!({ "email": 5 })),
    )
    .await;
    assert_eq!(reply.status, StatusCode::BAD_REQUEST);
    assert_eq!(reply.json()["error"]["code"], "invalid");
}
