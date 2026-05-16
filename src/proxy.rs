use crate::error::AppError;
use actix_web::{http::Method, web, HttpRequest, HttpResponse};

pub async fn forward(
    device_ip: &str,
    port: u16,
    path: &str,
    req: &HttpRequest,
    body: web::Bytes,
    client: &reqwest::Client,
) -> Result<HttpResponse, AppError> {
    if path.split('/').any(|seg| seg == "..") {
        return Err(AppError::BadRequest {
            code: "proxy_path_invalid",
            message: "path traversal segments are not allowed".into(),
        });
    }

    let target_url = format!("http://{}:{}/{}", device_ip, port, path);

    let mut req_builder = match *req.method() {
        Method::GET => client.get(&target_url),
        Method::POST => client.post(&target_url),
        Method::PUT => client.put(&target_url),
        Method::DELETE => client.delete(&target_url),
        Method::PATCH => client.patch(&target_url),
        Method::HEAD => client.head(&target_url),
        _ => {
            return Err(AppError::BadRequest {
                code: "proxy_method_unsupported",
                message: format!("method {} is not supported by the proxy", req.method()),
            });
        }
    };

    for (name, value) in req.headers() {
        if !is_hop_by_hop(name.as_str()) {
            if let Ok(val) = value.to_str() {
                req_builder = req_builder.header(name.as_str(), val);
            }
        }
    }

    req_builder = req_builder.header(
        "X-Forwarded-For",
        req.connection_info().peer_addr().unwrap_or(""),
    );
    req_builder = req_builder.header(
        "X-Forwarded-Host",
        req.headers()
            .get("Host")
            .and_then(|h| h.to_str().ok())
            .unwrap_or(""),
    );

    if !body.is_empty() {
        req_builder = req_builder.body(body);
    }

    let response = req_builder
        .send()
        .await
        .map_err(|e| AppError::Upstream(e.into()))?;

    let status = actix_web::http::StatusCode::from_u16(response.status().as_u16())
        .map_err(|e| AppError::Upstream(anyhow::anyhow!("upstream returned invalid status: {}", e)))?;

    let mut res_builder = HttpResponse::build(status);
    for (name, value) in response.headers() {
        if !is_hop_by_hop(name.as_str()) {
            if let Ok(val) = value.to_str() {
                res_builder.insert_header((name.as_str(), val));
            }
        }
    }

    let body = response
        .bytes()
        .await
        .map_err(|e| AppError::Upstream(e.into()))?;

    Ok(res_builder.body(body))
}

fn is_hop_by_hop(name: &str) -> bool {
    matches!(
        name.to_lowercase().as_str(),
        "connection"
            | "transfer-encoding"
            | "host"
            | "upgrade"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailers"
    )
}
