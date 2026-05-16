use crate::{auth, config::Config, models::AppState, request_id, routes};
use actix_web::{
    middleware::from_fn,
    test::{self, TestRequest},
    web, App,
};
use std::sync::Arc;

fn build_state() -> Arc<AppState> {
    Arc::new(AppState::new(Config::for_testing()))
}

macro_rules! app {
    ($state:expr) => {{
        let data = web::Data::new($state);
        test::init_service(
            App::new()
                .app_data(data)
                .wrap(from_fn(request_id::inject_request_id))
                .wrap(from_fn(auth::require_api_key))
                .configure(routes::configure),
        )
        .await
    }};
}

const KEY: &str = "test-api-key-32-characters-long!!";

// ── Public endpoints ──────────────────────────────────────────────────────────

#[actix_web::test]
async fn health_is_public() {
    let app = app!(build_state());
    let resp = test::call_service(&app, TestRequest::get().uri("/health").to_request()).await;
    assert_eq!(resp.status(), 200);
}

#[actix_web::test]
async fn livez_is_public() {
    let app = app!(build_state());
    let resp = test::call_service(&app, TestRequest::get().uri("/livez").to_request()).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["status"], "alive");
}

#[actix_web::test]
async fn readyz_returns_503_before_first_poll() {
    let app = app!(build_state());
    let resp = test::call_service(&app, TestRequest::get().uri("/readyz").to_request()).await;
    // last_poll is None on a fresh AppState → not ready
    assert_eq!(resp.status(), 503);
}

// ── Authentication ────────────────────────────────────────────────────────────

#[actix_web::test]
async fn devices_without_key_is_401() {
    let app = app!(build_state());
    let resp = test::call_service(&app, TestRequest::get().uri("/devices").to_request()).await;
    assert_eq!(resp.status(), 401);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["error"], "unauthorized");
}

#[actix_web::test]
async fn devices_with_wrong_key_is_401() {
    let app = app!(build_state());
    let resp = test::call_service(
        &app,
        TestRequest::get()
            .uri("/devices")
            .insert_header(("X-API-Key", "wrong-key"))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 401);
}

#[actix_web::test]
async fn devices_with_valid_key_is_200() {
    let app = app!(build_state());
    let resp = test::call_service(
        &app,
        TestRequest::get()
            .uri("/devices")
            .insert_header(("X-API-Key", KEY))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert!(body.is_array());
}

// ── Request ID ────────────────────────────────────────────────────────────────

#[actix_web::test]
async fn response_always_has_request_id() {
    let app = app!(build_state());
    let resp = test::call_service(&app, TestRequest::get().uri("/health").to_request()).await;
    assert!(resp.headers().get("x-request-id").is_some());
}

#[actix_web::test]
async fn client_request_id_is_echoed() {
    let app = app!(build_state());
    let resp = test::call_service(
        &app,
        TestRequest::get()
            .uri("/health")
            .insert_header(("X-Request-Id", "my-trace-id-abc"))
            .to_request(),
    )
    .await;
    let id = resp
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert_eq!(id, "my-trace-id-abc");
}

// ── 404 ───────────────────────────────────────────────────────────────────────

#[actix_web::test]
async fn unknown_device_health_is_404() {
    let app = app!(build_state());
    let resp = test::call_service(
        &app,
        TestRequest::get()
            .uri("/devices/nonexistent-id/health")
            .insert_header(("X-API-Key", KEY))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 404);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["error"], "not_found");
}

// ── Command validation ────────────────────────────────────────────────────────

#[actix_web::test]
async fn command_not_in_allowlist_is_403() {
    let state = build_state();
    {
        let mut devices = state.devices.write().await;
        devices.push(crate::models::TailscaleDevice {
            id: "test-device-0".into(),
            hostname: "test-host0".into(),
            dns_name: "test-host0.ts.net".into(),
            ip: "100.0.0.0".into(),
            os: "linux".into(),
            last_seen: None,
            is_online: true,
        });
    }
    let app = app!(state);
    let resp = test::call_service(
        &app,
        TestRequest::post()
            .uri("/devices/test-device-0/command")
            .insert_header(("X-API-Key", KEY))
            .insert_header(("Content-Type", "application/json"))
            .set_payload(r#"{"command":"rm","args":[]}"#)
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 403);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["error"], "command_not_allowed");
}

#[actix_web::test]
async fn command_with_slash_is_400() {
    let state = build_state();
    // Seed a device so we get past the device-lookup step
    {
        let mut devices = state.devices.write().await;
        devices.push(crate::models::TailscaleDevice {
            id: "test-device-1".into(),
            hostname: "test-host".into(),
            dns_name: "test-host.ts.net".into(),
            ip: "100.0.0.1".into(),
            os: "linux".into(),
            last_seen: None,
            is_online: true,
        });
    }
    let app = app!(state);
    let resp = test::call_service(
        &app,
        TestRequest::post()
            .uri("/devices/test-device-1/command")
            .insert_header(("X-API-Key", KEY))
            .insert_header(("Content-Type", "application/json"))
            .set_payload(r#"{"command":"/bin/sh","args":[]}"#)
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 400);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["error"], "command_invalid");
}

#[actix_web::test]
async fn command_with_shell_injection_arg_is_400() {
    let state = build_state();
    {
        let mut devices = state.devices.write().await;
        devices.push(crate::models::TailscaleDevice {
            id: "test-device-2".into(),
            hostname: "test-host2".into(),
            dns_name: "test-host2.ts.net".into(),
            ip: "100.0.0.2".into(),
            os: "linux".into(),
            last_seen: None,
            is_online: true,
        });
    }
    let app = app!(state);
    let resp = test::call_service(
        &app,
        TestRequest::post()
            .uri("/devices/test-device-2/command")
            .insert_header(("X-API-Key", KEY))
            .insert_header(("Content-Type", "application/json"))
            .set_payload(r#"{"command":"uptime","args":["; cat /etc/passwd"]}"#)
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 400);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["error"], "arg_invalid");
}

// ── Proxy ─────────────────────────────────────────────────────────────────────

#[actix_web::test]
async fn proxy_disabled_when_no_ports_allowed() {
    let mut cfg = Config::for_testing();
    cfg.allowed_proxy_ports = vec![]; // disable proxy
    let state = Arc::new(AppState::new(cfg));
    let app = app!(state);
    let resp = test::call_service(
        &app,
        TestRequest::get()
            .uri("/proxy/somedevice/80/path")
            .insert_header(("X-API-Key", KEY))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 403);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["error"], "proxy_disabled");
}

#[actix_web::test]
async fn proxy_rejects_port_not_in_allowlist() {
    let app = app!(build_state()); // allowed_proxy_ports = [8080]
    let resp = test::call_service(
        &app,
        TestRequest::get()
            .uri("/proxy/somedevice/9999/path")
            .insert_header(("X-API-Key", KEY))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 403);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["error"], "proxy_port_not_allowed");
}
