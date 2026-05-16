use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize, Serializer};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::config::Config;

fn serialize_datetime<S>(dt: &Option<DateTime<Utc>>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match dt {
        Some(d) => {
            let zero_time = "0001-01-01T00:00:00Z".parse::<DateTime<Utc>>().unwrap();
            if d == &zero_time {
                serializer.serialize_none()
            } else {
                let formatted = d.format("%d, %b %H:%M").to_string();
                serializer.serialize_str(&formatted)
            }
        }
        None => serializer.serialize_none(),
    }
}

fn serialize_datetime_required<S>(dt: &DateTime<Utc>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let zero_time = "0001-01-01T00:00:00Z".parse::<DateTime<Utc>>().unwrap();
    if dt == &zero_time {
        serializer.serialize_none()
    } else {
        let formatted = dt.format("%d, %b %H:%M").to_string();
        serializer.serialize_str(&formatted)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TailscaleDevice {
    pub id: String,
    pub hostname: String,
    pub dns_name: String,
    pub ip: String,
    pub os: String,
    #[serde(serialize_with = "serialize_datetime")]
    pub last_seen: Option<DateTime<Utc>>,
    pub is_online: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceHealth {
    pub port: u16,
    pub status_code: Option<u16>,
    pub reachable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceHealth {
    pub device_id: String,
    pub reachable: bool,
    pub latency_ms: Option<u64>,
    #[serde(serialize_with = "serialize_datetime_required")]
    pub last_checked: DateTime<Utc>,
    pub http_services: HashMap<u16, ServiceHealth>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeviceWithHealth {
    pub device: TailscaleDevice,
    pub health: Option<DeviceHealth>,
}

#[derive(Debug, Deserialize)]
pub struct CommandRequest {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub timeout_secs: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct CommandResponse {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub duration_ms: u64,
}

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub devices_total: usize,
    pub devices_reachable: usize,
    #[serde(serialize_with = "serialize_datetime")]
    pub last_poll: Option<DateTime<Utc>>,
}

pub struct AppState {
    pub devices: Arc<RwLock<Vec<TailscaleDevice>>>,
    pub health: Arc<RwLock<HashMap<String, DeviceHealth>>>,
    pub config: Arc<Config>,
    pub http_client: reqwest::Client,
    pub last_poll: Arc<RwLock<Option<DateTime<Utc>>>>,
}

impl AppState {
    pub fn new(config: Config) -> Self {
        AppState {
            devices: Arc::new(RwLock::new(Vec::new())),
            health: Arc::new(RwLock::new(HashMap::new())),
            config: Arc::new(config),
            http_client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .unwrap(),
            last_poll: Arc::new(RwLock::new(None)),
        }
    }
}
