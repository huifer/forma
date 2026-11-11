#!/usr/bin/env bash
# 生产全流程冒烟：DSL → 构建 → 真 MySQL 迁移执行 → 容器部署 → 业务 API 断言。
#
# 为什么需要它（2026-11-11）：结构断言与 cargo test 覆盖不到「产物在真库里能不能跑」。
# 一次手工生产实测抓出 6 个真实缺陷（迁移废迁移、SSE 500、Bool default 编译失败、
# 定时任务不执行、partial update 写 NULL、容器迁移不可重入）。本脚本把它们变成常驻门禁。
#
# 用法：bash scripts/prod-smoke.sh
# 依赖：docker（含 mysql:8 镜像）、curl、python3、cargo（构建编译器）
# 退出码：0 = 全部断言通过；1 = 有断言失败

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
FORMA="${FORMA_BIN:-$ROOT/target/debug/forma}"
WORK="$(mktemp -d)"
SUFFIX="$$-$(date +%s)"
NET="forma-prodsmoke-$SUFFIX"
DB_CONTAINER="forma-prodsmoke-db-$SUFFIX"
APP_CONTAINER="forma-prodsmoke-app-$SUFFIX"
DB_PASS="forma-smoke-db-pass"
JWT="forma-smoke-jwt-secret-0123456789abcdef"
DB_NAME="shop"
# 宿主机访问 MySQL 的端口（避免与本机 3306/3307 冲突）
DB_PORT="$(python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1]);s.close()')"
APP_PORT="$(python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1]);s.close()')"

PASS=0
FAIL=0
FAILED_ASSERTIONS=""

cleanup() {
  docker rm -f "$APP_CONTAINER" >/dev/null 2>&1
  docker rm -f "$DB_CONTAINER" >/dev/null 2>&1
  docker network rm "$NET" >/dev/null 2>&1
  rm -rf "$WORK"
}
trap cleanup EXIT

ok()   { PASS=$((PASS+1)); printf '  ✓ %s\n' "$1"; }
bad()  { FAIL=$((FAIL+1)); FAILED_ASSERTIONS="$FAILED_ASSERTIONS\n    - $1"; printf '  ✗ %s\n     %s\n' "$1" "${2:-}"; }
assert_eq() { # assert_eq <label> <expected> <actual>
  if [ "$2" = "$3" ]; then ok "$1"; else bad "$1" "expected=$2 actual=$3"; fi
}
assert_contains() { # assert_contains <label> <needle> <haystack>
  case "$3" in *"$2"*) ok "$1" ;; *) bad "$1" "missing '$2' in: ${3:0:300}" ;; esac
}
assert_not_contains() {
  case "$3" in *"$2"*) bad "$1" "unexpected '$2' in: ${3:0:300}" ;; *) ok "$1" ;; esac
}

if [ ! -x "$FORMA" ]; then
  echo "building compiler…"
  (cd "$ROOT" && cargo build --locked --quiet) || exit 1
fi

# ── 1. 生成生产测试项目 ─────────────────────────────────────────────
PROJ="$WORK/shop"
mkdir -p "$PROJ/forma"
cat > "$PROJ/forma.yaml" <<'YAML'
project:
  name: shopProd
  version: 1.0.0

source: ["forma/**/*.fm"]

targets:
  db:
    engine: mysql
    out: db
  backend:
    language: rust
    framework: axum
    database: sqlx
    out: generated/backend
  frontend:
    framework: react
    out: generated/web

deployment:
  provider: all
  replicas: 2
  image: registry.example.test/shop-backend:1.0.0
  host: api.shop.example.test
  namespace: shop
YAML

cat > "$PROJ/forma/app.fm" <<'FM'
app shopProd {
  version = "1.0.0"
  locale  = ["zh", "en"]
}

auth module {
  provider jwt
  roles [admin, operator, customer]
  tokenHours 12
}

