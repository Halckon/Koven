# D10 legacy 消费者退出审计（2026-11-14）

> **性质**：只读代码审计 · **状态**：当前快照，非新能力授权 · **基线**：`99d72095` · **范围**：codegen 非 `lower_frontend/` 子树中直接调用旧 `check_types` / `check_ownership` 的测试消费者，以及直接依赖

## 结论与边界

普通源码测试迁至 Unit 不等于旧 API 已可删除。下表是定向消费者审计，不是所有
Rust 引用的完备依赖图，也不证明 D10/E2 完成。未刷新远端；本地 main 引用为
`3b7a9a7a`。发布链与其他并行分支的验证不能替代本快照，本文不更改其状态。

检索依据为 `crates/lang-codegen/src` 下 `check_types(` / `check_ownership(` 调用，
随后读取所属测试、helper 及已有 Unit 对照。没有执行测试、故障注入或 native。

## 仍保留的消费者与最小安全步骤

| 消费者（相对 `crates/lang-codegen/src/ssa/`） | 已有 Unit 证据／差异 | 可消除依赖的最小安全步骤 |
|---|---|---|
| `lowering_entry_contract_tests.rs::legacy_only` | 普通 L0080/L0084 已独立 Unit；仍有 source/analysis mismatch 优先级、旧 own receiver L0170 与 receiver String 拒绝 | 保留旧边界见证。不能用 Unit 成功替代拒绝；先列出该旧入口最终消费者及其明确退休合同 |
| `lower_frontend_tests.rs` | scalar/straight-line 等已转 Unit；仍有 whole-file 多根 generic 去重、同名 overload、旧拒绝边界、混配分析链 | 优先核 ordinary multi-root 的根选择、实例身份、递归目标、完整 SSA/LLVM 计数；逐项等价前不删旧 fixture。多次独立 root lowering 不能假称同一 whole-file 图 |
| `unit_llvm_tests.rs::callable_plan_tests` | ordinary nullable planner 已转 Unit；旧 callback specialization 按环境产生实例，Unit ADR-0034 callable view 共享 key | 普通参数映射可单独审计，但旧实例数不可改小冒充迁移。必须保留身份差异或经独立退休决策处理 |
| `refine_closure_tests.rs::source_closure_tests` | guard、shared cleanup、when、Nothing 已转 Unit；Unit 不可达对照已有。仍核旧 deferred callable origin、static capture、nested 边界与具体 callback ABI | 先定义相应 Unit 事实的对照关系；仅 SSA 成功/无 thunk 不能替代旧静态 capture 和 arena/来源断言 |
| `map_source.rs::canonical_callable_tests` | 普通三形状具体化已转 Unit；旧 MissingFact 只读负控仍直接测试旧 resolver | 保留负控直到旧 resolver 无消费者或独立退休。不得本地运行变异路径来凑验收 |
| `borrow_result_contract_tests.rs::single` | 普通借用结果走 memory/rooted Unit；helper 仍被 whole-file wrapper 与旧 unsupported 边界调用 | wrapper 的未选中声明发射和 Unit selected-root 是真实差异，`cd7abc2b` 已加强差异见证；不能直接替换 helper |
| `single_readonly_method_tests.rs` | 普通正例已 Unit；legacy helper 明确只承载旧拒绝及精确源码片段 | 不把正例通过当作负控等价；先证明旧入口不再需要此拒绝合同 |
| `lower_frontend_owned_source_continue_tests.rs` | RangeView 负控明确区分 Sequential source proof | 保留旧 UnsupportedNode 证据；不借迁移扩张 RangeView 能力 |
| `integer_range_value_tests.rs` | 普通双布局已 Unit；四个 shape-boundary whole 仍走旧 helper | 按四个边界逐个对照；不能从普通 Range value 成功推出 whole-file 边界可删 |
| `rc_nullable_while_bounded_tests.rs`、`rc_nullable_while_observation_tests.rs` | 已有 Unit/refine 观察；旧 oracle 查询 per-family synthetic block，refine 是不同 phi 结构 | 将语义不变量与旧结构身份分开；已有注释明确结构 oracle 不匹配，不能取消断言来获得绿灯 |
| `while_flow_tests/if_cleanup_observation.rs` | 同时观察 Single/Unit Phase 3；`UnitDropFact` 不提供旧公开 condition/owner/table | 在未有等价观察接口前保留旧条件清理事实，不以 drop 数量替代精确条件 |
| `map_provider_exits_pipeline_tests/local_sources/deep/nullable_map_guard_support.rs` | 旧 type mapper、plans/layouts 和 checkpoint 身份观察 | 先建立明确字段映射和消费者清单；当前审计未证明 Unit 等价，不直接替换 |
| `unit_lower/nullable_when/source.rs::frontend_registration_tests` | 明确 CI-only mapper 合同，真实 FE 身份，非 SSA/native 结果 | 保留 CI-only 分类；不得本地执行或以普通 Unit 样例替代 mapper 合同 |

## 两个容易误判的点

1. `single_callable_plan_named_reordering_uses_sparse_declaration_slots` 的旧 helper
   计数是 **2**。Unit 的
   `unit_callable_plan_frozen_named_routes_preserve_sparse_slots_and_owner_tokens`
   明确多个 route 共享 key、callable arguments 为空。源码参数重排相似不代表
   实例模型等价，不能把 2 改成 1 后宣称去掉旧依赖。
2. `name_resolution/compilation_unit/names.rs` 的 Unit 名称解析仍实际调用
   `resolve_names`。因此“Unit 已上线”不意味着整个旧 frontend 实现可 `cfg(test)`。
   此项是生产依赖，不在本轮测试迁移可删除范围。

## 下一步的退出条件

- 每个旧 helper 必须有实际消费者清单；零消费者才可直接删除。
- 对 ordinary 多根/overload 优先做源码与 root/instance 对照，不扩语言语义。
- 对身份、ABI、拒绝边界保留独有证据；若决定退休旧内部合同，需明确它不再被要求，
  不能把设计改变藏进测试迁移。
- 所有新验证绑定实际最终头；普通 exact、本地编译/静态检查、CI-only 动态验收分别记录。
- 不为持续开发频率新增已有等价测试，不将审计文字当作生产能力交付。
