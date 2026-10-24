//! The study panel over the real MCP endpoint: the resource and its
//! metadata, which tools the panel may call, the panel's view riding in
//! `_meta`, its own tools, the signed media links, and what an account
//! with no cards is told.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use flash_server::media::{ingest_media, media_dir};
use flash_server::media_store::DiskStore;
use flash_server::service::now_ms;
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::mcp_tests::{
    call_tool, harness, harness_named, initialize, initialize_as, initialize_with, result_of, rpc,
    rpc_with, tool_json, Harness,
};

const PNG: &[u8] = &[
    0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0, 1, 2, 3,
];

/// A host that renders MCP Apps, as it introduces itself.
fn ui_host() -> Value {
    json!({"extensions": {"io.modelcontextprotocol/ui": {"mimeTypes": ["text/html;profile=mcp-app"]}}})
}

/// The whole `tools/call` result, `_meta` included.
async fn call_raw(h: &Harness, session: &str, name: &str, args: Value) -> Value {
    let (status, _, body) = rpc(
        h,
        Some(session),
        json!({"jsonrpc": "2.0", "id": 77, "method": "tools/call",
               "params": {"name": name, "arguments": args}}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let result = result_of(&body);
    assert_ne!(result, Value::Null, "rpc error: {body}");
    result
}

async fn tools(h: &Harness, session: &str) -> Vec<Value> {
    let (_, _, body) = rpc(
        h,
        Some(session),
        json!({"jsonrpc": "2.0", "id": 5, "method": "tools/list"}),
    )
    .await;
    result_of(&body)["tools"].as_array().unwrap().clone()
}

async fn get(h: &Harness, url: &str) -> (StatusCode, Vec<u8>) {
    let response = h
        .app
        .clone()
        .oneshot(Request::get(url).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1_000_000)
        .await
        .unwrap();
    (status, bytes.to_vec())
}

async fn two_cards(h: &Harness, session: &str) {
    let created = call_tool(
        h,
        session,
        3,
        "create_cards",
        json!({"deck": "Pharm", "cards": [
            {"front_html": "Antidote for warfarin?", "back_html": "Vitamin K"},
            {"front_html": "Antidote for heparin?", "back_html": "Protamine sulfate"}
        ]}),
    )
    .await;
    assert_eq!(created["created"], 2, "{created}");
}

/// The panel's script must parse: one syntax error (a name declared
/// twice, 2026-10-05) stops all of it, and hosts then show an empty frame
/// or "Runtime error" over every card made and every session, while every
/// server-side test still passes. Node checks the syntax. Without Node on
/// the machine the check is skipped and says so, unless
/// `FLASH_REQUIRE_NODE` is set, which turns a missing Node into a failure.
#[test]
fn the_panel_script_parses() {
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("panel/panel.js");
    let checked = std::process::Command::new("node")
        .arg("--check")
        .arg(&script)
        .output();
    match checked {
        Ok(out) => assert!(
            out.status.success(),
            "the panel's script does not parse:\n{}",
            String::from_utf8_lossy(&out.stderr)
        ),
        Err(e) if std::env::var_os("FLASH_REQUIRE_NODE").is_some() => {
            panic!("node not found ({e}) and FLASH_REQUIRE_NODE is set")
        }
        Err(e) => eprintln!("node not found ({e}): the panel's script was not checked"),
    }
}

#[tokio::test]
#[ignore = "writes the panel's tool results for browser tests; set FLASH_PANEL_RESULTS_DIR"]
async fn panel_results_for_a_browser() {
    let dir = std::path::PathBuf::from(
        std::env::var_os("FLASH_PANEL_RESULTS_DIR").expect("FLASH_PANEL_RESULTS_DIR"),
    );
    std::fs::create_dir_all(&dir).unwrap();
    let save = |name: &str, value: &Value| {
        let text = serde_json::to_string_pretty(value).unwrap();
        std::fs::write(dir.join(format!("{name}.json")), text).unwrap();
    };
    let shipped: Vec<flash_core::locale::UiLocale> = flash_server::testing::other_languages()
        .into_iter()
        .chain(flash_core::locale::UiLocale::all().filter(|l| l.pseudo().is_some()))
        .collect();
    let catalog = flash_server::i18n::server_catalog(
        &flash_server::i18n::DeploymentWords::NONE,
        &std::iter::once(flash_core::locale::UiLocale::EN)
            .chain(shipped)
            .collect::<Vec<_>>(),
    )
    .unwrap();
    let page =
        flash_server::mcp_panel::read("http://flash.test", flash_server::mcp_panel::URI, &catalog);
    let page = serde_json::to_value(&page).unwrap();
    std::fs::write(
        dir.join("panel.html"),
        page["contents"][0]["text"].as_str().unwrap(),
    )
    .unwrap();
    let words: serde_json::Map<String, Value> = flash_server::testing::other_languages()
        .into_iter()
        .map(|locale| {
            let tr = flash_server::i18n::Tr::new(catalog.clone(), locale, None);
            let key = flash_server::i18n::Value::from("1");
            let words = json!({
                "reveal": tr.t("panel-show-answer").as_str(),
                "again": tr.t("panel-again").as_str(),
                "easy": tr.t("panel-easy").as_str(),
                "key": tr.args("panel-key", &[("key", key)]).as_str(),
                "remind": tr.t("panel-remind-fallback").as_str(),
            });
            (locale.tag().to_string(), words)
        })
        .collect();
    save("panel-words", &Value::Object(words));

    let h = harness();
    let s = initialize_with(&h, ui_host()).await;

    // An account with no cards yet.
    save(
        "study-empty",
        &call_raw(&h, &s, "start_study_session", json!({})).await,
    );

    // Cards made: a subdeck, tags, a cloze note (two cards).
    let created = call_raw(
        &h,
        &s,
        "create_cards",
        json!({"deck": "Cardio::ECG", "cards": [
            {"front_html": "Antidote for warfarin?", "back_html": "Vitamin K", "tags": ["exam-2", "pharm"]},
            {"front_html": "Name this rhythm", "back_html": "Atrial flutter"},
            {"note_type": "cloze", "front_html": "{{c1::Digoxin}} slows the {{c2::AV node}}", "back_html": ""}
        ]}),
    )
    .await;
    save("cards-created", &created);
    let first = view(&created)["card_ids"][0].as_i64().unwrap();

    // A card edited: what it said before beside what it says now.
    save(
        "cards-edited",
        &call_raw(
            &h,
            &s,
            "update_cards",
            json!({"cards": [{"card_id": first, "back_html": "Vitamin K (phytonadione)"}]}),
        )
        .await,
    );

    // A session: the first card, its answer, a grade, and the last grade.
    let started = call_raw(
        &h,
        &s,
        "start_study_session",
        json!({"deck": "Cardio::ECG"}),
    )
    .await;
    save("study-card", &started);
    let sid = view(&started)["session_id"].as_i64().unwrap();
    let mut card = view(&started)["card"]["card_id"].as_i64().unwrap();
    save(
        "panel-show-answer",
        &call_raw(&h, &s, "panel_show_answer", json!({"card_id": card})).await,
    );
    let archived = call_raw(
        &h,
        &s,
        "panel_archive",
        json!({"session_id": sid, "card_id": card, "archived": true}),
    )
    .await;
    assert_ne!(tool_json(&archived)["card"]["card_id"], json!(card));
    save("panel-archive-next", &archived);
    let undone = call_raw(
        &h,
        &s,
        "panel_archive",
        json!({"session_id": sid, "card_id": card, "archived": false}),
    )
    .await;
    assert_eq!(tool_json(&undone)["card"]["card_id"], json!(card));
    save("panel-archive-undone", &undone);
    let source = call_raw(&h, &s, "panel_edit_source", json!({"card_id": card})).await;
    save("panel-edit-source", &source);
    let fields = tool_json(&source);
    save(
        "panel-edit-saved",
        &call_raw(
            &h,
            &s,
            "panel_save_edit",
            json!({"card_id": card,
                   "front_html": format!("{} (checked)", fields["front_html"].as_str().unwrap()),
                   "back_html": fields["back_html"]}),
        )
        .await,
    );
    let mut graded = 0;
    loop {
        let next = call_raw(
            &h,
            &s,
            "panel_grade",
            json!({"session_id": sid, "card_id": card, "rating": 3}),
        )
        .await;
        graded += 1;
        let out = tool_json(&next);
        if graded == 1 {
            save("panel-grade-next", &next);
        }
        if out["state"] != "card" {
            save("panel-grade-done", &next);
            break;
        }
        card = out["card"]["card_id"].as_i64().unwrap();
        assert!(graded < 20, "the session never ended: {next}");
    }

    // A call that fails: the answer to a card that does not exist, which
    // the host passes on as the JSON-RPC error it is.
    let (_, _, body) = rpc(
        &h,
        Some(&s),
        json!({"jsonrpc": "2.0", "id": 78, "method": "tools/call",
               "params": {"name": "panel_show_answer", "arguments": {"card_id": 999_999}}}),
    )
    .await;
    let frame = body
        .lines()
        .filter_map(|l| l.strip_prefix("data:"))
        .map(str::trim)
        .find(|payload| payload.starts_with('{'))
        .unwrap_or(&body);
    let error = serde_json::from_str::<Value>(frame).unwrap()["error"].clone();
    assert!(error["message"].is_string(), "{body}");
    save("panel-show-answer-failed", &error);

    // New cards held back by the day's limit.
    call_raw(
        &h,
        &s,
        "create_cards",
        json!({"deck": "Held", "cards": [
            {"front_html": "Held one", "back_html": "1"},
            {"front_html": "Held two", "back_html": "2"}
        ]}),
    )
    .await;
    call_raw(
        &h,
        &s,
        "set_limits",
        json!({"deck": "Held", "new_per_day": 0}),
    )
    .await;
    save(
        "study-held",
        &call_raw(&h, &s, "start_study_session", json!({"deck": "Held"})).await,
    );

    let today = h.store.get_settings(h.user).unwrap().study_date(now_ms());
    for (deck, name, behind) in [
        ("Boards", "study-goal", false),
        ("Pharm", "study-goal-behind", true),
    ] {
        let cards: Vec<Value> = (1..=30)
            .map(|i| json!({"front_html": format!("{deck} {i}"), "back_html": format!("{i}")}))
            .collect();
        call_raw(
            &h,
            &s,
            "create_cards",
            json!({"deck": deck, "cards": cards}),
        )
        .await;
        let goal = today.checked_add(jiff::Span::new().days(20)).unwrap();
        call_raw(
            &h,
            &s,
            "set_limits",
            json!({"deck": deck, "goal_date": goal.to_string()}),
        )
        .await;
        if behind {
            let id = h.store.find_deck_by_name(h.user, deck).unwrap().unwrap();
            h.store
                .set_deck_goal(
                    h.user,
                    id,
                    Some(flash_store::DeckGoal {
                        date: goal,
                        pace: 1,
                        cards: 30,
                    }),
                )
                .unwrap();
        }
        let start = call_raw(&h, &s, "start_study_session", json!({"deck": deck})).await;
        let out = tool_json(&start);
        let sid = out["session_id"].as_i64().unwrap();
        let mut card = out["first_card"]["card_id"].as_i64().unwrap();
        for graded in 1.. {
            let next = call_raw(
                &h,
                &s,
                "panel_grade",
                json!({"session_id": sid, "card_id": card, "rating": 4}),
            )
            .await;
            let out = tool_json(&next);
            if out["state"] != "card" {
                assert!(out["goal"].is_object(), "{next}");
                break;
            }
            card = out["card"]["card_id"].as_i64().unwrap();
            assert!(graded < 40, "the session never ended: {next}");
        }
        save(
            name,
            &call_raw(&h, &s, "start_study_session", json!({"deck": deck})).await,
        );
    }

    // Many cards made at once: paged, and fetched a page at a time.
    let many: Vec<Value> = (1..=45)
        .map(|i| json!({"front_html": format!("Card {i}"), "back_html": format!("Answer {i}")}))
        .collect();
    let made = call_raw(
        &h,
        &s,
        "create_cards",
        json!({"deck": "Many", "cards": many}),
    )
    .await;
    save("cards-many", &made);
    let ids: Vec<Value> = view(&made)["card_ids"].as_array().unwrap()[20..40].to_vec();
    save(
        "panel-cards",
        &call_raw(&h, &s, "panel_cards", json!({"card_ids": ids})).await,
    );

    save(
        "panel-reminders",
        &call_raw(
            &h,
            &s,
            "panel_reminders",
            json!({"timezone": "America/New_York"}),
        )
        .await,
    );
}

#[test]
fn the_panel_markup_holds_its_script_once() {
    let markup = include_str!("../../panel/panel.html");
    assert_eq!(markup.matches("__FLASH_PANEL_SCRIPT__").count(), 1);
    assert_eq!(markup.matches("__FLASH_I18N_SCRIPT__").count(), 1);
    assert_eq!(markup.matches("__FLASH_NEGOTIATE_SCRIPT__").count(), 1);
    assert_eq!(markup.matches("__FLASH_PAYLOAD_SCRIPT__").count(), 1);
    assert_eq!(markup.matches("__FLASH_PANEL_WORDS__").count(), 1);
    assert_eq!(markup.matches("__FLASH_PANEL_LANGUAGES__").count(), 1);
    assert_eq!(markup.matches("<script").count(), 4);
    assert!(markup.contains(
        "<script>\n__FLASH_NEGOTIATE_SCRIPT__</script>\n<script>\n__FLASH_I18N_SCRIPT__</script>\n<script>\n__FLASH_PAYLOAD_SCRIPT__</script>\n<script>\n__FLASH_PANEL_SCRIPT__</script>"
    ));
    assert!(!include_str!("../../panel/negotiate.js").contains("</script"));
    assert!(!include_str!("../../panel/payload.js").contains("</script"));
    assert!(!include_str!("../../panel/panel.js").contains("</script"));
    assert!(!include_str!("../../static/i18n.js").contains("</script"));
}

#[test]
fn the_panel_page_carries_its_words() {
    let every: Vec<_> = flash_core::locale::UiLocale::all().collect();
    let catalog =
        flash_server::i18n::server_catalog(&flash_server::i18n::DeploymentWords::NONE, &every)
            .unwrap();
    let page =
        flash_server::mcp_panel::read("https://flash.test", flash_server::mcp_panel::URI, &catalog);
    let text = serde_json::to_value(&page).unwrap().to_string();
    assert!(!text.contains("__FLASH_"), "every mark is written in");
    assert!(text.contains("data-flash-i18n"), "{text}");
    assert!(text.contains("data-flash-languages"), "{text}");
    assert!(text.contains("window.flashNegotiate"), "{text}");
    assert!(text.contains("panel-show-answer"), "{text}");
    assert!(text.contains("interval-days"), "{text}");
    assert!(
        text.contains("<html lang=\\\"en\\\" dir=\\\"ltr\\\" data-script=\\\"Latn\\\">"),
        "{text}"
    );
}

/// The page a host reads, with every language of the table shipped: a
/// header per language, each message folder's words carried once
/// (English's and the Spanish base's under both Spanishes), and the
/// whole kept to a budget, since a host loads it before the panel shows.
#[test]
fn the_panel_carries_each_folder_once_within_its_budget() {
    const BUDGET: usize = 160 * 1024;
    let all: Vec<_> = flash_core::locale::UiLocale::all().collect();
    let catalog = std::sync::Arc::new(
        flash_server::i18n::Catalog::new(flash_server::i18n::CORE)
            .shipping(&all)
            .unwrap(),
    );
    let page = flash_server::mcp_panel::page_text("https://flash.test", &catalog);
    let islands = |kind: &str| -> Vec<Value> {
        let open = format!(r#"<script type="application/json" data-flash-{kind}>"#);
        page.split(open.as_str())
            .skip(1)
            .map(|rest| serde_json::from_str(rest.split_once("</script>").unwrap().0).unwrap())
            .collect()
    };
    let headers: Vec<String> = islands("lang")
        .iter()
        .map(|h| h["lang"].as_str().unwrap().to_string())
        .collect();
    let tags: Vec<String> = all.iter().map(|l| l.tag().to_string()).collect();
    assert_eq!(headers, tags, "a header per language, in the table's order");
    let mut folders: Vec<String> = islands("i18n")
        .iter()
        .map(|i| i["folder"].as_str().unwrap().to_string())
        .collect();
    let carried = folders.len();
    folders.sort();
    folders.dedup();
    assert_eq!(
        folders.len(),
        carried,
        "a folder carried twice: {folders:?}"
    );
    assert!(
        page.len() <= BUDGET,
        "the panel page is {} bytes",
        page.len()
    );
}

#[test]
fn the_panel_shows_every_text_color_and_highlight_the_site_does() {
    let markup = include_str!("../../panel/panel.html");
    for hue in flash_store::richtext::HIGHLIGHT_HUES {
        assert!(
            markup.contains(&format!(".hl-{hue}{{color:var(--ink-{hue})}}")),
            "{hue}"
        );
        assert!(
            markup.contains(&format!(".hl-bg-{hue}{{background:var(--tint-{hue})}}")),
            "{hue}"
        );
        assert_eq!(markup.matches(&format!("--ink-{hue}:")).count(), 3, "{hue}");
        assert_eq!(
            markup.matches(&format!("--tint-{hue}:")).count(),
            3,
            "{hue}"
        );
    }
}

#[tokio::test]
async fn the_panel_is_a_resource_the_server_declares() {
    let h = harness();
    let (_, _, body) = rpc(
        &h,
        None,
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
            "protocolVersion": "2025-06-18", "capabilities": ui_host(),
            "clientInfo": {"name": "test", "version": "0"}}}),
    )
    .await;
    let caps = result_of(&body)["capabilities"].clone();
    assert!(caps["resources"].is_object(), "{caps}");
    assert!(
        caps["extensions"]["io.modelcontextprotocol/ui"].is_object(),
        "{caps}"
    );

    let session = initialize(&h).await;
    let (_, _, body) = rpc(
        &h,
        Some(&session),
        json!({"jsonrpc": "2.0", "id": 2, "method": "resources/list"}),
    )
    .await;
    let listed = result_of(&body)["resources"].clone();
    assert_eq!(listed.as_array().unwrap().len(), 1, "{listed}");
    let uri = listed[0]["uri"].as_str().unwrap().to_string();
    assert!(uri.starts_with("ui://flash/"), "{uri}");
    assert_eq!(listed[0]["mimeType"], "text/html;profile=mcp-app");

    let (_, _, body) = rpc(
        &h,
        Some(&session),
        json!({"jsonrpc": "2.0", "id": 3, "method": "resources/read", "params": {"uri": uri}}),
    )
    .await;
    let content = result_of(&body)["contents"][0].clone();
    assert_eq!(content["mimeType"], "text/html;profile=mcp-app");
    let page = content["text"].as_str().unwrap();
    assert!(page.starts_with("<!doctype html>"));
    assert!(page.contains("ui/initialize") && page.contains("panel_grade"));
    assert!(
        page.contains("panel_cards"),
        "the cards view pages through them"
    );
    // The handshake names the page as the MCP Apps schema does; a host
    // that checks it refused `clientInfo` and left the frame at no height.
    assert!(
        page.contains("appInfo:") && !page.contains("clientInfo"),
        "ui/initialize must carry appInfo"
    );
    // Nothing loaded from anywhere else, nothing fetched at all.
    assert!(!page.contains("<script src") && !page.contains("<link"));
    // The cards are in the site's font, from this server, which serves it
    // to the host's sandbox.
    assert!(!page.contains("__FLASH_ORIGIN__"));
    let font = page
        .split("src:url(")
        .nth(1)
        .and_then(|rest| rest.split(')').next())
        .unwrap();
    assert!(
        font.starts_with("http") && font.contains("/static/fonts/Geist-"),
        "{font}"
    );
    let path = &font[font.find("/static/").unwrap()..];
    let response = h
        .app
        .clone()
        .oneshot(Request::get(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK, "{path}");
    assert_eq!(response.headers()["access-control-allow-origin"], "*");
    // ChatGPT's page origin rides only in its own key: Claude reads
    // `ui.domain`, wants its own format there, and draws nothing on any
    // other value.
    assert!(content["_meta"]["ui"].get("domain").is_none(), "{content}");
    assert!(
        content["_meta"]["openai/widgetDomain"]
            .as_str()
            .is_some_and(|d| d.starts_with("http")),
        "{content}"
    );
    let csp = &content["_meta"]["ui"]["csp"];
    assert_eq!(csp["connectDomains"], json!([]), "{content}");
    assert_eq!(csp["resourceDomains"].as_array().unwrap().len(), 1, "{csp}");

    // The page's earlier URI, which ChatGPT's reviewed tools and Claude's
    // kept tool lists still name, is today's page under that name; only
    // today's is listed.
    let (_, _, body) = rpc(
        &h,
        Some(&session),
        json!({"jsonrpc": "2.0", "id": 5, "method": "resources/read",
               "params": {"uri": "ui://flash/study-panel-v2.html"}}),
    )
    .await;
    let earlier = result_of(&body)["contents"][0].clone();
    assert_eq!(earlier["uri"], "ui://flash/study-panel-v2.html", "{body}");
    assert_eq!(earlier["text"], content["text"]);
    assert_ne!(uri, "ui://flash/study-panel-v2.html");

    let (_, _, body) = rpc(
        &h,
        Some(&session),
        json!({"jsonrpc": "2.0", "id": 4, "method": "resources/read",
               "params": {"uri": "ui://flash/other.html"}}),
    )
    .await;
    assert!(body.contains("\"error\""), "{body}");
}

/// Every panel version a host may still hold answers: a bump that forgets
/// to keep the version it replaces leaves that host with "not found".
#[test]
fn every_earlier_panel_version_still_answers() {
    use flash_server::mcp_panel::{is_panel, URI};
    let current: u32 = URI
        .trim_start_matches("ui://flash/panel-v")
        .trim_end_matches(".html")
        .parse()
        .unwrap();
    for v in 3..=current {
        let uri = format!("ui://flash/panel-v{v}.html");
        assert!(is_panel(&uri), "{uri} is not answered");
    }
}

/// A request the way a client on MCP 2026-07-28 sends it (claude.ai's
/// backend does): no `initialize`, the version and the client's
/// capabilities in the request's own `_meta`.
async fn call_2026(h: &Harness, id: i64, method: &str, mut params: Value, caps: Value) -> String {
    params["_meta"] = json!({
        "io.modelcontextprotocol/protocolVersion": "2026-07-28",
        "io.modelcontextprotocol/clientInfo": {"name": "test", "version": "0"},
        "io.modelcontextprotocol/clientCapabilities": caps,
    });
    // The standard headers mirror the body: the method, and the tool's
    // name or the resource's URI.
    let name = params["name"]
        .as_str()
        .or(params["uri"].as_str())
        .map(str::to_string);
    let mut headers = vec![
        ("MCP-Protocol-Version", "2026-07-28"),
        ("Mcp-Method", method),
    ];
    if let Some(name) = name.as_deref() {
        headers.push(("Mcp-Name", name));
    }
    let (status, _, body) = rpc_with(
        h,
        None,
        json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}),
        &headers,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{method}: {body}");
    body
}

