# viz-antv reference application

AntV（G2Plot）Adapter 参考工程（RFC-0010 V0.6）：验证 Forma 的第二图表 Adapter 产物
可安装、可编译（CI 执行 `npm install` + `npx tsc --noEmit`）。

```bash
cargo run -- build examples/viz-antv
cd examples/viz-antv/generated/web && npm install && npx tsc --noEmit
```

与 `examples/school`（echarts adapter）对照：ChartSpec / transform 共享，仅 option 投影不同。
