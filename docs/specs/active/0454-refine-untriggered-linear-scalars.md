# SPEC-0454: 无家族触发的直线标量函数进入细化层

> **性质**：实施 Spec · **状态**：in-progress · **读取时机**：推进 E3 简单标量默认路由时 · **唯一真源**：本次切片范围与验收；语义从属现行 Guide

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P1-E3-0454` |
| 所属 Phase | Phase 4，消费既有 Phase 2/3 事实 |
| 语言规范 | 现行 [Guide v0.47](../../guide/README.md)，[表达式与运算符](../../guide/04-expressions-operators.md) |
| 批准依据 | 用户在 E3 差距审计后要求继续推进；依先前建议先实施简单标量路由，限下述首片 |
| 前置 Spec | 无；复用已落地主干的 validated Unit 与 refine 基础设施 |
| 前置 ADR | [ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md) |
| 阻塞项 | 无；官方双宿主 CI 与 PR 验收在实现后独立核验 |
| 影响范围 | `lang-codegen` 的 subset 路由、最近测试与架构快照 |
| 语言语义变更 | 否 |

## 1. Goal

完成 SPEC-0454：无原有迁移触发的、已被完整 subset 验证的直线 Boolean/整数函数，
经正常 Unit 入口改走 refine；SSA/LLVM 与 native 保持原语义，原 caller/ownership 边界不扩张。

## 2. 背景与输入

从 main `51a3aad36bd754c268a05c26c8dd0b80940286cb` 新建
`feature/spec-0454`，不混入未完成的 0451–0453 薄壳退役。
[审计输入](https://github.com/Halckon/Koven/blob/55a3a0fc24a8ac3df312822fb72515545a6ddf39/docs/architecture/ownership-refinement-gaps.md)
的 A1 已证实：`map_cross_function_routing_is_isolated` 的简单标量 helper lowering 成功，
但缺少最终 migrated/nullable/call trigger，仍进入 Compatibility。

本片只缩小这个已证实缺口，不声称移除全部 Compatibility、完成 ADR-0032 或其余 E3。

## 3. 范围

新增只读触发判定，且必须先满足原 signature/body/drop/loop 全部预检：

- 非泛型、非 StaticSelf 的普通具名函数；无 receiver、borrow/carrier 返回或 lambda body。
- 参数限既有 Boolean/八种整数；保留原 Value 与 Copyable Borrow 读取合同，不接受 Inout。
  返回类型限这些标量或 Unit；不纳入 Char、浮点、String、nullable、聚合或 callable 签名。
- 表达式体或直线 block：标量 literal/name/Group、既有 prefix、非短路标量 binary，
  普通 val/var、简单局部赋值与 return。每个值表达式仍经现有 typed/ownership subset 验证。
- 直线限制指没有源码分支、循环与调用；checked 运算内部既有 Abort CFG 不因此禁用或改写。
- 声明/输入顺序、无关 sibling、single 与 project 不改变该函数路由或执行结果。
  未调用 sibling 的局部变量类型不得进入当前 callable 的 type demands；保留既有 refine 必需的
  Boolean 控制类型和 Unit storage，不把它们误归因于未调用源码。

## 4. 非目标

- 不取消全局迁移触发门，不让所有 subset 支持构造自动进入 refine。
- 不扩张 receiver caller、普通借用运输/receipt、借用返回、闭包、泛型实例或循环权限。
- 不新增 if/when/短路触发，不迁移 owned nullable when、inline nullable 或 place/loan proof。
- 不新增 AST/typed/ownership 事实、SSA operation、LLVM ABI、依赖或公开 API。
- 不删除旧函数体基础设施、薄壳或测试，不把本片当作历史 Spec 自动验收/归档。

## 5. 验收标准

- [x] R1：先证明普通入口的目标标量函数当前不进入 refine；新增期望测试因路由断言失败。
- [x] R2：Boolean/八种整数、表达式/局部 val-var/赋值/return/Unit 空体正例进入 refine；SSA/LLVM 完整验证。
- [x] R3：有调用、源码分支/短路/while、非标量签名及泛型等无原触发见证保持旧路由/原行为；receiver 四邻居不扩张。
- [x] R4：single 与双文件 project 的标量结果、正逆输入顺序和无关 sibling 隔离验证；实际 native 成功与 checked Abort 保持。
- [x] R5：最近 subset/Map sibling/验证门/owner receipt/LLVM 契约在默认与 feature 下通过；CI-only 不本地伪造。
- [x] R6：fmt、双配置 all-target Clippy、适用 workspace check、docs、依赖、尺寸与空白门禁通过；独立审查具体范围。
- [x] R7：代码候选 `44de27fb` / base `51a3aad3` 的独立 PR 双宿主 CI 已验收；账本提交的过滤/复用另行核对，不冒充重跑，批准前不合并。

## 6. 技术方案

在现有 `refine/subset/scalar_text.rs` 增加保守的直线形状筛选，只作为最终路由 OR 项。
它不替代现有 Checker，不试跑 lowering，不查询 observation/receipt 来决定路由。
完整 signature、body、drop 与 loop 条件保持在该 OR 之外。

扩大回归发现既有 type demands 按整份 source 收集全部 body-symbol 类型，会让新迁入的
标量函数被未调用 sibling 的 String/Long 局部变量污染。保留其独立失败见证，在既有 field/local
职责中按同 source identity 和 callable span 限定局部类型需求；不删活跃 binding、capture 或签名需求。
refine 原有 Boolean/Unit 基础类型和显式 live-value 边参数则属于表示变化：最近测试保留
checked failure→Abort、正常继续、逐参数类型/arity 以及原实源/native 输出，不强制空 CFG 边。

测试挂在原 `refine_subset_tests` 的标量职责子模块，不向接近 1000 行的父模块塞入测试正文。
既有 `no nullable` 拒绝条目移成精确正例；Map sibling 对标量 helper 的期望随批准范围变更，
其余实源、SSA/LLVM、错误 kind/span 及负控不删减。

## 7. 顺序实施计划

1. [x] 建立本合同与库存/roadmap；添加路由正反例 → R1 有真实路由失败；实现前三项边界 3/3。
2. [x] 最小触发实现、保留原实源并适配直接受影响的路由/表示断言，修正类型需求污染 → R2–R4。
3. [x] 定向共享契约、双 Clippy 与结构门禁、独立审查 → R5–R6。
4. [x] 更新 Architecture 与本账本，普通推送独立 Draft PR #424 → 代码候选 R7 通过；保持 Draft/in-progress，不自动合并或归档。

## 8. 提交边界

| 顺序 | 范围 | 提交 |
|---|---|---|
| 1 | 可构建的代码、测试、当前事实与本片合同 | `feat(codegen): refine untriggered linear scalars (SPEC-0454)` |
| 2 | 必要的审查/CI 修正与真实验收 | 按具体发现独立记录，不强行归档 |

## 9. 未决问题

范围内无；完整统一路由、owned nullable when、receiver 推广与全部旧 body 退休由后继 Spec 承接。

## 10. 验证记录

一次选择的命令、输入与结果在此汇总；原始日志不入库。Spec 保持 in-progress，CI/归档不预填。
首轮本地代码输入为 main `51a3aad3` 上的实现提交 `ba8059da`，`crates/lang-codegen` tree
`ae25f4a44dbb2b097cf44cbe7a77c84f372a8b43`。后续 CI 两项测试适配后的同路径 tree 为
`b35042fef1cf7880adb6c373c8106e8653653a6c`，仅两测试文件变化，生产代码与首轮一致；
frontend/manifest/toolchain 无改动。
宿主 macOS AArch64，Rust 1.96.0、LLVM21；复用隔离 target 且 Cargo 门禁顺序执行。

| 验收项 | 输入、目标/命令 | 实际结果与边界 |
|---|---|---|
| R1 | main `51a3aad3` + 新测试、生产未改 | 1 项因 `fun f(): Int = 1` 未路由 refine 失败；编译成功。边界 3/3；初版泛型夹具语法误写先修正，未算产品失败 |
| R2–R4 | `refine_subset_tests::scalar` 9 项 + 前轮 6 失败目标 | 定向 15/15、无忽略；之后均纳入最终双配置回归。新模块含 16 个路由正例、7 个成功能力负控、宽度/Boolean native、4 个 project link/run cell、3 个 checked SIGABRT、mixed refine/legacy 执行及 String/Long 局部污染见证 |
| R3/R5 首失败 | 默认 618 项 | 563 passed / 6 failed / 49 ignored，非通过。6 项涉及全局路由断言、checked 边参数及基础类型需求；另补独立 String/Long 需求污染失败测试后修正 |
| R3/R5 最终 | `cargo test --locked -p lang-codegen --lib [--features loan-observation-test-support] --` 加下述 filters | 默认 **571 passed / 0 failed / 49 ignored**；feature **599 / 0 / 53**，退出码均 0。忽略分别为 48/52 项既有 CI-only 与 1 项 debugserver 权限限制，未新增 ignore |
| R5 CI-only 库存 | `python3 scripts/check_borrow_faults.py --list-only` | 62 项库存核验，退出 0；仅 compile/list，未执行故障测试 |
| R6 Rust | `cargo fmt --all -- --check`；`cargo check --locked --workspace --all-targets`；两配置 `cargo clippy --locked --workspace --all-targets [--features loan-observation-test-support] -- -D warnings` | 全部退出 0；未声明另跑 feature check |
| R6 结构 | docs/dependencies/size gates、`git diff --check`；size policy 47 tests | 全部通过；2294 个手写 Rust，36 个既有超千行文件，未增加尺寸欠账，全部修改的 Rust 文件在 1000 行内 |
| R6 独立审查 | 最终逻辑只读复审，检查路由、local demands 的 source/Span/capture/substitution 及旧断言适配 | 未发现具体问题；审查不代替动态测试或官方 CI |
| R5 追加消费者 | `ba8059da`，`ssa::refine_closure_tests::` | 默认/feature 各 **15/15，无忽略**，退出 0；不计入 620/652 selection 数字 |
| R7 首跑失败 | [PR #424 run 43565910184](https://github.com/Halckon/Koven/actions/runs/43565910184)，head `ba8059da` / base `51a3aad3` | 两宿主 frontend 均 848 passed / 0 failed / 12 ignored；codegen Linux **3223 / 2 / 55**，macOS **3220 / 2 / 56**。两项失败见下文；Preview producer/consumer 均 skipped，非通过。push 43565752643 成功不代替 PR |
| R7 失败复现 | 同实现，两个 `ssa::lower_frontend_tests` 目标 | 本地 **0 passed / 2 failed**，退出 101；精确复现与 CI 相同断言，不算修复前通过 |
| R5 CI 修复重验 | `b35042fe…`，完整 `ssa::lower_frontend_tests::` | 默认/feature 各 **287/287，无忽略**，退出 0；原源码、kind/span、身份与 native/确定性检查保留 |
| R5 CI 修复直接消费者 | 同输入，`ssa::refine_subset_tests::scalar::`、`ssa::unit_lower_scalar_tests::`、`ssa::verify_scalar_tests::` | 两配置各 **24/24，无忽略**，含入口 Borrow 跨 CFG 不传 loan 参数的原 verifier 合同；不与其他选择合并计数 |
| R6 CI 修复门禁 | 同输入，fmt、workspace all-target check、双 workspace all-target Clippy、尺寸/依赖/空白；两文件独立只读复审 | 门禁均退出 0，复审无具体发现；root boundary 文件 960 行，未增加尺寸欠账；docs 在账本更新后重验 |
| R7 修复候选 | [PR #424 run 43573023199](https://github.com/Halckon/Koven/actions/runs/43573023199)，head `44de27fbdc5874599d0806d8f541376f74aab8a7` / base `51a3aad36bd754c268a05c26c8dd0b80940286cb`；push 43573019227 | PR 与 push 均 success。双宿主 Check/Clippy、Targeted Tests、Preview producer 与独立 consumer 均实际 success；Editor lexical 按改动过滤 skipped，不算执行 |
| R7 普通与专用 CI | 同 run 两宿主 | frontend 各 **848 / 0 / 12**；codegen Linux **3225 / 0 / 55**，macOS **3222 / 0 / 56**（passed/failed/ignored）。各宿主 62 个 borrow fault **逐个实际执行一次**；独立 parent/child receipt、conformance、generated resource 均 success，Linux native sanitizer success；忽略项不当作执行 |
| R7 Preview 来源 | 同 run/attempt 1 的 x86_64 Linux 与 AArch64 macOS | producer 包 14105774656 / 14107030417，独立 consumer 证据 14106592085 / 14107260389。安装/运行、中文路径管理、实际 source_commit=`44de27fb…` 匹配；损坏 checksum 被拒且 current 不变是预期负控，不是流水线失败 |
| R7 账本后续 | 本行所在仅文档提交 | 不改 `b35042fe…` codegen tree；核对该提交的结构检查及官方过滤/复用，不把旧候选绿灯无条件推广到后续代码 |

首轮扩大回归 filters：`ssa::refine_subset_tests::`、`ssa::refine_tests::`、`ssa::unit_lower`、
`llvm::`、`receipt`、`borrow_return_transport`、`for_atomic`。默认选择 620、feature 选择 652，
包含忽略项；不是完整 codegen suite。未运行 frontend 全量、完整 codegen lib、本地 CI-only
故障注入、Linux sanitizer 或 0360 新增动态计数/native；官方完整门禁仍由 R7 核验。

新测试构造期间的泛型语法/数值 literal expected-type 及过强跨源码 IR/跨分析对象等值断言
均先纠正；同源正逆输入仍逐字节比较 SSA/LLVM，新增 sibling 以不变路由和实际执行结果验收。
这些新夹具问题没有被计成产品回归，原有源码、拒绝 kind/span、receipt 负控保持。

CI 两失败为 `declarative_type_roots_do_not_enter_the_scalar_instance_graph` 和
`lowers_real_scalar_expression_functions_through_verified_ssa`：前者的空类型表断言改为与
独立分析、正常 Unit lowering 的无声明同形 entry 完整类型表相等，仍禁止 unused 声明
引入任意类型/布局，保留原五根、零动作、源身份、SSA/LLVM 与名称隔离检查。
后者按 ADR-0016 及既有 verifier 合同，callee entry loan 支配全 CFG、正常返回隐式结束，
不要求冗余 loan edge 参数；新断言逐个核对 Shared loan 读取身份/次数/顺序、add 与
零减输入的数据流、精确 checked value 运输以及原 failure→Abort/continue、arity/type 和来源。
适配阶段基线比较初版误用 `ty.kind` 曾编译失败，修正为实际 `Vec<SsaTypeKind>` 完整比较后才记通过。
不为旧表示断言回退路由、改参数模式、篡改 fixture 或修改生产 lowering。

### PR #424 P2：恢复 refine 调用 Compatibility 的 native 覆盖

审阅后的修复输入为 `ed632b8e0e6a548a93c9e34f8024580e4f5550c4`。
原 mixed witness 的 main 仍走 Compatibility，只覆盖该方向调用 linear。
先增加 main 必须 refine 的断言：真实 **0 passed / 1 failed，exit 101**，
观测列表仅 `koven.linear.d0`，没有把覆盖缺口称为生产代码缺陷。

随后在原 main 中加入整型根 `replace`，检查其旧值/新值，保持调用 linear 与
branching。现在明确断言 main/linear 进入 refine、branching 保持 Compatibility，
保留实际 native 状态、完整 stdout `mixed-linear-ok\\n` 和空 stderr 断言。
标量组默认/feature 均 **9 passed / 0 failed / 0 ignored，exit 0**，无生产代码改动。
以上只验收本次覆盖修复；新 head 官方 CI 与旧已验收候选分别归因，不无条件沿用旧绿灯。
本轮 fmt、docs、尺寸、空白及默认/feature 的 lang-codegen all-target Clippy
`-D warnings` 均通过；未运行无关全量，官方新候选 CI 不预填。
