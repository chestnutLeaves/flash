//! The mobile API's plumbing: the /api/v1 mount sits outside the
//! same-origin guard, speaks one JSON error envelope, and only admits
//! bearer tokens minted for the first-party client.

use crate::common;

use axum::http::StatusCode;
use common::*;

#[tokio::test]
async fn meta_is_public_and_reports_features() {
    let t = AppBuilder::new("api-meta").build();
    let reply = send(&t.app, json_get("/api/v1/meta", None)).await;
    assert_eq!(reply.status, StatusCode::OK);
    let json = reply.json();
    assert_eq!(json["min_app_version"], "1.0.1");
    assert_eq!(json["features"]["signup"], false);
    assert_eq!(json["features"]["passkeys"], true);
}

#[tokio::test]
async fn unknown_api_routes_answer_in_the_envelope() {
    let t = AppBuilder::new("api-404").build();
    let reply = send(&t.app, json_get("/api/v1/nope", None)).await;
    assert_eq!(reply.status, StatusCode::NOT_FOUND);
    let json = reply.json();
    assert_eq!(json["error"]["code"], "not_found");
    assert_eq!(json["error"]["what"], "route");
}

#[tokio::test]
async fn protected_routes_need_a_first_party_bearer() {
    let t = AppBuilder::new("api-bearer").build();
    let user = member(&t.store, "FlashTester", "flashtester@example.com");

    // No token: 401 in the envelope, with the challenge header.
    let reply = send(&t.app, json_get("/api/v1/badge", None)).await;
    assert_eq!(reply.status, StatusCode::UNAUTHORIZED);
    assert_eq!(reply.json()["error"]["code"], "unauthorized");
    assert_eq!(reply.header("www-authenticate"), Some("Bearer"));

    // An MCP grant is a valid OAuth token but not the app's.
    let mcp = oauth_bearer(&t.store, user);
    let reply = send(&t.app, json_get("/api/v1/badge", Some(&mcp))).await;
    assert_eq!(reply.status, StatusCode::UNAUTHORIZED);

    // Garbage is just garbage.
    let reply = send(&t.app, json_get("/api/v1/badge", Some("nope"))).await;
    assert_eq!(reply.status, StatusCode::UNAUTHORIZED);

    // The app's own grant works, and a POST needs no Origin header.
    let bearer = api_bearer(&t.store, user);
    let reply = send(&t.app, json_get("/api/v1/badge", Some(&bearer))).await;
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(reply.json()["badge"], 0);
}

fn in_language(
    mut req: axum::http::Request<axum::body::Body>,
    languages: &'static str,
) -> axum::http::Request<axum::body::Body> {
    req.headers_mut().insert(
        axum::http::header::ACCEPT_LANGUAGE,
        axum::http::HeaderValue::from_static(languages),
    );
    req
}

fn message(reply: &Reply) -> String {
    reply.json()["error"]["message"]
        .as_str()
        .unwrap_or_else(|| panic!("no message in {}", reply.text()))
        .to_string()
}

fn spanish_app(tag: &str) -> TestApp {
    AppBuilder::new(tag)
        .languages(&[spain(), latin_america()])
        .build()
}

#[tokio::test]
async fn refusals_speak_the_language_the_app_asks_for() {
    let t = spanish_app("api-refusal-language");
    let user = member(&t.store, "FlashTester", "flashtester@example.com");
    t.store
        .set_password_hash(
            user,
            Some(&flash_server::password::hash_password("hunter42hunter42").unwrap()),
        )
        .unwrap();
    let bearer = api_bearer(&t.store, user);
    let new_deck = |name: &str| {
        json_post(
            "/api/v1/decks",
            Some(&bearer),
            &serde_json::json!({ "name": name }),
        )
    };
    let made = send(&t.app, new_deck("Pharm")).await;
    assert_eq!(made.status, StatusCode::CREATED, "{}", made.text());

    let taken = send(&t.app, in_language(new_deck("Pharm"), "es-ES")).await;
    assert_eq!(taken.status, StatusCode::CONFLICT);
    assert_eq!(taken.json()["error"]["code"], "conflict");
    assert_eq!(message(&taken), "ya tienes un mazo con ese nombre");
    let taken = send(&t.app, new_deck("Pharm")).await;
    assert_eq!(message(&taken), "a deck with that name already exists");

    let bio = t
        .store
        .create_deck(user, "Bio", "", flash_server::service::now_ms())
        .unwrap();
    t.store
        .set_deck_archived(user, bio, Some(flash_server::service::now_ms()))
        .unwrap();
    let archived = send(&t.app, in_language(new_deck("Bio::Cells"), "es-MX")).await;
    assert_eq!(archived.status, StatusCode::CONFLICT);
    assert_eq!(
        message(&archived),
        "Tu mazo “Bio” está archivado. Desarchívalo o elige otro nombre."
    );
    let archived = send(&t.app, new_deck("Bio::Cells")).await;
    assert_eq!(
        message(&archived),
        "Your deck \"Bio\" is archived. Unarchive it, or pick another name."
    );

    let sign_in = || {
        json_post(
            "/api/v1/auth/password",
            None,
            &serde_json::json!({ "email": "flashtester@example.com", "password": "wrongwrongwrong" }),
        )
    };
    let wrong = send(&t.app, in_language(sign_in(), "es-ES,es;q=0.9")).await;
    assert_eq!(wrong.status, StatusCode::UNAUTHORIZED);
    assert_eq!(wrong.json()["error"]["code"], "invalid_credentials");
    assert_eq!(message(&wrong), "Esos datos no coinciden");
    let wrong = send(&t.app, sign_in()).await;
    assert_eq!(message(&wrong), "those details didn't match");

    let blank = || {
        json_patch(
            "/api/v1/settings/profile",
            Some(&bearer),
            &serde_json::json!({ "display_name": "   " }),
        )
    };
    let refused = send(&t.app, in_language(blank(), "es-419")).await;
    assert_eq!(refused.status, StatusCode::BAD_REQUEST);
    assert_eq!(refused.json()["error"]["code"], "invalid");
    assert_eq!(
        message(&refused),
        "el nombre es obligatorio (hasta 80 caracteres)"
    );
    let refused = send(&t.app, blank()).await;
    assert_eq!(
        message(&refused),
        "a name is required (up to 80 characters)"
    );

    let nope = send(&t.app, in_language(json_get("/api/v1/nope", None), "es-ES")).await;
    assert_eq!(nope.status, StatusCode::NOT_FOUND);
    assert_eq!(nope.json()["error"]["what"], "route");
    assert_eq!(message(&nope), "No encontrado");
    let nope = send(
        &t.app,
        in_language(json_get("/api/v1/decks/999999", Some(&bearer)), "es-ES"),
    )
    .await;
    assert_eq!(message(&nope), "Mazo no encontrado");
    let nope = send(&t.app, json_get("/api/v1/nope", None)).await;
    assert_eq!(message(&nope), "not found: route");
}

