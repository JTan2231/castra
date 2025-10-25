#!/bin/bash
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=scripts/castra-bus-common.sh
. "${SCRIPT_DIR}/castra-bus-common.sh"

usage() {
  cat <<'EOF'
Usage: castra-bus-publish.sh [--topic TOPIC] --payload JSON
       castra-bus-publish.sh --raw JSON

Examples:
  castra-bus-publish.sh --topic broadcast --payload '{"event":"ready"}'
  castra-bus-publish.sh --raw '{"type":"heartbeat"}'

Options:
  --topic TOPIC   Publish to the provided topic (defaults to broadcast).
  --payload JSON  JSON payload to embed under the "payload" key.
  --raw JSON      Send a fully formed JSON frame directly.
  --help          Show this help message.
EOF
}

topic="broadcast"
payload_json=""
raw_frame=""

while (($#)); do
  case "$1" in
    --help)
      usage
      exit 0
      ;;
    --topic)
      [[ $# -ge 2 ]] || { usage; exit 1; }
      topic=$2
      shift 2
      ;;
    --payload)
      [[ $# -ge 2 ]] || { usage; exit 1; }
      payload_json=$2
      shift 2
      ;;
    --raw)
      [[ $# -ge 2 ]] || { usage; exit 1; }
      raw_frame=$2
      shift 2
      ;;
    *)
      bus_log "error" "Unknown argument: $1"
      usage
      exit 1
      ;;
  esac
done

if [[ -n "$raw_frame" && -n "$payload_json" ]]; then
  bus_log "error" "Choose either --payload or --raw, not both."
  exit 1
fi

bus_resolve_config
bus_open_socket
trap 'bus_close_socket' EXIT

bus_handshake

if [[ -n "$raw_frame" ]]; then
  bus_send_frame "$raw_frame"
  ack=$(bus_read_ack) || {
    bus_log "error" "Failed to read broker ack."
    exit 1
  }
else
  payload_json=${payload_json:-'{}'}
  ack=$(bus_publish "$topic" "$payload_json")
fi

printf '%s\n' "$ack"

if [[ "$ack" != *'"status":"ok"'* ]]; then
  bus_log "warn" "Broker ack reported error: ${ack}"
fi
