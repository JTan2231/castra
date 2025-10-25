#!/usr/bin/env sh
set -euo pipefail

# Castra injects these environment variables before running the script. They
# make it easy to correlate guest logs with the host-side bootstrap run.
echo "[castra] run id ${CASTRA_RUN_ID:-unknown}"
echo "[castra] applying bootstrap for ${CASTRA_VM:-unknown}"

dir="state"
mkdir -p "$dir"

# The payload is optional. When present we surface its contents under the state
# directory so subsequent steps (or humans) can inspect what was shipped.
if [ -d "${CASTRA_PAYLOAD_DIR:-}" ] && [ -f "${CASTRA_PAYLOAD_DIR}/app.env" ]; then
  cp "${CASTRA_PAYLOAD_DIR}/app.env" "$dir/app.env"
  echo "[castra] copied payload env to $dir/app.env"
fi

# Persist a couple of sentinel files so the bootstrap verification stage has
# something concrete to check.
printf '%s\n' "${SERVICE_USER:-deployer}" > "$dir/bootstrap-user"
printf '%s\n' "${APP_NAME:-demo-service}" > "$dir/app-name"

echo "[castra] bootstrap complete for ${CASTRA_VM:-unknown}"