/// A 2026-07-28 client requires cache hints on every list and read
/// result; without them claude.ai could not load the panel ("Unable to
/// reach Flash").
#[tokio::test]
async fn the_panel_resource_carries_the_cache_hints_a_current_client_requires() {
    let h = harness();
    for (id, method, params) in [
        (2, "resources/list", json!({})),
        (
            3,
            "resources/read",
            json!({"uri": "ui://flash/study-panel-v2.html"}),
        ),
    ] {
        let body = call_2026(&h, id, method, params, ui_host()).await;
        let result = result_of(&body);
        assert!(result["ttlMs"].is_u64(), "{method}: {body}");
        assert!(result["cacheScope"].is_string(), "{method}: {body}");
    }
}

/// Such a client (naming the extension per request, not in a handshake)
/// gets the panel's view in `_meta`, and the model reads exactly what any
/// other client's does: a host saying it can show a panel is not a host
/// showing ours, so the model is never told the card is in one.
#[tokio::test]
async fn a_current_client_gets_the_panel_view_and_the_model_reads_the_card_as_always() {
    let h = harness();
    let session = initialize(&h).await;
    two_cards(&h, &session).await;
    let mut seen = Vec::new();
    for (id, caps) in [(4, ui_host()), (5, json!({}))] {
        let body = call_2026(
            &h,
            id,
            "tools/call",
            json!({"name": "start_study_session", "arguments": {}}),
            caps,
        )
        .await;
        let result = result_of(&body);
        assert!(result["_meta"]["flash/panel"].is_object(), "{body}");
        let mut out = tool_json(&result);
        assert!(out.get("study_panel").is_none(), "{body}");
        assert!(out["first_card"]["front_html"].is_string(), "{body}");
        out["session_id"] = json!(0);
        seen.push(out);
    }
    assert_eq!(seen[0], seen[1]);
}

