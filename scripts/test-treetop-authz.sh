#!/bin/sh
set -eu

ROOT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
COMPOSE_FILE="$ROOT_DIR/tests/treetop/docker-compose.yml"
TREETOP_IMAGE="${TREETOP_IMAGE:-ghcr.io/terjekv/treetop-rest:latest}"
TREETOP_URL="${MREG_TEST_TREETOP_URL:-http://127.0.0.1:9999}"

cleanup() {
  docker compose -f "$COMPOSE_FILE" down -v >/dev/null 2>&1 || true
}

trap cleanup EXIT INT TERM

docker pull "$TREETOP_IMAGE"
docker compose -f "$COMPOSE_FILE" up -d --remove-orphans

attempt=0
until curl -fsS "$TREETOP_URL/api/v1/health" >/dev/null 2>&1; do
  attempt=$((attempt + 1))
  if [ "$attempt" -ge 30 ]; then
    echo "treetop-rest did not become healthy in time" >&2
    exit 1
  fi
  sleep 1
done

MREG_TEST_TREETOP_URL="$TREETOP_URL" cargo test --test treetop_authz