enum OrderStatus {
  pending   @label { zh "待支付" en "Pending" }
  paid      @label { zh "已支付" en "Paid" }
  shipped   @label { zh "已发货" en "Shipped" }
  cancelled @label { zh "已取消" en "Cancelled" }
}

entity Customer {
  id        Uuid      @id @default(uuid)
  email     Email     @required @unique
  name      String(80)  @required
  phone     String(20)?
  createdAt Timestamp @default(now) @readonly
}

entity Product {
  id        Uuid          @id @default(uuid)
  sku       String(40)    @required @unique
  title     String(160)   @required
  price     Decimal(10,2) @required @min(0)
  stock     Int           @required @min(0)
  active    Bool          @default(true)
  createdAt Timestamp     @default(now) @readonly

  index [active]
  searchable [title]
}

entity Order {
  id        Uuid          @id @default(uuid)
  customer  Customer      @required
  status    OrderStatus   @default(pending)
  total     Decimal(12,2) @required
  notifyTo  Email?        @notify(log, "audit", OrderPay)
  note      Text?
  createdAt Timestamp     @default(now) @readonly

  index [customer]
  index [status]
  searchable [note]
}

rules OrderRules {
  guard Order.delete {
    deny status == "paid"
      reason { zh "已支付订单不可删除" en "Paid orders cannot be deleted" }
  }
}

api Customers {
  entity Customer
  operations [list get create update delete]
  list {
    search  [email, name]
    paginate offset
  }
  auth {
    delete → role in ["admin"]
  }
}

api Products {
  entity Product
  operations [list get create update delete]
  list {
    filters [active]
    search  [title]
    paginate offset
  }
  auth {
    delete → role in ["admin"]
  }
}

api Orders {
  entity Order
  operations [list get create update delete]
  list {
    filters [customer, status]
    search  [note]
    sort    [createdAt desc]
    paginate offset
  }
  auth {
    update → role in ["admin", "operator"]
    delete → role in ["admin"]
  }
}

api OrderPay {
  POST "/orders/{id}/pay"
  label { zh "支付" en "Pay" }
  pre  { status == "pending" }
  message { zh "只有待支付订单可以支付" en "Only pending orders can be paid" }
  effect {
    set Order.status = paid
  }
  audit { action "order.pay" }
}

api OrderShip {
  POST "/orders/{id}/ship"
  label { zh "发货" en "Ship" }
  pre  { status == "paid" }
  message { zh "只有已支付订单可以发货" en "Only paid orders can be shipped" }
  effect {
    set Order.status = shipped
  }
  audit { action "order.ship" }
}

job nightlyCancel {
  cron "0 3 * * *"
  label { zh "夜间清理超时未支付订单" en "Nightly stale-order cleanup" }
  op OrderPay
}

live OrderFeed {
  api Orders.list
  events [create update delete]
}

page OrderList {
  route "/orders"
  title { zh "订单管理" en "Orders" }
  table {
    query   Orders.list
    columns [status: badge, total: money, createdAt: datetime]
    filter  status as select
    search  [note]
    actions {
      create → OrderEdit
      row    [edit → OrderEdit, delete → Orders.delete confirm]
    }
  }
}

page OrderEdit {
  route "/orders/{id}"
  title { zh "编辑订单" en "Edit Order" }
  form Order {
    layout stack {
      status   Select from OrderStatus
      customer Select from Customer @required
      total    Input money
      note     Textarea
    }
    submit → Orders.update redirect "/orders"
    submit create → Orders.create redirect "/orders"
  }
}

chart OrdersByStatus {
  type  bar
  title { zh "订单状态分布" en "Orders by status" }

  source Orders.list

  encoding {
    x { field status type nominal }
    y { field value  type quantitative format integer }
  }

  transform {
    aggregate { op count groupBy [status] }
  }

  axis {
    x { label "状态" }
    y { label "订单数" format integer }
  }

  tooltip { enabled true }
  legend  { enabled false }
}
FM

