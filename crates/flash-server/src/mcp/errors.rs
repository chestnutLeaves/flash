use rmcp::ErrorData as McpError;

use crate::service::ServiceError;

/// What a refused deletion confirmation means and what to do next. A bare
/// code left the model guessing whether its own call had deleted anything.
/// In every case this call deleted nothing.
fn confirmation_hint(code: &str) -> Option<&'static str> {
    Some(match code {
        "confirmation_target_changed" => {
            "the cards, the deck or the subdecks under it changed (a subdeck added, moved or removed counts), or were already deleted (perhaps from another chat or the app), after the preview. This call deleted nothing. Read the current state; if something still needs deleting, call again without confirm_token and ask the user once more."
        }
        "confirmation_expired" => {
            "the ten minutes passed. This call deleted nothing. Call again without confirm_token for a fresh preview and ask the user once more."
        }
        "confirmation_target_mismatch" => {
            "this token was issued for a different deck or set of card IDs. This call deleted nothing. Repeat exactly the arguments of the call that returned the token, or start over without confirm_token."
        }
        "confirmation_invalid" => {
            "this token is unknown to this account and connector. This call deleted nothing. Call again without confirm_token for a fresh preview."
        }
        "too_many_pending_confirmations" => {
            "too many deletions are waiting for an answer. Finish or let lapse the ones already asked (ten minutes) before staging more."
        }
        _ => return None,
    })
}

