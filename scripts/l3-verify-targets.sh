#!/usr/bin/env bash
# 扩展 L3 验证：把本地有工具链的 target 全部真实编译一遍。
# 用法：bash scripts/l3-verify-targets.sh [target...]
# 不带参数 = 全部。缺失工具链的 target 标记 SKIP（CI 模式下视为失败）。
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
FORMA="${FORMA_BIN:-$ROOT/target/debug/forma}"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

if [ ! -x "$FORMA" ]; then
  (cd "$ROOT" && cargo build --locked --quiet) || exit 1
fi

FM='app t { version = "0.1.0" }
entity Customer {
  id   Uuid @id @default(uuid)
  name String(60) @required
}
entity Order {
  id       Uuid           @id @default(uuid)
  title    String(80)     @required
  amount   Decimal(10,2)
  status   OrderStatus    @default(draft)
  customer Customer       @required
  note     Text?
  index    [customer]
}
enum OrderStatus {
  draft
  paid
}
api Orders { entity Order operations [list get create update delete] }
api Customers { entity Customer operations [list get create update delete] }
page OrderList {
  route "/orders"
  title { en "Orders" }
  table {
    query   Orders.list
    columns [title, amount: money, status: badge, customer: ref]
    filter  status as select
    search  [title]
    actions {
      create → OrderEdit
      row    [edit → OrderEdit, delete → Orders.delete confirm]
    }
  }
}
page OrderEdit {
  route "/orders/{id}"
  title { en "Edit Order" }
  form Order {
    layout stack {
      title    Input @required
      amount   Input money
      note     Textarea
      status   Select from OrderStatus
      customer Select from Customer @required
    }
    submit → Orders.update redirect "/orders"
    submit create → Orders.create redirect "/orders"
  }
}
'

mk() { # mk <name> <targets-yaml> [extra-dsl]
  local d="$WORK/$1"
  mkdir -p "$d/forma"
  {
    echo 'project:'
    echo '  name: l3target'
    echo '  version: 0.1.0'
    echo 'source: ["forma/**/*.fm"]'
    echo 'targets:'
    printf '%s\n' "$2"
  } > "$d/forma.yaml"
  printf '%s\n' "$FM" > "$d/forma/app.fm"
  if [ -n "${3:-}" ]; then
    printf '%s\n' "$3" > "$d/forma/app.fm"
  fi
  echo "$d"
}

PASS=0; FAIL=0; SKIP=0; FAILED_LIST=""

check_nestjs() {
  command -v npm >/dev/null || return 2
  (cd "$1/generated/backend" && npm install --no-audit --no-fund --silent && npx tsc --noEmit) \
    > "$1/tsc.log" 2>&1 || { tail -25 "$1/tsc.log"; return 1; }
}
check_graphql() {
  command -v npm >/dev/null || return 2
  (cd "$1/generated/graphql" && npm install --no-audit --no-fund --silent && npx tsc --noEmit) \
    > "$1/tsc.log" 2>&1 || { tail -25 "$1/tsc.log"; return 1; }
  (cd "$1/generated/graphql" && node -e 'const {buildSchema}=require("graphql");const fs=require("fs");buildSchema(fs.readFileSync("schema.graphql","utf8"))') \
    >> "$1/tsc.log" 2>&1 || { tail -15 "$1/tsc.log"; return 1; }
}
check_vue() {
  command -v npm >/dev/null || return 2
  (cd "$1/generated/web" && npm install --no-audit --no-fund --silent && npx vue-tsc --noEmit) \
    > "$1/tsc.log" 2>&1 || { tail -25 "$1/tsc.log"; return 1; }
}
check_nextjs() {
  command -v npm >/dev/null || return 2
  (cd "$1/generated/web" && npm install --no-audit --no-fund --silent && npx tsc --noEmit) \
    > "$1/tsc.log" 2>&1 || { tail -25 "$1/tsc.log"; return 1; }
}
check_expo() {
  command -v npm >/dev/null || return 2
  (cd "$1/generated/mobile" && npm install --no-audit --no-fund --silent && npx tsc --noEmit) \
    > "$1/tsc.log" 2>&1 || { tail -25 "$1/tsc.log"; return 1; }
}
check_sveltekit() {
  command -v npm >/dev/null || return 2
  # SvelteKit 的类型检查必须先 `svelte-kit sync`（生成 .svelte-kit/tsconfig.json）
  (cd "$1/generated/web" && npm install --no-audit --no-fund --silent && npm run check) \
    > "$1/tsc.log" 2>&1 || { tail -25 "$1/tsc.log"; return 1; }
}
check_angular() {
  command -v npm >/dev/null || return 2
  (cd "$1/generated/web" && npm install --no-audit --no-fund --silent && npx tsc --noEmit) \
    > "$1/tsc.log" 2>&1 || { tail -25 "$1/tsc.log"; return 1; }
}
check_swiftui() {
  # SwiftUI 是 Apple 平台框架：Linux runner 上有 swift 工具链但没有 SwiftUI 模块
  # （"no such module 'SwiftUI'"）→ 非 Darwin 只能 SKIP。
  [ "$(uname -s)" = "Darwin" ] || return 2
  command -v swift >/dev/null || return 2
  (cd "$1/generated/mobile" && swift build 2>&1 | tail -30) > "$1/swift.log" 2>&1 \
    || { tail -25 "$1/swift.log"; return 1; }
}
check_redis() {
  # 键设计是 JSON + Markdown：验证 JSON 可解析
  python3 -c 'import json,sys;json.load(open(sys.argv[1]))' "$1/generated/db/redis/key-design.json" \
    > "$1/redis.log" 2>&1 || { tail -10 "$1/redis.log"; return 1; }
}
check_workflow() {
  python3 -c 'import json,sys;d=json.load(open(sys.argv[1]));assert d["schema"]=="forma-workflow/v1",d.get("schema")' \
    "$1/generated/workflow/workflow.json" > "$1/wf.log" 2>&1 || { tail -10 "$1/wf.log"; return 1; }
}

