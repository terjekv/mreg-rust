#!/bin/sh
set -eu

ROOT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
COMPOSE_FILE="$ROOT_DIR/tests/treetop/docker-compose.yml"
export TREETOP_IMAGE="${TREETOP_IMAGE:-ghcr.io/treetop-policy-engine/treetop-rest@sha256:e29baeb5b498f21c72dccd9d08ab43198fa4705cecb40643d0ee97b63cc1c932}"
TREETOP_URL="${MREG_TEST_TREETOP_URL:-http://127.0.0.1:9999}"

cleanup() {
  docker compose -f "$COMPOSE_FILE" down -v >/dev/null 2>&1 || true
}

trap cleanup EXIT INT TERM

docker pull "$TREETOP_IMAGE"
docker compose -f "$COMPOSE_FILE" up -d policy-host
# Treetop fetches policies during startup. Wait for the fixture server before
# starting it so a cold container start cannot defer readiness until a reload.
curl -fsS --retry 30 --retry-delay 1 --retry-all-errors --max-time 5 \
  http://127.0.0.1:18080/mreg.cedar >/dev/null
docker compose -f "$COMPOSE_FILE" up -d --remove-orphans

attempt=0
until curl -fsS "$TREETOP_URL/readyz" >/dev/null 2>&1; do
  attempt=$((attempt + 1))
  if [ "$attempt" -ge 60 ]; then
    echo "treetop-rest did not become healthy in time" >&2
    docker compose -f "$COMPOSE_FILE" logs >&2
    exit 1
  fi
  sleep 1
done

MREG_TEST_TREETOP_URL="$TREETOP_URL" cargo test --locked --test treetop_authz "$@"
