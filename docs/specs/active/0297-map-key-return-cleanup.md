# SPEC-0297: Map 查询与删除的 key return 清理

> **性质**：变更合同 · **状态**：in-progress · **读取时机**：实施或验收 Map key 提前返回修复时 · **唯一真源**：本 Spec 的范围与验收账本

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P4-0297` |
| 所属 Phase | Phase 3 事实消费、Phase 4 SSA/native |
| 语言规范 | 现行 [Guide v0.43](../../guide/README.md)、[所有权](../../guide/10-ownership-borrowing-drop.md) |
| 批准依据 | 2026-10-11 已确认0297优先；2026-10-12 PR77合并后独立分支集成，只激活0297 |
| 前置 Spec | [SPEC-0294](../../archive/specs/0294-map-put-control-exit.md) |
| 前置 ADR | [ADR-0016](../../adr/accepted/0016-interprocedural-borrow-abi.md) |
| 关联 ADR | 无新增；若需要改变 ABI，先形成独立决定 |
| 阻塞项 | 接收当前main后重验；发布与生命周期另行闭环 |
| 影响范围 | `lang-codegen` Map lowering、直接测试与实现事实 |
| 语言语义变更 | 否；不启用新 Guide |

## 1. Goal

`get`、`contains`、`remove` 的 key 求值提前 return 时，结束实际 receiver/key
借用并清理已求值 owner；完全退出传播 Diverged，正常出口仍执行恰一次 Map 操作。

## 2. 基线与已核实边界

起草时静态基线为 main `e168c485a5379764cf927433452f8b7059f7b334`。
`ssa/lower_frontend/map.rs` 的三操作通过 `lower_map_owner` 要求 key 为 Value；
`lower_frontend.rs::require_value` 将 Diverged 转为 UnsupportedNode。unit 对应
`unit_lower/map.rs::map_owner_operand` / `require_lowered_value` 将其转为 MissingFact。
它们没有 Map.put 已接入的完整退出传播。

single 的 owned receiver 仅保存在 temporary 中，未像 put 一样登记 receiver 的
pending cleanup identity；frontend 已发布的 return loan-end 可能因此在
`lower_frontend/call_lifetimes.rs::emit_control_transfer_cleanup` 缺失。
应按真实 call/argument identity 证明该缺口，不能伪造 SSA loan。

`if (flag) { return } else { key }` 有正常 Value 出口，不等于整个表达式 Diverged。
unit mixed 不能仅凭代码缺少显式 Diverged 分支就判失败；single/unit 必须分别记录。
既有 `map_operands_follow_argument_control_flow` 只证明无退出 if，不替代 return 验收。
SPEC-0294 明确只修 put，本片不重写其有界归档。

后续使用冻结的0296 CLI（生产实现 `ae65b2ce`，非上述main二进制）执行
[普通源码矩阵](../../archive/migrations/pr72-74-followup-audit.md#冻结cli的map-key-return矩阵)：
owned named `MutableMap<Int, Int>` 的三操作在single中，mixed、反向mixed、双return
均以MissingFact拒绝；unit两种mixed均编译运行成功，双return为MissingFact。
六个无return对照均成功。CLI未提供错误Span或内部事实，不能从该输出单独定位
具体MissingFact调用点；single的完整Diverged转换仍是后续静态边界，不是本次诊断。

## 3. 范围与非目标

- 三操作各覆盖 mixed、反向 mixed、双 return、直接 return；receiver 精确求值一次。
- 先使用 owned named receiver；Borrow receiver、temporary receiver 按 Guide 合法性
  分别建立对照，不能把其中一种结果扩大到所有 receiver。
- String key 使用 named、grouped 和 temporary 形状；退出前已求值 owner 按事实清理，
  未求值后缀不运行；正常查询不得消费 Borrow key，remove 仅移交被删除 value。
- 沿用 frontend 清理事实、既有 carry slots 和 verifier；不按源码名称重推语义。
- 不扩普通调用、Elvis、Map withValue/requireValue 或 nullable V 的 owned 三态表示；
  不把本片扩大为一般 CFG 重构，也不处理 SPEC-0296 的最后判空提前 drop。

`break` / `continue` 参数退出只登记为未批准的后继实施候选，不纳入本片 return Goal。
它们的语言语义并未禁用。启动前须另定范围，先证明目标 loop identity、前缀 temporary
恢复、provider/source loan 在 continue 保留及 break 退出、嵌套 loop 只清理最近目标，
再以两入口普通源码和正常 native 验证；不能把 put-return 通过视作这些能力已接通。

## 4. 验收标准

- [ ] 同一合法源码先通过 typed/ownership，再分别记录三操作 × 四种 return 的
  single/unit 结果、错误 kind/Span 或健康 SSA；已有成功用例保留成功。
- [ ] mixed 两种动态输入保持正常 Map 结果或提前返回；双/直接 return 无 Map 操作、
  无后缀调用，verifier 与未改写 LLVM 均通过。
- [ ] String/Resource 前缀正常 native 验证求值顺序、Borrow key 可复用、清理一次及
  未执行后缀；普通输出不冒充逐指针计数、sanitizer 或完整内存证明。
- [ ] 原 put-return 与无退出三操作回归保持；非法 borrowed move 的诊断不放宽。
- [ ] 按实际改动运行 fmt、受影响 crate clippy、尺寸与文档检查，更新 Architecture。

## 5. 实施与提交

本次从 actual main `289a8ad94d275f6ad497df2b12bdd134cebe4b81` 新建
`fix/spec-0297`；接收树为 `1e485f4cf59bb315d4294704cf9277ff53b68c1b`。
原实现基于 `8c6907c2`，恢复冻结树 `f880ce36d341bfe92e3757b83d3f8d42939c1012`
已逐字节复核；原七提交对象未恢复，本分支采用新的逻辑提交，不冒充原提交历史。
只接入0297合同、既有回归与有界生产修复，保留0302 fixture及0303治理、归档和proposal。
0298–0300继续draft；发布、PR、CI与合并按当时授权独立闭环。

## 6. 验证记录

原实现和恢复后普通定向验收为历史观察；接收新main后在本分支重新运行，结果待登记。
起草阶段的冻结CLI结论见§2，不作为新集成树的功能验收。main基线CI
[38219409371](https://github.com/Halckon/Koven/actions/runs/38219409371)仍待终态核验。

| 检查 | 实际结果 | 边界 |
|---|---|---|
| 当前main普通源码红测与真实facts | 待运行 | 同一typed/ownership成功源码，分别核对single/unit及错误Span |
| 两入口SSA、原始LLVM与普通native | 待运行 | 不注入或改写IR；覆盖三操作四形状和合法receiver/owner前缀 |
| 直接相关回归、fmt/clippy/尺寸/docs | 待运行 | 不以全仓聚合代替定向检查 |

本地不运行fault injection、校准、sanitizer、reducer或聚合驱动；远端CI尚未执行。
