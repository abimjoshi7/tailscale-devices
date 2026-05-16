# tailscale-devices

A lightweight HTTP API for managing and monitoring devices on your Tailscale network. Discover devices, check reachability, run allowlisted SSH commands, and proxy HTTP requests — all from a single service running inside your tailnet.

## Features

- **Device discovery** via Tailscale CLI, LocalAPI, or Admin API
- **Health monitoring** — reachability checks with configurable polling
- **SSH command execution** — with strict host-key checking and a command allowlist
- **HTTP proxy** — forward requests to services running on tailnet devices
- **Prometheus metrics** on `/metrics`
- **API key authentication** with constant-time comparison
- **Graceful shutdown** (SIGTERM / Ctrl-C)
- **Structured JSON logging** (`LOG_FORMAT=json`)

---

## Quick Start

### Local (cargo)

```bash
cp deploy/env.example .env
# Edit .env — at minimum set API_KEY
cargo run --release
```

### Docker

```bash
# CLI mode — mounts the host Tailscale socket for device discovery
docker run -d \
  --name tailscale-devices \
  --network host \
  -v /var/run/tailscale:/var/run/tailscale:ro \
  -e API_KEY=your-secret-key \
  -e DISCOVERY_MODE=Cli \
  -e BIND_TAILSCALE=false \
  -p 3453:3453 \
  ghcr.io/abimjoshi7/tailscale-devices:latest

# Admin API mode — no host socket needed
docker run -d \
  --name tailscale-devices \
  -e API_KEY=your-secret-key \
  -e DISCOVERY_MODE=AdminApi \
  -e TAILSCALE_AUTH_KEY=tskey-api-... \
  -e TAILSCALE_TAILNET=example.com \
  -p 3453:3453 \
  ghcr.io/abimjoshi7/tailscale-devices:latest
```

### systemd (Linux server)

```bash
# Install binary
sudo cp target/release/tailscale-devices /usr/local/bin/

# Create user and config dir
sudo useradd -r -s /sbin/nologin tailscale-devices
sudo mkdir -p /etc/tailscale-devices /var/lib/tailscale-devices
sudo cp deploy/env.example /etc/tailscale-devices/env
sudo chown tailscale-devices: /var/lib/tailscale-devices
sudo chmod 640 /etc/tailscale-devices/env

# Edit /etc/tailscale-devices/env then:
sudo cp deploy/tailscale-devices.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now tailscale-devices
sudo journalctl -u tailscale-devices -f
```

---

## Configuration

All configuration is via environment variables (or `.env` file).

| Variable | Default | Description |
|---|---|---|
| `LISTEN_PORT` | `3453` | HTTP server port |
| `BIND_TAILSCALE` | `false` | Bind to Tailscale IP instead of `0.0.0.0` |
| `DISCOVERY_MODE` | `LocalApi` | `Cli` \| `LocalApi` \| `AdminApi` |
| `HEALTH_POLL_SECS` | `30` | Device discovery + reachability poll interval |
| `API_KEY` | *(unset)* | API key for auth. Must be ≥ 16 chars. Unset = no auth (warning logged). |
| `API_KEYS` | *(unset)* | Comma-separated list of keys (supports rotation) |
| `ALLOWED_COMMANDS` | *(see below)* | Comma-separated commands allowed via SSH |
| `ALLOWED_PROXY_PORTS` | *(empty)* | Comma-separated ports the proxy may forward to. Empty = proxy disabled. |
| `SSH_USER` | `root` | SSH username for command execution |
| `SSH_KNOWN_HOSTS` | `~/.ssh/known_hosts` | Path to SSH known_hosts file (strict checking) |
| `MAX_COMMAND_TIMEOUT_SECS` | `60` | Maximum SSH command timeout |
| `MAX_PROXY_BODY_BYTES` | `4194304` | Max proxy request body (4 MB) |
| `SHUTDOWN_TIMEOUT_SECS` | `30` | Graceful shutdown grace period |
| `LOG_FORMAT` | *(pretty)* | Set to `json` for structured JSON logs |
| `RUST_LOG` | `info` | Log level filter (e.g. `debug`, `tailscale_devices=trace`) |
| `TAILSCALE_AUTH_KEY` | *(unset)* | Required for `AdminApi` mode |
| `TAILSCALE_TAILNET` | *(unset)* | Required for `AdminApi` mode (e.g. `example.com`) |

**Default allowed commands:** `uptime`, `uname`, `hostname`, `whoami`, `id`, `date`, `df`, `free`, `ps`, `ss`, `w`, `lscpu`

---

## API Reference

All authenticated endpoints require the header:
```
X-API-Key: <your-api-key>
```

Every response includes `X-Request-Id` for log correlation.

### Public endpoints (no auth)

#### `GET /health`
Server and device summary.
```bash
curl http://100.x.x.x:3453/health
```
```json
{
  "status": "ok",
  "devices_total": 3,
  "devices_reachable": 2,
  "last_poll": "16, May 19:43"
}
```

