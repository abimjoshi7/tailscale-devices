use crate::config::{Config, DiscoveryMode};
use crate::models::TailscaleDevice;
use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
struct LocalApiStatus {
    #[serde(rename = "Self")]
    self_device: Option<LocalApiPeer>,
    #[serde(rename = "Peer")]
    peer: HashMap<String, LocalApiPeer>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct LocalApiPeer {
    public_key: String,
    host_name: String,
    dns_name: String,
    #[serde(rename = "TailscaleIPs")]
    tail_scale_i_ps: Vec<String>,
    #[serde(rename = "OS")]
    os: String,
    last_seen: Option<DateTime<Utc>>,
    online: bool,
}

#[derive(Debug, Deserialize)]
struct AdminApiResponse {
    devices: Vec<AdminApiDevice>,
}

#[derive(Debug, Deserialize)]
struct AdminApiDevice {
    id: String,
    name: String,
    hostname: String,
    addresses: Vec<String>,
    os: String,
}

#[derive(Debug, Deserialize)]
struct CliStatus {
    #[serde(rename = "Self")]
    self_device: CliPeer,
    #[serde(rename = "Peer")]
    peers: HashMap<String, CliPeer>,
}

#[derive(Debug, Deserialize)]
struct CliPeer {
    #[serde(rename = "PublicKey")]
    public_key: String,
    #[serde(rename = "HostName")]
    hostname: String,
    #[serde(rename = "DNSName")]
    dns_name: String,
    #[serde(rename = "TailscaleIPs")]
    tail_scale_i_ps: Vec<String>,
    #[serde(rename = "OS")]
    os: String,
    #[serde(rename = "Online")]
    online: bool,
    #[serde(rename = "LastHandshake")]
    last_handshake: Option<DateTime<Utc>>,
}

pub async fn fetch_devices(config: &Config, client: &reqwest::Client) -> Result<Vec<TailscaleDevice>> {
    match config.discovery_mode {
        DiscoveryMode::LocalApi => fetch_local_api(client).await,
        DiscoveryMode::AdminApi => fetch_admin_api(config, client).await,
        DiscoveryMode::Cli => fetch_cli().await,
    }
}

async fn fetch_local_api(client: &reqwest::Client) -> Result<Vec<TailscaleDevice>> {
    let response = client
        .get("http://127.0.0.1:41112/localapi/v0/status")
        .header("Sec-Tailscale", "okay")
        .send()
        .await?
        .json::<LocalApiStatus>()
        .await?;

    let mut devices = Vec::new();

    if let Some(s) = response.self_device {
        devices.push(peer_to_device(s.public_key.clone(), s)?);
    }

    for (_, peer) in response.peer {
        devices.push(peer_to_device(peer.public_key.clone(), peer)?);
    }

    Ok(devices)
}

fn peer_to_device(id: String, peer: LocalApiPeer) -> Result<TailscaleDevice> {
    let ip = peer
        .tail_scale_i_ps
        .first()
        .ok_or_else(|| anyhow!("peer {} has no tailscale ips", id))?
        .clone();

    Ok(TailscaleDevice {
        id,
        hostname: peer.host_name,
        dns_name: peer.dns_name,
        ip,
        os: peer.os,
        last_seen: peer.last_seen,
        is_online: peer.online,
    })
}

async fn fetch_admin_api(config: &Config, client: &reqwest::Client) -> Result<Vec<TailscaleDevice>> {
    let auth_key = config
        .tailscale_auth_key
        .as_ref()
        .ok_or_else(|| anyhow!("TAILSCALE_AUTH_KEY not set for admin_api mode"))?;

    let tailnet = config
        .tailnet
        .as_ref()
        .ok_or_else(|| anyhow!("TAILSCALE_TAILNET not set for admin_api mode"))?;

    let url = format!("https://api.tailscale.com/api/v2/tailnet/{}/devices", tailnet);

    let response = client
        .get(&url)
        .bearer_auth(auth_key)
        .send()
        .await?
        .json::<AdminApiResponse>()
        .await?;

    let devices = response
        .devices
        .into_iter()
        .map(|d| {
            let ip = d
                .addresses
                .iter()
                .find(|addr| addr.starts_with("100."))
                .cloned()
                .unwrap_or_default();

            let is_online = !ip.is_empty();

            TailscaleDevice {
                id: d.id,
                hostname: d.hostname,
                dns_name: d.name,
                ip,
                os: d.os,
                last_seen: None,
                is_online,
            }
        })
        .collect();

    Ok(devices)
}

async fn fetch_cli() -> Result<Vec<TailscaleDevice>> {
    let output = tokio::process::Command::new("tailscale")
        .args(["status", "--json"])
        .output()
        .await?;

    if !output.status.success() {
        return Err(anyhow!(
            "tailscale status failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let status: CliStatus = serde_json::from_slice(&output.stdout)?;
    let mut devices = Vec::new();

    devices.push(cli_peer_to_device(status.self_device.public_key.clone(), status.self_device)?);

    for (_, peer) in status.peers {
        devices.push(cli_peer_to_device(peer.public_key.clone(), peer)?);
    }

    Ok(devices)
}

fn cli_peer_to_device(id: String, peer: CliPeer) -> Result<TailscaleDevice> {
    let ip = peer
        .tail_scale_i_ps
        .first()
        .ok_or_else(|| anyhow!("peer {} has no tailscale ips", id))?
        .clone();

    Ok(TailscaleDevice {
        id,
        hostname: peer.hostname,
        dns_name: peer.dns_name,
        ip,
        os: peer.os,
        last_seen: peer.last_handshake,
        is_online: peer.online,
    })
}
