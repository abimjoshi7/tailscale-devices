use crate::error::AppError;
use crate::models::{AppState, DeviceWithHealth};
use actix_web::{web, HttpResponse};
use std::sync::Arc;

pub async fn list_devices(state: web::Data<Arc<AppState>>) -> Result<HttpResponse, AppError> {
    let devices = state.devices.read().await;
    let health = state.health.read().await;

    let combined: Vec<DeviceWithHealth> = devices
        .iter()
        .map(|d| DeviceWithHealth {
            device: d.clone(),
            health: health.get(&d.id).cloned(),
        })
        .collect();

    json_response(&combined)
}

pub async fn device_health(
    state: web::Data<Arc<AppState>>,
    id: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let id = id.into_inner();
    let devices = state.devices.read().await;
    let health = state.health.read().await;

    let device = devices
        .iter()
        .find(|d| d.id == id)
        .ok_or(AppError::NotFound("device"))?;

    let response = DeviceWithHealth {
        device: device.clone(),
        health: health.get(&device.id).cloned(),
    };

    json_response(&response)
}

fn json_response<T: serde::Serialize>(value: &T) -> Result<HttpResponse, AppError> {
    let body = serde_json::to_string_pretty(value)
        .map_err(|e| AppError::Internal(e.into()))?;
    Ok(HttpResponse::Ok()
        .content_type("application/json")
        .body(body))
}
