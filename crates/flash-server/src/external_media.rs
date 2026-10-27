//! Asynchronous localization of images referenced by MCP-created cards.

use std::time::Duration;

use flash_core::NoteId;
use serde::Deserialize;

use crate::{http, jobs::Outcome, media, state::AppState};

#[derive(Debug, Deserialize)]
struct Payload {
    note_id: i64,
    side: String,
    url: String,
}

/// Handles one external image. The generic jobs worker supplies durable
/// retries; a permanently invalid or unsupported resource is removed from the
/// note after the worker's final attempt.
pub fn job(state: &AppState, row: &flash_store::JobRow, now: i64) -> Outcome {
    let Some(user) = row.user else { return Outcome::Fail("job has no user".into()) };
    let payload: Payload = match serde_json::from_str(&row.payload) {
        Ok(value) => value,
        Err(e) => return Outcome::Fail(format!("invalid payload: {e}")),
    };
    if row.attempts > 3 {
        let _ = state.services.store().apply_external_media(
            user,
            NoteId(payload.note_id),
            &payload.side,
            &payload.url,
            None,
            Some("external image localization failed after three attempts"),
            now,
        );
        return Outcome::Fail("external image localization exhausted retries".into());
    }
    if !allowed_url(&payload.url) {
        let _ = state.services.store().apply_external_media(
            user,
            NoteId(payload.note_id),
            &payload.side,
            &payload.url,
            None,
            Some("external image URL is not allowed"),
            now,
        );
        return Outcome::Fail("external image URL is not allowed".into());
    }

    let result = (|| {
        let agent = http::agent(Duration::from_secs(15), false);
        let mut response = agent.get(&payload.url).call().map_err(|e| e.to_string())?;
        let status = response.status().as_u16();
        if !(200..300).contains(&status) {
            return Err(format!("external image HTTP {status}"));
        }
        let bytes = response
            .body_mut()
            .with_config()
            .limit(flash_store::media::MAX_FILE_BYTES + 1)
            .read_to_vec()
            .map_err(|e| e.to_string())?;
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok());
        let filename = filename_for(&payload.url, content_type);
        let id = media::ingest_media(
            &state.services,
            state.media.as_ref(),
            user,
            false,
            &filename,
            &bytes,
            now,
        )
        .map_err(|e| e.to_string())?;
        Ok::<_, String>(id)
    })();

    match result {
        Ok(id) => match state.services.store().apply_external_media(
            user,
            NoteId(payload.note_id),
            &payload.side,
            &payload.url,
            Some(id),
            None,
            now,
        ) {
            Ok(_) => Outcome::Done,
            Err(e) => Outcome::Retry(format!("apply localization: {e}")),
        },
        Err(why) => {
            if row.attempts >= 3 {
                let _ = state.services.store().apply_external_media(
                    user,
                    NoteId(payload.note_id),
                    &payload.side,
                    &payload.url,
                    None,
                    Some(&why),
                    now,
                );
                Outcome::Fail(why)
            } else {
                Outcome::Retry(why)
            }
        }
    }
}

fn allowed_url(url: &str) -> bool {
    let Ok(parsed) = url::Url::parse(url) else { return false };
    if !matches!(parsed.scheme(), "http" | "https") {
        return false;
    }
    let Some(host) = parsed.host_str() else { return false };
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        return match ip {
            std::net::IpAddr::V4(ip) => !(ip.is_private()
                || ip.is_loopback()
                || ip.is_link_local()
                || ip.is_unspecified()
                || ip.octets()[0] == 169 && ip.octets()[1] == 254),
            std::net::IpAddr::V6(ip) => !(ip.is_loopback() || ip.is_unspecified()),
        };
    }
    !host.eq_ignore_ascii_case("localhost")
}

fn filename_for(url: &str, content_type: Option<&str>) -> String {
    let path_name = url::Url::parse(url)
        .ok()
        .and_then(|u| {
            u.path_segments()
                .and_then(|mut p| p.next_back())
                .map(str::to_owned)
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "external-image".to_owned());
    if path_name.contains('.') {
        path_name.to_string()
    } else {
        let ext = match content_type.and_then(|v| v.split(';').next()).unwrap_or("") {
            "image/jpeg" => "jpg",
            "image/png" => "png",
            "image/gif" => "gif",
            "image/webp" => "webp",
            "image/avif" => "avif",
            "image/bmp" => "bmp",
            _ => "jpg",
        };
        format!("{path_name}.{ext}")
    }
}