#[tokio::test]
async fn the_panel_reaches_only_the_tools_it_needs() {
    let h = harness();
    let session = initialize(&h).await;
    let tools = tools(&h, &session).await;
    let visibility = |name: &str| -> Vec<String> {
        let tool = tools
            .iter()
            .find(|t| t["name"] == name)
            .unwrap_or_else(|| panic!("no tool {name}"));
        serde_json::from_value(tool["_meta"]["ui"]["visibility"].clone())
            .unwrap_or_else(|_| panic!("{name} declares no visibility: {tool}"))
    };
    for tool in &tools {
        let name = tool["name"].as_str().unwrap();
        let who = visibility(name);
        let expected: &[&str] = match name {
            "panel_show_answer" | "panel_grade" | "panel_cards" | "panel_reminders"
            | "panel_edit_source" | "panel_save_edit" | "panel_archive" => &["app"],
            "start_study_session" | "end_session" | "boost_new_today" => &["model", "app"],
            _ => &["model"],
        };
        assert_eq!(who, expected, "{name}");
        // Some hosts cut a description near 500 characters. The study
        // tools carry the grading protocol and run longer on purpose,
        // with "when to use" first; the card tools and the panel's own
        // fit whole.
        let description = tool["description"].as_str().unwrap();
        if !matches!(name, "start_study_session" | "submit_review") {
            assert!(
                description.len() < 500,
                "{name}: {} chars",
                description.len()
            );
        }
    }
    let start = tools
        .iter()
        .find(|t| t["name"] == "start_study_session")
        .unwrap();
    let uri = start["_meta"]["ui"]["resourceUri"].as_str().unwrap();
    assert!(uri.starts_with("ui://flash/"));
    assert_eq!(start["_meta"]["openai/outputTemplate"], uri);
    // The older flat key, which the MCP Apps library still writes.
    assert_eq!(start["_meta"]["ui/resourceUri"], uri);
    assert!(start["outputSchema"].to_string().contains("first_card"));
    // The card tools open the same panel (only the model calls them) and
    // keep the output schema they always had; restore_cards answers in
    // update_cards' fields.
    for name in ["create_cards", "update_cards", "restore_cards"] {
        let tool = tools.iter().find(|t| t["name"] == name).unwrap();
        assert_eq!(tool["_meta"]["ui"]["resourceUri"], uri, "{name}");
        assert_eq!(tool["_meta"]["openai/outputTemplate"], uri, "{name}");
        assert_eq!(tool["_meta"]["openai/widgetAccessible"], false, "{name}");
        assert!(
            tool["outputSchema"].to_string().contains("deck_totals"),
            "{name}: {tool}"
        );
    }
}