/// The code an error message leads with (`source_changed: read the
/// cards again`), or that is the whole message
/// (`confirmation_target_changed`): lowercase words joined by `_`.
fn leading_code(msg: &str) -> Option<&str> {
    let code = msg.split_once(':').map_or(msg, |(code, _)| code);
    ((3..=40).contains(&code.len()) && code.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'))
        .then_some(code)
}

/// Every refusal names a code in its `data`: the message's own leading
/// code, else its kind (`invalid`, `not_found`, `over_cap`,
/// `internal`). The code is what the counts keep; the message, which
/// can carry a deck's name, never leaves the response.
pub(super) fn to_mcp_err(err: ServiceError) -> McpError {
    let coded = |code: &str| Some(serde_json::json!({ "code": code }));
    match err {
        ServiceError::Invalid(msg) => {
            let code = leading_code(&msg).unwrap_or("invalid").to_string();
            McpError::invalid_params(msg.into_string(), coded(&code))
        }
        ServiceError::Malformed(msg) => {
            let code = leading_code(&msg).unwrap_or("invalid").to_string();
            McpError::invalid_params(msg, coded(&code))
        }
        ServiceError::Refused { code, message } => {
            McpError::invalid_params(message.into_string(), coded(code))
        }
        ServiceError::Store(flash_store::StoreError::Invalid(msg)) => {
            let code = leading_code(&msg).unwrap_or("invalid").to_string();
            tracing::warn!(error_code=%code, "MCP operation rejected");
            let msg = match confirmation_hint(&code) {
                Some(hint) => format!("{code}: {hint}"),
                None => msg,
            };
            McpError::invalid_params(msg, coded(&code))
        }
        ServiceError::Store(flash_store::StoreError::NotFound(what)) => {
            McpError::invalid_params(format!("not found: {what}"), coded("not_found"))
        }
        ServiceError::OverCap { .. } => {
            McpError::invalid_params(err.to_string(), coded("over_cap"))
        }
        other => {
            tracing::error!("mcp service error: {other:?}");
            McpError::internal_error("internal error", coded("internal"))
        }
    }
}

/// A call that worked, for the counts: the tool's name, with what it did
/// where a success can mean two things. A delete is `:preview` while it
/// only staged a confirmation and `:confirmed` once it deleted; a panel
/// grade that wrote no review (the card had left the queue) is
/// `:not_recorded`, so it is not counted as studying.
pub(super) fn tool_success_detail(
    tool: &str,
    result: &Result<rmcp::model::CallToolResponse, McpError>,
) -> String {
    let Ok(rmcp::model::CallToolResponse::Complete(r)) = result else {
        return tool.to_string();
    };
    let field = |name: &str| {
        r.structured_content
            .as_ref()
            .and_then(|c| c.get(name))
            .and_then(|v| v.as_bool())
    };
    let outcome = match tool {
        "delete_cards" | "delete_deck" if field("pending") == Some(true) => Some("preview"),
        "delete_cards" | "delete_deck" if field("deleted") == Some(true) => Some("confirmed"),
        "panel_grade" if field("recorded") == Some(false) => Some("not_recorded"),
        _ => None,
    };
    match outcome {
        Some(outcome) => format!("{tool}:{outcome}"),
        None => tool.to_string(),
    }
}

/// Why a tool call failed, as a fixed code for the counts, or None when
/// it worked. Arguments the tool could not read come back as an error
/// result rather than an error (`bad_arguments`); an error names its
/// code (`to_mcp_err`), else its JSON-RPC kind.
pub(super) fn tool_failure_reason(
    result: &Result<rmcp::model::CallToolResponse, McpError>,
) -> Option<String> {
    match result {
        Ok(rmcp::model::CallToolResponse::Complete(r)) if r.is_error == Some(true) => {
            Some("bad_arguments".into())
        }
        Ok(_) => None,
        Err(e) => Some(
            e.data
                .as_ref()
                .and_then(|d| d.get("code"))
                .and_then(|c| c.as_str())
                .filter(|c| leading_code(&format!("{c}:")).is_some())
                .map(str::to_string)
                .unwrap_or_else(|| match e.code {
                    rmcp::model::ErrorCode::INVALID_PARAMS => "invalid".into(),
                    rmcp::model::ErrorCode::INTERNAL_ERROR => "internal".into(),
                    _ => "error".into(),
                }),
        ),
    }
}

#[cfg(test)]
mod count_tests {
    use super::*;
    use rmcp::model::{CallToolResponse, CallToolResult};

    fn complete(value: serde_json::Value) -> Result<CallToolResponse, McpError> {
        Ok(CallToolResponse::Complete(CallToolResult::structured(
            value,
        )))
    }

    /// A success that can mean two things says which in the counts.
    #[test]
    fn a_success_says_what_it_did_where_it_can_mean_two_things() {
        let staged = complete(serde_json::json!({"pending": true, "deleted": false}));
        let done = complete(serde_json::json!({"pending": false, "deleted": true}));
        let missed = complete(serde_json::json!({"recorded": false}));
        let graded = complete(serde_json::json!({"recorded": true}));
        assert_eq!(
            tool_success_detail("delete_cards", &staged),
            "delete_cards:preview"
        );
        assert_eq!(
            tool_success_detail("delete_deck", &done),
            "delete_deck:confirmed"
        );
        assert_eq!(
            tool_success_detail("panel_grade", &missed),
            "panel_grade:not_recorded"
        );
        assert_eq!(tool_success_detail("panel_grade", &graded), "panel_grade");
        assert_eq!(tool_success_detail("list_decks", &staged), "list_decks");
    }

    /// A failure's code is its leading code, a bare code, or its kind;
    /// never the text of the message.
    #[test]
    fn a_failure_is_counted_by_its_code_never_its_text() {
        assert_eq!(
            leading_code("source_changed: read the cards again"),
            Some("source_changed")
        );
        assert_eq!(
            leading_code("confirmation_target_changed"),
            Some("confirmation_target_changed")
        );
        assert_eq!(leading_code("no deck named 'Bio: Cells'"), None);
        let named = Err(to_mcp_err(ServiceError::Malformed(
            "no_deck: no deck named 'Pharmacology'".into(),
        )));
        assert_eq!(tool_failure_reason(&named).as_deref(), Some("no_deck"));
        let plain = Err(to_mcp_err(ServiceError::Malformed(
            "rating must be 1-4".into(),
        )));
        assert_eq!(tool_failure_reason(&plain).as_deref(), Some("invalid"));
        let broken = Err(to_mcp_err(ServiceError::Internal("disk on fire".into())));
        assert_eq!(tool_failure_reason(&broken).as_deref(), Some("internal"));
    }
}
