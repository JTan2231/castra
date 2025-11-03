#!/usr/bin/env sh
set -euo pipefail

workdir="state"
mkdir -p "$workdir"

apk add npm
npm install -g @openai/codex

timestamp=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
{
  echo "castra_vm=${CASTRA_VM:-quickstart}"
  echo "run_id=${CASTRA_RUN_ID:-unknown}"
  echo "completed_at=${timestamp}"
} > "$workdir/quickstart.txt"

echo "[quickstart] bootstrap complete (${timestamp})"
