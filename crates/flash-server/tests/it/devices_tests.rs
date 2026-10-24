//! The devices list: every browser and phone signed in to the account,
//! on the settings page and in the app, each one signed out by itself or
//! all but the one asking. Signing in again from the same browser or the
//! same install replaces its row instead of adding one.

use crate::common;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use common::*;
use flash_core::UserId;
use flash_server::api::MOBILE_CLIENT_ID;
use flash_server::auth::{browser_name, hash_token, urlencode, SESSION_TOUCH_EVERY_MS};
use flash_server::password::hash_password;
use flash_server::service::now_ms;
use flash_store::Store;
use serde_json::json;

const CHROME_ON_WINDOWS: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
     (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36";
const PASSWORD: &str = "hunter42hunter42";

fn with_password(store: &Store, email: &str) -> UserId {
    let user = member(store, "FlashTester", email);
    store
        .set_password_hash(user, Some(&hash_password(PASSWORD).unwrap()))
        .unwrap();
    user
}

/// A password sign-in from a browser, carrying the cookie it already
/// holds, if any; the session cookie it is given back.
async fn browser_login(t: &TestApp, email: &str, held: Option<&str>) -> String {
    let mut req = Request::post("/auth/login/password")
        .header(header::ORIGIN, BASE)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(header::USER_AGENT, CHROME_ON_WINDOWS);
    if let Some(cookie) = held {
        req = req.header(header::COOKIE, cookie);
    }
    let body = format!(
        "email={}&password={}",
        urlencode(email),
        urlencode(PASSWORD)
    );
    let reply = send(&t.app, req.body(Body::from(body)).unwrap()).await;
    reply
        .set_cookies()
        .into_iter()
        .find(|c| c.starts_with("__Host-flash="))
        .map(|c| c.split(';').next().unwrap().to_string())
        .unwrap_or_else(|| panic!("no session cookie: {}", reply.text()))
}

/// A sign-in from the app; its access token.
async fn app_login(t: &TestApp, email: &str, label: &str, install: Option<&str>) -> String {
    let mut body = json!({ "email": email, "password": PASSWORD, "device_label": label });
    if let Some(install) = install {
        body["install_id"] = json!(install);
    }
    let reply = send(&t.app, json_post("/api/v1/auth/password", None, &body)).await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.text());
    reply.json()["access_token"].as_str().unwrap().to_string()
}

async fn device_rows(t: &TestApp, access: &str) -> Vec<serde_json::Value> {
    let reply = send(&t.app, json_get("/api/v1/auth/sessions", Some(access))).await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.text());
    reply.json()["items"].as_array().unwrap().clone()
}

async fn signed_in_page(t: &TestApp, cookie: &str) -> bool {
    send(&t.app, get("/settings", Some(cookie))).await.status == StatusCode::OK
}

fn app_token_live(store: &Store, access: &str) -> bool {
    store
        .lookup_api_token(&hash_token(access), now_ms())
        .unwrap()
        .is_some()
}

