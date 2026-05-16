#!/bin/bash
# Quick smoke-test for all API endpoints.
# Usage: API_KEY=<your-key> ./test_api.sh [tailscale-ip] [port]

TAILSCALE_IP="${1:-100.108.215.102}"
PORT="${2:-3453}"
BASE_URL="http://$TAILSCALE_IP:$PORT"

# Prefer API_KEY env var; fall back to the dev default from .env
API_KEY="${API_KEY:-dev_local_only_change_me_32chars}"
AUTH=(-H "X-API-Key: $API_KEY")

echo "Testing Tailscale Devices API"
echo "Base URL : $BASE_URL"
echo "Auth key : ${API_KEY:0:8}..."
echo "============================================"

section() { echo ""; echo "--- $* ---"; }

# ── Public endpoints (no auth required) ──────────────────────────────────────

section "/health (public)"
curl -sf "$BASE_URL/health" | jq .

section "/livez (public)"
curl -sf "$BASE_URL/livez" | jq .

section "/readyz (public)"
curl -sf "$BASE_URL/readyz" | jq . || echo "(503 = monitor has not polled yet, normal on first start)"

section "/metrics (public, Prometheus text format)"
curl -sf "$BASE_URL/metrics" | head -20
echo "..."

# ── Authenticated endpoints ───────────────────────────────────────────────────

section "GET /devices (requires auth)"
curl -sf "${AUTH[@]}" "$BASE_URL/devices" | jq .

DEVICE_ID=$(curl -sf "${AUTH[@]}" "$BASE_URL/devices" | jq -r '.[0].id // empty')

if [ -z "$DEVICE_ID" ]; then
    echo ""
    echo "No devices found — skipping per-device endpoint tests."
else
    section "GET /devices/$DEVICE_ID/health"
    curl -sf "${AUTH[@]}" "$BASE_URL/devices/$DEVICE_ID/health" | jq .
    # Reachability is checked via TCP port 22 only. Devices without SSH
    # running will appear unreachable even when active in Tailscale.

    section "POST /devices/$DEVICE_ID/command (uptime)"
    curl -sf "${AUTH[@]}" \
         -H "Content-Type: application/json" \
         -d '{"command":"uptime","args":[]}' \
         "$BASE_URL/devices/$DEVICE_ID/command" | jq .
fi

# ── Auth rejection checks ─────────────────────────────────────────────────────

section "GET /devices without key → expect 401"
STATUS=$(curl -o /dev/null -sw "%{http_code}" "$BASE_URL/devices")
[ "$STATUS" = "401" ] && echo "OK 401" || echo "FAIL got $STATUS"

section "GET /devices with wrong key → expect 401"
STATUS=$(curl -o /dev/null -sw "%{http_code}" -H "X-API-Key: wrong" "$BASE_URL/devices")
[ "$STATUS" = "401" ] && echo "OK 401" || echo "FAIL got $STATUS"

# ── Proxy (403 when ALLOWED_PROXY_PORTS is empty) ────────────────────────────

section "GET /proxy/somedevice/9999/path → expect 403 (port not allowed)"
curl -sf "${AUTH[@]}" "$BASE_URL/proxy/somedevice/9999/path" | jq . || true

echo ""
echo "============================================"
echo "Smoke test complete."
