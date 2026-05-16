use crate::error::AppError;
use crate::models::AppState;
use actix_web::{
    body::{BoxBody, MessageBody},
    dev::{ServiceRequest, ServiceResponse},
    middleware::Next,
    web, Error, ResponseError,
};
use std::sync::Arc;
use subtle::ConstantTimeEq;

const PUBLIC_PATHS: &[&str] = &["/health", "/livez", "/readyz", "/metrics"];

pub async fn require_api_key(
    req: ServiceRequest,
    next: Next<impl MessageBody + 'static>,
) -> Result<ServiceResponse<BoxBody>, Error> {
    let path = req.path();
    if PUBLIC_PATHS.contains(&path) {
        return next.call(req).await.map(|r| r.map_into_boxed_body());
    }

    let state = req
        .app_data::<web::Data<Arc<AppState>>>()
        .expect("AppState is registered for every request");

    if !state.config.auth_required() {
        return next.call(req).await.map(|r| r.map_into_boxed_body());
    }

    let provided = req
        .headers()
        .get("X-API-Key")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let authorized = provided.as_deref().is_some_and(|key| {
        state
            .config
            .api_keys
            .iter()
            .any(|k| eq_constant_time(key.as_bytes(), k.as_bytes()))
    });

    if !authorized {
        let (http_req, _) = req.into_parts();
        let response = AppError::Unauthorized.error_response();
        return Ok(ServiceResponse::new(http_req, response).map_into_boxed_body());
    }

    next.call(req).await.map(|r| r.map_into_boxed_body())
}

fn eq_constant_time(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        // Still do a comparison of equal-length placeholders to avoid timing leaks
        // on the length difference itself.
        let _ = a.ct_eq(&vec![0u8; a.len()][..]);
        return false;
    }
    a.ct_eq(b).into()
}
