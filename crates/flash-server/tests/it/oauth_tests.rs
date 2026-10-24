//! OAuth 2.1 flow tests against the real router via tower::oneshot:
//! DCR, PKCE happy/sad paths, single-use codes with replay revocation,
//! refresh rotation, and MCP bearer discovery.

use crate::common;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use base64::Engine;
use common::*;
use flash_server::service::now_ms;
use sha2::{Digest, Sha256};
use tower::ServiceExt;

struct Harness {
    app: axum::Router,
    session_cookie: String,
    store: std::sync::Arc<flash_store::Store>,
}

fn harness() -> Harness {
    let t = AppBuilder::new("oauth").build();
    let user = t
        .store
        .create_user("Tester", None, "member", now_ms())
        .unwrap();
    Harness {
        session_cookie: web_session(&t.store, user),
        store: t.store,
        app: t.app,
    }
}

/// (status, headers, JSON body, which is Null when the body isn't JSON).
async fn send(
    app: &axum::Router,
    request: Request<Body>,
) -> (StatusCode, Vec<(String, String)>, serde_json::Value) {
    let r = common::send(app, request).await;
    let json = r.json();
    (r.status, r.headers, json)
}

async fn register_client(h: &Harness) -> String {
    let (status, _, body) = send(
        &h.app,
        Request::post("/oauth/register")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                serde_json::json!({
                    "client_name": "Test Client",
                    "redirect_uris": ["https://client.example.com/callback"],
                    "token_endpoint_auth_method": "none"
                })
                .to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    body["client_id"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn registration_is_small_and_keeps_only_what_it_uses() {
    let h = harness();
    // Anything a client sends beyond the two fields we act on is accepted
    // and dropped; a body that could only be padding is refused outright.
    let padded = serde_json::json!({
        "client_name": "Padded",
        "redirect_uris": ["https://client.example.com/callback"],
        "software_statement": "x".repeat(20 * 1024),
    })
    .to_string();
    let (status, _, _) = send(
        &h.app,
        Request::post("/oauth/register")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(padded))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);

    // Control characters cannot reach the consent page through the name.
    let (status, _, body) = send(
        &h.app,
        Request::post("/oauth/register")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                serde_json::json!({
                    "client_name": "Cla\u{0}ude\n\r",
                    "redirect_uris": ["https://client.example.com/callback"],
                    "logo_uri": "https://client.example.com/logo.png",
                })
                .to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["client_name"], "Claude");
    assert!(body.get("logo_uri").is_none());
}

/// Runs authorize + consent, returns the authorization code.
async fn get_code(h: &Harness, client_id: &str, challenge: &str) -> String {
    get_code_with_state(h, client_id, challenge, "xyz").await
}

/// `get_code` with the client's own `state`, which must come back on
/// the redirect unchanged.
async fn get_code_with_state(h: &Harness, client_id: &str, challenge: &str, state: &str) -> String {
    let uri = authorize_uri(client_id, challenge, state);
    let (status, _, _) = send(
        &h.app,
        Request::get(&uri)
            .header(header::COOKIE, &h.session_cookie)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "consent page should render");

    // The consent token is server-side; extract it from the rendered form.
    let response = h
        .app
        .clone()
        .oneshot(
            Request::get(&uri)
                .header(header::COOKIE, &h.session_cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let html = String::from_utf8(
        axum::body::to_bytes(response.into_body(), 1_000_000)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    let token = html
        .split("name=\"token\" value=\"")
        .nth(1)
        .and_then(|s| s.split('"').next())
        .expect("consent token in page")
        .to_string();

    let (status, headers, _) = send(
        &h.app,
        Request::post("/oauth/consent")
            .header(header::COOKIE, &h.session_cookie)
            .header(header::ORIGIN, BASE)
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::from(format!("token={token}&decision=approve")))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let location = header_of(&headers, "location").expect("redirect");
    assert!(location.contains(&format!("state={state}")));
    // RFC 9207: the response names its issuer, exactly the metadata's.
    assert!(
        location.ends_with(&format!("&iss={}", flash_server::auth::urlencode(BASE))),
        "{location}"
    );
    location
        .split("code=")
        .nth(1)
        .unwrap()
        .split('&')
        .next()
        .unwrap()
        .to_string()
}

fn authorize_uri(client_id: &str, challenge: &str, state: &str) -> String {
    format!(
        "/oauth/authorize?response_type=code&client_id={client_id}\
         &redirect_uri=https%3A%2F%2Fclient.example.com%2Fcallback\
         &state={state}&code_challenge={challenge}&code_challenge_method=S256"
    )
}

/// A plugin directory's `state` is a signed blob over a kilobyte long.
/// Signed in, it goes through consent and comes back intact; signed
/// out, the whole authorize request fits the login page's `next`.
#[tokio::test]
async fn a_long_state_survives_consent_and_the_login_detour() {
    let h = harness();
    let client_id = register_client(&h).await;
    let state = "s".repeat(3_000);
    let challenge = s256(&"v".repeat(50));
    get_code_with_state(&h, &client_id, &challenge, &state).await;

    let uri = authorize_uri(&client_id, &challenge, &state);
    let r = common::send(&h.app, Request::get(&uri).body(Body::empty()).unwrap()).await;
    assert_eq!(r.status, StatusCode::SEE_OTHER);
    let login = r.location().to_string();
    assert!(login.starts_with("/login?next="), "{login}");
    let r = common::send(&h.app, Request::get(&login).body(Body::empty()).unwrap()).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text());
    assert!(
        r.text().contains(&state),
        "the sign-in form carries the request on"
    );
}

async fn exchange(
    h: &Harness,
    client_id: &str,
    code: &str,
    verifier: &str,
) -> (StatusCode, serde_json::Value) {
    let (status, _, body) = send(
        &h.app,
        Request::post("/oauth/token")
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::from(format!(
                "grant_type=authorization_code&code={code}\
                 &redirect_uri=https%3A%2F%2Fclient.example.com%2Fcallback\
                 &client_id={client_id}&code_verifier={verifier}"
            )))
            .unwrap(),
    )
    .await;
    (status, body)
}

fn s256(verifier: &str) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

#[tokio::test]
async fn metadata_endpoints_are_served() {
    let h = harness();
    let (status, _, body) = send(
        &h.app,
        Request::get("/.well-known/oauth-authorization-server")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["issuer"], BASE);
    assert_eq!(body["code_challenge_methods_supported"][0], "S256");
    assert_eq!(body["authorization_response_iss_parameter_supported"], true);

    let (status, _, body) = send(
        &h.app,
        Request::get("/.well-known/oauth-protected-resource")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["resource"], format!("{BASE}/mcp"));
}

#[tokio::test]
async fn full_pkce_flow_and_bearer_access() {
    let h = harness();
    let client_id = register_client(&h).await;
    let verifier = "a".repeat(50);
    let code = get_code(&h, &client_id, &s256(&verifier)).await;

    let (status, body) = exchange(&h, &client_id, &code, &verifier).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let access = body["access_token"].as_str().unwrap();
    assert_eq!(body["token_type"], "Bearer");
    assert!(body["refresh_token"].is_string());

    // Bearer token opens /mcp (a bad MCP body is fine: past the 401).
    let (status, _, _) = send(
        &h.app,
        Request::post("/mcp")
            .header(header::AUTHORIZATION, format!("Bearer {access}"))
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::ACCEPT, "application/json, text/event-stream")
            .body(Body::from("{}"))
            .unwrap(),
    )
    .await;
    assert_ne!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn wrong_verifier_is_rejected() {
    let h = harness();
    let client_id = register_client(&h).await;
    let code = get_code(
        &h,
        &client_id,
        &s256("correct-verifier-value-123456789012345678"),
    )
    .await;
    let (status, body) = exchange(
        &h,
        &client_id,
        &code,
        "wrong-verifier-value-1234567890123456789",
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "invalid_grant");
}

/// A token request whose form cannot be read is answered as before (the
/// form extractor's own 422), now that the handler sees the rejection to
/// log it.
#[tokio::test]
async fn an_unreadable_token_form_is_still_a_422() {
    let h = harness();
    let r = h
        .app
        .clone()
        .oneshot(
            Request::post("/oauth/token")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(format!(
                    "grant_type=authorization_code&code_verifier={}",
                    "v".repeat(129)
                )))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn code_replay_revokes_the_grant() {
    let h = harness();
    let client_id = register_client(&h).await;
    let verifier = "b".repeat(50);
    let code = get_code(&h, &client_id, &s256(&verifier)).await;

    let (status, body) = exchange(&h, &client_id, &code, &verifier).await;
    assert_eq!(status, StatusCode::OK);
    let access = body["access_token"].as_str().unwrap().to_string();

    // Replay the same code: rejected AND the earlier token is revoked.
    let (status, _) = exchange(&h, &client_id, &code, &verifier).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, _, _) = send(
        &h.app,
        Request::post("/mcp")
            .header(header::AUTHORIZATION, format!("Bearer {access}"))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from("{}"))
            .unwrap(),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "revoked token must not work"
    );
}

#[tokio::test]
async fn refresh_rotation_works_and_old_refresh_dies() {
    let h = harness();
    let client_id = register_client(&h).await;
    let verifier = "c".repeat(50);
    let code = get_code(&h, &client_id, &s256(&verifier)).await;
    let (_, body) = exchange(&h, &client_id, &code, &verifier).await;
    let refresh1 = body["refresh_token"].as_str().unwrap().to_string();

    let refresh = |token: String| {
        let app = h.app.clone();
        async move {
            send(
                &app,
                Request::post("/oauth/token")
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(Body::from(format!(
                        "grant_type=refresh_token&refresh_token={token}&client_id=x"
                    )))
                    .unwrap(),
            )
            .await
        }
    };
    let (status, _, body) = refresh(refresh1.clone()).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["access_token"].is_string());
    let refresh2 = body["refresh_token"].as_str().unwrap().to_string();
    let (status, _, body) = refresh(refresh2).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    // The rotated-out refresh token no longer works once its successor
    // has been used (a moment after its rotation, it would bring back the
    // same pair: a_lost_refresh_reply_is_repeated...).
    let (status, _, body) = refresh(refresh1).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
}

/// A connector that retries a refresh whose reply it lost gets the pair
/// that refresh made; once that pair's refresh token has been used, the
/// first token coming back again revokes the grant (reuse detection).
#[tokio::test]
async fn a_lost_refresh_reply_is_repeated_and_a_later_replay_revokes_the_grant() {
    let h = harness();
    let client_id = register_client(&h).await;
    let verifier = "e".repeat(50);
    let code = get_code(&h, &client_id, &s256(&verifier)).await;
    let (_, body) = exchange(&h, &client_id, &code, &verifier).await;
    let refresh1 = body["refresh_token"].as_str().unwrap().to_string();
    let refresh = |token: String| {
        let app = h.app.clone();
        let client_id = client_id.clone();
        async move {
            send(
                &app,
                Request::post("/oauth/token")
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(Body::from(format!(
                        "grant_type=refresh_token&refresh_token={token}&client_id={client_id}"
                    )))
                    .unwrap(),
            )
            .await
        }
    };

    let (status, _, lost) = refresh(refresh1.clone()).await;
    assert_eq!(status, StatusCode::OK, "{lost}");
    let (status, headers, again) = refresh(refresh1.clone()).await;
    assert_eq!(status, StatusCode::OK, "{again}");
    assert_eq!(again["access_token"], lost["access_token"]);
    assert_eq!(again["refresh_token"], lost["refresh_token"]);
    assert_eq!(header_of(&headers, "cache-control"), Some("no-store"));

    let refresh2 = again["refresh_token"].as_str().unwrap().to_string();
    let (status, _, newest) = refresh(refresh2).await;
    assert_eq!(status, StatusCode::OK, "{newest}");
    let (status, _, body) = refresh(refresh1).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["error"], "invalid_grant");
    let access = newest["access_token"].as_str().unwrap();
    let (status, _, _) = send(
        &h.app,
        Request::post("/mcp")
            .header(header::AUTHORIZATION, format!("Bearer {access}"))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from("{}"))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "the grant was revoked");
}

