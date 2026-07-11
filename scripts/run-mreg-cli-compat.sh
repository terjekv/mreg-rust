#!/usr/bin/env bash
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
MREG_CLI_COMMIT=${MREG_CLI_COMMIT:-72e598d3602812fc61a2d3a248ac8f4385dfb118}
MREG_CLI_DIR=${MREG_CLI_DIR:-${TMPDIR:-/tmp}/mreg-cli-compat-$MREG_CLI_COMMIT}
SERVER_LOG=${MREG_CLI_SERVER_LOG:-${TMPDIR:-/tmp}/mreg-rust-compat.log}
RESULT_LOG=${MREG_CLI_RESULT_LOG:-${TMPDIR:-/tmp}/mreg-cli-compat-result.json}
CONTAINER=mreg-cli-compat-$$
SERVER_PID=

cleanup() {
    docker rm --force "$CONTAINER" >/dev/null 2>&1 || true
    if [[ -n "$SERVER_PID" ]]; then
        kill "$SERVER_PID" >/dev/null 2>&1 || true
    fi
}
trap cleanup EXIT

if [[ ! -d "$MREG_CLI_DIR/.git" ]]; then
    git clone https://github.com/unioslo/mreg-cli.git "$MREG_CLI_DIR"
fi
git -C "$MREG_CLI_DIR" checkout --detach "$MREG_CLI_COMMIT"

cd "$ROOT"
cargo build --release
env \
    MREG_LISTEN=127.0.0.1 \
    MREG_PORT=8000 \
    MREG_STORAGE_BACKEND=memory \
    MREG_RUN_MIGRATIONS=false \
    MREG_AUTH_MODE=none \
    MREG_ALLOW_DEV_AUTHZ_BYPASS=true \
    ./target/release/mreg-rust >"$SERVER_LOG" 2>&1 &
SERVER_PID=$!

for _ in {1..30}; do
    if curl --fail --silent http://127.0.0.1:8000/api/meta/health/heartbeat >/dev/null; then
        break
    fi
    sleep 1
done
if ! curl --fail --silent http://127.0.0.1:8000/api/meta/health/heartbeat >/dev/null; then
    cat "$SERVER_LOG"
    exit 1
fi

docker build -f "$MREG_CLI_DIR/ci/Dockerfile" -t mreg-cli-compat "$MREG_CLI_DIR"
if [[ $(uname -s) == Darwin ]]; then
    SERVER_URL=http://host.docker.internal:8000
    NETWORK_ARG=
else
    SERVER_URL=http://127.0.0.1:8000
    NETWORK_ARG=host
fi

RUN_ARGS=(--name "$CONTAINER" --tty --entrypoint bash)
if [[ -n "$NETWORK_ARG" ]]; then
    RUN_ARGS+=(--network "$NETWORK_ARG")
fi
docker run "${RUN_ARGS[@]}" mreg-cli-compat \
    -c "cd /build/ci; /root/.local/bin/uv run bash -c 'echo test | mreg-cli -u test -d example.org --url $SERVER_URL --source testsuite --record new_testsuite_log.json --record-without-timestamps -v ERROR >/dev/null'"
docker cp "$CONTAINER:/build/ci/new_testsuite_log.json" "$RESULT_LOG"

python3 "$ROOT/scripts/check-mreg-cli-compat.py" \
    "$MREG_CLI_DIR/ci/testsuite-result.json" \
    "$RESULT_LOG"
