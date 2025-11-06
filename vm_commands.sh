#!/usr/bin/env bash
set -euo pipefail

usage() {
    cat <<'EOF'
Usage:
  vm_commands.sh send [--wait] [command] "<command text>"
  vm_commands.sh launch_subagent "<prompt>"
  vm_commands.sh interrupt <pgid>
  vm_commands.sh list
  vm_commands.sh view-output <run_id> [stdout|stderr|both]

Environment:
  SSH_TARGET      Remote ssh target (e.g. user@host). Required.
  SSH_PORT        Optional ssh port (defaults to 22).
  SSH_EXTRA_OPTS  Additional ssh options (space-separated).
  SSH_STRICT=1    Keep strict host key checking (default disables).
Flags:
  --wait          Stream command output and wait for completion.
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

    local wait_mode=0
    local -a cmd_parts=()
    local command_started=0

    while [[ $# -gt 0 ]]; do
        case "$1" in
            --wait)
                if (( command_started )); then
                    echo "--wait must appear before the command payload." >&2
                    exit 1
                fi
                wait_mode=1
                shift
                ;;
            --)
                shift
                command_started=1
                while [[ $# -gt 0 ]]; do
                    cmd_parts+=("$1")
                    shift
                done
                break
                ;;
            *)
                command_started=1
                cmd_parts+=("$1")
                shift
                ;;
        esac
    done

    if [[ ${#cmd_parts[@]} -eq 0 ]]; then
        echo "No command provided for send." >&2
        usage
        exit 1
    fi

    local run_id
    run_id="$(date +%s%N)"
    local cmd="${cmd_parts[*]}"

ssh_invoke bash -s -- "$run_id" "$wait_mode" "$cmd" <<'EOF'
set -euo pipefail

RUN_ID="$1"
WAIT_MODE="$2"
shift 2

if [[ $# -eq 0 ]]; then
    echo "No command payload supplied." >&2
    exit 1
fi

CMD="$*"
RUN_DIR="/run/castra-agent/${RUN_ID}"
SESSION="agent-${RUN_ID}"

mkdir -p "$RUN_DIR"
chmod 700 "$RUN_DIR"

printf '%s\n' "$CMD" > "$RUN_DIR/command"
date -Is > "$RUN_DIR/started_at"

cat > "$RUN_DIR/entry.sh" <<'SCRIPT'
#!/usr/bin/env bash
set -euo pipefail

RUN_DIR="$1"
WAIT_MODE="${2:-0}"
CMD_FILE="$RUN_DIR/command"
CMD="$(cat "$CMD_FILE")"

exec </dev/null
mkdir -p "$RUN_DIR"
touch "$RUN_DIR/stdout" "$RUN_DIR/stderr"

echo $$ > "$RUN_DIR/pid"
PGID="$(awk '{print $5}' "/proc/$$/stat")"
echo "$PGID" > "$RUN_DIR/pgid"

trap 'date -Is > "$RUN_DIR/stopped_at"' EXIT

if [[ "$WAIT_MODE" == "1" ]]; then
    set +m
    if command -v stdbuf >/dev/null 2>&1; then
        stdbuf -oL -eL bash -lc "$CMD" \
            > >(tee -a "$RUN_DIR/stdout") \
            2> >(tee -a "$RUN_DIR/stderr" >&2)
    else
        bash -lc "$CMD" \
            > >(tee -a "$RUN_DIR/stdout") \
            2> >(tee -a "$RUN_DIR/stderr" >&2)
    fi
else
    if command -v stdbuf >/dev/null 2>&1; then
        stdbuf -oL -eL bash -lc "$CMD" >>"$RUN_DIR/stdout" 2>>"$RUN_DIR/stderr"
    else
        bash -lc "$CMD" >>"$RUN_DIR/stdout" 2>>"$RUN_DIR/stderr"
    fi
fi
SCRIPT

chmod 500 "$RUN_DIR/entry.sh"

COMMAND_STATUS=0

if [[ "$WAIT_MODE" == "1" ]]; then
    set +e
    "$RUN_DIR/entry.sh" "$RUN_DIR" "$WAIT_MODE"
    COMMAND_STATUS=$?
    set -e

    if [[ ! -s "$RUN_DIR/pgid" ]]; then
        for _ in {1..50}; do
            if [[ -s "$RUN_DIR/pgid" ]]; then
                break
            fi
            sleep 0.1
        done
    fi
else
    if command -v tmux >/dev/null 2>&1; then
        tmux has-session -t "$SESSION" 2>/dev/null && tmux kill-session -t "$SESSION"
        tmux new-session -d -s "$SESSION" -- "$RUN_DIR/entry.sh" "$RUN_DIR" "$WAIT_MODE"

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
        setsid "$RUN_DIR/entry.sh" "$RUN_DIR" "$WAIT_MODE" >/dev/null 2>&1 &
        launcher_pid="$!"
        printf '%s\n' "$launcher_pid" > "$RUN_DIR/launcher_pid"

        for _ in {1..50}; do
            if [[ -s "$RUN_DIR/pgid" ]]; then
                break
            fi
            sleep 0.1
        done
    fi
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
exit "$COMMAND_STATUS"
EOF
}

launch_subagent() {
    if [[ $# -lt 1 ]]; then
        echo "launch_subagent requires a prompt string." >&2
        usage
        exit 1
    fi

    local prompt="$*"
    local escaped_prompt
    printf -v escaped_prompt '%q' "$prompt"

    local remote_cmd="codex exec --json --dangerously-bypass-approvals-and-sandbox"
    remote_cmd+=" ${escaped_prompt}"

    send_command command "$remote_cmd"
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
for dir in /run/castra-agent/*; do
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

view_output() {
    if [[ $# -lt 1 || $# -gt 2 ]]; then
        echo "view-output requires <run_id> and optional stream selector (stdout|stderr|both)." >&2
        usage
        exit 1
    fi

    local run_id="$1"
    local stream="${2:-both}"

    case "$stream" in
        stdout|stderr|both) ;;
        *)
            echo "Invalid stream selector: $stream (expected stdout, stderr, or both)." >&2
            exit 1
            ;;
    esac

    ssh_invoke bash -s -- "$run_id" "$stream" <<'EOF'
set -euo pipefail

RUN_ID="$1"
STREAM="$2"
RUN_DIR="/run/castra-agent/${RUN_ID}"

if [[ ! -d "$RUN_DIR" ]]; then
    echo "Run directory not found for $RUN_ID under /run/castra-agent." >&2
    exit 1
fi

print_file() {
    local label="$1"
    local file="$2"

    if [[ ! -f "$file" ]]; then
        printf '===== %s (missing) =====\n' "$label"
        return
    fi

    if [[ ! -s "$file" ]]; then
        printf '===== %s (empty) =====\n' "$label"
        return
    fi

    printf '===== %s =====\n' "$label"
    cat "$file"
    [[ $(tail -c1 "$file" 2>/dev/null || true) == $'\n' ]] || printf '\n'
}

case "$STREAM" in
    stdout)
        print_file "stdout" "$RUN_DIR/stdout"
        ;;
    stderr)
        print_file "stderr" "$RUN_DIR/stderr"
        ;;
    both)
        print_file "stdout" "$RUN_DIR/stdout"
        print_file "stderr" "$RUN_DIR/stderr"
        ;;
esac
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
    launch_subagent)
        if [[ ${1:-} == "command" ]]; then
            shift
        fi
        launch_subagent "$@"
        ;;
    interrupt)
        interrupt_process "$@"
        ;;
    list)
        list_runs
        ;;
    view-output)
        view_output "$@"
        ;;
    *)
        usage
        exit 1
        ;;
esac