echo "== 1. 构建（DSL → 产物） =="
if ! "$FORMA" build "$PROJ" > "$WORK/build.log" 2>&1; then
  bad "forma build" "$(tail -5 "$WORK/build.log")"
  echo "prod-smoke: $PASS passed, $FAIL failed"
  exit 1
fi
ok "forma build"
if [ -f "$PROJ/db/migrations/0002_upgrade.sql" ]; then
  bad "首次构建不得产生伪迁移" "$(cat "$PROJ/db/migrations/0002_upgrade.sql")"
else
  ok "首次构建只有基线迁移（无伪迁移）"
fi
if [ -f "$PROJ/k8s/deployment.yaml" ] && [ -f "$PROJ/k8s/service.yaml" ] \
   && [ -f "$PROJ/terraform/main.tf" ]; then
  ok "部署清单（k8s + terraform）已生成"
else
  bad "部署清单缺失" "$(find "$PROJ" -maxdepth 2 -type f -name '*.yaml' -o -name '*.tf' 2>/dev/null | head -10)"
fi
if ! "$FORMA" migrate "$PROJ" verify > "$WORK/verify.log" 2>&1; then
  bad "迁移链校验" "$(tail -3 "$WORK/verify.log")"
else
  ok "迁移链校验通过"
fi

echo "== 2. 真实 MySQL + 容器部署 =="
docker network create "$NET" >/dev/null
docker run -d --name "$DB_CONTAINER" --network "$NET" --network-alias db \
  -p "127.0.0.1:$DB_PORT:3306" \
  -e MYSQL_ROOT_PASSWORD="$DB_PASS" -e MYSQL_DATABASE="$DB_NAME" mysql:8.0 >/dev/null
for _ in $(seq 1 90); do
  docker exec "$DB_CONTAINER" mysqladmin ping -uroot -p"$DB_PASS" --silent >/dev/null 2>&1 && break
  sleep 2
done
if ! docker exec "$DB_CONTAINER" mysqladmin ping -uroot -p"$DB_PASS" --silent >/dev/null 2>&1; then
  bad "MySQL 未就绪" "container=$DB_CONTAINER"
  echo "prod-smoke: $PASS passed, $FAIL failed"
  exit 1
fi
ok "MySQL 8 就绪"

DB_URL="mysql://root:${DB_PASS}@127.0.0.1:${DB_PORT}/${DB_NAME}"

# 等待宿主机端口转发可用（容器 healthcheck 通过 != 端口映射已就绪）
PORT_OK=""
for _ in $(seq 1 30); do
  PORT_OK="$(python3 -c "import socket;s=socket.socket();r=s.connect_ex(('127.0.0.1',${DB_PORT}));s.close();print(1 if r==0 else 0)" 2>/dev/null)"
  [ "$PORT_OK" = "1" ] && break
  sleep 1
done
assert_eq "宿主机 MySQL 端口可达" "1" "$PORT_OK"

# CLI 迁移真执行（生产路径之一）
if ! DATABASE_URL="$DB_URL" "$FORMA" migrate "$PROJ" up --execute > "$WORK/migrate.log" 2>&1; then
  bad "forma migrate up --execute" "$(tail -5 "$WORK/migrate.log")"
else
  ok "forma migrate up --execute（真执行）"
fi
assert_contains "迁移已落库（表存在）" "orders" \
  "$(docker exec "$DB_CONTAINER" mysql -uroot -p"$DB_PASS" -N -B "$DB_NAME" -e 'SHOW TABLES' 2>/dev/null | tr '\n' ' ')"

# 容器化部署（entrypoint 迁移路径）
echo "== 3. Docker 镜像与容器 =="
IMAGE="${PROD_SMOKE_IMAGE:-forma-prodsmoke:$SUFFIX}"
if [ -n "${PROD_SMOKE_IMAGE:-}" ]; then
  ok "复用已构建镜像 ${IMAGE}（跳过 docker build）"