#[tokio::test]
async fn a_session_carries_the_panel_view_beside_what_the_model_reads() {
    let h = harness();
    let session = initialize_with(&h, ui_host()).await;
    two_cards(&h, &session).await;
    let result = call_raw(
        &h,
        &session,
        "start_study_session",
        json!({"deck": "Pharm"}),
    )
    .await;
    let out = tool_json(&result);
    assert_eq!(out["cards_due"], 2);
    assert!(out.get("study_panel").is_none(), "{out}");
    let panel = &result["_meta"]["flash/panel"];
    assert_eq!(panel["state"], "card", "{panel}");
    assert_eq!(panel["session_id"], out["session_id"]);
    assert_eq!(panel["card"]["card_id"], out["first_card"]["card_id"]);
    assert_eq!(panel["remaining"], 2);
    assert_eq!(panel["deck_label"], "Pharm");
    assert_eq!(panel["deck"], "Pharm");
    // The model's copy never has the back.
    assert!(!result["content"].to_string().contains("Vitamin K"));
    assert!(!result["content"].to_string().contains("Protamine"));

    // A subdeck and tags read as the site shows them.
    call_tool(
        &h,
        &session,
        8,
        "create_cards",
        json!({"deck": "Cardio::ECG", "cards": [
            {"front_html": "Name this rhythm", "back_html": "Flutter", "tags": ["exam-2"]}
        ]}),
    )
    .await;
    let sub = call_raw(
        &h,
        &session,
        "start_study_session",
        json!({"deck": "Cardio::ECG", "tags": ["exam-2"]}),
    )
    .await;
    assert_eq!(sub["_meta"]["flash/panel"]["deck_label"], "Cardio › ECG");
    assert_eq!(sub["_meta"]["flash/panel"]["deck"], "Cardio::ECG");
    assert_eq!(sub["_meta"]["flash/panel"]["tags"], json!(["exam-2"]));
}

