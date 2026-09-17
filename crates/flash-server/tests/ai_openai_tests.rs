//! The generic adapter end to end: a scripted loopback endpoint that
//! speaks the Chat Completions protocol stands in for any vendor or
//! local model server, and a generation goes through the whole
//! pipeline against it: source, job, the runner, drafts, and the ledger
//! rows that name the model the operator brought with the counts the
//! endpoint reported.

mod common;

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;

use axum::http::StatusCode;
use common::*;
use flash_server::ai::openai::OpenAiProvider;
use flash_server::ai::runner;
use flash_server::config::AiOutput;
use flash_server::policy::AiInput;
use flash_server::service::now_ms;
use serde_json::{json, Value};

/// A loopback server answering each `chat/completions` call with the
/// next of `replies`, and handing back what it was sent.
fn serve(replies: Vec<Value>) -> (String, std::thread::JoinHandle<Vec<Value>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let handle = std::thread::spawn(move || {
        let mut seen = Vec::new();
        for reply in replies {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = Vec::new();
            let mut chunk = [0u8; 8192];
            let (head_end, length) = loop {
                let n = stream.read(&mut chunk).unwrap();
                assert!(n > 0, "closed before the head");
                buf.extend_from_slice(&chunk[..n]);
                if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                    let head = String::from_utf8_lossy(&buf[..pos]).to_string();
                    let length = head
                        .lines()
                        .find_map(|l| {
                            l.split_once(':')
                                .filter(|(k, _)| k.eq_ignore_ascii_case("content-length"))
                        })
                        .map(|(_, v)| v.trim().parse::<usize>().unwrap())
                        .unwrap_or(0);
                    break (pos + 4, length);
                }
            };
            while buf.len() < head_end + length {
                let n = stream.read(&mut chunk).unwrap();
                assert!(n > 0, "closed before the body");
                buf.extend_from_slice(&chunk[..n]);
            }
            seen.push(serde_json::from_slice(&buf[head_end..head_end + length]).unwrap());
            let body = reply.to_string();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\
                 Connection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).unwrap();
            stream.flush().unwrap();
        }
        seen
    });
    (format!("http://127.0.0.1:{port}/v1"), handle)
}

fn completion(content: Value) -> Value {
    json!({
        "id": "chatcmpl-local",
        "model": "local-model-2026",
        "choices": [{ "message": { "role": "assistant", "content": content.to_string() }, "finish_reason": "stop" }],
        "usage": { "prompt_tokens": 120, "completion_tokens": 45 }
    })
}

#[tokio::test]
async fn a_generation_runs_through_a_local_endpoint_and_is_recorded_unpriced() {
    let extraction =
        json!({ "text": "The heart has four chambers.", "figures": [], "est_words": 5 });
    let drafts = json!({ "cards": [
        { "note_type": "basic", "front": "How many chambers does the heart have?", "back": "Four", "tags": ["anatomy"], "figures": [] },
        { "note_type": "cloze", "front": "The heart has {{c1::four}} chambers.", "back": "", "tags": [], "figures": [] }
    ] });
    let (base, server) = serve(vec![completion(extraction), completion(drafts)]);
    let provider = OpenAiProvider::new(
        base,
        String::new(),
        "local-model".into(),
        "local-model".into(),
        AiOutput::Schema,
    );
    let t = AppBuilder::new("ai-openai")
        .ai(Arc::new(provider), vec![AiInput::Text])
        .build();
    let user = member(&t.store, "FlashTester", "flashtester@example.com");
    let bearer = api_bearer(&t.store, user);
    let deck = t
        .services
        .create_deck(user, "Anatomy", "", now_ms())
        .unwrap();

    let source = send(
        &t.app,
        json_post(
            "/api/v1/ai/sources",
            Some(&bearer),
            &json!({ "text": "The heart has four chambers: two atria and two ventricles." }),
        ),
    )
    .await;
    assert_eq!(source.status, StatusCode::CREATED, "{}", source.text());
    let source_id = source.json()["id"].as_i64().unwrap();
    let job = send(
        &t.app,
        json_post(
            "/api/v1/ai/jobs",
            Some(&bearer),
            &json!({ "source_ids": [source_id], "deck_id": deck.0, "instruction": "" }),
        ),
    )
    .await;
    assert_eq!(job.status, StatusCode::CREATED, "{}", job.text());
    let job_id = job.json()["job_id"].as_i64().unwrap();

    assert_eq!(runner::run_once(&t.state).await, 1, "the job ran");
    let listed = send(
        &t.app,
        json_get(&format!("/api/v1/ai/jobs/{job_id}/drafts"), Some(&bearer)),
    )
    .await;
    assert_eq!(listed.status, StatusCode::OK, "{}", listed.text());
    let items = listed.json()["items"].as_array().cloned().unwrap();
    assert_eq!(items.len(), 2, "{items:?}");

    // Both calls went to the endpoint in the protocol's shape, in order.
    let seen = server.join().unwrap();
    assert_eq!(seen.len(), 2);
    assert_eq!(seen[0]["model"], "local-model");
    assert_eq!(
        seen[0]["response_format"]["json_schema"]["name"],
        "extraction"
    );
    assert_eq!(seen[1]["response_format"]["json_schema"]["name"], "drafts");
    assert_eq!(seen[0]["messages"][0]["role"], "system");

    // The ledger names the model the operator gave, with the counts the
    // endpoint reported.
    let rows: Vec<(String, String, i64, i64)> = t
        .store
        .with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT step, model, input_tokens, output_tokens FROM ai_calls \
                 WHERE ok = 1 ORDER BY id",
            )?;
            let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?;
            Ok(rows.collect::<rusqlite::Result<_>>()?)
        })
        .unwrap();
    assert_eq!(rows.len(), 2, "{rows:?}");
    for (step, model, input, output) in &rows {
        assert!(step == "extract" || step == "generate", "{step}");
        assert_eq!(model, "local-model");
        assert_eq!((*input, *output), (120, 45));
    }
}
