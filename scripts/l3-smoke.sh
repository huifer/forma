#!/usr/bin/env bash
# L3 artifact verification for the opt-in (track-B / M10) targets.
#
# Why: CI's `generated-backend` / `web` / `flutter` jobs only compile the DEFAULT stack.
# The extra targets were verified by structural assertions only. This script generates one
# scratch project per target and runs the target's real toolchain, so "emitted" also means
# "compiles / parses". It is deliberately explicit about what is NOT verifiable here
# (missing toolchains, live DB servers) instead of silently skipping.
#
# Usage: bash scripts/l3-smoke.sh [--keep]
#   FORMA_BIN   override the compiler binary (default: target/debug/forma)
#   GITHUB_ACTIONS=1  any SKIP is a hard failure (CI must have all toolchains)
#   --keep      keep scratch projects for inspection (default: delete on exit)

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
FORMA="${FORMA_BIN:-$ROOT/target/debug/forma}"
KEEP=0
for arg in "$@"; do [ "$arg" = "--keep" ] && KEEP=1; done

WORK="$(mktemp -d)"
if [ "$KEEP" -eq 0 ]; then trap 'rm -rf "$WORK"' EXIT; fi

if [ ! -x "$FORMA" ]; then
  echo "building compiler (cargo build --locked)…"
  (cd "$ROOT" && cargo build --locked --quiet) || exit 1
fi

read -r -d '' FM <<'DSL' || true
app l3smoke { version = "0.1.0" }
entity Order {
  id Uuid @id @default(uuid)
  title String(80) @required
  amount Decimal(10,2)
}
api Orders { entity Order operations [list get create update delete] }
page OrderList { title { en "Orders" } table { query Orders.list columns [title] } }
DSL

PASS=0
FAIL=0
SKIP=0

mkcase() {
  local name="$1" targets="$2" extra="${3:-}" d="$WORK/$1"
  mkdir -p "$d/forma"
  {
    echo 'project:'
    echo '  name: l3smoke'
    echo '  version: 0.1.0'
    echo 'source: ["forma/**/*.fm"]'
    echo 'targets:'
    printf '%s\n' "$targets"
  } > "$d/forma.yaml"
  if [ -n "$extra" ]; then
    printf '%s\n' "$extra" > "$d/forma/app.fm"
  else
    printf '%s\n' "$FM" > "$d/forma/app.fm"
  fi
  if ! "$FORMA" build "$d" > "$d/build.log" 2>&1; then
    echo "  forma build failed:"; sed -n '1,25p' "$d/build.log"
    return 1
  fi
}

need() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "  tool missing: $1"
    return 1
  fi
}

run_case() {
  local name="$1" targets="$2" checker="$3" extra="${4:-}" d="$WORK/$1" rc
  if ! mkcase "$name" "$targets" "$extra"; then
    echo "FAIL $name (generation)"; FAIL=$((FAIL + 1)); return
  fi
  "$checker" "$d" "$name"
  rc=$?
  case $rc in
    0) echo "PASS $name"; PASS=$((PASS + 1)) ;;
    2) echo "SKIP $name"; SKIP=$((SKIP + 1)) ;;
    *) echo "FAIL $name"; FAIL=$((FAIL + 1)) ;;
  esac
}

check_go() {
  need go || return 2
  (cd "$1/generated/backend" && go mod tidy >/dev/null 2>&1 && go build ./...) \
    > "$WORK/$2.log" 2>&1 || { sed -n '1,20p' "$WORK/$2.log"; return 1; }
}

check_php() {
  need php || return 2
  local f
  while IFS= read -r -d '' f; do
    php -l "$f" > "$WORK/$2.log" 2>&1 || { sed -n '1,20p' "$WORK/$2.log"; return 1; }
  done < <(find "$1/generated/backend" -name '*.php' -print0)
}