#[tokio::test]
async fn mcp_401_carries_discovery_pointer() {
    let h = harness();
    let (status, headers, _) = send(
        &h.app,
        Request::post("/mcp")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from("{}"))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let www = header_of(&headers, "www-authenticate").expect("WWW-Authenticate");
    assert!(www.contains("/.well-known/oauth-protected-resource"));
}

#[tokio::test]
async fn consent_page_csp_lets_the_callback_hand_off_to_an_app() {
    // Chrome checks form-action against every hop of the consent POST's
    // redirect chain, and a client's callback may hop on to its app's
    // own scheme (chatgpt.com answers ChatGPT's Android app with a 302
    // to `chatgpt://`). Any form-action cancels that hop, so there is none.
    let h = harness();
    let client_id = register_client(&h).await;
    let uri = format!(
        "/oauth/authorize?response_type=code&client_id={client_id}\
         &redirect_uri=https%3A%2F%2Fclient.example.com%2Fcallback\
         &code_challenge={}&code_challenge_method=S256",
        s256(&"d".repeat(50))
    );
    let r = common::send(
        &h.app,
        Request::get(&uri)
            .header(header::COOKIE, &h.session_cookie)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK);
    let csp = header_of(&r.headers, "content-security-policy").expect("CSP present");
    assert!(!csp.contains("form-action"), "got: {csp}");
    assert!(csp.contains("frame-ancestors 'none'"), "got: {csp}");
    // An account made from an assistant may see no other page, so this
    // one asks the browser for its time zone (a script from 'self', which
    // the policy allows).
    assert!(r.text().contains("/static/timezone.js"));
    assert!(csp.contains("script-src 'self'"), "got: {csp}");
}

