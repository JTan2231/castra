#!/usr/bin/env bash
# Recreate Castra's managed-image pipeline for alpine-minimal@v1 using a pre-fetched VHD.
# - Validates the source checksum (optional).
# - Converts the VHD into qcow2 (rootfs.qcow2).
# - Mounts the qcow2 via qemu-nbd to enable TTY autologin and broker-aware SSH handshake.
# - Installs/configures OpenSSH (unless skipped) and wires an OpenRC service that announces to the broker.
# - Emits manifest.json aligned with ImageManager expectations.
#
# Run as root on a Linux host with qemu-img/qemu-nbd available.

set -euo pipefail

SOURCE_SHA="8f58945cd972f31b8a7e3116d2b33cdb4298e6b3c0609c0bfd083964678afffb"
SPEC_DIGEST="171847f444ddb6e6d0399e9f2000d26727703d8560001eb9386337d34ae1ea21"
DEFAULT_IMAGE_ID="alpine-minimal"
DEFAULT_IMAGE_VERSION="v1"

usage() {
  cat <<'EOF'
Usage: castra-alpine-postprocess.sh --vhd PATH [options]

Options:
  --state-root PATH       State root to populate (default: $PWD/.castra/manual)
  --vm-name NAME          VM identity to advertise to the broker (default: alpine)
  --nbd-device DEV        nbd node to use when mounting (default: /dev/nbd0)
  --skip-checksums        Skip validating the source VHD checksum
  --skip-ssh-install      Assume OpenSSH already exists inside the guest; skip apk add
  --help                  Show this message
EOF
}

die() {
  echo "error: $*" >&2
  exit 1
}

log() {
  echo "[+] $*"
}

require_cmd() {
  for cmd in "$@"; do
    command -v "$cmd" >/dev/null 2>&1 || die "missing required command: $cmd"
  done
}

ensure_root() {
  if [[ ${EUID:-$(id -u)} -ne 0 ]]; then
    die "must run as root (sudo)"
  fi
}

# --- argument parsing -------------------------------------------------------------------------

VHD_PATH=""
STATE_ROOT=""
VM_NAME="alpine"
NBD_DEVICE="/dev/nbd0"
VERIFY_SHA=1
INSTALL_SSH=1

