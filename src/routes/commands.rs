use crate::error::AppError;
use crate::models::{AppState, CommandRequest};
use actix_web::{web, HttpResponse};
use std::sync::Arc;

pub async fn run_command(
    state: web::Data<Arc<AppState>>,
    id: web::Path<String>,
    body: web::Json<CommandRequest>,
) -> Result<HttpResponse, AppError> {
    let id = id.into_inner();
    let device = {
        let devices = state.devices.read().await;
        devices
            .iter()
            .find(|d| d.id == id)
            .cloned()
            .ok_or(AppError::NotFound("device"))?
    };

    let response = crate::commands::run_ssh(&device, &state.config, &body).await?;

    let body = serde_json::to_string_pretty(&response)
        .map_err(|e| AppError::Internal(e.into()))?;
    Ok(HttpResponse::Ok()
        .content_type("application/json")
        .body(body))
}
