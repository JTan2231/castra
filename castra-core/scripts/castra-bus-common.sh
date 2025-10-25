#!/bin/bash
set -euo pipefail

# Shared helpers for Castra guest ↔ broker bus interactions.
# These routines assume bash (for /dev/tcp and parameter expansion) and BusyBox-friendly utilities.

BUS_CONF_FILE="${BUS_CONF_FILE:-/etc/conf.d/castra-handshake}"
BUS_MAX_FRAME_BYTES=65536
BUS_DEFAULT_HEARTBEAT_INTERVAL=25

bus_log() {
  local level=$1
  shift
  printf '[castra-bus] %s: %s\n' "$level" "$*" >&2
}

bus_resolve_config() {
  if [[ -n "${BUS_CONF_FILE}" && -r "${BUS_CONF_FILE}" ]]; then
    # shellcheck disable=SC1090
    . "${BUS_CONF_FILE}"
  fi

  BUS_HOST=${BUS_HOST:-${BROKER_HOST:-10.0.2.2}}
  BUS_PORT=${BUS_PORT:-${BROKER_PORT:-7070}}
  BUS_VM_NAME=${BUS_VM_NAME:-${BROKER_VM_NAME:-$(hostname -s 2>/dev/null || printf 'vm')}}
  BUS_HEARTBEAT_INTERVAL=${BUS_HEARTBEAT_INTERVAL:-${HEARTBEAT_INTERVAL:-$BUS_DEFAULT_HEARTBEAT_INTERVAL}}

  if ! [[ "$BUS_PORT" =~ ^[0-9]+$ ]]; then
    bus_log "error" "Invalid broker port: ${BUS_PORT}"
    return 1
  fi

  if ! [[ "$BUS_HEARTBEAT_INTERVAL" =~ ^[0-9]+$ ]]; then
    bus_log "error" "Heartbeat interval must be numeric (seconds)."
    return 1
  fi

  if (( BUS_HEARTBEAT_INTERVAL <= 0 )); then
    bus_log "warn" "Heartbeat interval <= 0; clamping to ${BUS_DEFAULT_HEARTBEAT_INTERVAL}s."
    BUS_HEARTBEAT_INTERVAL=$BUS_DEFAULT_HEARTBEAT_INTERVAL
  fi

  if (( BUS_HEARTBEAT_INTERVAL > 30 )); then
    bus_log "warn" "Heartbeat interval ${BUS_HEARTBEAT_INTERVAL}s exceeds broker SLA; clamping to 30s."
    BUS_HEARTBEAT_INTERVAL=30
  fi
}

bus_open_socket() {
  exec 3<>"/dev/tcp/${BUS_HOST}/${BUS_PORT}" || {
    bus_log "error" "Failed to connect to broker at ${BUS_HOST}:${BUS_PORT}"
    return 1
  }
}

bus_close_socket() {
  exec 3>&- || true
  exec 3<&- || true
}

bus_handshake() {
  local greeting response

  IFS= read -r greeting <&3 || {
    bus_log "error" "Broker greeting unavailable; connection dropped."
    return 1
  }

  printf 'hello vm:%s capabilities=bus-v1\n' "${BUS_VM_NAME}" >&3

  IFS= read -r response <&3 || {
    bus_log "error" "Broker closed connection before handshake completed."
    return 1
  }

  case "$response" in
    'ok session='*)
      BUS_SESSION_TOKEN=${response#ok session=}
      bus_log "info" "Session granted (token=${BUS_SESSION_TOKEN})."
      ;;
    'ok')
      bus_log "error" "Handshake accepted but session denied (missing capabilities?)."
      return 1
      ;;
    *)
      bus_log "error" "Broker rejected handshake: ${response}"
      return 1
      ;;
  esac
}

bus__byte_prefix() {
  local len=$1
  printf '\\x%02x\\x%02x\\x%02x\\x%02x' \
    $(((len >> 24) & 0xff)) \
    $(((len >> 16) & 0xff)) \
    $(((len >> 8) & 0xff)) \
    $((len & 0xff))
}

bus_send_frame() {
  local json=$1
  local len prefix

  len=$(printf '%s' "$json" | LC_ALL=C wc -c | tr -d ' ')

  if (( len > BUS_MAX_FRAME_BYTES )); then
    bus_log "error" "Frame too large: ${len} bytes (max ${BUS_MAX_FRAME_BYTES})."
    return 1
  fi

  prefix=$(bus__byte_prefix "$len")
  if ! printf '%b' "$prefix" >&3; then
    bus_log "error" "Failed to write frame prefix to broker."
    return 1
  fi
  if ! printf '%s' "$json" >&3; then
    bus_log "error" "Failed to write frame payload to broker."
    return 1
  fi
}

