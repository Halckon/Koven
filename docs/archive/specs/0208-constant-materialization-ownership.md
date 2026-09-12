# SPEC-0208：常量重新物化与所有权事实

> **性质**：实施 Spec · **状态**：done · **读取时机**：实施或评审 v0.36 对应 Goal 时 · **唯一真源**：本 Spec

## 1. 元数据

| 字段 | 值 |
|---|---|
| 状态 | `done` |
| Goal ID | `KOV-P3-208` |
| 所属 Phase | Phase 3 |
| 语言规范 | 现行 [v0.36 §36](../../guide/05-declarations-callables.md#36-无运行时存储的关联常量与封闭求值) |
| 批准依据 | 2026-09-12 用户明确启用 v0.36 并要求分阶段实施；依赖未完成者保持 draft |
| 前置 Spec | SPEC-0026、0028、0029 `done` |
| 前置 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` ownership/liveness/drop facts 与测试；Architecture |
| 语言语义变更 | 否；实施启用后的 v0.36 materialization 契约 |

## 2. Goal

完成后，ownership checker 消费 SPEC-0026 facts，把 scalar/Char const use 视为 Copyable inline
value，把每个 String const use 视为从编译期 UTF-8 bytes 新建的普通 String temporary owner，
不移动常量声明、不产生全局 owner 或 capture obligation。

## 3. 范围与需求

- constant declaration 是 compile-time identity，不进入 runtime owner state、loan graph、closure
  environment 或 drop plan；任何 use 都不能把 declaration 标为 moved。
- constant initializer 内的 dependency use 只由 SPEC-0026 evaluator 消费，不产生 runtime
  materialization；本 Spec 只为普通运行时 expression 中的 constant use 发布 facts。
- Boolean/integer/Char use 是 Copyable value；String use 每次独立 materialize，按普通 String
  literal的 Value/Borrow/return/ASAP drop 规则交付，多个 use 不共享 owner。
- String materialization 的 temporary/root/drop facts 绑定具体 use expression；失败 analysis 不
  发布半成品 facts，顺序与既有 expression evaluation 一致。
- object/companion 只贡献关联命名空间；无 singleton owner、init guard、exit drop 或 hidden retain。

## 4. 非目标

- 不生成 SSA/LLVM，不实现 runtime global、跨文件分析、associated function 或 object receiver。

## 5. 验收标准

- [x] scalar/Char const 可重复 Value-deliver 且没有 move/drop/loan；声明永不进入 moved state。
- [x] 同一 String const 的重复 use、Borrow call、concat/equality、return 与分支产生独立 owner，
  每个正常路径精确 drop 一次。
- [x] closure 引用 const 不产生 capture；object/companion 不产生 owner/init/drop facts。
- [x] return/abort/branch/loop 下 String temporary 的 liveness/drop 与普通 literal 一致。
- [x] validated marker、determinism 与现有 ownership/drop suite 回归。
- [x] Architecture 与实现事实同步。

## 6. 技术方案与边界

在 ownership expression dispatch 最前消费 constant-use descriptor，复用现有 String literal
temporary/drop 机制；不把 constant symbol伪装成 local variable，也不建立第二套 String owner。

## 7. 实施计划

1. [x] 建立 const-use materialization ownership facts → 验证：scalar/String model 测试。
2. [x] 接 liveness/drop/capture → 验证：控制流与交付矩阵。
3. [x] 同步验收与 Architecture → 验证：按[分层验收](../../development/testing.md)选择目标测试与必要下游检查，并记录命中数。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | typed const use 接入既有 temporary/loan/drop 路径与定向测试 | `feat(frontend): treat constant reads as runtime values (SPEC-0208)` |
| 2 | materialization 计划、validated marker、控制流验收与完成文档 | `feat(frontend): own constant materialization (SPEC-0208)` |

## 9. 未决问题

- 无；状态门禁由元数据表达。

## 10. 验证记录

实施前按[分层验收](../../development/testing.md)将第 5 节各项映射到实际测试目标/过滤器；
记录命中数、结果与未运行原因。同一状态下的有效证据只运行一次，不默认运行 frontend 全量。

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-27 roadmap 审计 | 通过 | 当前 constant initializer 仍走普通 runtime ownership flow；constant use 也没有 String temporary/materialization facts |

### 首切片：运行时读取与声明隔离

单文件 checker 跳过 constant initializer；expression/place/root/liveness/drop 直接查询已验证的
Phase 2 constant use，限定读取不再求值命名空间 receiver。String 二元操作的裸名称读取生成
use-expression temporary，Borrow 调用复用既有 temporary loan 与 CallReturn cleanup。

本切片未发布独立 materialization plan 或 validated ownership marker，也未完成
return/abort/branch/loop 交付矩阵；第 5 节完整验收项保持未勾选，0209 仍不可启用。


| 验收项 / 命令 | 实际结果 | 边界 |
|---|---|---|
| `cargo test -p lang-frontend --test ownership_constants` | 4 passed | 重复 Borrow 的独立 temporary/CallReturn drop、concat/equality 逆序 drop、scalar/Char Value 与 String return、普通/move closure 无 const capture |
| `cargo test -p lang-frontend --test ownership_constants --test ownership_checking --test ownership_closures --no-fail-fast` | ownership_checking 29 passed、ownership_closures 13 passed；当轮 constants 1 passed、3 fixture 语法失败，修正后由上一行重跑 4 passed | 共享 place/root/liveness/drop 调用方回归；没有把初次夹具失败算作通过 |
| `cargo clippy -p lang-frontend --lib --test ownership_constants --test ownership_checking --test ownership_closures -- -D warnings` | passed | 实现与受影响测试 |
| `cargo fmt --all -- --check`、`python3 scripts/check_docs.py`、`git diff --check` | passed；文档 351 | 无 inventory 迁移 |
| 独立代码审查 | 未发现本切片缺陷 | 检查 descriptor/category、place/root、pending-call cleanup、二元逆序 drop 与 capture；未额外运行 Cargo |

失败证据：修复前字符串二元测试只取得 1 条 temporary drop，预期 4 条；实现后通过。
未运行 frontend 全量、workspace check 或 native：本切片没有新增公开阶段 API，也未接入 Phase 4。


### 第二切片：可查询计划与验证能力

`ConstantMaterializationPlan` 绑定 Phase 2 descriptor，区分 InlineCopy/StringTemporary；
`ValidatedConstantMaterializations` 仅在 typed constants 有效、ownership 无诊断及 deferred 时发布。
`matches` 拒绝另一轮 typed analysis，`plan_at` 按 expression identity 查询；BTreeMap 去重并固定
发布顺序，initializer、group 和不可达尾句不会重复物化。

独立审查发现并修复两处控制流缺口：LocalVariable 不再丢弃 initializer 的 return/abort/break/
continue，drop planner 的显式 loop 仅在 checker 确认 break 出口时继续。修改中的解构 match arm
遗漏被编译器及复审检出并恢复。回归包含终止 initializer 与无出口 loop 后的不可达读取。

完整 SPEC 验收仍待最终逐项核对；本切片不启用 0209。


| 验收项 / 命令 | 实际结果 | 边界 |
|---|---|---|
| `cargo test -p lang-frontend --test ownership_constants --test ownership_checking --test ownership_closures --test ownership_nullable_when --no-fail-fast` | 7 + 29 + 13 + 26 = 75 passed；0 failed/ignored | 物化 identity/查询/确定性、失败清空、不可达排除；String 与 literal 的 Value/Borrow、return/abort、branch/loop cleanup 对照；共享变量、closure、nullable 控制流回归 |
| 独立复审 | 发现并修复 initializer 终止流及无出口 loop 后的规划缺陷；最终无剩余发现 | 保留 LocalDestructuring 分支，核对变量调用方、closure 状态与嵌套 loop break 消费 |

失败证据：API 新测试先报缺失公开接口；接入后终止 initializer 回归得到 4 个计划而非预期 3 个，
修复控制流后通过。未运行 frontend 全量或 native；本轮未接入 Phase 4。


| 追加门禁 | 结果 | 范围 |
|---|---|---|
| `cargo check --workspace --all-targets` | passed，8m 05s | 新增公开 ownership 产物的跨 crate 编译兼容性 |
| `cargo clippy -p lang-frontend --lib --test ownership_constants --test ownership_checking --test ownership_closures --test ownership_nullable_when -- -D warnings` | passed | 本轮实现和直接/共享测试目标 |
| `cargo fmt --all -- --check`、`python3 scripts/check_docs.py`、`git diff --check` | passed；文档 351 | 未改变 Spec inventory |


### 最终审计补修（尚未结项）

补充全部十种 scalar/Char 的重复 Value 交付与 group 无重复物化、deferred marker 门禁。
独立审计并用合法源码复现三个缺口：

- `stop() == TEXT` 后的局部常量绑定仍产生不可达 Named drop；通用 Binary 现在传播左侧终止。
- `view((TEXT))` 的 loan/drop owner 原绑定 Group，无法查询对应物化计划；现在仅对 constant
  Group 归一到叶 use owner，保留原 call argument/drop point。覆盖 Borrow、Value 提前 return、discard。
- `"${stop()}" + TEXT` 原产生四条虚假 drop；插值及专用 String operand 现在都传播终止。

以上是 Phase 3 修复；没有新增公开 API，不重复上一切片已通过的 workspace 编译门禁。
SPEC 仍保持 in-progress；未满足完整验收前不启用 0209。


剩余验收缺口（独立静态审计，待失败测试和修复）：`TEXT + "${if (flag) { return } else { 0 }}"`
中已求值的左 String temporary 只保存在 planner 的局部 pending drop，未进入 ValueState，右侧
return 的 cleanup 看不到它；break/continue 需同矩阵核验。后续应将左 operand 的存活义务接入
既有控制转移清理，并验证正常路径恰好一次 drop、Abort 不展开。普通 literal 也有此风险，不能
仅以 literal parity 证明规范正确。


| 本次补修门禁 | 结果 | 范围 |
|---|---|---|
| `cargo test -p lang-frontend --test ownership_constants --test ownership_checking --test ownership_closures --test ownership_nullable_when --no-fail-fast` | 12 + 29 + 13 + 26 = 80 passed；0 failed/ignored | 含新增 scalar/group/deferred、binary abort 与 String interpolation abort 回归 |
| `cargo clippy -p lang-frontend --lib --test ownership_constants --test ownership_checking --test ownership_closures --test ownership_nullable_when -- -D warnings` | passed | 本轮内部实现和受影响测试 |
| `cargo fmt --all -- --check`、`python3 scripts/check_docs.py`、`git diff --check` | passed；文档 351 | 未迁移 inventory |
| 独立复审 | 已复现缺口修复无新增发现；保留上述 String 左 operand 跨右控制流清理缺口 | 未额外运行 Cargo；未认定完整 Spec 通过 |


## 11. 最终验收（2026-09-12）

本节结论取代前述各实施切片当时的未完成状态；保留失败及修复记录供追溯。

最后两处清理遗漏已经修复：String 左 temporary 和插值输入 temporary 均随既有 ValueState
传递，return/break/continue 清理，Abort 不展开。正常 binary 仍按右左顺序在运算后清理；
插值输入在外层 String 完成后逆序清理，外层结果 owner 不受影响，也不清理仍 live 的 named owner。

| 第 5 节验收项 | 直接证据（ownership_constants） | 结论 |
|---|---|---|
| scalar/Char 重复交付 | `every_closed_scalar_type_repeats_value_delivery_without_runtime_owners`；十种封闭 scalar/Char、group，无 loan/drop/capture | 通过 |
| String 独立 owner 与正常清理 | `repeated_string_borrows_own_distinct_temporaries_without_declaration_loans`、`string_binary_operands_drop_materializations_in_reverse_order`、`scalar_delivery_and_string_return_do_not_own_constant_declarations`、`grouped_string_reads_keep_the_materialization_owner_identity` | 通过 |
| namespace/closure 无 owner/capture | `closure_reads_do_not_capture_constant_or_namespace_identity`；checker/drop/liveness 跳过声明 initializer，capture 排除 Constant/ObjectValue/Classifier | 通过 |
| return/abort/branch/loop | `constant_cleanup_matches_literal_on_control_flow_edges`、`string_left_temporary_cleanup_follows_right_control_transfer`、`nested_string_left_temporaries_cleanup_in_reverse_evaluation_order`、`interpolation_prefix_temporaries_follow_control_transfer_and_abort`；直接清理数量/逆序断言补足 literal 对照 | 通过 |
| marker、确定性与回归 | `materialization_plans_exclude_dependencies_and_bind_exact_analysis`、`ownership_failure_clears_materialization_capability`、`deferred_ownership_does_not_publish_constant_capability`；typed constants 缺失门禁经独立代码审查 | 通过 |
| Architecture | ownership.md 已同步计划、owner identity 与清理实现事实 | 通过 |

| 最终检查 | 实际结果 | 复用边界 |
|---|---|---|
| `cargo test -p lang-frontend --test ownership_constants --test ownership_checking --test ownership_nullable_when --no-fail-fast` | 16 + 29 + 26 = 71 passed；0 failed/ignored | 最终源码状态；包含两处新增清理修复 |
| `cargo clippy -p lang-frontend --lib --test ownership_constants --test ownership_checking --test ownership_nullable_when -- -D warnings` | passed | 最终实现与受影响测试 |
| `cargo fmt --all -- --check` | passed | 最终 Rust 源码 |
| 独立最终复审 | 先前发现均已落实修复，无剩余明确缺口 | 正常逆序、嵌套 control identity、loop-depth、Abort、插值边界与 named-owner 保护 |
| 公开 API 与其他回归 | 复用第二切片 workspace check 及已记录 closure 回归 | 后续没有新增公开 API 或修改 capture 契约 |

本轮失败证据：String 左 temporary 的 return 清理为 0、预期 1；两个 String const 插值输入 drop
为 0、预期 2。实现后对应矩阵全部通过。未运行 frontend 全量或 native；Phase 4 由 SPEC-0209 承接。

归档门禁：`python3 scripts/check_docs.py`（351 Markdown）、`python3 -m unittest discover -s scripts/tests -p 'test_check_docs.py'`（21 passed）、`git diff --check` 均通过；inventory 同步为 206 份归档、active 0209、v0.36 draft 0210。