#[tokio::test]
async fn the_panel_reveals_grades_and_never_grades_a_card_twice() {
    let h = harness();
    let session = initialize_with(&h, ui_host()).await;
    two_cards(&h, &session).await;
    let started = call_raw(&h, &session, "start_study_session", json!({})).await;
    let sid = started["_meta"]["flash/panel"]["session_id"]
        .as_i64()
        .unwrap();
    let first = started["_meta"]["flash/panel"]["card"]["card_id"]
        .as_i64()
        .unwrap();
    assert_eq!(started["_meta"]["flash/panel"]["view"], "study");
    assert_eq!(started["_meta"]["flash/panel"]["reviewed"], 0);
    // The card expected next rides along, front only, so a grade can show
    // it at once.
    let ahead = &started["_meta"]["flash/panel"]["next_card"];
    assert_ne!(ahead["card_id"].as_i64().unwrap(), first, "{started}");
    assert!(ahead.get("back_html").is_none(), "{ahead}");

    let answer = call_tool(
        &h,
        &session,
        20,
        "panel_show_answer",
        json!({"card_id": first}),
    )
    .await;
    assert_eq!(answer["card_id"], first);
    assert!(
        answer["back_html"].as_str().unwrap().contains("Vitamin K")
            || answer["back_html"].as_str().unwrap().contains("Protamine")
    );
    // Each grade button says when the card comes back: a new card's Again
    // and Hard stay within the session, Good and Easy move it days on.
    let back_in: Vec<i64> = answer["comes_back_in_ms"]
        .as_array()
        .unwrap()
        .iter()
        .map(|ms| ms.as_i64().unwrap())
        .collect();
    let (minute, day) = (60_000, 86_400_000);
    assert_eq!(back_in[..2], [10 * minute, 15 * minute], "{answer}");
    assert!(back_in[2] >= day && back_in[3] > back_in[2], "{answer}");
    // Showing the answer records nothing.
    assert_eq!(review_count(&h), 0);
    // A session with cards left has no week ahead to show yet.
    assert!(view(&started).get("week").is_none(), "{started}");

    let graded = call_tool(
        &h,
        &session,
        22,
        "panel_grade",
        json!({"session_id": sid, "card_id": first, "rating": 3}),
    )
    .await;
    assert_eq!(graded["recorded"], true, "{graded}");
    assert_eq!(graded["state"], "card");
    assert_eq!(graded["remaining"], 1);
    // The progress: one reviewed, one to go.
    assert_eq!(graded["reviewed"], 1);
    let second = graded["card"]["card_id"].as_i64().unwrap();
    assert_ne!(second, first);
    assert_eq!(second, ahead["card_id"].as_i64().unwrap(), "{graded}");
    // The last card has none after it.
    assert!(graded.get("next_card").is_none(), "{graded}");

    // The model grades the second card in chat; the panel, still showing
    // it, taps a grade later: not recorded again.
    call_tool(
        &h,
        &session,
        23,
        "submit_review",
        json!({"session_id": sid, "card_id": second, "rating": 3}),
    )
    .await;
    let late = call_tool(
        &h,
        &session,
        24,
        "panel_grade",
        json!({"session_id": sid, "card_id": second, "rating": 1}),
    )
    .await;
    assert_eq!(late["recorded"], false, "{late}");
    assert_eq!(late["state"], "caught_up", "{late}");
    // The chat's review counts toward the session's progress too.
    assert_eq!(late["reviewed"], 2, "{late}");
    assert_eq!(review_count(&h), 2);
    // Caught up: both cards come back on a later day, which the panel says.
    let next = &late["next_due"];
    assert!(next["in_days"].as_u64().unwrap() >= 1, "{late}");
    assert!(next["cards"].as_u64().unwrap() >= 1, "{late}");
    // And the week ahead, a study day at a time, agrees with it.
    let week = late["week"].as_array().unwrap();
    assert_eq!(week.len(), 7, "{late}");
    let day = next["in_days"].as_u64().unwrap() as usize;
    if day <= 7 {
        assert_eq!(week[day - 1]["cards"], next["cards"], "{late}");
    }
    assert!(week
        .windows(2)
        .all(|w| w[0]["at_ms"].as_i64() < w[1]["at_ms"].as_i64()));
    // "Study on the web" opens the site, counted as the panel's.
    assert_eq!(
        late["site_url"], "http://localhost:8437/?via=panel",
        "{late}"
    );
    // The open server sends no reminders, so it never offers them, and
    // the panel's tool for them turns nothing on.
    assert!(late.get("reminders_offer").is_none(), "{late}");
    let reminders = call_tool(
        &h,
        &session,
        25,
        "panel_reminders",
        json!({"timezone": "America/New_York"}),
    )
    .await;
    assert_eq!(reminders, json!({"on": false}));
}

fn missing_required(root: &Value, schema: &Value, value: &Value, at: &str) -> Vec<String> {
    if let Some(r) = schema["$ref"].as_str() {
        let name = r.trim_start_matches("#/$defs/");
        return missing_required(root, &root["$defs"][name], value, at);
    }
    if let Some(branches) = schema["anyOf"].as_array() {
        if value.is_null() {
            return Vec::new();
        }
        let typed = branches.iter().find(|b| b["type"] != "null").unwrap();
        return missing_required(root, typed, value, at);
    }
    let mut missing = Vec::new();
    if let Some(object) = value.as_object() {
        for key in schema["required"].as_array().into_iter().flatten() {
            let key = key.as_str().unwrap();
            if !object.contains_key(key) {
                missing.push(format!("{at}.{key}"));
            }
        }
        if let Some(props) = schema["properties"].as_object() {
            for (key, sub) in props {
                if let Some(v) = object.get(key) {
                    missing.extend(missing_required(root, sub, v, &format!("{at}.{key}")));
                }
            }
        }
    }
    if let (Some(items), Some(values)) = (schema.get("items"), value.as_array()) {
        for (i, v) in values.iter().enumerate() {
            missing.extend(missing_required(root, items, v, &format!("{at}[{i}]")));
        }
    }
    missing
}

/// Some hosts check a tool's result against its declared output schema
/// and turn a mismatch into an error: a panel field left out when empty
/// but still listed as required made every grade fail there. A session
/// with no tags, graded to the end in the panel and in the chat, answers
/// every call with what its schema promises.
#[tokio::test]
async fn every_study_result_has_what_its_output_schema_requires() {
    let h = harness();
    let session = initialize_with(&h, ui_host()).await;
    two_cards(&h, &session).await;
    let schemas: std::collections::HashMap<String, Value> = tools(&h, &session)
        .await
        .into_iter()
        .filter(|t| t.get("outputSchema").is_some())
        .map(|t| {
            (
                t["name"].as_str().unwrap().to_owned(),
                t["outputSchema"].clone(),
            )
        })
        .collect();
    let check = |name: &str, result: &Value| {
        let schema = &schemas[name];
        let out = &result["structuredContent"];
        assert!(out.is_object(), "{name}: {result}");
        let missing = missing_required(schema, schema, out, name);
        assert!(missing.is_empty(), "{name} lacks {missing:?}: {out}");
    };

    let started = call_raw(&h, &session, "start_study_session", json!({})).await;
    check("start_study_session", &started);
    let sid = started["_meta"]["flash/panel"]["session_id"]
        .as_i64()
        .unwrap();
    let first = started["_meta"]["flash/panel"]["card"]["card_id"]
        .as_i64()
        .unwrap();
    let shown = call_raw(&h, &session, "panel_show_answer", json!({"card_id": first})).await;
    check("panel_show_answer", &shown);
    let graded = call_raw(
        &h,
        &session,
        "panel_grade",
        json!({"session_id": sid, "card_id": first, "rating": 3}),
    )
    .await;
    check("panel_grade", &graded);
    let second = graded["structuredContent"]["card"]["card_id"]
        .as_i64()
        .unwrap();
    let reviewed = call_raw(
        &h,
        &session,
        "submit_review",
        json!({"session_id": sid, "card_id": second, "rating": 3}),
    )
    .await;
    check("submit_review", &reviewed);
    let late = call_raw(
        &h,
        &session,
        "panel_grade",
        json!({"session_id": sid, "card_id": second, "rating": 1}),
    )
    .await;
    check("panel_grade", &late);
    assert_eq!(late["structuredContent"]["state"], "caught_up", "{late}");
    let cards = call_raw(
        &h,
        &session,
        "panel_cards",
        json!({"card_ids": [first, second]}),
    )
    .await;
    check("panel_cards", &cards);
}

#[tokio::test]
async fn a_caught_up_session_tells_the_model_when_the_next_cards_come() {
    let h = harness();
    let session = initialize_with(&h, ui_host()).await;
    two_cards(&h, &session).await;
    let started = call_raw(&h, &session, "start_study_session", json!({})).await;
    let sid = view(&started)["session_id"].as_i64().unwrap();
    for (id, card) in [(30, 0), (31, 1)] {
        let current = if card == 0 {
            view(&started)["card"]["card_id"].as_i64().unwrap()
        } else {
            let again = call_raw(&h, &session, "start_study_session", json!({})).await;
            view(&again)["card"]["card_id"].as_i64().unwrap()
        };
        call_tool(
            &h,
            &session,
            id,
            "submit_review",
            json!({"session_id": sid, "card_id": current, "rating": 3}),
        )
        .await;
    }
    let done = call_raw(&h, &session, "start_study_session", json!({})).await;
    let out = tool_json(&done);
    assert_eq!(out["cards_due"], 0, "{out}");
    assert!(out["next_due"]["in_days"].as_u64().unwrap() >= 1, "{out}");
    let note = out["note"].as_str().unwrap();
    assert!(note.contains("The next cards come"), "{note}");
    assert_eq!(view(&done)["state"], "caught_up");
    assert!(view(&done)["next_due"].is_object());
}

