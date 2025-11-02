#!/usr/bin/env bash
set -euo pipefail

usage() {
    cat <<'EOF'
Usage:
  vm_commands.sh send [command] "<command text>"
  vm_commands.sh interrupt <pgid>
  vm_commands.sh list

Environment:
  SSH_TARGET      Remote ssh target (e.g. user@host). Required.
  SSH_PORT        Optional ssh port (defaults to 22).
  SSH_EXTRA_OPTS  Additional ssh options (space-separated).
  SSH_STRICT=1    Keep strict host key checking (default disables).
EOF
}

declare -a SSH_CMD=()

build_ssh_command() {
    if [[ -z "${SSH_TARGET:-}" ]]; then
        echo "SSH_TARGET is not set. Export SSH_TARGET=user@host." >&2
        exit 1
    fi

    SSH_CMD=(ssh)

    if [[ "${SSH_STRICT:-0}" != "1" ]]; then
        SSH_CMD+=(-o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null)
    fi

    if [[ -n "${SSH_PORT:-}" ]]; then
        SSH_CMD+=(-p "${SSH_PORT}")
    fi

    if [[ -n "${SSH_EXTRA_OPTS:-}" ]]; then
        local -a extra_opts
        # shellcheck disable=SC2206
        extra_opts=(${SSH_EXTRA_OPTS})
        SSH_CMD+=("${extra_opts[@]}")
    fi

    SSH_CMD+=("${SSH_TARGET}")
}

ssh_invoke() {
    build_ssh_command
    "${SSH_CMD[@]}" "$@"
}

send_command() {
    if [[ $# -eq 0 ]]; then
        echo "No command provided for send." >&2
        usage
        exit 1
    fi

    local run_id
    run_id="$(date +%s%N)"
    local cmd="$*"

ssh_invoke bash -s -- "$run_id" "$cmd" <<'EOF'
set -euo pipefail

RUN_ID="$1"
shift

if [[ $# -eq 0 ]]; then
    echo "No command payload supplied." >&2
    exit 1
fi

CMD="$*"
RUN_DIR="/run/vizier/${RUN_ID}"
SESSION="vizier-${RUN_ID}"

mkdir -p "$RUN_DIR"
chmod 700 "$RUN_DIR"

printf '%s\n' "$CMD" > "$RUN_DIR/command"
date -Is > "$RUN_DIR/started_at"

cat > "$RUN_DIR/entry.sh" <<'SCRIPT'
#!/usr/bin/env bash
set -euo pipefail

RUN_DIR="$1"
CMD_FILE="$RUN_DIR/command"
CMD="$(cat "$CMD_FILE")"

exec </dev/null
mkdir -p "$RUN_DIR"
touch "$RUN_DIR/stdout" "$RUN_DIR/stderr"

echo $$ > "$RUN_DIR/pid"
PGID="$(awk '{print $5}' "/proc/$$/stat")"
echo "$PGID" > "$RUN_DIR/pgid"

trap 'date -Is > "$RUN_DIR/stopped_at"' EXIT

if command -v stdbuf >/dev/null 2>&1; then
    stdbuf -oL -eL bash -lc "$CMD" >>"$RUN_DIR/stdout" 2>>"$RUN_DIR/stderr"
else
    bash -lc "$CMD" >>"$RUN_DIR/stdout" 2>>"$RUN_DIR/stderr"
fi
SCRIPT

chmod 500 "$RUN_DIR/entry.sh"

if command -v tmux >/dev/null 2>&1; then
    tmux has-session -t "$SESSION" 2>/dev/null && tmux kill-session -t "$SESSION"
    tmux new-session -d -s "$SESSION" -- "$RUN_DIR/entry.sh" "$RUN_DIR"

    for _ in {1..50}; do
        if [[ -s "$RUN_DIR/pgid" ]]; then
            break
        fi
        sleep 0.1
    done

    pane_pid="$(tmux display-message -p -t "$SESSION" '#{pane_pid}' 2>/dev/null || true)"
    if [[ -n "$pane_pid" ]]; then
        printf '%s\n' "$pane_pid" > "$RUN_DIR/pane_pid"
    fi
else
    setsid "$RUN_DIR/entry.sh" "$RUN_DIR" >/dev/null 2>&1 &
    launcher_pid="$!"
    printf '%s\n' "$launcher_pid" > "$RUN_DIR/launcher_pid"

    for _ in {1..50}; do
        if [[ -s "$RUN_DIR/pgid" ]]; then
            break
        fi
        sleep 0.1
    done
fi

pgid=""
if [[ -s "$RUN_DIR/pgid" ]]; then
    pgid="$(cat "$RUN_DIR/pgid")"
fi

echo "RUN_ID=$RUN_ID"
echo "SESSION=$SESSION"
if [[ -n "$pgid" ]]; then
    echo "PGID=$pgid"
else
    echo "PGID=<unknown>"
fi
EOF
}

interrupt_process() {
    if [[ $# -ne 1 ]]; then
        echo "interrupt requires exactly one argument: <pgid>" >&2
        usage
        exit 1
    fi

    local pgid="$1"
    if [[ "$pgid" != [0-9]* ]]; then
        echo "Invalid PGID: $pgid" >&2
        exit 1
    fi

    ssh_invoke "kill -SIGINT -- -$pgid"
}

list_runs() {
    ssh_invoke bash -s <<'EOF'
set -euo pipefail
shopt -s nullglob
printf '%s\t%s\t%s\t%s\n' "RUN_ID" "PGID" "STATUS" "COMMAND"
for dir in /run/vizier/*; do
    [[ -d "$dir" ]] || continue
    run_id="$(basename "$dir")"
    pgid="$(cat "$dir/pgid" 2>/dev/null || echo '-')"
    status="running"
    [[ -f "$dir/stopped_at" ]] && status="stopped"
    cmd="$(tr -d '\r' < "$dir/command" 2>/dev/null || echo '')"
    printf '%s\t%s\t%s\t%s\n' "$run_id" "$pgid" "$status" "$cmd"
done
EOF
}

if [[ $# -lt 1 ]]; then
    usage
    exit 1
fi

action="$1"
shift

case "$action" in
    send)
        if [[ ${1:-} == "command" ]]; then
            shift
        fi
        send_command "$@"
        ;;
    interrupt)
        interrupt_process "$@"
        ;;
    list)
        list_runs
        ;;
    *)
        usage
        exit 1
        ;;
esac
