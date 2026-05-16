use actix_web::{body::MessageBody, dev::{ServiceRequest, ServiceResponse}, middleware::Next, Error, HttpResponse};
use prometheus::{
    register_counter_vec, register_gauge, register_histogram_vec,
    CounterVec, Encoder, Gauge, HistogramVec, TextEncoder,
};
use std::sync::OnceLock;
use std::time::Instant;

pub struct Metrics {
    pub http_requests_total: CounterVec,
    pub http_request_duration_seconds: HistogramVec,
    pub devices_total: Gauge,
    pub devices_reachable: Gauge,
    pub monitor_polls_total: CounterVec,
}

static METRICS: OnceLock<Metrics> = OnceLock::new();

pub fn init() -> &'static Metrics {
    METRICS.get_or_init(|| Metrics {
        http_requests_total: register_counter_vec!(
            "tailscale_http_requests_total",
            "Total HTTP requests",
            &["method", "status"]
        )
        .expect("metric registration failed"),

        http_request_duration_seconds: register_histogram_vec!(
            "tailscale_http_request_duration_seconds",
            "HTTP request latency",
            &["method"],
            vec![0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0]
        )
        .expect("metric registration failed"),

        devices_total: register_gauge!(
            "tailscale_devices_total",
            "Number of discovered Tailscale devices"
        )
        .expect("metric registration failed"),

        devices_reachable: register_gauge!(
            "tailscale_devices_reachable",
            "Number of currently reachable Tailscale devices"
        )
        .expect("metric registration failed"),

        monitor_polls_total: register_counter_vec!(
            "tailscale_monitor_polls_total",
            "Total monitor poll cycles",
            &["result"]
        )
        .expect("metric registration failed"),
    })
}

pub fn get() -> &'static Metrics {
    METRICS.get().expect("metrics not initialized")
}

pub async fn metrics_handler() -> HttpResponse {
    let encoder = TextEncoder::new();
    let families = prometheus::gather();
    let mut buf = Vec::new();
    encoder
        .encode(&families, &mut buf)
        .unwrap_or_else(|e| tracing::error!(error = %e, "metrics encoding failed"));

    HttpResponse::Ok()
        .content_type(encoder.format_type())
        .body(buf)
}

pub async fn track_request(
    req: ServiceRequest,
    next: Next<impl MessageBody + 'static>,
) -> Result<ServiceResponse<impl MessageBody>, Error> {
    let method = req.method().to_string();
    let start = Instant::now();

    let res = next.call(req).await?;

    let status = res.status().as_u16().to_string();
    let elapsed = start.elapsed().as_secs_f64();
    let m = get();
    m.http_requests_total
        .with_label_values(&[&method, &status])
        .inc();
    m.http_request_duration_seconds
        .with_label_values(&[&method])
        .observe(elapsed);

    Ok(res)
}
