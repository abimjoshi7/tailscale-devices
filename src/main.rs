mod auth;
mod commands;
mod config;
mod error;
mod metrics;
mod models;
mod monitor;
mod proxy;
mod request_id;
mod routes;
mod tailscale;
#[cfg(test)]
mod tests;

use actix_web::{middleware::from_fn, web, App, HttpServer};
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenvy::dotenv().ok();
    init_tracing();

    let config = match config::Config::from_env() {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(error = %e, "invalid configuration; refusing to start");
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, e.to_string()));
        }
    };

    if !config.auth_required() {
        tracing::warn!(
            "API_KEY is not set — all endpoints except /health /livez /readyz /metrics \
             will accept unauthenticated requests. Set API_KEY before exposing this service."
        );
    }
    if config.allowed_proxy_ports.is_empty() {
        tracing::info!("ALLOWED_PROXY_PORTS not set — /proxy/* will reject all requests");
    }

    tracing::info!(
        discovery_mode = ?config.discovery_mode,
        auth_required = config.auth_required(),
        allowed_commands = config.allowed_commands.len(),
        "starting tailscale-devices"
    );

    metrics::init();

    let bind_addr = config.get_bind_address().await.map_err(io_err)?;
    let state = Arc::new(models::AppState::new(config.clone()));

    match tailscale::fetch_devices(&state.config, &state.http_client).await {
        Ok(devices) => {
            tracing::info!(count = devices.len(), "initial device fetch ok");
            metrics::get().devices_total.set(devices.len() as f64);
            *state.devices.write().await = devices;
            *state.last_poll.write().await = Some(chrono::Utc::now());
        }
        Err(e) => tracing::warn!(error = %e, "initial device fetch failed; monitor will retry"),
    }

    let cancel = CancellationToken::new();
    let monitor_handle = {
        let s = state.clone();
        let c = cancel.clone();
        tokio::spawn(async move { monitor::run(s, c).await })
    };

    let app_state = web::Data::new(state.clone());
    let shutdown_timeout = config.shutdown_timeout_secs;

    tracing::info!(addr = %bind_addr, "starting http server");

    let server = HttpServer::new(move || {
        App::new()
            .app_data(app_state.clone())
            .wrap(from_fn(metrics::track_request))
            .wrap(from_fn(request_id::inject_request_id))
            .wrap(from_fn(auth::require_api_key))
            .configure(routes::configure)
    })
    .shutdown_timeout(shutdown_timeout)
    .bind(&bind_addr)?
    .run();

    let server_handle = server.handle();
    let signal_cancel = cancel.clone();
    tokio::spawn(async move {
        wait_for_shutdown_signal().await;
        tracing::info!("shutdown signal received; stopping server and monitor");
        signal_cancel.cancel();
        server_handle.stop(true).await;
    });

    let server_result = server.await;

    cancel.cancel();
    match tokio::time::timeout(Duration::from_secs(shutdown_timeout), monitor_handle).await {
        Ok(Ok(())) => tracing::info!("monitor stopped cleanly"),
        Ok(Err(e)) => tracing::warn!(error = %e, "monitor task panicked"),
        Err(_) => tracing::warn!("monitor did not stop within grace period"),
    }

    server_result
}

fn init_tracing() {
    use tracing_subscriber::{fmt, EnvFilter};

    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    let log_format = std::env::var("LOG_FORMAT")
        .unwrap_or_default()
        .to_lowercase();

    if log_format == "json" {
        fmt().json().with_env_filter(filter).init();
    } else {
        fmt().with_env_filter(filter).init();
    }
}

async fn wait_for_shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut sigterm = match signal(SignalKind::terminate()) {
            Ok(s) => s,
            Err(e) => {
                tracing::error!(error = %e, "failed to install SIGTERM handler");
                let _ = tokio::signal::ctrl_c().await;
                return;
            }
        };
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = sigterm.recv() => {}
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

fn io_err(e: anyhow::Error) -> std::io::Error {
    std::io::Error::other(e.to_string())
}
