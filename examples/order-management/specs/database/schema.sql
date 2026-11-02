-- Database structure is owned by SQL DDL — no Forma-specific syntax here.
-- orders 表通过名称约定（orders ≡ Order）绑定到实体 Order。
CREATE TABLE orders (
    id TEXT PRIMARY KEY,
    amount DECIMAL(12, 2) NOT NULL,
    status TEXT NOT NULL,
    customer_email TEXT NOT NULL,
    created_at TIMESTAMP NOT NULL
);

CREATE INDEX idx_orders_status
ON orders(status);
