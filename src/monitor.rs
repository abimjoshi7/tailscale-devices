use crate::models::{AppState, DeviceHealth, TailscaleDevice};
use crate::tailscale;
use chrono::Utc;
use futures::stream::{FuturesUnordered, StreamExt};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::{interval, MissedTickBehavior};
use tokio_util::sync::CancellationToken;

const MAX_PARALLEL_HEALTH_CHECKS: usize = 16;
const REACHABILITY_TIMEOUT: Duration = Duration::from_secs(3);

pub async fn run(state: Arc<AppState>, cancel: CancellationToken) {
    let mut ticker = interval(Duration::from_secs(state.config.health_poll_secs));
    ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                tracing::info!("monitor: shutdown signal received, exiting");
                return;
            }
            _ = ticker.tick() => {}
        }

        match tailscale::fetch_devices(&state.config, &state.http_client).await {
            Ok(devices) => {
                *state.devices.write().await = devices;
                *state.last_poll.write().await = Some(Utc::now());
            }
            Err(e) => {
                tracing::error!(error = %e, "device fetch failed");
            }
        }

        let devices = state.devices.read().await.clone();
        let results = run_health_checks(devices, state.http_client.clone(), &cancel).await;

        let mut health_map = state.health.write().await;
        for (device_id, health) in results {
            health_map.insert(device_id, health);
        }
    }
}

async fn run_health_checks(
    devices: Vec<TailscaleDevice>,
    client: reqwest::Client,
    cancel: &CancellationToken,
) -> Vec<(String, DeviceHealth)> {
    let mut in_flight = FuturesUnordered::new();
    let mut iter = devices.into_iter();
    let mut results = Vec::new();

    for _ in 0..MAX_PARALLEL_HEALTH_CHECKS {
        if let Some(d) = iter.next() {
            in_flight.push(check_device_health(d, client.clone()));
        }
    }

    while let Some(maybe_result) = tokio::select! {
        biased;
        _ = cancel.cancelled() => None,
        r = in_flight.next() => Some(r),
    } {
        match maybe_result {
            Some(Some(pair)) => results.push(pair),
            Some(None) => {}
            None => break,
        }
        if let Some(d) = iter.next() {
            in_flight.push(check_device_health(d, client.clone()));
        }
    }

    results
}

async fn check_device_health(
    device: TailscaleDevice,
    client: reqwest::Client,
) -> Option<(String, DeviceHealth)> {
    let start = std::time::Instant::now();
    let reachable = is_device_reachable(&device.ip, &client).await;

    let latency_ms = if reachable {
        Some(start.elapsed().as_millis() as u64)
    } else {
        None
    };

    let health = DeviceHealth {
        device_id: device.id.clone(),
        reachable,
        latency_ms,
        last_checked: Utc::now(),
        http_services: Default::default(),
    };

    Some((device.id, health))
}

async fn is_device_reachable(ip: &str, _client: &reqwest::Client) -> bool {
    let addr = format!("{}:22", ip);
    tokio::time::timeout(REACHABILITY_TIMEOUT, tokio::net::TcpStream::connect(addr))
        .await
        .map(|r| r.is_ok())
        .unwrap_or(false)
}