#[tokio::test]
async fn refusals_follow_the_variant_and_the_accounts_choice() {
    let t = spanish_app("api-refusal-variant");
    let user = member(&t.store, "FlashTester", "flashtester@example.com");
    t.store
        .set_password_hash(
            user,
            Some(&flash_server::password::hash_password("hunter42hunter42").unwrap()),
        )
        .unwrap();
    let bearer = api_bearer(&t.store, user);
    let remove = || {
        json_req(
            "DELETE",
            "/api/v1/settings/password",
            Some(&bearer),
            Some(&serde_json::json!({ "current": "hunter42hunter42" })),
        )
    };
    let spain = send(&t.app, in_language(remove(), "es-ES")).await;
    assert_eq!(spain.status, StatusCode::CONFLICT);
    assert_eq!(spain.json()["error"]["code"], "last_login_method");
    assert_eq!(
        message(&spain),
        "Es tu única forma de iniciar sesión; primero añade otra"
    );
    let mexico = send(&t.app, in_language(remove(), "es-MX")).await;
    assert_eq!(
        message(&mexico),
        "Es tu única forma de acceder; primero agrega otra"
    );
    let english = send(&t.app, remove()).await;
    assert_eq!(
        message(&english),
        "that's the only way to sign in, so add another method first"
    );

    let chose = send(
        &t.app,
        json_put(
            "/api/v1/settings/language",
            Some(&bearer),
            &serde_json::json!({ "language": "es-419" }),
        ),
    )
    .await;
    assert_eq!(chose.status, StatusCode::NO_CONTENT, "{}", chose.text());
    for asked in ["en", "es-ES"] {
        let reply = send(&t.app, in_language(remove(), asked)).await;
        assert_eq!(
            message(&reply),
            "Es tu única forma de acceder; primero agrega otra",
            "{asked}"
        );
    }
    let reply = send(&t.app, remove()).await;
    assert_eq!(
        message(&reply),
        "Es tu única forma de acceder; primero agrega otra"
    );

    let reset = send(
        &t.app,
        json_put(
            "/api/v1/settings/language",
            Some(&bearer),
            &serde_json::json!({ "language": null }),
        ),
    )
    .await;
    assert_eq!(reset.status, StatusCode::NO_CONTENT, "{}", reset.text());
    let reply = send(&t.app, in_language(remove(), "en")).await;
    assert_eq!(
        message(&reply),
        "that's the only way to sign in, so add another method first"
    );
}

#[tokio::test]
async fn the_rate_limit_speaks_the_language_asked_for() {
    let t = AppBuilder::new("api-refusal-throttle")
        .languages(&[spain(), latin_america()])
        .tight_rate_limits()
        .build();
    let mut throttled = None;
    for _ in 0..=TIGHT_BURST_GENEROUS {
        let reply = send(&t.app, in_language(json_get("/api/v1/meta", None), "es-ES")).await;
        if reply.status == StatusCode::TOO_MANY_REQUESTS {
            throttled = Some(reply);
            break;
        }
    }
    let throttled = throttled.expect("the API's budget ran out");
    assert_eq!(throttled.json()["error"]["code"], "rate_limited");
    assert_eq!(
        message(&throttled),
        "Demasiadas solicitudes seguidas; espera un momento"
    );
    let english = send(&t.app, json_get("/api/v1/meta", None)).await;
    assert_eq!(english.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(message(&english), "slow down");
}

#[tokio::test]
async fn api_replies_carry_no_store_and_security_headers() {
    let t = AppBuilder::new("api-headers").build();
    let reply = send(&t.app, json_get("/api/v1/badge", None)).await;
    assert_eq!(reply.header("cache-control"), Some("no-store"));
    assert_eq!(reply.header("x-content-type-options"), Some("nosniff"));
}
