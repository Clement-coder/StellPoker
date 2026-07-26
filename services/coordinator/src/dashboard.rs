//! Local development dashboard for the Stellar Poker stack.
//!
//! Serves a single-page HTML view at `GET /` that polls `/api/health` every
//! few seconds and renders the live status of each local service: the
//! Soroban container, the three MPC nodes, the coordinator itself, and the
//! Soroban poker_table contract deployment.
//!
//! The page is a static asset embedded at compile time so the binary needs
//! no separate web-asset deployment. The dashboard talks only to the
//! coordinator's existing `/api/health` endpoint, so it works against any
//! coordinator instance without extra wiring.

use axum::response::Html;

const DASHBOARD_HTML: &str = include_str!("dashboard.html");

/// Handler for `GET /`. Returns the embedded dashboard page.
pub async fn dashboard_page() -> Html<&'static str> {
    Html(DASHBOARD_HTML)
}
