use crate::error::AppError;
use crate::models::AppState;
use actix_web::{web, HttpRequest, HttpResponse};
use std::sync::Arc;

pub async fn proxy_handler(
    state: web::Data<Arc<AppState>>,
    path: web::Path<(String, u16, String)>,
    req: HttpRequest,
    body: web::Bytes,
) -> Result<HttpResponse, AppError> {
    let (device_name, port, path_str) = path.into_inner();

    if state.config.allowed_proxy_ports.is_empty() {
        return Err(AppError::Forbidden {
            code: "proxy_disabled",
            message: "proxy is disabled; configure ALLOWED_PROXY_PORTS to enable".into(),
        });
    }

    if !state.config.allowed_proxy_ports.contains(&port) {
        return Err(AppError::Forbidden {
            code: "proxy_port_not_allowed",
            message: format!("port {} is not in the proxy allowlist", port),
        });
    }

    if body.len() > state.config.max_proxy_body_bytes {
        return Err(AppError::BadRequest {
            code: "proxy_body_too_large",
            message: format!(
                "request body exceeds MAX_PROXY_BODY_BYTES ({} bytes)",
                state.config.max_proxy_body_bytes
            ),
        });
    }

    let ip = {
        let devices = state.devices.read().await;
        devices
            .iter()
            .find(|d| d.id == device_name || d.hostname == device_name || d.dns_name == device_name)
            .map(|d| d.ip.clone())
            .ok_or(AppError::NotFound("device"))?
    };

    crate::proxy::forward(&ip, port, &path_str, &req, body, &state.http_client).await
}
