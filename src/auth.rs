use crate::error::AppError;
use crate::models::AppState;
use actix_web::{
    body::MessageBody,
    dev::{ServiceRequest, ServiceResponse},
    middleware::Next,
    web, Error,
};
use std::sync::Arc;
use subtle::ConstantTimeEq;

const PUBLIC_PATHS: &[&str] = &["/health", "/livez", "/readyz"];

pub async fn require_api_key(
    req: ServiceRequest,
    next: Next<impl MessageBody + 'static>,
) -> Result<ServiceResponse<impl MessageBody>, Error> {
    let path = req.path();
    if PUBLIC_PATHS.iter().any(|p| *p == path) {
        return next.call(req).await;
    }

    let state = req
        .app_data::<web::Data<Arc<AppState>>>()
        .expect("AppState is registered for every request");

    if !state.config.auth_required() {
        return next.call(req).await;
    }

    let provided = req
        .headers()
        .get("X-API-Key")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let Some(provided) = provided else {
        return Err(AppError::Unauthorized.into());
    };

    let provided_bytes = provided.as_bytes();
    let matches = state
        .config
        .api_keys
        .iter()
        .any(|k| eq_constant_time(provided_bytes, k.as_bytes()));

    if !matches {
        return Err(AppError::Unauthorized.into());
    }

    next.call(req).await
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
