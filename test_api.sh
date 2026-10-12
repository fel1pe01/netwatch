#!/bin/bash
set -e

# Start the server in background
./target/release/netwatch &
SERVER_PID=$!

# Wait for server to start
sleep 3

echo "Testing /api/v1/incidents/hypotheses..."
curl -s http://localhost:8080/api/v1/incidents/hypotheses | python3 -m json.tool

echo ""
echo "Testing /api/v1/incidents/hypotheses/test..."
curl -s http://localhost:8080/api/v1/incidents/hypotheses/test | python3 -m json.tool

echo ""
echo "Testing hypothesis confirmation..."
curl -s -X POST -H "Content-Type: application/json" \
  -d '{"hypothesis_id":"test","confirmed":true,"actor":"test"}' \
  http://localhost:8080/api/v1/incidents/test/hypothesis/confirm | python3 -m json.tool

# Clean up
kill $SERVER_PID
wait $SERVER_PID 2>/dev/null || true