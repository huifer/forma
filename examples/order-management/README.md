# order-management — Forma 端到端示例

一个由五种现有标准描述的订单管理项目。**没有任何 Forma 自有业务语法**：

| 文件 | 标准 | 拥有的事实 |
|---|---|---|
| `specs/database/schema.sql` | SQL DDL（SQLite） | 表、列、索引、约束 |
| `specs/schemas/order.schema.json` | JSON Schema 2020-12 | Order 类型的字段与校验规则 |
| `specs/api/openapi.yaml` | OpenAPI 3.1 | REST 契约 |
| `specs/ui/order-form.ui.json` | JSON Forms UI Schema | 表单布局与控件 |
| `specs/workflows/order-approval.bpmn` | BPMN 2.0 | 审批流程 |

`forma.yaml` 只声明清单、标准版本与编译目标；实体身份由 `x-forma-entity` 扩展与名称约定（`orders` ≡ `Order`）绑定。

## 快速开始

```bash
# 在仓库根目录构建 CLI 后：
cd examples/order-management

forma doctor     # 跨标准一致性诊断
forma graph      # 查看语义图（谁定义/引用了 Order）
forma build      # 生成 TS 类型、Hono 路由、SQLite 迁移、React 表单
```

## 演示：跨标准语义变更传播

「Order 增加一个付款状态」——一条语义补丁，四处同步：

```bash
forma patch patches/add-payment-status.json
```

补丁执行结果：

1. `schema.sql` — `orders` 表新增 `payment_status TEXT` 列
2. `order.schema.json` — `Order` 新增 `paymentStatus` 属性（enum）
3. `openapi.yaml` — `components.schemas.Order` 同步属性
4. `order-form.ui.json` — 表单新增控件
5. 自动校验 + 增量重编译受影响产物

撤销演示：删除补丁影响的四处内容即可（或 `git checkout specs/`）。
