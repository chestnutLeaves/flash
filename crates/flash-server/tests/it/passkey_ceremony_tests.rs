//! Passkeys end to end over the API, with a software authenticator where
//! the browser's would be: enrol one, then sign in with it. And the row a
//! user enrolled before the webauthn-rs 0.6 upgrade: stored in 0.5's
//! shape, it must still sign them in, or an upgrade locks people out
//! without a single error on the way.

use crate::common;

use axum::http::StatusCode;
use base64::Engine;
use common::*;
use ring::digest::{digest, SHA256};
use ring::rand::SystemRandom;
use ring::signature::{EcdsaKeyPair, KeyPair, ECDSA_P256_SHA256_ASN1_SIGNING};
use serde_json::{json, Value};
use webauthn_authenticator_rs::softpasskey::SoftPasskey;
use webauthn_authenticator_rs::WebauthnAuthenticator;

fn origin() -> url::Url {
    url::Url::parse(BASE).unwrap()
}

fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// Starts a passkey login and hands the challenge to `answer`, which
/// returns the authenticator's credential; finishes with that credential.
async fn log_in(t: &TestApp, answer: impl FnOnce(&Value) -> Value) -> Reply {
    let start = send(
        &t.app,
        json_post("/api/v1/auth/passkey/login/start", None, &json!({})),
    )
    .await;
    assert_eq!(start.status, StatusCode::OK, "{}", start.text());
    let ceremony = start.json();
    let credential = answer(&ceremony["options"]);
    send(
        &t.app,
        json_post(
            "/api/v1/auth/passkey/login/finish",
            None,
            &json!({"ceremony_id": ceremony["ceremony_id"], "credential": credential}),
        ),
    )
    .await
}

/// Enrols a software passkey for `user` the way the app's settings do;
/// returns the device and its credential id (base64url).
async fn enrol(t: &TestApp, user: flash_core::UserId) -> (SoftPasskey, String) {
    let bearer = api_bearer(&t.store, user);
    let mut device = SoftPasskey::new(true);
    let start = send(
        &t.app,
        json_post("/api/v1/settings/passkeys/start", Some(&bearer), &json!({})),
    )
    .await;
    assert_eq!(start.status, StatusCode::OK, "{}", start.text());
    let ceremony = start.json();
    let credential = device
        .do_registration(
            origin(),
            serde_json::from_value(ceremony["options"].clone()).unwrap(),
        )
        .unwrap();
    let finish = send(
        &t.app,
        json_post(
            "/api/v1/settings/passkeys/finish",
            Some(&bearer),
            &json!({"ceremony_id": ceremony["ceremony_id"], "credential": credential, "label": "Laptop"}),
        ),
    )
    .await;
    assert_eq!(finish.status, StatusCode::CREATED, "{}", finish.text());
    (device, credential.id)
}

/// A sign-in starts before anyone is known, so its challenge, on the web
/// and in the app, names nobody's passkey: the authenticator finds its
/// own.
#[tokio::test]
async fn a_login_challenge_names_no_account_s_passkey() {
    let t = AppBuilder::new("passkey-anonymous-start").build();
    let flashtester = member(&t.store, "FlashTester", "flashtester@example.com");
    let ada = member(&t.store, "Ada", "ada@example.com");
    let (_, mine) = enrol(&t, flashtester).await;
    let (_, theirs) = enrol(&t, ada).await;

    let app = send(
        &t.app,
        json_post("/api/v1/auth/passkey/login/start", None, &json!({})),
    )
    .await;
    assert_eq!(app.status, StatusCode::OK, "{}", app.text());
    let web = send(&t.app, form_post("/auth/login/start", None, "")).await;
    assert_eq!(web.status, StatusCode::OK, "{}", web.text());
    for options in [&app.json()["options"], &web.json()] {
        let listed = &options["publicKey"]["allowCredentials"];
        for id in [&mine, &theirs] {
            assert!(!listed.to_string().contains(id.as_str()), "{listed}");
        }
        // The same options as before otherwise: a modal prompt, no
        // extensions the clients never sent.
        assert!(options.get("mediation").is_none(), "{options}");
        assert!(
            options["publicKey"].get("extensions").is_none(),
            "{options}"
        );
    }
}

