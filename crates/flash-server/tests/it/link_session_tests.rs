//! A link session (`auth::issue_link_session`) studies like any other,
//! and is asked to sign in again before what changes how the account is
//! reached: passwords, deleting it, other devices, connected apps,
//! exports, approving an assistant, admin pages.

use crate::common;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use common::*;
use flash_server::auth::urlencode;
use flash_server::password::hash_password;

const PASSWORD: &str = "correct horse battery staple";

fn with_password(t: &TestApp, email: &str) -> flash_core::UserId {
    let user = member(&t.store, "FlashTester", email);
    t.store
        .set_password_hash(user, Some(&hash_password(PASSWORD).unwrap()))
        .unwrap();
    user
}

/// A form post from the settings page, so a refusal knows where to come
/// back to.
fn settings_post(path: &str, cookie: &str, body: &str) -> Request<Body> {
    Request::post(path)
        .header(header::ORIGIN, BASE)
        .header(header::REFERER, format!("{BASE}/settings"))
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(header::COOKIE, cookie)
        .body(Body::from(body.to_string()))
        .unwrap()
}

#[tokio::test]
async fn a_link_session_studies_like_any_other() {
    let t = AppBuilder::new("link-studies").build();
    let user = with_password(&t, "flashtester@example.com");
    let cookie = link_session(&t.store, user);
    for path in ["/", "/decks", "/settings"] {
        let r = send(&t.app, get(path, Some(&cookie))).await;
        assert_eq!(r.status, StatusCode::OK, "{path}");
    }
}

#[tokio::test]
async fn a_link_session_signs_in_again_before_what_changes_the_account() {
    let t = AppBuilder::new("link-gated").build();
    let user = with_password(&t, "flashtester@example.com");
    let cookie = link_session(&t.store, user);

    for path in ["/export.csv", "/export.apkg"] {
        let r = send(&t.app, get(path, Some(&cookie))).await;
        assert_eq!(r.status, StatusCode::SEE_OTHER, "{path}");
        assert_eq!(
            r.location(),
            format!("/login?again=1&next={}", urlencode(path)),
            "{path}"
        );
    }
    for (path, body) in [
        (
            "/settings/password/change",
            "current=x&new_password=y&confirm=y",
        ),
        ("/settings/password/remove", "current=x"),
        ("/settings/delete-account", "confirm=DELETE"),
        ("/settings/devices/sign_out_others", ""),
        ("/settings/apps/disconnect", "client_id=x"),
    ] {
        let r = send(&t.app, settings_post(path, &cookie, body)).await;
        assert_eq!(r.status, StatusCode::SEE_OTHER, "{path}");
        assert_eq!(r.location(), "/login?again=1&next=/settings", "{path}");
    }
    // Still an account, still a password: nothing above ran.
    assert!(t
        .store
        .user_by_email("flashtester@example.com")
        .unwrap()
        .is_some());

    let authorize = "/oauth/authorize?response_type=code&client_id=x";
    let r = send(&t.app, get(authorize, Some(&cookie))).await;
    assert_eq!(r.status, StatusCode::SEE_OTHER);
    assert!(r.location().starts_with("/login?again=1&next="));
}

#[tokio::test]
async fn the_sign_in_page_keeps_a_link_session_and_a_real_sign_in_replaces_it() {
    let t = AppBuilder::new("link-upgrade").build();
    let user = with_password(&t, "flashtester@example.com");
    let cookie = link_session(&t.store, user);

    // A full session is sent on; a link session is shown the form.
    let full = web_session(&t.store, user);
    let r = send(&t.app, get("/login?next=/decks", Some(&full))).await;
    assert_eq!(r.status, StatusCode::SEE_OTHER);
    let r = send(
        &t.app,
        get("/login?again=1&next=/export.csv", Some(&cookie)),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK);
    assert!(r.text().contains("sign in again"), "{}", r.text());

    // Signing in with the password from that browser replaces it.
    let login = send(
        &t.app,
        Request::post("/auth/login/password")
            .header(header::ORIGIN, BASE)
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .header(header::COOKIE, &cookie)
            .body(Body::from(format!(
                "email=flashtester%40example.com&password={}&next=%2Fexport.csv",
                urlencode(PASSWORD)
            )))
            .unwrap(),
    )
    .await;
    assert_eq!(login.status, StatusCode::SEE_OTHER, "{}", login.text());
    assert_eq!(login.location(), "/export.csv");
    let fresh = login
        .header("set-cookie")
        .and_then(|c| c.split(';').next())
        .expect("a new session")
        .to_string();
    let r = send(&t.app, get("/export.csv", Some(&fresh))).await;
    assert_eq!(r.status, StatusCode::OK);
    // The link session is gone with it.
    let r = send(&t.app, get("/", Some(&cookie))).await;
    assert_eq!(r.status, StatusCode::SEE_OTHER);
}

#[tokio::test]
async fn the_devices_list_says_which_came_from_an_email_link() {
    let t = AppBuilder::new("link-devices").build();
    let user = with_password(&t, "flashtester@example.com");
    let _link = link_session(&t.store, user);
    let full = web_session(&t.store, user);
    let page = send(&t.app, get("/settings", Some(&full))).await.text();
    assert!(page.contains("signed in from an email link"), "{page}");
}

#[tokio::test]
async fn admin_pages_ask_a_link_session_to_sign_in_again() {
    let t = AppBuilder::new("link-admin").build();
    let admin = t
        .store
        .create_user("Admin", Some("admin@example.com"), "admin", 0)
        .unwrap();
    let cookie = link_session(&t.store, admin);
    let r = send(&t.app, get("/users", Some(&cookie))).await;
    assert_eq!(r.status, StatusCode::SEE_OTHER);
    assert_eq!(r.location(), "/login?again=1&next=/users");
}