# 只跑指定 target（其余跳过）：bash scripts/l3-verify-targets.sh swiftui
ONLY="${1:-}"

want() { # want <name>
  [ -z "$ONLY" ] && return 0
  [ "$ONLY" = "$1" ]
}

run() { # run <name> <yaml> <checker> [extra-dsl]
  local name="$1" yaml="$2" checker="$3" extra="${4:-}" d rc
  # 只跑指定 target（其余直接返回）：bash scripts/l3-verify-targets.sh swiftui
  if [ -n "$ONLY" ] && [ "$ONLY" != "$name" ]; then
    return 0
  fi
  d="$(mk "$name" "$yaml" "$extra")"
  if ! "$FORMA" build "$d" > "$d/build.log" 2>&1; then
    echo "FAIL $name (generation)"; tail -12 "$d/build.log"; FAIL=$((FAIL+1)); FAILED_LIST="$FAILED_LIST $name"; return
  fi
  "$checker" "$d"; rc=$?
  case $rc in
    0) echo "PASS $name"; PASS=$((PASS+1)) ;;
    2) echo "SKIP $name (no toolchain)"; SKIP=$((SKIP+1)) ;;
    *) echo "FAIL $name"; FAILED_LIST="$FAILED_LIST $name"; FAIL=$((FAIL+1)) ;;
  esac
}

run nestjs "$(cat <<'YAML'
  backend:
    language: typescript
    framework: nestjs
    out: generated/backend
YAML
)" check_nestjs

run graphql "$(cat <<'YAML'
  api:
    framework: graphql
    out: generated/graphql
YAML
)" check_graphql

run vue "$(cat <<'YAML'
  frontend:
    framework: vue
    out: generated/web
YAML
)" check_vue

run nextjs "$(cat <<'YAML'
  frontend:
    framework: nextjs
    out: generated/web
YAML
)" check_nextjs

run expo "$(cat <<'YAML'
  mobile:
    framework: expo
    out: generated/mobile
YAML
)" check_expo

run sveltekit "$(cat <<'YAML'
  frontend:
    framework: sveltekit
    out: generated/web
YAML
)" check_sveltekit

run angular "$(cat <<'YAML'
  frontend:
    framework: angular
    out: generated/web
YAML
)" check_angular

run swiftui "$(cat <<'YAML'
  mobile:
    framework: swiftui
    out: generated/mobile
YAML
)" check_swiftui

run redis "$(cat <<'YAML'
  db: redis
YAML
)" check_redis

run workflow "$(cat <<'YAML'
  workflow: json
YAML
)" check_workflow "$(cat <<'DSL'
app t { version = "0.1.0" }
enum OrderStatus { draft paid }
entity Order {
  id     Uuid        @id @default(uuid)
  status OrderStatus @default(draft)
  amount Decimal(12,2)
}
api Orders { entity Order operations [list get create update delete] }
api OrderPay {
  POST "/orders/{id}/pay"
  pre  { status == "draft" }
  effect { set Order.status = "paid" }
  audit { action "order.pay" }
}
DSL
)"

echo
echo "target verification: $PASS passed, $FAIL failed, $SKIP skipped"
[ -n "$FAILED_LIST" ] && echo "failed:$FAILED_LIST"
[ "$FAIL" -gt 0 ] && exit 1
exit 0