#[tokio::test]
async fn each_account_signs_in_with_the_passkey_it_enrolled() {
    let t = AppBuilder::new("passkey-ceremony").build();
    let flashtester = member(&t.store, "FlashTester", "flashtester@example.com");
    let ada = member(&t.store, "Ada", "ada@example.com");
    for user in [flashtester, ada] {
        let (mut device, id) = enrol(&t, user).await;
        let reply = log_in(&t, |options| {
            // A discoverable authenticator picks its own credential; the
            // software one has to be told which is its own.
            let mut options = options.clone();
            options["publicKey"]["allowCredentials"] = json!([{"type": "public-key", "id": id}]);
            let credential = device
                .do_authentication(origin(), serde_json::from_value(options).unwrap())
                .unwrap();
            serde_json::to_value(credential).unwrap()
        })
        .await;
        assert_eq!(reply.status, StatusCode::OK, "{}", reply.text());
        assert_eq!(reply.json()["user"]["id"], user.raw());
    }
}

/// A passkey row exactly as webauthn-rs 0.5.5 serialized it (captured from
/// that version), with this test's key and credential id in place of the
/// captured ones.
const STORED_BY_0_5: &str = r#"{"cred":{"cred_id":"CRED_ID","cred":{"type_":"ES256","key":{"EC_EC2":{"curve":"SECP256R1","x":"KEY_X","y":"KEY_Y"}}},"counter":0,"transports":null,"user_verified":true,"backup_eligible":false,"backup_state":false,"registration_policy":"required","extensions":{"cred_protect":"Ignored","hmac_create_secret":"NotRequested","appid":"NotRequested","cred_props":"Ignored"},"attestation":{"data":"Self_","metadata":"None"},"attestation_format":"packed"}}"#;

/// What an authenticator answers a login challenge with, signed by `key`:
/// user present and verified, sign counter 1.
fn assertion(options: &Value, cred_id: &[u8], key: &EcdsaKeyPair) -> Value {
    let client_data = serde_json::to_vec(&json!({
        "type": "webauthn.get",
        "challenge": options["publicKey"]["challenge"],
        "origin": BASE,
        "crossOrigin": false,
    }))
    .unwrap();
    let mut auth_data = digest(&SHA256, b"localhost").as_ref().to_vec();
    auth_data.push(0x05);
    auth_data.extend_from_slice(&1u32.to_be_bytes());
    let signed = [auth_data.as_slice(), digest(&SHA256, &client_data).as_ref()].concat();
    let signature = key.sign(&SystemRandom::new(), &signed).unwrap();
    json!({
        "id": b64(cred_id),
        "rawId": b64(cred_id),
        "response": {
            "authenticatorData": b64(&auth_data),
            "clientDataJSON": b64(&client_data),
            "signature": b64(signature.as_ref()),
            "userHandle": null,
        },
        "extensions": {},
        "type": "public-key",
    })
}

#[tokio::test]
async fn a_passkey_stored_by_webauthn_rs_0_5_still_signs_in() {
    let t = AppBuilder::new("passkey-0-5-row").build();
    let flashtester = member(&t.store, "FlashTester", "flashtester@example.com");
    let rng = SystemRandom::new();
    let pkcs8 = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, &rng).unwrap();
    let key =
        EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, pkcs8.as_ref(), &rng).unwrap();
    // Uncompressed point: 0x04, then x and y.
    let point = key.public_key().as_ref();
    let cred_id = b"enrolled-under-webauthn-rs-0.5";
    let row = STORED_BY_0_5
        .replace("CRED_ID", &b64(cred_id))
        .replace("KEY_X", &b64(&point[1..33]))
        .replace("KEY_Y", &b64(&point[33..]));
    t.store
        .add_passkey(flashtester, cred_id, &row, "phone", 1)
        .unwrap();

    let reply = log_in(&t, |options| assertion(options, cred_id, &key)).await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.text());
    assert_eq!(reply.json()["user"]["id"], flashtester.raw());
    // The sign-in wrote the new counter back, in a shape that still loads.
    let stored = &t.store.passkeys_for_user(flashtester).unwrap()[0].passkey_json;
    assert!(stored.contains(r#""counter":1"#), "{stored}");

    // The same row with a signature that doesn't match is refused.
    let reply = log_in(&t, |options| {
        let mut forged = assertion(options, cred_id, &key);
        let sig = forged["response"]["signature"]
            .as_str()
            .unwrap()
            .to_string();
        let mut bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(sig)
            .unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 0x01;
        forged["response"]["signature"] = json!(b64(&bytes));
        forged
    })
    .await;
    assert_eq!(reply.status, StatusCode::UNAUTHORIZED, "{}", reply.text());
}
