# 普通源码回归输入

以下 SPEC-0295/0296 最小源码从历史 evidence 原字节迁入；不是生成输出。

- `map-word-frequency-0295/known-boundary/`：完整最小多文件项目；
  `src/p/provider.ko` 由 `src/native/unit_owned_null_cleanup_tests.rs` 编译时读取。
- `owned-null-cleanup-0296/`：single scalar、loop、nullable factory、临时结果边界源码；
  `single-loop-boundary.ko` 由 `src/ssa/lower_frontend_map_tests.rs` 编译时读取。
- 其余最小源码保留用于手动重现；成功/失败随具体源码版本与测试断言解释，不从文件名推断。

原始运行观察及平台边界见[历史证据索引](../../../../docs/development/evidence/README.md)。

SPEC-0300 新增的 `map-loop-owner-0300/` 由
`src/ssa/lower_frontend_map_loop_tests.rs` 与 `src/native/unit_map_loop_owner_tests.rs` 消费：

- `normal-exhaustion.ko`：0/1/3 轮，Map 后续使用/不再使用，remove 与 put/contains 对照。
- `distinct-owners.ko`：own 参数 seed 与同函数两个同类型 Map 的独立 owner。
- `guarded-abort-boundary.ko`：保留初次红测中带 error() 的较广 guard 组合，当前为静态负例。

0296 的原始 `single-loop-boundary.ko` 原字节不变，现在由 single 与 unit 的 SSA/native
正例共同消费；文件名中的 boundary 不表示当前仍拒绝。输出不替代内存计数或安全结论。
