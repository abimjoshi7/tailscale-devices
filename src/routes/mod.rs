pub mod commands;
pub mod devices;
pub mod health;
pub mod proxy;

use actix_web::web;

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("")
            .route("/health", web::get().to(health::get_health))
            .route("/livez", web::get().to(health::livez))
            .route("/readyz", web::get().to(health::readyz))
            .route("/devices", web::get().to(devices::list_devices))
            .route("/devices/{id}/health", web::get().to(devices::device_health))
            .route("/devices/{id}/command", web::post().to(commands::run_command))
            .route("/proxy/{device}/{port}/{path:.*}", web::get().to(proxy::proxy_handler))
            .route("/proxy/{device}/{port}/{path:.*}", web::post().to(proxy::proxy_handler))
            .route("/proxy/{device}/{port}/{path:.*}", web::put().to(proxy::proxy_handler))
            .route("/proxy/{device}/{port}/{path:.*}", web::delete().to(proxy::proxy_handler))
            .route("/proxy/{device}/{port}/{path:.*}", web::patch().to(proxy::proxy_handler)),
    );
}
