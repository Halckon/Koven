# 历史边界回归输入

这些最小源码从 SPEC-0295/0296 的历史 evidence 原字节迁入；不是生成输出。

- `map-word-frequency-0295/known-boundary/`：完整最小多文件项目；
  `src/p/provider.ko` 由 `src/native/unit_owned_null_cleanup_tests.rs` 编译时读取。
- `owned-null-cleanup-0296/`：single scalar、loop、nullable factory、临时结果边界源码；
  `single-loop-boundary.ko` 由 `src/ssa/lower_frontend_map_tests.rs` 编译时读取。
- 其余最小源码保留用于手动重现；成功/失败随具体源码版本与测试断言解释，不从文件名推断。

原始运行观察及平台边界见[历史证据索引](../../../../docs/development/evidence/README.md)。