check_grpc() {
  need protoc || return 2
  protoc --proto_path="$1/generated/grpc" \
    --descriptor_set_out="$WORK/$2.pb" "$1/generated/grpc"/*.proto \
    > "$WORK/$2.log" 2>&1 || { sed -n '1,20p' "$WORK/$2.log"; return 1; }
}

check_ts() {
  need npm || return 2
  (cd "$1" && npm install --no-audit --no-fund --silent && npx tsc --noEmit) \
    > "$WORK/$2.log" 2>&1 || { sed -n '1,25p' "$WORK/$2.log"; return 1; }
}

check_python() {
  need python3 || return 2
  python3 -m compileall -q "$1/generated/backend" \
    > "$WORK/$2.log" 2>&1 || { sed -n '1,20p' "$WORK/$2.log"; return 1; }
}

check_mongodb_json() {
  need python3 || return 2
  python3 -c 'import json,sys; json.load(open(sys.argv[1]))' \
    "$1/generated/db/collections.json" > "$WORK/$2.log" 2>&1 \
    || { sed -n '1,20p' "$WORK/$2.log"; return 1; }
}

run_case go "$(cat <<'YAML'
  backend:
    language: go
    framework: gin
    out: generated/backend
YAML
)" check_go

run_case php "$(cat <<'YAML'
  backend:
    language: php
    framework: laravel
    out: generated/backend
YAML
)" check_php

run_case fastapi "$(cat <<'YAML'
  backend:
    language: python
    framework: fastapi
    out: generated/backend
YAML
)" check_python

run_case grpc "$(cat <<'YAML'
  api:
    framework: grpc
    out: generated/grpc
YAML
)" check_grpc

# trpc / playwright 用同一个 TS 检查器，但输出目录不同：包一层薄函数。
check_ts_trpc() { check_ts "$1/generated/api" "$2"; }
check_ts_playwright() { check_ts "$1/generated/web/e2e" "$2"; }
run_case trpc "$(cat <<'YAML'
  api:
    framework: trpc
    out: generated/api
YAML
)" check_ts_trpc

run_case playwright "$(cat <<'YAML'
  web:
    framework: react
    e2e: playwright
    out: generated/web
YAML
)" check_ts_playwright

run_case mongodb "$(cat <<'YAML'
  db:
    engine: mongodb
    out: generated/db
YAML
)" check_mongodb_json

# live SSE 通道：结构断言曾漏掉生成 Rust 的括号语法错误（filter_map 多一个 }），
# 只有真实 cargo check 能抓。
check_live_backend() {
  command -v cargo >/dev/null || return 2
  (cd "$1/generated/backend" && cargo check) > "$1/cargo.log" 2>&1 \
    || { tail -25 "$1/cargo.log"; return 1; }
}
run_case live-backend "$(cat <<'YAML'
  backend:
    language: rust
    framework: axum
    database: sqlx
    out: generated/backend
YAML
)" check_live_backend "$(cat <<'DSL'
app l3smoke { version = "0.1.0" }
entity Order {
  id Uuid @id @default(uuid)
  title String(80) @required
  active Bool @default(true)
}
api Orders { entity Order operations [list get create update delete] }
live OrderFeed {
  api Orders.list
  events [create update delete]
}
DSL
)"

echo
echo "Explicitly NOT verifiable in this smoke run (structural assertions in tests/ only):"
echo "  - Spring / .NET / SwiftUI / Compose / 小程序 / uni-app / SvelteKit / Angular / Electron / Tauri:"
echo "    no Maven / dotnet / Swift-on-Linux / Gradle / framework CLI in the CI runner; several are"
echo "    documented minimal skeletons by design."
echo "  - SQL Server / Oracle DDL: requires a live server; replay simulator covers MySQL chain only."
echo "  - Stripe / workflow / desktop wrappers: config & JSON specs, no runtime service to call."
echo
echo "L3 smoke: $PASS passed, $FAIL failed, $SKIP skipped (scratch: $WORK)"

if [ "$FAIL" -gt 0 ]; then exit 1; fi
if [ "${GITHUB_ACTIONS:-}" != "" ] && [ "$SKIP" -gt 0 ]; then
  echo "CI requires every toolchain: $SKIP case(s) were skipped."
  exit 1
fi
exit 0