#[test]
fn a_browser_is_named_the_way_a_person_would() {
    let cases = [
        (CHROME_ON_WINDOWS, "Chrome on Windows"),
        (
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 \
             (KHTML, like Gecko) Version/18.5 Safari/605.1.15",
            "Safari on macOS",
        ),
        (
            "Mozilla/5.0 (iPhone; CPU iPhone OS 18_5 like Mac OS X) AppleWebKit/605.1.15 \
             (KHTML, like Gecko) Version/18.5 Mobile/15E148 Safari/604.1",
            "Safari on iPhone",
        ),
        (
            "Mozilla/5.0 (iPhone; CPU iPhone OS 18_5 like Mac OS X) AppleWebKit/605.1.15 \
             (KHTML, like Gecko) CriOS/140.0.0.0 Mobile/15E148 Safari/604.1",
            "Chrome on iPhone",
        ),
        (
            "Mozilla/5.0 (Linux; Android 15; Pixel 9) AppleWebKit/537.36 \
             (KHTML, like Gecko) Chrome/140.0.0.0 Mobile Safari/537.36",
            "Chrome on Android",
        ),
        (
            "Mozilla/5.0 (Linux; Android 14; SAMSUNG SM-S921B) AppleWebKit/537.36 \
             (KHTML, like Gecko) SamsungBrowser/27.0 Chrome/125.0.0.0 Mobile Safari/537.36",
            "Samsung Internet on Android",
        ),
        (
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
             (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36 Edg/140.0.0.0",
            "Edge on Windows",
        ),
        (
            "Mozilla/5.0 (X11; Linux x86_64; rv:142.0) Gecko/20100101 Firefox/142.0",
            "Firefox on Linux",
        ),
        (
            "Mozilla/5.0 (X11; CrOS x86_64 16181.61.0) AppleWebKit/537.36 \
             (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36",
            "Chrome on ChromeOS",
        ),
        ("curl/8.9.1", ""),
        ("", ""),
    ];
    for (user_agent, name) in cases {
        assert_eq!(browser_name(user_agent), name, "{user_agent}");
    }
}

#[tokio::test]
async fn the_app_lists_browsers_and_phones_and_signs_a_browser_out() {
    let t = AppBuilder::new("devices-app").build();
    with_password(&t.store, "flashtester@example.com");
    let browser = browser_login(&t, "flashtester@example.com", None).await;
    let phone = app_login(&t, "flashtester@example.com", "iPhone", Some("install-a")).await;

    let rows = device_rows(&t, &phone).await;
    assert_eq!(rows.len(), 2, "{rows:?}");
    // The phone asking comes first and says so.
    assert_eq!(rows[0]["kind"], "app");
    assert_eq!(rows[0]["label"], "iPhone");
    assert_eq!(rows[0]["current"], true);
    assert_eq!(rows[1]["kind"], "browser");
    assert_eq!(rows[1]["label"], "Chrome on Windows");
    assert_eq!(rows[1]["current"], false);

    // Signing the browser out from the phone ends its session.
    assert!(signed_in_page(&t, &browser).await);
    let id = rows[1]["id"].as_i64().unwrap();
    let reply = send(
        &t.app,
        json_delete(&format!("/api/v1/auth/sessions/browser/{id}"), Some(&phone)),
    )
    .await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT);
    assert!(!signed_in_page(&t, &browser).await);
    let again = send(
        &t.app,
        json_delete(&format!("/api/v1/auth/sessions/browser/{id}"), Some(&phone)),
    )
    .await;
    assert_eq!(again.status, StatusCode::NOT_FOUND);
    // A kind the list never names is nothing to sign out.
    let odd = send(
        &t.app,
        json_delete(&format!("/api/v1/auth/sessions/tv/{id}"), Some(&phone)),
    )
    .await;
    assert_eq!(odd.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn sign_out_others_from_the_app_takes_browsers_too() {
    let t = AppBuilder::new("devices-app-others").build();
    with_password(&t.store, "flashtester@example.com");
    let browser = browser_login(&t, "flashtester@example.com", None).await;
    let tablet = app_login(&t, "flashtester@example.com", "iPad", None).await;
    let phone = app_login(&t, "flashtester@example.com", "iPhone", None).await;
    let user = t
        .store
        .user_by_email("flashtester@example.com")
        .unwrap()
        .unwrap()
        .user;
    let connector = oauth_bearer(&t.store, user);

    let reply = send(
        &t.app,
        json_post(
            "/api/v1/auth/sessions/revoke_others",
            Some(&phone),
            &json!({}),
        ),
    )
    .await;
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(reply.json()["revoked"], 2);
    assert!(!signed_in_page(&t, &browser).await);
    assert!(!app_token_live(&t.store, &tablet));
    assert!(app_token_live(&t.store, &phone));
    // A connector is not a device: it stays connected.
    assert!(app_token_live(&t.store, &connector));
}

#[tokio::test]
async fn the_settings_page_lists_every_device_and_signs_one_out() {
    let t = AppBuilder::new("devices-web").build();
    with_password(&t.store, "flashtester@example.com");
    let browser = browser_login(&t, "flashtester@example.com", None).await;
    let phone = app_login(&t, "flashtester@example.com", "iPhone", None).await;

    let html = send(&t.app, get("/settings", Some(&browser))).await.text();
    assert!(html.contains("Devices"));
    assert!(html.contains("Chrome on Windows"));
    assert!(html.contains("This browser"));
    assert!(html.contains("iPhone"));
    // The phone has not refreshed since it signed in, so there is no use
    // to report: it says when it signed in instead.
    assert!(html.contains("Flash app · signed in just now"), "{html}");

    // A browser from before uses were kept says when it signed in too.
    let user = t
        .store
        .user_by_email("flashtester@example.com")
        .unwrap()
        .unwrap()
        .user;
    let three_days = 3 * 86_400_000;
    t.store
        .create_web_session(
            "older",
            user,
            "",
            now_ms() - three_days,
            now_ms() + three_days,
        )
        .unwrap();
    let html = send(&t.app, get("/settings", Some(&browser))).await.text();
    assert!(html.contains("Signed in 3d ago"), "{html}");

    let rows = device_rows(&t, &phone).await;
    let id = rows.iter().find(|r| r["kind"] == "app").unwrap()["id"]
        .as_i64()
        .unwrap();
    let reply = send(
        &t.app,
        form_post(
            "/settings/devices/sign_out",
            Some(&browser),
            &format!("kind=app&id={id}"),
        ),
    )
    .await;
    assert_eq!(reply.status, StatusCode::SEE_OTHER);
    assert_eq!(reply.location(), "/settings?saved=devices");
    assert!(!app_token_live(&t.store, &phone));

    let html = send(&t.app, get("/settings?saved=devices", Some(&browser)))
        .await
        .text();
    assert!(html.contains("Signed out"));
    assert!(!html.contains("iPhone"));
    assert!(signed_in_page(&t, &browser).await, "this browser stays");
}

#[tokio::test]
async fn sign_out_everywhere_else_spares_this_browser() {
    let t = AppBuilder::new("devices-web-others").build();
    with_password(&t.store, "flashtester@example.com");
    let here = browser_login(&t, "flashtester@example.com", None).await;
    let elsewhere = browser_login(&t, "flashtester@example.com", None).await;
    let phone = app_login(&t, "flashtester@example.com", "iPhone", None).await;

    let reply = send(
        &t.app,
        form_post("/settings/devices/sign_out_others", Some(&here), ""),
    )
    .await;
    assert_eq!(reply.status, StatusCode::SEE_OTHER);
    assert!(signed_in_page(&t, &here).await);
    assert!(!signed_in_page(&t, &elsewhere).await);
    assert!(!app_token_live(&t.store, &phone));
}

#[tokio::test]
async fn nobody_signs_out_another_accounts_device() {
    let t = AppBuilder::new("devices-isolation").build();
    with_password(&t.store, "flashtester@example.com");
    let victim_browser = browser_login(&t, "flashtester@example.com", None).await;
    let victim_phone = app_login(&t, "flashtester@example.com", "iPhone", None).await;
    let rows = device_rows(&t, &victim_phone).await;
    let (_other, other_cookie) = signed_in(&t.store, "Other", "other@example.com");
    let other_user = t
        .store
        .user_by_email("other@example.com")
        .unwrap()
        .unwrap()
        .user;
    let other_phone = api_bearer(&t.store, other_user);

    for row in &rows {
        let (kind, id) = (row["kind"].as_str().unwrap(), row["id"].as_i64().unwrap());
        let web = send(
            &t.app,
            form_post(
                "/settings/devices/sign_out",
                Some(&other_cookie),
                &format!("kind={kind}&id={id}"),
            ),
        )
        .await;
        assert_eq!(web.status, StatusCode::NOT_FOUND, "web {kind} {id}");
        let api = send(
            &t.app,
            json_delete(
                &format!("/api/v1/auth/sessions/{kind}/{id}"),
                Some(&other_phone),
            ),
        )
        .await;
        assert_eq!(api.status, StatusCode::NOT_FOUND, "api {kind} {id}");
    }
    assert!(signed_in_page(&t, &victim_browser).await);
    assert!(app_token_live(&t.store, &victim_phone));
}

#[tokio::test]
async fn a_device_id_never_reaches_a_connector() {
    let t = AppBuilder::new("devices-not-connectors").build();
    let (user, _) = signed_in(&t.store, "FlashTester", "flashtester@example.com");
    let phone = api_bearer(&t.store, user);
    let connector = oauth_bearer(&t.store, user);
    let connector_id = t
        .store
        .lookup_api_token(&hash_token(&connector), now_ms())
        .unwrap()
        .unwrap()
        .id;
    let reply = send(
        &t.app,
        json_delete(
            &format!("/api/v1/auth/sessions/app/{connector_id}"),
            Some(&phone),
        ),
    )
    .await;
    assert_eq!(reply.status, StatusCode::NOT_FOUND);
    assert!(app_token_live(&t.store, &connector));
}

#[tokio::test]
async fn signing_in_again_replaces_the_row_of_the_same_install_or_browser() {
    let t = AppBuilder::new("devices-replace").build();
    let user = with_password(&t.store, "flashtester@example.com");

    // The same install signing in twice is one phone.
    let first = app_login(&t, "flashtester@example.com", "iPhone", Some("install-a")).await;
    let second = app_login(&t, "flashtester@example.com", "iPhone", Some("install-a")).await;
    assert!(!app_token_live(&t.store, &first));
    assert!(app_token_live(&t.store, &second));
    // Another install is another phone; no install id replaces nothing.
    app_login(&t, "flashtester@example.com", "iPad", Some("install-b")).await;
    app_login(&t, "flashtester@example.com", "Old app", None).await;
    let phones = t
        .store
        .list_tokens_for_user(user, MOBILE_CLIENT_ID, now_ms())
        .unwrap();
    assert_eq!(phones.len(), 3, "{phones:?}");

    // A browser signing in again with its old cookie is one browser.
    let cookie = browser_login(&t, "flashtester@example.com", None).await;
    let renewed = browser_login(&t, "flashtester@example.com", Some(&cookie)).await;
    assert!(!signed_in_page(&t, &cookie).await);
    assert!(signed_in_page(&t, &renewed).await);
    let browsers = t.store.list_web_sessions(user, None, now_ms()).unwrap();
    assert_eq!(browsers.len(), 1, "{browsers:?}");
    assert_eq!(browsers[0].label, "Chrome on Windows");
}

#[tokio::test]
async fn a_browser_records_its_last_use_now_and_then() {
    let t = AppBuilder::new("devices-touch").build();
    let (user, cookie) = signed_in(&t.store, "FlashTester", "flashtester@example.com");
    let last_used = || t.store.list_web_sessions(user, None, now_ms()).unwrap()[0].last_used_at;
    assert_eq!(last_used(), None);

    assert!(signed_in_page(&t, &cookie).await);
    let first = last_used().expect("the first request records it");
    assert!(signed_in_page(&t, &cookie).await);
    assert_eq!(last_used(), Some(first), "not again within the window");

    // Once the window has passed, the next request moves it.
    let id = t.store.list_web_sessions(user, None, now_ms()).unwrap()[0].id;
    t.store
        .touch_web_session(id, first - SESSION_TOUCH_EVERY_MS)
        .unwrap();
    assert!(signed_in_page(&t, &cookie).await);
    assert!(last_used().unwrap() >= first);
}

#[tokio::test]
async fn expired_browser_sessions_are_swept() {
    let t = AppBuilder::new("devices-purge").build();
    let (user, _) = signed_in(&t.store, "FlashTester", "flashtester@example.com");
    t.store
        .create_web_session("gone", user, "Safari on iPhone", 1, 2)
        .unwrap();
    assert_eq!(t.store.purge_expired_web_sessions(now_ms()).unwrap(), 1);
    assert_eq!(
        t.store
            .list_web_sessions(user, None, now_ms())
            .unwrap()
            .len(),
        1,
        "the open one stays"
    );
}
