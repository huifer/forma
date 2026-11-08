# access-depth reference application

P3/C-3 CI 参考工程：验证**行级访问任意深度关系遍历**生成的 Rust 后端可编译。

- `school.district.name` / `school.district.code` 两层关系遍历 → 逐跳 `repo::find_{E}` 守卫；
- `auth module` 但**无 `@tenant`**（P2/A-2 抓出的存量编译错误组合）；
- 目标实体 `District` / `School` 没有 `update`/`delete` API，仍需生成 `find_X` 仓储函数。

本地验证：

```bash
cargo run -- build examples/access-depth
cd examples/access-depth/generated/backend && cargo check
```

CI 的 `generated-backend` job 会同时编译本工程与 `examples/school`。
