#!/bin/bash

TAILSCALE_IP="100.108.215.102"
PORT="3453"
BASE_URL="http://$TAILSCALE_IP:$PORT"

echo "Testing Tailscale API endpoints..."
echo "=================================="
echo ""

# Test health endpoint
echo "Testing /health endpoint:"
curl -s "$BASE_URL/health" | jq .
echo ""

# Test devices endpoint
echo "Testing /devices endpoint:"
curl -s "$BASE_URL/devices" | jq .
echo ""

# Test connectivity
echo "Testing server connectivity:"
if curl -s "$BASE_URL/health" > /dev/null 2>&1; then
    echo "✅ Server is reachable on $BASE_URL"
else
    echo "❌ Server is NOT reachable on $BASE_URL"
fi
