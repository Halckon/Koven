# Unit 多根符号映射与旧见证退出条件

> **性质**：局部差异审计，非规范 · **状态**：候选待裁决，未退休 · **基线**：`0505b8cb` · **范围**：原 scalar generic/overload 两组普通源码

## 实测映射

同一原源通过共同 Unit Program 的 Rooted/MemoryDocument 两布局，以下名称和
函数位置均一致。`d` 是声明索引、`t` 是本次 Unit 类型索引；它们是此次输入的
观察值，不是跨程序/版本固定数字。LLVM adapter 将 SSA 名加 `f<位置>.` 前缀。

| 原 legacy SSA 名 | Unit SSA 名 | 本次 Unit 函数位置 |
|---|---|---|
| identity<Int> | koven.identity.d0.t2 | 0 |
| identity<Long> | koven.identity.d0.t3 | 1 |
| relay<Int> | koven.relay.d1.t2 | 2 |
| relay<Long> | koven.relay.d1.t3 | 3 |
| recurse<Int> | koven.recurse.d2.t2 | 4 |
| useInt / useIntAgain | koven.useInt.d4 / koven.useIntAgain.d5 | 5 / 6 |
| useLong / recursive | koven.useLong.d6 / koven.recursive.d7 | 7 / 8 |
| choose<Int>（单参数） | koven.choose.d0.t2 | 0 |
| choose<Int>（带 fallback） | koven.choose.d1.t2 | 1 |
| one / two | koven.one.d2 / koven.two.d3 | 2 / 3 |

例如原 overload oracle 的 `f0.choose<Int>` / `f1.choose<Int>`，在本次 Unit 图中
分别对应 `f0.koven.choose.d0.t2` / `f1.koven.choose.d1.t2`。两者不能按裸旧名称
一对一查找，必须以原声明/签名与实际 DirectCall 目标区分。原 unused 泛型仍不发射。

实现依据：`ssa/lower_frontend/orchestrate.rs::instance_function_name`、
`ssa/unit_lower.rs::instance_function_name` 与 `llvm/adapter/call.rs`。

## 哪些证据可承接，哪些不能删除

| 旧断言职责 | 现有共同 Unit 证据 | 退出条件 |
|---|---|---|
| 两种具体类型各唯一实例、共享调用去重、unused 不发射 | 同图九函数、计划 key 并集、真实源与类型、DirectCall 身份 | 可作为 Unit 语义承接证据；不宣称旧入口已无人使用 |
| recurse 自调用保持实例身份 | 同一 Program 中真实递归目标等于自身 | 不需要保留旧字符串切片作为 Unit 的唯一检查方式 |
| overload 相同类型参数仍不混同 | 原两声明 span、不同真实目标、两种参数模式、verified LLVM | 必须保留声明身份与参数顺序，不得只比较计数 |
| 原旧 SSA/LLVM 名字的确切拼写 | 如上名称存在有意差异 | 旧入口保留期间继续保留原 oracle；不能改生产名或放松断言求等价 |
| 同输入稳定生成 | Unit 逆序根映射/SSA/LLVM 一致，旧 repeated oracle 仍在 | 稳定性合同不能随名称拼写退休 |

[ADR-0008](../adr/accepted/0008-internal-value-and-allocation-abi.md)不承诺跨版本
稳定符号；[ADR-0010](../adr/accepted/0010-first-native-object-and-linker-contract.md)
要求同输入内部符号确定性。这允许提出“退休旧入口时，退休旧字符串拼写 oracle，
保留语义身份与确定性”的候选，不构成现在删除旧入口或其测试的授权。

最小安全顺序：先取得多根 CI 负控动态结果，再明确批准多根测试入口及旧入口消费
范围的退休；仅删除已无消费者且不再承担独有合同的 helper。不得据本表删除全部
legacy frontend、receiver/closure/拒绝边界见证，或启用生产多 entry。

## 本轮验证

原 Unit 两个 ordinary exact 2/2（双布局）与原 legacy 两个 ordinary exact 2/2
通过。符号观察使用临时普通诊断，随后完整移除，Rust 测试文件无最终 diff；
不把临时输出作为新持久测试。未执行 CI-only、native 或全量测试，未推送。