/// The panel's `_meta` view of a tool result.
fn view(result: &Value) -> &Value {
    &result["_meta"]["flash/panel"]
}

#[tokio::test]
async fn made_cards_open_the_panel_and_the_model_reads_what_it_always_did() {
    let h = harness();
    let session = initialize_with(&h, ui_host()).await;
    let result = call_raw(
        &h,
        &session,
        "create_cards",
        json!({"deck": "Cardio::ECG", "cards": [
            {"front_html": "Antidote for warfarin?", "back_html": "Vitamin K", "tags": ["exam-2"]},
            {"note_type": "cloze", "front_html": "{{c1::Digoxin}} slows the {{c2::AV node}}", "back_html": ""}
        ]}),
    )
    .await;
    // What the model reads: the same fields as ever, and a note that the
    // cards are on screen (this host renders the panel).
    let out = tool_json(&result);
    let mut keys: Vec<&str> = out
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "card_ids",
            "created",
            "deck_totals",
            "note",
            "source_ids",
            "updated"
        ],
        "{out}"
    );
    assert_eq!(result["structuredContent"], out);
    assert!(!result["content"].to_string().contains("flash/panel"));

    // The panel's view: every card made, the cloze note's two included,
    // both sides, in the deck the site names.
    let panel = view(&result);
    assert_eq!(panel["view"], "cards", "{panel}");
    assert_eq!(panel["mode"], "created");
    assert_eq!(panel["deck"], "Cardio::ECG");
    assert_eq!(panel["deck_label"], "Cardio › ECG");
    // Where the site lists every card in that deck.
    let deck_url = panel["deck_url"].as_str().unwrap();
    assert!(
        deck_url.starts_with("http") && deck_url.contains("/decks/"),
        "{panel}"
    );
    assert_eq!(panel["card_ids"], out["card_ids"]);
    let cards = panel["cards"].as_array().unwrap();
    assert_eq!(cards.len(), 3, "{panel}");
    assert!(cards[0]["back_html"]
        .as_str()
        .unwrap()
        .contains("Vitamin K"));
    assert_eq!(cards[0]["tags"], json!(["exam-2"]));
    assert!(cards[0].get("before").is_none());
    assert!(cards[1]["back_html"].as_str().unwrap().contains("Digoxin"));
}

/// Cards the panel shows need no retelling: a host that renders the panel
/// is told so beside the result, and the model confirms in a line. Every
/// host is told where else new cards can be studied: the site.
#[tokio::test]
async fn a_panel_host_is_told_the_cards_are_on_screen() {
    let note = |result: &Value| result["structuredContent"]["note"].clone();
    let cards = json!({"deck": "Pharm", "cards": [
        {"front_html": "Antidote for warfarin?", "back_html": "Vitamin K"}
    ]});
    let site = "study them any time: http://localhost:8437/study?via=chat";

    let h = harness();
    let session = initialize_with(&h, ui_host()).await;
    let made = call_raw(&h, &session, "create_cards", cards.clone()).await;
    let said = note(&made).as_str().unwrap_or("").to_string();
    assert!(said.contains("sees these cards in Flash's panel"), "{made}");
    assert!(said.ends_with(site), "{made}");
    assert_eq!(tool_json(&made)["created"], 1);
    let card = tool_json(&made)["card_ids"][0].clone();
    let edited = call_raw(
        &h,
        &session,
        "update_cards",
        json!({"cards": [{"card_id": card, "back_html": "Vitamin K (phytonadione)"}]}),
    )
    .await;
    assert!(note(&edited).is_string(), "{edited}");
    // Listing a card's earlier versions is for the user to choose from.
    let listed = call_raw(&h, &session, "restore_cards", json!({"card_id": card})).await;
    assert!(note(&listed).is_null(), "{listed}");

    let plain = harness_named("no-panel-host");
    let session = initialize(&plain).await;
    let made = call_raw(&plain, &session, "create_cards", cards.clone()).await;
    let said = note(&made).as_str().unwrap_or("").to_string();
    assert!(!said.contains("panel"), "{made}");
    assert!(said.ends_with(site), "{made}");

    // Grok's backend introduces itself so, names the extension, and shows
    // the view below its reply.
    let grok = harness_named("grok-shows-it-below");
    let session = initialize_as(&grok, "connectors-manager", ui_host()).await;
    let made = call_raw(&grok, &session, "create_cards", cards).await;
    assert!(note(&made).is_string(), "{made}");
}

#[tokio::test]
async fn the_panel_pages_through_many_cards_twenty_at_a_time() {
    let h = harness();
    let session = initialize_with(&h, ui_host()).await;
    let cards: Vec<Value> = (1..=25)
        .map(|n| json!({"front_html": format!("Question {n}"), "back_html": format!("Answer {n}")}))
        .collect();
    let result = call_raw(
        &h,
        &session,
        "create_cards",
        json!({"deck": "Many", "cards": cards}),
    )
    .await;
    let panel = view(&result);
    let ids = panel["card_ids"].as_array().unwrap().clone();
    assert_eq!(ids.len(), 25);
    assert_eq!(
        panel["cards"].as_array().unwrap().len(),
        20,
        "the first page"
    );

    let rest = call_tool(
        &h,
        &session,
        60,
        "panel_cards",
        json!({"card_ids": ids[20..]}),
    )
    .await;
    let rest = rest["cards"].as_array().unwrap();
    assert_eq!(rest.len(), 5);
    assert!(rest[4]["back_html"].as_str().unwrap().contains("Answer 25"));

    // More than a page, or a card that is not the user's, is refused.
    for (id, card_ids) in [(61, json!(ids[..21])), (62, json!([999_999]))] {
        let (_, _, body) = rpc(
            &h,
            Some(&session),
            json!({"jsonrpc": "2.0", "id": id, "method": "tools/call",
                   "params": {"name": "panel_cards", "arguments": {"card_ids": card_ids}}}),
        )
        .await;
        assert!(
            body.contains("\"error\"") || body.contains("\"isError\":true"),
            "{body}"
        );
    }
}

#[tokio::test]
async fn edited_cards_show_the_side_that_changed_beside_what_it_said() {
    let h = harness();
    let session = initialize_with(&h, ui_host()).await;
    two_cards(&h, &session).await;
    let listed = call_tool(&h, &session, 70, "list_cards", json!({"deck": "Pharm"})).await;
    let card = listed["cards"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["back_html"].as_str().unwrap().contains("Vitamin K"))
        .unwrap()
        .clone();
    let result = call_raw(
        &h,
        &session,
        "update_cards",
        json!({"cards": [{"card_id": card["card_id"], "back_html": "Vitamin K (phytonadione)"}]}),
    )
    .await;
    assert_eq!(tool_json(&result)["updated"], 1);
    let panel = view(&result);
    assert_eq!(panel["view"], "cards", "{panel}");
    assert_eq!(panel["mode"], "edited");
    assert_eq!(panel["deck"], "Pharm");
    let shown = &panel["cards"][0];
    assert_eq!(shown["card_id"], card["card_id"]);
    assert!(shown["back_html"]
        .as_str()
        .unwrap()
        .contains("phytonadione"));
    let before = &shown["before"];
    assert!(
        before["back_html"].as_str().unwrap().contains("Vitamin K"),
        "{shown}"
    );
    assert!(!before["back_html"]
        .as_str()
        .unwrap()
        .contains("phytonadione"));
    // The front did not change, so there is no old front to show.
    assert!(before.get("front_html").is_none(), "{shown}");

    // Asking for the card's earlier versions shows it beside the newest
    // one: what a restore would bring back.
    let listed = call_raw(
        &h,
        &session,
        "restore_cards",
        json!({"card_id": card["card_id"]}),
    )
    .await;
    let revision = tool_json(&listed)["revisions"][0]["revision_id"].clone();
    let shown = &view(&listed)["cards"][0];
    assert_eq!(view(&listed)["mode"], "edited", "{listed}");
    assert!(shown["back_html"]
        .as_str()
        .unwrap()
        .contains("phytonadione"));
    let was = shown["before"]["back_html"].as_str().unwrap();
    assert!(
        was.contains("Vitamin K") && !was.contains("phytonadione"),
        "{shown}"
    );

    // The restore shows the card back as it was, beside the edit it undid.
    let restored = call_raw(
        &h,
        &session,
        "restore_cards",
        json!({"card_id": card["card_id"], "revision_id": revision}),
    )
    .await;
    assert_eq!(tool_json(&restored)["updated"], 1);
    let shown = &view(&restored)["cards"][0];
    assert!(!shown["back_html"]
        .as_str()
        .unwrap()
        .contains("phytonadione"));
    assert!(shown["before"]["back_html"]
        .as_str()
        .unwrap()
        .contains("phytonadione"));
}

