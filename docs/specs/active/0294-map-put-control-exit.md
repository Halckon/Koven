# SPEC-0294: Map.put 参数 return 提前退出的清理与 Diverged 传播

> **性质**：单一变更合同 · **状态**：in-progress · **读取时机**：实施或验收本修复时 · **唯一真源**：本 Spec 的范围与验收账本

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P4-0294` |
| 所属 Phase | Phase 4 |
| 语言规范 | 现行 [Guide v0.43](../../guide/README.md)、[所有权与 drop](../../guide/10-ownership-borrowing-drop.md) |
| 批准依据 | 2026-10-11 用户同意独立修复已确认的 Map.put 提前退出问题并补正常回归 |
| 前置 Spec | SPEC-0288、SPEC-0290 |
| 前置 ADR | ADR-0016 |
| 关联 ADR | 无新增 |
| 阻塞项 | 发布及双宿主 CI 尚未执行 |
| 影响范围 | `lang-codegen` 的 single/unit Map.put lowering、直接回归和当前事实文档 |
| 语言语义变更 | 否 |

## 1. Goal

合法 Map.put 源码的 key/value 求值以 return 提前退出时，single 与 unit 入口遵守已提交的 frontend
清理事实并正确传播 Diverged；正常完成的参数仍按原有顺序交付且执行 MapPut。

## 2. 背景

动态 mixed return/Int 在 unit 已成功，single 在 receiver reservation 的控制退出清理处
MissingFact。双 return 与直接 return 的 single 先遇到同一缺项；unit 将 Diverged 要求为
Value，报整个 put 的 MissingFact。失败发生于 frontend 之后的 lowering，不是运行时内存
故障证据。本分支从 actual origin/main `6667635b` 建立，与 SPEC-0289 / PR72 独立。

## 3. 范围与需求

- single Map.put 根据实际 receiver reservation 登记 pending call cleanup identity；控制退出
  消费 frontend loan-end/drop facts，正常调用完成后退休该 identity。
- single/unit key 和 value lowering 遇到 Diverged 立即返回，不恢复失效 operand、不产生
  MapPut、不求值后续参数或调用后续语句。
- mixed 分支的正常路径保持 carry/pending owner 重绑定；unit 的已求值 named MoveOnly key
  也登记参数 temporary identity，以消费 frontend 的 return 前缀 drop 事实。
- 先用合法正常源码记录红测，再实施最小修复；补直接 SSA 与未改写 native object/link/run。

## 4. 非目标

不改变 frontend、Guide、LLVM/runtime ABI；不扩 Elvis、普通函数调用 capability 或一般 CFG；
break/continue 的 loop 出口临时值恢复仍是独立审计候选，不属于本片 return 验收；
不泛化重构其他 Map 操作。本地不运行故障注入、校准、LLVM 改写、counter、sanitizer 或聚合
门禁；这些既有完整矩阵留到 CI，未执行前不声称通过。普通 native 输出不证明堆内存全闭合。

## 5. 验收标准

- [x] mixed return/Int、双 return、直接 return 在 key/value 的两条入口已记录原有通过/失败边界。
- [x] 同一选择修复后通过，typed SSA/verifier 与 LLVM 正常生成；完整退出不生成 MapPut。
- [x] 正常 native 覆盖动态真假分支、求值顺序/未执行后缀及资源析构输出。
- [x] 动态 if、Boolean when、短路条件的无退出 Map.put 对照保持成功。
- [x] fmt、受影响 crate clippy、文档与尺寸静态检查完成；逐项记实际命中数与未运行项。
- [x] Architecture 已更新本片 return 已验证事实，不扩称一般控制退出。
- [ ] 独立发布后记录 exact head 双宿主 CI，再归档。

## 6. 技术方案与边界

仅调整 `ssa/lower_frontend/map.rs` 与 `ssa/unit_lower/map.rs` 的 Map.put 接线；沿用现有
pending loan / operand slots、分支快照和清理机制，不从 AST 重推借用事实。不存在 SSA loan
实体的 receiver reservation 使用现有 None pending 表示法，不伪造 BorrowBegin/BorrowEnd。

## 7. 实施计划

1. [x] Spec/inventory 与红测 → 验证 §10 的首次失败。
2. [x] 最小实现及直接回归 → 验证同一测试选择与正常 native。
3. [x] 当前事实与本地账本 → 验证文档、格式、尺寸、clippy。
4. [ ] 发布后 exact head CI → 依证据归档，不提前宣称 CI 通过。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | Spec 与正常源码红测 | `test(codegen): expose Map.put control exits (SPEC-0294)` |
| 2 | Map.put 最小修复与正常 native | `fix(codegen): preserve Map.put control exits (SPEC-0294)` |
| 3 | grouped key 正常回归、本地验收记录与当前事实 | `test(codegen): verify grouped Map.put return cleanup (SPEC-0294)` |

## 9. 未决问题

无语义未决项。完成状态依赖独立 exact-head CI，不复用 PR72 的成功覆盖本分支。

## 10. 验证记录

| 验收项 / 命令（目标与过滤器） | 结果（实际测试数） | 未运行原因 / 复用证据 |
|---|---|---|
| `cargo test --locked -p lang-codegen --lib map_put_control_exit -- --nocapture`（生产代码未改） | 红测：20 个，5 passed / 15 failed / 0 ignored；1218 filtered | single 9 SSA + native mixed 均 MissingFact；unit 4 全 return SSA + native both MissingFact；unit 5 mixed SSA 通过。native unit 在首次全退出前已运行 4 个 mixed 源码。 |
| `python3 scripts/check_docs.py`；`git diff --check` | passed；610 Markdown | 新 Spec/inventory 与依赖图同步 |
| 同一 20-test 选择（receiver/Diverged 初修） | 19 passed / 1 failed | unit named String key 的 return drop 缺 temporary identity；保留失败，不以 Int 成功替代验收。 |
| 同一 20-test 选择（补齐 unit MoveOnly key 前缀） | 20 passed / 0 failed / 0 ignored；1218 filtered | 18 SSA；2 native tests 各真实 object/link/run 14 份正常源码；无 LLVM 改写/计数。 |
| 同一 20-test 选择（再补 grouped `(key)`） | 20 passed / 0 failed / 0 ignored；1218 filtered | 两 native tests 各 15 份源码，共 30 次正常 object/link/run；named/temporary/grouped String key 均通过。 |
| `cargo test --locked -p lang-codegen --lib map_operands_follow_argument_control_flow -- --nocapture` | 3 passed / 0 failed | single/unit SSA 与正常 native；未执行 counter |
| `cargo test --locked -p lang-codegen --lib map_mutation_collects_drop_glue_before_abort -- --nocapture` | 3 passed / 0 failed | 正常源码 `error` 的既定 Abort，不改写产物或注入故障 |
| `cargo test --locked -p lang-codegen --lib native_map_put_preserves_copyable_key_and_value_bindings -- --nocapture` | 1 passed / 0 failed | 正常 native，保留 Copyable 与下标赋值行为 |
| `cargo test --locked -p lang-codegen --lib map_nullable_value_owned_remove_remains_unsupported -- --nocapture` | 2 passed / 0 failed | 保留原有结构化 UnsupportedNode/Span 边界 |
| `cargo fmt --all -- --check` | passed | grouped case 后重跑 |
| `cargo clippy --locked -p lang-codegen --all-targets -- -D warnings` | passed | grouped case 后重跑；不等于执行全部 Rust tests |
| `python3 -m unittest scripts.tests.test_check_docs -q` | 37 passed | 文档 checker inventory 的定向静态合同 |
| `python3 scripts/check_docs.py`；`git diff --check` | passed；610 Markdown | 修正 Map Architecture 页增长越过 200 行，最终仍 200 行；不改其他历史事实 |
| `python3 scripts/check_rust_sizes.py --base origin/main` | passed；1027 手写 Rust，38 份既有超限 | 本次 5 份变更 Rust 分别 450/402/186/444/682 行；无新增/增长例外 |
| 聚合、counter、故障注入、校准、sanitizer、双宿主 CI | 未执行 | 本地授权边界；由独立 PR CI 验证 |

本地环境：Linux x86_64，Rust 1.96.0、LLVM 21.1.8；先 `source ../toolchains/env.sh`，
Cargo 复用 `CARGO_TARGET_DIR=../koven-pr69-fixes/target`，全程串行。新增 20 + 既有 9 个
不同测试通过；重复红绿运行不重复计作新增覆盖。原生运行观察分支/顺序/Resource 析构，
不替代 allocation/free 计数或 sanitizer。独立只读审阅已核对 exact reservation identity、
分支重绑定、Diverged 返回和 named key temporary 清理；未把审阅当成 CI 执行证据。