bus_read_frame() {
  local len_bytes_str len_bytes len payload payload_len

  if command -v od >/dev/null 2>&1; then
    if ! len_bytes_str=$(dd bs=1 count=4 <&3 2>/dev/null | od -An -t u1); then
      len_bytes_str=""
    fi
  elif command -v hexdump >/dev/null 2>&1; then
    if ! len_bytes_str=$(dd bs=1 count=4 <&3 2>/dev/null | hexdump -v -e '1/1 "%u "'); then
      len_bytes_str=""
    fi
  else
    bus_log "error" "Neither od nor hexdump is available to decode broker frames."
    return 1
  fi

  if [[ -z "${len_bytes_str// }" ]]; then
    return 1
  fi

  read -r -a len_bytes <<<"${len_bytes_str}"
  if (( ${#len_bytes[@]} != 4 )); then
    bus_log "error" "Failed to decode broker frame length."
    return 1
  fi

  len=$(( (len_bytes[0] << 24) | (len_bytes[1] << 16) | (len_bytes[2] << 8) | len_bytes[3] ))
  if (( len == 0 )); then
    printf ''
    return 0
  fi

  if (( len > BUS_MAX_FRAME_BYTES )); then
    bus_log "error" "Broker frame length ${len} exceeds max ${BUS_MAX_FRAME_BYTES}."
    return 1
  fi

  payload=$(dd bs=1 count="$len" <&3 2>/dev/null || true)
  payload_len=$(printf '%s' "$payload" | LC_ALL=C wc -c | tr -d ' ')
  if [[ "$payload_len" != "$len" ]]; then
    bus_log "error" "Truncated broker frame; expected ${len} bytes, received ${payload_len}."
    return 1
  fi

  printf '%s' "$payload"
}

bus_read_ack() {
  local expected=${1-}
  local ack
  ack=$(bus_read_frame) || {
    bus_log "error" "Failed to read ack from broker."
    return 1
  }

  if [[ "$ack" != *'"type":"ack"'* ]]; then
    bus_log "error" "Broker reply missing ack envelope: ${ack}"
    return 1
  fi

  if [[ -n "${expected:-}" && "$ack" != *'"ack":"'"${expected}"'"'* ]]; then
    bus_log "error" "Broker ack mismatch; expected ${expected}, received: ${ack}"
    return 1
  fi

  printf '%s' "$ack"
}

bus_json_quote() {
  local str=$1
  local escaped='"'
  local i ch code

  for (( i = 0; i < ${#str}; i++ )); do
    ch=${str:i:1}
    case "$ch" in
      '"') escaped+='\"' ;;
      '\\') escaped+='\\\\' ;;
      $'\b') escaped+='\\b' ;;
      $'\f') escaped+='\\f' ;;
      $'\n') escaped+='\\n' ;;
      $'\r') escaped+='\\r' ;;
      $'\t') escaped+='\\t' ;;
      "'") escaped+="'" ;;
      *)
        printf -v code '%d' "'$ch"
        if (( code < 32 )); then
          printf -v code '%02X' "$code"
          escaped+="\\u00${code}"
        else
          escaped+="$ch"
        fi
        ;;
    esac
  done

  escaped+='"'
  printf '%s' "$escaped"
}

bus_build_publish() {
  local topic=$1
  local payload_json=$2
  local topic_json

  topic_json=$(bus_json_quote "$topic")
  printf '{"type":"publish","topic":%s,"payload":%s}' "$topic_json" "$payload_json"
}

bus_publish() {
  local topic=${1:-broadcast}
  local payload_json=${2:-'{}'}
  local frame ack

  frame=$(bus_build_publish "$topic" "$payload_json")
  bus_send_frame "$frame"
  ack=$(bus_read_ack "publish") || return 1

  if [[ "$ack" != *'"status":"ok"'* ]]; then
    bus_log "warn" "Broker returned non-ok publish ack: ${ack}"
  fi

  printf '%s' "$ack"
}

bus_send_heartbeat() {
  local ack
  bus_send_frame '{"type":"heartbeat"}'
  ack=$(bus_read_ack "heartbeat") || return 1

  if [[ "$ack" != *'"status":"ok"'* ]]; then
    bus_log "warn" "Heartbeat ack reported error: ${ack}"
  fi

  printf '%s' "$ack"
}