#[tokio::test]
async fn unregistered_redirect_uri_never_redirects() {
    let h = harness();
    let client_id = register_client(&h).await;
    let uri = format!(
        "/oauth/authorize?response_type=code&client_id={client_id}\
         &redirect_uri=https%3A%2F%2Fevil.example.com%2Fsteal\
         &code_challenge=x&code_challenge_method=S256"
    );
    let (status, headers, _) = send(
        &h.app,
        Request::get(&uri)
            .header(header::COOKIE, &h.session_cookie)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(header_of(&headers, "location").is_none());
}

/// A malformed authorize request is a page, not a bounce. Anyone can
/// register a client with any https URI, so a redirect on error would
/// make the authorize endpoint an open redirector from this origin; the
/// redirect happens only after the person presses Approve or Deny.
#[tokio::test]
async fn a_malformed_authorize_request_is_a_page_not_a_redirect() {
    let h = harness();
    let client_id = register_client(&h).await;
    let registered = "https%3A%2F%2Fclient.example.com%2Fcallback";
    for (query, code) in [
        (
            format!("response_type=token&client_id={client_id}&redirect_uri={registered}&code_challenge=x&code_challenge_method=S256"),
            "unsupported_response_type",
        ),
        (
            format!("response_type=code&client_id={client_id}&redirect_uri={registered}&code_challenge_method=S256"),
            "invalid_request",
        ),
        (
            format!("response_type=code&client_id={client_id}&redirect_uri={registered}&code_challenge=x&code_challenge_method=plain"),
            "invalid_request",
        ),
        (
            format!("response_type=code&client_id={client_id}&redirect_uri={registered}&code_challenge=x&code_challenge_method=S256&resource=https%3A%2F%2Fother.example%2Fmcp"),
            "invalid_target",
        ),
        (
            format!("response_type=code&client_id={client_id}&redirect_uri={registered}&code_challenge=x&code_challenge_method=S256&scope=admin"),
            "invalid_scope",
        ),
    ] {
        let r = common::send(
            &h.app,
            Request::get(format!("/oauth/authorize?{query}"))
                .header(header::COOKIE, &h.session_cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST, "{query}");
        assert!(r.location().is_empty(), "{query} redirected to {}", r.location());
        assert!(r.text().contains(code), "{query}: page lacks {code}");
        assert!(!r.text().contains("client.example.com"), "{query}: page names the client's host");
    }
    // The same request with our own MCP endpoint as the resource is fine.
    let ours = "http%3A%2F%2Flocalhost%3A8437%2Fmcp";
    let r = common::send(
        &h.app,
        Request::get(format!(
            "/oauth/authorize?response_type=code&client_id={client_id}&redirect_uri={registered}&code_challenge=x&code_challenge_method=S256&resource={ours}"
        ))
        .header(header::COOKIE, &h.session_cookie)
        .body(Body::empty())
        .unwrap(),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text());
}

/// A plugin directory pins the resource it discovered before the server
/// moved; an origin named in `FLASH_PREVIOUS_BASE_URLS` still counts as
/// this server, and any other origin is still refused.
#[tokio::test]
async fn a_previous_origin_is_still_this_servers_resource() {
    let t = AppBuilder::new("oauth-moved")
        .previous_base_url("https://old.example.com/")
        .build();
    let user = t
        .store
        .create_user("Tester", None, "member", now_ms())
        .unwrap();
    let h = Harness {
        session_cookie: web_session(&t.store, user),
        store: t.store,
        app: t.app,
    };
    let client_id = register_client(&h).await;
    let registered = "https%3A%2F%2Fclient.example.com%2Fcallback";
    for (resource, status) in [
        ("https%3A%2F%2Fold.example.com%2Fmcp", StatusCode::OK),
        ("https%3A%2F%2Fold.example.com%2Fmcp%2F", StatusCode::OK),
        (
            "https%3A%2F%2Fother.example.com%2Fmcp",
            StatusCode::BAD_REQUEST,
        ),
        (
            "https%3A%2F%2Fold.example.com%2Fapi",
            StatusCode::BAD_REQUEST,
        ),
    ] {
        let r = common::send(
            &h.app,
            Request::get(format!(
                "/oauth/authorize?response_type=code&client_id={client_id}&redirect_uri={registered}&code_challenge=x&code_challenge_method=S256&resource={resource}"
            ))
            .header(header::COOKIE, &h.session_cookie)
            .body(Body::empty())
            .unwrap(),
        )
        .await;
        assert_eq!(r.status, status, "{resource}");
        if status != StatusCode::OK {
            assert!(r.text().contains("invalid_target"), "{resource}");
        }
    }
}

/// Redirect URIs are parsed as URLs: a loopback *host* over plain http,
/// or any https host; never userinfo, a fragment, or a lookalike.
#[tokio::test]
async fn redirect_uris_are_parsed_not_prefix_matched() {
    let h = harness();
    let attempt = |uri: &str| {
        Request::post("/oauth/register")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                serde_json::json!({ "client_name": "Probe", "redirect_uris": [uri] }).to_string(),
            ))
            .unwrap()
    };
    for bad in [
        "http://localhost@evil.example/cb",
        "http://localhost.evil/cb",
        "http://evil.example/cb",
        "https://client.example/cb#frag",
        "https://user:pw@client.example/cb",
        "javascript:alert(1)",
        "https://",
        "http://127.0.0.2/cb",
    ] {
        let (status, _, body) = send(&h.app, attempt(bad)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{bad} accepted: {body}");
    }
    for good in [
        "http://localhost:8080/cb",
        "http://127.0.0.1/cb",
        "http://[::1]:1234/cb",
        "https://client.example/cb?x=1",
    ] {
        let (status, _, body) = send(&h.app, attempt(good)).await;
        assert_eq!(status, StatusCode::CREATED, "{good} refused: {body}");
    }
}

/// The host the person approves is shown as the browser's address bar
/// would show it: a look-alike Unicode host appears in its `xn--` form.
#[tokio::test]
async fn the_consent_page_shows_a_unicode_host_in_ascii() {
    let h = harness();
    let lookalike = "https://\u{0441}hatgpt.com/cb";
    let (status, _, body) = send(
        &h.app,
        Request::post("/oauth/register")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                serde_json::json!({ "client_name": "ChatGPT", "redirect_uris": [lookalike] })
                    .to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let client_id = body["client_id"].as_str().unwrap();
    let uri = format!(
        "/oauth/authorize?response_type=code&client_id={client_id}&redirect_uri={}\
         &state=xyz&code_challenge={}&code_challenge_method=S256",
        flash_server::auth::urlencode(lookalike),
        s256(&"v".repeat(50))
    );
    let r = common::send(
        &h.app,
        Request::get(&uri)
            .header(header::COOKIE, &h.session_cookie)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK);
    let page = r.text();
    assert!(page.contains("xn--hatgpt-"), "no punycode host on the page");
    assert!(!page.contains("\u{0441}hatgpt.com"), "Unicode host shown");
}

/// Expired authorization codes are rows a signed-in member can mint at
/// the limiter's pace; housekeeping removes them.
#[test]
fn expired_authorization_codes_are_purged() {
    let t = AppBuilder::new("oauth-purge").build();
    let user = member(&t.store, "Ada", "ada@example.com");
    let now = now_ms();
    for (i, expires) in [(1, now - 1), (2, now + 60_000)] {
        t.store
            .create_oauth_code(
                &format!("hash-{i}"),
                "client",
                user,
                "https://client.example/cb",
                "challenge",
                "flashcards",
                None,
                expires,
            )
            .unwrap();
    }
    assert_eq!(t.store.purge_expired_oauth_codes(now).unwrap(), 1);
    assert_eq!(t.store.purge_expired_oauth_codes(now).unwrap(), 0);
}

const CHATGPT_SHARED: &str = "https://chatgpt.com/connector_platform_oauth_redirect";

/// Opens the consent page for `client_id` sending back to `redirect_uri`
/// and returns the page's status and its consent token, if it has one.
async fn consent_page(
    h: &Harness,
    client_id: &str,
    redirect_uri: &str,
    challenge: &str,
) -> (StatusCode, Option<String>) {
    let uri = format!(
        "/oauth/authorize?response_type=code&client_id={client_id}&redirect_uri={}\
         &state=xyz&code_challenge={challenge}&code_challenge_method=S256",
        flash_server::auth::urlencode(redirect_uri)
    );
    let r = common::send(
        &h.app,
        Request::get(&uri)
            .header(header::COOKIE, &h.session_cookie)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    let token = r
        .text()
        .split("name=\"token\" value=\"")
        .nth(1)
        .and_then(|s| s.split('"').next())
        .map(str::to_string);
    (r.status, token)
}

async fn decide(h: &Harness, token: &str, decision: &str) -> String {
    let (status, headers, _) = send(
        &h.app,
        Request::post("/oauth/consent")
            .header(header::COOKIE, &h.session_cookie)
            .header(header::ORIGIN, BASE)
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::from(format!("token={token}&decision={decision}")))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    header_of(&headers, "location")
        .expect("redirect")
        .to_string()
}

/// ChatGPT registers a callback of its own per connector, then, because
/// this server names its issuer, sends its shared callback under that
/// client: the connect goes through, code exchange and all.
#[tokio::test]
async fn chatgpt_may_use_its_shared_callback_under_a_per_connector_registration() {
    let h = harness();
    h.store
        .create_oauth_client(
            "chatgpt-client",
            "ChatGPT",
            &["https://chatgpt.com/connector/oauth/abc123".to_string()],
            "{}",
            now_ms(),
        )
        .unwrap();
    let verifier = "c".repeat(50);
    let (status, token) =
        consent_page(&h, "chatgpt-client", CHATGPT_SHARED, &s256(&verifier)).await;
    assert_eq!(status, StatusCode::OK);
    let location = decide(&h, &token.unwrap(), "approve").await;
    assert!(
        location.starts_with(&format!("{CHATGPT_SHARED}?code=")),
        "{location}"
    );
    assert!(location.ends_with(&format!("&iss={}", flash_server::auth::urlencode(BASE))));
    let code = location
        .split("code=")
        .nth(1)
        .unwrap()
        .split('&')
        .next()
        .unwrap();

    let (status, _, body) = send(
        &h.app,
        Request::post("/oauth/token")
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::from(format!(
                "grant_type=authorization_code&code={code}&redirect_uri={}\
                 &client_id=chatgpt-client&code_verifier={verifier}",
                flash_server::auth::urlencode(CHATGPT_SHARED)
            )))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    // The per-connector callback it registered still works too.
    let (status, _) = consent_page(
        &h,
        "chatgpt-client",
        "https://chatgpt.com/connector/oauth/abc123",
        &s256(&verifier),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

/// Only a client registered with a ChatGPT callback may use ChatGPT's
/// shared one.
#[tokio::test]
async fn another_client_may_not_use_chatgpts_shared_callback() {
    let h = harness();
    let client_id = register_client(&h).await;
    let (status, token) =
        consent_page(&h, &client_id, CHATGPT_SHARED, &s256(&"d".repeat(50))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(token.is_none());
}

/// ChatGPT's mobile website names its callbacks on `m.chatgpt.com` under
/// the client registered with `chatgpt.com` ones: both of them connect,
/// the code goes to the mobile-web callback as asked, and the exchange
/// names it again.
#[tokio::test]
async fn chatgpts_mobile_website_connects_under_its_chatgpt_registration() {
    let h = harness();
    h.store
        .create_oauth_client(
            "chatgpt-client",
            "ChatGPT",
            &["https://chatgpt.com/connector/oauth/abc123".to_string()],
            "{}",
            now_ms(),
        )
        .unwrap();
    for (i, callback) in [
        "https://m.chatgpt.com/connector/oauth/abc123",
        "https://m.chatgpt.com/connector_platform_oauth_redirect",
    ]
    .into_iter()
    .enumerate()
    {
        let verifier = format!("{i}").repeat(50);
        let (status, token) = consent_page(&h, "chatgpt-client", callback, &s256(&verifier)).await;
        assert_eq!(status, StatusCode::OK, "{callback}");
        let location = decide(&h, &token.unwrap(), "approve").await;
        assert!(
            location.starts_with(&format!("{callback}?code=")),
            "{location}"
        );
        let code = location
            .split("code=")
            .nth(1)
            .unwrap()
            .split('&')
            .next()
            .unwrap();
        let (status, _, body) = send(
            &h.app,
            Request::post("/oauth/token")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(format!(
                    "grant_type=authorization_code&code={code}&redirect_uri={}\
                     &client_id=chatgpt-client&code_verifier={verifier}",
                    flash_server::auth::urlencode(callback)
                )))
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{callback}: {body}");
    }

    // A client that registered the mobile-web callback itself still
    // matches it exactly.
    h.store
        .create_oauth_client(
            "mobile-web-client",
            "ChatGPT",
            &["https://m.chatgpt.com/connector/oauth/def456".to_string()],
            "{}",
            now_ms(),
        )
        .unwrap();
    let (status, _) = consent_page(
        &h,
        "mobile-web-client",
        "https://m.chatgpt.com/connector/oauth/def456",
        &s256(&"f".repeat(50)),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

/// The mobile-web host counts as ChatGPT's only for a callback the
/// client registered there, only over https, and only for that exact host.
#[tokio::test]
async fn only_chatgpts_own_mobile_web_callbacks_count_as_its_registration() {
    let h = harness();
    h.store
        .create_oauth_client(
            "chatgpt-client",
            "ChatGPT",
            &["https://chatgpt.com/connector/oauth/abc123".to_string()],
            "{}",
            now_ms(),
        )
        .unwrap();
    let challenge = s256(&"e".repeat(50));
    for refused in [
        "https://m.chatgpt.com/connector/oauth/other",
        "https://m.chatgpt.com.evil.example/connector/oauth/abc123",
        "https://x.m.chatgpt.com/connector/oauth/abc123",
        "https://m.chatgpt.com:8443/connector/oauth/abc123",
        "http://m.chatgpt.com/connector/oauth/abc123",
    ] {
        let (status, token) = consent_page(&h, "chatgpt-client", refused, &challenge).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{refused}");
        assert!(token.is_none(), "{refused}");
    }
    let other = register_client(&h).await;
    for refused in [
        "https://m.chatgpt.com/connector_platform_oauth_redirect",
        "https://m.chatgpt.com/connector/oauth/abc123",
    ] {
        let (status, _) = consent_page(&h, &other, refused, &challenge).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{refused}");
    }
}

/// A refusal names the issuer too (RFC 9207 covers error responses).
#[tokio::test]
async fn a_denied_consent_names_the_issuer() {
    let h = harness();
    let client_id = register_client(&h).await;
    let (_, token) = consent_page(
        &h,
        &client_id,
        "https://client.example.com/callback",
        &s256(&"e".repeat(50)),
    )
    .await;
    let location = decide(&h, &token.unwrap(), "deny").await;
    assert!(location.contains("error=access_denied"), "{location}");
    assert!(location.contains("state=xyz"));
    assert!(location.ends_with(&format!("&iss={}", flash_server::auth::urlencode(BASE))));
}

fn code_in(location: &str) -> String {
    location
        .split("code=")
        .nth(1)
        .and_then(|s| s.split('&').next())
        .expect("a code in the redirect")
        .to_string()
}

/// Whether `access` still opens the MCP endpoint (a revoked grant gets
/// 401; a live one gets past auth to complain about the body).
async fn token_works(h: &Harness, access: &str) -> bool {
    let (status, _, _) = send(
        &h.app,
        Request::post("/mcp")
            .header(header::AUTHORIZATION, format!("Bearer {access}"))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from("{}"))
            .unwrap(),
    )
    .await;
    status != StatusCode::UNAUTHORIZED
}

/// A second tap on Approve (a phone still showing the page while it hands
/// off to the app) gets a redirect too, never "consent expired" in place
/// of the first: a fresh code, with the first one withdrawn so only the
/// newest works.
#[tokio::test]
async fn a_second_approve_gets_a_fresh_code_and_withdraws_the_first() {
    let h = harness();
    let client_id = register_client(&h).await;
    let verifier = "f".repeat(50);
    let (_, token) = consent_page(
        &h,
        &client_id,
        "https://client.example.com/callback",
        &s256(&verifier),
    )
    .await;
    let token = token.unwrap();
    let first = decide(&h, &token, "approve").await;
    let second = decide(&h, &token, "approve").await;
    assert!(second.contains("state=xyz"), "{second}");
    let (first, second) = (code_in(&first), code_in(&second));
    assert_ne!(first, second);

    let (status, _) = exchange(&h, &client_id, &first, &verifier).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "the withdrawn code");
    let (status, body) = exchange(&h, &client_id, &second, &verifier).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(token_works(&h, body["access_token"].as_str().unwrap()).await);
}

/// Once the client redeemed the code, another Approve on the same screen
/// sends no new code (a code redeemed twice revokes the grant): it says
/// the connection is made, and the connection keeps working.
#[tokio::test]
async fn an_approve_after_the_code_was_redeemed_says_connected() {
    let h = harness();
    let client_id = register_client(&h).await;
    let verifier = "g".repeat(50);
    let (_, token) = consent_page(
        &h,
        &client_id,
        "https://client.example.com/callback",
        &s256(&verifier),
    )
    .await;
    let token = token.unwrap();
    let code = code_in(&decide(&h, &token, "approve").await);
    let (status, body) = exchange(&h, &client_id, &code, &verifier).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let access = body["access_token"].as_str().unwrap().to_string();

    let r = common::send(
        &h.app,
        Request::post("/oauth/consent")
            .header(header::COOKIE, &h.session_cookie)
            .header(header::ORIGIN, BASE)
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::from(format!("token={token}&decision=approve")))
            .unwrap(),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK);
    let html = r.text();
    assert!(html.contains("You&#39;re connected"), "{html}");
    assert!(html.contains("Test Client"), "{html}");
    assert!(token_works(&h, &access).await, "the grant survives");
}

/// The first answer stands: a deny answered again is a deny again, even
/// from the other button. A screen nobody opened is still refused.
#[tokio::test]
async fn the_first_answer_stands_and_an_unknown_screen_is_refused() {
    let h = harness();
    let client_id = register_client(&h).await;
    let (_, token) = consent_page(
        &h,
        &client_id,
        "https://client.example.com/callback",
        &s256(&"h".repeat(50)),
    )
    .await;
    let token = token.unwrap();
    assert!(decide(&h, &token, "deny")
        .await
        .contains("error=access_denied"));
    let again = decide(&h, &token, "approve").await;
    assert!(again.contains("error=access_denied"), "{again}");
    assert!(!again.contains("code="), "{again}");

    let r = common::send(
        &h.app,
        Request::post("/oauth/consent")
            .header(header::COOKIE, &h.session_cookie)
            .header(header::ORIGIN, BASE)
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::from("token=never-shown&decision=approve"))
            .unwrap(),
    )
    .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
}