fn review_count(h: &Harness) -> i64 {
    h.store
        .with_conn(|c| Ok(c.query_row("SELECT COUNT(*) FROM review_log", [], |r| r.get(0))?))
        .unwrap()
}

#[tokio::test]
async fn card_media_loads_from_a_signed_link_and_nothing_else() {
    let h = harness_named("mcp-panel-media");
    let media = DiskStore::new(media_dir(&h.data_dir));
    let id = ingest_media(&h.services, &media, h.user, false, "cat.png", PNG, now_ms()).unwrap();
    let session = initialize_with(&h, ui_host()).await;
    call_tool(
        &h,
        &session,
        30,
        "create_cards",
        json!({"deck": "Cats", "cards": [
            {"front_html": format!("Which cat?<img src=\"/media/{}\">", id.0), "back_html": "Tabby"}
        ]}),
    )
    .await;
    let started = call_raw(&h, &session, "start_study_session", json!({})).await;
    let front = started["_meta"]["flash/panel"]["card"]["front_html"]
        .as_str()
        .unwrap()
        .to_string();
    // The model's copy keeps the plain reference.
    assert!(tool_json(&started)["first_card"]["front_html"]
        .as_str()
        .unwrap()
        .contains(&format!("src=\"/media/{}\"", id.0)));
    let at = front.find("/mcp-media/").expect("a signed link");
    let link = front[at..].split('"').next().unwrap().replace("&amp;", "&");
    assert!(link.contains("/s?e="), "{link}");

    // No credentials needed: the signature is the credential.
    let (status, bytes) = get(&h, &link).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(bytes, PNG);

    let tampered = link.replace("&s=", "&s=A");
    assert_eq!(get(&h, &tampered).await.0, StatusCode::FORBIDDEN);
    let (base, query) = link.split_once('?').unwrap();
    let later = query
        .split('&')
        .map(|kv| match kv.strip_prefix("e=") {
            Some(e) => format!("e={}", e.parse::<i64>().unwrap() + 1),
            None => kv.to_string(),
        })
        .collect::<Vec<_>>()
        .join("&");
    assert_eq!(
        get(&h, &format!("{base}?{later}")).await.0,
        StatusCode::FORBIDDEN
    );
    let other = link.replacen(&format!("/mcp-media/{}/", id.0), "/mcp-media/999999/", 1);
    assert_eq!(get(&h, &other).await.0, StatusCode::FORBIDDEN);
    assert!(get(&h, &format!("/mcp-media/{}/s", id.0))
        .await
        .0
        .is_client_error());
    // The Bearer route still wants its Bearer.
    assert_eq!(
        get(&h, &format!("/mcp-media/{}", id.0)).await.0,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn an_account_with_no_cards_is_offered_its_first_ones_not_called_caught_up() {
    let h = harness();
    let session = initialize_with(&h, ui_host()).await;
    let decks = call_tool(&h, &session, 40, "list_decks", json!({})).await;
    assert!(
        decks["note"].as_str().unwrap().contains("no cards yet"),
        "{decks}"
    );
    let started = call_raw(&h, &session, "start_study_session", json!({})).await;
    let note = tool_json(&started)["note"].as_str().unwrap().to_string();
    assert!(note.contains("no cards yet"), "{note}");
    assert!(!note.contains("caught up"), "{note}");
    assert_eq!(started["_meta"]["flash/panel"]["state"], "empty");

    // A deck of its own with nothing in it reads as empty, not caught up.
    call_tool(&h, &session, 41, "create_deck", json!({"name": "Spanish"})).await;
    let scoped = call_tool(
        &h,
        &session,
        42,
        "start_study_session",
        json!({"deck": "Spanish"}),
    )
    .await;
    let note = scoped["note"].as_str().unwrap();
    assert!(note.contains("no card is in this deck"), "{note}");

    // With cards, the old answers stand: no note on the decks, and once
    // everything is graded the session is caught up.
    two_cards(&h, &session).await;
    let decks = call_tool(&h, &session, 43, "list_decks", json!({})).await;
    assert!(decks.get("note").is_none(), "{decks}");
    let started = call_tool(
        &h,
        &session,
        44,
        "start_study_session",
        json!({"deck": "Pharm"}),
    )
    .await;
    let sid = started["session_id"].as_i64().unwrap();
    let mut card = started["first_card"]["card_id"].as_i64();
    while let Some(id) = card {
        let r = call_tool(
            &h,
            &session,
            45,
            "submit_review",
            json!({"session_id": sid, "card_id": id, "rating": 4}),
        )
        .await;
        card = r["next_card"]["card_id"].as_i64();
    }
    let again = call_tool(
        &h,
        &session,
        46,
        "start_study_session",
        json!({"deck": "Pharm"}),
    )
    .await;
    assert!(
        again["note"].as_str().unwrap().contains("caught up"),
        "{again}"
    );
}

#[tokio::test]
async fn card_fields_carry_the_writing_guidance() {
    let h = harness();
    let session = initialize(&h).await;
    let tools = tools(&h, &session).await;
    for name in ["create_cards", "update_cards"] {
        let schema = tools.iter().find(|t| t["name"] == name).unwrap()["inputSchema"].to_string();
        assert!(schema.contains("answerable on its own"), "{name}: {schema}");
        assert!(schema.contains("as short as it can be"), "{name}: {schema}");
        assert!(schema.contains("<sup>"), "{name}: {schema}");
    }
}

async fn call_refused(h: &Harness, session: &str, name: &str, args: Value) -> Value {
    let (_, _, body) = rpc(
        h,
        Some(session),
        json!({"jsonrpc": "2.0", "id": 79, "method": "tools/call",
               "params": {"name": name, "arguments": args}}),
    )
    .await;
    let frame = body
        .lines()
        .filter_map(|l| l.strip_prefix("data:"))
        .map(str::trim)
        .find(|payload| payload.starts_with('{'))
        .unwrap_or(&body);
    let error = serde_json::from_str::<Value>(frame).unwrap()["error"].clone();
    assert!(error["message"].is_string(), "not refused: {body}");
    error
}

async fn studied(h: &Harness, session: &str) -> (i64, i64) {
    let started = call_raw(h, session, "start_study_session", json!({})).await;
    let p = view(&started);
    (
        p["session_id"].as_i64().unwrap(),
        p["card"]["card_id"].as_i64().unwrap(),
    )
}

#[tokio::test]
async fn the_quick_edit_saves_the_cards_fields_and_shows_it_again() {
    let h = harness_named("mcp-panel-edit");
    let s = initialize_with(&h, ui_host()).await;
    call_tool(
        &h,
        &s,
        3,
        "create_cards",
        json!({"deck": "Anatomy", "cards": [
            {"front_html": "Nerve to the deltoid?", "back_html": "Axilary nerve", "tags": ["arm"]}
        ]}),
    )
    .await;
    let (_, card) = studied(&h, &s).await;

    let source = tool_json(&call_raw(&h, &s, "panel_edit_source", json!({"card_id": card})).await);
    assert_eq!(source["note_type"], "basic", "{source}");
    assert_eq!(source["labels"], json!(["Front", "Back"]));
    assert_eq!(source["front_html"], "Nerve to the deltoid?");
    assert_eq!(source["back_html"], "Axilary nerve");
    assert_eq!(
        source["card_url"],
        format!("http://localhost:8437/cards/{card}?via=panel"),
        "{source}"
    );

    let saved = tool_json(
        &call_raw(
            &h,
            &s,
            "panel_save_edit",
            json!({"card_id": card, "front_html": "Nerve to the deltoid?",
                   "back_html": "Axillary nerve (C5-C6)"}),
        )
        .await,
    );
    assert_eq!(saved["card"]["card_id"], card, "{saved}");
    assert_eq!(saved["card"]["front_html"], "Nerve to the deltoid?");
    assert_eq!(saved["answer"]["back_html"], "Axillary nerve (C5-C6)");
    assert!(saved["answer"]["comes_back_in_ms"].is_array(), "{saved}");

    let row = h.services.card(h.user, flash_core::CardId(card)).unwrap();
    assert_eq!(row.back, "Axillary nerve (C5-C6)");
    assert_eq!(row.tags, vec!["arm"]);
    assert_eq!(
        h.services
            .card_revisions(h.user, flash_core::CardId(card))
            .unwrap()
            .len(),
        1
    );
    let state = h
        .store
        .get_card_state(h.user, flash_core::CardId(card))
        .unwrap();
    assert!(state.last_review_ms.is_none(), "an edit is no review");
}

#[tokio::test]
async fn a_cloze_shows_its_text_and_an_edit_that_takes_its_card_away_is_refused() {
    let h = harness_named("mcp-panel-edit-cloze");
    let s = initialize_with(&h, ui_host()).await;
    call_tool(
        &h,
        &s,
        3,
        "create_cards",
        json!({"deck": "Pharm", "cards": [
            {"note_type": "cloze", "front_html": "{{c1::Digoxin}} slows the {{c2::AV node}}", "back_html": ""}
        ]}),
    )
    .await;
    let (_, card) = studied(&h, &s).await;
    let source = tool_json(&call_raw(&h, &s, "panel_edit_source", json!({"card_id": card})).await);
    assert_eq!(source["labels"], json!(["Text", "Extra"]), "{source}");
    assert_eq!(
        source["front_html"], "{{c1::Digoxin}} slows the {{c2::AV node}}",
        "the note's text, both gaps, not the card's side"
    );

    let row = h.services.card(h.user, flash_core::CardId(card)).unwrap();
    let text = if row.front.contains("Digoxin") {
        "{{c1::Digoxin}} slows the AV node" // this card is c2's
    } else {
        "Digoxin slows the {{c2::AV node}}" // this card is c1's
    };
    let error = call_refused(
        &h,
        &s,
        "panel_save_edit",
        json!({"card_id": card, "front_html": text, "back_html": ""}),
    )
    .await;
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .contains("would remove the card you're studying"),
        "{error}"
    );
    let row = h.services.card(h.user, flash_core::CardId(card)).unwrap();
    assert!(
        row.front.contains("[...]") || row.front.contains("..."),
        "{}",
        row.front
    );
    let source = tool_json(&call_raw(&h, &s, "panel_edit_source", json!({"card_id": card})).await);
    assert_eq!(
        source["front_html"],
        "{{c1::Digoxin}} slows the {{c2::AV node}}"
    );
}

#[tokio::test]
async fn the_quick_edit_keeps_images_and_names_no_one_elses() {
    let h = harness_named("mcp-panel-edit-media");
    let media = DiskStore::new(media_dir(&h.data_dir));
    let id = ingest_media(&h.services, &media, h.user, false, "arm.png", PNG, now_ms()).unwrap();
    let s = initialize_with(&h, ui_host()).await;
    call_tool(
        &h,
        &s,
        30,
        "create_cards",
        json!({"deck": "Anatomy", "cards": [
            {"front_html": format!("Name the bone<img src=\"/media/{}\">", id.0), "back_html": "Humerus"}
        ]}),
    )
    .await;
    let (_, card) = studied(&h, &s).await;
    let source = tool_json(&call_raw(&h, &s, "panel_edit_source", json!({"card_id": card})).await);
    let front = source["front_html"].as_str().unwrap();
    assert!(
        front.contains("/mcp-media/"),
        "the panel loads it signed: {front}"
    );

    // Sent back as a browser serializes it, the image is the card's again.
    let edited = front.replace("Name the bone", "Name the highlighted bone");
    call_raw(
        &h,
        &s,
        "panel_save_edit",
        json!({"card_id": card, "front_html": edited, "back_html": "Humerus"}),
    )
    .await;
    let row = h.services.card(h.user, flash_core::CardId(card)).unwrap();
    assert_eq!(
        row.front_html.as_deref(),
        Some(format!("Name the highlighted bone<img src=\"/media/{}\">", id.0).as_str())
    );

    call_raw(
        &h,
        &s,
        "panel_save_edit",
        json!({"card_id": card, "front_html": "Name the highlighted bone", "back_html": "Humerus"}),
    )
    .await;
    let row = h.services.card(h.user, flash_core::CardId(card)).unwrap();
    assert!(!row.front_html.unwrap_or_default().contains("/media/"));

    let (other, _) = flash_server::testing::signed_in(&h.store, "Other", "other@example.com");
    let theirs = ingest_media(&h.services, &media, other, false, "x.png", PNG, now_ms()).unwrap();
    call_refused(
        &h,
        &s,
        "panel_save_edit",
        json!({"card_id": card, "front_html": format!("Mine<img src=\"/media/{}\">", theirs.0),
               "back_html": "Humerus"}),
    )
    .await;
    let row = h.services.card(h.user, flash_core::CardId(card)).unwrap();
    assert!(!row
        .front_html
        .unwrap_or_default()
        .contains(&theirs.0.to_string()));
}

#[test]
fn unsigning_undoes_signing() {
    use flash_server::mcp_panel::MediaLinks;
    let base = "https://flash.example";
    let signed = format!(
        "Bone<img src=\"{base}/mcp-media/12/s?e=99&amp;s=abc\"> and <img src=\"{base}/mcp-media/7\">"
    );
    assert_eq!(
        MediaLinks::unsign_html(&signed, base),
        "Bone<img src=\"/media/12\"> and <img src=\"/media/7\">"
    );
    let other = "<img src=\"https://elsewhere.example/mcp-media/12/s\"><img src=\"https://flash.example/mcp-media/x\">";
    assert_eq!(MediaLinks::unsign_html(other, base), other);
}

#[test]
fn a_script_loses_only_the_lines_that_are_all_comment() {
    use flash_server::mcp_panel::without_comment_lines as strip;
    let script = "// a line comment
const a = 1; // kept with its code
/**
 * A block, alone on its lines.
 */
  /* indented, one line */
/* a block that ends */ const b = 2;
/* a block
   that ends in code */ const c = 3;
const url = \"https://example.com\";
";
    assert_eq!(
        strip(script),
        "const a = 1; // kept with its code
/* a block that ends */ const b = 2;
/* a block
   that ends in code */ const c = 3;
const url = \"https://example.com\";
"
    );
    let open = "const a = 1;\n/* open\nstill\n";
    assert_eq!(strip(open), open);
    assert_eq!(strip("// only"), "");
    assert_eq!(strip("const a = 1;"), "const a = 1;");
}