else
  if ! (cd "$PROJ" && docker build --progress=plain -f generated/backend/Dockerfile -t "$IMAGE" . >"$WORK/docker.log" 2>&1); then
    bad "docker build" "$(tail -12 "$WORK/docker.log")"
    echo "prod-smoke: $PASS passed, $FAIL failed"
    exit 1
  fi
  ok "docker build"
fi
RUN_UID="$(docker run --rm --entrypoint id "$IMAGE" -u 2>/dev/null | tr -d '[:space:]')"
assert_eq "镜像以非 root 运行（uid 10001）" "10001" "$RUN_UID"
IMAGE_ENV="$(docker inspect "$IMAGE" --format '{{json .Config.Env}}' 2>/dev/null)"
assert_not_contains "镜像不内嵌数据库凭据" "DATABASE_URL=mysql" "$IMAGE_ENV"

docker run -d --name "$APP_CONTAINER" --network "$NET" -p "127.0.0.1:$APP_PORT:8080" \
  -e DB_HOST=db -e DB_USER=root -e DB_PASS="$DB_PASS" -e DB_NAME="$DB_NAME" \
  -e "DATABASE_URL=mysql://root:${DB_PASS}@db:3306/${DB_NAME}" \
  -e "JWT_SECRET=$JWT" -e CORS_ORIGINS="https://shop.example.test" -e PORT=8080 \
  "$IMAGE" >/dev/null

BASE="http://127.0.0.1:${APP_PORT}"
B="$BASE/api"
HEALTH=""
for _ in $(seq 1 60); do
  HEALTH="$(curl -s --max-time 2 "$BASE/health" 2>/dev/null)"
  [ -n "$HEALTH" ] && break
  sleep 1
done
assert_eq "GET /health" '{"status":"ok"}' "$HEALTH"
READY="$(curl -s --max-time 2 "$BASE/ready" 2>/dev/null)"
assert_eq "GET /ready" '{"ready":true}' "$READY"

LOGS="$(docker logs "$APP_CONTAINER" 2>&1)"
assert_contains "容器启动日志含服务就绪" "backend listening" "$LOGS"

echo "== 4. 业务与安全行为 =="
H='Content-Type: application/json'

# 4.1 认证与角色（服务端分配）
REG1="$(curl -s -X POST "$B/auth/register" -H "$H" -d '{"username":"boss","password":"BossPass123"}')"
ROLE1="$(printf '%s' "$REG1" | python3 -c 'import sys,json;d=json.load(sys.stdin);print(d.get("role",""))')"
assert_eq "首个用户 = 引导角色(admin)" "admin" "$ROLE1"
ADMIN="$(printf '%s' "$REG1" | python3 -c 'import sys,json;print(json.load(sys.stdin)["token"])')"
REG2="$(curl -s -X POST "$B/auth/register" -H "$H" -d '{"username":"bob","password":"BobPass123","role":"admin"}')"
ROLE2="$(printf '%s' "$REG2" | python3 -c 'import sys,json;print(json.load(sys.stdin)["role"])')"
assert_eq "后续用户 = 最低权限角色（客户端 role 被忽略）" "customer" "$ROLE2"
CUST="$(curl -s -X POST "$B/auth/login" -H "$H" -d '{"username":"bob","password":"BobPass123"}' | python3 -c 'import sys,json;print(json.load(sys.stdin)["token"])')"
A="Authorization: Bearer $ADMIN"
C="Authorization: Bearer $CUST"

# 4.2 未认证 → 401
assert_eq "无 token 访问受保护资源 → 401" "401" \
  "$(curl -s -o /dev/null -w '%{http_code}' "$B/orders")"

