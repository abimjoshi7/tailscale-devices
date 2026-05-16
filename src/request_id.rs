use actix_web::{
    body::MessageBody,
    dev::{ServiceRequest, ServiceResponse},
    middleware::Next,
    Error, HttpMessage,
};
use uuid::Uuid;

pub const REQUEST_ID_HEADER: &str = "X-Request-Id";

#[derive(Clone)]
#[allow(dead_code)]
pub struct RequestId(pub String);

pub async fn inject_request_id(
    req: ServiceRequest,
    next: Next<impl MessageBody + 'static>,
) -> Result<ServiceResponse<impl MessageBody>, Error> {
    let id = req
        .headers()
        .get(REQUEST_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(String::from)
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    req.extensions_mut().insert(RequestId(id.clone()));

    let span = tracing::info_span!("request", request_id = %id);
    let _guard = span.enter();

    let mut res = next.call(req).await?;
    res.headers_mut().insert(
        actix_web::http::header::HeaderName::from_static("x-request-id"),
        actix_web::http::header::HeaderValue::from_str(&id)
            .unwrap_or_else(|_| actix_web::http::header::HeaderValue::from_static("invalid")),
    );

    Ok(res)
}
