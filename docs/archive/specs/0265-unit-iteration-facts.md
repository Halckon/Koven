# SPEC-0265: unit 顺序迭代前端事实

> **性质**：有界变更合同 · **状态**：done · **读取时机**：追溯 M1A A4 交付时 · **唯一真源**：本页

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P3-265` |
| 所属 Phase | Phase 2–3；现行语义实现 |
| 语言规范 | [Guide v0.40 §37](../../guide/12-collections-destructuring.md#37-借用式顺序容器迭代-provider) |
| 批准依据 | 用户于2026-10-04要求持续实施M1A；按实际依赖独立交付A4 |
| 前置 Spec | SPEC-0182、SPEC-0197、SPEC-0198、SPEC-0263 |
| 前置 ADR | 无 |
| 影响范围 | `lang-frontend` unit typed/ownership事实、有效交接与定向测试 |
| 语言语义变更 | 否 |

## 1. Goal 与范围

初始以 `11acf62` 为主干基线，最终整合至 `8408d70`，承接[M1A](../../development/multifile-program-spec-draft.md) A4：
unit `for` 发布同轮、source-qualified typed provider/binding/projection与ownership退出计划。
Array/List/MutableList均支持Name、Discard及concrete value-class borrowed解构；
owned、Borrow、Inout、field、temporary source均由前端按现行规则检查。
公开事实可验证source求值身份、Shared source/element loan、temporary延寿及有序退出，
错误、Deferred和失败候选不得泄漏半份可执行事实。

原基线typed binding为LoopSource，ownership把for并入普通loop，drop planner没有provider
生命周期；本片已补齐这些前端事实。复用single的算法及现有unit place、类型替换、trial、callable/flow与身份链，
不复制完整driver，不以AST代替所有权事实。

## 2. 交接合同

typed计划以UnitStatementId查询，source、binding及projection均保留source identity。
body检查前binding类型已知；仅intrinsic provider有效，错误Span与现行诊断一致。
ownership建立独立iteration source/element身份，不伪装为同步call；source覆盖body/backedge，
temporary hidden owner活到FinishProvider与EndSource之后，binding仅Borrow。

正常/continue先清理本轮owner及element派生loan，继续保留source；break/exhaustion再
FinishProvider、EndSource、temporary drop。return先交付operand，再清理当前callable内
离开的provider和外围scope；最近loop限定break/continue；Abort没有cleanup edge。
源表达式自身return/abort发生于AcquireProvider之前，不得发布伪造的退出。
ordinary与constant capability保持隔离，均核对同轮完整事实及错误原子性。

## 3. 非目标与剩余

不实现A5 unit SSA/native；Inout/field的native支持继续按§37.4延后。现有unit for后端
UnsupportedSource边界保持，类型精确化已将旧Boolean source/Deferred预期更新为有效Array source/Int binding。A3字段Borrow已由SPEC-0264交付；A5–A10
及三文件CLI/两宿主总退出条件继续开放。无新provider、iterator值或语言语义。
不扩大既有closure动态实例运输边界，不以内部候选事实宣称公开可执行。

## 4. 单一验收账本

| ID | 合同 | 实际结果 |
|---|---|---|
| T1 | baseline有效红测：binding精确类型、L0159及source loan冲突 | 已先运行3项有效红测：typed binding精确类型/L0159两项失败，ownership source move+break缺L0135一项失败；最小实现后绿。 |
| T2 | 三provider/五source、Name/Discard/跨文件generic分量、源一次身份；错误Span/poison/重载回滚/inputs置换 | 新增typed 7/7绿；三provider×五source矩阵、跨文件generic分量/discard、source唯一call、完整Span、错误原子性、overload trial与inputs置换均有断言。 |
| O1 | source冲突、MoveOnly交付/捕获拒绝、shared嵌套与Inout/field前端正反例 | 新增ownership 18/18绿；三provider×五source、this字段/临时对象字段/共享capture源能力、移动/Inout/捕获拒绝和嵌套shared正例覆盖。 |
| O2 | 有序正常/continue/break/return/exhaustion；Abort无清理；最近callable、source自身分歧、未后继读的source保活 | 18项ownership中验证正常/continue/break/return/exhaustion、独立Abort、最近lambda/while、source求值中return及已求值索引backing、named source保活、call/receiver/capture loan先结束。 |
| V1 | ordinary/constant有效交接、同轮/schema校验、错误不发布；破坏source/binding/退出顺序的反例 | constant新增2/2绿；私有iteration筛选13/13绿（含旧single 5项、unit schema mutation 7项、constant缺计划1项）。校验外→内错序、额外/遗漏provider与binding、foreign source、body/temporary owner与loan先后、普通/constant身份隔离。 |
| R1 | multifile type/ownership、single type/ownership iteration、constant与直接下游gate定向回归 | multifile typed 114/114、ownership 95/95、constant 22/22；single type_iteration 14/14、ownership_iteration 184/184；codegen既有unit-for拒绝原子性1/1，全部0failed/ignored。 |
| E1 | fmt、受影响Clippy、workspace check、docs/inventory/DAG、尺寸/diff、独立review | 独立fresh review及修复复审通过；frontend --all-targets Clippy -D warnings、workspace check、fmt --check、docs 507、Python门禁单测84、DAG、尺寸（base 8408d70）与diff均通过。 |

## 5. 执行与验证

按T1红测→typed→ownership/dataflow→ordered cleanup→schema/capability→定向回归实施。
Cargo门禁串行使用共享target，必须等待任务间交接，不并发运行：

```sh
# 以下每个Cargo命令均串行，统一环境：
# CARGO_TARGET_DIR=/Users/ckfei/.codex/worktrees/7e7c/Koven/target
# LLVM_SYS_211_PREFIX=/opt/homebrew/opt/llvm@21
cargo test --locked --offline -p lang-frontend --test multifile_type_checking --test multifile_ownership_checking --test multifile_constant_ownership --test type_iteration --test ownership_iteration --no-fail-fast
cargo test --locked --offline -p lang-frontend --lib iteration_
cargo test --locked --offline -p lang-frontend --test guide_litmus
cargo test --locked --offline -p lang-codegen --lib native::unit_tests::for_atomic::actual_for_rejections_are_atomic_before_reservation_and_llvm -- --exact
cargo clippy --locked --offline -p lang-frontend --all-targets -- -D warnings
cargo check --locked --offline --workspace
cargo fmt --all -- --check
python3 scripts/check_docs.py
python3 scripts/gen_spec_dag.py
python3 -m unittest scripts.tests.test_check_docs scripts.tests.test_check_rust_sizes
python3 scripts/check_rust_sizes.py --base origin/main
git diff --check
```

### 审阅与缺陷闭环

独立只读fresh review核完整producer/drop/schema/source权限，所有真实发现都有回归：
member receiver loan、临时closure capture、不可达lambda、grouped constant owner身份及
索引求值return五类实际遗漏均先有效红测再修复；schema还用独立mutation拒绝body owner
延后和loan晚于其owner释放。Named scope与pending形成时provider集合是独立义务证据，
不以完整动作副本作为顺序oracle。复审确认无新增确定缺陷；reviewer未运行Cargo，测试以本页实跑为准。

### 尺寸与交付边界

保留原baseline不变，仅登记有界例外：`compilation_unit.rs` 1116→1144；`dataflow.rs`
相对本次基线1188→1225（历史baseline1185）；`drop_planner.rs` 1435→1570；`bodies.rs`
1337→1358；`bodies/checker.rs` 1222→1226；constant测试入口1008→1011。
fresh review接受父模块必要接线，provider算法与schema已按职责独立；planner增长用于实际
控制点清理、owner义务及已验证的Index/canonicalization修复，后续按词法owner/call pending
职责拆分，不机械切片。所有新手写文件均低于1000行。

### 首轮远端反馈

[PR43 首轮 CI](https://github.com/Halckon/Koven/actions/runs/37196964995)关联实现提交
`9bedf67d3b2ce16a9787a48a5046ed9ee88b86db`；两宿主均在 Guide Litmus11 的旧 unit typed
快照失败：本片已闭合的 LoopSource/ControlJoin/Assignment 仍列为 Deferred。保留精确快照检查，
将预期更新为仅剩非值 callee `listOf` 的 Deferred(Call)，同步 Guide 一致性账本。
本地切换工作树后重新编译 frontend，先复现同一失败，再验证完整 `guide_litmus` 23/23 通过，
0 failed/ignored。首轮失败不算远端通过，修复后实现提交的验收见下节；归档提交仍需最终 CI 收口。

本片未运行frontend全量或A5 native for；原unit-for拒绝原子性仍通过，
不将前端交接表述为native支持。PR 按用户授权交付，CI 全绿后再归档合并。

## 6. 双宿主验收与归档

[PR43](https://github.com/Halckon/Koven/pull/43) 实现 head
`4a07caf8c58e6fa4e27f72ec43e606fee91a4312` 的
[CI37197790941](https://github.com/Halckon/Koven/actions/runs/37197790941) completed/success，
10/10 job 成功，包含 Ubuntu/macOS workspace check、Clippy 与 Targeted Tests。
两份原始日志逐名核实新增 typed 7 项、ownership 18 项、constant 2 项、schema mutation 7 项
全部实际执行并通过；Guide Litmus11 也通过，所在套件23/23、0失败/忽略/筛除。
Ubuntu 无 ignored；macOS 仅既有 LLDB 断点测试因 debugserver task-port 权限 ignored，
本片新增测试没有跳过。独立实现审阅、缺陷修复复审及本地验收已闭环。

本次归档只更新合同状态、索引/inventory、DAG 和进度链接；最终归档 head 仍须通过 PR CI
才合并，不能用以上实现 head 代替。M1A A4 已交付，A5–A10 与完整三文件 native/CLI
总验收仍开放；本片不声明 unit native for 已支持。