# 4.3 业务写入 + Bool 默认值
curl -s -o /dev/null -X POST "$B/customers" -H "$A" -H "$H" -d '{"email":"c@example.test","name":"Customer"}'
curl -s -o /dev/null -X POST "$B/products" -H "$A" -H "$H" -d '{"sku":"SKU-1","title":"机械键盘","price":"399.00","stock":5}'
ACTIVE="$(curl -s "$B/products" -H "$A" | python3 -c 'import sys,json;print(json.load(sys.stdin)[0]["active"])')"
assert_eq "Bool @default(true) 生效" "True" "$ACTIVE"
CID="$(curl -s "$B/customers?q=c@" -H "$A" | python3 -c 'import sys,json;print(json.load(sys.stdin)[0]["id"])')"
printf '{"customer":"%s","total":"399.00","note":"中文备注"}' "$CID" > "$WORK/order.json"
curl -s -o /dev/null -X POST "$B/orders" -H "$A" -H "$H" --data-binary "@$WORK/order.json"
OID="$(curl -s "$B/orders?q=中文" -H "$A" | python3 -c 'import sys,json;print(json.load(sys.stdin)[0]["id"])')"

# 4.4 全文搜索（中文）
HITS="$(curl -s "$B/products?q=机械" -H "$A" | python3 -c 'import sys,json;print(len(json.load(sys.stdin)))')"
assert_eq "FULLTEXT 中文搜索命中" "1" "$HITS"

# 4.5 命令状态机 + 守卫
assert_eq "命令 pay → 200" "200" \
  "$(curl -s -o /dev/null -w '%{http_code}' -X POST "$B/orders/$OID/pay" -H "$A" -H "$H" -d '{}')"
assert_eq "重复 pay（pre 不满足）→ 409" "409" \
  "$(curl -s -o /dev/null -w '%{http_code}' -X POST "$B/orders/$OID/pay" -H "$A" -H "$H" -d '{}')"
assert_eq "删除已支付订单（guard deny）→ 409" "409" \
  "$(curl -s -o /dev/null -w '%{http_code}' -X DELETE "$B/orders/$OID" -H "$A")"
assert_eq "命令 ship → 200" "200" \
  "$(curl -s -o /dev/null -w '%{http_code}' -X POST "$B/orders/$OID/ship" -H "$A" -H "$H" -d '{}')"

# 4.6 授权（服务端权威）
assert_eq "customer 角色更新订单 → 403" "403" \
  "$(printf '{"customer":"%s","total":"1.00","status":"pending"}' "$CID" > "$WORK/put-cust.json"; curl -s -o /dev/null -w '%{http_code}' -X PUT "$B/orders/$OID" -H "$C" -H "$H" --data-binary "@$WORK/put-cust.json")"
assert_eq "admin 更新订单 → 200" "200" \
  "$(printf '{"customer":"%s","total":"399.00"}' "$CID" > "$WORK/put-admin.json"; curl -s -o /dev/null -w '%{http_code}' -X PUT "$B/orders/$OID" -H "$A" -H "$H" --data-binary "@$WORK/put-admin.json")"
STATUS_AFTER="$(curl -s "$B/orders?q=中文" -H "$A" | python3 -c 'import sys,json;print(json.load(sys.stdin)[0]["status"])')"
assert_eq "partial update 未提供 status 时保持原值（COALESCE）" "shipped" "$STATUS_AFTER"

# 4.7 校验规则
assert_eq "违反 @min(0) → 400" "400" \
  "$(curl -s -o /dev/null -w '%{http_code}' -X POST "$B/products" -H "$A" -H "$H" -d '{"sku":"BAD","title":"x","price":"-1","stock":1}')"

# 4.8 审计落库
AUDITS="$(docker exec "$DB_CONTAINER" mysql -uroot -p"$DB_PASS" -N -B "$DB_NAME" -e 'SELECT COUNT(*) FROM audit_logs' 2>/dev/null | tr -d '[:space:]')"
if [ "${AUDITS:-0}" -ge 2 ]; then ok "审计落库（≥2 条）"; else bad "审计落库" "count=$AUDITS"; fi

# 4.9 定时任务自注册
JOBS="$(curl -s "$B/jobs" -H "$A" | python3 -c 'import sys,json;print(len(json.load(sys.stdin)["jobs"]))')"
assert_eq "定时任务自注册（不依赖 seed）" "1" "$JOBS"

