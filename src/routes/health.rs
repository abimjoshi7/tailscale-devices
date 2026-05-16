use crate::error::AppError;
use crate::models::{AppState, HealthResponse};
use actix_web::{web, HttpResponse};
use std::sync::Arc;

pub async fn get_health(state: web::Data<Arc<AppState>>) -> Result<HttpResponse, AppError> {
    let devices = state.devices.read().await;
    let health = state.health.read().await;
    let last_poll = state.last_poll.read().await;

    let devices_total = devices.len();
    let devices_reachable = health.values().filter(|h| h.reachable).count();

    let response = HealthResponse {
        status: "ok".to_string(),
        devices_total,
        devices_reachable,
        last_poll: *last_poll,
    };

    json_response(&response)
}

pub async fn livez() -> HttpResponse {
    HttpResponse::Ok()
        .content_type("application/json")
        .body(r#"{"status":"alive"}"#)
}

pub async fn readyz(state: web::Data<Arc<AppState>>) -> HttpResponse {
    let last_poll = *state.last_poll.read().await;
    if last_poll.is_some() {
        HttpResponse::Ok()
            .content_type("application/json")
            .body(r#"{"status":"ready"}"#)
    } else {
        HttpResponse::ServiceUnavailable()
            .content_type("application/json")
            .body(r#"{"status":"not_ready","reason":"no successful device poll yet"}"#)
    }
}

fn json_response<T: serde::Serialize>(value: &T) -> Result<HttpResponse, AppError> {
    let body = serde_json::to_string_pretty(value)
        .map_err(|e| AppError::Internal(e.into()))?;
    Ok(HttpResponse::Ok()
        .content_type("application/json")
        .body(body))
}
