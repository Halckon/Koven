# SPEC-0297: Map 查询与删除的 key return 清理

> **性质**：变更合同 · **状态**：done · **读取时机**：实施或验收 Map key 提前返回修复时 · **唯一真源**：本 Spec 的范围与验收账本

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P4-0297` |
| 所属 Phase | Phase 3 事实消费、Phase 4 SSA/native |
| 语言规范 | 现行 [Guide v0.43](../../guide/README.md)、[所有权](../../guide/10-ownership-borrowing-drop.md) |
| 批准依据 | 2026-10-11 已确认0297优先；2026-10-12 PR77合并后独立分支集成，只激活0297 |
| 前置 Spec | [SPEC-0294](../../archive/specs/0294-map-put-control-exit.md) |
| 前置 ADR | [ADR-0016](../../adr/accepted/0016-interprocedural-borrow-abi.md) |
| 关联 ADR | 无新增；若需要改变 ABI，先形成独立决定 |
| 阻塞项 | 本地定向检查与独立审查已通过；远端发布与生命周期另行闭环 |
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

- [x] 同一合法源码先通过 typed/ownership，再分别记录三操作 × 四种 return 的
  single/unit 结果、错误 kind/Span 或健康 SSA；已有成功用例保留成功。
- [x] mixed 两种动态输入保持正常 Map 结果或提前返回；双/直接 return 无 Map 操作、
  无后缀调用，verifier 与未改写 LLVM 均通过。
- [x] String/Resource 前缀正常 native 验证求值顺序、Borrow key 可复用、清理一次及
  未执行后缀；普通输出不冒充逐指针计数、sanitizer 或完整内存证明。
- [x] 原 put-return 与无退出三操作回归保持；非法 borrowed move 的诊断不放宽。
- [x] 按实际改动运行 fmt、受影响 crate clippy、尺寸与文档检查，更新 Architecture。

## 5. 实施与提交

本次从 actual main `289a8ad94d275f6ad497df2b12bdd134cebe4b81` 新建
`fix/spec-0297`；接收树为 `1e485f4cf59bb315d4294704cf9277ff53b68c1b`。
原实现基于 `8c6907c2`，恢复冻结树 `f880ce36d341bfe92e3757b83d3f8d42939c1012`
已逐字节复核；原七提交对象未恢复，本分支采用新的逻辑提交，不冒充原提交历史。
只接入0297合同、既有回归与有界生产修复，保留0302 fixture及0303治理、归档和proposal。
0298–0300继续draft；发布、PR、CI与合并按当时授权独立闭环。

## 6. 实施结果与集成验收

2026-10-12 接收上述main后，生产修复仍只涉及single/unit两个Map lowering helper，
与恢复冻结树逐字节相同。single将source `(call, argument)` cleanup obligation与SSA
owner/loan运输分开，owner、Borrow receiver和put共用前缀登记；不制造SSA loan，
不结束调用者的借用。两入口保留key的Value/Diverged区别，完整退出不生成目标操作。
透明Group只穿透查找既有named/Borrow binding；temporary/control仍lower原始外层
expression，保留其alias及AfterExpression/CallReturn身份。

下表红测输入为 `2484d6c65ac05defe1207a50968c608a8a34fdc7`，生产/native与工程检查输入为
`77b476facb80cb96ed7c4366be830308efb07b2f`；随后仅更新本账本与Architecture。
两份普通源码回归分别保存在 `ssa/lower_frontend_map_key_return_tests.rs` 和
`native/unit_map_key_return_tests.rs`。完整日志及恢复payload/bundle留仓外，不新增evidence dump。

| 验收项 | 实际命令或目标 | 结果与边界 |
|---|---|---|
| 新main真实facts与红测 | `cargo test --locked -p lang-codegen --lib map_key_return_tests -- --nocapture`，仅登记SSA回归时 | 24项：6通过、18失败、0 ignored；single三操作四形状在return Span报MissingFact，unit仅双/直接return在key Span报MissingFact；真实receiver LoanEnd/Begin身份相符 |
| SSA、原始LLVM与普通native | 同一命令，接入生产修复和native回归后 | 42项通过、0失败/ignored、1271 filtered：24 SSA/LLVM + 18 native组；full exit目标get/contains/remove和后缀DirectCall为0，mixed为1；single精确receiver/reservation与unit contract/drop facts均通过 |
| 正常结果与owner清理 | 上行native子集 | 两入口各42份源码，合计84次普通emit/link/run；Int/String、Resource value/prefix、Borrow/temporary/grouped receiver、key复用、missing/present-zero、无Map/无后缀对照；仅证明源码结果、求值及deinit顺序 |
| 原put return回归 | `cargo test --locked -p lang-codegen --lib map_put_control_exit` | 20通过，0失败/ignored，1293 filtered；含两入口普通native |
| 无退出CFG、既有Abort边界 | `cargo test --locked -p lang-codegen --lib map_operands_follow_argument_control_flow`；`cargo test --locked -p lang-codegen --lib map_mutation_collects_drop_glue_before_abort` | 各3通过，0失败/ignored，各1310 filtered；Abort只验证既有不展开行为 |
| Borrow key/非法move诊断 | `cargo test --locked -p lang-frontend --test ownership_map` | 10通过，0失败/ignored/filtered；包含Borrow key复用与非法borrowed move的L0133；frontend生产无改动 |
| 格式与静态检查 | `cargo fmt --all -- --check`；`cargo clippy --locked -p lang-codegen --all-targets -- -D warnings` | 均通过；Linux x86_64，Rust1.96.0、LLVM21.1.8，无cargo clean |
| 尺寸与文档 | `python3 scripts/check_rust_sizes.py --base 289a8ad94d275f6ad497df2b12bdd134cebe4b81`；`python3 scripts/check_docs.py`；`python3 -m unittest scripts.tests.test_check_docs`；`git diff --check` | 尺寸通过，1037手写Rust文件、38项历史超限；native/unit_tests.rs只加2行注册至1935行，低于既有2030例外且无新职责；文档650页、checker 41项及diff通过 |

准备阶段曾因测试模块登记位置发生编译错误，另一次过窄过滤器为零命中；均已纠正，
未计作红测或成功验收，原始记录保留。原实现时期的Group临时值四组红绿和恢复复验仍为
历史观察，不替代本表新main输入的检查。新增native/SSA文件分别476/305行，生产两文件
分别486/438行；既有大文件仅保留模块注册职责。

边界：get仍只取Copyable V，Resource不是Hashable key；Borrow receiver原生正例仅为
MutableMap<String, Int>参数的get/contains；temporary/grouped receiver也只覆盖get/contains，
Resource值覆盖owned contains/remove。以上不是完整笛卡尔积或逐指针/完整内存证明。
未运行macOS、远端CI、全库/聚合驱动、fault injection、校准、损坏IR、reducer或sanitizer；
不扩大普通调用、loop/Elvis及其他Guide能力。Spec保持active，发布、归档与合并未执行。

独立审查覆盖代码head `77b476fa` 和文档head `7b6f930aa109e831d341ef87e53421bd969dce68`，
无阻塞；独立重跑文档650页、`python3 -m unittest scripts.tests.test_check_docs scripts.tests.test_gen_spec_dag -q`
的42项（41 docs + 1 DAG）、尺寸及diff检查均通过。0302规则/fixture、0303归档/proposals、
Guide/compiler contracts及0298–0300保持原字节；本轮仅追加此收据与索引状态。接收基线main CI
[38219409371](https://github.com/Halckon/Koven/actions/runs/38219409371)仍待终态核验，不算0297的CI。

## 7. PR78 实现验收与归档收据

§1–6保留本地实施检查点原文，包括当时的active、未发布及未运行范围；本节记录后续交付，
不将历史观察改写为重跑，也不扩大return-only Goal。2026-10-12，PR78实现CI通过后有界归档。

| 项目 | 精确输入与实际结果 |
|---|---|
| PR / 实现head | [PR78](https://github.com/Halckon/Koven/pull/78)，`c72f4416dcb15f1d8ca534c4470f09c80282882e` |
| tree / base | tree `11a66ed52760ca7017e6459a761d6695b5b830eb`；base `289a8ad94d275f6ad497df2b12bdd134cebe4b81` |
| CI输入 | [run38222351928](https://github.com/Halckon/Koven/actions/runs/38222351928)，attempt1，pull_request；synthetic merge `2dc47f9d1d0e36658996c07aeb6e77df9325979e` |
| 实现CI终态 | 2026-10-12 08:29:06 UTC completed/success；15实体job成功，唯一Tree-sitter因`editors=false`合法skip；`reuse=false`，双宿主代码与preview实际运行 |
| macOS新增选择 | [Targeted Tests](https://github.com/Halckon/Koven/actions/runs/38222351928/job/114722271435)：42个unique `map_key_return_tests`逐名ok，24 SSA/LLVM + 18 native组，无新增ignored |
| Linux新增选择 | [Targeted Tests](https://github.com/Halckon/Koven/actions/runs/38222351928/job/114722271571)：同42项逐名ok，checkout同一synthetic merge，无新增ignored |
| 完整codegen目标 | Linux 1313通过/0失败/0ignored；macOS 1312通过/0失败/1旧ignored，唯一LLDB断点测试因CI缺debugserver task-port权限；不是0297跳过 |
| 其他必需门禁 | 直接依赖与变更检测、双宿主Check/严格Clippy、preview producer/独立consumer、Docs/Fmt/尺寸均成功；[CI Passed](https://github.com/Halckon/Koven/actions/runs/38222351928/job/114726297186)与[Code CI Evidence](https://github.com/Halckon/Koven/actions/runs/38222351928/job/114726297143)独立确认required jobs成功 |
| 接收基线 | PR77 main `289a8ad9` 的[run38219409371](https://github.com/Halckon/Koven/actions/runs/38219409371) attempt1于08:03:17 UTC成功；此项是接收基线证据，不是0297功能验收 |

Code CI Evidence v1精确绑定上述PR/head/base/synthetic，代码输入摘要为
`64f8f9989ff41333cda99981d5e009ebf6a5603c6abdbc2baa6fdbeaa99d8ff5`；无失败或重跑。
step层面仅macOS Targeted中的两条Linux-only sanitizer运行/保存步骤按`runner.os == Linux`
合法跳过；不能视作macOS动态检测通过。

远端六提交重新生成了metadata，不能冒称原本地SHA。发布前本地终点`681a5039`与远端
`c72f4416`的完整tree相同；本地动态输入`77b476fa`对应远端`47123eb455f1d4ec3c21b0c3bec0e36753230205`，
红测输入`2484d6c6`对应远端`9d66bf97aa5c4874a0626fb8e2c71f9b2efcdc53`。
原日志、完整发布映射与payload/bundle留仓外；本页只保留可追溯简要验收。

每宿主42个测试名证明所选24 SSA/LLVM与18 native组确实执行；两入口共84份源码程序是
夹具结构及本地普通emit/link/run口径，不伪称CI提供84份独立程序日志。普通结果与deinit
顺序不替代逐指针或完整内存证明，远端通用检测门也不自动成为0297专属内存证明。
Borrow/temporary receiver、Copyable get、Resource value/key边界继续按§3和§6限定；
不启用break/continue、Elvis、一般CFG或其他Guide能力，0298–0300仍draft。

本次归档只改变合同位置/状态、索引、账本链接、冻结inventory及生成DAG；生产、测试、
fixture、0302治理和0303归档/proposals保持原字节。归档静态检查及独立审查均通过：
文档650页、上述docs/DAG命令42项、暂存diff检查；独立核对CI身份/逐名命中/合法skip，
无阻塞。本次归档未运行Cargo或新增动态验收。
最终归档head CI、merge和actual-main CI尚未执行，本收据不将它们提前写成通过。
