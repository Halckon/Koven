# SPEC-0341: D10 统一 compilation-unit 检查与代码生成管线

> **性质**：分阶段实施合同 · **状态**：in-progress · **读取时机**：实施或验收单文件与 project 管线统一时 · **唯一真源**：本片目标、边界与验收；语言语义以 Guide 为准

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P4-0341` |
| 所属 Phase | driver 与 Phase 2–4 接线；各阶段消费真实同轮产物，不重推或伪造事实 |
| 语言规范 | 现行 [Guide v0.45](../../guide/README.md)、[单文件 Unit](../../guide/02-names-files-packages.md#单文件输入与共同名称解析)与[一致性边界](../../guide/15-conformance-and-staging.md#v045-启用边界) |
| 批准依据 | 2026-10-19 用户明确 D10 终态为单文件和 project 共用 compilation-unit 检查与代码生成；共享 lowering 核心只作迁移步骤；另批准 v0.45 import/root/入口规则与审后启用 |
| 基线 | 原 main `c0c5156a5f28d206b5a87986173f2bbf5bae6611`；迁移实验使用 actual `91c53596c126f90745acee24d783739d7cc9dcc8`、tree `dbb6b85be5eb8906be0d2721c1f78c946df779ad`；仍在 `feature/spec-0341-shared-lowering-core` 范围，不另开 PR |
| 前置 Spec | [SPEC-0255](../../archive/specs/0255-neutral-lowering-support.md)、[SPEC-0331](../../archive/specs/0331-map-borrowed-iteration.md)；旧验收不继承为本次迁移通过 |
| 前置 ADR | [ADR-0031](../../adr/accepted/0031-single-file-compilation-unit-adapter.md)局部扩展单文件 root adapter；既有 unit、entry、标准源与 ABI 决定保持 |
| 影响范围 | CLI 单文件/source-set/entry 适配、frontend 共同分析链、compilation-unit lowering、重复旧实现退役、直接测试及事实文档 |
| 语言语义变更 | 仅用户批准并在 Guide v0.45 启用的 import/root/阶段一致性边界；其余语义不变 |

## 1. Goal

D10 的终态是单文件和 project 共用 compilation-unit 名称、类型、所有权检查及代码生成
管线；入口只承担真实 source-set、身份、entry 选择和诊断呈现适配。独立 single checker/
`lower_frontend` 生产副本在能力与迁移验收完成后退役，不能把共同 helper 抽取当成终态。
共同语义的生产解释与构造责任只有一份，不用万能 context、巨 trait 或回调隐藏双份算法。

原审计 E2/E3 编号正文尚未定位，本 Spec 不自行替它们命名或宣布达标。统一路由与旧副本
退役本身，也不能证明既有 Unit 中类型×控制流特化已达到通用 lowering 目标；该目标需与
原审计正文另行对账。剩余共享抽取只按实际迁移需要进行，不延长旧 15 域或 carry B/C 清单。

## 2. 输入、兼容与不可退让边界

- 源码集合、root、import 与标准环境只遵守 Guide 的唯一规则；单文件不扫描兄弟文件，
  frontend 不读文件系统、不从 package 拼造路径。内建环境与标准源加载两模式相同，
  不把环境等价解释为自动导入，不额外提供 Pair/Result；既有 s01/s02 继续 L0080，E4 独立。
- 单文件 conventional main、显式零参数 `--entry`、private 可选及 argv 保留既有 driver
  合同；project 的 qualified selector 与可见性保留。一旦选中同一声明，后续语义一致。
- unit 的 sealed basic/const capability、同轮来源、DeclarationId、typed/ownership facts、
  planner 预算、验证与失败原子性保持；不能用伪造事实或跳过 validator 令旧 single 通过。
- 迁移既不假定 Unit 是能力超集，也不以能力交集作为终态。现有成功能力必须保全；Unit
  enum UnsupportedNode 等真实回退先修复并验证，不能隐藏成 entry 差异或共同拒绝。
- schema、语言诊断、backend capability 和内部 InvalidSsa 分层；错误 kind、Span、label、
  顺序与操作错误必须单独核验，不能以布尔接受性掩盖诊断回退。内部 ID 不作跨轮裸值比较。
- owner/loan/pending/receiver/Group alias、drop、正常及提前退出/Abort 运输以真实事实证明；
  保留末端 verifier。普通 native 成功不能替代释放次数、无泄漏或来源身份的资源证据。

## 3. 迁移步骤与回退清单

1. **规范与证据固定。** 完成 v0.45 文档治理与独审，正式启用单独记账。以 actual91c 和
   固定普通源码建立两模式差异表；22 个附件只作接线首证，扩展到全部经安全筛选的仓内
   普通目标，不把缺源码的 144 项矩阵视为可复跑集合。
2. **补齐真实回退。** 逐项选择已定位的普通失败，先保存原单文件成功与 Unit 红结果，
   再做最小责任修复。当前至少追踪 enum payloadless/payload Unit UnsupportedNode、
   附件普通程序的 Unit/SSA 缺口、诊断全文差异、import/root/标准源 driver 新合同。
   不将合成损坏、故障注入、计数校准或 sanitizer 混入普通云端目标。
3. **生产入口切换。** 由单文件 adapter 构造真实有限 unit，复用同一检查和 native 链；
   保留 entry、来源和操作失败身份。切换以本页验收为门槛，不能用永久回退旧 checker/
   lowerer 掩盖尚未统一的责任。
4. **删除重复实现。** 在实际调用方迁移后移除独立 single checker/lowering 副本及无消费者
   专用壳，保留有效普通回归；确认依赖方向与所有公开入口。旧共享 helper 仅在新链仍有
   真实消费者时保留，不为满足旧计划继续制造层次。
5. **交付审计。** 固定输入/环境与选中声明，审核管线唯一性、能力保全、诊断/运行结果和
   资源证据，完成工程门及精确头双宿主 CI。通用 lowering 尚未对账的目标明确保持开放。

每片保持可独立审阅和回退。回退是恢复上一已验证提交与记录未完成缺口，不能静默更改
源码集合、降低期望、删测试或切回双管线而仍称统一完成。新增长期协议另行 ADR。

## 4. 模块与依赖

driver 负责 source descriptor、受信任标准源授权、entry 选择与诊断输出；frontend 负责
语言分析，codegen 只消费 validated facts。保持 unit→旧 single 禁边、support 不依赖 model
的既有合同及 IR core 单向边界；实际新依赖进入现有生产扫描，不用模块改名逃出门禁。
无需新的 crate、宽 trait 或公开 capability；确需新增 API 时先给直接 caller 与最窄可见度。

## 5. 非目标与历史计划

- 不自动关闭 0291、0333–0340 或 E4，不新增批准范围之外的语言规则、标准包、prelude、ABI、并发或依赖系统。
- 不以净行数、单次耗时或完全同形 SSA 证明结构/性能/语义；不把阶段接线当通用算法已达标。
- §7 及 §9 起的历史实施与验收保留原义。其旧 A–E、carry B/C/D 未来任务安排由本次
  §1–6 取代，不再作为必须继续抽取 helper 的承诺；已发生的成功、误选和失败不删除。
- 本地 B2 `505eb20583457271a33401b12424fb455d86f87c` / tree
  `c274c3e8d0c669baa15407cb1ac16d04e26b6f4e` 的 check exit101（E0282/E0283）冻结且未推送；
  不修、不随文档发布，不能作为可用实验/迁移基线。文档安装须使用干净 actual91c 后继。

## 6. 验收标准

- [ ] v0.45 审查、全文保全与治理门通过，正式启用收据和实现进度分开记录。
- [ ] 单文件与 project 的生产调用链共用 compilation-unit 检查及代码生成；独立 single
  checker/`lower_frontend` 重复实现退役，真实适配逐项有来源及必要性。
- [ ] 固定且持续扩展的普通源码集合、环境与 entry 映射可追溯；两模式不一致数和
  InvalidSsa 数均为 0，原成功能力无回退。不能靠两边拒绝、删样例、筛除失败或重写目标凑零。
- [ ] import/root/标准加载及 entry 兼容普通正负例通过；诊断 kind/message/Span/label/order
  和阶段差异有明确核对。选中同一声明后无按模式分叉的语义解释。
- [ ] SSA/verifier、LLVM/native stdout/exit/stderr 与必要 owner/loan/drop 资源证据分别通过；
  不把基本 validated capability 自动升级为 const 或其他新增能力。
- [ ] 工程门、独立审查和精确交付头双宿主 CI 通过；Architecture 只更新实际已实现事实。
- [ ] 原审计的通用 lowering 目标已定位并独立对账；无证据时保持开放，不借统一路由宣称完成。

## 7. 验证账本与执行边界

遵守[测试与分层验收](../../development/testing.md#证据保留)。下表保留首片前的选择记录；B 的实际结果见§9，其余待执行项不是通过记录；
每片补精确输入 SHA/tree、命令/完整身份、实际命中数、结果、未运行原因与稳定来源。
现有测试先核实现再选择；禁止宽过滤器夹带合成损坏、计数/故障注入、校准或 sanitizer。
这些只在 CI，云端及用户本机均不执行；普通源码负例与普通 native 另账。

| 验收项 / 目标 | 执行位置与当前结果 |
|---|---|
| A0 原 main 基线，精确选择与误选更正见下 | 上述 c0c5156a/tree，9个普通目标各命中1/passed1/exit0；另1个含人工MissingFact的CI-only混合目标被误执行、passed1/exit0，单列保留，不计允许的普通验收；新映射未安装 |
| B 合法四路纯映射、八整数宽度、Char 与 String 字节/独立 owner | 云端普通定向；B首片实际结果见§9 |
| `ssa::lowering_entry_contract_tests::paired_const_string_uses_original_specialized_capability` | 普通源码；未在本候选编写任务运行 |
| 同模块 `paired_literals_preserve_bytes_group_origin_and_string_owner_kind`、`receiver_string_borrow_keeps_existing_single_unit_capability_difference` | 普通源码/既有能力差；待实际记录 |
| `ssa::lower_frontend_tests::constant_lowering_tests` 的 namespace、精确scalar/Char、独立String owner、unused constant 四个普通目标 | 逐名选择；待实际记录 |
| 对应 single/unit-const 普通 native 常量输出 | 先审测试体排除注入，按精确身份执行；待确定选择 |
| `recovery_stops_at_each_original_validation_boundary`、mixed/foreign analysis 与非法 materialization 配对 | 合成损坏/故障只在 CI；不得算作普通本地验证 |
| `production_lowering_dependencies_remain_directional` | 实际生产树只读检查；待实际记录；合成反向依赖负控只在 CI |
| C/D 各域配对与资源目标 | 按每片实际范围补足，当前未选择/未运行 |
| fmt、受影响 crate clippy、docs、尺寸与 diff；必要时直接消费者 check | 待实际执行；不机械追加 frontend 全量 |
| exact-head 双宿主 CI、独立审查与最终交付 | 待执行；旧 main/前置 Spec 的绿色不继承到实现头 |

A0 实际命令为 `cargo test --locked --offline -p lang-codegen --lib <exact> -- --exact --nocapture`，
2026-10-19 在云端按下列完整身份逐项串行运行；首编1m40s，随后复用同一测试二进制。
运行日志/退出码在仓外保留，后续交付另记稳定 CI 来源。下列9项为普通基线，均failed0/ignored0：

- `ssa::lowering_entry_contract_tests::paired_const_string_uses_original_specialized_capability`
- `ssa::lowering_entry_contract_tests::paired_literals_preserve_bytes_group_origin_and_string_owner_kind`
- `ssa::lowering_entry_contract_tests::paired_print_concat_and_drop_preserve_lifetimes`
- `ssa::lowering_entry_contract_tests::paired_generic_delivery_keeps_literal_contracts`
- `ssa::lowering_entry_contract_tests::paired_interpolation_rejection_keeps_exact_unsupported_span`
- `ssa::lowering_entry_contract_tests::receiver_string_borrow_keeps_existing_single_unit_capability_difference`
- `ssa::lowering_dependency_tests::production_lowering_dependencies_remain_directional`
- `ssa::lower_frontend_tests::constant_lowering_tests::exact_scalar_and_char_values_keep_their_ssa_types`
- `ssa::lower_frontend_tests::constant_lowering_tests::string_constant_reads_materialize_independent_literal_owners`

另实际误执行了 `ssa::unit_lower::constant::tests::namespace_constants_need_no_storage_initializer_or_closure_capture`，
命中1/passed1/failed0/ignored0/exit0。该测试先跑普通源码，末尾在基线 `unit_lower/constant.rs:247–264`
以 `None` 删除 constant ownership 调私有 driver，断言 `MissingFact`；属于含人工缺失事实的混合目标，
不能记为纯正常源码或声称没有注入。保留本次实际执行及退出码，不倒写成功为授权；后续本地整项排除，
只在 CI 验收。A0合计实际执行10项，其中普通9项、误纳CI-only混合项1项，不把两者合并报合规通过。

## 8. 提交与收尾

继续在现有 SPEC-0341 分支/PR 范围内提交，文档启用、每项回退修复、入口切换和重复实现
退役分别保留可审提交与实际收据。文档不得携带冻结 B2 失败生产树；每片只宣称真实执行的
验收。完整目标与门槛未满足时保持 in-progress，不归档 D10；交付/发布按用户最新授权。

## 9. B 首片实际验收（2026-10-19）

生产受验提交 `4a7bdc812a1cda456f2af49a6c97758ac514201a`，tree
`b3ca54fdf01fc6ae4ce1d8ab2936fe88b525900d`（基于合同提交 `0b8af21d`）。
五个 Rust 文件与独审候选逐字一致：四路映射移到同层私有模块，两 caller 保留原验证/类型/Span/
append/cleanup 顺序，新增实际生产依赖边界。没有新增接受能力或公开 API。

| 实际目标 | 结果 |
|---|---|
| A0 上列9个普通 exact 原样复验 | 各实际1 passed、0 failed、0 ignored、exit0 |
| `ssa::constant_materialization::tests::{boolean_plan_maps_to_scalar, integer_plan_preserves_exact_value, char_plan_preserves_unicode_scalar, string_plan_preserves_bytes_and_supplied_type}` | 分别使用完整 exact，4项各1 passed/exit0；从正常源码获得真实同轮plan，无手工缺事实 |
| `native_tests::associated_constants_reach_native_scalar_char_and_string_operations` | 1项1产物，完整stdout/成功exit/空stderr原断言通过 |
| `native::unit_tests::constants::all_constant_types_run_across_files_and_namespaces` | 1项6个namespace产物，完整stdout/成功exit/空stderr原断言通过 |
| `cargo fmt --all -- --check` | exit0 |
| `cargo clippy --locked --offline -p lang-codegen --all-targets -- -D warnings` | exit0，实际1m08s；编译检查不执行测试 |
| `python3 scripts/check_rust_sizes.py --base c0c5156a5f28d206b5a87986173f2bbf5bae6611`、`git diff --check` | 通过；1832手写Rust文件、历史38超限原样报告，无新增尺寸例外 |

以上15个测试逐条完整 `--exact --nocapture` 串行，均使用 `cargo test --locked --offline -p lang-codegen --lib`；
首项实际编译1m24s，其余复用。Rust/Cargo1.96.0，LLVM/Clang21.1.8，Linux x86_64。
原日志保留云端 `cloud-recovery-20261018/d10-readonly/logs/after-{1..15}.log` 和退出码；精简README保留选择与命令。
本轮15项经完整测试体/helper复核，不包含A0误纳项或其他合成损坏/故障注入/校准；A0历史不删除。
普通native输出不证明动态释放次数或无泄漏。新依赖门禁的合成禁边负控仅CI，当前尚无本PR CI结果。
本docs后继不冒称重新运行生产测试；B首片已验，A全域责任冻结及C/D/E仍开放，不归档D10。

## 10. C/D 整数表达式核心小片


本片接续已验B，收敛整数intrinsic事实校验与完整表达式流程；不代表C/D整体完成。
实现输入为本地 `365dc53697306e248911b9ff5d043808c43055c1`、tree
`f69193a139525edf59844f749b5c371ca4a8bcb3`。本节是后继小片合同，不改变§9验收输入。

### 唯一责任与适配

- `ssa::expression_core` 为SSA同层私有模块，共享原 `Unit/Value(ValueId)/Diverged` 三态，
  保留原derive；两入口私有导入原名。整数窄接口以原生 `ExpressionId/TypeId` 或
  `UnitExpressionId/UnitTypeId` 查询descriptor、表达式类型、builtin类型与AST receiver，
  另提供原receiver lowering、结果SSA type和scalar/checked提交端口；不引入宽可选context。
- 核心唯一执行：descriptor身份 → unit receiver同source → receiver/result类型身份 →
  既有AST receiver校验 → 八整数准入 → Invert同型 → receiver恰求值一次 → 三态分流 →
  结果SSA type → IntegerNot或CheckedIntegerConvert构造与提交。缺facts不按成员拼写补猜。
- single `lower_frontend/integer.rs` 先取原span；unit `unit_lower/integer.rs` 保留当前source
  与传入span。两边原dispatch和外层temporary/drop时点不动；receiver必须走原 `lower`。
  Diverged原样返回；Unit报operation span的MissingFact，不以通用require-value替代。
  结果type仅在receiver成功后查询；single缺映射仍UnsupportedNode，unit仍MissingFact。
- `checked.rs` 两份及其CFG不改：single linear/Map carry与unit pending/loan/Place重绑继续
  走原提交方法。该状态差异不由本片统一，也不能据此永久排除D后续CFG/cleanup收敛。
- 依赖门禁显式扫描新核心及子树，禁止core反向引用driver/planner/LLVM、IR/support引用
  core及core的SSA根glob/alias；既有禁边继续保持。既有integer纯helper不另复制。

### 小片验收标准（实际结果见§11）

- [x] 两个消费者均调用唯一核心；adapter只桥接事实和提交，没有重复kind分支或整段验证。
- [x] 八型Invert、两入口128种转换、一次receiver求值、live/pending String owner和既有
  非intrinsic源码成员拒绝边界保持；复用已全文审阅的ordinary exact，不用宽过滤器。
- [x] 新真实源码配对核逻辑source名、精确span、整数宽度/符号及definition→use；反转source
  建立/inputs顺序且加入同名其它source；unit-basic与unit-const正常wrapper保留能力门。
- [x] 原真实控制流native覆盖receiver条件return与正常求值路径，核完整stdout/exit/stderr；
  SSA/LLVM/native结果分账，不据普通native宣称动态资源计数或所有pending/Place组合已验。
- [x] MissingFact人工删除、foreign-analysis错配、非法SSA及新合成依赖禁边负控只在CI。
  `namespace_constants_need_no_storage_initializer_or_closure_capture`末尾人工None使完整exact
  继续排除普通选择；新增测试及helper先全文审阅，不把部分正常前缀当作完整ordinary资格。
- [ ] fmt、受影响crate clippy、docs、尺寸/diff、独立审查与精确头双宿主CI完成后另记实际账本。

本小片后仍按表达式→语句/控制转移→driver推进：优先审共同block顺序与非Diverged的
AfterStatement cleanup，保留unit ControlBody等能力差；再对CFG/owner/loan/pending/receiver/
iteration角色逐项建立状态合同，最后收敛driver骨架。此节不勾选§6的C/D/E完成项。

## 11. 整数小片实际验收（2026-10-19）

- 合同先提交 `43948dae`；实现 `79366871` 新增唯一整数流程与共享三态，两端薄适配保原生ID/查询次序/诊断，两个 `checked.rs` 字节未变。受验最终为测试修正提交 `1daebd9856e5458900660b9b7ff5fa1494516f29`、tree `b2ea55b8396701ad6c5657b31a9a4a27d9dc471a`；后继docs不冒称重跑。
- 既有11个ordinary基线及首轮复验均各实际1/exit0。新增2项首轮各命中1、failed1/exit101：普通配对误把默认Borrow实参期待成直接Parameter；const配对的other源写成UByte常量19，缺u后缀，在unit完整type前置报缺descriptor，尚未进入unit lowering。后者由Guide和原子撤回代码解释，原日志未打印具体诊断码。原红日志保留，不记生产回归。
- 测试单文件修正保原完整资格门：显式查询参数Loan→Read→RootPlace→SharedBorrow→call，操作tag区分同span生产者；other源改19u，raw diagnostics增加空集硬断。生产与FE均未修改。修后13个完整exact各1 passed/0 failed/0 ignored/exit0，首编1m31s；八型inv、两入口128转换、一次求值、live/pending owner、自然同名成员拒绝、真实生产依赖门及2个正常native均保持。
- 新 `ssa::integer_core_contract_tests::{paired_integer_intrinsics_preserve_source_identity_and_receiver_use, paired_const_integer_intrinsics_preserve_source_identity_and_receiver_use}` 分别完整exact通过：真实source建立/inputs正逆、同名other source、逻辑来源/字节Span、整数类型、Borrow及call→inv→convert全定义使用链；unit-basic/const分别走正常资格入口。const源码的basic拒绝保持。
- 同头另复验B首片15项中除已重复生产guard外的14个ordinary，均各1/exit0；完整函数/helper与原受验版本逐字核同。合计27个唯一ordinary exact，其中4个native测试覆盖9个正常产物，完整stdout/exit/stderr原硬断通过，不外推计数或泄漏。
- `cargo check --locked --offline -p lang-codegen --tests`：实现47.91s、测试修正39.11s均0；`cargo fmt --all -- --check`与`cargo clippy --locked --offline -p lang-codegen --all-targets -- -D warnings`均0，后者57.37s；尺寸门1834手写Rust/历史38超限通过，无新增例外。测试均沿§9同工具链和逐exact串行命令，原日志仅仓外 `d10-readonly/logs/integer-*`，README保精简命令与分类。
- 本片没有执行合成缺事实/非法SSA/故障注入/校准；§7既有误纳历史不删除。精确新远端头双宿主CI仍待验；§6的A/C/D/E均继续开放。下一责任是共同block/statement执行顺序及cleanup端口设计，不将checked/CFG长期差异视为D10完成。

## 12. C/D 普通 statement 顺序核心小片（待验）

本片设计输入为 `fbb50f06f393d17d4a427aa66d612c78f703b21b`；只收敛普通statement的
执行后cleanup与Unit序列两项责任，不代表共同CFG/cleanup执行器或D10整体完成。

- 新私有 `statement_core` 沿用 `LoweredValue` 与本地 `StatementId`，两个端口只调用原
  inner及原AfterStatement；不加source校验、不预取facts或Span、不扩宽整数接口。
- 核心统一inner成功后仅非Diverged发cleanup；序列按原顺序调用，遇Diverged或错误即停，
  全正常返回Unit。Value仍由statement wrapper原样返回；空容器由外层wrapper正常cleanup。
- single仍只接受Block/LambdaBody普通序列，unit另保ControlBody；Map专用inner绕行、
  control-tail/lambda尾值、原emit_drops/loop/branch/return/Abort时点与状态原样保留。
  Diverged不替换成terminator检查，不触及0321、0291或0333–0340的独立边界。
- 依赖门禁扫描新核心及子树，保原禁边；statement→expression仅复用结果类型，显式禁止
  expression→statement反向边，IR/support仍不读取核心；新合成坏依赖负控仅CI。
- [x] 两生产消费者经过唯一wrapper/Unit序列算法；真实差异仍在原适配，没有复制旧循环。
- [x] 复用已全文核读的12个ordinary exact，新增小型真实源码pair核空/嵌套/正常顺序及
  提前return，沿用既有events/来源与定义使用查询；人工MissingFact/foreign facts整项仅CI。
- [ ] 定向SSA/LLVM及普通native输出、fmt/clippy、docs/尺寸/diff与独审记录实际结果；
  exact-head双宿主CI另账，未运行不写通过。普通native成功不替代动态资源证据。

后续CFG、owner/loan/pending/receiver/iteration与driver共同责任继续开放，不以本片归档。

## 13. 普通 statement 小片实际验收（2026-10-19）

- 合同先提交 `2e5149a7`；生产/测试受验为 `3a77ed0188740ec0fe9de7f7b4349c01dac9641f` / tree `c664ac23e3a34152c4ed663a489a2d41545aac1a`。8个Rust文件共+188/-28；共享32行核心，两入口适配15/20行，重复wrapper策略和Unit循环已移除，入口保留薄转调。single仍Block/LambdaBody，unit仍另含ControlBody；Map专用inner、tail/lambda、原cleanup和CFG运输逐字保持。
- 已全文审查12个普通exact/helper，在输入`fbb50f06`各实际1/exit0建立基线；安装后原12与两个新pair均各1/exit0，首编1m26s。新增58行复用既有同源single/basic/const及反序查询，锁空/嵌套顺序、unused String的非空AfterStatement Literal→Drop、println清理顺序与提前return停止内外序列；原helper不改，无另建大型查询框架。
- 同头再验前两片27项去掉重复的生产依赖guard与paired_print后25项，全部各1/exit0。合39个唯一ordinary exact，6个native测试沿原helper共12次正常运行，stdout/exit/stderr原硬断通过；不外推释放次数、泄漏或完整资源证明。
- `cargo check --locked --offline -p lang-codegen --tests`35.32s/0；fmt通过；Clippy（同§11命令）1m07s/0。尺寸门`python3 scripts/check_rust_sizes.py --base fbb50f06`为1837手写Rust/历史38超限通过，无新增例外；首次size调用漏`--base`仅usage退出，正确补基线后通过，原失败保留。测试命令沿§9逐exact串行，原日志仅仓外`d10-readonly/logs/statement-*`，后继docs提交不冒称重跑行为。
- 本轮未运行任何合成坏依赖/缺事实/非法SSA/注入/校准；新statement依赖负控留CI。§12前两项及本地普通验收成立，精确新远端头双hostCI待验；C/D/E及CFG/owner/loan/pending/receiver/iteration/driver共用责任继续开放。

## 14. C/D control-tail顺序核心小片（待实施）

本片只收敛已支持control-tail的顺序执行与body cleanup时点；不关闭CFG/cleanup/driver或D10。设计/安装基线为 `64461b187e34d721bfa4f6a1402abe053be195de` / tree `93aa3fc098c4845bc147952f942d87a96d71d55c`；既有39个ordinary的原结果不继承为新生产头通过。

- 在既有statement_core复用StatementLowering两端口：空elements直接Unit；prefix经共享Unit sequence，错误或Diverged立即停；末项经共享statement wrapper保留三态，非Diverged才发body AfterStatement。两入口删除原重复循环和末项收尾。
- 适配保留原AST查询/错误Span、single只识别ControlBody、unit另识别LambdaBody及原clone/fallback；source-qualified DropPoint仍由unit现场构造。core不持有或解释bindings、owner、loan、pending、receiver、CFG状态，不新增trait/context或尾循环回调。
- 原branch结果交付/BranchExit、single lambda空体cleanup与结果合同、return/break/continue/Abort、iteration专用cleanup不动；Diverged不得替换为terminator检查。不触及0321、0291或0333–0340。
- 验收优先复用第三片39个ordinary的完整exact及已审helper，按本次实际影响选取并逐项记实际命中；补小型真实源码pair锁tail Value交付及prefix/末项提前退出，复用既有source/Span/owner定义使用查询，不新建大型query模块。空tail/fallback保原行为；不可达的人工错误只留CI。
- [x] 两消费者经唯一tail算法，原两段重复删除；空/正常/Value/Diverged/错误顺序与原资源关系保持。
- [ ] 普通SSA/LLVM/native完整输出、依赖门、工程门及独审记录实际结果；人工缺事实/非法SSA/合成禁边/注入/校准仅CI，精确头双宿主CI另账。普通native不证明释放次数或无泄漏。

## 15. control-tail 小片实际验收（2026-10-19）

- 合同/原则先提交 `8117e705`；受验生产为 `89caa13810a9cd781c8b623724e5292549fb0063` / tree `e95063e2cdd02b1aca0cdb703769ff18b6bd5be2`。5个Rust文件+174/-32：既有statement_core增加18行唯一tail顺序算法，两入口删除重复prefix/last/body cleanup策略；清理unit失用的单个导入。原AST查询/fallback、unit clone及能力差、single lambda空体清理和return/loop专门状态原样保留。
- 在第三片同生产输入复用12候选中的10项已验基线，另两项真实when/Unit lambda ordinary各1/exit0。新153行测试含最小只读observer接线，旧paired调用顺序和既有来源/关系断言保持；两个新pair锁prefix五事件/无用owner清理、正常tail的实际String Value恰一次进入Return且不Drop，以及prefix return后不可达tail不生成、last return保留Diverged。固定源只有一个正常出口，未扩大为任意phi/多活join的独立证明。
- 新头43个唯一ordinary exact全部各1/exit0，首编1m25s；包含原39、两项既有邻控和两项新pair。6个native测试仍沿原helper共12次正常运行，完整stdout/exit/stderr硬断通过，不外推释放计数或泄漏。
- 原未fmt候选check37.78s/0有一条未用import警告；纯格式及该导入清理后check-v2 43.71s/0无warning。fmt通过，Clippy（同§11命令）58.11s/0；尺寸`--base 64461b18`为1837手写Rust/历史38超限通过，无新增例外。完整原日志仅仓外`d10-readonly/logs/tail-*`；后继文档头不冒称重跑。
- §14算法/本地普通验收成立，精确远端头双hostCI另验；本轮不运行合成缺事实/非法SSA/注入/校准。共同checked CFG、owner/loan/pending/receiver/iteration/driver责任继续开放，下一设计须按状态角色不变量形成唯一算法，不将本小片称D10终态。

## 16. C/D checked scalar CFG 共同构造合同（实际结果见§17）

设计输入 `4daf1c7fd584a39f26a349ed78a30cac592a750d` / tree `aa66078423410df02dcde504e641baf65e561ed8`。本片从已验证Operation/结果type/Span到同型failure-success CFG及scalar交付，统一实际构造算法；不将角色运输的全部共同责任冒称已完成。

- 在expression_core同层子模块构造唯一checked流程：原Boolean事实查询→append两个Value结果→原角色快照/保序列→failure后success两块→failed真边Abort/假边success→切当前block→原角色恢复→交付原scalar。原首错/Span与append后才收集运输事实的次序保持；Abort不新增cleanup或pending unwind。
- 仅引入成对EntityId/EntityType的CarryColumn与入口私有强类型快照。窄端口仅读Boolean事实、借出原IR cursor、读取角色、恢复当前实体；不得在端口决定append/块/边/Abort/返回值或复制共同编排，不能用万能context、可选字段或类型相等猜owner/loan身份。
- 两端删除完整checked_operation的重复CFG构造及重复arithmetic Operation构造，保薄转调。single linear/Map proof与unit Value/access列顺序、pending Place/receiver及各自Map恢复按原helper整体执行；不重排capture/range/nullable或Value/Loan receiver时点，不额外去重列、不提前验证列、不改变其他CFG消费者。
- 原整数receiver验证/一次求值/结果类型查询不动，所有已支持arithmetic、复合赋值与conversion消费者经唯一核心。实际生产依赖扫描覆盖expression_core子树，禁止核心读取driver/planner/LLVM和IR/support反向边；损坏事实、非法SSA、注入/计数/校准及合成禁边仅CI。
- [x] 核心及两薄适配独审确认输入/输出/状态所有者不变量，原两份CFG算法删除，真实能力及诊断差异保留。
- [ ] 普通原转换/算术/pending/receiver/容器/Map direct与conditional来源及成功native先后采证，明确const-enabled入口、静态failure CFG与正常产物边界；工程/依赖门与精确头双hostCI分别记录。

后续carry的有序槽关联、alias索引与参数重绑仍须按共同角色逐项收敛；不能把这些双份算法永久改名为策略。整体CFG/cleanup/driver与D10保持开放，不混入0291或0333–0340。

## 17. checked scalar CFG 小片实际验收（2026-10-19）

- §16合同先提交`81eb1484`；受验生产为`343f3bfe32278f507c2b418a05042109c3cc030a` / tree `9116cb98fd121074d49230957cb0038bd668102a`。6个Rust文件+269/-116，含134行`expression_core/checked.rs`；两入口完整checked CFG与arithmetic Operation构造重复已删，薄适配保各自Boolean事实、强类型角色快照与原恢复helper。core独占append、failure/success块与边、Abort、success切换及原scalar交付，append后才capture、首错/Span及Value/access列顺序保持。
- 15个checked相关ordinary完整函数/helper先独审，与既有43重合conversion四项；新增11在原生产头`4daf1c7f`各actual1/exit0，Map28条SSA/LLVM链先绿后双native各14产物。新生产头联合54个唯一exact全部各1 passed/0 failed/0 ignored/exit0，首编1m26s。含const-enabled receiver入口、复合赋值owner重绑、pending/loan及Map直接/条件checked成功延续；静态failure CFG不外推为动态overflow/Abort运行。
- 第10项真实生产树依赖扫描通过，新增checked子模块required命中，原expression_core子树边界保持。第52项Map两入口共28条SSA/LLVM链成功，再运行53/54两个native；全部8个native exact沿原helper合40次正常运行，完整stdout/exit/stderr合同成立，不证明动态释放计数或泄漏。未新增测试框架；合成缺事实、非法SSA、禁边注入及校准仍仅CI。
- `cargo check --locked --offline -p lang-codegen --tests`37.57s/0无warning，fmt通过，Clippy（同§11命令）55.46s/0。尺寸`--base 4daf1c7f`为1838手写Rust/历史38超限通过，无新增例外；完整日志仅仓外`d10-readonly/logs/checked-cfg-*`。六安装字节及54实际结果已独审，后继docs头不冒称重跑；精确远端双hostCI另验。
- 本片统一checked构造算法，角色槽关联/alias索引/参数重绑及其他CFG、cleanup、调用和driver责任继续开放。single单池与unit Value/access双池、pending Place/receiver、Map恢复等原能力不因抽取扩大；D10仍为active分阶段实施。

## 18. C/D temporary alias carry 完整角色小片（实际结果见§19）

设计输入生产`343f3bfe32278f507c2b418a05042109c3cc030a` / tree `9116cb98fd121074d49230957cb0038bd668102a`，当前docs后继`b33cdca53f2653ee919c9f30cfdbfd16ae3b3dc5`。本片消费既有temporary身份与SSA实体，仅统一该角色的有序槽关联及逐列恢复；Guide、能力和生命周期不变，不关闭全carry/CFG/cleanup或D10。

- 新SSA同层私有carry核心独占临时key有序遍历、所属槽池首个同source复用、仅miss查entity.ty并追加槽、挂全部key及对应当前Value的逐列fan-out写回。关联真源是物理列位置；seed不去重，不建立全局source→parameter替换表，不按type混同owner，不提前验证命中列。
- 三数据端口限定为source getter、temporary keys可变字段借用、由ValueId/EntityType构造其它角色全空的槽；适配只保local/source-qualified key、本地存储和原调用位置。核心不借整个lowerer、不接收CFG或恢复算法callback，不以多个Option拼万能context。
- 删除single `control/carried.rs`与unit `cfg.rs`两份temporary收集循环，以及single `control/carried.rs`与unit `cfg/rebind.rs`两份逐key写回循环。single逐列temp→capture/proof交错、unit独立temp阶段，以及各自clear、长度/kind检查、首错和Span原时点保留；single仅非空keys才提取Value，unit仍检查每个Value列。
- shared核心仅依赖model、中立error及诊断Span；禁回checked/expression/statement/driver/planner/LLVM，IR/model与lowering_support禁反向carry。实际生产文件纳required扫描与对应规则；合成坏依赖仅CI。
- entry loan、pending连续槽与Place/receiver、循环同source snapshot拆列、Group交付/清理、nullable/Map多列证明及其它角色调度保原事实。named/pending/loan的共同列关联仍须后继逐项收敛，不把本小片当长期多套helper的终点；不混0291或0333–0340。
- 验收复用既有54 ordinary及两项已独审真实临时源测试；新增最小Borrow Group配对先在原生产头核准入，再核实际HeapAllocate/payload→RootPlace/Loan→checked双边列→success当前实体→BorrowEnd先于Drop。私有temporary多key来源以真实Group/Temporary producer链静读说明，SSA observer不冒称枚举私有表；正常源码测试不注入facts/state，不执行期待Abort/native fault或资源计数。
- [x] 两端消费者经唯一temporary收集/逐列恢复，四份重复责任删除；端口与原错误/角色调度经独审。
- [ ] 原头新源准入及新头ordinary SSA/LLVM、实际依赖门/工程门分别记准确结果；缺事实、非法SSA、故障/计数/校准仅精确头CI，双host CI另账。

## 19. temporary alias carry 小片实际验收（2026-10-19）

- §18合同先提交`1726a5a7`；受验生产为`d45687cd9770968a6b96dca78a41774da5dbb25b` / tree `3031b30fe253a065f1cadc35ab9a52f7f3829819`。生产5文件+99/-39，新增51行`ssa/carry.rs`，两端四段temporary收集/逐key写回循环已删；测试依赖扫描另+72/-1。核心独占首同source物理列匹配、miss才查完整type/追加及当前Value fan-out，三端口只有字段读取/借用/空槽构造；原seed、clear、参数kind门、角色交错和错误Span保持。
- 两项既有ordinary临时源邻控先在原生产输入各1/exit0。新Borrow Group成对源码首次基线因用生成函数名识别unit真实callee而失败（0 passed/1 failed/exit101）；真实unit使用声明编号名称，此为测试查询误用，未改生产。仅改为FunctionId对应的真实source path/完整声明Origin，并在只读observer层收齐六份结果再断言；真实前端/lowering/verifier错误仍原样传播。修版原生产头`2f28ec9b`基线1/exit0，六观察为2 single+4 unit-basic，无const扩称。
- 新生产头57个唯一ordinary exact全部各1 passed/0 failed/0 ignored/exit0，首编1m27s；第57项六观察均一次ok。新query锁两个不同同型owner、RootPlace/双Shared loan、checked两边完整列、success当前实体及原scalar交付、BorrowEnd先于Drop；Group多key依据真实producer静读，不声称observer读取私有temporary表。第10项实际生产依赖门通过；第52项28条Map SSA/LLVM链成功后，53/54双native各14产物成功。全部8 native exact仍共40次正常运行，原stdout/exit/stderr合同保持，不外推次数或泄漏。
- 有限check40.00s/0无warning；fmt通过，`cargo clippy --locked --offline -p lang-codegen --all-targets -- -D warnings`1m07s/0；尺寸`--base b33cdca5`为1840手写Rust/历史38超限通过，无新增例外；本段安装后文档门713份通过，`git diff --check`通过。完整日志保存在仓外`d10-readonly/logs/carry-alias-*`；初次基线红保留。新增`dependency_guard_checks_carry_boundary`为CI-only，本地仅真实生产树扫描；没有运行合成缺事实、坏依赖、非法SSA、注入或校准。后继docs头不冒称重跑行为。
- 前片checked远端`0a113082`的PR run `39239267234`完整成功，双host54 ordinary及3依赖负控各实际通过；三个上传ZIP经SHA/路径/契约内容只读独审，Linux generated完整pass、Mac仍partial，未执行产物。此证据归前片固定头，当前temporary片精确远端CI另验。capture loan及named/pending等其它carry角色、多入边合流、cleanup、调用和driver共同责任仍开放，D10未完成。

## 20. C/D capture Loan alias 共享角色合同（实际结果见§21）

设计输入为temporary生产`d45687cd9770968a6b96dca78a41774da5dbb25b`及docs后继`ebd250e1`。本片把已有capture单前沿关联/逐列恢复纳入同一个carry引擎，不新增第二套Loan算法，不改变closure资格、形成或清理合同。

- 以固定Temporary/CaptureLoan编译期角色区分同型key的不同存储字段；角色仅给Value/Loan实体包装，槽仍三个纯数据端口。现collect/apply直接提炼为唯一共享算法，四个temporary消费者与四个capture消费者同用；删除两份capture搜索/追加及两份逐key写回循环。
- 保原ordered key、所属槽池首同Entity复用、命中不查type、miss才InvalidModel/追加、原Span与seed列位置。single逐列先temp、非空capture才Loan检查/写回、后proof；unit保symbol/receiver/captures全空skip与其后的Loan检查，不因capture为空跳过其它角色。source-qualified键与真实物理列不转换/不全局去重。
- SharedFieldLoan父槽与SharedReferenceFollow子loan保持不同实体，thunk退出使用当前子先父后；closure Drop后carrier退休不补BorrowEnd。single与unit第二live owner边界、capture key交集合流/严格集合相等等原差异不动；multi-incoming、loop-edge proof与pending角色另有事实合同，不能套单前沿算法或借本片放宽。
- 既有五项真实ordinary先在原生产头确认基线；最小只读observer沿实际ClosureConstruct的thunk/Source Origin及父子def-use，核checked两边原实体对应列、success当前参数及精确BorrowEnd。保完整原源码产品/SSA/LLVM门，不根据内部展示名、同type或计数替代身份；不为跨角色同Loan多key假造私有状态。
- [x] 唯一typed alias引擎与四份capture重复删除，经独审核端口、角色顺序/首错与依赖方向；没有回调中的隐含算法。
- [ ] 原头真实observer准入及新头相关普通回归/工程/依赖门实采；合成缺事实、非法SSA、坏依赖、故障/计数/校准仅CI，精确头双host另验。整体carry/CFG/cleanup/调用/driver与D10仍开放。

## 21. capture Loan alias 小片实际验收（2026-10-19）

- §20合同先提交`10e56eb3`；受验生产为`1150c7a2f7d07c45eb5ae334a663fcf7040047e9` / tree `aa38f6866fa5aa1f5e6c2a7072ef6f02422121dd`。4个既有生产文件+118/-71，carry从51行变77行；Temporary/CaptureLoan两个编译期角色只包装Value/Loan实体，三槽端口仍纯字段/空构造。temporary与capture共用唯一collect/fan-out，另四段capture重复循环已删；未另建Loan引擎或新增依赖豁免。
- 原五个closure ordinary在前片生产头各1 passed/exit0；unit最小exact含pointer/Shared/Owned三真实源码。新增只读observer仅检查Shared产品，以实际ClosureConstruct.thunk、完整Source Origin、SharedFieldLoan父槽与Follow子loan、checked双边对应物理列及success当前参数连接到BorrowEnd子先父。原三源/全部原oracle/真实SSA与LLVM门保持；新query失败收齐原产品后硬断。tests头`c4c907eb`原生产基线实际1 passed/0 failed/0 ignored/exit0、编译1m31s，三原产品与新观察全绿，不把一个exact写成三个test。
- 生产候选静审发现single缺LoanId显式导入，在安装前仅补一行后重冻；旧候选未编译，不记为运行红。v2有限check40.07s/0无warning。single本列temporary→capture→proof与非空capture才kind门、unit原skip/kind→capture→named/receiver保持；Shared父/Follow子不合并，closure Drop退休及两端merge/第二live owner能力差异未改。
- 新头62个唯一ordinary全部各1 passed/0 failed/0 ignored/exit0，首编1m39s；选择严格原57+五个已完整静审closure exact，第62项三环境及升级Shared observer全过，第57项temporary六观察保持。原8 native exact共40正常产物保持，第52项Map28条SSA/LLVM先于53/54双native各14产物。fmt通过，Clippy（同§19命令）1m14s/0；尺寸`--base ebd250e1`为1840手写Rust/历史38超限通过，无新增例外。本段安装后文档门713份及`git diff --check`通过，后继docs头不冒称重跑行为；原日志仓外保留。实际生产树依赖扫描仍第10项，合成坏依赖/缺事实/非法SSA/故障/计数/校准只CI，原native仍沿正常输出合同，不新增动态计数结论。
- 本片收敛一个前沿的temporary/capture槽关联与当前实体fan-out；named Borrow seed、pending Value/Loan/Place、result-source、loop-edge与多入边合流具有待核责任，cleanup/call/driver及全域结构审计继续开放。没有扩大Guide或unit/single能力，D10仍in-progress。

## 22. C/D result-source indexed alias 完整角色合同（实际结果见§23）

设计输入生产`1150c7a2f7d07c45eb5ae334a663fcf7040047e9`，当前文档头`03074abc59fece34266e49d46ece363e91a95852`。本片统一borrowed result所保存ancestor loan的单前沿有序关联与indexed当前实体恢复；不改变返回借用、Range或其它语言能力。

- 在现唯一`carry::collect_aliases`内接收保序的`(key, typed ID)`数据迭代，增固定ResultSourceLoan角色；两端symbol原域与Vec原index保留，首个同source物理列命中不查type，仅miss查询完整实体并追加。缺失错误以预构造Copy数据传入：旧temporary/capture仍InvalidModel，本角色仍MissingFact；无验证callback、预扫描或全局去重seed。
- 共享核心独占indexed fan-out的entry/必要resize/按index赋位，保`resize(index + 1, current)`填充值和重复key原写序。删除single `control/carried.rs`与unit `cfg/range.rs`各自collector及indexed写回循环；原平面fan-out与indexed输出是不同容器责任，不另复制首同源引擎。unit全部纯数据AliasSlot投影可集中到`cfg/alias_slots.rs`，不接整个lowerer或隐藏算法。
- 入口保原角色调度、非空keys时Loan检查、长度门和错误Span；single在原位置clear后逐列就地写回，unit保所有空symbol的局部map且全成功才提交。不得新增index连续性/唯一性检查、改变命中验证或错误时部分状态；公共多边merge的`range::{keys,arguments,rebind}`及loop出边核验不在此片替换。
- 实际producer的ancestor Vec次序、RangeEnd/结果binding清理后逆序BorrowEnd保持。现有普通named Range exact在原生产头已六产品SSA/LLVM成功（normal/break/continue×single/unit）；这只是单descriptor、单owned List根、单ancestor index 0的成功门。经测试维护成本评审，复用原exact精简输出六个真实产品的确定性SSA文本、完整module结构Debug与既有LLVM文本；原生产头两次运行先确认逐字确定性，再与共享后同源/同工具链/同选项产品逐字比较。保原验证器与语义oracle，默认不规范化任何差异，全文仅仓外保存；该证据称完整结构不变对照，不称新独立生命周期证明，不伪造多index、重复alias或损坏事实。
- [x] 四份重复责任经唯一collector/indexed写回替代，原入口身份/状态提交/首错差异与依赖方向独审。
- [ ] 原头确定性与前后完整结构对照、新头普通相关回归及工程门分别实采；精确远端双host CI另账，缺事实/非法SSA/合成禁边/故障注入/校准仅CI。

pending的命中前验证与frame provenance、named Borrow的独立seed、整体CFG/调用/cleanup/driver共同责任继续开放。不得把真实暂留差异永久改名为策略，或把本角色统一称为D10完成。


## 23. result-source indexed alias 小片实际验收（2026-10-19）

- §22合同先提交`39ce5f97`；经测试维护成本评审，`19d5bd01`将验收改为既有真实产品的完整输出前后对照。419行专用生命周期observer草稿未安装；实际tests头`53eb3568`只给原exact净增15行分段输出，原源码、六程序构建顺序、全部helper、原SSA/LLVM门不变，无新测试框架、环境开关或第二次LLVM lower。
- 原生产头两次同源/同工具链/同选项，各1 passed/0 failed/0 ignored/exit0；六产品（normal/break/continue×single/unit）的SSA文本、module结构Debug及既有LLVM返回值共18段、1,944,049字节逐字相同。只读比较器按UTF-8长度/同key边界、完整18键顺序、段外真实exact与唯一成功摘要、独立exit0核验，零规范化。现有Debug隐藏的跨分析防串owner、未输出的SourceMap不作相等宣称；源/provider/helper及工具另由固定输入绑定，不称新增独立生命周期证明。
- 受验生产`199fe0dd8df18d7419e71dc7d1b80a80a16b9fe0` / tree `d9231efaf10b9d94e40a9572d3ca40f80d0f5f50`：5个Rust文件实际Git+165/-118。现唯一collector接保序数据与Copy错误，新增ResultSourceLoan、indexed只读展平及唯一indexed fan-out；删除两份collector和两份entry/resize/赋位算法，保原非空kind门、single就地写回、unit空symbol seed/成功提交。两个旧unit槽投影逐字移到77行`cfg/alias_slots.rs`，第三槽同样纯数据；carry 77→109，unit cfg 954→910，无新增尺寸例外。
- 新生产头63个唯一ordinary（原62+上述既有range exact）各1 passed/0 failed/0 ignored/exit0，首编1m35s。第63项18段与两份原头payload再逐字全同；该源仅单named descriptor、单owned List根、ancestor index0，不外推多index/alias交集或动态释放。原temporary六观察、closure三产品与Shared当前父子loan query保持；Map28条SSA/LLVM先于双native各14产物，原8 native exact共40正常产物，完整stdout/exit/stderr合同保持。
- 有限check35.31s/0无warning，fmt通过，Clippy（同§11命令）59.72s/0无warning；尺寸`--base 03074abc`为1841手写Rust/历史38超限通过，docs713/diff检查通过。3260完整tracked输入及工具绑定已冻结；实际递归依赖扫描覆盖新slot文件，未扩白名单，合成坏依赖/缺事实/非法SSA/注入/计数/校准仍仅CI。完整原日志与18段全文只在仓外，后继docs头不冒称重跑，精确远端CI另验。
- 本片仅关闭§22单前沿result-source责任；named/pending、关系proof、多入边merge、其它CFG/调用/cleanup/driver仍开放。后继先按全域责任矩阵与有限阶段出口收敛，不能依helper或测试数量判定D10完成。

## 24. 全域责任矩阵与有限收口顺序

本节是A/C/D/E的唯一全域进度点，后续原位更新；“片已共享/已验/已发布”不等于“域完成”。域关闭须核全部共同算法与生产消费者，真实差异有事实依据和固定回归，最终满足§6/E。下表`S`指`crates/lang-codegen/src/ssa/`，`L/U`指其`lower_frontend/`、`unit_lower/`；按输入事实→输出SSA→状态所有者划界，函数名是生产锚。
result-source已发布至`0119b198`，PR run `39255702345`完整CI成功，63普通及4个CI-only guard双host各一次通过；三ZIP只读审计核592输入/252 choice hashes与精确源码树，Linux generated pass、Mac generated partial，包外二进制hash未重算。完整证据只归该头。
全入边carry-A已发布`797f6829`，PR run `39264278602`完整CI成功，双host67 ordinary及4个CI-only guard各一次通过；三新ZIP只读审计及精确源码树核验通过，Linux generated pass/Mac generated partial及包外hash边界保留。B1单前沿source关联本地验收见§26；B2恢复、C/D及完整carry域仍开放。

| 领域 | 已共享核心/删除责任 | 真实适配差异 | 仍双份共同算法与域验收出口 |
|---|---|---|---|
| 常量 | `S/constant_materialization::operation`四路映射，§9已验 | local/qualified descriptor、const capability、Group origin及原Span | 两端`constant::lower_constant`保身份查验；映射片已收口，常量/字面量全域还须连同表达式登记/清理逐消费者核验。 |
| 整数intrinsic | `S/expression_core::lower_integer`身份校验→receiver一次求值→三态→操作，§11已验 | unit同source、结果SSA type错误映射 | intrinsic流程已唯一；不能连带关闭Range/scalar域；配对来源、八型、转换、求值/定义使用链保全。 |
| statement/tail | `S/statement_core::{lower_statement,lower_sequence,lower_tail_sequence}`顺序与cleanup触发，§13/15已验 | ControlBody/LambdaBody准入、lambda空体与fallback | 两端Map专用inner、branch-result交付和cleanup执行仍开放；验正常/提前退出、Value/Unit和body清理时点。 |
| checked scalar | `S/expression_core/checked::lower_checked_operation`完整append/块/边/Abort/交付，§17已验 | `L/U/checked::capture_roles/restore_roles`的单池/双池及Map/receiver角色 | checked构造已唯一，状态运输未全闭；验全部算术/conversion消费者与原append后capture、首错、成功实体。 |
| temporary/capture alias | `S/carry::{collect_aliases,apply_alias_column}`有序首同实体列与fan-out，§19/21已验 | 本地/qualified key、角色交错、clear/kind门、父子loan | 单前沿八段重复已删；多入边/loop-edge未闭；验seed不去重、当前物理列、不同同型owner、子先父后结束。 |
| result-source alias | 同一collector及`indexed_aliases/apply_indexed_alias_column`，§23本地已验 | single就地写回；unit保空symbol、成功后整表提交；MissingFact保原 | 四段收集/indexed写回已删；`U/cfg/range::{keys,rebind}`多边及loop出边仍开放；全文对照仅证现有六产品结构保持。 |
| named/pending carry | `S/carry/source_columns::{seed_slot,associate_source}`统一独立seed与首source关联，§26已验 | unit named Borrow独立seed；single non-entry named Borrow首同source合列、entry loan过滤；forwarded loan、unit连续Value/Loan/Place槽、receiver、命中前查type及长度门各保原合同 | 原关联12份收为1份；`L/control/carried::rebind_linear_bindings`与`U/cfg/rebind`的共同投影/恢复仍待B2，资格、frame provenance及C关系门不能由关联成功替代。 |
| proof/iteration carry | 既有typed alias不替代关系证明 | nullable owner↔loan、field child、source/element、Map路径及loop snapshot拆列 | `L/control/{proof_join,map_frames}`/`pending_call`与`U/cfg/{nullable,pending_child}`仍有关系校验/重绑责任；先冻结角色合同，验整对运输，不按type或旧source猜身份。 |
| 多入边合流 | `S/carry/columns::IncomingColumns`统一完整incoming签名/整列复用/转置，§25已验 | single保未变baseline/部分key交集；unit预留result/receiver/named/borrow seed、对binding/temporary/capture等要求同key；result-source的unit只核展平(symbol,index)，single核完整symbol集合/Vec长度，空Vec symbol差异保留；零/单出口状态不同 | `L/control::merge_exits`与`U/cfg::merge_unit_exits`已删除各自整列匹配，merge块/边及状态重绑仍待后继；仅全部incoming同列复用与单边alias改变已有42关系验收，不关闭CFG域。 |
| 分支CFG | 仅statement/tail与checked局部共用 | basic/const短路、nullable/Map proof和结果准入 | `L/control::{lower_if,lower_short_circuit,lower_when}`与`U/control`/`control/{conditional,branch}`仍双份；验条件→sibling状态→结果交付→BranchExit→join和原拒绝顺序。 |
| 循环CFG | `S/provider`已有有状态snapshot/header/guard/step核心，不重造 | single逐出口LoopExit后merge；unit普通while先merge后清理、继承proof才逐出口先清理；source/Map能力差 | `L/U/loop_control`及`iteration/numeric_iteration`的header/backedge/continue/break/耗尽骨架仍双份；保原先后，验全部已支持family/退出组合。 |
| 调用参数/receiver | shared callable身份、carry与借用事实helper仅覆盖局部 | single key/forwarded loan；unit pending frame/created slots/loop depth、Value/Inout receiver与ABI owner | `L`主`lower_call`/`borrow_argument`/`readonly_receiver`与`U/call`/`receiver`/两端`call_lifetimes`仍各做顺序求值、参数重排、刷新、提交/结束；验nested/后参CFG/正常与提前退出。 |
| owner/loan/drop/cleanup | 仅触发顺序/若干查询已共享，执行器未统一 | 条件receiver、nullable/Map/iteration事实及首错；single ValueId交付与unit精确ValueDelivery不互换 | `L/drops`/`borrow_result::end_result_bindings`/`call_lifetimes`对`U/ownership`及对应模块仍双份；验generic→Copyable跳过、Group别名退休、BorrowEnd先于Drop、正常/return/break/continue/Abort分流。 |
| 其它表达式/值构造 | error/String decoder、`S/integer`/`integer_range`映射、`borrow_result_support`已共享 | source/type准入与错误时点、String RHS pending/三态、各family已支持能力 | 两端literal/String、`integer_range::lower_integer_range`、prefix/binary/assignment及aggregate/container/closure消费仍未全统一；按family列全体消费者，验求值/构造/交付，不能以末端Operation helper关闭域。 |
| driver/planning | `CallableArena`身份与unit basic/const driver已有共用 | raw校验vs sealed view、single原roots vs unit entry、StaticSelf/deinit/callback/recipe与预算首错 | `L/U/orchestrate`及`L/instances/callable_planner::plan`/`S/unit_plan::plan_unit_instances_from_facts`仍双份worklist/声明/入口/body/return骨架；验原roots/budget/失败原子性/末端verify，禁止single包装unit取能力并集。 |

有限六批按完整领域出口推进，可多提交但同一时间只推进一个未完成步骤；每批先设计独审，再实施与验收，不以逐循环小抽取、巨trait、万能context或回调中的旧算法代替收敛：
1. **完整carry角色/全入边列合同**（下一批，依赖§23）：冻结named/Borrow seed、pending Value/Loan/Place、三类alias、proof/source/element及join角色；出口是共同关联/投影/恢复/整列匹配唯一，真实seed/准入/提交差异有依据，零/单/多出口及跨边alias身份可验证，再独审生产候选。
2. **分支与退出清理**（依赖1）：出口是普通条件/分支/结果交付/BranchExit/join无双份主算法，明确loan-end、owner转移和cleanup的状态所有者，原正常/提前退出/Abort、错误顺序与真实能力差不变。
3. **调用求值帧与receiver**（依赖1–2）：出口是共同源码顺序求值→参数映射→当前operand→提交/结束唯一，borrowed-result source chain纳入；保nested/后参数CFG、entry/new loan、receiver激活及已支持退出深度，未支持能力仍归原Spec。
4. **循环/provider接入**（依赖1–3）：出口是header/backedge/continue/break/耗尽共同骨架唯一，既有provider复用，LoopExit/merge原先后与source/element寿命保全；每个现支持family/退出组合有归属，未证等价者明确开放，不永久排除整个状态域。
5. **剩余表达式/值构造family**（涉及owner/pending依赖2–3；纯scalar可先独立设计）：literal/null/String、Range、prefix/binary/assignment、aggregate/enum/Box/Rc、container/closure有限列项；出口是各family完整事实消费/求值/构造/交付唯一，全部消费者和成功/拒绝/Span/求值次数有证据。
6. **planner/driver与E审计**（依赖前述域稳定）：出口是worklist/预算消费与声明→入口→body/return→verify共同骨架唯一，入口只留真实身份/来源/能力；逐A全表核core/消费者/差异/回归，无开放共同责任，精确交付头双宿主CI与必要资源证据齐备才关闭0341。

第1批按三层收口：`S/carry`唯一持有原序seed/首source关联/全incoming整列匹配/role→物理列/参数投影及普通容器fan-out；入口只供原顺序角色、身份与资格结果，CFG/cleanup持有零单出口、receiver/clear/commit/Map/loop状态。
A统一两端多入边列匹配/投影；B接全部单前沿named/pending/proof/source/element/Map关联与普通恢复；C接全部edge/关系消费者并删除旧路径；D完成成对身份验收和工程审计。可依赖小提交，A/B只记责任片，A–D全部闭合才申请域出口。
Seed与Associate分离，保同源独立prefix和Copyable pending snapshot；column为layout-local短生命周期位置并关联typed角色，不以全局source去重。proof pair/child triple保root/scope/frame，Map/provider关系不被同型实体替代。
共同kind/type/长度/连续槽/列关系检查归共享核，在原门调用；保miss-only与命中前查type、single先完成原role/type/Map准备再逐列匹配/type门与unit后缀完成后全表门。保single原value不可达断言、其余错误/Span及部分写回，不新增全域原子提交。
零出口single直接Diverged、unit先验证再清状态（保frame外壳）；unit receiver normalize可先Drop。named Borrow seed/entry过滤及result-source完整symbol/Vec形状对展平(symbol,index)的差异按本节事实矩阵保留。
single普通loop slot仅views全量复核而unit另查所有alias/type，列为待裁定校验差距，不当能力差永久豁免；existing field/Map关系门仍保。未证明既有前置等价或另准修复前，不宣称验证合同/域全闭。
验收复用现paired桥、目标parameter Definition.index→每条incoming完整签名及既有nullable observer，覆盖零/单/多出口、仅一边alias变化、独立seed、当前Value/Loan/Place及关系；完整产物对照只证零行为。新增只读query共享后目标≤200行，超预算先回审，不削弱断言或新造大框架。
所有现carry生产消费者逐项归核，新模块纳实际依赖扫描；IR/support逆边与carry反依赖禁边不放宽。缺事实/非法SSA/故障计数/校准/native Abort等只CI；生产候选另经具体数据API、错误时点及全差集独审授权。

每批ordinary须先全文审exact/helper并分记SSA/LLVM/native完整stdout/exit/stderr；结构对照不替代独立资源证明。缺事实/foreign-analysis/非法SSA/禁边负控/故障与计数注入/校准/sanitizer只CI；新核心纳现有依赖门，不放宽unit→single或IR/support反向禁边。不改Guide，不夹带0321、0291、0333–0340能力修复；新长期状态协议另立ADR。片发布、域关闭和D10完成分别记账。

## 25. 完整 carry 域 A：全入边列布局小片实际验收（2026-10-19）

- §24先冻结完整carry的A–D出口，合同头`61c9f950`；本片只实现A的全incoming签名收集/首个完整列匹配、role→layout-local ColumnId及逐边参数转置。`carry/columns.rs`113行唯一拥有这些算法，seed独立追加、不因同源合并；空列仍按真实exit数产生空arguments。两入口仅供固定角色顺序与单exit字段投影，single先保全role/type/Map准备再逐role复用type门、unit后缀完成后全表门、关系验证、receiver normalize、零单出口和状态恢复/清理位置保持。普通loop alias校验差距仍开放。
- 受验生产`227ea6787702beb023a3fb4bdc02174436b73e7f` / tree `af3e8b696734b7cfdf488d8937ca987ffc40e045`，7文件实际Git+259/-118（净141；6生产与1依赖required注册）。unit旧range arguments重复展平投影已删除，keys/rebind保留；旧single准备/恢复、unit准备及关联之后type/关系/IR/恢复区机械逐字保持。新core只依model和中立error，原依赖规则扫描新模块，无扩白名单。
- 新普通测试复用现paired桥及Graph的目标parameter Definition.index→每条incoming，不造通用AST解释器；旧wrapper原分析/验证/LLVM门保持，仅窄test可见性。普通default Borrow标量每次读可能产生Read，因此新别名源明确own参数，并硬核typed mode/entry Value。旧生产四项先采：既有单出口String＋新零/多出口String24产品、空列merge6产品、pending同/异全incoming与独立seed12产品，均成功。tests-only初次all-targets check因closure lifetime报4条错误101，未执行exact；仅显式modes类型修正后check40.20s/0，原失败日志保留，旧生产首编1m49s、四项各actual1/exit0。
- 新测试module498行中查询382行、fixture/调用聚合116行（含lifetime显式类型排版增加4行）；超过§24目标已逐块独审，保真实AST/typed/call定位和全部incoming关系，不删判据压行。Graph遍历复用既有代码，不新增通用解释器；该例外仅归本片，未来仍先复用真实fixture/完整结构对照。query只捕获新增只读观察错误以收齐42产品，真实前置/SSA/LLVM门仍直接执行。
- 新头67个唯一ordinary（原63＋既有pending nullable field child proof＋上述三新exact）各1 passed/0 failed/0 ignored/exit0，首编1m45s。新42关系查询通过；proof原四产品全边owner/parent/child合同保持。第63项六真实Range产品18段SSA文本/module Debug/既有LLVM共1,944,049字节，与原生产头的两次运行逐字全同、零规范化；这只证明该六产品结构保持，不称42份全文对照或新增生命周期证明。原8 native exact仍40正常产物，完整stdout/exit/stderr合同保持。
- 有限all-targets check41.81s/0、fmt0、Clippy1m04s/0，尺寸`--base 3e11aa7c`为1843手写Rust/历史38超限通过，docs713/diff通过。3262完整tracked输入及工具绑定稳定；绑定初稿runner hash/数量文字在执行前纠正，正式清单`7497388f`、67收据`f0dabc6e`。原始日志仅仓外，后继文档头不冒称重跑；精确远端CI另验，所有合成缺事实/坏依赖/非法SSA/注入/校准仅CI。
- A片验收不关闭完整carry域：B的单前沿全部角色关联与普通恢复、C的edge/关系消费者及校验差距裁定、D的全域验收仍按§24推进；后续分支/退出cleanup、调用/receiver、loop/provider及planner/driver共同语义尚需统一，0341保持active。


## 26. 完整 carry 域 B1：单前沿 source 关联小片实际验收（2026-10-19）

- 受验生产`077277f7537814f26ff0e0412fdde24976719193` / tree `285c52188450944cd909c7159b63a17ced844064`，14文件+308/-273净35；新`carry/source_columns.rs`58行，原12个首source关联算法收为1个，删除11份重复。独立seed始终追加完整metadata，核心命中不触发Query；Known/Query、Map原命中type门、两个Added-only字段及四scalar seed接点分别保留。三typed alias委托同一引擎；现恢复/关系/IR后缀14保护区158,561字节逐字相同，未把改变容器分配轨迹纳入语义等价宣称。
- tests-only前置`9303a1dc`复用原pending Copyable snapshot exact/source/helper，906→992行；含imports/胶水的窄query共72行，复用既有Graph，检查snapshot/current named不同参数、全部回边及真实call首参来源，先保原SSA/LLVM门。一次真实Program/LLVM额外输出SSA/module结构Debug/LLVM三段；无第二次lower、私有状态造模或通用AST解释器。同旧生产头两次实际各1/exit0，三段共93,173字节严格确定，新共享头相同三段逐字一致；原六Range产品18段1,944,049字节也对两次原头完整相同，均零规范化。全文对照证明这组既有结构不变，关系query另记，未新增全域生命周期证明。
- 68唯一ordinary严格原67加上述snapshot，逐项各1 passed/0 failed/0 ignored/exit0，首编1m34s；原42合流关系、4 proof产品、6 temporary观察、closure及8 native exact共40正常产物保持。原helper完整安全分类不变，实际生产依赖扫描第10项通过；CI-only缺事实/坏依赖/非法SSA/注入与校准未在本地运行。3263 tracked输入及工具绑定稳定，正式清单`b2c138bd`、结果`efea4013`、两全文比较`1fbfe2ec/9b769683`已独审。
- 有限all-targets check44.94s/0无warning，fmt0、Clippy1m01s/0，尺寸`--base fabe96b6`为1844手写Rust/历史38超限通过、无新增例外；文档713份与diff检查通过。原日志/全文仅仓外保留，后继docs头不冒称重跑。前片carry-A精确远端CI与三包只读审计已在§24更新，不借给本B1新头。
- 本片仅关闭单前沿首source关联的重复责任；普通恢复投影及其共享fan-out接线仍归B2，既有平面/indexed写回核心继续复用，dense连续性只有unit一份时保留真实状态所有者，不把搬helper称删除重复。C的完整edge/关系消费者与single loop alias验证差距、D全域证据和§24其它领域继续开放，0341保持active，后继精确远端CI另验。


## 27. 统一管线目标纠偏与隔离接线证据（2026-10-19）

用户确认共享 lowering 只作迁移步骤，终态改按§1；v0.45 的 import/root/入口合同另经明确
批准。此目标修订不追认 B2 红头为可用基线，也不把旧 helper 里程碑当成管线统一完成。

隔离实验基 actual `91c53596c126f90745acee24d783739d7cc9dcc8`，原 bootstrap 385 行逐字
保持，仅加 test-only adapter；35 场景为附件22、仓内 enum2/entry5/diagnostic6，共70个独立
single/unit 进程。唯一 no-run build exit0/61.969s；一次串行采样 exit0/57.199s、70/70完整，
输入/工具绑定无漂移。run-binding SHA `d222dcd066678f75779ee1a1b7238b3195d9f3cc8a564a9aadc75a94e41b2b1c`，
controller receipt SHA `8f83b736ec389b62adf85066f39562383e40836513ca8a52bdf8b48b0dbfbebc`；
原始 trace/日志与完整输入清单保留于仓外 `e2-single-unit-experiment/`，不入生产测试框架。

两份 enum 原 single 均 native 成功，Unit 在已通过名称/类型/ownership/view 后因
UnsupportedNode 拒绝，属于真实能力回退；缺失/非法/歧义 entry 的 single 选择失败被保留，
并未记为 Unit native 成功。显式零参数 entry 双模式成功，显式参数化 entry 双模式按旧合同
拒绝。lexer/parser/names/types 四诊断全文相同但阶段前缀不同；ownership 的 Unit 多一个参数
声明 label；mixed 单文件首停 L0001，Unit 聚合四码。w06 和六诊断场景固定 frontend-only。
附件22份另账：11份仅 Unit native 成功，7份双 native 成功且完整进程输出相同；
w09 仅 single 成功、Unit UnsupportedNode，x01 仅 single 成功、Unit InvalidSsa；
x02 双 UnsupportedNode，w06 双前端 L0135 但主诊断文本不同。两份 enum 加上 w09/x01
共4份真实 single 成功而 Unit 拒绝，不能按总成功率掩盖回退。
实验仍保原 single 环境，未注入 ranges 标准源码，也未实施 v0.45 新 import/root 合同。
这些只是接线和差异证据，不证明整套迁移就绪；后续仓内扩展继续逐项记账。

附件三份144行矩阵没有完整源或原日志；仅22份源可追溯，不能声称144项可复跑。
新增 ordinary 以完整测试体/helper 安全审查为前提；不得借扩大回归执行 CI-only 合成目标。
