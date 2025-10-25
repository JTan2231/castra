#!/bin/bash
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=scripts/castra-bus-common.sh
. "${SCRIPT_DIR}/castra-bus-common.sh"

usage() {
  cat <<'EOF'
Usage: castra-bus-agent.sh [--stdin-json]

Establishes a broker session, keeps heartbeats flowing, and optionally forwards
newline-delimited JSON frames from stdin to the bus.

Options:
  --stdin-json    Read JSON frames from stdin and send each one verbatim.
  --help          Show this help text.
EOF
}

MODE="heartbeat"
if [[ "${1:-}" == "--help" ]]; then
  usage
  exit 0
elif [[ "${1:-}" == "--stdin-json" ]]; then
  MODE="stdin-json"
  shift
fi

bus_resolve_config
BUS_BACKOFF_SECONDS=1
BUS_MAX_BACKOFF_SECONDS=30
HEARTBEAT_PID=""

bus_reset_backoff() {
  BUS_BACKOFF_SECONDS=1
}

bus_backoff_sleep() {
  local delay=$BUS_BACKOFF_SECONDS
  sleep "$delay"
  if (( BUS_BACKOFF_SECONDS < BUS_MAX_BACKOFF_SECONDS )); then
    BUS_BACKOFF_SECONDS=$(( BUS_BACKOFF_SECONDS * 2 ))
    if (( BUS_BACKOFF_SECONDS > BUS_MAX_BACKOFF_SECONDS )); then
      BUS_BACKOFF_SECONDS=$BUS_MAX_BACKOFF_SECONDS
    fi
  fi
}

cleanup() {
  if [[ -n "${HEARTBEAT_PID:-}" ]]; then
    kill -TERM "${HEARTBEAT_PID}" 2>/dev/null || true
    wait "${HEARTBEAT_PID}" 2>/dev/null || true
    HEARTBEAT_PID=""
  fi
  bus_close_socket
}

trap 'cleanup' EXIT
trap 'cleanup; exit 0' INT TERM

heartbeat_loop() {
  while true; do
    bus_send_heartbeat >/dev/null || {
      bus_log "error" "Heartbeat failed; exiting."
      return 1
    }
    sleep "${BUS_HEARTBEAT_INTERVAL}"
  done
}

while true; do
  bus_close_socket 2>/dev/null || true

  if ! bus_open_socket; then
    bus_log "warn" "Unable to connect to broker at ${BUS_HOST}:${BUS_PORT}; retrying in ${BUS_BACKOFF_SECONDS}s."
    bus_backoff_sleep
    continue
  fi

  if ! bus_handshake; then
    bus_log "warn" "Handshake failed; retrying in ${BUS_BACKOFF_SECONDS}s."
    bus_close_socket
    bus_backoff_sleep
    continue
  fi

  bus_reset_backoff

  if [[ "${MODE}" == "stdin-json" ]]; then
    bus_log "info" "Forwarding JSON frames from stdin."
    heartbeat_loop &
    HEARTBEAT_PID=$!

    while IFS= read -r line; do
      [[ -z "$line" ]] && continue
      bus_send_frame "$line"
      ack=$(bus_read_ack) || {
        bus_log "error" "Broker dropped connection while sending stdin frame."
        cleanup
        exit 1
      }
      printf '%s\n' "$ack"
      if [[ "$ack" != *'"status":"ok"'* ]]; then
        bus_log "warn" "Broker ack signalled error: ${ack}"
      fi
    done

    bus_log "info" "stdin closed; shutting down."
    cleanup
    break
  else
    bus_log "info" "Heartbeat-only mode. Press Ctrl+C to exit."
    if ! heartbeat_loop; then
      bus_log "warn" "Heartbeat loop stopped; retrying in ${BUS_BACKOFF_SECONDS}s."
      cleanup
      bus_backoff_sleep
      continue
    fi
  fi
done