#### `GET /livez`
Always 200 while the process is running.

#### `GET /readyz`
200 after the first successful device poll; 503 before.

#### `GET /metrics`
Prometheus text format metrics.
```bash
curl http://100.x.x.x:3453/metrics
```

---

### Authenticated endpoints

#### `GET /devices`
All discovered devices with health status.
```bash
curl -H "X-API-Key: $KEY" http://100.x.x.x:3453/devices
```
```json
[
  {
    "device": {
      "id": "nodekey:abc...",
      "hostname": "stonedmonk",
      "dns_name": "stonedmonk.tail7c28f6.ts.net.",
      "ip": "100.78.60.112",
      "os": "linux",
      "last_seen": "16, May 19:28",
      "is_online": true
    },
    "health": {
      "device_id": "nodekey:abc...",
      "reachable": true,
      "latency_ms": 12,
      "last_checked": "16, May 19:43",
      "http_services": {}
    }
  }
]
```

#### `GET /devices/{id}/health`
Health for a single device by ID (the `nodekey:...` value from `/devices`).
```bash
curl -H "X-API-Key: $KEY" http://100.x.x.x:3453/devices/nodekey:abc.../health
```

#### `POST /devices/{id}/command`
Run an allowlisted SSH command on a device.
```bash
curl -X POST \
  -H "X-API-Key: $KEY" \
  -H "Content-Type: application/json" \
  -d '{"command": "df", "args": ["-h"], "timeout_secs": 10}' \
  http://100.x.x.x:3453/devices/nodekey:abc.../command
```
```json
{
  "stdout": "Filesystem      Size  Used Avail Use% Mounted on\n...",
  "stderr": "",
  "exit_code": 0,
  "duration_ms": 142
}
```

**Rejected examples:**
```bash
# Blocked — not in allowlist
{"command": "rm", "args": ["-rf", "/"]}  → 403 command_not_allowed

# Blocked — shell metacharacter in arg
{"command": "uptime", "args": ["; cat /etc/passwd"]}  → 400 arg_invalid

# Blocked — path traversal in command
{"command": "/bin/sh", "args": []}  → 400 command_invalid
```

#### `GET|POST|PUT|DELETE|PATCH /proxy/{device}/{port}/{path}`
Proxy an HTTP request to a service running on a tailnet device. `{device}` can be the device ID, hostname, or DNS name. Port must be in `ALLOWED_PROXY_PORTS`.

```bash
# Forward GET to port 8080 on stonedmonk
curl -H "X-API-Key: $KEY" \
  http://100.x.x.x:3453/proxy/stonedmonk/8080/api/status

# Forward POST with body
curl -X POST \
  -H "X-API-Key: $KEY" \
  -H "Content-Type: application/json" \
  -d '{"key":"value"}' \
  http://100.x.x.x:3453/proxy/stonedmonk/3000/api/data
```

---

## Error Responses

All errors return structured JSON:
```json
{
  "error": "command_not_allowed",
  "message": "command 'rm' is not in the allowlist"
}
```

| HTTP | `error` code | Cause |
|---|---|---|
| 400 | `command_empty` | Empty command field |
| 400 | `command_invalid` | Slash or whitespace in command name |
| 400 | `arg_invalid` | Shell metacharacter in an argument |
| 400 | `proxy_path_invalid` | `..` traversal in proxy path |
| 400 | `proxy_body_too_large` | Body exceeds `MAX_PROXY_BODY_BYTES` |
| 401 | `unauthorized` | Missing or wrong `X-API-Key` |
| 403 | `command_not_allowed` | Command not in `ALLOWED_COMMANDS` |
| 403 | `proxy_disabled` | `ALLOWED_PROXY_PORTS` is empty |
| 403 | `proxy_port_not_allowed` | Port not in `ALLOWED_PROXY_PORTS` |
| 404 | `not_found` | Device ID not found |
| 502 | `upstream_failure` | SSH or upstream HTTP request failed |
| 500 | `internal_error` | Unexpected server error |

---

## SSH Setup

The service uses strict SSH host-key checking. Before a device can accept commands, its host key must be in `known_hosts`:

```bash
# Add a device's host key (run once per device)
ssh-keyscan 100.78.60.112 >> ~/.ssh/known_hosts
# or by hostname:
ssh-keyscan stonedmonk.tail7c28f6.ts.net >> ~/.ssh/known_hosts
```

The SSH user (`SSH_USER`, default `root`) must have a public key installed on the target device.

---

## Monitoring

Point Prometheus at `/metrics`:
```yaml
scrape_configs:
  - job_name: tailscale-devices
    static_configs:
      - targets: ['100.x.x.x:3453']
```

Key metrics:
- `tailscale_devices_total` — total discovered devices
- `tailscale_devices_reachable` — currently reachable (port 22 TCP)
- `tailscale_monitor_polls_total{result="ok|error"}` — poll health
- `tailscale_http_requests_total{method, status}` — request volume
- `tailscale_http_request_duration_seconds` — latency histogram
