#!/bin/sh
# Headless container entrypoint for the AuraRafi player.
#
# Responsibility boundary:
#   - resolve one runtime manifest from RAFI_MANIFEST, an explicit
#     --manifest argument, or the standard /session/project locations;
#   - reject missing, unreadable or oversized inputs before the player starts;
#   - exec the player so tini can forward SIGTERM to a single process.
#
# The player owns protocol, token validation, frame limits and output writes.
# This script never logs credentials and never writes inside the project mount.

set -eu

PLAYER=${RAFI_PLAYER_BIN:-/usr/local/bin/aurarafi-player-bin}
PROJECT_DIR=${RAFI_PROJECT_DIR:-/session/project}
OUT_DIR=${RAFI_OUT_DIR:-/session/out}
MANIFEST=${RAFI_MANIFEST:-}
MAX_FRAMES=${RAFI_MAX_FRAMES:-0}
IDLE_TIMEOUT=${RAFI_IDLE_TIMEOUT_SECONDS:-60}
MANIFEST_LIMIT_BYTES=67108864
MAX_ARGS=32
MAX_ARG_BYTES=8192
MAX_FRAMES_LIMIT=10000000
IDLE_TIMEOUT_LIMIT=86400
EX_USAGE=64
EX_NOINPUT=66
EX_UNAVAILABLE=69
EX_CANTCREAT=73

log() {
    printf 'rafi-entrypoint: %s\n' "$*" >&2
}

# exec replaces this shell, so the player is the direct child of tini and
# receives SIGTERM without an intermediate shell. This trap covers the short
# validation window before exec.
trap 'exit 143' TERM INT HUP

is_uint() {
    case $1 in
        '' | *[!0-9]*) return 1 ;;
        *) return 0 ;;
    esac
}

if ! is_uint "$MAX_FRAMES" || [ "${#MAX_FRAMES}" -gt 8 ]; then
    log "RAFI_MAX_FRAMES must be a non-negative integer no larger than $MAX_FRAMES_LIMIT"
    exit "$EX_USAGE"
fi
if [ "$MAX_FRAMES" -gt "$MAX_FRAMES_LIMIT" ]; then
    log "RAFI_MAX_FRAMES exceeds the supported limit of $MAX_FRAMES_LIMIT"
    exit "$EX_USAGE"
fi
if ! is_uint "$IDLE_TIMEOUT" || [ "${#IDLE_TIMEOUT}" -gt 5 ]; then
    log "RAFI_IDLE_TIMEOUT_SECONDS must be a non-negative integer no larger than $IDLE_TIMEOUT_LIMIT"
    exit "$EX_USAGE"
fi
if [ "$IDLE_TIMEOUT" -gt "$IDLE_TIMEOUT_LIMIT" ]; then
    log "RAFI_IDLE_TIMEOUT_SECONDS exceeds the supported limit of $IDLE_TIMEOUT_LIMIT"
    exit "$EX_USAGE"
fi

wants_help=0
has_headless=0
has_manifest=0
manifest_arg=
previous=
for arg in "$@"; do
    case $arg in
        --help | -h | --version | -V) wants_help=1 ;;
        --headless | --headless=*) has_headless=1 ;;
        --manifest) has_manifest=1 ;;
        --manifest=*)
            has_manifest=1
            manifest_arg=${arg#--manifest=}
            ;;
    esac
    if [ "$previous" = "--manifest" ] && [ -n "$arg" ]; then
        manifest_arg=$arg
    fi
    previous=$arg
done

# Help and version are allowed without a project so operators can inspect the
# image before mounting a session. Validate the binary first so a broken image
# fails with a clear message instead of a shell exec error.
if [ ! -x "$PLAYER" ]; then
    log "player executable is missing or not executable: $PLAYER"
    exit "$EX_UNAVAILABLE"
fi
if [ "$wants_help" -eq 1 ]; then
    exec "$PLAYER" "$@"
fi

if [ "$#" -gt "$MAX_ARGS" ]; then
    log "too many arguments ($#); the supported maximum is $MAX_ARGS"
    exit "$EX_USAGE"
fi
arg_bytes=0
for arg in "$@"; do
    arg_bytes=$((arg_bytes + ${#arg}))
done
if [ "$arg_bytes" -gt "$MAX_ARG_BYTES" ]; then
    log "argument payload is $arg_bytes bytes; the supported maximum is $MAX_ARG_BYTES"
    exit "$EX_USAGE"
fi

if [ "$has_manifest" -eq 1 ] && [ -z "$manifest_arg" ]; then
    log "--manifest requires a path"
    exit "$EX_USAGE"
fi

# Discovery is deterministic: explicit argument, explicit environment, then the
# two documented project layouts.
if [ -n "$manifest_arg" ]; then
    MANIFEST=$manifest_arg
elif [ -n "$MANIFEST" ]; then
    :
elif [ -f "$PROJECT_DIR/runtime.ron" ]; then
    MANIFEST=$PROJECT_DIR/runtime.ron
elif [ -f "$PROJECT_DIR/.aura_rafi/runtime.ron" ]; then
    MANIFEST=$PROJECT_DIR/.aura_rafi/runtime.ron
else
    log "no runtime manifest found"
    log "set RAFI_MANIFEST, pass --manifest PATH, or place runtime.ron in $PROJECT_DIR"
    exit "$EX_NOINPUT"
fi

if [ ! -f "$MANIFEST" ]; then
    log "runtime manifest is not a regular file: $MANIFEST"
    exit "$EX_NOINPUT"
fi
if [ ! -r "$MANIFEST" ]; then
    log "runtime manifest is not readable: $MANIFEST"
    exit "$EX_NOINPUT"
fi
manifest_bytes=$(wc -c < "$MANIFEST" | tr -d '[:space:]')
if ! is_uint "$manifest_bytes"; then
    log "could not determine the size of the runtime manifest: $MANIFEST"
    exit "$EX_NOINPUT"
fi
if [ "$manifest_bytes" -gt "$MANIFEST_LIMIT_BYTES" ]; then
    log "runtime manifest is $manifest_bytes bytes; the supported maximum is $MANIFEST_LIMIT_BYTES"
    exit "$EX_USAGE"
fi

if [ ! -d "$OUT_DIR" ]; then
    log "output directory is missing: $OUT_DIR"
    exit "$EX_NOINPUT"
fi
if [ ! -w "$OUT_DIR" ]; then
    log "output directory is not writable: $OUT_DIR"
    exit "$EX_CANTCREAT"
fi

if [ "$has_headless" -eq 0 ]; then
    set -- --headless "$@"
fi
if [ "$has_manifest" -eq 0 ]; then
    set -- "$@" --manifest "$MANIFEST"
fi

log "starting headless player with manifest $MANIFEST"
exec "$PLAYER" "$@"