while [[ $# -gt 0 ]]; do
  case "$1" in
    --vhd)
      [[ $# -ge 2 ]] || die "--vhd expects a path"
      VHD_PATH="$2"
      shift 2
      ;;
    --state-root)
      [[ $# -ge 2 ]] || die "--state-root expects a path"
      STATE_ROOT="$2"
      shift 2
      ;;
    --vm-name)
      [[ $# -ge 2 ]] || die "--vm-name expects a value"
      VM_NAME="$2"
      shift 2
      ;;
    --nbd-device)
      [[ $# -ge 2 ]] || die "--nbd-device expects a device (e.g. /dev/nbd1)"
      NBD_DEVICE="$2"
      shift 2
      ;;
    --skip-checksums)
      VERIFY_SHA=0
      shift
      ;;
    --skip-ssh-install)
      INSTALL_SSH=0
      shift
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      die "unknown argument: $1"
      ;;
  esac
done

[[ -n "$VHD_PATH" ]] || die "missing required --vhd argument"
[[ -f "$VHD_PATH" ]] || die "VHD not found: $VHD_PATH"

STATE_ROOT="${STATE_ROOT:-$PWD/.castra/manual}"
STATE_ROOT="$(cd "$(dirname "$STATE_ROOT")" 2>/dev/null && pwd)/$(basename "$STATE_ROOT")"
VHD_PATH="$(realpath "$VHD_PATH")"
VM_NAME="${VM_NAME:-alpine}"

ensure_root
require_cmd qemu-img qemu-nbd sha256sum stat mount umount modprobe sed awk grep chroot mountpoint date

[[ -b "$NBD_DEVICE" ]] || die "nbd device not present: $NBD_DEVICE (modprobe nbd?)"
if [[ -f "/sys/block/${NBD_DEVICE#/dev/}/pid" && -s "/sys/block/${NBD_DEVICE#/dev/}/pid" ]]; then
  die "nbd device $NBD_DEVICE is busy (disconnect it first)"
fi

mkdir -p "$STATE_ROOT"

WORK_DIR="$(mktemp -d "${TMPDIR:-/tmp}/castra-alpine.XXXXXX")"
MOUNT_DIR="$WORK_DIR/mnt"
mkdir -p "$MOUNT_DIR"
QCOW_STAGE="$WORK_DIR/rootfs.qcow2"

cleanup() {
  local rc=$?
  if mountpoint -q "$MOUNT_DIR"; then
    umount "$MOUNT_DIR" || log "warning: failed to unmount $MOUNT_DIR"
  fi
  if [[ -n "${NBD_ATTACHED:-}" ]]; then
    qemu-nbd --disconnect "$NBD_DEVICE" || log "warning: failed to disconnect $NBD_DEVICE"
  fi
  rm -rf "$WORK_DIR"
  exit "$rc"
}
trap cleanup EXIT

# --- pipeline replication ---------------------------------------------------------------------

IMAGE_DIR="$STATE_ROOT/images/$DEFAULT_IMAGE_ID/$DEFAULT_IMAGE_VERSION"
mkdir -p "$IMAGE_DIR"

if [[ $VERIFY_SHA -eq 1 ]]; then
  log "verifying VHD checksum"
  ACTUAL_SHA="$(sha256sum "$VHD_PATH" | awk '{print $1}')"
  if [[ "$ACTUAL_SHA" != "$SOURCE_SHA" ]]; then
    die "checksum mismatch: expected $SOURCE_SHA, got $ACTUAL_SHA"
  fi
else
  log "skipping VHD checksum verification (--skip-checksums)"
fi

log "converting VHD → qcow2"
qemu-img convert -f vpc -O qcow2 "$VHD_PATH" "$QCOW_STAGE"

modprobe nbd max_part=16
log "attaching qcow2 via $NBD_DEVICE"
qemu-nbd --connect "$NBD_DEVICE" "$QCOW_STAGE"
NBD_ATTACHED=1

ROOT_PART="${NBD_DEVICE}p1"
for _ in {1..20}; do
  [[ -b "$ROOT_PART" ]] && break
  sleep 0.25
done
[[ -b "$ROOT_PART" ]] || die "root partition not detected at $ROOT_PART"

log "mounting $ROOT_PART"
mount "$ROOT_PART" "$MOUNT_DIR"

# --- guest customization ----------------------------------------------------------------------

rewrite_getty() {
  local inittab="$1/etc/inittab"
  [[ -f "$inittab" ]] || die "$inittab not found inside guest"

  log "configuring autologin on tty1/ttyS0"
  if grep -q '^tty1::' "$inittab"; then
    sed -i 's#^tty1::.*#tty1::respawn:/sbin/agetty --autologin root --noclear tty1 linux#' "$inittab"
  else
    echo 'tty1::respawn:/sbin/agetty --autologin root --noclear tty1 linux' >>"$inittab"
  fi

  if grep -q '^ttyS0::' "$inittab"; then
    sed -i 's#^ttyS0::.*#ttyS0::respawn:/sbin/agetty --autologin root --noclear ttyS0 115200 vt100#' "$inittab"
  else
    echo 'ttyS0::respawn:/sbin/agetty --autologin root --noclear ttyS0 115200 vt100' >>"$inittab"
  fi
}

write_handshake_assets() {
  local root="$1"
  local bin="$root/usr/local/bin"
  local confd="$root/etc/conf.d"
  local initd="$root/etc/init.d"
  mkdir -p "$bin" "$confd" "$initd"

  cat >"$bin/castra-handshake.sh" <<'EOF'
#!/bin/sh
set -eu
CONF="/etc/conf.d/castra-handshake"
if [ "${1:-}" = "--config" ]; then
  CONF="$2"
  shift 2
fi
[ -f "$CONF" ] && . "$CONF"
: "${BROKER_HOST:=10.0.2.2}"
: "${BROKER_PORT:=7070}"
: "${BROKER_CAPABILITIES:=bus-v1,ssh}"
: "${BROKER_VM_NAME:=alpine}"
: "${BROKER_RETRY_DELAY:=5}"
: "${BROKER_MAX_ATTEMPTS:=30}"

attempt=1
while [ "$BROKER_MAX_ATTEMPTS" -eq 0 ] || [ "$attempt" -le "$BROKER_MAX_ATTEMPTS" ]; do
  if printf 'hello vm:%s capabilities=%s\n' "$BROKER_VM_NAME" "$BROKER_CAPABILITIES" \
    | nc -w 5 "$BROKER_HOST" "$BROKER_PORT" | grep -qi '^ok'; then
    echo "castra-handshake: handshake acknowledged after $attempt attempt(s)"
    exit 0
  fi
  echo "castra-handshake: retrying in ${BROKER_RETRY_DELAY}s (attempt $attempt failed)" >&2
  attempt=$((attempt + 1))
  sleep "$BROKER_RETRY_DELAY"
done
echo "castra-handshake: exhausted retries contacting broker ${BROKER_HOST}:${BROKER_PORT}" >&2
exit 1
EOF
  chmod +x "$bin/castra-handshake.sh"

  cat >"$confd/castra-handshake" <<EOF
BROKER_HOST="10.0.2.2"
BROKER_PORT="7070"
BROKER_CAPABILITIES="bus-v1,ssh"
BROKER_VM_NAME="${VM_NAME}"
BROKER_RETRY_DELAY=5
BROKER_MAX_ATTEMPTS=30
EOF

  cat >"$initd/castra-handshake" <<'EOF'
#!/sbin/openrc-run
description="Castra broker handshake announcer"
command="/usr/local/bin/castra-handshake.sh"
command_args="--config /etc/conf.d/castra-handshake"
depend() {
  need net
  use sshd
}
start() {
  ebegin "Announcing VM to Castra broker"
  ${command} ${command_args}
  eend $?
}
EOF
  chmod +x "$initd/castra-handshake"
}

configure_sshd() {
  local root="$1"
  local sshd_config="$root/etc/ssh/sshd_config"

  if [[ $INSTALL_SSH -eq 1 ]]; then
    log "installing OpenSSH via apk (inside guest)"
    chroot "$root" /bin/sh -eu <<'CHROOT'
apk update
apk add --no-cache openssh ca-certificates
rc-update add sshd default
# regenerate host keys on first boot
rm -f /etc/ssh/ssh_host_*
CHROOT
  else
    log "skipping OpenSSH install (--skip-ssh-install)"
  fi

  mkdir -p "$root/etc/ssh"
  touch "$sshd_config"

  grep -qF "PermitRootLogin without-password" "$sshd_config" || echo "PermitRootLogin without-password" >>"$sshd_config"
  grep -qF "PasswordAuthentication yes" "$sshd_config" || echo "PasswordAuthentication yes" >>"$sshd_config"
  grep -qF "ChallengeResponseAuthentication no" "$sshd_config" || echo "ChallengeResponseAuthentication no" >>"$sshd_config"
  grep -qF "UseDNS no" "$sshd_config" || echo "UseDNS no" >>"$sshd_config"

  chroot "$root" /bin/sh -eu <<'CHROOT'
if ! rc-status default | grep -q '^sshd'; then
  rc-update add sshd default
fi
CHROOT
}

enable_handshake_service() {
  local root="$1"
  chroot "$root" /bin/sh -eu <<'CHROOT'
rc-update add castra-handshake default
CHROOT
}

rewrite_getty "$MOUNT_DIR"
write_handshake_assets "$MOUNT_DIR"
configure_sshd "$MOUNT_DIR"
enable_handshake_service "$MOUNT_DIR"

# --- finalize artifact ------------------------------------------------------------------------

umount "$MOUNT_DIR"
if [[ -n "${NBD_ATTACHED:-}" ]]; then
  qemu-nbd --disconnect "$NBD_DEVICE"
  unset NBD_ATTACHED
fi

FINAL_QCOW="$IMAGE_DIR/rootfs.qcow2"
log "sealing qcow2 into $FINAL_QCOW"
mv "$QCOW_STAGE" "$FINAL_QCOW"

TIMESTAMP="$(date +%s)"
FINAL_SHA="$(sha256sum "$FINAL_QCOW" | awk '{print $1}')"
FINAL_SIZE="$(stat --format=%s "$FINAL_QCOW")"

cat >"$IMAGE_DIR/manifest.json" <<EOF
{
  "spec_digest": "$SPEC_DIGEST",
  "last_checked": $TIMESTAMP,
  "artifacts": {
    "rootfs.qcow2": {
      "final_sha256": "$FINAL_SHA",
      "size": $FINAL_SIZE,
      "updated_at": $TIMESTAMP,
      "source_sha256": "$SOURCE_SHA"
    }
  }
}
EOF

log "manifest.json written to $IMAGE_DIR/manifest.json"
log "all done – alpine-minimal qcow2 ready with autologin + broker handshake"
