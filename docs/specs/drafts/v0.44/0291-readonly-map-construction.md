# SPEC-0291: 非空只读 Map 构造：MutableMap 立即消费式转换 consume()

> **性质**：变更合同 · **状态**：draft · **读取时机**：准备或验收非空只读 Map 构造路径时 · **唯一真源**：本 Spec；语言语义以启用后的 Guide v0.43 为准

| 字段 | 值 |
|---|---|
| 状态 | draft |
| Goal ID | `KOV-P2-0291` |
| 所属 Phase | Phase 1 规范（Guide 12）；Phase 2 类型检查；Phase 3 所有权与 move；Phase 4 typed SSA；Phase 5 LLVM；Phase 6 native |
| 语言规范 | 拟并入 v0.43 Guide：[12-collections-destructuring.md](../../../guide/12-collections-destructuring.md) 中 `MutableMap` 第 5 条"消费式转换 `consume()`"（WIP 位于 `feature/spec-0289`，尚未启用） |
| 批准依据 | 2026-10-09 用户确认：(1) 消费式转换并入 v0.43，不新开 v0.44；(2) `mutableMap.consume(): Map<K, V>` 为立即转换，复制式 `toMap()` 延后；(3) 本草案改名为 0291；(4) Guide 正文按快照恢复 |
| 前置 Spec | [SPEC-0288](../../../archive/specs/0288-map-native-execution.md) 已合入 main（`ef60f2fc`）；SPEC-0289 的 v0.43 启用（WIP） |
| 前置 ADR | 无（不改变布局或 ABI） |
| 关联 ADR | 无 |
| 阻塞项 | (1) **v0.43 尚未启用**：SPEC-0289 的 v0.43 WIP 未合入 main，Guide 启用与 `check_docs.py` 的 v0.43 门禁更新需单独批准；(2) 实施前须确认 §3.1 的规范文本；(3) `consume` 与 Guide 05 现有调用拼写 `consume(x)` 的关系需在启用前写清（见 §6） |
| 影响范围 | `lang-frontend`（方法识别、receiver move 检查、诊断）、`lang-codegen`（SSA 操作与 verifier、LLVM 移交、native 测试）、Guide 12 |
| 语言语义变更 | 是：新增 `MutableMap<K, V>.consume(): Map<K, V>`，需 v0.43 启用 |

## 1. Goal

完成后，已填充的 `MutableMap<K, V>` 可以通过预声明的立即消费式转换 `consume()` 零复制地得到同 K/V 的只读 `Map<K, V>`，
并且只读 Map 的查询与 `size` 在单文件与编译单元中都可原生执行、精确释放。

## 2. 背景

Guide 12 规定 `Map<K, V>` 为只读独占 owning 容器，但 `mapOf()` / `mutableMapOf()` 在 v1 不接受参数，
因此只读 Map 当前只能是空的。r3 候选（`docs/proposals/collection-algorithm-ownership-r3.md` §3.2）规定：
**消费式**一律用 `consume()` 取得 owner，源 binding 之后不可用；**复制式**用 `.toList()` 等以元素 clone 为前提。
本 Spec 只实现 Map 的消费式路径；复制式 `toMap()` 延后，与 `.toList()` 同属 clone 前提的物化。
命名与 Kotlin 对齐的是复制式 `toMap()`，但 Koven 的 `consume()` 语义是移动，不复制。

## 3. 范围与需求

### 3.1 语言规范（拟并入 v0.43）

- `MutableMap<K, V>.consume(): Map<K, V>` 为预声明、立即产出的转换（不返回延迟序列）。
- receiver 必须是 owned 本地绑定；调用成功后源绑定不可用，之后使用报既有 move 诊断。
- 结果是同 K/V 的只读 `Map<K, V>` owner；不复制、不 clone 任何键或值，条目缓冲区整体移交。
- 活跃借用期间不能转换同一来源，沿用 L0135 冲突诊断。
- 复制式 `toMap()` 不在本版启用。

