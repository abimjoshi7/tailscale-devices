use crate::error::ConfigError;
use anyhow::Result;
use std::path::PathBuf;
use std::str::FromStr;

#[derive(Debug, Clone)]
pub enum DiscoveryMode {
    AdminApi,
    Cli,
}

impl FromStr for DiscoveryMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "admin_api" | "adminapi" | "admin-api" => Ok(DiscoveryMode::AdminApi),
            "cli" => Ok(DiscoveryMode::Cli),
            "local_api" | "localapi" | "local-api" => Err(
                "LocalApi mode is not supported (Tailscale uses a Unix socket, not TCP). \
                 Use DISCOVERY_MODE=Cli instead."
                    .into(),
            ),
            _ => Err(format!("unknown discovery mode '{}'. Valid: Cli, AdminApi", s)),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    pub port: u16,
    pub discovery_mode: DiscoveryMode,
    pub tailscale_auth_key: Option<String>,
    pub tailnet: Option<String>,
    pub health_poll_secs: u64,
    pub ssh_user: String,
    pub ssh_known_hosts: PathBuf,
    pub bind_tailscale: bool,
    pub api_keys: Vec<String>,
    pub allowed_commands: Vec<String>,
    pub allowed_proxy_ports: Vec<u16>,
    pub max_command_timeout_secs: u64,
    pub max_proxy_body_bytes: usize,
    pub shutdown_timeout_secs: u64,
}

impl Config {
    pub async fn get_bind_address(&self) -> Result<String> {
        if self.bind_tailscale {
            let output = tokio::process::Command::new("tailscale")
                .args(["ip", "-4"])
                .output()
                .await?;

            if !output.status.success() {
                return Err(anyhow::anyhow!(
                    "failed to get tailscale ip: {}",
                    String::from_utf8_lossy(&output.stderr)
                ));
            }

            let ip = String::from_utf8(output.stdout)?.trim().to_string();
            if ip.is_empty() {
                return Err(anyhow::anyhow!("tailscale ip -4 returned empty output"));
            }

            Ok(format!("{}:{}", ip, self.port))
        } else {
            Ok(format!("0.0.0.0:{}", self.port))
        }
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.health_poll_secs == 0 {
            return Err(ConfigError("HEALTH_POLL_SECS must be > 0".into()));
        }

        if self.max_command_timeout_secs == 0 {
            return Err(ConfigError("MAX_COMMAND_TIMEOUT_SECS must be > 0".into()));
        }

        if let DiscoveryMode::AdminApi = self.discovery_mode {
            if self.tailscale_auth_key.is_none() {
                return Err(ConfigError(
                    "DISCOVERY_MODE=admin_api requires TAILSCALE_AUTH_KEY".into(),
                ));
            }
            if self.tailnet.is_none() {
                return Err(ConfigError(
                    "DISCOVERY_MODE=admin_api requires TAILSCALE_TAILNET".into(),
                ));
            }
        }

        for key in &self.api_keys {
            if key.len() < 16 {
                return Err(ConfigError(
                    "API_KEYS entries must be at least 16 characters".into(),
                ));
            }
        }

        Ok(())
    }

    pub fn auth_required(&self) -> bool {
        !self.api_keys.is_empty()
    }

    pub fn from_env() -> Result<Self, ConfigError> {
        let port = parse_env("LISTEN_PORT", 3453)?;
        let discovery_mode = std::env::var("DISCOVERY_MODE")
            .ok()
            .map(|m| m.parse::<DiscoveryMode>())
            .transpose()
            .map_err(ConfigError)?
            .unwrap_or(DiscoveryMode::Cli);

        let tailscale_auth_key = non_empty_env("TAILSCALE_AUTH_KEY");
        let tailnet = non_empty_env("TAILSCALE_TAILNET");
        let health_poll_secs = parse_env("HEALTH_POLL_SECS", 30)?;
        let ssh_user = std::env::var("SSH_USER").unwrap_or_else(|_| "root".to_string());
        let bind_tailscale = parse_env("BIND_TAILSCALE", false)?;
        let max_command_timeout_secs = parse_env("MAX_COMMAND_TIMEOUT_SECS", 60)?;
        let max_proxy_body_bytes = parse_env("MAX_PROXY_BODY_BYTES", 4 * 1024 * 1024)?;
        let shutdown_timeout_secs = parse_env("SHUTDOWN_TIMEOUT_SECS", 30)?;

        let ssh_known_hosts = std::env::var("SSH_KNOWN_HOSTS")
            .ok()
            .map(PathBuf::from)
            .or_else(dirs_known_hosts)
            .ok_or_else(|| ConfigError("could not resolve SSH known_hosts path".into()))?;

        let api_keys = parse_csv_env("API_KEYS")
            .or_else(|| std::env::var("API_KEY").ok().map(|k| vec![k]))
            .unwrap_or_default()
            .into_iter()
            .filter(|k| !k.is_empty())
            .collect();

        let allowed_commands = parse_csv_env("ALLOWED_COMMANDS").unwrap_or_else(|| {
            vec![
                "uptime", "uname", "hostname", "whoami", "id", "date",
                "df", "free", "ps", "ss", "w", "uname", "lscpu",
            ]
            .into_iter()
            .map(String::from)
            .collect()
        });

        let allowed_proxy_ports = parse_csv_env("ALLOWED_PROXY_PORTS")
            .unwrap_or_default()
            .into_iter()
            .filter_map(|p| p.parse::<u16>().ok())
            .collect();

        let cfg = Config {
            port,
            discovery_mode,
            tailscale_auth_key,
            tailnet,
            health_poll_secs,
            ssh_user,
            ssh_known_hosts,
            bind_tailscale,
            api_keys,
            allowed_commands,
            allowed_proxy_ports,
            max_command_timeout_secs,
            max_proxy_body_bytes,
            shutdown_timeout_secs,
        };

        cfg.validate()?;
        Ok(cfg)
    }
}

#[cfg(test)]
impl Config {
    pub fn for_testing() -> Self {
        Config {
            port: 3453,
            discovery_mode: DiscoveryMode::Cli,
            tailscale_auth_key: None,
            tailnet: None,
            health_poll_secs: 30,
            ssh_user: "root".into(),
            ssh_known_hosts: PathBuf::from("/dev/null"),
            bind_tailscale: false,
            api_keys: vec!["test-api-key-32-characters-long!!".into()],
            allowed_commands: vec!["uptime".into(), "df".into(), "hostname".into()],
            allowed_proxy_ports: vec![8080],
            max_command_timeout_secs: 10,
            max_proxy_body_bytes: 1024 * 1024,
            shutdown_timeout_secs: 5,
        }
    }
}

fn non_empty_env(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.is_empty())
}

fn parse_env<T: FromStr>(key: &str, default: T) -> Result<T, ConfigError> {
    match std::env::var(key) {
        Ok(v) if !v.is_empty() => v.parse::<T>().map_err(|_| {
            ConfigError(format!("{} could not be parsed: {}", key, v))
        }),
        _ => Ok(default),
    }
}

fn parse_csv_env(key: &str) -> Option<Vec<String>> {
    std::env::var(key).ok().map(|v| {
        v.split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    })
}

fn dirs_known_hosts() -> Option<PathBuf> {
    std::env::var("HOME").ok().map(|h| {
        let mut p = PathBuf::from(h);
        p.push(".ssh");
        p.push("known_hosts");
        p
    })
}