# 4.10 SSE 实时推送
( curl -s --max-time 5 -N "$B/live/OrderFeed" > "$WORK/sse.out" 2>&1 & )
sleep 1
printf '{"customer":"%s","total":"1.00","note":"sse-probe"}' "$CID" > "$WORK/sse-order.json"
curl -s -o /dev/null -X POST "$B/orders" -H "$A" -H "$H" --data-binary "@$WORK/sse-order.json"
sleep 2
assert_contains "SSE 收到 create 事件" 'create' "$(cat "$WORK/sse.out" 2>/dev/null)"

# 4.11 CORS 策略
ACAO="$(curl -s -D - -o /dev/null -H 'Origin: https://shop.example.test' "$B/products" -H "$A" | grep -i 'access-control-allow-origin' | tr -d '\r')"
assert_contains "CORS 精确允许配置来源" "https://shop.example.test" "$ACAO"
ACAO2="$(curl -s -D - -o /dev/null -H 'Origin: https://evil.example' "$B/products" -H "$A" | grep -i 'access-control-allow-origin' | tr -d '\r')"
assert_not_contains "CORS 拒绝未授权来源" "evil.example" "$ACAO2"

# 4.12 请求体上限
head -c 3000000 /dev/zero | tr '\0' 'x' > "$WORK/big.json"
assert_eq "超限请求体 → 413" "413" \
  "$(curl -s -o /dev/null -w '%{http_code}' -X POST "$B/products" -H "$A" -H "$H" --data-binary "@$WORK/big.json")"

# 4.13 认证失败限速
RLIMIT_CODE=""
for _ in $(seq 1 25); do
  RLIMIT_CODE="$(curl -s -o "$WORK/rl.json" -w '%{http_code}' -X POST "$B/auth/login" -H "$H" \
    -d '{"username":"bruteforce","password":"WrongPass123"}')"
  [ "$RLIMIT_CODE" = "429" ] && break
done
assert_eq "认证失败限速 → 429" "429" "$RLIMIT_CODE"
assert_contains "限速稳定错误码" "FORMA_RATE_LIMITED" "$(cat "$WORK/rl.json" 2>/dev/null)"

echo "== 5. 迁移幂等与双入口 =="
# CLI 先执行过 → 容器首次启动就应跳过（共享台账生效）
START_LOGS="$(docker logs "$APP_CONTAINER" 2>&1)"
case "$START_LOGS" in
  *"already applied, skip"*) ok "容器启动跳过 CLI 已应用的迁移（共享台账）" ;;
  *"0001_init.sql"*) ok "容器启动执行基线迁移（CLI 未先行时）" ;;
  *) bad "容器启动迁移日志" "$START_LOGS" ;;
esac
docker restart "$APP_CONTAINER" >/dev/null
sleep 8
assert_contains "重启后仍跳过已应用迁移" "already applied, skip" \
  "$(docker logs "$APP_CONTAINER" 2>&1 | tail -12)"
HEALTH2="$(curl -s --max-time 3 "$BASE/health" 2>/dev/null)"
assert_eq "重启后服务仍健康" '{"status":"ok"}' "$HEALTH2"
EXEC_OUT="$(DATABASE_URL="$DB_URL" "$FORMA" migrate "$PROJ" up --execute 2>&1 | tail -1)"
case "$EXEC_OUT" in
  *"no pending migrations"*|*"executed 0 migration"*) ok "CLI 与容器共享迁移台账（无待执行）" ;;
  *) bad "CLI 与容器共享迁移台账（无待执行）" "$EXEC_OUT" ;;
esac

echo
echo "prod-smoke: $PASS passed, $FAIL failed"
if [ "$FAIL" -gt 0 ]; then
  printf 'failed assertions:%b\n' "$FAILED_ASSERTIONS"
  exit 1
fi
exit 0