### 3.2 前端（lang-frontend）

- single-file 与 compilation-unit 两条路径识别 `consume()`，返回类型为 `Map<K, V>`。
- 检查 receiver 的 owner 状态与 move 退休，发布 typed 描述符（receiver、K、V、源与结果类型）。
- 非 owned receiver 给出结构化诊断，保留真实 `Span`。

### 3.3 SSA、verifier 与 LLVM（lang-codegen）

- 新增 SSA 操作：消费一个 `MutableMap` owner，产生一个同 K/V 的 `Map` owner。
- verifier：输入必须是 owned `MutableMap`；输出必须是同 K/V 的 `Map`；源 owner 已消费。
- LLVM：原样移交 header `{buffer, size, capacity, tombstones}` 与条目缓冲区，不改变布局、不复制条目、不析构。
- drop：只读 Map 与 MutableMap 使用同一套 drop glue（遍历 Occupied 槽位，分别析构 MoveOnly 键与值，再释放缓冲区）。

## 4. 非目标

- `mapOf(...)` / `mutableMapOf(...)` 带参数形式：依赖 `to` 运算符的规范冲突裁决（Guide 01 写作构建 `Pair`，Guide 04 写作 range，编译器中为 deferred），延后。
- 复制式 `toMap()`：需要 K/V 可 clone 的前提，延后。
- `Map` → `MutableMap` 反向转换、只读借用视图、Map 迭代（M3B）。
- nullable V 的 owned `remove` 三态表示（独立问题）。
- 用户自定义 `Hashable`。

## 5. 验收标准

- [ ] v0.43 启用后，Guide 12 的第 5 条与 r3 §3.2 一致；`check_docs.py` 的 v0.43 门禁更新并通过。
- [ ] 前端 single-file：非空 `MutableMap<String, Int>` 调用 `consume()` 后，只读查询通过；源绑定再次使用被拒绝。
- [ ] 前端 compilation-unit：同上，并验证正逆输入顺序的 SSA 一致。
- [ ] 前端负例：Borrow/Inout/字段/元素 receiver 被拒绝，诊断码与 `Span` 正确；活跃借用期间的转换报 L0135。
- [ ] SSA verifier 正例与负例：输入非 MutableMap、K/V 不一致、源未消费均被拒绝。
- [ ] native（single 与 unit 各一组）：String 键 + MoveOnly/Resource 值，空表与非空表；转换后 `contains`、`get`、`requireValue`、`withValue`、`size` 结果正确。
- [ ] native 计数：每个键、值与缓冲区精确释放一次，无重复析构。
- [ ] 不改变 `MutableMap` 或 `Map` 的既有布局与已有测试结果。
- [ ] 门禁：`cargo fmt`、`cargo clippy -D warnings`、相关 targeted tests、`check_docs.py`、`check_rust_sizes.py`、`git diff --check` 均实际运行并记录结果。
- [ ] 未运行的检查明确标注，不计作通过。

## 6. 开放决策

1. receiver 是否接受 owned 临时值，例如 `mutableMapOf<Int, Int>().consume()`。建议接受，待确认。
2. 诊断码分配（非 owned receiver、转换后使用）。
3. **命名冲突核对**：v0.42 Guide 05 写有"调用仍写 `consume(x)`"，Guide 07 有示例 `fun consume(own value: T)`。需要在启用前确认：预声明的方法形式 `mm.consume()` 与该调用拼写互不歧义，并确认 `consume` 不是保留字。
4. 确认 `consume()` 对 `MutableMap` 的签名与 r3 §8.1 中 `List.consume(): ConsumingSeq<T>` 的关系：两者结果类型不同（Map 直接产出，List 首片为序列）。

## 7. 验证记录

尚未开始实施。草案本身已经用 `check_docs.py` 对 `feature/spec-0289` 的 Guide 条目做过检查，但 v0.43 门禁尚未启用，因此不计为通过。
