use crate::i18n::filters;

use askama::Template;
use axum::extract::State;
use axum::response::Response;

use super::{respond, Shell};
use crate::auth::{AdminUser, Theme};
use crate::service::now_ms;
use crate::state::AppState;

// ---- users (admin) ----

struct UserRowView {
    display_name: String,
    email: String,
    role: String,
    joined: String,
    deck_count: u32,
    card_count: u32,
    review_count: u32,
    last_studied: String,
    last_login: String,
    mcp_client: String,
}

#[derive(Template)]
#[template(path = "users.html")]
struct UsersPage {
    shell: Shell,
    users: Vec<UserRowView>,
}

pub(super) async fn users_page(
    State(state): State<AppState>,
    AdminUser(admin): AdminUser,
    theme: Theme,
    viewer: crate::when::ViewerZone,
) -> Response {
    let shell = Shell::new(&state, theme, "users", true);
    respond(state.services.clone(), move |s| {
        let now = now_ms();
        // Dates in the zone of the device looking.
        let zone = viewer.or_saved(&s.store().get_settings(admin)?.timezone);
        let shell = shell.in_zone(&zone);
        let ago = |ts| crate::when::ago(now, ts, &zone).into_string();
        let mut users = Vec::new();
        for u in s.admin_users(now)? {
            users.push(UserRowView {
                display_name: u.display_name,
                email: u.email.unwrap_or_else(|| "—".to_string()),
                role: u.role,
                joined: crate::when::date(u.created_at, &zone),
                deck_count: u.deck_count,
                card_count: u.card_count,
                review_count: u.review_count,
                last_studied: match (u.last_review_at, u.last_review_source) {
                    (Some(ts), Some(src)) => format!("{} · {src}", ago(ts)),
                    (Some(ts), None) => ago(ts),
                    _ => "never".to_string(),
                },
                last_login: u
                    .last_login_at
                    .map(ago)
                    .unwrap_or_else(|| "never".to_string()),
                mcp_client: if u.mcp_clients.is_empty() {
                    "—".to_string()
                } else {
                    u.mcp_clients.join(", ")
                },
            });
        }
        Ok(UsersPage { shell, users })
    })
    .await
}
