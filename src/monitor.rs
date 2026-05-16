use crate::metrics;
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
const MAX_FETCH_RETRIES: u32 = 3;
const RETRY_BASE_MS: u64 = 1_000;

pub async fn run(state: Arc<AppState>, cancel: CancellationToken) {
    let mut ticker = interval(Duration::from_secs(state.config.health_poll_secs));
    ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                tracing::info!("monitor: shutdown signal received, exiting");
                return;
            }
            _ = ticker.tick() => {}
        }

        match fetch_with_retry(&state, &cancel).await {
            Some(devices) => {
                let count = devices.len() as f64;
                *state.devices.write().await = devices;
                *state.last_poll.write().await = Some(Utc::now());
                metrics::get().devices_total.set(count);
                metrics::get().monitor_polls_total.with_label_values(&["ok"]).inc();
            }
            None => {
                metrics::get().monitor_polls_total.with_label_values(&["error"]).inc();
            }
        }

        let devices = state.devices.read().await.clone();
        let reachable = run_health_checks(devices, state.http_client.clone(), &cancel).await;

        let mut health_map = state.health.write().await;
        let mut reachable_count = 0u32;
        for (device_id, health) in reachable {
            if health.reachable {
                reachable_count += 1;
            }
            health_map.insert(device_id, health);
        }
        drop(health_map);
        metrics::get().devices_reachable.set(reachable_count as f64);
    }
}

async fn fetch_with_retry(
    state: &AppState,
    cancel: &CancellationToken,
) -> Option<Vec<crate::models::TailscaleDevice>> {
    for attempt in 0..MAX_FETCH_RETRIES {
        if cancel.is_cancelled() {
            return None;
        }

        match tailscale::fetch_devices(&state.config, &state.http_client).await {
            Ok(devices) => return Some(devices),
            Err(e) if attempt + 1 < MAX_FETCH_RETRIES => {
                let delay_ms = RETRY_BASE_MS * 2u64.pow(attempt);
                tracing::warn!(
                    error = %e,
                    attempt = attempt + 1,
                    max = MAX_FETCH_RETRIES,
                    retry_ms = delay_ms,
                    "device fetch failed, retrying"
                );
                tokio::select! {
                    biased;
                    _ = cancel.cancelled() => return None,
                    _ = tokio::time::sleep(Duration::from_millis(delay_ms)) => {}
                }
            }
            Err(e) => {
                tracing::error!(error = %e, "device fetch failed after all retries");
                return None;
            }
        }
    }
    None
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
    _client: reqwest::Client,
) -> Option<(String, DeviceHealth)> {
    let start = std::time::Instant::now();
    let reachable = is_device_reachable(&device.ip).await;

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

async fn is_device_reachable(ip: &str) -> bool {
    let addr = format!("{}:22", ip);
    tokio::time::timeout(REACHABILITY_TIMEOUT, tokio::net::TcpStream::connect(addr))
        .await
        .map(|r| r.is_ok())
        .unwrap_or(false)
}
