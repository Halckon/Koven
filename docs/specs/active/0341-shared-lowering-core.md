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

### 2.1 并行生产责任收敛：capture 规则与 Unit 文件对齐

本批基于 main `4491f1a412f6caf8e9faa8fbfd6f1a84da373d2f`，不依赖未合入的
frontend 合同批次。Single/Unit capture 的 scope 祖先、同来源 Span 包含与
Transferability 合并规则收敛到私有共享实现；保留两个真实 producer 的类型求值、
身份适配和全部旧 witness。这是重复规则退休，不是 Single 消费者或 E2 终态退休。

Unit 类型 producer 在原 index/name/environment 资格门后只对齐一次 canonical ParsedFile
借用，将同一 files 顺序供 signature collector 和 body checker 使用。只减少该处重复对齐，
不改变 index、诊断顺序、recovery 资格或 public API；不宣称整条管线复杂度或吞吐改善已测得。
不修改 E3/refine、native 路由、常量/基本 capability 边界或既有语言语义。

| 验收项 | 目标 | 本批状态 |
|---|---|---|
| capture 合并格、来源与 scope 边界 | 私有 `capture_rules` lib 测试 | 待精确头官方 CI；保留原 producer suites |
| Unit files 对齐及输入资格 | signature recovery 私有 lib 测试与现有 body 测试 | 待精确头官方 CI |
| 共享调用方行为 | 既有 closure/multifile/type suites | 未继承旧 CI 为本批通过 |
| 格式、文档与尺寸 | rustfmt、check_docs、check_rust_sizes、diff check | 静态通过：833 Markdown、2057 Rust 文件；39 项历史超限未扩大 |

已知基线在 codegen 出现 54 项失败后由 heap-handle 递归测试栈溢出中止；不能把该状态
归因于本批，也不能据此免除本批验证。未在本地运行 Cargo、build、test 或 Clippy。

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

### 当前生产接线前置 A0

先把现 project 唯一 Unit 编排与标准 loader 的中立错误抽至 CLI 内部共同入口；只借原
`UnitNameSnapshot`、同轮类型环境、原 project selector 与 object 路径。names gate→types→
complete→basic/const→ownership validation→entry→sealed view→emit 的既有顺序及诊断
保持；project 外壳保输入/路径、呈现、exit 与产物生命周期。共同错误只有一份表示，不为
尚无消费者的 single 策略预建 variant、trait、context 或回调。验收复用原普通 project/std
及完整输出 oracle；本片属于行为中性提取，不声称行为红测或单文件已统一。A0 后紧接
parse-once 接续 A1 和真实 single 接线 A2，不转成长周期 helper 抽取。

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

## 28. 单源 Unit 兼容回退：x01 nullable class root Borrow（2026-10-19）

- 原163字节 x01 源及成功oracle固定。actual91c旧生产首次红在SSA verifier：InvalidSsa，真实OperationContract为nullable owner错误构造inner RootPlace；r3随后真实红在use(n)的原live-If Call总门（UnsupportedNode，[124,130)）。两个停点和未到阶段均完整保留，未修改verifier或放宽测试。
- 最小修复复用现NullableBranch/Shared view、原参数提交与ASAP End-before-Drop；按已发布当前callable/source-qualified事实选择class root Borrow，并在入口与adapter双边排除Map.remove来源。第四接点只给全部真实Borrow参数均有当前proof的普通同步source call资格，保留原Map机会及非候选门；没有复制single控制流模块。
- 734f65af/tree7010362e首次原4exact全绿：x01完整FE、真实owner/view/入边/callee关系、verified SSA/LLVM、另一次public-native lowering/object、链接与进程exit0/stdout精确`retest\n`/stderr空。实际SSA证实use后先End/Drop，再进入内层If/println。三邻控为While retest、Map B3、无root Borrow后继续返回owner；与旧生产完整正文156572B逐字相等（各输出种类沿原exact），终态25件封包abb5dc2a独审通过。
- 同头工程fmt/check绿，Clippy因新测试4个usize恒等cast报101；依首失败停止，diff/真实依赖/Resource-group尚未执行。原工程红15件b4321357独审保留。
- 后继tests-only125b8d6f/tree60b33cdc仅删4个恒等cast，生产/fixture/oracle保持。新no-run112.486s/0，新exe5178的x01＋原Resource-group＋真实生产依赖3exact全绿；x01全文6997B与734f逐字同。四工程fmt/check/clippy/diff全部0（13.647/75.672/59.158/0.008s）。原三邻控历史实跑归734f65af/0e042bd8，不称新头已重跑。此项仅修单个真实兼容回退，不代表单文件入口已迁Unit或D10通用lowering完成。

## 29. 单源 Unit 兼容回退：enum case 条件（2026-10-19）

- 输入与范围：actual91c53596 基线，修复提交 c15ab26f，测试提交 3b0882df 与 5d282900；验收 HEAD 5d28290078ad200b3d31a52f8da384bd1f013260 / tree 515d69a1515ef2835991b5065075673b53397ad1，clean。生产仅 Unit control.rs / enum_lower.rs；本片不涉及 x01 或 tempMap。
- 原失败证据：两份完整原始源、package/path/driver entry/golden 不变，经完整 Unit 前置后，分别在 113..127 与 273..289 返回 UnsupportedNode；先全采两个 Result，再统一硬成功断言，初始 exact 真实 exit101。
- 最小实现：普通 concrete、non-generic、Copyable enum 的零参数且零 payload case 条件复用同轮 source-qualified construction descriptor、真实 variant symbol、result/SSA root 与 ownership plan 校验，然后复用既有 tagged discriminant / Equal / Boolean 条件。条件位置不构造 enum 值、不复制 single lowering 模块。Guide 中其他 case 可有 payload，命中的 Shape.Point 必须无 payload；一般 enum 值 equality、有参数 case、MoveOnly/generic 能力边界不扩张。一般非 case/no descriptor 继续 UnsupportedNode；真实事实不一致 MissingFact；保 entry.span。
- 主证：两个原 driver 的 ordinary exact 完成 lex/parse/index/name 诊断+validate、raw type 诊断+complete+validate、basic ownership 诊断/deferred/validate、sealed owned view，然后真实 Unit lower、SSA verifier、LLVM verifier及源锚定 query。query 连真实声明 variant/descriptor、实际 driver DirectCall callee、同 block Conditional 消费与无 condition TaggedConstruct。独立 native exact 对同两源均实际 object/link/run，exit0、stderr 空、stdout 分别逐字为 `11\n12\n21\n22\n` 和 `12\n20\n0\n`，显式 cleanup；两正常 Result 完整收集。成功 child stdout 由硬断言验证，未另存独立 raw child 日志；该结果不是 sanitizer/macOS/全功能验收。
- 原测试迁移与邻控：旧 Guide 同源码、同 area entry 从拒绝迁为真实 SSA+LLVM 硬成功，保留其既有 basic analyze helper，未调用 complete，不冒充主证完整前置或 native。cross-file construction/TypeTest/projection/重排确定性及 MoveOnly、generic 拒绝三条也用旧 basic helper，未调用 complete；前者是 SSA/实体证据，后两项保原 UnsupportedNode。
- 已执行：同当前实际 binary 的六个 exact 各一次，六项均 1 test pass / exit0，前五真绿后才执行双源 native；完整收据、原 libtest stdout/stderr、3269 输入、8tools、实际 exe 前后核验及独立终审通过。6 是 libtest 调用数，另有 2 次 clang driver 和 2 次实际 native 程序；不声称完整 OS 子进程跟踪。固定证据 manifest359194cb / SUMS d5129c3c / suite a6c5449c（全名与 SHA 见仓外 enum-result-manifest.json）。
- 联合验收待办：独立 enum 工程未执行；后续在 v0.45 联合头做一次工程及 9 exact（8 普通行为＋1 真实生产依赖）验收，enum native 仍以前五 enum 项同头实际全绿为前置。不据独立六项行为绿宣称联合工程或全仓通过；workspace/全量/frontend/CLI、macOS/sanitizer、远端 CI、push/PR/merge 均未执行。原始日志与完整绑定保留于仓外 `e2-enum-repair/`；x01 对应证据保留于 `e2-x01-repair/`，原文件不入库，正式远端交付另登记稳定来源。

## 30. v0.45 与兼容修复联合头验收（2026-10-19）

- 以 actual `e591a9b0` 的 v0.45 树为基，按原顺序独立迁入 x01 四提交、enum 三提交及§28/29两个文档提交；唯一测试注册 EOF 冲突保三项注册，原实现/fixture/oracle不改。联合受验头 `4f8084bf57fac5055488bdaf2bc8ed0ca48ab4be` / tree `295bddd32179f27ecbbbebd107ae90dab9dfe07e`，19路径+1483/-45；未将冻结B2混入。
- 唯一no-run141.368s/0产生实际exe `fad97513`；同头9唯一exact各1/0/exit0（8普通行为＋1真实生产依赖）。x01完整FE、SSA关系/LLVM与另一次public-native链成功，完整6997B与125b/734f逐字同。enum前五项真绿后双源native成功，原golden/exit0/stderr空与cleanup硬断言保持；成功child raw未另存。旧三nullable邻控只复用原734f/0e042历史证据，不声称联合头重跑。
- fmt/check --tests/clippy --all-targets/diff分别12.244/46.342/67.637/0.016s，全部0；731文档和diff门在最终两节工作字节各一次0，随后提交的3291个输入文件逐字保持，因此不重复。9项与四工程各步3291个输入文件/12工具/新exe/HEAD/tree/clean前后核同；原日志和固定绑定留仓外 `d10-unit-integrated-validation/`，receipt `e8a5467e/a9c76350/c5fbbecf/d71f0aad`，发布时另登记精确远端头与CI。
- 此验收关闭§28/29所列联合工程待办，不关闭单文件驱动迁Unit、w09或D10通用lowering目标；宽测试、CI-only合成/注入/校准、远端CI与其他宿主均未运行。

## 31. 单源 Unit 兼容回退：w09 Temporary Map 累加（2026-10-19）

- 原附件 w09 的222字节、`w09_map_temp_source.ko`与`main`保持，fixture SHA `55ce4c7d25c06516cfacd9921fcd16b90c15c63adbd9d44d9133c92c49539007`。首个普通红测在`1399bf28`收齐两fresh：single完整前置、SSA/LLVM成功，Unit完整前置后在For `[146,182)` 返回UnsupportedNode；未改成功oracle或按Assignment文本放行。
- 最小生产`54719039`仅让已验证factory/source、MoveOnly及双Shared loan的Temporary进入现conditional/full-plan；复用完整exit/cleanup、实际Call owner及checked角色运输门，不复制single模块或放宽其他source。preflight仍只遍历owned plans；dead tail沿原sequence跳过，实际到达typed For却缺owned plan仍MissingFact。全部Temporary迁入后，`7ec5fe80`独立删除旧straight/guard-transfer专用死链（5文件+1/-608）；共享straight helpers及nested关系消费者保留。
- 补测同一182字节真实body `error(): Nothing`：原single为MissingFact、原Unit为UnsupportedNode，属于双方原不支持而非single能力回退；gate后Unit完整SSA/LLVM成功，legacy single仍MissingFact，两轮双臂聚合红原收据保留。`313be828`按目标收为Unit-only硬成功，不固定MissingFact为语义合同、不修旧single。真实FE锁Acquire和singleton Exhaustion/无Fallthrough；SSA锁实际main/owner/First、HasNext被Conditional消费、true目标error-origin Abort及无Next/回边。false耗尽的完整有序清理由实际full-plan/Unit lower校验，非新增独立false-edge query；没有执行Nothing native/Abort或声称unwind。
- 最终行为与工程同头`e0e960c1b6a92b8916c488aeef375eb69a944f55` / tree `80766ed4ca58ad54d69a8720a6b95562b0d045ad`，3293输入文件/12工具/实际exe绑定，clean。唯一no-run146.143s/0。四SSA-only exact各1 passed/0 failed/0 ignored/exit0：`d10_temporary_carry::w09_temporary_map_carry_reaches_verified_llvm`、`temporary::map_and_mutable_map_temporary_sources_reach_verified_llvm`、`temporary_exits::map_and_mutable_map_temporary_exits_reach_verified_llvm`、`d10_temporary_carry::unit_temporary_body_nothing_reaches_verified_llvm_without_execution`（共同前缀`ssa::map_provider_exits_pipeline_tests::`）；共19 fresh、846 checks全true，完整SSA verifier/LLVM末阶段齐。
- 上述四项真绿后，`native_map_provider_tests::d10_w09`的single/unit两个原w09 exact各一次1 pass/exit0；真实object/link/program硬验程序exit0、stdout为`map-temp`加一个LF、stderr空。native薄片2文件+29/-2，只参数化Unit helper的path/entry，旧wrapper默认值及行为保持；single旧native helper未调用complete，完整双臂前置来自独立ordinary。同源精确输出覆盖正常累加，未新增checked失败/全部resume incoming独立结构query。成功child输出由原assert消费，未另存child raw；helper无内置timeout，外层120s自身进程组覆盖clang/program后代。两exact不等于OS总进程数2。
- 同头随后fmt、`cargo check --locked --offline -p lang-codegen --tests`、`cargo clippy --locked --offline -p lang-codegen --all-targets -- -D warnings`及固定4f→e0的diff --check各一次0；controller含前后核验耗时13.725/49.372/72.022/1.618s。各步全输入/工具/exe前后稳定，无超时/重试；原始stdout/stderr、JSON artifact与收据留仓外`e2-w09-final-validation/`，manifest `78b293ed`、77项SUMS `d186ad2e`。原普通红及两轮Nothing红证据保留；原35/70矩阵本节未重跑，精确远端CI及其他宿主另验。
- 本片修复w09能力回退，并明确记录full-plan复用新增的Unit Nothing支持；不关闭单文件driver迁Unit、B2或D10全域目标。发布集成保留§30、按六原提交父序迁入且完整行为字节比对；纯docs后继不冒称重跑行为，正式远端头另登记。

## 32. 固定35源场景修后双臂对照（2026-10-19，终态已独核）

- 在实际修复头 `e0e960c1`（tree `80766ed4`）的独立树，仅安装原403行probe＋4行test-only注册；原35场景/34唯一源码、Source显示名/逻辑路径、entry、argv与7个FE-only硬门全字节保持，无生产CLI切换。唯一bin no-run exit0；70臂各一次真实采集完整，无超时、漂移或重试，不等于70源码全成功。
- 两臂FE各28、public-native各25；single native exit0仍12，Unit19→23。w09、x01及两enum的4个旧Unit退化全部实际到达public-native/link/run exit0；12个双native的exit/stdout/stderr全部相同。其余66份完整trace对旧actual91c逐字相同（single35、Unit31），只4份Unit trace变化，未规范化；本轮没有独立SSA关系/完整SSA-model-LLVM对照。
- 保留11个Unit-only收益（w01/02/03/04/07/10/11/13/14/17/19），single仍原MissingFact；保留3组诊断差：w06同L0135但loan冲突文案不同，moved诊断Unit多参数声明label，mixed为single Lexer首停与Unit names聚合4码。7诊断案例的同臂全文均与旧轮相同。x02和显式参数化entry边界、3个原driver无目标原样记录，后者不是Unit后端拒绝。
- 本轮为22附件＋13仓内源码回放，不称原exact全集/外部144源（仍缺122源），无标准源注入；最终统一driver/package/root/import协议及single退役另验。完整仓外结果 `e2-single-unit-rebound-experiment/`；run-binding `8941964c`，实际exe `b35f6506`，controller receipt `ecb1263d`，outcome-summary `53b9ec92`、逐臂对照 `ed0dae17`；3294输入/8工具稳定，旧310件封包保持。

## 34. 统一 Unit driver 直接前置 A0：project 核心与中立错误（2026-10-19）

- 固定母基 `f2d8bf7a` / tree `71e01208`，保留§32。独立 docs `7cba5bf4`、test-only `efd10243` / tree `678f24bb`，原生产先验；生产提取 `a6a1ef53` / tree `960f6b65` 仅五个 CLI 文件，普通测试源码及 oracle 不变。
- `unit_build::emit_unit_object` 借原 `UnitNameSnapshot`、同轮类型环境、project selector 与 object 路径，唯一推进 names gate→types→complete→basic/const→ownership validation→entry→sealed view→emit；project 外壳保输入/路径、呈现、exit 与产物生命周期。标准 loader 及共同阶段只用一份 `UnitBuildError`，project entry 小型失败包装保原完整文本，未加未用 single 策略或万能接口。
- 旧生产 test-only 与候选各18个完整 ordinary exact，逐项一次均 actual1 passed / 0 failed / 0 ignored / exit0，无超时、漂移或重试。仅 selector exact 从 `project_build::tests` 迁至 `unit_build::tests`，其余17名与全部断言保持；三 std、九 project basic/const ownership、names/operational/final-admission 与新 entry 全文回归覆盖原阶段顺序及输出。新 entry 测试在旧实现已绿，本片是行为中性前后比较，不声称语言行为红测。
- 两头唯一 no-run 分别62.362s/0、4.596s/0，实际 Cargo JSON 各绑定正常 CLI 与三个测试 executable；3293→3294输入、12工具与四exe在各自受验轮前后核同。两 integration exe 在候选本次合法增量仍同旧 SHA；生产 CLI 与 bin-test 为新字节，未按旧缓存猜测身份。
- 同候选随后 fmt、CLI standalone build、CLI all-targets clippy、docs、尺寸与固定 f2d→a6a diff 六项各一次 exit0；controller 含前后核验耗时15.691/2.675/33.235/5.191/7.516/2.300s。731份文档通过；1849个手写Rust文件中38个历史超限照报，未放宽尺寸policy。Python checker用 `-B` 抑制导入缓存，全文审查确认只读真实树，无附带synthetic测试。
- 完整原始绑定/输出/收据留仓外 `e2-unit-driver-a0-validation/`；基线 result `9339314b` / SUMS `d1e61611`，候选18+6 result `cb65d894` / SUMS `0d41c2f9`。18是libtest调用数，不冒充其CLI/link/native子进程总数。未执行整 target、合成事实、故障注入/计数/校准、预期 Abort、sanitizer、远端 CI 或其它宿主。真实 single 与 frontend API 未切换；A1 parse-once、A2 single 和 CLI/LSP/其它全部生产消费者的旧 checker 退役仍开放，本节不关闭 D10。

## 35. 单源 Unit 兼容回退：concrete generic enum（2026-10-19）

- 首采4份完整原源、8臂中7臂真实native成功；635B原`nominal-run.ko`的single成功，Unit完整前置后在`Maybe.Some(13)` `[398,412)` 返回UnsupportedNode，不能以三份boxed双臂成功掩盖。后继保原635B/`bootstrap`/空argv及`Owned<Resource>`尾，原831B返回/投影/rootdrop源、98B owned参数源也逐字保持；原generic必拒测试迁为硬成功，不固定旧后端缺口为语言合同。
- 实现只改5个Unit生产文件：复用既有concrete type替换/查找，按exact root及有序实参解析所有case的direct T或已闭合payload；enum字段键改为concrete root＋source-qualified payload symbol，删除模板case别名，实际EnumCase refinement归一root。构造核精确instance/参数映射，原求值与Value delivery顺序保持；projection、tag/when与root drop消费同一具体布局，不新增frontend公开API或复制single模块。
- tests-only`9aa47308`的6个ordinary先完整采齐真实exit101，均在完整FE/ownership后因lower UnsupportedNode失败；生产`8e6d2b71`使同6项真实SSA/LLVM成功，另6个旧nongeneric邻控及原635B native也成功。Int/Long同声明实例不串布局，descriptor index与实际SSA实体锁second先求值、first后求值，字段first在0、second在1；不以交换加法输出替代该关系。
- 最终在A0 source`95823ecf`（与actual`83179a34`同tree）独立按test`411e6679`→production`110d1529`保两提交。受验HEAD`110d15293e7f4831bd79ba00c5f89474be8e06fa` / tree`98b4b73f9ec1934abc551469de5dfce89d284f51`，clean；两次当前no-run分别绑定codegen libtest及CLI三产物。17个既有ordinary各1 test/exit0：原12＋原635 native＋三条project basic/const/argv成功合同＋loaded std ranges；其后fmt、codegen/CLI build与all-targets clippy、docs、尺寸及diff六门全0，3297输入/12工具/14配置检查/4产物前后稳定，无重试或漂移。
- 完整仓外证据`e2-concrete-enum-validation/`：六首红`26ce2c2b`，独立12绿`e60fa8c1`/native`8c8a58ab`，最终17绿`5db846d7`，最终manifest`1b73574a`/208件SUMS`0e0582c4`，均独审。635B native保原success-only oracle；成功child Output由测试捕获，未另存child raw或新设stdout/stderr golden。`Resource {}`无deinit；真实SSA/LLVM active-payload→HeapOwner释放关系不冒充动态free计数。
- 本轮只迁移上述concrete enum范围；generic callable模板中的enum构造、模板payload内嵌套/nullable T替换仍是待补迁移差距。既有MoveOnly when与真正deinit门禁未旁路；计数/allocator替换/私有事实/故障与预期Abort仅CI。本轮未重跑四源双臂矩阵、全仓或其它宿主，不关闭single driver迁Unit、A1/A2、剩余共同lowering责任或D10。

## 36. 统一 Unit driver 直接前置 A1：已解析名称续接（2026-10-19）

- 只改 frontend `analysis/unit_names.rs`：私有 `analyze_parsed_unit_names` 按值接原 SourceMap、descriptor/ParsedFile 配对及环境，唯一推进 index→resolve→配对 canonical 排序/unzip→validate。公开 `analyze_unit_names` 仍按原 descriptor 顺序先完整 lex/parse，再委托；无 AST clone、SourceId 重登记或新增公开 API，原首错与 recovery 诊断顺序保持。
- A0 `95823ecf` 上原 `unit_name_snapshot` 三个普通 exact 先各1 pass；独立 A1 `5d9514ee` 再跑原3＋新增同文件2项，各 actual1 passed / 0 failed / 0 ignored / exit0。新测试在基线不存在，不称行为红测。两项以独立真实手工前缀对照完整 names/diagnostics、四 AST 表长度/ID/Span/payload、来源、headers/roots 与环境 owner 身份；非空 roots 指针只佐证移动，不代替语义比较。
- 最终与§35合成的受验 HEAD `c46c0f515ebc401b6137b887678a4a9ebcfdb956` / tree `40860600d44d1387fa15d4ef8c1516398e6fe65e`，clean。父 source `c5491709` 与已发布 actual `d29ca08ebdc8f278747078d22c1c07f30e73944f` 同 tree `ed1d7016`；后继 A1 实现＋docs 两提交小包以该 actual 为恢复基，行为证据仍只归上述 c46c。
- 合成头三次定向 no-run（frontend lib＋names integration、CLI bin＋project integration、codegen lib）均 exit0；本次 JSON 绑定六个真实 executable。11个普通 exact 按原5＋loaded std ranges＋原635B generic SSA＋三条 project basic/const/argv＋原635B generic native 执行，各1/0/0/exit0，native 最后且以前10绿为前置。loaded std 只查公开 facts；generic 两项走既有手工前缀，仅作集成共存/链接邻控，不当成新续接直证；native 沿原 success-only oracle。
- 随后 fmt、frontend build、frontend all-targets check/clippy（build/check/clippy 均 offline/locked，clippy `-D warnings`）、docs、尺寸及 diff 七项各一次 exit0。3297输入、12工具、14配置检查与六产物前后核同，无漂移、重试或超时；731文档通过，1849手写 Rust 中38项历史超限照报，修改文件460行，无新尺寸例外。
- 原始 JSON/stdout/stderr/receipt 留仓外 `e2-unit-names-a1-validation/`：基线3结果 `33c47af8`、独立5结果 `f91f5566`；合成11结果 `f26b8c35`，最终11＋7 manifest `45469a1e` / 189件SUMS `1b50f453`，均独审。六个合成受验 binary 的原字节另存只读副本，index `141e1457`；target 后继复用不改历史收据。11是 libtest 调用数，不冒充 CLI/link/native 子进程总数。
- 本片仅完成 A2 的 parse-once 直接前置；CLI single 尚未切换，A2 的 root/import/entry、真正跨 crate caller 与可见度、旧 single checker/lowerer 退役及 D10 终态仍开放。未运行整 target/全仓、合成事实或外部 rustc 编译契约、注入/计数/校准、预期 Abort、sanitizer、其它宿主或远端 CI；纯 docs 后继不声称重跑行为。

## 37. 统一 Unit driver 直接前置 A2：初始 package 完整性事实（2026-10-19）

- parser 仅在原 qualified-name 失败分支记录名称完整性，与初始 package 的原 separator 结果合取；separator 仍无条件按原次序调用，无 package 为 true，后续 package/import/body 错误不污染。原 bump/recover/emit、公开 headers/roots、四 AST 表 ID/Span/payload 与诊断保持；真实 crate-private getter 供 index 消费，不提前公开 API。
- index 删除全文件 L0048 扫描，唯一 mismatch gate 改用上述初始事实：poison 或初始 L0053 不制造假 L0146；初始合法、后续 package 损坏时仍按真实路径报告 L0146。其余校验、声明收集与完整诊断排序不变。相对§36源头 `561393f5`，最终五文件+646/-36净610，其中四生产文件的生产区+45/-36净9，两测试区+601（parser 同模块430、index integration171），新增五个 ordinary exact。
- 先在旧生产 `29005eb8` 以两 exact 固定17份真实源码完整公开产物/四 AST payload 与 Diagnostic oracle，均1/0/0/exit0；再在 tests-only `452e877f` 各采三项真实 index 红，均0/1/0/原exit101：poison 与缺 separator 多报 L0146，late package 错误掩盖应报 L0146。红轮每项只计首失败 Eq；第二 poison、late 匹配路径及后续结构断言归绿轮，不冒领红覆盖。
- 合成§36文档后的 `42e44c9b` 原15 exact 全绿，但工程首 fmt 实际 exit1，后六门未执行；后继只改三文件空白与五处可选尾逗号，fixture/oracle 字节保持。最终受验 HEAD `06a55c80f78e438d61d44e8f23e21054bf6c7976` / tree `8450f3b3cebac6c801ace68bfefdb377cbabd551`，clean；原红、旧15绿与 fmt 失败分别保留。
- 新头唯一 frontend lib＋三个 integration 定向 no-run 54.66s/0，实际 JSON 绑定四 executable；原15 ordinary exact 各一次1/0/0/exit0，包含 parser 两项与10个新位断言、新 index 三项、既有 index/parser_file 五项及 A1 五项。原17源 oracle 全部保持，三个真实诊断红同 oracle 转绿，之前未到的分支全部走完；旧 A1 Debug/两遍断言只按原强度记账。
- 随后 fmt、frontend build、frontend all-targets check/clippy、docs、尺寸及 diff 原七门各一次 exit0；build/check/clippy 保持 offline/locked，clippy `-D warnings`。3297输入、12工具、配置/环境与四产物前后核同，无漂移或超时；731文档与1849手写 Rust/38项历史超限通过，无新尺寸豁免。仓外 `e2-header-fact-validation/` 保完整绑定/raw/receipt：baseline 27件 `34365fc9`、index 红33件 `9d52a1ba`、旧头 fmt 失败170件 `d4bca8fc`、最终15＋7的200件 `97b5fa5d`；各受验 binary 原字节另存独立只读副本，后继复用 target 不回写历史。
- 本片完成初始 header 事实与 index 诊断纠偏，A2 统一 driver 尚未接线；root/import/entry、真正跨 crate caller/可见度、CLI single 切换、LSP/其它生产消费者与旧 single checker/lowerer 退役仍待，D10保持开放。未整跑 target/全仓、合成事实、故障/计数/校准、预期 Abort、CI-only、其它宿主或远端 CI；纯文档后继不声称重跑行为。

## 38. 统一 Unit driver 直接前置 A2：未加载 import 说明（2026-10-19）

- 仅在 Unit resolver 原 wildcard／exact-prefix 失败分支补私有可选说明，生产 `names.rs` +51/-4；原 L0148 code/message/primary Span 保持。确知所需完整 package 未加载时，有意新增有序 Note→Help，说明当前 compilation unit 未加载该包，并提示显式 project source set、single-file 可用 `--project`；这是批准的诊断改变，不称全文零变化或自动 import。
- exact 仍先查原 `segments[..last]` 完整 package；成功查找、reference、visibility、alias 与冲突不变，真实长 package 优先。已加载包缺末端声明、声明 member 尾段、较短 prefix 有声明的歧义及单段 default-package 未分类仍原 L0148、无未加载提示；说明保原段名，不在 CLI 匹配英文或增加公开错误框架。
- tests-only `ee647eb7` 保旧生产，三项 ordinary 首采实际1红2绿：unloaded exact 原 exit101，仅完整 Diagnostic 的三条有序 Note/Help 缺失，其余全文/顺序/Span 同；该轮 line510 后 reference/localValue 断言未达。production `fd608118` 的原3新＋5旧各一次 actual1 passed/0 failed/0 ignored/exit0，完整引用断言到达；原 source/oracle 未改，普通 `koven.algorithms` 测试源不冒充 loader 授权。
- 按两原提交顺序合入 header docs source `01c10245`（与已发布 actual `e09f9c39` 同 tree `b4827c9f`）；受验 HEAD `5db8dd10f202340b13d44c7fa5f3465149732078` / tree `675235e4ef0b50320301c5468b342506d7da7638`，clean。唯一 no-run 56.956s/0，由本次 Cargo JSON 绑定 frontend lib＋四 integration 的五 executable，五项及 production lib 的 fresh 均 false。原 header15＋import8 去重23项逐项一次1/0/0/exit0，3297输入、12工具、14配置与五产物前后核同；无本轮 binary 整套副本。
- 同头随后 fmt、frontend build、all-targets check/clippy、docs、尺寸及 diff 原七门各一次 exit0；build/check/clippy 保持 offline/locked，clippy `-D warnings`。含前后核验耗时14.569/2.482/24.541/30.491/5.306/7.367/2.366s，731文档、1849手写Rust／38项历史超限通过，无新增尺寸豁免。完整仓外 `e2-import-failure-validation/` 分存旧红36件 `336693ae`、候选8绿66件 `c59462fa`、合成23绿152件 `c1622178`；最终23＋7结果 `d8346222`、200件SUMS `6cf0919a`，原失败与覆盖边界保持。
- 本片只补 Unit resolver 的未加载包解释能力；single CLI 尚未切换，A2 driver 接线、root/entry、loader 展示名碰撞、LSP／其它消费者及旧 single checker/lowerer 退役仍开放，不关闭 D10。未整跑 target／全仓、CI-only、合成事实、故障注入、预期 Abort、其它宿主或远端 CI；纯 docs 后继不冒称重跑行为。

## 39. 统一 Unit driver 直接前置 A2：标准源展示名碰撞解耦（2026-10-19）

- 只改 CLI 标准 loader，生产净 +14 行：仍先注册原展示名，仅对结构化 `DuplicateSourceName` 依序尝试 ` [standard source n]` 后缀并取首个空闲名；其它错误原样传播。上界 N 为追加前 descriptor 数，私有 caller 必须为每份既有 source 完整登记 descriptor；不扩大 SourceMap API、不无界重试。用户展示名不改，标准 logical root/path、两项授权顺序及成功注册的真实 SourceId 保持，不按内容或包名授信。
- tests-only `d0d03be4` 原七项首采实际2绿5红：无碰撞 A1 与旧 E 拒绝各1 pass/exit0，五碰撞各1 failed/原 exit101，均为真实 `DuplicateSourceName`。红轮首个 unwrap 后的断言及 B1 第二顺序未达，完整原收据保留。生产 `45346492` 后仅以 `50c11086` 单行迁移 E 的完整 Name→Key oracle；原7新＋3旧 std 各一次全绿，canonical、N=3 空洞首选、连续碰撞双顺序、权限、完整诊断与真实 ranges 来源链全部原硬断言到达。
- 在 source `574cd2b9` 上按 `baabb46c` tests→`f7495703` production→`e83a8cd0` E oracle 三小提交整合，差集仅原三个 CLI 文件。最终受验 HEAD `e83a8cd0dfc04cf2153249296f68b0330513a11d` / tree `c34f4c435f3865bb36ba22e2091fb18897753e5f`，clean；唯一 combined no-run 54.821s/0，由本次 Cargo JSON 绑定三个 harness 与非 test kovenc 四产物，fresh 均 false。原 collision10＋import8 共18个 ordinary exact 逐项一次1 passed/0 failed/0 ignored/exit0；header15 不经 loader，按真实影响审不机械重复，import 原后置引用断言保持。
- E 的真实 project root/key 本就重复，仍在 human/json 两 format 中被拒绝：旧版完整 SourceName stderr、修复后完整 SourceKey stderr 均由各自普通测试实采比较，CLI exit1、空 stdout、无最终产物或临时遗留。此绿证明展示名碰撞已与 logical key 错误解耦，不证明重复 key 项目可成功构建；通过时未另存 child stderr raw，证据为冻结完整 oracle 与实际 harness raw/receipt。
- 同头随后 fmt、CLI production build、frontend＋CLI all-targets check/clippy、docs、尺寸及 diff 七门各一次 exit0；build/check/clippy 为 offline/locked，clippy `-D warnings`。3297 输入、12 工具、14 配置及四产物前后核同，无漂移、重试或超时；731 Markdown、1849 手写 Rust／38 项历史超限通过，无新尺寸豁免。仓外 `e2-standard-display-validation/` 保留 baseline2绿5红结果 `53278650`、candidate10绿 `8b65bd54`、最终18绿 `93d2e415` 及18＋7结果 `4e75ddf8`／174件SUMS `6166efb7`；不复制 binary 套装，历史源码/raw/SHA 保留，不声称旧 binary 仍在复用路径。
- 本片仅完成共享 loader 的展示名直接前置，single CLI 尚未切换；A2 接线、root/entry、跨 crate caller/可见度、其它生产消费者与旧 single checker/lowerer 退役仍开放，不关闭 D10。未整跑 mixed target／全仓、CI-only、私有事实／合成 Rust／故障／Abort 或 native 程序，未运行远端 CI；独立 selector/docs 后继不混入上述受验树，纯 docs 追加不冒称重跑行为。

## 40. 统一 Unit driver 直接前置 A2：entry 选择输出基线（2026-10-19）

- tests-only 受验 HEAD `f4bb65c435ff1243e9567750bdc0b2fe34c4cfc3` / tree `e13d56e0c43d0591efe912f56c2350de7615a799`，clean；仅两个 CLI 测试文件。source base `32d79ff9` 与已发布 actual `3078e2e24ae2b0db79159e841130112ebc1c4de6` 同 tree `e472572e`，原 selector 与标准源 E 回归保留；本补记是独立 docs 后继，不冒称重跑。
- 原 conventional 四失败提升为 human/json 完整 exit2／空 stdout／含 LF 的全 stderr，显式成功保持；新增同一物理真实源的 single/project 五次对照，固定 private 选择、explicit 同名全部计数与 project 有效 shape 过滤差异。复用原 project entry 全文邻控，三个 exact 各一次 actual1 passed／0 failed／0 ignored／exit0，所有硬断言到达。22 是成功测试函数体的 9＋5＋8 计划调用数，不是 OS 子进程遥测；child 输出由完整 oracle 比较，未另存 child raw。
- 唯一 no-run exit0；本次 Cargo JSON 的两个 integration binary 与实际 `CARGO_BIN_EXE_kovenc`、3297 源／12 工具／14 配置前后核同，三 exact 无漂移、重试或超时。完整原始证据留仓外 `e2-entry-oracle-validation/`，结果 `d31015c7`／31 件 SUMS `a07aabd9`，独审通过。同头 fmt、CLI build、CLI all-targets check/clippy、docs、尺寸及 diff 七门各一次 exit0；后三项保原只读脚本与 common f2d 比较，731 Markdown／1849 手写 Rust／38 历史超限照报，无新增尺寸例外。工程原始证据留 `e2-entry-oracle-engineering/`，结果 `2d120199`／46 件 SUMS `0e048936`；这不是全 workspace 工程重跑。
- 本片只冻结 entry oracle 基线，没有生产行为修复或 CLI Unit 切换；无体入口尚未真实全链采样，root／symlink 实现与验收、A2 实际 caller／身份与旧 single 退役继续开放。未运行整 mixed target、CI-only、合成事实、故障注入或预期 Abort；不据本片关闭 D10。

## 41. 统一 Unit driver A2：single CLI 共同编译管线（2026-10-20）

- source base `ee9fc54c8f16ce8badab8da03eeefe7cc0444264` 与已发布 actual `a9301113b9947cfbf3d28dd8f4d73de85c60930c` 同 tree `c4c6caf674158e75eda50ecefe6267d0478624e8`。生产提交 `2bac18bf92fa62e64d7556e5add9886081f60475` 将 single bootstrap 接到与 project 相同的 compilation-unit names/type/ownership/native 管线；不是仅切换到共用 lowering helper，不改变现行 Guide。原单文件 checker/lowerer 尚有其它消费者，未在本片退役。
- single 原用户源 lex/parse 一次，初始 header 不完整先返回原语法诊断；完整 header 以已声明 package 核对用户给定路径的逻辑目录尾组件并推导 root，显式 root 仅以物理 identity 比较，不跟随 symlink 目标重推 package。原 ParsedFile 按值交付共同 Unit names，保真实 SourceId；只加载选中文件及既有真实授权标准源，不从 import 自动读邻文件。single/project 各自 entry、source-set/路径和操作输出协议保留。
- 六个新 `single_unit_cli` ordinary exact 先在 tests-only `f33104bb` 的旧生产逐项采真实红，各 actual1、原 exit101；红轮仅记首失败断言，不冒领后继未到子例。原测试源和 oracle 在 `2bac18bf` 六项逐一转绿，各 actual1/exit0。共享 target 曾因相对 dep-info 与源 mtime 误复用旧产物，该次 fresh:true 收据保持并明确无效；只刷新已审 11 源 mtime、内容 SHA 不变后，真实 rustc/fresh:false 的 r4 构建才作为候选证据，CLI SHA `d80c1e7c`，未删缓存或伪造候选身份。
- 普通 build-only 观察先采旧 CLI 四次 bodyless，再采候选二十二次（八旧诊断源双 renderer、single bodyless 四次、project bodyless 两次），全二十六次保完整 raw/exit/产物状态，不运行所得产物。八个 golden 只按独审后的真实 Unit producer 补 lexer/parser 后继诊断和 ownership/const_move 参数 label；其余 human/JSON golden 全文不变，九个原 `.ko` 与原 128 组合不缩减。bodyless 新增四 single＋二 project 的完整固定输出断言，single 原 exit2/UnsupportedNode 与 project exit1/entry-shape 错误单列，不将两者随意拒绝记为语义一致。
- tests-only oracle 提交后受验 HEAD `3df555a3877084830dc209b7a38aae9efc555abd` / tree `42a6975660df7e47fe876afd41efb93910e7acfd`，生产与六新 exact 字节仍同 `2bac18bf`。唯一定向 no-run 的四 integration＋CLI bin 五个 harness 均实际 fresh:false/真实 rustc，生产 CLI 复用已核 r4 同 SHA。二十一个 ordinary exact 各一次 actual1/exit0，无超时或身份漂移，涵盖 project 诊断/entry/argv、两 native argv、两个 Unit entry、两个 bootstrap 邻控及 single-file 全部五项硬 oracle；此前六新 exact 不冒称在此 test-only 后继重跑。
- 同 `3df555a` 顺序完成 fmt、CLI build、frontend＋CLI all-targets check/clippy、docs、尺寸及 diff 七门，各一次 exit0；build/check/clippy 保 `--offline --locked`，clippy `-D warnings`，尺寸和 diff 保 common `f2d8bf7a540f8aef27031f299ec4d38641f941e1`，不缩新基线。731 Markdown、1850 手写 Rust／38 项历史超限照报，无新尺寸豁免。3301 输入、12 工具、14 配置及 registry build-script 来源均前后绑定。
- 固定原 35 场景／70 single-project 臂与独立新增 4 enum／8 臂保各自原 cases/源码/argv 全字节，真实 CLI 每臂 human＋JSON 各一次，共 156 build；并非重跑已移走 API 的旧内部 probe。实际 110 次 exit0、24 次 single exit2、22 次 project exit1；55 次普通 program 各 exit0，21 个禁 run 槽永不启动，x02 两臂 UnsupportedNode 不执行产物，仍是能力缺口而非支持证明。七普通诊断源经全文安全审核后，从旧 probe backend-forbidden 合同明确改为公开 CLI build 观察／program-run-forbidden，未冒称旧门原样继承；无新增 production hook 或故障操作。
- 核心 35 对的原始 build 结果差 12：七完整前端诊断的宿主 exit 包装差、三个 entry 操作错误、显式参数化 main 的 single 拒绝／project 合法 Borrow argv 成功、x02 共同 UnsupportedNode 的宿主包装。真实 acceptance 差 1 仍单列；七可比完整 FE 诊断向量差 0、23 可比程序结果差 0、未解释模式差 0，其他不可比项不冒称相等。历史 23 个 Unit 成功源在两新宿主均继续成功，11 个旧 single 能力缺口没有退成双拒绝假 parity；额外 4 源两宿主全部成功、4 对程序结果差 0。generic 沿原 success-only oracle，不发明空 stdout golden。两个分组 InvalidSsa 受影响臂与 renderer 出现数均 0，InvalidModel 也为 0。
- 全部新 raw 输出保真实 CLI 展示名；原 source_name 与新 `src/<logical_path>` 映射另存 sidecar，只有冻结 expected diagnostic 的 source 字段作明确映射，不改 raw 或源码。每次原 subprocess 保实际 exit、完整 stdout/stderr、源/产物状态和 hash，核心与增量分别记账。矩阵 summary `d59b5220`／6779 件 result-manifest `70f5d2c3`，结果汇总 `40c9efda`，完整绑定和原文件保留于仓外 `e2-single-driver-cli-matrix-r1/`。
- 原始 red/r4/green/ordinary 与工程日志保留在仓外 `e2-single-driver-validation-r1/`，二十六观察在 `e2-single-driver-observations-r1/`；原失败与未到覆盖不覆盖重写。六新红结果 `9fe66e6a`、六新绿 `878c4401`、二十一 ordinary `051a7581`／107件 SUMS `46454eb7`、七工程 `a5fc10bd` 分账，真实 child 行为由原硬断言验证，未另存其所有 child raw 的 exact 不冒称 OS 全遥测。
- 本片不关闭 D10：LSP raw recovery、其它生产消费者续接及旧 checker/lowerer 真正删除仍开放。未运行整 mixed target／全仓、私有 facts／foreign IDs／合成 Rust、故障注入／计数／校准／预期 Abort、其它宿主或远端 CI；后继纯 docs 不冒称重跑 Rust 行为。

## 42. A2 CI 回退修复：concrete generic value class（2026-10-20）

- §41 后实际发布头 `cb4fd4ae` / tree `f9a48269`（本地 `32af6ba7`）的 PR CI `39354995533` 在 Linux/macOS 同一原 bootstrap exact 失败：原 `prelude.ko` 的 `Pair(20, 22)`、SourceId(0) `[367,379)` 到 Unit lowering 返回 `UnsupportedNode`。此前固定39场景没有泛型 value class，局部绿不能代替原整项 CI；原测试及标准源码保持不变，其含 intentional Abort 的完整 exact 仍仅 CI，未在云端拆分执行。
- 两个独立新普通 fixture 在 tests-only `32a73813` 先各 actual1 / exit101；8次 single/project × human/json build 均因同类 Unit UnsupportedNode 拒绝，产物与程序运行均为0，single exit2/project exit1区别保留。原始 stdout/stderr/receipt 全存，不把工具或 manifest 失败当目标红，不按候选输出反造 oracle；两 fixture 不是原混合 bootstrap 的摘录。
- 最小生产修复仅 `unit_lower/aggregate.rs` 与 `type_lower.rs`：构造、局部值投影、storage eligibility 与 interning 统一消费既有具体字段 resolver；保 exact owner/有序 arity/字段身份，aggregate field 用 resolved concrete type 而非模板类型。ValueClass 仅复用 direct T 或已闭合字段 fallback，不新增 frontend layout descriptor、nested/nullable T recipe、prelude 注入或加载政策。
- 本地 `17cb0d24` / tree `14d5ac1d` 保真实红提交为父。七个有界 ordinary exact 均 actual1 / exit0：上述两新 CLI exact、迁移的原泛型 nominal SSA exact、3个既有布局/解构邻控及新 current-receiver 来源边界。8次 CLI build 与8次程序运行全部成功并匹配预先明确的完整输出；copy/explicit named fields、异型实例、独立 enum 两 payload cases、MoveOnly whole-value/Consume 解构覆盖，不声称动态释放计数或无泄漏。原 MoveOnly 字段读取/临时 receiver 拒绝源与断言保留。
- generic ValueClass 的 implicit This、this 与递归 Group(this) 投影继续拒绝，普通具名局部值投影不受该 guard 阻挡。新5份真实源码均经完整 names/types/ownership 后仅到 SSA；锁 UnsupportedNode 及实际字段使用的同源/start/end/bytes，不构造私有事实、不执行 native。旧 single 的 readonly-receiver admission 也未支持泛型 ValueClass member；这是既有 lowering 边界，不升为语言禁令或 D10 全能力保全结论。
- 两次单 target no-run 真实 exit0，CLI 构建涉及的4个 workspace 目标（frontend/codegen/CLI/test）和 codegen libtest 均有对应 fresh:false 与实际 rustc 来源；新 CLI `de3afd69`、CLI test `1fe2fd38`、codegen test `546e27b2` 已绑定完整3304源/12工具/14配置及既有依赖归档。CLI 测试文件与两份 fixture 从红到绿逐字一致；旧红/旧 CI/未安装候选历史不覆盖。原始证据在仓外 `e2-generic-value-validation-r1/`，red summary `9d20bf87`、七绿 summary `8f9b436f` / 68件 SUMS `29d8be4e`。
- 首轮工程 fmt 真实 exit1，仅新增 SSA 测试两处换行；精确纯格式提交 `6ea42494` / tree `7f4c5f85` 保 tokens、字符串和断言不变，未改生产及 fixture。随后 fmt exit0、codegen libtest 真实重编为 `51353493`，同七 exact 再次逐项 actual1 / exit0；CLI 与其 test 二进制沿前轮真实构建来源复用并逐字核同，不伪称重新编译。后继 summary `d35d6f39` / 67件 SUMS `5421f4bd` 与首红均保留。
- 6ea 后九项工程实际 exit0：fmt、CLI build、codegen/CLI 各 all-targets check/clippy、docs、固定共同基线 `f2d8bf7a` 的 sizes/diff。build/check/clippy 保 offline/locked，未执行整测试 target；731份文档通过，1851个手写 Rust 文件中的38项旧超限照报，未抬限额或缩基线。9项结果 `0a99bf38` / 64件 SUMS `c53f216a`，最终 CLI SHA 仍为 `de3afd69`。
- 修复后在 `6ea42494` 重新执行固定39场景/78臂/156次 build，而非继承§41通过：35核心为70臂/140 build，4项 enum 增量为8臂/16 build；110次 build exit0、single 的24次 exit2 与 project 的22次 exit1原样保存。55次普通程序均 exit0，21个禁 run 槽未启动产物，2个 x02 臂仍拒绝且未 run，不把该已知能力缺口称为补齐。核心 raw build 差12、acceptance 差1保持，包含参数入口的已明示 driver 差；7组完整 frontend 诊断与23组可比程序均无语义差，4项增量双宿主也均成功且程序结果一致。必需成功 loss、意外接受、未解释模式差与精确 InvalidSsa 出现臂均为0；原39源、完整 oracle、双 renderer 及门数未缩减。新 summary `857304ce` / 6779文件 manifest `49c5d561` 保留完整原始结果，观察与允许 driver 差不冒称语言能力总验收。
- 发布前静查确认双平台 CI 的未过滤全包测试选择上述新 CLI target 与新/迁移 SSA，未为它们添加重复执行命令；这不替代真实 CI。新发布及精确头双平台 CI 尚未发生，原 bootstrap 完整 exact 与新增回归的实际命中仍须发布后核验。LSP snapshot finish 候选暂停、未安装；Legacy recovery、旧 checker/lowering 退役和 D10 终态仍开放。

## 43. A2 CI 阶段边界修复：companion callable 能力诊断（2026-10-20）

- §42 实际发布头 `dcec2a8b` / tree `d62eecee`（本地 `964f6691`）的 PR CI `39365329979` 在 Linux/macOS 均通过原完整 bootstrap 和七个 generic-value 修复 exact，但稍后的 native_cli 原 `single_static_recovery_build_preserves_source_roots` 失败：原 `p28_companion.ko` 的两 renderer build 泄漏 `InvalidModel/MissingFact`。这是后续目标的新失败，不回写或混同已修复的 `Pair(20, 22)` 回退；双平台原始日志保留，原 CI 总结果仍为失败。
- 原 p28 在 Unit 具有完整合法 typing、无 Deferred/diagnostics，不能为恢复旧 single 的 L0165 伪造前端拒绝。planner 未枚举 companion callable 模板才产生缺项；按 Guide 未完成后端须结构化拒绝，本片不新增 companion 函数执行、隐式 receiver、enclosing 泛型捕获或 prelude 加载。旧 single 无该 fixture 的 native 成功能力，改诊断不计新增 native 成功。
- 生产只改 `unit_plan.rs`、`lowering_support.rs` 与 `native.rs`。保持原 preflight/resolve 次序，仅在模板查找缺失且 resolved target 与原 selected Symbol 一致、真实签名属于 companion、has_body、两 receiver 均为空、own type-argument arity 匹配时，产生私有 `UnsupportedCompanionCall`；既有公开 `UnsupportedSource` 保 source Span，专门说明为 `companion function calls are not supported by native code generation`。其余缺模板仍 MissingFact，原 UnsupportedNode 文案不变，不靠名字/AST shape 猜测或修改事实。
- tests-only `9f5d633f` / tree `28e52b53` 在旧生产上先采四项 ordinary whole exact：三项真实 exit101，unused companion＋可达 top-level/instance 正控实际 exit0 并 emit 合法 object。六次 CLI build 完整收齐，p28 两 single 与新增两 project 均为目标 MissingFact；p16 原两 single raw 仍 L0165 `[185,194)`。native 三源×两输出状态的六个真实错误均收齐，原 P 为完整 Call `[95,105)`，独立带实参 `Factory.identity(7)` 为 `[109,128)`。首错误断言之后的期望文字/Span 与 p16 hard assertions 在红轮未到，不能冒领覆盖；全部产物均未执行。red summary `e147f36b` / 54件 SUMS `13608868` 保原输出和首断言限界。
- 绿色 `7e698713` / tree `5200ee36` 仅接三生产文件，红轮五测试/helper 文件逐字不变；先 fmt exit0，再 CLI 两 integration、codegen libtest、frontend final-admission 三次定向 no-run 均 exit0。CLI 构建涉及五个 workspace 目标，codegen libtest 与 frontend integration 也均有 fresh:false/实际 rustc 来源；新 CLI `3d22200d` 与四 test binary 绑定完整3305源、12工具、14配置及既有依赖归档。七个 ordinary exact 随后各一次 actual1 / exit0 / ignored0：原四项转为全绿，另含原 project p16 及两项 frontend 正控；8次 build、6次拒绝 native API 与1次成功 object emission 各自既有/新增全部硬断言到达，无产物执行。green summary `7975bab1` / 81件 SUMS `e48c8067` 与红轮独立保留，无重试或漂移。
- p28 CLI 旧 L0165/callee `[95,103)` 明确迁移为完整 backend operational error，single exit2/project exit1 与 empty stdout 保持；human/json 都锁同一完整 stderr，现有操作错误不公开 source Span，不伪造语言 JSON diagnostic。native API 另锁真实完整 Call 同源/start/end/bytes、public kind、完整说明、diagnostic=None 和已有/不存在目标＋邻文件的全目录字节保全；p16 原源码与 L0165/span/schema/全部槽保持。新增 project p28 的两槽是追加邻控，不冒称原已存在四个 p28 CLI 槽。
- 同一 `7e698713` 九项工程实际 exit0：复用该头已完成 fmt，随后 CLI build、codegen/CLI 各 all-targets check/clippy、docs、固定共同基线 `f2d8bf7a` 的 sizes/diff 各一次。build/check/clippy 保 offline/locked，未执行测试体或 aggregate；731文档通过，1852个手写 Rust 中38项旧超限照报，无新豁免。工程结果 `ea5b042c` / 68件 SUMS `6acc25f8`，最终 CLI 仍为 `3d22200d`，无需后继格式修正或重编。
- planner 修复后在 `7e698713` 重新跑固定39场景/78臂/156次真实 CLI build，未继承旧矩阵：35核心70臂/140 build，4项 enum 增量8臂/16 build，实际110次 exit0、single 24次 exit2、project 22次 exit1。55次普通程序均 exit0，21禁 run 槽和2个仍拒绝的 x02 臂未启动产物；x02 不记成能力补齐。核心 raw build 差12、acceptance 差1保留，7组完整 frontend 诊断和23组可比程序无差，4项增量程序对也无差；必需成功 loss、意外接受、未解释模式差、精确 InvalidSsa 与 InvalidModel 出现臂均0。39原源、全部预定 oracle、双 renderer、门数和原 argv 保全。summary `da0c3eef` / 6779文件 manifest `a63a9e8b` 保完整输出，424身份快照、1907进程记录含校验进程，不冒称1907次编译。
- 后继独立 CI 提交 `5c8c1332` 仅按已审四文件差集给 lang-cli 的既有 Cargo 命令增加 `--no-fail-fast`，任一目标失败最终仍非零；strict policy 同步整行，既有失败传播合同新增 CLI status37/恰前三命令断言，testing 短句说明包内收集、包间与阶段早停。故障 stub 合同未在云端执行，实际传播仍只归 CI；无新 target、并行、重试或通用框架，不声称全流水线持续执行或新增硬 timeout。CI/docs 后继不改已验 Rust/fixture，不冒称在后继头重跑七 exact 或矩阵。
- 原始输出、精确 source/tool/config/archive 与真实二进制绑定留仓外 `e2-companion-capability-validation-r1/`，固定矩阵另存；本轮不在云端运行整 mixed target、私有 facts/foreign IDs/合成 Rust、故障注入/计数/校准/预期 Abort。新发布头的原失败 whole exact、原 bootstrap 和新增目标仍须双宿主真实 CI 命中，全 run 通过前不关闭回退。LSP 实施保持暂停；Legacy recovery、旧 checker/lowering 退役与 D10 终态仍开放。

## 44. A2 tutorial 完整诊断 oracle 补迁移（2026-10-20）

- §43 发布头 `8cb11c51` / tree `0b3f3fcb`（本地 `563e8410`）的 PR CI `39371590093` 在 Linux/macOS 均实际通过全部93个受跟踪 target/name 身份，包括两个历史失败的 whole exact；随后原 tutorial 在 `reject-ownership` 的完整输出相等断言失败。两宿主的 expected/actual 进程字典完全相同：两条 L0131 各多一个真实 selected-parameter label。此前12个 tutorial row 到 `reject-typed` 已报告通过，但后续 tutorial 与资源阶段没有完成证据，CI 总结果仍失败。失败相等断言先于该次无产物检查，不把临时目录清理冒称失败原子性证明。
- 原 Markdown 源与 `scripts/check_tutorial.py` 全字节保留，只补 `examples.json` 两个 `build.stderr` 字符串。`reject-ownership` 保两 L0131、primary `[154,162)`/`[185,193)` 及 move `[122,130)`，各追加实际 `take` 参数 `[26,48)`；原源码及新全文也与§41已迁 ownership fixture/golden 相同。相邻 `reject-iteration-move` 保 L0135、primary `[151,156)` 和 loan `[126,131)`，改为共同 Unit 的 `move conflicts with an active loan`，追加实际 `consume` 参数 `[25,47)`。真实 selected callable/signature→ownership contract→consume/access_place/ensure_available 的来源链不改；不删 label、不宽化错误断言、不新增语言规则或 native 能力。
- 修改前四个原 diagnostic fence 各一次 JSON build-only 观察均为真实 exit2、空 stdout，保存完整输出、源码与目录前后状态；目录始终仅同字节 `source.ko`，无产物执行。`reject-typed` 与 `reject-immutable-place` 的完整输出不变，仅前述两项需迁移；显式 producer 差集构造的新字符串再与观察全文核等，不批量录制 golden。观察留仓外 `e2-tutorial-diagnostic-observations-r1/`，结果 `754558c8` / 26件 SUMS `d91489b8`，不计作测试通过。
- 独立 JSON 提交 `53acd0f3` / tree `83ac1a66` 保其余3304文件字节不变。原 runner 随后按四个单独精确 `--example` 调用各一次真实 exit0；每次 stdout 恰为该 ID 的 passed 与 `1 executed cases; 0 planned`、stderr 空，内部完整 exit/stdout/stderr 相等与 source-only 目录断言均到达。每条120秒上限，前后全源/工具/CLI身份一致；没有 whole tutorial、mock/stub、产物运行或重试。结果 `44d41824` / 32件 SUMS `679e6cc8` 留仓外；四次 build 数量来自已审单ID诊断闭包，不冒称单独 OS 子进程遥测，绿色 child raw 由原 runner 比较但未另行输出。
- 本节是受验 JSON 提交之后的独立文档后继；安装后仍须重绑最终 HEAD 并实际检查 docs、固定共同基线 `f2d8bf7a` 的 diff/sizes 与只读 JSON 选择完整性，不能用候选静审代替这些最终门。教程 JSON 明确参与双宿主 composition 选择与输入 fingerprint，不能用前一红头结果复用为新头通过。
- 本片不改任何 Rust/Koven/runner/标准源码或故障合同，不重编，不把既有七 exact 与固定39矩阵说成在文档后继重跑。新发布头仍须原 whole tutorial、全部后续阶段及双宿主 CI 实际完成；原失败日志保留。LSP 继续暂停，Legacy recovery、旧 checker/lowering 退役和 D10 终态仍开放。

## 45. LSP 同轮 snapshot 收尾内聚首片（2026-10-20）

- §44 两文件修正发布为 `b014a6e1` / tree `91ea7ce5`（本地 `6d6fd2be`）；PR CI `39377059046` 实际 completed/success，16项 job 全部成功。Linux/macOS 各93个受跟踪身份通过一次，原 whole tutorial 各32 executed / 1 planned，后继资源与两 producer、两 independent consumer 及 aggregate 门均完成。原三个失败头仍保留失败结论；macOS generated-resource 的允许 partial、Linux 未覆盖 UBSan 等限制不被全绿抹掉。以该精确新头解除 LSP 暂停，不把旧失败头或本片尚未发生的 CI 视为通过。
- 本片只把原 `UnitSnapshot::finish` 的同轮 name owner、optional raw typed/owned、完整诊断及 definition facts 移入内部 `UnitAnalysisSnapshot`，现 `UnitSession` 立即消费。原 names/type/basic ownership gates 与聚合不变，先排序完整诊断再构建导航，两底层错误逐值映回原 session 变体；registration-order source_ids 映射、URI/version、overlay、prepare/publish/commit/last-good 仍留宿主，SourceMap/Span 的真实 owner 随 name_snapshot 内聚。三测试文件仅 getter 投影，完整 raw Eq、same-analysis/compatible、AST 指针配对、诊断/位置/导航断言保留；独立 manual 链和全部原 fixture 不改。不是新增语义行为，不伪造先红后绿。
- LSP 实际19个 registry 依赖重新核官方 archive/lock/index 及全部源码闭包，不能沿用 CLI 依赖覆盖。仅复制原已取得、目标尚不存在的10个官方归档，再一次 offline/locked fetch 成功；该 fetch 覆盖 workspace 42个 lock 包，不冒称仅19包。7个 build.rs、实际 helper、两个 proc-macro 及其支撑库已独审，官方固定 rustc metadata-only feature probe 与项目故障/校准测试区分；实际 serde/core private.rs 逐字匹配官方模板。Cargo build-script 事件可能重放缓存，proc-macro2 的 fresh:true 不声称 probe 重跑；不编造依赖源码或 marker。
- `6d6fd2be` 基线单 LSP bin no-run 实际 exit0，43.18秒，唯一 harness `02ce0c6a` 有 fresh:false 与真实 rustc --test 来源；原14个 ordinary exact 依序各一次 actual1 / exit0 / ignored0，summary `5429db2b`。完整诊断与导航、raw recovery 身份、Unicode/CRLF、版本和 last-good 均按各原硬断言验证；P3 的真实普通 `this` 源在 finish 之前触发原 UnsupportedBody，仅证明旧 snapshot 保护与后续正常更新，不冒充 finish 错误排序的动态证明。三项主动错误/transport 故障兄弟留 CI，未运行 whole harness。
- 原六文件候选独立提交 `dce6ecf4`；fmt 首次真实检查只报 `unit_session.rs` 两处换行，原红保留，`5f7ca2fe` / tree `75a76d94` 的独立格式提交仅改语法外空白，随后 fmt 实际 exit0。此前仓外 wrapper 占位键准备错误在首读 binding 时停止，未启动 fmt，不把该准备失败算测试红。候选单 LSP bin no-run 随后真实 exit0，38.48秒、fresh:false 与真实 rustc，新 harness `7834937b`；同14项各一次 actual1 / exit0 / ignored0，summary `911ded05`。3306源、工具、配置、依赖及前后身份完整绑定，原 CLI `3d22200d` 不变，全部失败/绿色原日志分开保留。
- 同一 `5f7ca2fe` 七项有限工程实际 exit0：复用该头已绿 fmt，随后 LSP build、LSP all-targets check/clippy、docs、固定共同基线 `f2d8bf7a` 的 sizes/diff 各一次；build/check/clippy 使用 offline/locked。731文档通过，1853个手写 Rust 中38项旧超限照报，无新增豁免；工程结果 `66474d5a`。这些命令不运行测试体或生产 LSP 进程，CLI `3d22200d` 每阶段核同。没有改 frontend/codegen/CLI 生产或原源码矩阵，本片不冒称重跑固定39矩阵或 workspace 全检查。
- 原始绑定及运行记录留仓外 `e2-lsp-snapshot-validation-r1/`，不把源码/完整日志重复入库。此处没有 Legacy 实际调用方切换、完整 raw recovery/pathless 接线、标准 locator/协议/builtin-only 改动或 blanket RecoveryOnly 策略；没有退役旧 checker/lowering，D10 仍开放。后继文档安装需新头相关工程检查，新发布头仍须完整 CI 实际验收，不能由本轮28次定向通过替代。

## 46. LSP Legacy 十一原源持久诊断与导航 oracle（2026-10-20）

- §45 发布为 actual `438d8c65` / tree `26229bad`（本地 `865fb5b7`）；PR CI `39387718650` 实际 completed/success，16项 job 全部成功。Linux/macOS 各107个受跟踪身份与3个 CI-only 兄弟各通过一次，LSP 各45 passed；whole tutorial 各32 executed / 1 planned、后继资源、两 preview producer、两 independent consumer 与 aggregate 全完成。五资源与四 preview evidence ZIP 内容已核，两 consumer 各12条命令通过；既有平台 partial/未覆盖限制保留，不把本片尚未发布的结果算作该头 CI。
- 本片仅测试资产持久化，不切换 Legacy 生产 caller。原两个 Rust literal 与九份 `.ko` 共十一源逐字保留，`bodyless.ko` 保字节但不是第十二输入。原两个独立 manual 邻控仍先行；临时观察 exact 将同源的 manual 与真实 production 完整 snapshot 比较后，按唯一 marker 输出，使用 `--show-output`，没有 `RUST_TEST_NOCAPTURE`。观察头 `62d5c762` / tree `8707e180` 的三 exact 各 actual1 / exit0 / ignored0，单 LSP no-run 有 fresh:false 与真实 rustc，新 harness `285a351d`；观察结果不是 golden 自动批准。
- 已审只读提取器仅核固定日志/收据/hash、唯一 marker、JSON 与十一源身份，原样提取 payload；独立逐项审阅全部 raw/LSP/导航及其来源后，286641字节 JSON（SHA-256 `da3a2d4d1c5b03fd5bb3454bd0360cc3d979eb3f419216486ec360b1dd705c2d`）原样成为真实测试 fixture。32条 raw/LSP 诊断、19个有序 label、2612个 UTF-8 查询边界与973个非空查询完整保存，所有已观察非空查询均单目标。`success` 空诊断有真实依据，`typed_definitions` 的 L0133 原样保留，不误称无诊断源。
- 新持久 exact 以固定 `include_str!` JSON 为唯一 expected，先锁 schema、十一行长度、顺序、label、URI、完整原文和 byte length，再将真实 `analysis::analyze` 的整个 snapshot 与对应固定对象相等。完整 raw severity/code/message/primary Span/有序 details、LSP diagnostic 对象和每个 UTF-8 字符边界的全部有序 definition targets（含空结果）均硬比，不排序/去重或仅核计数。原独立 UTF-16 target 断言保留；不把 CRLF 每个 byte 边界称为合法 LSP cursor。无 live expected、UPDATE、环境开关或重生成路径；旧 capture exact 被替换，不以其历史绿代替新测试。
- `manual_stage_diagnostics_only` 在 fixture 中仅作历史辅助，不参与未来 Unit 阶段或 executable 资格期待。父 helper 的临时阶段 clones 移除，manual 正文恢复原 `865f` 字节，原两邻控及其完整 helper/断言保留。此 corpus 未覆盖 warning、Note/Help、多目标候选、跨源、严格 validate/validate_constants、新 Unit 上游资格、import 新合同或 pathless recovery；阶段诊断空也不能推导上游资格。
- 持久测试提交 `9958d8a8` 后 fmt 真实红只要求两处换行，直接按原 stdout hunk 修正、去空白字节全等；`eb021f9f` / tree `ba595666` 的 fmt 随后通过。前一观察轮 `924bc975` → `62d5c762` 也只修原 fmt 实报的空白，两次原红分开保留。持久单 LSP no-run 实际 exit0、fresh:false 与真实 rustc，新 harness `9e264706`；两原邻控与新 `matches_frozen_legacy_recovery_diagnostics_and_every_definition_offset` 各一次 actual1 / exit0 / ignored0。3308源、12工具、14配置、完整已审19依赖闭包及前后身份绑定，CLI `3d22200d` 不变。三绿 summary `d004b5a8` / 61件 SUMS `88183616`，不把旧观察三绿改名计入新持久验收。
- 同一 `eb021f9f` 七项有限工程真实 exit0：复用该头 fmt，随后 LSP build、LSP all-targets check/clippy、docs、固定共同基线 `f2d8bf7a` 的 sizes/diff 各一次；Cargo build/check/clippy 保 offline/locked，clippy `-D warnings`。731文档、1854个手写 Rust／38项旧超限照报，无新豁免；结果 `a48c9fae` / 52件 SUMS `0cce6ee9`。原始观察、提取、格式红、构建及运行记录留仓外 `e2-lsp-legacy-oracle-validation-r1/`，真实 golden 留所属测试目录，不重复提交完整日志。
- Koven 尚未发布，本基线用于审查迁移差异，不是永久保留旧行为或缺陷的兼容标准。单文档功能将直接使用共同 Unit；消费者迁完后删除旧 Legacy 分析入口、checker/lowering、仅依旧链的过渡测试辅助及无实际需求接口，不保双轨、错误回退或仅为旧内部 API 存在的适配层。规范正确的诊断、导航与容错仍须保全；已证旧缺陷和不合理限制可按现行规范与真实 producer 证据逐项修正，并明确更新对应 oracle/差异记录，不以任意拒绝或削弱断言冒充迁移成功。
- 后继文档需新 HEAD 的 docs/common-base diff/sizes 实跑；新发布头仍待完整 CI，不以局部三 exact 或旧头全绿替代。未运行 whole/mixed harness、故障/校准/Abort、私有事实/foreign IDs、合成 Rust 或 native 产物；没有改生产、协议、builtin-only 或 standard locator，也没有重跑原39矩阵。Legacy raw recovery/pathless 与真实上游资格接线、旧 checker/lowering 退役仍开放，不据此关闭 D10。

## 47. LSP 默认单文档直接接入 Unit raw recovery（2026-10-20）

- §46 最终发布为 actual `39e0f850` / tree `4c50211e`（本地 `dbf8b92e`）；PR CI `39400760951` 实际 completed/success，16项 job 全成功。双 OS 各108个受跟踪身份与3个 CI-only 兄弟各通过一次，LSP 各46 passed，whole tutorial 各32 executed / 1 planned；五资源、四 preview evidence 内容及两 consumer 各12条命令均核实。平台允许 partial/未覆盖边界保留，本片后继 CI 不继承该头通过。
- 本片真实默认单文档 caller 不再调用旧 `analyze_single_file`。显式 `MemoryDocument` 与原 SourceId 进入同一 Unit names/type/ownership producer，经 `UnitAnalysisSnapshot` 收尾完整诊断和导航；URI 仅展示，不冒充 root/path，不读磁盘或自动加载邻文件。Rooted 原逻辑路径身份保留；Memory 完整初始 header 取真实声明 package，真正缺省才是默认包，损坏 header 保 `UnresolvedHeader`，不伪造默认/部分 package。单文档模式与既有 `koven.sourceSet` 协议、builtin-only、版本/发布/last-good 边界不扩张。
- raw 与 strict 共用 checker 与真实 owner/provenance。所有 package 已命名且完整 parser/index/name/import 诊断无 Error 的有效性，经 provenance 逐级传给 signatures/types/owned，健康输入继续取得真实 basic/const validate 能力，不能因 memory 输入一律 RecoveryOnly。六张私有同轮恢复表只供真实诊断遍历，公开执行 facts/drop planner 不因恢复失败而放宽。nominal Copyability::Error 只在同一查询实际访问 Error 且没有 Unknown、Deferred、缺身份/arity、循环、invalid-inline 等不完整原因时恢复诊断；不把私有证明变成新的执行能力。
- 无名 fun/val/const/class 沿真实 AST 的 body/initializer/参数继续同一 checker，Missing/Error 名必须有真实 parser 诊断，不造 Symbol/DeclarationId 或跳过 body；健康后继仍可导航。确无实际 receiver 的 `this` 以已启用 Guide 澄清及 L0166 精确标记原 token，合法 instance 与既有可信 range-extension 原路径不变，本轮未新增可信 extension ordinary fixture，未授权 extension 保原 L0164；已有 L0164/companion L0153 的真实来源去重，不 blanket Deferred。source-set P3 旧 `this` 内部失败已不再适用，改以正常源码 `val emit = println` 的既有函数值 UnsupportedBody 验证原 last-good/version/导航保护，未制造故障或削掉断言。
- core125文件提交 `643770f9` 后独立临时 observer 提交仅用于迁移观察。首 fmt 实红按独审76文件/66非空白hunk作语义等价格式修正为 `78c93cdc`，不是声称所有token相同；当时7FE targets及LSP真实 no-run 均有 fresh:false/实际 rustc。30FE＋18LSP ordinary 共48项实际各 actual1/ignored0/exit0。原完整历史 snapshot oracle 真实 actual1/exit101，首对象差异不算绿、不冒领之后未到 fixture；独立 `--show-output` capture 则真实覆盖十一原源，内部错误/超时不被当作允许差异。无 RUST_TEST_NOCAPTURE 或自动更新 golden。
- 固定提取器只核原始日志/收据/hash、唯一 marker 与十一源身份，原样取得274017字节 Unit JSON（SHA-256 `f04c639c4bdb4f4e4607f0c7d40a2318f4ceeaeca42efa62f04895c247a4c8ff`）。独立逐诊断/producer/导航审确认：32条主诊断 code/severity/message/primary/order 不变，2612个 UTF-8 查询边界与973个非空查询及全部有序 targets 不变；仅9条 L0131 新增真实 selected-parameter label，1条 L0133 label 从参数名扩为完整参数声明 Span，label 总数19→28，LSP 仅对应 relatedInformation 变化。不是放宽任意输出或同时接受旧新对象。
- `51b50062` 将固定期望显式改名为 `unit_recovery_v1.json` 并迁移两个测试文件，删去临时 observer；原十一源逐字保留，旧 JSON/红日志留原历史。完整 snapshot 持久 exact 以固定 JSON 硬比真实 production raw/LSP/每 UTF-8 边界有序 targets；L2 完整比两个 relatedInformation，L3/L4 的原前缀/版本/导航硬断言保留。旧 manual-stage metadata 不充当 Unit 资格期待；坐标变换 helper 仅核固定数据，不作为旧引擎兼容层。健康/错误资格另由真实 FE ordinary 验证，不从空诊断或此 corpus 外推全部语言能力。
- golden 安装后的 fmt 真红仅一处换行，纯空白提交 `69807d0f` 后 fmt 绿；LSP实际重新编译，7FE harness 沿 `78c93cdc` 的真实相同输入复用，不假称重编。同52 ordinary（30FE/22LSP）各 actual1/ignored0/exit0，含原3后置oracle和新完整持久exact。随后 Clippy 报一个测试的 clone-to-slice lint，`1e23afc8` 只改 `std::slice::from_ref`，完整 Eq 不变；受影响 FE target 真实重编、其5个已审 ordinary 再绿，不把原52改标新头。该轮首次 fmt 的 transport 断连结果不确定，保留原记录；确认无存活进程后经限定新一次只读 fmt 采完整成功收据，没有盲重试安装。
- 工程 sizes 真红发现 signatures.rs 1068超过原1036；不抬额度，只将本次 recovery 身份核验与 parser 缺名证明移入私有职责模块，strict public wrapper/函数body/准入顺序保持。`0337f53e` 后实际 fmt 仅函数signature展开及可选末参尾逗号，独审语义等价并明确非纯空白，`d1b6b8bb` 保原红。parent1018、child66，旧欠账仍按固定基线报告，不机械搬无关算法。
- 私有职责拆分后的 `d1b6b8bb` / tree `47a4c485` 重新完成 fmt、7FE target no-run（含 frontend library 共8目标）及单 LSP bin no-run；所有指定目标均实际 fresh:false/真实 rustc，旧 CLI `c565a932` 此时只保身份，不作为新行为证据。同52 ordinary 再逐项 actual1/ignored0/exit0，summary `93c8411c` / 399件 SUMS `31f5b112`；这次全部 harness 来自本头实际重编，不冒称前一历史52已在本头执行。
- 同一 `d1b6b8bb` 共14项有限工程全部实际 exit0：复用本头 fmt，顺序执行 LSP build、frontend/codegen/CLI/LSP 各 all-targets check/clippy、显式 CLI build、docs及固定共同基线 `f2d8bf7a` 的 sizes/diff。Cargo build/check/clippy 保 offline/locked、Clippy -D warnings；731文档通过，1869手写Rust中的38项旧超限照报，无新豁免。结果 `5b28be09` / 105件 SUMS `ffc646a1`。新 CLI SHA `90e31e9d` 有本头真实 CG/kovenc fresh:false、实际 rustc、kovenc test:false；frontend fresh:true来自本头先行真实编译，不伪称重编。旧 `c565a932` 和旧工程失败均保原归因。
- 仍在同一 `d1b6b8bb` / 新 CLI `90e31e9d` 上完整重跑固定35核心＋4额外enum场景：78臂、156次双renderer build，实际退出110次0、24次2、22次1；55个必需程序全部成功，21臂禁运行与两个 x02 拒绝后不运行均保留。7组完整前端诊断、23核心与4额外程序可比对没有差异；success loss、意外接受、未解释模式差、InvalidSsa及InvalidModel均为0。核心12个raw差及1个acceptance差仍是明确驱动规则差，不算零原始差或把两边拒绝冒称成功；x02未支持边界不抹平。summary `5a6bc94b` / manifest `d83f08a4`，旧矩阵只作对照而不继承执行。独立实际终审 `661ce4ae` 已核6779件封条、424次身份与1907个原始进程记录，全部156 build和55程序输出亦与前轮已验收矩阵逐字相同。
- CI 静查发现 core frontend 仅 `--lib`，stage 显式清单漏了七个新 FE target 的30项 ordinary，不能把 package 存在当成选择。独审后的四文件增量已独立提交 `c6f7efc0`：七 target 加入既有首个 frontend `--locked --no-fail-fast` batch，严格 policy 与 CI-only composition 合同同步，stage/core/composition 的16/5/2共23次 Cargo调用不增，错误传播不弱化；本地不运行该混合脚本或 stub。三条 LSP 改名显式映射后，原108与本片52重叠15、新增37，最终应由双 OS 实际核145个主身份＋3个 CI-only 身份。选择已安装不等于新头CI已运行；最终单纯文本 policy 与 docs 门另采真实收据。
- 本片删除的是 LSP 旧 Analysis/DefinitionIndex/诊断转换与 Legacy 实现分支，保留实际单文档功能，不保旧内部API或错误回退；其它真实消费者尚在使用的旧 frontend checker/lowering 后续继续迁移并删除，D10 尚未完成。非法 classifier-body val/var、nested classifier/companion 与其它尚未接通结构仍 fail-closed，不声称全AST恢复或全部 native 能力。所有原始日志、错误记录、绑定与审核留仓外 `e2-lsp-unit-single-document-validation-r1/`；本地不运行 whole/mixed、故障/校准/Abort、私有facts/foreign IDs或合成Rust。最终 docs 门、新头双OS完整CI仍须实际闭合。

## 48. 独立 SingleFile 门面退役与 generated Unit 全链消费者（2026-10-20）

- §47 发布头 `f6b2c895` / tree `e8df77d2`（本地 `3cf2688a`）的 PR CI `39420573468` 已实际 completed/success，16项 job 全部成功。Linux/macOS 各145个主身份和3个 CI-only 身份各通过一次，新 FE30、三处改名、LSP47、原两个历史 whole 回归与 tutorial32 executed/1 planned 均实际到达；五资源和四 preview evidence 内容、双 independent consumer 各12命令及 aggregate 均核实。macOS counter-only/LLDB ignored、Linux无UBSan等原边界保留，不把该头全绿继承给本片。
- 本片原21路径核心提交 `1de88825` 删除202行独立 `analysis/single_file.rs` 及公共导出，同时迁移其真实 FE 测试与 CG 消费者；共享 `resolve_names`/`NameResolution`、真实类型/ownership算法不机械删除。按用户未发布产品不留兼容层的方向，旧 SingleFile observer/callback/拆分接口随门面退休，没有新增总模式、旧引擎回退或新公共 API。仍被真实消费者使用的 TypedFile/OwnershipCheckedFile、旧 native 入口和142个旧 lowering 文件尚未退休，D10仍开放。
- FE private cfg(test) module 保原五 recovery 源和五 gate 源字节，独立 manual 串真实 lex/parse/index/names/recovering types/ownership producers，另与 production Unit 门面作完整 AST/names/types/owned和SourceId/Span/MemoryDocument身份比较，不由同一门面自证。三个 integration 普通正控分别保空源/Unicode CRLF、真实 const 独立能力和显式空环境完整 L0080；原 raw frontier与Unit最终拒绝、受信receiver来源继续硬核。final-admission消费者先 complete 后进入真实 basic/const ownership，不按源码字符串猜能力。
- 原两个 integration targets分别迁为 `unit_single_source_analysis` 与 `single_file_fact_compile_contracts`。已退役门面的 HRTB/callback接口合同不作为兼容层保留；仍活跃 raw facts 的三项 E0308能力错配、iteration正控/E0599/E0616完整保留，并由既有真实Unit targets继续覆盖能力封闭/借用期。含原 `error("stop")` 的 complete whole、foreign身份及合成Rust compile合同均保持CI-only，未拆其中分支在云端执行。
- generated-owner exporter与boxed-enum LLVM helper现在消费同次 MemoryDocument UnitSource：原SourceMap/ParsedFile/names/typeenvironment和真实basic或const typed/owned按值持有，strict final准入先于ownership。每次临时重建合法view，实际原顶层entry/声明/deinit descriptor和SourceOrigin绑定唯一SSA函数，再核唯一有body LLVM定义。原 six-file/24 direct-call闭包对应的source、allocation/地址/释放顺序及已有stdout断言保留；部分mixed exact的旧native前半仍旧入口，不能声称所有resource/native代码都已Unit。
- 六个外部script消费者一同迁移，不留下后台执行旧checker的G4。I1的完整有序labels为move `[330,336)` 和真实inspect参数 `[114,124)`；I2保primary `[271,275)`，binding由旧name扩为完整 `[243,253)`并加入consume参数 `[186,200)`。原生成source字节、完整Eq、replay/shrink及手写golden均保留，不过滤label。sanitizer仅在固定唯一默认包非泛型inspect域内，完整symbol token严格接受 `koven.inspect.dN` / `fN.koven.inspect.dN` 两种真实形式；其它symbol、caller/freed-by、截断后缀不获stable。G4只定位真正Unit `ensure_available` 的唯一L0131报告点；red/restore原bytes/hash/diff/green及失败清理合同不弱化。所有generator/mutation/calibration执行仍仅CI，静审不是动态验收。
- 首门fmt在 `1de88825` 真exit1，完整原stdout/receipt保留。精确独审后的 `9651e88c` / tree `638664c8` 只改7原候选Rust文件27个日志hunk，5处为可选尾逗号增减，其余排版；没有source字面、宏参数、属性或断言语义改动，不谎称token全同。仓外r1格式提取因formatter显示的虚拟EOF空行未匹配真实单换行而停止，未安装或运行；r2明确保真实EOF后才安装。新fmt实际exit0。
- 同一 `9651e88c` 上三段offline/locked no-run实际通过：指定五FE integration连同normal library共6目标、FE lib-test1、CG lib-test1全部fresh:false，均有实际rustc和准确源路径；不是仅凭Cargo JSON的候选src_path接受旧产物。实际耗时47.96、56.62、129.86秒，新FE/CG harness分别 `88d0c86b` / `d90217cf`；编译含CI-only test body不代表执行。3324源、所列工具/配置和继承的19个LSP依赖前后绑定；后续独审核出CG18包中14包未直接pin完整源码，虽旧A2已保其中5个build.rs/archive，不能把Fresh缓存和实跑绿追认为全依赖来源闭包。该缺口与其后续补证分别记录，旧CLI `90e31e9d` 此时仅身份保全，不作为新生产行为证据。
- 本头8FE＋3CG共11ordinary exact依序各actual1/ignored0/exit0，summary `eb735922` / 124件SUMS `30f721c1`。两条独立普通MemoryDocument basic/const正控实际经同次facts→verified SSA/LLVM→publicUnitobject emission→Clang链接→新程序，内部硬比exit0、完整 `basic-memory\n` / `constant-memory\n` 和空stderr；这是helper内部完整断言，不是单独child原始进程遥测，更不是资源/释放计数证明。第三CG exact使用原r01源保完整L0080，在ownership/LLVM前拒绝；没有动态宿主Rust/C、fault guard或旧emit_link_and_run替代。
- 十四工程门在同一 `9651e88c` 实际全部通过：复用本头fmt后，LSP build、FE/CG/CLI/LSP check与clippy、真实CLI build、docs、sizes及固定共同 `f2d8bf7a` 的diff均按有限命令执行。summary `a7187453` / 107件SUMS `71767867`；docs731份Markdown、1870份手写Rust、38项历史超限原样报告，不抬额度。前十项保原较窄LSP依赖绑定，不假装在增强后重跑。
- 依赖继承缺口核清后，正式静审与当前只读比对补齐42官方archive=lock/index、1947源码文件、12个build.rs、41个当前CG依赖产物及7组33输出、18个选定工具与13个直接header。仅用于当前身份，不补造原A0缺失的依赖产物历史hash、不宣称完整OS/sysroot可复现；原11绿不重标、无清cache/下载/强制依赖重建。官方build/proc-macro闭包包括metadata编译、llvm-config查询、C预处理/`-c`编译与归档，不运行新生成探针，不混成Koven故障或校准。
- 增强绑定下CLI实际47.30秒exit0：normal codegen与kovenc均fresh:false，有真实rustc、kovenc test:false与新产物 `45bc496c`；frontend和registry依赖按实际Fresh复用，不伪称全部重编。其后docs/sizes/diff均以新CLI身份绑定真实通过。旧CLI `90e31e9d` 的身份保全至明确build阶段才替换，固定39矩阵使用本轮新CLI而非旧产物。
- 矩阵准备r1因既有controller所需artifact receipt schema不匹配在静审停止，未执行；r2修正只派生既有实际build结果后才启动。r2外层exec transport断连，后续落盘曾继续；局部PID namespace看不到旧会话，不能据此宣称进程已停。其终态保持未知，完整部分证据不改、不计矩阵通过；不把transport错误归为语言语义失败。经显式写路径和39原程序副作用独审，r3只换独立output根并将TMPDIR指向该根，隔离CLI sibling object/project executable及GCC临时文件，Git身份命令保no-optional-locks，固定源码/CLI/registry只读且不运行Cargo。原矩阵算法、156 build及55+2/21运行门不变；r2/r3从不拼接，TMPDIR也不冒称原CLI构建环境。启动时1.45GB磁盘和约8.94GB可见可用内存只是实际资源快照，不是硬限额或全OS无写认证。
- r3独立完整重采实际outer exit0、stderr为空，summary `9d5ff96a` / 6779项manifest `40e33ee7`，完整424身份快照和1907进程记录。固定39场景78臂156 build全部到达，110项exit0、24项exit2、22项exit1；55项必须成功的原程序全部exit0，21禁run臂与2个x02拒绝臂均未执行产物。核心raw12/acceptance1单文件/project入口差异仍明列；7对完整FE诊断、23对核心及4对增量程序无未解释差异，原成功损失、意外接受、InvalidSsa、InvalidModel均0。x02仍是能力缺口而非接受成功，两renderer均核，不以both-reject凑一致。独立实际终审 `89c9ce8e` 已逐项核全部文件、原始进程、冻结oracle与当前身份后PASS，不仅依赖controller summary。
- 矩阵实证及独审闭合后，为不改动未知r2仍可能读取的原 `9651e88c`，另建同头隔离后继树，仅安装已审CI4为 `927fd80c` / tree `2a1770f8`。两个target一对一替换且各无filter到达；stage仍16次Cargo/136个test参数/123个非特殊target，composition仍23次Cargo/125个非特殊target。实际安装独立选择审 `a7fc02ed` 核四payload精确匹配，余3320文件含全部Rust/Koven/golden和resource消费者字节不变。主身份156、原CI-only兄弟3、新门面承接CI-only5、既有Unit compile15及resource composition14共193个唯一身份和17条退休承接映射完整；这些只是静态选择，不冒称新CI实际覆盖。后继两文档不改已测生产；安装后的docs/common-f2d diff/sizes和两项已审纯文本policy单方法另留真实最终收据，新发布头双宿主完整CI仍待执行。
- 全部原始日志、身份、静审、失败与绿色收据留仓外 `e2-single-file-facade-retirement-validation-r1/`。不以11普通exact替代原全部generated/resource/fault/calibration或新头双宿主CI；任何原成功源后继拒绝必须保源与原oracle诊断，不能both-reject凑一致或宽成任意Err。共享checker/lowering余下真实消费者的迁移与删除仍是后续D10工作，不为旧内部API新增长期适配层。

## 49. 旧 native 门面退役与真实 Unit 能力补齐（2026-10-20）

- 前片本地发布基线 `9bbf6971` / tree `47022ab2` 对应实际远端头 `45174c1a` 的 PR CI `39445498004` 已全16项 job成功；Linux/macOS各193个唯一身份、tutorial32 executed/1 planned、五资源、四preview及消费者和aggregate均已核实。macOS partial counter/LLDB ignored、Linux无UBSan等边界不变；这些证据只归前片，不能继承成本片新CI。
- 本片删除公共 `NativeEntry`、`emit_native_object` 和其专属symbol/entry校验/plan桥接，移除 `ssa::validate_frontend_inputs` 转导出。`NativeUnitEntry` 及普通 `emit_native_unit_object` / `emit_native_owned_unit_object`、常量 `emit_native_constant_unit_object` / `emit_native_const_owned_unit_object` 四个出口保留。真实native消费者及共享 `emit_link_and_run` 改用同轮MemoryDocument UnitSource，经strict completion、真实Basic/Constant资格、ownership validation和封闭view进入Unit driver；不恢复旧门面、按源码猜能力或复制checker。原source、成功exit、已有完整stdout/stderr、drop顺序和object确定性oracle均保留；未原有的counter/stderr断言不冒称新增实证。私有 `lower_frontend` / `lower_scalar_file_with_entry`、旧checker及facts仍有真实测试消费者，不能机械删除；本片不宣称D10完成。
- 实际迁移先暴露runtime global被静默忽略：原首whole exact在声明正控之后收到 `Ok(())` 而真红，原34项只执行1项。后端现在于同链身份、既有map/borrow validator之后、planner及object reserve之前，按canonical source/root顺序拒绝真实顶层runtime Variable，包含modifier的完整root span经既有映射报UnsupportedSource。只检查真实root，不拒绝const/object/类型声明或函数局部，不实施global initializer；这是尚未实现的backend capability gate，不是前端语言禁令。三条新object-only正反控及原whole后来均实际通过。
- pending构造修复由原构造早退源真实MissingFact及三条tests-only红建立：FE在每个正常完成的MoveOnly operand后发布真实pending owner，正常commit仅结束当前构造；CG在nominal和list-form构造交付后登记owner，跨后续CFG重绑，commit移除当前owner别名，return/break/continue沿真实drop清理。具体Copyable不新增owner，完整Nothing operand、RuntimeLength和原能力边界不扩大。首生产后仍实际1绿/1红/38未跑，真实grouped const容器origin不等于外层argument；随后call/container共享原严格materialization-origin校验，仅实际constant Group归一到已发布origin，普通Group保外层身份。append/insert只共享身份校验，不登记构造pending alias；原源和oracle未改。
- nullable Rc原whole在object emission报InvalidModel/InvalidSsa且未link/run，原日志未给verifier内部首错，不能把静态推因写成实测细节。tests-only两目标真红、非nullable邻控真绿后，Unit仅为真实own `Rc?` 根的有界直接顶层无else `!= null` If接入非空proof；`SharedRetain`/payload只读使用同根当前Shared Loan，显式share产生独立owner，join/drop先结束view，原owner仍可返回复用。SSA与present/null正常native及原nullable whole实际绿；不靠NullableTake、伪造Value、放宽verifier或扩展While/When、Borrow/field/element、nested-proof等边界。
- 逗号when原whole在首true变体object emission报MissingFact，false变体与link/run未到达。真实Unit producer新增source-qualified `WhenAlternativeMatch(control, entry, alternative)`，逐条件保remaining/matched路径，只按原ASAP规则清理匹配边独有的纯内存owner；resource仍走原词法生命周期及ResourceLifetime deferred。CG在matched block重绑后消费真实事实，并独立保全false后继的owner、loan、pending、temporary和result-source loan状态，严格merge不变；新facts/SSA与普通native两控、原true/false whole后续实际绿。不删除binding凑合流，不扩大nullable/tagged subject或Map能力；r2漏implicit tuple字段是静审阻断、未安装执行，不冒作行为红。
- 关联常量原whole随后在strict final admission缺少 `Labels` qualifier的expression fact而真红，尚未ownership/native。修复只在同源两段路径、真实ObjectValue根且唯一Constant目标时补真实Type/Declaration role，保原Value及terminal选择；不发布runtime qualifier expression type、不实例化object，不改final-admission或private可见性门。新增Const事实正控与runtime参数shadow的Basic邻控、原中文stdout、全namespace和repeatability/argv whole后续完整通过，原成功源没有被改为拒绝。
- 条件resource负控明确迁移错误合同：原LEAF及conditional源码不变，旧native UnsupportedSource/resource-token包装随门面退休；现在真实Basic ownership validate在executable UnitSource/view/emit之前拒绝。硬核四阶段零diagnostics、同轮source/names/environment/typed/owned身份、恰一ResourceLifetime、真实If的完整 `if (flag) { consume(resource) }` span、再次validate仍拒绝，以及既存object与neighbor的整目录文件名/bytes完全保留。此exact不调用emit/LLVM/link/run，不宣称旧新错误包装相同或native emission原子性已在此测试执行。helper只把真实非空deferred的原recovery Err独立标记，未知validation错误仍失败；Const路径和成功资格不放宽，L0080邻控保完整诊断并另核Diagnostics tag。
- 固定47 ordinary whole exact在 `62781101` / tree `531414cc`、3330源及同一新CG harness `14b8ef8a` 下完整从头实际全绿：47项各actual1/ignored0/exit0、无timeout/errors、before/after绑定一致；fmt真实通过，CG fresh:false及实际rustc已核，不宣称FE/CLI/官方依赖在此轮重编。summary `721c7768`、259件封条 `98555037` 与独立全47实审 `b78a3f2a` 对应完整原日志。前序有限队列的15/1/21、1/1/38、21/1/18、25/1/17、29/1/15、38/1/8分别是当轮绿/红/未执行，不拼接成全绿；全部真实红、fmt红、E0277编译失败和未到达断言原样保留，编译失败不计行为红。
- 工程首6项在 `62781101` 实际绿：LSP build、FE/CG/CLI/LSP check、FE clippy。原第7项CG clippy真红后，`f343a47d` / tree `8b893ebd`仅两文件两hunk：旧lowering转导出限定cfg(test)，resource负控改用同Err值且原消息不变的expect_err。新fmt、CG fresh:false实际rustc与新harness `ca508b65`、resource admission及production dependency guard两whole exact各actual1/ignored0/exit0、CG/CLI/LSP clippy第07–09均实际通过，独立实审 `5ac98582`；原Clippy红保留。前6工程和47行为证据仍只归6278，不能重标为f343重跑或声称九工程门同一头执行。
- f343的隔离offline/locked CLI build已实际55.97秒exit0，receipt `f0205e5f`；FE/CG/CLI三个workspace target均fresh:false并有真实rustc，新CLI `134a089b`位于独立target。仅这三个package的dev.debug设0；opt-level0、assertions、overflow checks、panic-unwind、registry profile及llvm-sys DEBUG相关C行为、Koven输出优化/source-debug不变，不能据此声称与旧宿主debug2 profile等价。42官方依赖是当前archive/source身份集合，实际selected依赖产物按原始构建记录核，不声称42包全部编译；也不追认历史cached artifact完整来源认证或完整OS/sysroot可复现。旧9651的3324源和旧CLI `45bc496c`继续保全，旧矩阵r2终态仍未知，不拼接证据或推断已停。
- 原始日志、失败、每轮source/harness/工具/依赖身份及静审收据保留于仓外 `e2-native-facade-retirement-validation-r1/`；精确生产审查与边界库存位于 `d10-readonly/`，其中retirement-boundary-r1绑定6278，只是该头历史库存。f343重新静审的installed-selection ledger `7ac93f24` 保留原71个helper相关物理文件及398个显式test身份；当前为74文件/406身份，新增pending/nullable Rc/comma when三文件的8名均已在47内，不重复累加。按Cargo executable target与完整test name去重，原193、47（重合5）与15个whole CI-only合成250个focused身份，再加helper独有352个，共602；旧592无丢失、新10无重名。此为helper相关文件的保守显式测试库存和原CI无filter选择链静审，不是完整macro expansion、精确传递调用图或新头实际CI覆盖；双宿主须逐身份实核。47普通exact不替代原CI-only、active Abort、generated C、计数/故障/事实变异/foreign ID/calibration/sanitizer whole；这些未在本地执行，也不拆分或弱化oracle。
- f343与隔离新CLI `134a089b` 的固定39场景/78臂/156 build矩阵已独立完整采集，outer exit0且stderr为空，summary `f6643a85`、6779项manifest `49720bd7`。156 build分别110项exit0、24项exit2、22项exit1；55项必须成功的原程序全部exit0，21个禁run臂和2个x02拒绝臂均未执行产物。核心raw12/acceptance1入口差异仍明列；7对完整FE诊断、23对核心及4对增量程序均无未解释差异，原成功损失、意外接受、InvalidSsa、InvalidModel、未分类模式差异均0。x02仍是拒绝能力缺口，不以both-reject证明支持。新output/source/CLI/profile证据独立保留于仓外 `e2-native-facade-retirement-cli-matrix-r1/`，不借用§48旧矩阵或混入终态未知的旧r2；独立实际终审 `d95dce9c` 已核完整文件、身份、原始进程和冻结oracle后PASS；generic_nominal原无stdout/stderr golden，仅保留实流与两入口直接对比，不补造历史空输出oracle。
- 隔离CLI之后，f343的安装前docs、sizes和固定共同 `f2d8bf7a` 的diff三门均真实exit0，before/after绑定一致、无timeout/errors；docs核731份Markdown，sizes报告1876份手写Rust及38项历史超限，不抬额度、不静默清债。十四项工程命令由summary `532df19a` 与112件封条 `7dea1b4b`汇总，按上述不同源码头和宿主profile分别归属，不把6278前6项重标为f343执行；结构门禁也不证明语义等价。这三门检查的是尚未安装本候选的f343，不能代替文档安装后最终检查。
- 当前两文档仍为仓外候选，Spec §1–48完整字节前缀不改，Architecture仅Native object段最小事实替换且维持200行。本轮两文档候选独审、安装后docs/sizes/common-base diff与本片新头CI仍待，不宣称已执行或以旧门禁替代；安装后最终三门及源包身份另留实证收据，不循环改写已冻结文档。发布仍须新头双宿主全部602身份及原完整CI、tutorial、资源/preview、consumer/aggregate核验；旧checker/lowering剩余真实消费者迁移继续作为后续D10工作。

## 50. post054 修复与有限收尾检查点

截至2026-10-22，本节记录源 `5f8992d3` 的本地有限验收；文档安装后门禁与原PR新头双宿主CI另凭实际收据，不提前记通过。

- 旧实际远端 `054a74d064d2364b76642aa2d9112307f558ffbf` 与本地 `4335fe8b` 同 tree `9303a276`；[原PR CI](https://github.com/Halckon/Koven/actions/runs/39489968161)双宿主同25项失败，602账各427一次通过/24失败/151未到，另1阶段whole在旧602之外。该失败不被后继局部绿覆盖。当前受验源 `5f8992d3c5f1b225a8edb37944b3e325b8ffc209` / tree `366f14ce20733cd1be50ac9cbe356c5ac1312128`，3360 tracked；从4335为56个保序小提交，累计76路径+14484/-200。原52条后为测试EOF、unused red_span、两处Option早退等价改为`?`及BorrowBegin测试helper等价filter；不据静态等价直接继承运行结果。
- post054修复真实整数statement/Map写角色与primitive前缀、dead Map jump continuation及Resource Map Borrow有界循环；补普通direct-call前缀、Unit nullableWhen typed/owned产品、Group(null)上下文与fresh owned If proof。保持真实source/owner/loan/generation与完整whole oracle，不采用任意Err或both-reject。原25的历史分解仍为21个原能力whole在各受验头绿、1个0332精确阶段表绿但能力债OPEN、3个原CI-only whole待新头完整CI，不重写旧头归属。
- 当前同头light-r5八门、workspace check-r4与严格clippy-r3均已有实际成功和独审：summary分别 `25b43a96`、`de1085f0`、`23854344`。light含fmt/docs/直接依赖、PR base `c0c5156a` 与共同基线 `f2d8bf7a` 两尺寸、working/4335/PR-base三diff；731 Markdown、1903手写Rust/38历史超限不消债。check/clippy均保offline/locked/workspace/all-targets及实际输出边界，clippy保`-D warnings`；工程门禁零测试credit，旧check边界失败和clippy-r1/r2命令101不追认。
- 当前final_type_admission与CG测试harness已分别真实新编成功并独审，proof `5198b1bd` / `1e1ea39e`，实际exe `65850326` / `36c304d4`；真实rustc/depinfo与FE rlib `75e055e4` extern已绑定。该两次no-run本身不计测试执行，不能用metadata文件或旧harness替代当前运行产物。
- `c996359a` / tree `8d848c7f` 的ABC strict20保留原单批各actual1/exit0/ignored0历史，终审SHA `b4e9e8316249d19487ab549dc7b844a4d083b17333b441e876749504dcc8b4fd`。后继red_span改变8个CG whole的输入闭包，不能把旧20整体重标当前复用。当前普通集合仍139，分为127按三份验收覆盖的真实新运行身份与独立12 FE同输入保留，不称139同批或139新跑。
- 原127批真实停于112：001–111各whole exit0/actual1/passed1/ignored0且已独审接受，summary `bc9c59ba`、终审 `b716bd6e`；112真实harness exit0、1 passed，但`--nocapture`输出插在exact前缀与独立ok行之间，旧controller未识别exact结果，原receipt保持success:false。113–127原15项未启，原批batch_acceptance:false，不把解析失败追认成成功批，也不称Rust测试失败。
- 最小controller parser修复仅适配上述严格输出协议并保唯一身份/summary/actual1/ignored0/exit合同，不改源码、oracle或harness；单112实际 `0e30d1c2` / 独审 `e96802e3` 与原序113–127实际 `4564a08f` / 独审 `e960a243` 均已严格绿。metadata-only累计摘要 `d02db46a` / 独审 `261c01b8` 核定111＋1＋15三份验收覆盖127唯一身份，逐条绑定原exact/command/binary/receipt/raw；单112的新stdout与旧观察相同，但只计新执行的成功收据。原112协议失败及原批15未启历史仍保留，single_batch_success:false，不称“同批127全绿”；该累计本身零新运行、也不含12 FE复用。
- 独立12 FE已取得同输入保留正式据，终审SHA `75d74d0a12db47ed464413c6a1cdf58264748756ce7be72d0fe3c7a3a462a773`；summary `d9fa6daa`、复用接入summary `e03e2a7d`。418输入闭包及原12真实whole不变，当前实际head/tree、源码/工具/环境与新harness引用已核，new_runtime_credit=0；11份proof引用不是11个测试，不把12计为新跑。
- 第三隔离CLI已真实构建并独审：receipt `32ce9d26`、独审 `86df0262`、actual binary `139455d2`，源仍5f8992/366f。41个真实非Fresh artifacts含三workspace targets与38外部依赖，10个build-script事件有实际记录；仅FE/CG/CLI host dev debug0，LLVM-sys等外部依赖仍debug2，真实extern同次产物。两受保护旧CLI `45bc496c` / `134a089b` 未覆盖；该构建本身零矩阵credit，私有host profile不等价完整CI。
- 原39场景/78臂/156个renderer builds全部完成合同验收，actual `0c49aeff`、独审 `0d7076c7`；55个必需成功程序实际exit0、21个禁止run臂未跑。x02两条件槽仍同UnsupportedSource→UnsupportedNode，Single各renderer exit2、Project各exit1，均未跑且不计支持；显式参数入口原允许的Single InvalidEntry与Project Borrow argv差异仍保留。无InvalidSsa/InvalidModel、必成功丢失、意外接受或未分类模式不符。合同通过不等于全部源获支持，Single/Project双模式也不是双宿主CI。
- 当前673身份账按已审最小附录 `510edaac` 修订为139 ordinary＋153 legacy未授权＋57 explicit CI-only＋324 helper未授权；唯一修正为带allocator计数的field-resource whole从helper325转入CI-only57，总673/139、原25及identity顺序不变。原resource-field完整计数、nullableWhen三源8+8/Rc1+9与Abort、Atomic原2源×absent/present四格fault whole均仍只留CI，不能拿普通局部成功替代。静态673不是已执行覆盖。
- SPEC-0332原bounded done/archive与冻结历史不变；§5四Unit normal/break/continue/return payload-reader及其中体内原owner返回包装债仍OPEN，presence或expected reject不消账。SPEC-0341/D10继续active/in-progress，旧checker及私有lower_frontend真实消费者的迁移/删除仍未结清；原x02边界、历史失败与历史未知终态矩阵、源与仓外raw/绑定均保留，不自动清理或外推依赖/sysroot重建。
- 56条GitData前缀导出已获静态独审 `f9ba3935`：165 blob、每条完整message/父映射及全部中间tree与最终tree一致；这不是远端发布结果，也不含后续实际docs提交。五docs仅作事实同步，原Rust/依赖/feature/工具及受验产物闭包不因此改变；六个新docs-only结构/两尺寸/三diff门按独立384MiB/4GiB合同在安装后、提交前另取实际收据，旧light8/build的1GiB门不变，无Cargo/test重复或自动清理。随后仅追加真实docs source/tree，发布前fresh核原PR head/base及actual remote commit/tree/message/有序parent；054/main c0c只是旧预期，不预造SHA。新头双宿主CI的原25/673身份、tutorial/resource/preview/consumer/aggregate仍待实证，本地绿、静态导出或aggregate单项不能替代。


## 51. 迭代 final admission 两处阻塞修复候选（2026-10-29）

- 基线 main `981760459c33bf9b10416cadbc7c168d97b7525c`；独立本地分支
  `fix/spec-0341-iteration-final-admission`。不修改 PR #220 的 Unit counterpart、runner 或策略测试；
  原 `type_iteration.rs` Single raw witness 保留。旧入口净退休 0，D10/E2 继续开放。
- 原失败证据为 [PR #220 run 40825988901](https://github.com/Halckon/Koven/actions/runs/40825988901)，
  head `34b55e58d6279ed6fd043f467add9302b87ea440`，Ubuntu targeted job `123290381660`：
  deferred fixture 在 Single 多出静态 `listOf` 名称 80..86 的 L0165；ambiguous fixture 两入口
  在回滚 lambda 的 243..249 上分别缺 Error 原诊断/表达式类型。历史失败不追认通过。
- 根因与修复：共享 walker 的 contextual recovery 仅识别 L0123，现同样识别真实源函数 overload
  callee 上的原 typed L0124，仍要求 Error、无 selection 及未提交 lambda；不为任意祖先错误授权。
  Single 容器恢复给静态 callee 写 Deferred(Call)，Unit 可不发布该值；参数已失败时，两者按真实
  静态候选身份恢复 selector，不再把该非值壳作为独立 L0165 root。普通 runtime callee 仍遍历。
- 新增 `final_type_admission/iteration_recovery.rs` 九项 source-only 回归，使用相同 SourceMap/ParsedFile
  比较 Single、rooted Unit、MemoryDocument Unit 的完整诊断；断言 deferred 唯一根因的精确文案/
  Span/details、原 L0123/L0124 整值保留、无 Internal、失败计划与 nested call/实参类型及 lambda
  参数类型/mode 回滚；成功控制核唯一 source call、List provider、Borrow 交付、Int element/binding、
  lambda 参数与 nested argument 的 Int/Borrow 契约和原分析身份。另保独立声明及同一歧义调用普通
  实参里的 final-only L0080（AST 核实位于该普通实参内、回滚 lambda 外），分组 lambda 的
  L0123/L0124/成功三邻控，同名局部 callee 及两个独立 Deferred 实参各自根因、真实 FunctionValue
  callee 与普通实参错误。回归范围仅直接源函数 overload 与上述局部/函数值边界，不宣称扩展
  qualified/member/external overload 的 contextual recovery。已有 `final_type_admission` target 全量被
  bounded runner 选择，无需新增 target 或改接线。生产不改 raw checker、事实产物或 Guide。

| 验收项 | 目标/命令 | 本片结果 |
|---|---|---|
| 静态格式与空白 | `rustfmt --edition 2024 --check`（改动 Rust 入口）、`git diff --check` | 已通过 |
| 文档结构 | `python3 scripts/check_docs.py` | 已通过 |
| 直接行为及既有邻控 | 官方 CI 的 `final_type_admission` 与 `type_iteration` 完整 target | 待执行；本地禁止动态测试 |
| 编译与 lint、双平台完整门禁 | 官方 CI workspace check/clippy、Linux/macOS targeted | 待明确发布授权后由官方 CI 执行 |

上述仅为实现及静态核验候选，不宣称已通过编译或行为验收，不恢复 #220 已撤下的两个 Unit 对照。
没有本地 cargo/build/check/test/clippy/native 执行，没有推送、创建或合并 PR。


## 52. 迭代 Single 测试入口有界退休候选（2026-10-29）

基线 `8a0b336cf6d8f29b0a7c8bbf74a36520017679af`（#220/#223 均已合入）；独立本地分支
`fix/spec-0341-iteration-single-retirement`。本节记录静态候选，不是已执行/已合入的退休验收。
仅修改六个既有 Phase 2 测试/helper 文件；无 production、Guide、fixture、target/runner、ownership
或其他工作树变化，不推送、不创建 PR。Architecture 所述生产事实无变化；本批不改写其历史验收。

### 52.1 27 个现有对照逐项审计

A01–A03 位于 `integer_range_iteration.rs`，A04–A05 位于 `integer_range_iteration_ownership.rs`；
A06–A12 位于 `ownership_iteration/map_borrowed` 家族，A13–A15 位于 `type_iteration` 家族。
B01–B12 对应 `type_iteration.rs` 原测试和 `type_iteration_unit.rs` 的 12 个 counterpart；
后者仍真实运行旧 Single producer 并比较完整 raw/final diagnostics，不能算已退休。
P2 行 cleanup/独立 loan-end 为不适用，不由本批新增能力；所有 P3 行及原独立 ledger 查询保留。
“保留”不等于已证明不等价：本批优先把完整 producer caller 文件归零，不拆成隐藏旧调用的新 helper。

| ID | 原测试（名字保持不变） | 候选处置 | 诊断、身份、cleanup/loan-end 审计 |
|---|---|---|---|
| A01 | `range_sources_publish_owned_endpoint_delivery` | 保留 | P2；空 names/raw + complete 成功、Int/Long provider/value/source 身份相当；与 A03 共用 producer，本批不拆 helper。 |
| A02 | `range_iteration_accepts_existing_control_exit_syntax` | 保留 | P2；N/C/B/R、discard 结构与 A01 同 oracle；不以语法接受证明 cleanup，仍与 A03 共用 producer。 |
| A03 | `invalid_numeric_sources_preserve_borrow_provider_controls` | 保留 | P2；拒绝仅核 L0159/code/span，未固定完整 message/details；Borrow provider 正控保留，不把 code-only 当全诊断等价。 |
| A04 | `numeric_snapshots_allow_owned_items_source_reassignment_and_local_cleanup` | 保留 | P2/P3；两侧核 OwnedSnapshot/Owned binding/无 EndSource；Single 独立 validate_iteration_facts 保留，producer 与 A05 共用。 |
| A05 | `numeric_inner_loop_preserves_outer_container_borrow` | 保留：不等价 | 完整 L0135 文案和有序 labels 已知不同：Single active iteration source loan；Unit move/active loan 并含 parameter label。两套整值 oracle 均保留。 |
| A06 | `owned_borrowed_and_temporary_sources_preserve_bindings_and_cleanup` | 保留 | P1–P4；实际 root/temporary、K/V Shared 与 owner/cleanup/phi；保留 Single flat cleanup 观察及独立 A09，不以 Unit plan 投影替代全局 ledger。 |
| A07 | `borrowed_move_only_key_and_value_reject_owned_delivery` | 保留 | N1/N2；当前 rejection predicate 只核唯一 L0133/code，不证明完整诊断等价；真实 binding/call 身份及错误原子性保留。 |
| A08 | `active_sources_reject_consume_and_owned_return` | 保留 | N3/N4；当前 predicate 只核唯一 L0135/code；不外推与 Unit 的消息/labels 等价，来源 active-loan 保护仍保留。 |
| A09 | `public_map_facts_preserve_exact_identity_and_cleanup_in_both_entries` | 保留：独立证据 | Single 对 flat cleanup_steps 做全表 orphan/点位核验；Unit 按出口点查 iteration_cleanup_at。保留全局查询、Drop owner/order 与 EndBinding→FinishProvider→EndSource。 |
| A10 | `direct_borrow_return_source_keeps_unproven_continuation_boundary` | 保留 | L0164 只核 code、call primary 与存在 borrow label，未固定完整有序 payload；borrow_results bindings/ends/call_uses/forwarded_source_loans 的错误原子性均保留。 |
| A11 | `source_return_and_dead_tail_do_not_acquire_map_providers` | 保留 | D1/D2；source-return/dead-tail 的 raw typed 描述符与无 Acquire ownership/cleanup 边界；独立 validation 及 A12 query 不改。 |
| A12 | `public_unacquired_map_facts_preserve_typed_identity_without_owned_plans` | 保留：独立证据 | Single 查询完整 flat cleanup_steps；Unit 核实际 For/return/body 的有限点无 cleanup。不能把有限点缺席代替全表无 orphan/未 Acquire。 |
| A13 | `map_and_mutable_map_named_borrowed_iteration_complete_in_both_entries` | 转换为 Unit-only | P2；原 source 字节、K:Int/V:String、ForBinding/read 同一 symbol/type、println 已选 Unit 返回、空 raw/complete 原身份成功全保；真实 key/source/index 身份加强。 |
| A14 | `invalid_map_bindings_and_nominal_lookalike_preserve_diagnostics_in_both_entries` | 转换为 Unit-only | P2；7 源的 intrinsic/nominal 身份、真实 markers、无 partial plan 保留；将原部分 raw predicate 加强为完整 L0160/L0159 文案/primary/有序 label 整值 oracle，再保 complete==raw。 |
| A15 | `for_pattern_recovery_preserves_independent_body_roots_in_both_entries` | 转换为 Unit-only | P2；3 源原完整有序 raw/complete Diagnostic constructor 不变；保 L0160+独立 fooBar L0080、L0118 label、Error binding/derived、原 Deferred member/无 call 与真实 source key。 |
| B01 | `non_provider_source_is_rejected_at_the_complete_source_span` | 保留 | 12 项 Unit counterpart 共用严格完整 Single raw/final 对照；本项保原 source-span/L0159。删除 comparator 前须冻结逐源完整 golden。 |
| B02 | `binding_type_is_available_before_the_body_is_checked` | 保留 | binding-before-body 单诊断与完整 comparator；不把“仍有一条错误”当作完整 payload 等价证明。 |
| B03 | `range_view_publishes_a_distinct_shared_provider_without_owned_container_identity` | 保留 | RangeView/Borrow 身份与完整 comparator；单删旧测试只属去重，不使共用 Single caller 文件退休。 |
| B04 | `three_providers_publish_name_discard_and_ordered_borrowed_components` | 保留 | Array/List/MutableList、name/discard/ordered K/V projection、repeatability 与完整 comparator；不转移/重建 Single 观察。 |
| B05 | `invalid_patterns_do_not_publish_partial_plans` | 保留 | 逐 pattern 的码/完整 span/原子无 plan；完整 raw/final equality 仍依赖共用 Single comparator。 |
| B06 | `move_only_element_is_typed_without_phase_three_rejections` | 保留 | 类型阶段成功不代表所有权成功；保 P2 comparator，未借它删除 P3 witness。 |
| B07 | `user_named_types_and_methods_do_not_acquire_provider_identity` | 保留 | 同名 nominal List/Iterable/Iterator 拒绝及无 provider plan；完整诊断仍由 comparator 锁住。 |
| B08 | `body_errors_and_poisoned_source_do_not_publish_plans` | 保留 | body/poisoned-source recovery 及错误原子性；不按“双方拒绝”简化精确诊断对照。 |
| B09 | `late_generic_bound_failure_invalidates_iteration_plans` | 保留 | 后置 bound failure 原子撤销计划与完整 comparator；不以有错误替代逐值相等。 |
| B10 | `declaration_and_fixture_order_preserve_projected_types_and_diagnostics` | 保留：ordering | declaration/fixture 反序、field/binding 类型、labels 顺序及 repeated source 顺序独立要求均保留。 |
| B11 | `nominal_rejections_label_the_source_or_element_declaration` | 保留：labels | nominal source/element 的完整原始 label、span/order 与 comparator 保留，未降为 label 存在或 code-only。 |
| B12 | `source_modes_and_nested_loops_preserve_unique_source_identities` | 保留：identity | own/Borrow/inout source、nested Statement/Expression 的 source-qualified 身份和有序唯一计划；完整 comparator 保留。 |

### 52.2 实际净减少与保留边界

- 本次清单的 27 个对照测试定义仍保留 27 个（B01–B12 指 Unit counterpart；其原 Single 定义另保持不变）。
  仅 A13–A15 从三入口转换成 rooted/MemoryDocument 两入口，
  **转换 3 个旧 Single 测试路径，删除 Rust 测试定义 0 个**。三组 fixture 原字节不变，
  原 2+7+3=12 个 Single 源码展开观察移除；Unit 观察 4+14+6=24 个保留（36→24 是静态展开，非实跑数）。
- frontend tests 中实际调用 `check_types` 或 `check_ownership` 的文件 **65→63，净少 2 个**：
  `type_iteration/map_borrowed.rs`、`type_iteration/map_binding_controls/analysis.rs`。
  两文件的 `resolve_names`、`check_types`、`complete_file_types` 调用各 2→0；旧 ownership 调用未改，
  没有把旧调用搬到新文件、通用 mode 或兼容 adapter。另两份只读 Single facts 分支删除，原 Unit query 保留。
- A13 成功 oracle 仍要求空 names/raw 和 complete 原分析身份成功；A14 由静态核对两 producer 的
  `check_map_iteration` / `emit_iteration_error` 固定原 L0160 文案、nominal L0159 文案及 `type declared here`
  label，加入整值 Diagnostic 后再退休 Single 观察。A15 原完整 expected constructor 不变。
- #223 修复的是有界 final admission，不把 Single raw Error 与 Unit 未发布事实强行等同。
  原 overload/deferred 两个 raw witness、上游 names L0080 后继续 raw-check 的独有 recovery 测试、
  `final_type_admission/iteration_recovery.rs` 9 项及 `type_iteration_unit.rs` 的完整 Single comparator 均不改。
  后者文件头保留 #220 原失败历史，不把旧注释当作 #223 尚未修复的现状证据。
- 独立 cleanup/loan-end 不由 drops/plan 或“双方拒绝”投影代替；Single flat ledger 的全表 orphan 检查、
  Unit 按点查询、普通 borrow-results ends 空表、source-return/dead-tail 无 Acquire 及 numeric outer-loan
  两套不同完整 L0135 oracle 均保持原文件字节。真实 multi-source 与其他 ordering witness 不变。

### 52.3 静态核验与未执行项

| 项目 | 候选结果 |
|---|---|
| 27 项名字/路径清单、旧调用文件/API site 与测试定义计数 | 静态核对；无新旧调用文件，净少 2 caller / 3 Single 测试路径 |
| 原 fixture 字节、两份 Unit facts query token、保留 witness blobs | 静态核对；Unit query 不删 oracle，A13 只加强真实 key/source/index 核验 |
| `rustfmt --edition 2024 --check`、`git diff --check`、`python3 scripts/check_docs.py` | 静态通过；不是 Rust 编译或行为证据 |
| 本地 cargo/build/check/test/clippy/native | 用户禁止，未运行 |
| 新候选官方 CI、独立复核、发布 | 均未执行；候选未提交/推送，既有 #220/#223 CI 不冒充本 patch 验收 |

D10/E2 仍为 in-progress；以上净减少仅为当前候选差量，尚未交付的削减不计作 main 已退休。

## 53. Single 测试消费者成批退休（2026-10-30，待 CI）

> 合入状态订正（2026-10-30）：[#237](https://github.com/Halckon/Koven/pull/237) 已合入。
> 本节标题、验收表及结语中的“待 CI／待执行／仅发布 Draft／不合并”保留为首次发布时快照，
> 不代表当前状态；当前精确输入、官方 CI 与合入证据见[四批已合入验收对账](#四批已合入验收对账2026-10-30)。

基于 GitHub `main` 的 `18c23efd9a184844f4e3538a7b8d2d8e98d3adbd`，将两个已静审候选
合入隔离分支 `fix/spec-0341-single-retirement-batch`；不改变生产实现、Guide、fixture、runner
或 API。§52 的历史结果不回写，本节仅更新 B03/B04/B06/B12 与三个 codegen 单 root 的处置。

- codegen：保留三个原测试名和完整原源码，将两项 nested interpolation 拒绝及一项 unreachable
  non-null assertion 从 `lower_scalar_file` 转为严格 MemoryDocument Unit 单 root。保留原
  UnsupportedNode 与 typed/owned facts oracle，增加精确 source/span、单 root、SSA/LLVM 验证及
  单一 Int 类型表断言；这是测试中的断言，不表示本地已经运行验证。历史 Stage A witness
  中的保留旧测试描述不代表这三项仍使用旧入口；原 Unit witness 不删除。
- frontend：B03/B04/B06/B12 的四个原 Single 定义移除，同名 Unit 测试保留原源码、provider、
  Borrow、binding/projection、repeatability、nested source 身份断言；Rooted/MemoryDocument
  各自使用固定空的完整 raw Diagnostic 序列及 final admission 原分析身份成功，不再运行 Single
  comparator。其余完整 raw/final comparator、ordering、recovery 及独立 cleanup/loan-end 不变。
- 净退休：3 个 codegen Single lowering 调用点；4 组 frontend Single producer 测试及其 4 组
  comparator 路径。frontend 共用 Single helper 仍有其他消费者，不能宣称 caller 文件归零。
  不把 Phase 2 成功外推为 Phase 3 或 native 等价，也不外推 multi-source 覆盖。

| 验收项 | 输入与证据 | 状态 |
|---|---|---|
| 候选原样组合与基线 | codegen `a3eac896`，frontend 两文件工作树差量；两者基底均为 `18c23efd` | 静态组合，无冲突；原候选工作树保留 |
| 原源码、完整 oracle、保留边界 | 四个 Rust 文件相对上述 main 的 diff；独立只读静审 | 未发现有界语义或 oracle blocker；非 formal approve |
| 格式、文档、diff、尺寸 | rustfmt 四文件（skip_children）、check_docs.py、git diff --check、check_rust_sizes.py --base 18c23efd | 静态通过；39 个旧超限文件不变，无新例外 |
| 编译、clippy、行为和双宿主门禁 | 官方 PR CI：既有 codegen 完整测试及 stage 中 type_iteration/type_iteration_unit 完整 targets | 待执行；不缩小既有 CI 选择 |
| 本地 cargo/build/check/test/clippy/native | 用户要求不运行 | 未运行 |

仅发布 Draft PR 交由官方 CI 验证，不 Ready、不 formal approve、不合并；D10/E2 保持 in-progress。

## 54. 拒绝诊断固定 oracle 与 Single 消费者成批退休（2026-10-30，待 CI）

> 合入状态订正（2026-10-30）：[#239](https://github.com/Halckon/Koven/pull/239) 已合入。
> 本节标题、验收表及结语中的“待 CI／待执行／仅发布 Draft／不合并”保留为首次发布时快照，
> 不代表当前状态；当前精确输入、官方 CI 与合入证据见[四批已合入验收对账](#四批已合入验收对账2026-10-30)。
> 原候选基线 `c6c2b934` 不回写；最终同步后的官方 CI base 为 `593ac9a0`，两者用途不同。

基线为 `c6c2b9344c865bc1c4522238050b15c737978ee8`（#237 已合入），隔离分支
`fix/spec-0341-diagnostic-retirement-batch`。组合 codegen 候选 `7671459c` 与 frontend 两文件
未提交候选；原工作树保留，互不覆盖。此批不是纯测试变更：两处 Unit lowering 生产路径仅精化
既有拒绝的诊断位置，不扩大成功语义；其余三份 Rust 文件为测试消费者迁移。

- `lower_copyable_enum_case_condition` 的非 case、payload、MoveOnly 拒绝定位原 condition
  expression；snapshot-tag 路径仍只允许静态 type-test，第一项非 type-test 若为 expression，
  拒绝定位该 expression。UnsupportedNode 不降级为成功；AST 身份损坏仍以 MissingFact 失败。
- `enum_tag_slice_does_not_claim_general_payload_grouped_or_move_only_equality` 原名和四份完整
  fixture 不变：`other`、`(Flag.On)`、`Flag.On(1)`、`Flag.Off`。严格 MemoryDocument Unit
  保留四项 UnsupportedNode 与原精确 condition span，增加 source/entry 身份及 MoveOnly
  subject 实际 last-use drop 与非 String-cleanup 路由断言。净少一个 `lower_scalar_file` 调用点；
  `lower_frontend_return_tests.rs` 不再调用旧入口，不声称整个 codegen Single 入口已退休。
- frontend 的 B01/B02 原 Single 测试移除，同名 Rooted/MemoryDocument 测试及原四份源码保留。
  以独立字面 oracle 固定完整 raw/final Diagnostic：L0159 非 provider 三源，L0084 binding
  mismatch 一源；包含 severity、message、primary 与有序 labels，final 必须 Diagnostics，
  不能将 Internal 或成功视为等价。净少 2 组 producer 与 2 组 comparator 路径；共用 Single
  helper 仍有消费者，不计 caller 文件归零。既有四组空 oracle、其他完整 comparator/recovery 不变。
- 无新增语言能力、依赖、fixture、runner、CI 选择或 skip；不删除独立 cleanup/loan-end witness。
  不外推 native 或 multi-source 等价；D10/E2 保持 in-progress。

| 验收项 | 精确输入/选择 | 状态 |
|---|---|---|
| 基线、组合与完整 oracle | 上述 main 与候选，五 Rust 文件组合 diff | 独立只读静审无阻塞；无本地动态结果 |
| 格式、文档、diff、尺寸 | 五文件 rustfmt；check_docs.py；git diff --check；check_rust_sizes.py --base c6c2b934 | 静态通过；39 个旧超限文件无增长，无新增例外 |
| 直接错误与邻接回归 | 官方 codegen 完整 lib（含四原负例）；frontend type_iteration/type_iteration_unit 完整 targets | 待官方 PR CI |
| 双平台 check/clippy、既有 bounded/native/preview 门禁 | 本 Draft PR 的官方 CI，保持原选择 | 待执行 |
| 本地 cargo/build/check/test/clippy/native 或全量测试 | 用户要求禁止 | 未运行；无本地 red/green 证据 |

仅发布 Draft PR，不 Ready、不 formal approve、不合并。后续 CI 结果须绑定实际 head/base/merge，
不得用 #237 的结果冒充本批生产 span 修改验收。

## 55. Nullable 类型名与 Range 完整标签候选（2026-10-30）

基线为实际 main `eea83d4cfa896287cb3fafe703d36dae0489019b`，隔离分支
`fix/spec-0341-nullable-type-diagnostics`。本片为 Phase 2 诊断候选，旧 Single witness
及所有原源码保留，净退休为 0；不触碰 while lowering、L0082 标签差异或其他工作线。

- Unit body checker 的 `type_name` 对 Nullable 递归调用现有 inner formatter 并追加 `?`；
  不扩充合法操作数，不修改类型事实、错误码、主消息、primary 或 labels 顺序。
  非 nullable formatter 分支保持不变；nullable nominal/intrinsic 等内部类型仍沿用其既有
  inner 显示规则，本片不声明一般类型名美化已完成。
- `integer_range_values` 的 Int?/Long? × `..`/`..<` 四源不再跳过完整诊断对照：
  Single、rooted Unit、MemoryDocument Unit 均由既有独立 source-derived oracle 核
  L0085 severity/message/operator span、左右 operand 的有序 labels 与无额外 details，
  再核 raw==final、无部分 range facts。原 26 个非 nullable 负源、其他成功/拒绝测试、
  同轮 SourceMap/ParsedFile 身份、完整 raw/final comparator 与 L0082 独立 oracle 均保留。
- formatter 还服务 expected/found、调用、分支、构造和容器诊断，故风险不限于 Range；
  官方 CI 须保留完整既有 bounded stage、CLI/LSP 与双宿主门禁，不缩 runner 或测试选择。
  本片没有新增 CLI/LSP golden，不把 frontend 断言当成其消费链验收。
- checker 物理行 1184→1185，沿用现有 1226 行例外上限，不改 policy；此增长只修正
  既有 formatter 分支，独立静审确认职责与既有拆分约束未扩，不机械拆文件或压行。

| 验收项 | 精确输入/选择 | 状态 |
|---|---|---|
| 原源码、Single witness、完整固定诊断 | 两 Rust 文件的有界 diff | 静态核验通过；六个 test 定义及旧 API 调用数不变，无本地 red/green |
| 格式、文档、diff、尺寸 | rustfmt 两文件；check_docs.py；git diff --check；check_rust_sizes.py --base eea83d4c | 静态通过；823 Markdown、2035 手写 Rust，39 个旧超限文件；无新增例外 |
| 独立静审 | 生产一臂、原源码/Single/oracle 保全及一行增长 | 无本地候选阻塞；非 formal approve、非行为验收 |
| 编译、clippy、直接行为及共享 formatter 调用方 | 官方既有完整 CI 选择，绑定后续精确 head/base/merge | 未运行 |
| 本地 cargo/build/check/test/clippy/native | 用户要求禁止 | 未运行 |

上述为本地候选阶段的静态结果；现按用户批准提交并发布 Draft PR，启动官方 CI，
并邀请 Slack 独立审查。CI 与审查须分别绑定发布头核验；不 Ready、formal approve 或合并，
不以本地静审替代正式审查及动态验收。D10/E2 继续 in-progress。

### 55.1 首轮 CI 下游标签 oracle 修复候选

- Draft #243 head `b93330cabcfab9a8dac2d704c44fc46b7a9f8023`、base `eea83d4c`、
  测试合并 `84ac27a8` 的 [run 41057274362](https://github.com/Halckon/Koven/actions/runs/41057274362)
  attempt 1 实际失败。双宿主 frontend lib 均229通过；codegen lib 的
  `nullable_when_replacement_without_reproof_keeps_exact_type_error` 同报唯一失败：
  实际 `expected declaration#0, found declaration#0?`，旧 oracle 仍拼 Nullable Debug。
  Linux 2542通过/1失败，macOS 2541通过/1失败/1原ignore；两者exit101。
- `check_core.sh` 因此提前退出，CLI/LSP、stage 内 Range 六项、tutorial 与后继产品门均未到达；
  其上传缺失是前序失败的连带，preview跳过，aggregate失败。不得记录为相关目标通过。
- 此后仅更新两处测试期望：codegen generation 的真实失败标签，以及静态发现、首轮未到达的
  frontend grouped-null comparison 同类 oracle。两者以既有真实 Node declaration 身份固定
  `declaration#N?`；仍独立核 Nullable(inner)==实际 Node、参数/实参身份、stale smart-cast
  缺席及完整诊断。第二处增加显式 nullable inner 整值断言；不调用生产 formatter生成期望，
  不放宽 code/Span/details、修改原源码或把预期拒绝改为成功。生产不再变动。
- 此修复按授权接续原 Draft #243；两文件 rustfmt、docs（823）、diff、尺寸（2035 Rust/39旧超限）
  静态通过，独立静审无候选阻塞。未运行本地Cargo或动态检查，新头官方CI与Slack审查待验。

## 四批已合入验收对账（2026-10-30）

本节只补记 #237、#239、#240、#242 的已核实交付事实。§52 冻结候选快照与
§53／§54 的原始发布记录保留；本节优先说明这四批的当前合入状态。§55／§55.1
候选与失败历史原样保留，其后继 #243 的合入和验证状态在下方单独记录，不借四批 CI 为其背书。

### 精确输入、官方 CI 与实际合入

以下各 run、`CI Passed` 与相应 `Code CI Evidence` 均已回读为 success；测试 merge
是官方 PR CI 的组合输入，不是最终写入 main 的 merge commit。每行证据只适用于本行，
不以旧 head、其他 PR 或当前 main 的运行结果替代。静审、官方 CI 与合入分别记录。

| PR | 最终 CI base | 最终 head | 官方测试 merge | 官方 PR CI | 实际合入 main |
|---|---|---|---|---|---|
| [#237](https://github.com/Halckon/Koven/pull/237) | `18c23efd` | `bb2093b4` | `b230d9ec` | [41017271001](https://github.com/Halckon/Koven/actions/runs/41017271001)，success | [`c6c2b934`](https://github.com/Halckon/Koven/commit/c6c2b9344c865bc1c4522238050b15c737978ee8) |
| [#239](https://github.com/Halckon/Koven/pull/239) | `593ac9a0` | `3f44eb21` | `0f136272` | [41027117780](https://github.com/Halckon/Koven/actions/runs/41027117780)，success | [`67d68a46`](https://github.com/Halckon/Koven/commit/67d68a4656e79b5dce6c2d4fa18216d897777175) |
| [#240](https://github.com/Halckon/Koven/pull/240) | `67d68a46` | `5fa8fa7f` | `b5e5df0d` | [41039872478](https://github.com/Halckon/Koven/actions/runs/41039872478)，success | [`e69ad0aa`](https://github.com/Halckon/Koven/commit/e69ad0aaccbe00f888616e47e4a8339df5afc0cf) |
| [#242](https://github.com/Halckon/Koven/pull/242) | `e69ad0aa` | `11239aa6` | `6b55a4c8` | [41047646309](https://github.com/Halckon/Koven/actions/runs/41047646309)，success | [`37c4e188`](https://github.com/Halckon/Koven/commit/37c4e1888624fdd511250c9a7aabf1037f6c6129) |

§53 对应 #237 的 3 个 codegen Single 调用点与 4 组 frontend producer/comparator 退休；
§54 对应 #239 的 1 个 codegen 调用点与 2 组 frontend producer/comparator 退休。
#239 含两处生产 span 精化，不能记为纯测试变更；其成功路径与原拒绝谓词未扩张。
以上两节原源码、完整诊断、事实断言及剩余 Single 消费者边界均继续适用。

### #240：range iteration 与 trusted borrow

- 仅改 `integer_range_iteration.rs` 与 `multi_borrow_range_boundary_tests.rs` 两个测试文件，
  相对最终 base 为 +42/-70；本批未改生产实现、Guide、runner、CI 选择或 skip。
- frontend 退休原 Single producer，保留 33 份原源码、3 个原测试、Rooted/MemoryDocument
  双入口和原事实断言。正例使用完整空 raw/final oracle；L0159 拒绝保留独立固定完整诊断，
  final 必须完整等于 raw，Internal 或成功不作预期拒绝。
- codegen 退休 trusted short-range 的旧 lowering producer，保留原源码、标准库文本、
  `trusted.ko`、range 与 range-extension 双授权、严格 Unit admission，以及
  `UnsupportedNode` 和 `view(source, take(items, 1))` 精确 span；相邻两源 control 保留。
- 当时为避免 #239 共享文档冲突而将验收暂记 PR；本节补入账本，不改变原 diff 或扩张退休范围。

### #242：Range authority 与 extension

- 仅改 `type_range_source_authority.rs` 与 `type_range_extension.rs` 两个测试文件，
  相对最终 base 为 +78/-137；保留 authority 6 个及 extension 13 个原测试和源码。
- 两文件原 4 个 `check_types`、4 个 `resolve_names` 调用点删除，未搬至兼容 adapter。
  authority 的授权隔离、input-order、foreign SourceMap、第二 source、decoy 及真实
  SourceId/Span 合同保留；raw 接受不代表 complete、ownership 或 native 资格。
- extension 保留 source-qualified `UnitTypeRefId`，将原 Single 独占的 `source_kind`、
  `receiver_mode`、`origin`、`from_span`、`source_span` 五项补入 Unit binding oracle。
  原 code/predicate-only 拒绝不冒称完整 Diagnostic golden，不放宽拒绝为成功。

### 审查与剩余边界

- [Grok 补审](https://app.slack.com/archives/C0CBTCC2X2R/p1793363634557559)与
  [Cooper 补审](https://app.slack.com/archives/C0CBTCC2X2R/p1793364304960869)均为合入后的只读
  Slack 审查，不是 GitHub formal approval。2026-10-30 12:55 UTC 回读四 PR 的 reviews 均为 `[]`，
  不补写或冒充正式 approval。
- [补审边界澄清](https://app.slack.com/archives/C0CBTCC2X2R/p1793364509858319)：snapshot-tag
  首个非 TypeTest 非 Expression 时仍回退 span，属诊断精度边界；固定 offset 属独立字面 oracle
  的维护约束。nullable 的原 Single/Unit 诊断一致性缺口并非四批迁移删空全部 oracle，
  两个 Unit 入口的完整比较仍保留；该补审结论对应 `eea83d4c`，后继 #243 的状态另列，不当作当前仍未修复的证据。
- 本次四批未退休 while shape 的旧独立对照，未证明全局 Single 消费者归零；本地
  cargo/build/check/test/clippy/native 均未运行。D10/SPEC-0341/E2 保持 in-progress，
  不外推一般 native 或 multi-source 等价，不将本账本对账当作整体完成或当前 main CI 通过。

### 后继合入与 V2 基线边界

- 本次文档对账基线为实际 main [`ad9b3f11`](https://github.com/Halckon/Koven/commit/ad9b3f1143441f05ee1a9c16bd540e6e3133e241)，
  已包含 [#243](https://github.com/Halckon/Koven/pull/243) 与 [#244](https://github.com/Halckon/Koven/pull/244)。
- §55／§55.1 的 #243 已合入 [`24b00d5d`](https://github.com/Halckon/Koven/commit/24b00d5dfb1e51cebc6315f0765da981e6b72e81)。
  其最终 head `f251b3b7`、base `eea83d4c`、官方测试 merge `1fae6d75` 的
  [run 41060319957](https://github.com/Halckon/Koven/actions/runs/41060319957)、`CI Passed` 与
  `Code CI Evidence` 均 success；不同于 §55.1 保留的失败 run `41057274362`。
  因而 §55／§55.1 的“候选／待验／不合并”是各自记录时的历史状态，不代表当前 PR 仍待合入。
  该后继恢复本片四个 nullable Range 负源的完整标签对照，Single witness 保留、净退休为 0，
  不将 nullable 外壳显示修复扩大为通用复合类型美化或 while shape 已完成。
- #244 的五文件范围是 `conformance/ORACLE_AUDIT.md`、`conformance/README.md`、
  `scripts/conformance_gate.py`、`scripts/conformance_matrix.sh` 及其脚本测试。
  它增加离线证据校验、双宿主比对和路线图阶段／领域审计，未修改 SPEC-0341、Rust 编译器
  或本四批退休文件。既有 [V2 Spec 关联账本](../../../conformance/ORACLE_AUDIT.md) 已将
  `y01_import_take_for_sum`、`y03_method_take_for`、`y04_take_for_println` 三个 run_ok probe
  关联到 SPEC-0341；#244 没有改变此关联或 probe/期望源码，也不新增这三项的能力通过事实。
  因此仅作为本次已包含的基线与验收基础设施关联，不计入 D10/E2 退休量，
  也不把 V2 阶段里程碑的条件投射写成当前能力已经完成。

## 56. Range endpoint Single 路径有界退休候选（2026-10-30）

基线为实际 main `66d66a58a252175493dd7173fec95a14263fca1b`，独立云工作分支
`fix/spec-0341-range-endpoint-single-retirement`。本片只改一个 frontend 测试文件与本节，
无生产、Guide、fixture、runner、policy、CLI/LSP 或 while 修改；Architecture 生产事实不变。

- `invalid_range_endpoints_have_operand_diagnostics` 原名字和 15 对端点 × 两个运算符源码
  全保留，包括四个 nullable 与 26 个非 nullable 负源。先将原 Single comparator 隐含的
  resolved/Boolean/builtin/Copyable/annotation 结果固定成独立 oracle，再切换该测试入口。
  新的专属 Unit producer 明确核真实 root 的 `UnitTypeKind::Error` 与 `Copyability::Error`，
  不固定内部数字 ID、不从观察输出或生产 formatter 生成期望。
- Rooted 与 MemoryDocument 共享同一 SourceMap、SourceId、ParsedFile，实际 source-qualified
  root/operand 查询、operator 完整 Span、两入口完整诊断比较和无部分 range facts 保留。
  原 L0085 severity/message、左右有序 labels、无额外 details、raw == final 的固定断言不变；
  final 必须是 Diagnostics，Internal、成功或未到达类型阶段均不充作预期拒绝。
- 原三入口 `observe` 与其他五个测试保持，L0082 的 Single declaration label 和 Unit 空 details
  两套独立完整 oracle 不变。新 endpoint producer 不调用 Single，也不先执行旧路径后过滤结果，
  不引入 mode 开关或把旧调用搬到 adapter。为隔离其他 witness，局部保留小范围 Unit 接线重复。
- 静态净变化是 1 条 Single 测试路径、30 个源码展开观察：本测试 90 → 60 个观察格；
  六个测试定义均保留。共享的 `resolve_names`、`check_types`、`complete_file_types` 各仍有一个
  调用点，caller 文件与这三个 API 调用点净减少均为 0，不声称整文件、全局 Single 或 D10/E2 完成。
  §55 的 formatter 修复仍为净退休 0；其既有 CI 不替代本候选验收。

| 验收项 | 精确输入/选择 | 状态 |
|---|---|---|
| 原源与 oracle 保全 | 六测试、30 源、其他五测试及原 observe/rejected_api/helper 字节核对 | 静态通过；移除本片新增项并恢复唯一调用后逐字等于基线 |
| 格式、文档、diff、尺寸 | 目标 rustfmt；check_docs.py；git diff --check；check_rust_sizes.py --base 66d66a58 | 静态通过；823 Markdown、2035 Rust/39 旧超限；目标 611→721 行，无新例外 |
| 独立静审 | Unit root/诊断/身份合同、Single 调用边界与退休计数 | 无 P0/P1 或候选阻塞；独立字节核验一致；非 formal approval 或行为验收 |
| 新头编译与行为 | 既有 stage 全选 integer_range_values 六测试；官方双宿主完整门禁 | 未运行；未发布 |
| 本地 Cargo/build/check/test/clippy/native | 用户禁止 | 未运行 |

本节记录本地静态候选；现按用户授权提交并发布 Draft PR，触发精确头官方 CI。
CI 与 Slack 独立审查分别闭环，不将本地静审替代动态验收；不 Ready、formal approve 或合并。
D10/SPEC-0341/E2 继续 in-progress。


## 57. Owned-unit 来源门收敛与 recovery 身份负合同（2026-11-01）

基于 main `4491f1a412f6caf8e9faa8fbfd6f1a84da373d2f` 的本地候选；不触 E3/refine，
不删除任何旧 witness，也不把来源门去重计作 Single 消费者退休。

- basic/const 两个生产 handoff 复用私有 source/name/signature 校验；仍只重建一次 index，
  `MismatchedSource` 优先于 `MismatchedAnalysis`。两种 ownership→typed owner 与能力检查
  继续分别执行，不增加公开 API，不更改 basic/const/recovery 的可交换性。
- 同批将 call 参数声明来源查询复用既有 `source_callable_signature`；原 receiver、range
  等四个生产消费者与参数查询共用同一 Declaration/Symbol 查找。External/FunctionValue/
  StructuralComponent 的无源码语义和缺失参数的精确内部错误保持；真实 top-level/member/
  external 源码正例与参数越界负例注册于 frontend lib。此项减少一份生产查找逻辑，非消费者退休。
- 新 MemoryDocument 封闭交接合同覆盖真实非零 SourceId、名称/环境 clone、同 typed 重查
  ownership、fresh names/environment/typed 拒绝、输入种类变化及来源与分析同时错配。
  常量专用入口同时检查有/无常量声明，两者均不能获得 basic 能力。
- raw recovery 的真实类型错误来源还需独立核对身份；用户诊断不授权跳过 provenance。
  回归使用真实 producer，不伪造私有事实，不扩大 recovery 入口可见性。
- 新 raw 测试注册于现有 frontend lib；MemoryDocument 测试注册于已被 stage integration
  选择的 `owned_compilation_unit_view`，不新增单独 CI 或改变执行顺序。
- 格式及静态审查与行为验收分离；未运行本地 Cargo/build/test/Clippy，按用户授权准备发布 Draft PR 并验证精确头。
  当前 main 已有独立 E3 Targeted 失败，不能归因为本片或用旧绿 CI 代替精确头验证。

## 58. 所有权语法规则共享与成功清理合同（2026-11-02）

本地准备基线为 `f824daeb`，#278 发布时对齐 `2370718c`，15 个文件与该基线变化无交集。
本批收敛 frontend 重复规则并补成功清理合同；不触 E3，
不计作旧消费者退休。

- 元素索引身份仅共享整数字面量、Group、正负号解释；source-qualified 准入、place
  构造及错误传播仍由原入口保持。Single 多层索引与 Unit 单层索引能力差异不在本片修复。
- borrow-return 来源仅共享现有声明语法选择，保留首匹配、marker/span 与两端 symbol
  身份映射；既有 inout/own 排除及不支持来源的诊断边界保持，不借重构扩大语义。
- `ownership_map_require/loan_end_unit` 对原成功源码走 MemoryDocument/Rooted：核真实
  receiver 来源 loan、非 owning binding、planner 发布的实际 last-use end 及后续 owner 交付；
  不将 `UnitLoanFact::end_span` 的调用源码范围冒充实际 loan-end。
- `ownership_iteration/conditional_source_unit` 核 If/When 选择的临时 List source 在
  normal/continue/break/return 与独立 exhaustion 上的 source-qualified 身份、完整动作序列
  和 source drop；回边不得结束 provider/来源，分支 factory 不得重复析构。
- 旧 Single 成功 witness 原样保留：Unit 的 temporary expression/value-origin 证据并不等同
  Single 的 CleanupOwnerId/cleanup_conditions owner-table；本片不退休该独立合同。
- `withValue` 正常返回尚无同范围可查询的实际结束事件，不能用 direct-call 发布切片替代；
  不为补测试扩大其准入或修改执行语义。
- 未运行本地 Cargo/build/test/Clippy；静态检查不替代精确头官方 CI。旧原件删除另待
  等价覆盖与实际验收证据，D10/E2 保持 in-progress。

## 59. requireValue 成功消费者退休与借用调用寻址共享（2026-11-02）

本地准备基线为 #278 合后的 `6f7dedd`，#280 发布前已同步 `e89c363`；
精确头 `133316ea` 的 PR CI `41429518757` 全绿后，#280 合入 main `62966ec`。
以下退休仅限本节明确的 frontend 消费者，不代表全部 Single 观察等价。

- `ownership_map_require` 两个旧成功测试的源码原件已由 `loan_end_unit` 保留：四种
  MutableMap value 与 readonly Map，均走 MemoryDocument/Rooted。新的合同覆盖旧
  Receiver marker、单结果 binding、真实来源 loan 和无诊断，并额外检查实际结束点与 owner 交付。
- #278 精确头 PR CI `41414146989` 已在双平台实际通过这些合同；此次退休旧 Single helper
  的 `resolve_names`、`check_types`、`check_ownership` 各一个静态调用点（各五次运行调用）。
  保留所有拒绝测试，不将测试总数下降误写为行为覆盖下降。
- `borrow_result_call` 两端重复的 Group-only 调用寻址共用私有函数，typed 来源资格仍由
  原 Checker 提供；AST 查找失败仍返回 None，不穿透其他表达式、不扩大任何借用准入。
- `withValue` 保持当前语义与原 witness；Single owner-table、多层索引和 inout 未等价合同不退休。
- #280 精确头双平台实际通过新增共享寻址合同与 require 三项测试、Preview；未运行本地
  Cargo/build/test/Clippy。原源码 Single diagnostics/binding/loan 观察有意退休，保全的是
  Unit 侧上述独立断言，不声称双 Unit 仍执行 Single 或保持其全部独立观察。


## 60. Map、primitive 与字段替换合同族退休（2026-11-02）

#281 基于 `e89c363`，精确头 `68e4938c`，已合入 main `4723ebc`。本节只记录
frontend 测试观察面收敛，无生产、E3/refine 或 `withValue` 语义修改。

- 四个测试文件（含一个 Unit helper）保留 38 个原主文件测试，退休 75 次 Single 分析：
  Map 27、field replace 26、primitives 22；三个旧分析 API 各净少两个静态调用点。
- Map 对原源分别执行 MemoryDocument/Rooted，null/key 清理从原 AST 定位表达式与边界，
  独立核对 origin、调用返回点及消费/字段 root。并不声称每个测试都比较两入口完整结构。
- field replace 保留已有完整双 Unit 对照，将原 sibling 计数固定成 Unit 独立 oracle；
  global 拒绝差异、临时值与多源身份旧 witness 保留。
- primitives 保留三个旧独立测试（六次 Single 执行）的 return loan-end、条件清理和
  owner lineage；其他调用不先执行 Single 再过滤。replace/swap 参数数量及临时清理断言保留。
- 这是有意移除对应原源码的 Single 观察。其他源码的 L0133/L0134/L0135 见证仍存在，
  但不能冒充已退休原源码的 Single 等价证据。
- 精确头 PR CI `41443578094` 成功：双平台 Map 13、primitives 14、field replace 14 项
  实际通过（包含子模块与保留测试），Check/Clippy、Targeted、Preview 完整通过。
  此处不将旧基线 PR CI 直接称作合后 main 组合验证；未运行本地 Rust 门禁。

## 61. 资源与 Rc 合同族退休候选（2026-11-02）

Draft #282 的精确头 `2ae1e859` 基于 `e89c363`，三个 frontend 测试文件，
尚未在包含 #280/#281 的新 main 组合上验证或交付。不得由下列旧基线绿灯推断已合入。

- resource 的 18 项测试、19 处普通 plans 调用、27 个循环展开源案例改为双 Unit，
  保留 named-drop 的 name/boundary/text/order，并先核 source-unit 再投影原 AST 边界。
  七项 inspected 测试、八处 callback 调用、15 个源案例保持，包含 owner/condition/field-parent、
  精确 receiver 诊断及 deferred；不宣称 Unit named-drop 投影承接全部 owner graph。
- Rc 四项测试、五次旧分析改走双 Unit，保持同入口重新分析稳定性、四个 effects、
  SharedOwner root、Place loan 及 L0132/L0131/L0134 和拒绝时空产物。
  额外完成门与 owned.validate 有效；is_same_analysis 自身份断言不算独立差分证据。
- 合计退休 32 次 Single 执行，三个分析 API 各净少一个 Rc 静态调用点；resource 的
  旧分析调用点因保留 inspected 等合同仍存在。withValue、生产代码与 E3 不变。
- PR CI `41460279741` 对该精确头/旧基线成功，双平台 Rc 4 项、resource 30 项实际通过，
  Check/Clippy、Targeted 与两平台 Preview 均成功，无重跑。本地 Rust 门禁未运行。
- 非阻塞边界仍保留：resource 沿用 raw typed→validate（不等同 Rc 完成门）；Rc 是
  同入口重复稳定性，并未额外证明跨入口完整 effects 结构相等。

## 62. 聚合构造与解构下一批范围（2026-11-02）

本批 #283 基于 main `4723ebc`，修订精确头 `8f9d0fb879bfdcdd9dd7f0d11f12eebc00d8072a`
经 PR CI `41474073941` attempt 2 成功后，已合入 main `7f3f7fb4`。
目标是以原源码、原环境、原诊断 sidecar 和独立 AST/typed oracle 承接 construction 与
structural 的可等价合同，不更改生产能力、Guide、withValue 或 E3。

- construction 的 guarded-owner 条件与不同 owner 身份没有等价 Unit 公开事实，原 Single
  witness 必须保留；其他构造 source-order、root、Nothing 前缀与精确拒绝合同逐项迁移。
- structural 的 projection kind/field/copyability/type、成员与结构调用 target、诊断顺序与
  primary/label span 必须保全；自定义环境不换 standard 环境，fixture 和 sidecar 不放宽。
- 本地实施保留全部 12 个原测试：construction 迁七项、十个不同源码及一次重复分析，
  退休 11 次 Single 执行，guarded-owner 原测试与旧 helper 逐字保留。structural 初版拟迁
  四项五源，但精确头 `cd3bef82` 的 PR CI `41470688662` 在首轮 MemoryDocument 显示
  自动 component1 的 L0132 不等价：field 测试六诊断仅有五个，partial-move fixture 缺
  `214..224` 的第二个 L0132；Rooted 后轮未到达，不能据此推断其结果。
- 修订版将这两个 structural 测试及三个原源完整恢复 Single（包括旧 helper、全部八项
  projection、诊断与 fixture sidecar）。仅迁另外两个已在旧头通过的源；合计退休
  13 次 Single 执行，三个旧分析 API 静态调用点净减少均为零。源码字节和 custom 环境不变。
- construction 每入口在同一 SourceMap/AST 上重新分析，比较完整 construction plans；
  structural 原八项 projection、receiver type、Source 与 StructuralComponent 目标在恢复的
  Single 原件中保留，不再把失败的 Memory 原源观察声称为等价承接。
  source-qualified ID 先核来源，再索引实际原 AST/名称，不能跨来源裸比较。
- 初版双向静审未发现上述动态差异，不能当作等价证明；目标 rustfmt、git diff --check、文档检查（835 Markdown）
  与尺寸守卫（2070 Rust、41 项旧超限，无新增债）通过。未运行本地 Cargo/build/test/Clippy。
  修订版精确头 PR CI 已通过；此证据不冒充后续 #286 与 main 新组合验证。任何不等价项保留 Single，
  不以其他源码同码、测试数或静审替代实际验收。

D10/SPEC-0341/E2 继续 in-progress；以上局部退休不关闭未等价 owner、loan、索引或 E3 边界。

## 63. two-phase 与 closure 拒绝合同迁移候选（2026-11-03）

原候选 `cbef473d` 基于 main `4723ebc9` 独立重建，不声称恢复此前未发布的提交。
本地组合现基于 main `be1022f3`，三个 borrow Rust 文件与已独审候选逐字相同；
仅保留主干账本并顺延本节编号，不将旧静审升级为新头动态验收。

- `ownership_two_phase_borrows` 的 10 个测试、21 个源码配置改由 MemoryDocument/Rooted
  独立检查原诊断、span/label 与错误清空 oracle；原 Worker prelude、文件名和标准环境保持。
  剩余 8 个 Single 测试保留，包括实际 loan-end、owner/temporary drop、activation 与
  overload 选中事实，以及错误时 `loan_ends` 清空见证（Unit 无同名完整列表）；不以 Unit 源码范围代替这些独立 witness。
- `ownership_closures` 的 6 个测试内 15 个分析源例迁至相同两入口，保留原限制性环境、
  源码和全部诊断/空事实断言。混合测试的 `this` capture 成功 witness 仍 Single，
  真实 closure last-use loan-end、owner cleanup、transferability、fixture 等不迁移。
- 本片候选替换 36 次 Single 分析执行（21 + 15），每入口各 36 次，不等于删除 36 个测试。
  两个旧 Single helper 仍有调用者，旧 API 静态调用点净减为 0。
- 两入口分别满足原 oracle，不声称跨入口所有 facts 结构相等，也不声称完整 ownership
  final-admission。源码具备真实诊断差异时必须保留 witness，不放宽 oracle。
- 未运行 Cargo/build/test/Clippy，尚无新头官方 CI；迁移仅完成静态准备，不能据此宣称
  行为等价或 D10/SPEC-0341/E2 完成。本片不改生产实现、E3 或 withValue。

- #288 初版精确头 `98bdb187` 的 PR CI `41566478040` 双平台 Cargo Check 通过，
  Clippy 因 `ownership_closures` 重复加载 parser assertion 模块及四个传递模块失败；
  Targeted/Preview 未执行，不能视为行为已验收。本地修订把共享 helper 改为引用 crate 根
  唯一 parser assertion 模块，two-phase 根补同一声明；源码、断言与迁移计数不变，
  不添加 lint allow。修订仅静态核验，仍待新精确头官方 CI。

## 64. Nullable 旧测试双入口迁移候选（2026-11-03）

Draft #286 原精确头 `6a582b0749382ea05ec2e09e82f4d6514786a44d` 基于 main `4723ebc`，
PR CI `41555963781` 成功仅覆盖该旧基线组合。本地现将 #283 合后的 main `7f3f7fb4`
三方合入：只解决本账本冲突，不改 nullable 测试语义；新组合尚未发布或获官方 CI 验证。
本候选为重新构建而非此前未发布工作区的恢复；不修改生产实现、Guide 或 E3。

- `ownership_nullable_when` 七个拒绝测试保留 11 个原源与全部原断言，改为分别执行
  MemoryDocument/Rooted。其余 19 个 Single 成功/owner/loan/cleanup-condition 见证原样保留。
- `ownership_checking` 的 12 个 `!!` 测试移入行为专属子模块，保留 25 个原源配置及一次
  重复分析；每种入口执行原诊断、主 span、drop、transfer/Abort、错误清空及循环合同。
  原 AST 查询只将 Unit operand identity 显式投影到同源 expression；helper 检查来源资格。
- 两个测试目标各自保留原名称/类型环境：`!!` 仍使用原限制性 `environments()`，nullable
  when 仍用 `standard_environments()`；不追加 provider 或改源码以让 Unit 通过。
- 候选退休 37 次 Single 分析执行（11 + 25 + 1），不等于删除 37 个测试或旧公开 API。
  两个原 Single helper 均仍有调用者；静态 API 调用点净减为 0。原测试函数叶名称、字符串 token
  和断言数量逐项静态核对不变；源文件名保持。`!!` 的完整测试路径增加子模块前缀，
  两个现有 CI target 已被 stage 全选，不依赖旧完整测试路径。
- 每个入口独立检查同一原 oracle，不声称完整跨入口 facts 相等、完整 final-admission，
  或与 Single 所有内部事实等价。保留的 owner/loan 独立见证不被 Unit span/临时事实替代。

| 验收项 | 目标/选择 | 本候选结果 |
|---|---|---|
| 原源与断言保全 | 两原测试目标及 `ownership_checking/non_null_unit.rs`，共 57 个测试名称 | 静态字符串 token 与断言数量核对通过 |
| 原候选格式与本次 diff | 原候选直接 rustfmt；本次 git diff --check | 原候选格式通过；本次仅解文档冲突，Rust 原字节保留、diff 通过；不运行 Cargo |
| 原发布头行为及 Clippy | PR CI `41555963781`，精确头 `6a582b07` / 基线 `4723ebc` | 成功；不外推当前 main 组合 |
| 当前 main 组合行为及 Clippy | main `7f3f7fb4` 与 #286 的本地合并 | 未运行；待发布授权及新精确头官方 CI |

D10/SPEC-0341/E2 继续 in-progress；#286 仍为 Draft、未合入；本地新组合未发布，不将旧基线绿灯或本地静审作为组合验收完成。

## 65. Map 查询与 consume 类型合同的本地候选（2026-11-03，基线 be1022f）

本片独立基于已包含 #283/#284 的 main `be1022f`，不改写上方旧基线验收账本。
仅迁移 `type_map` 中已有 Unit 对照的五项测试；D10/E2 仍为 in-progress。

- nullable get/index、MoveOnly get/index、nullable borrow argument 三项共八个原源，
  改为同一 SourceMap/ParsedFile 上 MemoryDocument/Rooted 分别分析。保留 L0130/L0136/L0084、
  原 primary span 切片、withValue 消息谓词与 map_gets/non_null_uses 空事实。
- consume 的四种 local-root K/V 与两个用户同名方法原源保持。两入口均精确核对
  Map<K,V>；用户方法 target 保留 source-qualified Symbol，factory target 保留 Declaration
  到原 symbol 的映射，方法返回 Int、调用数、构造数和 complete 原分析身份合同不变。
  先独立补齐 Single/Unit 的 map_consumes 空事实，再退休 Single 对照；九张 Map 事实表
  对普通用户 consume 均须为空，不能按方法拼写劫持为 intrinsic。
- 合计候选退休 14 次 Single 执行；consume 文件 resolve_names/check_types 各少两个静态
  调用点、complete_file_types 少一个。type_map 的旧 helper 与其他十项 Single-only 测试
  原样保留；全目标仍为 17 项。生产、Guide、ownership witness、withValue 与 E3 不变。
- 已运行目标直接 rustfmt、git diff --check、文档结构门（835 Markdown）及基于 be1022f
  的尺寸门（2072 Rust、41 项既存超限，无新增债）；静态核对测试名称、原源码行及其余
  十二项顶层测试逐字不变。未运行 Cargo/build/test/Clippy 或官方 CI，未推送；以上不构成
  双入口动态等价证据，也不代表本片可合入。后续需精确头官方 type_map 全目标及相关门禁。


## 66. Text extension 双模式消费者迁移候选（2026-11-03）

Text 独立候选最初基于 main `4723ebc94f92412128490ea1f59706ba414fa1d4`；当时的
候选不包含 #283/#286 变更或账本。当前 §68 合批已对齐 `be1022f`，其 main 基线已包含
#283，不再将 #283 描述为尚未合入；本切片不判断 #286 的验收或合入状态，也不把其他
批次的候选数量累加为本批完成量。此节仅记录 Text 切片，D10/E2/SPEC-0341 仍为 in-progress。

- `type_text_extension` 的六个旧 Single 消费测试改为同一个 SourceMap、SourceId、
  ParsedFile 和 standard environments 下独立运行 MemoryDocument/Rooted；保持原
  `koven-std/koven/text/text.ko` 路径、`package koven.text` 前缀和精确宿主授权开关。
- 保留原解析/名称无诊断、成功类型无诊断、L0164 拒绝、无 text binding/primitive、
  owned result、receiver identity、非 range 扩展及 primitive 参数全 Borrow 等判据。
  两模式均核 canonical callable，并以真实输入 SourceId 核 binding.source、以本轮
  callable receiver 类型核 binding.receiver_type，不跨分析比较类型或 callable ID。
- 来源授权隔离测试、显式/别名/wildcard import、缺失 import 与 shadowing，以及
  builtin Result/foreign TextError 的真实多源测试原样保留；测试函数仍为十个。
- 本片移除 `resolve_names`/`check_types` 各一个静态调用点；按原源码与授权组合展开，
  六个测试分别为 1、2、1、10、10、7，共 31 次原 Single 调用，而非 31 个测试。
  每个组合现在分别执行两种 Unit 输入模式。以上为静态盘点，不声称动态通过或最终退休验收。
- 仅改测试与本文，不改生产语义；未运行本地 Cargo/build/test/Clippy，未发布或合并。
  格式与静态门不替代后续精确头官方 CI/Slack 审查，也不代表完整诊断 golden、ownership
  或 native 对等性；不据此关闭 D10/E2。

## 67. 整数 Range 正例消费者退休候选（2026-11-03）

独立本地基线 `4723ebc9`；本节不累计其他尚未合入的候选，也不宣称 D10/E2 完成。

- `integer_range_values` 四组正例保留原源码与断言，改用 rooted/MemoryDocument 两个
  真实 Unit producer，复用同一 SourceMap、SourceId 和 ParsedFile。四组分别有
  4/6/8/8 个源码样本，移除 26 次 Single 分析执行，保留 52 次 Unit 分析；这是运行路径
  的静态计数，不是已执行测试结果，六个测试函数均保留。旧 API 静态调用点未删除。
- 原 comparator 隐含的结果 builtin、Boolean 标记、copyability、annotation identity、
  range fact 数量改为固定预期；membership 为 2 个事实，equality 为 3 个事实，
  普通构造与存储为 1 个事实。继续检查 AST/Unit-qualified operand/result 身份、操作种类、
  空名称/raw 诊断、resolved 与绑定原分析的 final admission；两 Unit 模式完整比较现有
  Observation 字段。shape 原本仅打印，未将其冒充结构化比较或扩大为所有 typed facts 等价。
- public API 正反例与 L0082 特有的 Single builtin-declaration label 完整原样保留，
  endpoint 拒绝测试不变。ownership loan、cleanup owner-table 等独立 witness 未改动；
  本片不新增 ownership 证据，不修改任何生产语义、公开 API 或 CI 选择器。
- 本地仅作 rustfmt、diff、文档图与 Rust 大小静态检查；未运行 Cargo/build/test/Clippy。
  新候选仍须精确头官方 CI 行为验收，不能用旧基线结果替代。

## 68. Text 与 Range 本地合批基线（2026-11-03）

本地组合基线为 main `be1022f3251f45dfd2b47ebede98875638ba9b9f`，整合独立静审候选
Text `bbed2b88b15cf21ca20527dfab20f86d75174eb0` 与 Range
`61226192007abc9dd8eecf01f8f16cd6ced2d883`。上两节的 `4723ebc` 是各自准备基线，
不是本组合基线；按当前 main 的既有账本保留第 60–62 节，新增节顺延，不覆盖历史记录。
两份 Rust 测试文件与各自候选逐字一致，没有在合批中改写源码、断言或生产语义。

本组合静态计数为十个迁移测试中的 57 次原 Single 执行（Text 31 + Range 26），
不是 57 个测试，也不是动态通过证明；Text 仍十个测试、Range 仍六个测试，保留的
多源与 Single 专属拒绝合同不动。没有累计 #283/#286 等其他候选的退休数量。
本批仅本地准备、格式/差异/文档/尺寸核验和 bundle 交付；未运行 Cargo/build/test/Clippy，
未推送、发布或合并，仍需后续授权及精确组合头官方 CI/独立审查。D10/E2 保持 in-progress。

## 69. Primitive 类型合同族退休候选（2026-11-03）

本地分支 `fix/spec-0341-primitive-contract-retirement` 基于 main `be1022f`，仅改
`type_ownership_primitives` 测试与本账本；本节为待官方精确头 CI 验证的候选，不宣称已合入。

- 保留全部 17 项测试与原源码字节；其中 15 项退休 33 次 Single 分析（按原循环静态展开），
  `resolve_names`、`check_types` 各净少两个静态调用点。本文件不再调用旧 Single 分析 API。
- 14 项标准环境单源测试将原独立 oracle 放入每入口 callback，在原 SourceMap/AST 上分别
  检查 Rooted 与 MemoryDocument；custom binding 测试保留双入口及原自定义环境，
  不以标准环境替换显式 external intrinsic 身份。
- BASIC 的两条 replace/swap fact 分别在每入口实际类型表中断言 `Builtin(Int)`；
  原 operand AST 顺序、来源资格、fact 查询、external target、type arguments 与 parameter
  modes 保留。nominal/nullable/generic、trial rollback、错误原子性及静态发散事实不放宽。
- 两项真实多源测试逐字保留，继续核输入顺序与 import alias 身份，不降为单源。
  deferred 的 raw 无诊断不等同最终消费成功；原 final admission 结果比较和 Internal
  错误分支保持，匹配拒绝不记作成功准入。
- 这是有意退休原源码的 Single 观察，不声称双 Unit 证明全部 Single 等价。未修改生产代码、
  `ownership_primitives` 的真实 loan-end/条件清理/owner lineage witness、withValue 或 E3；
  Phase 2 静态 primitive descriptor 也不替代 Phase 3 commit/native 证据。
- 静态核验保留 17 项测试身份（首项移除 `single_` 前缀）、全部源码字符串与两项多源原件。
  直接 rustfmt、`git diff --check`、文档检查（835 Markdown）和尺寸守卫（2072 Rust、41 项
  旧超限，无新增债）通过；目标文件由 638 行减为 501 行。未运行 Cargo/build/test/Clippy，
  未推送或发布；动态行为与正式 CI 尚未验证，D10/SPEC-0341/E2 继续 in-progress。

### Primitive 候选首轮 CI 与本地修复

Draft #291 精确头 `5f4505d9` 的正式 PR CI `41575210627` 在 macOS/Ubuntu 的
Workspace Check 阶段均报 12 处 E0507：callback 借用 `&CompilationUnitTypes`，
却直接调用消耗 `self` 的 `validate`。Targeted 与 Preview 跳过，未产生动态通过证据；
此前静审未识别此编译错误。

本地修复仅在这 12 处先 clone 同一 typed product 再 validate，保留每入口原成功/拒绝
断言及原 product 的跨入口比较；其余持有 owned product 的校验不变。未修改生产 API、
final admission、测试输入、两多源原件或 ownership witness。修复尚未推送、未运行本地
Cargo/build/test/Clippy，仍需新精确头的正式 CI，不以静态检查替代编译和行为验证。


## 70. Exclusive root return 前缀事件观测（2026-11-03）

基于 main `be1022f` 的本地候选仅增加 Unit 实际 planner 动作的只读观测，不退休 Single。

- `exclusive_call_prefix_ends()` 仅接受无 receiver、owned-result 的直接 declaration/symbol/external
  调用与 Exclusive owned 名称根；从真实 `EndCallLoan` 顺序提取 return 放弃前缀事件，
  保留 source-qualified call/argument/target、控制点及同点先行普通 drop 数。
- 不从 loan descriptor 或 `end_span` 推断执行，不更改注册、正常返回退休或 unwind 遍历。
  provider、projection、borrowed binding/result、receiver 与正常完成不在此观测片内。
- 新测试在 Rooted/MemoryDocument 原源上核对 replace return、嵌套逆序、直接 inout 调用、
  abort 无 unwind、正常返回无前缀事件及未访问实参/借用根/投影/Shared 排除。
  原 `ownership_primitives.rs` Single witnesses 逐字保留；D10/E2 仍 in-progress。
- 本地仅执行 direct rustfmt、diff-check、文档与尺寸静态门禁；未运行 Cargo/build/test/Clippy，
  测试新增断言尚未动态验证，不把该候选当作等价替换证明。后续需精确头官方 CI。

- 首轮 Draft PR #292 的精确远端头 `c0e99ec9`（与本地候选同 tree）在官方 CI
  `41575238625` 的 Ubuntu job `125718556173` 中，该套件 13 过、3 失败；三项均停在
  新 oracle 把 Inout loan begin span 误比为名称值 span。既有 contract 明确使用源码 `&`
  marker；修订仅从原 AST 的对应 CallArgument 提取 Inout marker，保留精确 span 相等并核对
  源码为 `&`，原 event 身份、控制点和 drop 顺序断言不变。生产实现与 Single witness 不改。
  修订仅经过 direct rustfmt 与静态检查，未本地动态验证，仍须新精确头官方 CI。

### §70 回归补强（2026-11-06）

基于 main `c4ba670a`，仅选择备份 `0bcdb219` 的 Exclusive prefix 测试增量，不合并整支。
现有 primitive 中 break/continue 使用 Copyable Int 且只核无 commit；本片使用 loop 内形成
的 MoveOnly owner，核真实 Exclusive loan、source-qualified call/argument/root、
ControlTransfer Named drop 与无 prefix event/无 replacement commit。Rooted/MemoryDocument
双入口均执行完整 type final gate 与 ownership validate。return 用例额外绑定同一 typed
call 的 External `replace` symbol 身份，避免仅由空 primitive 表推断调用种类。
不修改生产代码、Single witness、观测范围或 E3/native 能力。

| 验收项 | 测试目标 / 过滤器 | 实际结果 |
|---|---|---|
| 编译与共享 helper 邻控 | `unit_direct_call_loan_ends`，整项排除 `exclusive_prefix::abort_and_normal_return_do_not_invent_prefix_unwind_events` 与 `temporary_field_pending::abort_does_not_unwind_the_temporary_field_parent` | 云端 Rust 1.96.0 编译通过；28 passed、2 filtered |
| 静态检查 | fmt、frontend all-targets Clippy（`-D warnings`）、文档、尺寸、diff-check | 全通过；839 Markdown、2118 Rust，39 项历史超限，无新增尺寸债 |

含 Abort 的两项完整保留，仅留官方 CI；不运行故障注入或 Mac 构建。尚未发布或进行
精确头官方 CI，D10/E2 保持 in-progress；本片增加边界证据，不声称退休额外 Single 路径。

## 71. View carrier 类型位置退休候选（2026-11-03）

本候选基于 main `be1022f`，仅修改 `type_range_carrier` 前五测试的观察入口；尚未发布，
尚待精确头官方 CI，不把静审当作运行等价证明，也不与其他候选累计作交付统计。

- 保留全部七项测试及原源码。前五项共 16 源：Borrow 形参三源、非法存储六源、普通
  owned 结果一源、同名用户类型一源、local/generic 绕过五源。每源从三入口降为
  MemoryDocument/Rooted 两入口，候选退休 16 次 Single 执行，观察次数 48→32。
- 新 producer 在同一 SourceMap、SourceId、AST、`range_type.ko` 路径及 standard
  环境上执行两入口；两次 names 均明确断言无诊断并 validate 后无条件进入 type 阶段。
  保留原空诊断、L0162/L0163 的 code 存在谓词，不升级为完整 Diagnostic 等价或准入证明。
- 原 `stage_codes` 返回 `(memory, unit, single)`；本次删除的是调用端旧名 `legacy`
  所指的第三项 Single，并保留其余两项原 oracle。原 helper、第六项七源名称错误恢复
  （含 `from missing`/`from this`）与第七项 from/参数映射、L0164 精确 span 及 qualified
  拒绝边界逐字保留。`resolve_names`/`check_types` 静态调用点及调用文件净减少均为零。
- Guide §N1a 的身份与使用位置边界不变；无生产、ownership、owner/loan-end/drop、E3
  或 native 修改，不以类型事实替代后续阶段证明。
- 直接 rustfmt --check、16 源字节/七测试名/原 helper 与末两测试保全静审、文档检查
  （835 Markdown）、尺寸守卫（2072 Rust，41 项旧超限，无新增债）及 `git diff --check`
  通过。未运行 Cargo/build/test/Clippy，动态验收与后续组合验证均未完成。
  D10/SPEC-0341/E2 保持 in-progress。

## 72. E2 owner transport A 的本地纵切（2026-11-03）

基线 `be1022f`；只扩展 Unit 的独立观察能力，不退休现有 Single owner/condition witness，
不改变 Guide 或 Phase 4 消费边界。此次实现尚未编译或运行 Rust 测试，不算动态验收完成。

- 普通函数的 String Value 参数与非插值 String 构造产生独立、source-qualified owner
  definition；opaque token 带分析内身份，不能跨产物串用。Name 消耗实际当前值，Group
  通过逐求值帧局部结果转交；binding 不重新分配 owner。
- 已求值 Value 实参把同一个 token 放入本 call、本 argument 的 pending 槽；checked root
  replace 的真实 commit 把 RHS owner 交给 root，把旧 owner 交给结果。嵌套调用没有共享
  last-result 槽，commit 前后仍沿用原 cleanup 顺序。
- branch、loop、opaque call、capture/closure、swap、赋值、聚合/解构与未建模类型不在
  A 范围。遇到不支持转换，只回滚该 callable 的新 definitions/events，并保留独立状态；
  不产生语言错误，不写 resource_deferred，不清空旧 drop/loan/iteration facts。
- 新观测内一致性检查核 source/product identity、Name 消耗、binding/pending 槽及 replace
  双向运输。测试覆盖嵌套/二次 replace、参数、Name/Group、同文本不同定义、双源与输入
  换序、跨分析 token、缺失 pending/交换新旧值等损坏事实；这些是待运行测试，非已通过证明。
- 同一已准入 Checker 开启/关闭观察后的旧 planner Analysis 全字段相等测试覆盖普通、
  branch、loop、opaque 与原 capture 源；不得用 drop 数量或来源 span 代替 owner identity。
- capture formation 输入、完整 Single owner graph、branch 条件与 loop phi 仍未交付。
  双入口行为、真实 producer mutation 实验和全套官方 CI 仍待验收；D10/E2 保持 in-progress。

- PR #294 的 `03cc0f68` 官方 CI 暴露跨分析 owner token 使 recovery 完整事实比较失败，
  以及双源 fixture 的包名与 rooted 路径不匹配。修订只在观察整体对应各自产物内的 token，
  保留所有 source-qualified 结构字段、外来 token 隔离和既有完整事实 oracle；fixture
  路径改为 `p/root.ko` / `q/root.ko`。新增重新分析全事实相等、token/查询不互通及损坏
  事实不等断言；旧清理语义不变。仅完成静态检查，未运行本地 Cargo/build/test/Clippy，
  动态验收仍待修订头官方 CI。

- #294 的 `bca2b9cb` 与 main `4c32d21f` 本地组合保留两类独立发布门控：owner 观察仅要求
  successful；Exclusive prefix 仍额外要求 deferred 为空，resource_deferred 早返不发布该
  prefix。补充原 replace-return 源的 owner 观察开启/关闭完整 planner 事实相等回归，明确
  prefix 非空且 Unsupported owner 观察不干扰它。四处生产接线、两份账本冲突已本地整合；
  本节顺延为72，既有64–71完整保留。compilation_unit 组合为1156行，登记精确增长例外，
  待独立静审；仅执行静态门禁，未运行本地 Cargo/build/test/Clippy，未推送，组合动态验证待CI。

## 73. L0082 builtin 实参标签保全候选（2026-11-03）

独立分支 `fix/spec-0341-l0082-label-parity` 基于远端 main
`041e8238fb8793ee7837fbbccdc704a77e83c1fd`，tree
`226587cdddaec9ae697adaf78378e40dbf26caa0`。本片仅修 Phase 2 的真实诊断差距，
不退休 Single，不改变类型接受性、final admission、ownership 或 E3。

- 原 Range L0082 的 Single 带类型名 label，Unit signature 仅有主诊断；Unit body 对同一
  builtin 身份已带 label。signature 的 `ExternalTypeBinding::Builtin` 非空实参一臂
  复用现有 `emit_with_label`，label 取该轮真实 `segment.name_span`，primary、code、
  message 和排序不变。未修改 capability、intrinsic 或 external Value 的独立诊断合同。
- 保留 Range 原全部源码与 Single producer，将双预期收敛为保全既有 Single 的独立
  label oracle，并恢复无例外的完整 raw/final Diagnostic 相等断言；不是只删比较差异。
- 新子模块挂在既有 `integer_range_values` target 下，覆盖 Int/Boolean/IntRange/LongRange
  的 signature 参数与 body 局部标注、return signature 与 body 混合顺序、Unicode/CRLF，
  以及两真实源逆序输入的来源隔离。由源码 token 和固定消息构造完整独立 Diagnostic，
  分别核 signature/body/raw/final 序列；成功或 Internal 均不得作为拒绝。
- 新回归在生产修改前编写；本地禁止 Cargo/build/test/Clippy，未取得动态 red/green。
  静态门与后续官方 CI 分开记账，D10/E2 仍 in-progress，不归档本 Spec。

| 验收项 | 目标与结果 |
|---|---|
| 完整直接诊断与旧见证 | `integer_range_values` 全 target，含新增三个测试；待官方 CI，未本地执行 |
| body 邻接与资格 | `multifile_type_checking`、`multifile_constant_qualification`、`final_type_admission`；待官方 CI |
| 下游与工程门 | 既有 CLI/LSP、双宿主 check/clippy 与完整既有选择保持；待官方 CI |
| 本地静态门 | 直接 rustfmt 与 diff 通过；文档检查 836 Markdown 通过；尺寸检查 2083 Rust、39 项既存超限通过，无新增例外 |

签名文件从 1019 增为 1021 行；沿用现有 1036 行有限例外，不修改尺寸 policy。
新增两行只把同一 L0082 的现有来源交给同文件既有发射接口，不增加签名职责或公开 API；
此说明是本地变更范围核对，不冒称独立审查。未推送、发布、联系他人或合并。


## 74. Automatic component L0132 保全候选（2026-11-03）

本节和 §82 的自动 component 观察属于历史迁移证据。现行 Guide 合同及保留的真实字段
L0132 回归见 [SPEC-0392 §64](0392-compiler-layered-review.md#64-r2-退役自动-componentn)。

独立分支 `fix/spec-0341-component-partial-move` 基于远端 main
`041e8238fb8793ee7837fbbccdc704a77e83c1fd`，不叠加未合入的 L0082 或 owner 候选。

- 静态根因：Unit Phase 2 已生成 `StructuralComponent(field)` 调用与同一调用的真实
  aggregate projection（泛型字段类型已替换）；Phase 3 的 `check_call` 没有 Single
  `checker/call.rs` 既有的末端 partial-move 拒绝。因此 MoveOnly 结果未按原合同报 L0132。
- 只在 `dataflow/traversal.rs` 的正常 call 尾部补拒绝：先结束本次 call loan 与最后使用
  closure，要求存在正常 continuation、diagnostic count 自 call 开始未变、投影确为
  StructuralComponent、实际类型为 MoveOnly；primary 取 callee Member 的 name span，
  label 由原 source-qualified field symbol 查询。保持 Single 的时点及错误抑制规则，
  不新增部分移动状态，不改变 receiver/loan、owner/capture、drop planner 或类型 producer。
- 原四个 structural 测试、custom environment、源码及 fixture/sidecar 保留，未退休 Single。
  新子模块四项分别核原 fixture 的两条精确 code/span 与完整 component Diagnostic、
  borrowed owner 拒绝、Copyable 分量与显式同名 owned-result 方法通过、receiver L0131
  优先且无 L0132 级联。错误源另断言不发布 drop plan；不把此断言外推为全域资源等价。
- 原 fixture 的字段错误先于 component 错误，新增断言保留两者，防止误用全局空诊断门
  漏掉第二条。完整 component 期望由固定消息、旧 sidecar 的 `214..224` 与原字段源码
  位置建立；Single 和两种 Unit 各自接受相同完整期望，不从新 producer 重录。
- 已只读核对 owner Draft #294 的 `0bb1f84702c451f1be5f4e85589b9ea7ff4e4096`：
  其改动没有 `dataflow/traversal.rs`、`ownership_structural.rs` 或新子模块，双方共同修改
  SPEC-0341 追加账本。未发布的云端 capture/分支/循环改动无法核对；合批前仍须复核。

| 验收项 | 目标与状态 |
|---|---|
| 原回归与四项新增普通源码 | `ownership_structural` 全 target；待官方精确头 CI，未本地执行 |
| 邻接调用/receiver 行为 | `ownership_checking`、`ownership_two_phase_borrows`、`multifile_two_phase_borrows`、`multifile_ownership_checking`；待官方 CI |
| 下游与错误门禁 | 既有 frontend/codegen/CLI/LSP、双宿主门禁保持；待官方 CI |
| 本地静态检查 | rustfmt、diff、836 Markdown 文档检查均通过；2083 Rust、39 项既存超限的尺寸检查通过；原四测试与 fixture/sidecar 逐字保全 |

新回归先于生产修复编写；用户禁止本机 Cargo/build/test/Clippy，未取得动态 red/green。
没有独立审查或官方 CI 结果，不发布、不推送、不合并；D10/E2 继续 in-progress。

## 75. Receiver 接受/激活及 Map 接受合同退休候选（2026-11-03）

本地基线 main `041e8238`；不依赖尚未合入的 #294 owner transport，也不累计其他并行候选。
本片只修改两个 integration suite 的四项测试，生产算法、公开 API 和 E3 不变。

- `ownership_two_phase_borrows` 的 completed nested reads 四源、disjoint fields 一源，
  保留原无诊断谓词、源码和 standard environments，分别在 MemoryDocument/Rooted 执行。
- 同文件 loop-exit 两源与 branch-return 一源改读既有真实 `receiver_facts()`。
  Single 的 `Some(DropPoint::CallEntry(call))` 对应 Unit receiver 专用 activation identity
  `Some(call)`，提前退出仍为 `None`；这不是从 loan `end_span` 推断实际结束事件。
  loop 两 reservation 另外核对不同 call ID、原 receiver/call 源片段；branch 核对唯一外层
  reservation 与原 call 源片段，避免仅用数量相等代替身份断言。现有
  `multifile_two_phase_borrows` 的 loop/branch/activation 合同是 main 上的直接 Unit 参照，
  不将该参照或静审称为本次原源码的动态通过证据。
- `ownership_map_with` 仅迁 scoped String action/restored receiver 的无诊断消费者，复用
  原双 Unit helper，继续检查 callback 参数模式/类型及同次 final type admission。
  generic borrow 的四源、原 Single helper 和 loans 数量见证保持逐字不变。
- 合计候选退休九次 Single 执行（4 + 1 + 3 + 1），不是删除九个测试或任何旧 API。
  每个迁移源现在保留两个 Unit 输入模式。所有测试名、其他测试与原分析 helper 保全；
  loan-end、owner/drop、overload 和 conditional-capture 的独立 Single 见证不动。
  receiver activation 不证明 cleanup 完整性，也不授予未支持的 owner/loan graph 能力。
- 直接 rustfmt、静态源码与未改见证/helper 保全、文档检查、尺寸守卫及 `git diff --check`
  用于本地静审；按要求未运行 Cargo/build/test/Clippy。动态验收、精确组合头 CI 和独立
  审查仍待完成，未推送或合入，D10/SPEC-0341/E2 继续 in-progress。
## 76. E2 分支 owner 观察边界测试批次（2026-11-03）

基线为 #294 `0bb1f84702c451f1be5f4e85589b9ea7ff4e4096`。本片不开放分支观察，
不修改生产产品或退休 Single witness；D10/E2 继续 in-progress。

- 新增七种真实分支源：if 值、嵌套 if、两支 replace、Boolean subject when、无 subject
  when、单支 return 与隐式 else。每种在成功 replace 前缀之后检查首个不支持点为精确
  source-qualified control expression，definitions/events 整体回滚，旁侧 callable 仍完整。
- 同批源码复用既有 enabled/disabled planner 比较 helper，比较清空独立 owner 观察后的
  全部旧 Analysis 字段、身份与顺序；不是 drop 数量或 Span oracle。损坏观察测试分别
  注入定义与事件，防止 Unsupported 暴露前缀或旁侧 callable lineage。
- 可达性：`owner_transport_tests::branch_boundaries` 与
  `drop_planner::owner_transport::tests::branch_boundaries` 已登记到既有 `cfg(test)` 链。
- 静态检查：独立 rustfmt（只格式化新增文件）、`git diff --check` 和
  `python3 scripts/check_docs.py`。具体执行结果以本片交付记录为准。
- 遵守本轮限制，未运行 Cargo/build/test/Clippy。上列 Rust 行为断言均为新增而未执行，
  需后续精确组合头官方 CI；不据此证明分支 owner graph 已实现或 gate 已通过。


## 77. E2 显式 Unit If 无 phi 观察候选（2026-11-03）

基于 §73 本地边界提交 `76776732`（#294 基线 `0bb1f847`）；不假设 capture #298 已合入。
本片只扩展 Phase 3 的独立观察，不改变合法性/旧 cleanup 产品，不退休 Single witness。

- 显式 else、Unit 结果、两边均继续且 cleanup 后同 symbol 的 owner token 全部相同才
  准入。真实 then/else 事件分别存储并从同一父 location 重放；没有 Choice/phi，更不
  从 span、drop 数量或 symbol 交集伪造 owner identity。
- 原 If cleanup 求值、边界 Drop、continuation 与 merge 顺序移入专用 branch 子模块，
  观察关闭时走相同路径。隐式 else 仍执行真实 branch cleanup，但不进入新观察切片。
- 新测试覆盖分支局部定义和 Drop、两支消费同一个输入 token、原 owner 保持到 join
  后 replace、嵌套 If；损坏测试覆盖扁平化、缺失消费边、外来嵌套 token、重复静态定义
  创建、错配 BranchExit。保留 §73 七源拒绝矩阵并增加支持源全 planner 开关等价比较。
- 独立静审后补充两个真实回滚源：成功 then 后 else 遇 Unsupported，以及成功 If 后
  suffix 遇 Unsupported；均检查精确失败点、全部 prefix/branch 清空、旁侧 callable
  独立完整。它们与双支 replace 不同 token 拒绝源复用相同源码常量，额外比较完整
  planner on/off Analysis，防止仅由观察状态或 drop 数量证明 cleanup 不变。
- 不同存活 token 的 join、String-valued If、return/隐式 else、When/loop/capture
  保持 Unsupported。后续 Choice 需只引用先前定义、禁止前向和循环引用；return payload
  必须先 Consume/交付再进行退出 cleanup，非继续边不得伪装成缺失输入。
- 静态门：独立 rustfmt、文档检查、diff check；结果见本片交接。共用 drop_planner
  文件缩小，未提升尺寸例外。公开 event 与 validator 将和 capture 分支产生集成交叉，
  必须在精确组合头独立审查，不能据各片静态检查宣布组合正确。
- 按本轮限制未运行 Cargo/build/test/Clippy，新增 Rust 断言尚未动态验证；未推送/合入，
  D10/E2 保持 in-progress，不据此关闭分支身份整体欠账。

## 78. Capture formation 的保守后继候选（2026-11-03）

基于 #294 精确头 `0bb1f84702c451f1be5f4e85589b9ea7ff4e4096` 的独立本地候选；
仅扩展 Phase 3 Unit owner 观察，不退休 Single witness，不改变语言或 Phase 4 能力。

- 支持普通函数直线路径中的 leaf move closure：捕获列表非空、全部为 Owned/Move 的
  String binding。形成点保留原 `take_named` 次序，读取实际移出的 `OwnedValue.owner`；
  不从 capture reference span、声明位置或旧 drop 数量生成 owner。
- 环境构造产生单独 `Closure` token；`Capture` 保存当前环境与有序 `(source, owner)` 输入，
  所以 replace 后捕获的是新 root owner，旧结果另有独立 token。Name/Group/binding 转交
  环境 token，不重建捕获。实际 `Captured` drop 点发布 `CaptureDrop`，随后才发布环境的
  named `Drop`；回放核对环境 token/source 槽、输入位置与先释放内容再释放环境。
- Shared、Copyable capture、捕获 closure 值、空 capture、直接丢弃的 closure temporary、
  调用执行和其他未支持转换仍使整个 callable 观察原子 Unsupported。lambda body 自身仍
  不支持；观察 formation 不证明 body 执行、跨调用交付、动态实例、branch/loop phi 或 native。
- 新定向测试核 replace 后两来源形成、环境别名运输、相同文本不同 owner、owned 参数，
  并拒绝输入 token 交换、缺 capture release、外来分析 token；原完整 planner 开关对照另加
  两项纯 formation 源。非法事实 mutation 仅写入测试，未本地执行。
- direct rustfmt、`git diff --check`、尺寸守卫（2089 Rust、39 项既有超限，无新增超限）与
  文档结构检查通过。未运行 Cargo/build/test/Clippy，未推送或合入；编译、行为和官方 CI
  仍待验证，独立静审另行记录。D10/SPEC-0341/E2 保持 in-progress。

- 独立静审未发现 P0/P1；补强 lambda observation 非空断言，另加环境形成 token 与
  capture release 的 environment/owner 三处跨分析负控。两个实际同时存活的环境各保留
  独立输入，测试在首环境释放点分别替换为另一环境、source 槽及 owner，要求全部拒绝。
  共六项 capture 定向测试均只完成源码与静态检查，仍未编译或执行，不据此宣称动态通过。

## 79. Capture 与 If 分立观察整合候选（2026-11-03）

- 本地组合 #298/#299 已审发布树，依赖 #294；保持两类独立切片，不把组合可达性
  默认为已验收能力。同 callable 首次遇另一类语法时原子 Unsupported，清空观察前缀，
  嵌套 If 内 capture 同样拒绝；每个 callable 重置 admission，既有 traversal/cleanup 不变。
- validator 独立拒绝同时含 Closure 定义与 If 事件树的观察，防止 conservation 成功被当作
  混用授权。未开放 capture×If，所以未把 capture lookup 扩展为跨互斥路径任取输入。
- 新增五项测试：双向及两边/嵌套混用的确切拒绝点与 sibling 隔离；空 If 注入 capture
  负控；Copyable 参数跨 join 与 String owner 正例；嵌套真实控制树与重复 control 负控；
  同源 observation on/off 完整 Analysis 对照。既有两片所有正负例和 Single witness 保留。
- 本地不运行 Cargo/build/test/Clippy。组合静态检查、独立审查与精确官方 PR CI 结果
  分别登记，不能把两片历史绿灯转写为本组合已通过。D10/E2 保持 in-progress。

## 80. 循环拒绝边界测试候选（2026-11-04）

- 基线 main `ba9d61f5`；生产逻辑不变。八种 While/Loop 输入覆盖常量 false 零轮、
  未改变条件的回边、continue/break 和嵌套循环，精确核对外层 statement Syntax 拒绝，
  完整清空观察前缀并保持 sibling；另核独立前缀确实准入及 Unsupported 事实注入拒绝。
- `owner_transport_tests::loop_boundaries` 与 planner `tests::loop_boundaries` 分别承担
  admission 和完整 Analysis differential；后者保留 cleanup 全字段、身份与次序。
- 本地仅静态检查，禁止 Cargo/build/test/Clippy；编译和行为未执行，官方精确头 CI 待验。
  不宣称真实多轮运行或终止，不开放 loop owner/phi/capture 组合；D10/E2 继续未闭合。
- 本地 direct rustfmt check、`git diff --check`、文档结构检查（837 文件）及 Rust 尺寸
  守卫通过；独立静态审查未发现阻断项。以上不替代尚未执行的编译/行为验证。

## 81. Range 接受与 receiver activation 的有界退休候选（2026-11-04）

基于 main `ba9d61f5`，与独立诊断候选分开；仅两项测试、三次 Single 执行退休候选。
不改生产、公开 API、fixture、CI 选择、E3 或其他 Single witness。

- `integer_range_values::range_identity_does_not_enable_extra_public_apis` 仅把 IntRange/
  LongRange 两个合法正例改为原有双 Unit helper。独立冻结旧 comparator 的 range fact
  数量、builtin、boolean、copyable，保留 `assert_complete` 对空 names/raw、resolved、
  final success、真实 range fact 的检查及 annotation identity；原 AST root/operator 与
  operand/result identity 检查仍在 helper 内。十八个拒绝源、原 Single producer、完整
  raw/final Diagnostic oracle 与隐含 comparator 字段均保留，不宣称整个 public API 族退休。
- `ownership_two_phase_borrows::completed_nested_call_releases_conditional_temporary_capture`
  原源分别走两种 Unit 输入，读真实 receiver_facts，保持无诊断和 activation=call，另核
  唯一外层 reservation 与原 receiver/call 源片段。现有 multifile 同类正例是实现参照，
  不是本候选原源码的动态通过证据。仍只 names/typed basic validation，不执行 ownership
  validate 或 final admission，不冒称 capture cleanup/loan-end 等价；其他测试及旧
  Single helper 不变。
- 盘点中撤回 numeric snapshot 十源候选：其旧 helper 的 Single iteration validator 还
  检查 AST/typed provider、exit condition 对应 cleanup_conditions、AST Return 枚举。
  Unit template/required_exits validator 不能代替这些完整隐含 oracle，原文件逐字保留。
- 最终候选独立只读静审无 P0/P1；直接 rustfmt --check、源码/测试名/helper 保全、
  十八拒绝源与 numeric 文件保全、文档检查（837 Markdown）、尺寸守卫（2097 Rust，
  39 项旧超限、无新增债）及 `git diff --check` 通过。
- 本地不运行 Cargo/build/test/Clippy，不推送或创建 PR；静审不能替代本精确组合的动态
  验收。D10/SPEC-0341/E2 继续 in-progress；不累计其他未合入候选的退休量。

## 82. Component 与 builtin 诊断重复 Single 观察退休候选（2026-11-04）

基于 main `ba9d61f5` 的本地候选，只迁六项测试、十三次重复 Single 执行；不改生产、
公开 API、fixture、Guide、CI 选择或 E3，不依赖新 owner observation 扩张。

- `ownership_structural/component_partial_move.rs` 四项测试各去掉一次 Single producer。
  原 MemoryDocument/Rooted 循环、fixed Diagnostic、fixture/sidecar、完整 component label、
  L0131 主 span、projection/call target 身份、copyability 与失败 drop 空表原断言保留。
  父 suite 的原综合 projection/fixture Single 测试及 helper 不动。此处仍是 basic
  names/typed validation，不升级为 final-admission 或 cleanup 完整性证明。
- `integer_range_values/builtin_argument_diagnostics.rs` 前两测试去掉九次 Single producer：
  四 builtin 的 signature/body 各一源，共八源，另有 CRLF/Unicode 的复合源一项。
  两入口各自对源码 token 独立构造的完整 Diagnostic 检查 signature/body 分区、raw 与
  final Diagnostics；不接受 Internal 或成功冒充拒绝。删除的跨 Single 相等检查由原已存在
  的独立 expected 约束覆盖，不以双 Unit 互相比对替代固定 oracle。
- 第三项反序多源诊断测试原样保留且不计退休量；父 suite 的 public API/L0082 Single
  消费者和隐含 comparator 字段不动。两个子模块全部测试名及原源码保留。
- 独立只读静审未发现 P0/P1；直接 rustfmt --check、测试名/源码及原父 suite/fixture
  字节保全、第三项 Unit-only 测试保全、文档检查（837 Markdown）、尺寸守卫
  （2097 Rust、39 项旧超限，无新增债）和 `git diff --check` 通过。
- 本片仅静态开发与独立静审；按要求不运行本地 Cargo/build/test/Clippy，动态验收仍待
  精确组合头官方 CI。候选尚未发布，不将历史 main CI 转写为本候选通过。
  D10/SPEC-0341/E2 继续 in-progress；owner/loan-end/conditional cleanup 等缺口不关闭。

### Expected move literal 类型消费者后继（2026-11-06）

基于 main `c4ba670a76a567e1475deee47378fa791d25f419`，独立分支
`feature/spec-0341-move-literal-consumers`。本片不依赖 #326–329，不改 #323 或生产。

- `type_callable/expected_move_literals.rs` 两项原测试改走同一 AST、原自定义 builtin/
  capability 环境的 MemoryDocument/Rooted 双入口；两个 Single 分析改为四个 Unit 分析。
  保留原源码与测试名称，父 target、其余十九项测试和既有多文件对照均不改。
- 正例保留 local/argument/return 三个 lambda 与同一 ordinary Function canonical
  identity 的相等性、Int→Boolean、三个真实参数的 Borrow mode 和 Int 类型。
  反例保持六条 L0084 顺序及 primary 文本、shared/first/second 的 mode 为 None，
  无 expected 的 inferred 仍为零参数 move Function→Int。
- names 原已要求无诊断；Unit helper validate 后仅取 raw types，不增加 final/ownership
  前置。UnitSymbolId 从真实 body_symbol_types 按 source-unit 与原 local symbol 匹配，
  包含错误 lambda 参数的类型恢复事实；不伪造符号或用名称直接代替类型身份。
  lambda 仍从原 AST 枚举并严格核数量，再由已验证源身份限定 UnitExpressionId。

| 验收项 | 检查 / 命令 | 实际结果 |
|---|---|---|
| 云端编译与直接行为 | Rust1.96.0、默认debug=2、locked/offline，`type_callable expected_move_literals::` | 目标编译通过，2 passed；各含双入口，19项未执行 |
| 共享邻控 | `multifile_type_checking lambdas::` | 4 passed，150项未执行 |
| 静态质量 | frontend lib build、all-targets Clippy、fmt、docs、尺寸、diff | 全通过；839 Markdown、2118 Rust、39项历史超限，无新增债 |
| 原合同保全 | 原源码/测试名/事实断言与父target | 机械保全与独立只读复核无阻断；测试模块174行 |

未执行故障注入、含 Abort 的整项、Mac 构建、native 或官方双平台 CI；未推送或开 PR。
本片不是新的 closure/native 能力，D10/E2 保持 in-progress，不等待其他 CI，不自动合并。

## 83. E2 Choice 测试模型前置（2026-11-04）

基线 main `8b2e9e18`；本批不开放 String-result If，不改变公开 API、生产 planner、cleanup
或 Single witness。候选仍处于模型验证阶段，不代表语言路径或下游 lowering 已可使用。

- `owner_transport_tests::choice_model` 仅由已有 `cfg(test)` 路径编译。私有 ChoiceId 具有
  product/callable/control 身份，Value 区分真实 definition 与选择引用；选择不新增 producer。
  真 Unit If 观察提供 source-qualified definition catalog，手工事件图只验证传输合同，
  不声称这份源码已经产生 String-result Choice；也不从图推断缺席的 Return/opaque call。
- 模型仅有一个选择、两条 String 定义输入及完整后续，拒绝前向/循环选择引用；分别回放
  两边的 Pending/Binding/Move/Forward/Replace/Drop，Replace 的 incoming/previous 均覆盖。
  静态创建区域唯一，未执行分支定义必须保持 Uncreated，其余定义须实际 Drop；终态 Result
  不冒充返回或 temporary cleanup。Create 和 join 的结果槽都核对唯一占用。
- 两项真实 String-result replace 消费者仍核精确 Unsupported 点/原因、定义/事件全清空、
  sibling Complete 及原 cleanup 数量。对应源码通过同输入 observation on/off helper
  比较清空独立观察后的完整 Analysis；原 `ownership_primitives` Single cleanup 见证不改。
- 对抗图覆盖反置/错误边输入、错误尾值、外来 product/source/callable、未定义 choice、
  已 drop/未创建输入、前向/循环引用、重复消费、错误 Pending/Binding 槽、未消费结果、
  未选边重建，以及先占用 Result 再创建/合流的槽冲突。
- 独立静审指出的 Create/join 短暂结果槽冲突已补防护与两项回归。按用户约束未运行本地
  Cargo/build/test/Clippy；Rust 动态验收与官方精确头 CI 均未完成，不能以静态模型宣称通过。
  正式生产接入仍需独立接口兼容合同、真实 tail 交付证据和准入验证。

本候选静态验收：独立复核未发现剩余 P0/P1；直接 rustfmt --check、文档结构检查
（837 Markdown）、Rust 尺寸守卫（2101 文件、39 项旧超限，无新增超限）与
`git diff --check` 通过。新增八项模型/真实边界测试和一项完整 Analysis 差分测试，
其中恶意图十九种；这些 Rust 测试尚未执行，不能当作动态通过记录。

## 84. Resource terminal 重复 Single 观察退休候选（2026-11-04）

基于 main `8b2e9e18`，仅迁 `ownership_resource_deinit` 三测试、六次 Single 执行：
root replace 返回一源、field replace 返回一源、field replace 中断四源
（return、Abort、break、continue）。不改生产、公开模型、Guide、CI、target 或 E3。

- 原源码、测试名及独立 drop 数量、target、顺序、原 AST 边界期望保留。三测试改用
  MemoryDocument/Rooted 两种真实 Unit 分析，既有 helper 在 named-drop 投影前提供
  实际 ownership facts 检查；不能用投影后的 NamedDrop 冒充完整 drop 表。
- 保留 names/typed 校验、无诊断/无 deferred、typed identity 相容和 source-qualified
  drop 原 AST 校验。field 返回案例原 ownership validate 仍在；不扩张其他案例门禁。
- 同文件 distinct owner origins、old/new owner 不同、field parent owner identity、
  resource 条件 deferred 差异及旧 inspected_plans 均保留。本片不证明新 owner graph
  已支持 resource、field replacement、Return terminal 或 Loop，不替代这些独立合同。
- 文档结构检查、尺寸守卫、diff 检查及原测试名/源码字面量/非候选代码静态保全通过。
  本地 Cargo/build/test/Clippy 未运行；获准发布 Draft PR，动态验收待精确候选头 CI。
  D10/E2 继续 in-progress，不以历史 main CI 计入本候选通过。

### Resource 类型消费者有界迁移（2026-11-06）

基于 main `c4ba670a`，本片只迁 `type_checking` 的六项资源类型消费者，
不改 §84 ownership terminal、生产代码、Guide、CI 或其他在途候选。

- 两项 deinit 与四项 replace/swap 测试按完整领域迁至私有 `resource_unit` 模块；
  保留原函数名、源码、parser 与名称/类型环境。测试完整名称增加 `resource_unit::`
  前缀，仓库内未发现依赖原完整名称的精确过滤器。
- 每源实际运行 MemoryDocument/Rooted 两入口，检查入口 key/source 身份及 names
  diagnostics/validate。保留无诊断、has_deinit、两项 call 数量及 L0097/L0122/L0084
  原 oracle；has_deinit 查询改用真实 Unit 签名声明中的 nominal 描述。
- 未限定 deinit 的 names-only 拒绝源仍留原处；未迁其余 Single 消费者。不新增 ownership
  或最终 typed admission 结论，不把资源类型通过等同于资源 owner graph 支持。
- 云端 Rust 1.96.0、默认 debug=2 下，`cargo test -p lang-frontend --test type_checking
  --no-run --locked --offline`、`resource_unit::` 六项及原 names-only 精确测试一项通过；
  `cargo build -p lang-frontend --lib --locked --offline`、frontend all-targets Clippy
  （`-D warnings`）、fmt、尺寸守卫、diff 检查与独立审查通过。
- 根测试文件从 2193 行降至 2123 行，新模块 123 行；未提升历史尺寸额度。
  不运行本地 Abort/故障注入、全量 type_checking 或 Mac 构建。仅备份独立分支，
  尚未创建 PR；本候选官方 CI 未运行，D10/E2 继续 in-progress。

## 85. Choice 前置：真实 control tail 内部通道（2026-11-04）

基于 main `58479df5`，本片仅保留实际 control body 求值产生的尾表达式及已有
`EvaluatedOwner`，不开放不同 owner 的 String-result If，不新增公开协议。

- `control_body` 保留原 bool 包装；私有 `evaluate_control_body` 只在表达式尾部
  返回实际 expression 与单次求值结果，空体、非表达式尾部及前缀退出不制造尾值。
- `control_tail` 直接复用 `evaluate_owner_expression`；Read 不交付 owner，尤其不能
  把已执行 temporary cleanup 的值返回为可交付身份。原临时清理、作用域清理、分支
  出口与 borrow 处理顺序不变。此通道既不从最后一个 event 猜身份，也不新增 producer。
- 原公开 `UnitOwnerId`、事件及 observer API 不变。String-result If 仍精确 Unsupported、
  definitions/events 全事务清空；现有 Single cleanup 见证不退休。真实 body adapter 的
  独立测试不等同于完整 String If 的生产准入证明。
- 后续兼容方向为独立版本化 value observation，旧 observer 对 Choice 继续 Unsupported。
  新层须把 Definition/Choice 区分贯通 Binding/Pending/Replace 两端及 Drop，并验证
  两边实际 tail、正常继续、外部存活身份及完整 continuation，不能仅接 producer。
  在该协议及验证完整前，不放宽现有准入或承诺公开 API 形状。

候选新增实际 literal/local Group 尾身份、Read 临时清理、空/非表达式/退出体与
普通 When/replace 消费者回归，并比较完整 Analysis observation on/off 产品。
仅执行静态检查；本地 Cargo/build/test/Clippy 禁止运行，Rust 动态结果待获准发布后的
精确头官方 CI。本片为内部前置，不代表 D10/E2 或 Choice 生产能力完成。

## 86. Container 诊断消费者迁移候选（2026-11-04）

基于 main `58479df5`，仅迁 `ownership_containers` 三项测试、四次 Single 分析：
相同/未知 element index 冲突两源、已 move 容器 element 读取一源、size 读取一源。
复用既有 `mutable_unit::check_each` 的 MemoryDocument/Rooted 真 Unit 路径，不改 helper。

- 保留原源码、测试名、诊断序列 L0135/L0131、size 主 span `value` 与原失败 loan/drop
  空表期望。原 helper 仅有 parse/name/type 成功和 ownership 分析，无隐藏 owner/loan-end
  validator；本片不删除其余 Single 使用或 field/element lifetime 事实。
- Unit helper 逐源核 names/types、final admission、typed identity、完整诊断 code 序列和
  ownership 失败资格；增加的准入检查尚待本精确组合动态验收，不把类型错误当所有权成功。
- 不改生产、Choice/capture/loop、resource 或 receiver 测试、Guide、CI、target。
  不宣称所有容器消费者退休；此处只保留既有 code/span oracle，不声称完整 Diagnostic 比较。
- 原测试名、源码字面量与非候选代码静态保全、尺寸守卫、diff 检查通过。
  独立静审无阻断；rustfmt 命令不可用，尚未执行，未运行本地 Cargo/build/test/Clippy。
  已获准发布 Draft PR 并运行官方 CI；与并行候选分节记录，合批时保留各节。
  D10/E2 继续 in-progress，正式准入和四源码行为仍需精确候选官方 CI。

### Phase 2 核心容器消费者后继（2026-11-06）

基于 main `c4ba670a76a567e1475deee47378fa791d25f419`，独立分支
`feature/spec-0341-container-consumers`，不叠加 #326/#327 或修改 #323。

- `type_containers` 六项原测试原位迁移：核心构造与 element identity、构造推导失败、
  索引/size/get/set 诊断矩阵、源码同名符号、嵌套容器泛型推导、size receiver/type/span。
  原六份源码、测试名、自定义十六 builtin 与原 intrinsic 环境、重复解析和原 oracle
  保留；六次 Single 分析替换为十二次 MemoryDocument/Rooted 分析，不新增测试数量。
- helper 复用同一 AST/环境，核 source ID 和精确 root/path，名称无诊断后 validate，
  只观察 raw types。size 的 AST ID 由实际 Unit facts 配对，先核 source-unit 再投影局部
  ID；MoveOnly 只适配 Unit API 的非 Option 返回。不增加 final/ownership 前置。
- 原源码同名符号测试此前没有 names 空断言；Unit 公共 API 必需的 names.validate
  及明确空诊断断言收紧名称前置，最终实际通过，不据此改变其原类型/构造 oracle。
- 首轮尝试七项迁移，target 实际 31 passed/1 failed：signature 的 `List<Nothing>`、
  `Array<Any>` 缺两条原 L0125，只产生 L0094/L0091。该项已完整恢复原 Single witness，
  不改预期或生产图算法；deferred callable reference 的原 Single witness 同样保留。
  首轮失败是有界已验证缺口，不是最终迁移通过，后续应独立修复 signature storage 检查。
- 既有二十四项 MutableList 测试与 `mutable_unit` helper 不变。直接 check_types
  静态调用由两处降为一处，剩余 helper 仅服务上述两项 witness；不宣称全仓退休比例。

| 验收项 | 检查 / 命令 | 实际结果 |
|---|---|---|
| 云端编译 | Rust 1.96.0、默认 debug=2，`cargo test --offline --locked -p lang-frontend --test type_containers --no-run`、frontend lib build | 编译链接通过；首轮 Option API 适配错误已修正 |
| 直接与共享邻控 | 完整 `type_containers` | 32 passed、0 failed、0 filtered；包含六项双入口与原两项 witness、二十四项 MutableList |
| 静态质量 | frontend all-targets Clippy `-D warnings`、fmt、docs、尺寸、diff | 全部通过；839 Markdown、2119 Rust 文件、39 项历史超限，无新增债 |
| 原合同保全 | 原源码、测试名、断言与范围外测试/helper | 独立只读复核无阻断；原名称前置差异如上披露 |

未运行故障注入、含 Abort 的整项、Mac 构建或官方 CI；本片未推送或开 PR，
D10/E2 与本 Spec 保持 in-progress，不自动合并。

## 87. Choice 前置：V1 value 投影与库内双路径验证（2026-11-04）

基于 main `d7021580`，仅新增向后兼容、按需调用的
`UnitOwnerObservation::value_observation_v1`；不更改旧公开 owner 事件或现有 observer
准入，不接线 String-result If 的生产求值，也不触及 capture/loop 候选。

- 新 V1 类型区分真实 Definition 与 Choice。Choice 使用独立 product/callable/control
  身份，比较使用 Arc 指针而非结构值相等；不往 producer catalog 添加伪 String 定义。
  对外只有只读 getter，图、边输入及 Choice 身份不可外部构造或改写；事件枚举可被匹配，
  但单独构造事件不能发布新的 observation。旧 API 的形状与行为保持不变。
- 首个生产 adapter 只投影旧 Complete 的直线 String 事实：Create、Move、Forward、
  Binding、Pending、Replace 两端和 named Drop 均使用原 token；不从 span 或 drop
  计数重建身份。完整原 catalog 与 cleanup point/events 原样保留，value Drop 只是
  守恒位置投影，不替代 cleanup 计划。参数、If、capture、unsupported 和未消费结果
  返回 None，不发布半个前缀；现有 Analysis 与 Single 见证不变。
- 原 cfg(test) Choice 回放迁到实际库 validator；不是保留两套算法。单 Choice 分别回放
  两边及完整 continuation，检查真实 producer、边序/尾值、prefix 存活状态、Pending/
  Binding/Replace/Drop 消费、source/product/callable 和未选边未创建。全部实际路径
  必须终止于 Dropped；未消费 Result、跨分析 token、重复/缺少事件和多余 suffix 拒绝。
- 图守恒不证明 AST 类型、正常继续、Unit 函数结果或未记录的 Return/opaque call。
  本片生产 adapter 不产生 Choice；手工 Choice 图借用真实 catalog 仅为 validator 负控，
  不声称该图对应 catalog 的原 AST/cleanup 路径。未来正式接线必须同时保存 typed
  admission 和实际两边 tail/cleanup 证据，仍拒绝嵌套 Choice、When、循环、Return、
  swap、capture×If 与任意调用；不得以图通过放宽这些边界。

验收目标：保留原八项模型/边界测试及原 Analysis 差分；新增真实 nested-replace producer
映射、跨产品 token、未消费/重复/缺失图、unsupported/non-String 投影三组测试；增加
外部消费者 API 可命名/匹配的编译见证。禁止本地 Cargo/build/test/Clippy，Rust 行为与
编译均待获准发布后的官方精确头 CI，静态检查不等于动态通过。本片不代表 Choice
生产能力或 D10/E2 完成。

本候选静态验收：独立复核未发现剩余 P0/P1；直接 rustfmt、文档检查（838 Markdown）、
Rust 尺寸守卫（2100 文件、39 项历史超限且无新增增长）和 diff 检查通过。实现自然归属
owner_transport 子模块，大型 compilation_unit 根文件无差异，尺寸政策未修改。
正式 Choice 接线还需独立真实 producer 收集事务：旧 Unsupported 已原子清空 catalog，
不能复用旧入口的空事实制造 Choice，也不能改变旧入口合同来绕过此缺口。

## 88. Choice 首片：独立 producer 事务与单绑定 typed 准入（2026-11-04）

基于未发布的 §87 本地候选 `89bca6e7`；V1 尚未对外发布，因此本片先修正其 catalog
边界：`producers()` 返回独立只读 producer catalog，而非旧 event observation；Drop
显式携带真实 UnitDropPoint。旧公开 owner API、既有 cleanup 与 Single 见证均不改变。

- 准入仅为无修饰的顶层非泛型普通 Unit 函数，一个无模式 marker 的 Boolean 参数；
  函数 block 唯一不可变 `val`，binding 精确 String/MoveOnly，initializer 是该参数
  条件的显式 If。两边仅无插值 String literal / 多层 Group literal，可包单尾 ControlBody。
  不接受 var、nullable binding、多语句、后续使用、branch 局部声明、Return、
  隐式 String 函数结果、调用、replace、capture、循环、When 或嵌套控制。
- 独立事务复用本轮 product 关联但拥有独立定义表；真实 String 完成点创建 token，
  每层实际求值完成才记录 Group 转交。两边实际 tail/正常继续、每层完成计数、If
  自身 Consume 完成以及先 true 后 false 的收集顺序均须成立，不能只凭 AST计划
  补发 producer 或跳过未发生的 frame 完成。
- `bound_owner` 的实际绑定阶段登记 Choice；旧 OwnedValue.owner 仍为 None，不把
  Choice 伪装真实 producer。仅在真实值移除后的 drop hook 登记精确 local statement
  AfterStatement；唯一本片 binding 没有后续用途，因此其 ASAP 清理必须在此处闭合。
  重复/遗漏/Read/退出/错槽/错清理均原子清空新事务，成功也要再经双路径 validator。
- 新图只作为密封的私有能力挂在原观察上，getter 同时核 product 指针、callable 和图
  守恒。旧 Unsupported 的 definitions/events 仍空，新定义不能通过旧 lookup 解析。
  旧 Eq 保持旧协议事实比较，新能力单独检验；不能用旧 Analysis 相等代替新图验收。
- catalog 自检 product/callable/dense index/唯一 String origin/source；导入旧完整事实
  时保留原 token。Drop 仅支持目前覆盖的 AfterStatement/AfterExpression/FunctionEntry，
  不因 source 相同接受错边 BranchExit 或未验证控制退出。完整清理仍由原 owned facts
  承担，V1 图不反向推导 typed admission 或改变原诊断/清理。

新增真实两边 literal/Group 的端到端绑定与精确 Drop、旧 Unsupported/lookup/sibling
隔离、完整 Analysis observation on/off 比较；拒绝 nullable、参数 mode、多语句、
后续使用、退出、调用与嵌套控制。另有九种手工事务 trace（包含正常trace及八种遗漏/
重复/Read/退出/错清理负控）、跨 product/callable 错附、独立 catalog 恶意索引/源头/
身份与未覆盖 cleanup point 负控。手工 trace 不充当实际执行证明。

未运行本地 Cargo/build/test/Clippy，未推送。Rust 编译、端到端行为与官方精确头 CI
均待授权发布后验收，不宣称 Choice 通用生产支持或 D10/E2 完成。Pending/Replace
消费、branch-local binding、多次 Choice 和后续使用仍是后续独立合同。

本片独立静审未发现剩余 P0/P1；根据审阅补强能力错附、精确 binding 类型、逐层完成、
控制完成与实际 Drop 的 fail-closed 合同。直接 rustfmt --check、文档检查（838
Markdown）、尺寸守卫（2103 文件、39 项历史超限，无新增增长）及 diff 检查通过。
这些是静态结果，不能替代待运行的 Rust 编译与动态验收。

## 89. Choice 后续边界：Group、branch-local 与非继续退出（2026-11-04）

基于未动态验收的 §88 候选 `8ad81194`，本片只增加真实来源回归，不扩生产准入。
前片尚待官方 CI，不能把静态接线或手工模型通过当作继续放宽语言路径的依据。

| 边界 | 当前可证明部分 | 后续必须新增的合同 |
| --- | --- | --- |
| literal 的多层尾 Group | 实际构造一次，每层实际求值完成产生有序 Forward，所有层保留同一个真实 definition；单尾 ControlBody 不增加 producer | 当前窄片内，仅补独立原 AST 身份/事件顺序与完整 Analysis 差分，不扩大到 Group 包整个 If |
| branch-local `val local = literal; ((local))` | 独立真实 body adapter 产生 Binding → consuming Name Move → Group Result；同一 owner 到达实际 tail，已消费 local 不得再次 named cleanup；全函数 V1 仍拒绝 | 新事务需显式 initializer frame、branch-local symbol→value 侧状态和实际 take_named 成功后的 Move hook；每边局部表分别闭合，不复用旧 owner=None，也不从最后一个事件或 drop 重建 token |
| prefix Return / 非继续 body | Return 前实际 producer 可完成并按原事实清理；不可达 String tail 不构造、不交付，scope 恢复；整个新能力保持原子拒绝，sibling 不受影响 | 必须区分 Return、Abort 等终止原因及实际退出清理，不能把 bool continues=false 当成正常缺边；现有双输入 Choice 和两条完整 continuation 合同不变 |

branch-local 下一生产增量应只考虑一条不可变、精确 String/MoveOnly 局部绑定及其唯一
Consume Name/Group tail，initializer 仍限无插值 literal/Group。绑定 hook 必须对应真实
插入且先于 ASAP cleanup；Move 必须以实际 take_named 返回 Some 为证据。取走的局部
值从独立侧状态移除，边出口不得残留局部 owner。原 Binding/Drop 单切片通过后，再对
两边分别静审与官方 CI，不能顺手接 var、额外局部、replace、外部存活 owner 或后续使用。

非继续路径不实施：单边退出需要显式终止/正常继续描述及路径敏感的后续回放。当前
双边必须正常交付的检查不得删除，也不得用未创建 owner 或空事件代替 Return/Abort
语义。Abort 无展开与 Return 清理不能合并成一个“未继续”事件；隐式 String 返回仍拒绝。

本片新增三项回归：真实三层 Group 的精确转交序列；双 branch-local 的真实 binding/
move/tail 与无重复清理；Return 前构造及不可达 tail 的不执行证据。所有源码同时核完整
Analysis observation 开关差分。直接 body adapter 的证据只属于实际 body 求值，不冒充
完整 If 已准入。未运行本地 Cargo/build/test/Clippy，动态结果待精确官方 CI。

独立静审无剩余 P0/P1。Return 前未使用 local 的回归明确核首声明处 AfterStatement
ASAP cleanup，而非宣称 Return 专用清理已证明。直接 rustfmt --check、文档检查
（838 Markdown）、尺寸守卫（2104 文件、39 项历史超限，无新增增长）及 diff 检查通过。
生产代码无差异；所有 Rust 编译/动态门禁未运行。

## 90. Choice 正例独立 cleanup oracle 补强（2026-11-04）

基于 §89 候选 `93a66820`，仅补强原两种 literal/Group 正例的断言，不改生产或准入。
从源码 AST 独立定位 If 及唯一以它初始化的 local statement，分别要求 V1 Drop 与真实
cleanup 指向该声明的 AfterStatement；chosen 恰有一次 named Drop，sibling 恰有一次，
全表恰两项，排除额外 branch temporary cleanup。旧 observer 精确冻结为该 If 的
AfterExpression / Syntax，definitions/events 继续为空；Choice control 与 binding
initializer 同时对独立 AST identity 核验，不以同轮两个产物互相比对代替 expected。

拒绝矩阵原样保留，本片不声称逐例旧 point/reason 已封闭。未运行本地 Cargo/build/test/
Clippy，新增断言与此前候选仍待精确组合头官方 CI；没有推送、发布或合并。

静态验收：直接 rustfmt --check、文档结构检查（838 Markdown）、尺寸守卫
（2104 Rust 文件、39 项既有超限，无新增增长）及 git diff --check 通过。
这些检查不构成新增 Rust 断言的动态通过证据，D10/E2 继续 in-progress。

## 91. Capture release 的分支出口身份防御候选（2026-11-04）

基于 main `d51c2cc7`。本片仅补强 owner observation validator，不改变实际 cleanup、
Capture 与 If 的分立准入、控制尾值、Choice、循环或旧 Single witness。

- 原 CaptureDrop replay 核 source、环境、slot 和 owner，但没有像普通 Drop 一样核
  BranchExit 所属边。新增同一规则：BranchExit 必须等于当前 replay 的 control/branch。
- 回归以真实线性 Owned String capture 的完整观察为正例，只将 CaptureDrop 的 point
  改为同源的伪 BranchExit，分别覆盖 true/false 编号，保留环境、slot、owner 和事件位置。
  线性 replay 无当前边，因此两种篡改均应拒绝。此处是防御性校验缺口，未发现或声称
  生产 planner 已生成错误清理事件；同 callable Capture×If 仍整体 Unsupported。
- 静态检查确认原 guard 对这种 point 仅检查 source；新 guard 拒绝无所属边的释放。
  `git diff --check` 通过。环境没有 rustfmt，格式检查未执行；未运行本地 Cargo、build、
  test 或 Clippy。新增回归尚未动态执行，必须由精确候选官方 CI 验证。

## 92. 顶层 Capture 与 Unit If 顺序组合候选（2026-11-04）

基于 main `d51c2cc7` 与 CaptureDrop edge 防御候选 `68600ad2`，不依赖 #310 控制尾值接口。
本片只扩展观察准入，不改变语言合法性、旧 cleanup、Single witness、Choice 或循环。

- producer 以真实 If body/cleanup 深度限制 Capture/CaptureDrop：顶层允许双向顺序，
  分支内形成和释放仍整体 Syntax Unsupported；嵌套层数在正常和错误退出均恢复。
- validator 独立拒绝 If 子树中的形成/释放。跨 If 存活的环境及 slot 沿既有 location
  replay 分叉，必须在 join 保持一致；不递归查找历史以推测 identity，不增加 phi。
- 回归覆盖 capture→If→同环境转交、If→capture、嵌套 If 后 capture；保留 true/false/
  嵌套分支内形成拒绝及 sibling 隔离。另增分支内释放原子回滚，并比较全部源码的
  observation enabled/disabled 完整旧 Analysis。
- mutation 将原合法 capture 整段移入 If true 边，或将原释放同时移入两边，要求即使
  最终 locations 守恒仍拒绝。先前同源伪 BranchExit 防御回归保留。
- 未运行本地 Cargo/build/test/Clippy；环境没有 rustfmt。静态准备不等于动态准入证据，
  后续必须用精确候选官方 CI 验证。D10/E2 继续 in-progress。
- 独立静审未见阻断，形成与释放分别有单事件双边 mutation，避免一条 gate 遮蔽另一条。
  diff、838 Markdown 文档结构、尺寸门通过（2096 Rust 文件，39 项既存超限无新增债）。

## 93. 顶层 capture 环境的逐边释放候选（2026-11-04）

基于顶层顺序组合 `1f2d6ea8`；仅扩展已在 If 之前形成的 leaf 环境的释放观察。
不改变实际 cleanup、语言合法性、Single witness、循环、Choice 或控制尾值。

- producer 保留 branch depth 对 Capture 形成的拒绝，允许已形成环境的 CaptureDrop。
  slot 仍由当前环境 token 与 source-qualified symbol 查顶层形成事件，不扁平化 If，
  不从任意 sibling/未执行路径搜历史来补身份。
- validator 允许逐边 slot 释放，保留 token/product 身份、captured location、实际 edge
  和先 slot 后环境的守恒校验。两边从同一状态独立 replay，join 必须一致；不借此支持
  分支内形成、非继续路径或携带不同环境的 join。
- 回归覆盖两条边与嵌套三路径的同环境/slot 精确释放、漏释放、重复释放、错误 edge，
  以及 return 边仍原子 Unsupported。之前分支内形成与 foreign-token 边界保留。
  全部新增源码参与 observation on/off 的完整旧 Analysis 比较。
- 本地未运行 Cargo/build/test/Clippy；没有 rustfmt。静态准备和审查不能替代精确候选
  官方 CI，未推送。D10/E2 继续 in-progress。
- 独立静审与增量复审未见阻断；补充一边转交、另一边未使用环境的真实 BranchExit
  正例，精确断言 control/false edge；return 拒绝精确到 Syntax 与外层 If。
  diff、838 Markdown 文档结构、尺寸门通过（2097 Rust 文件，39 项既存超限无新增债）。

## 94. Capture 三片候选的最新 main 静态集成（2026-11-04）

基于 main `d7021580`，仅移入 `68600ad2`、`1f2d6ea8`、`efc767df` 的独立差异，
保留 #308 resource retirement、#310 control-tail 及现有 SPEC 各节。未扩大生产或测试范围。

- 集成前九个候选 Rust 文件与 `efc767df` 逐字节一致；固定工具链 Rust 1.96.0 的
  `rustfmt --edition 2024 --config skip_children=true` 仅改三个候选测试文件的格式，
  生产差异保持候选原样。此前各片所述“没有 rustfmt”为当时验证限制；本次已找到
  固定工具链，先以只读检查确认格式失败，格式修复后对九个候选 Rust 文件检查通过。
- `python3 scripts/check_docs.py` 通过（838 Markdown 文件）；
  `python3 scripts/check_rust_sizes.py --base d7021580` 通过（2098 Rust 文件，
  39 项既存超限，无新增超限债）；`git diff --check HEAD` 通过。
- 静审未见生产语义阻断。尚缺两个存活环境跨 If、单环境多 slot 逆序释放、嵌套边冒用
  祖先 control 的组合回归；此轮只记录，不扩大范围。路径 helper 对含嵌套 If 的边
  只递归子树，完整守恒仍由 `is_consistent()` 校验，不把 helper 当作通用路径枚举证明。
- 未运行本地 Cargo/build/test/Clippy，未推送或创建 PR。正式验收仍须获准发布后对
  精确集成头运行现有官方 CI：fmt、Clippy、macOS/Linux composition、公共入口 conformance
  与 Linux native sanitizer 等适用门禁；特别核 owner transport/capture/combination 的
  lib 回归和 observation on/off 完整 Analysis 比较。历史绿灯不能替代本集成头证据。
  D10/E2 保持 in-progress。

## 95. Choice 与 Capture 的静态组合候选（2026-11-04）

基于 main `d7021580`，完整保留 Choice `89bca6e7` → `8ad81194` → `93a66820`
及 §90 oracle 补强，并保留 Capture 三片与 §94 的格式修正。§91–94 为原 Capture
无编号标题的消歧编号；原验收限制与历史未运行说明保留，不以本次静态检查覆盖。

- planner 同时保留独立 Choice 事务与 capture branch depth。普通 callable 开始时建立
  新 product identity/Choice 准入并将深度归零；unsupported callable 清事务并归零。
- depth 包装实际 branch body、cleanup 和两边 Choice tail 完成通知，正常或错误返回
  均恢复深度。String-result If 仍先使旧 observer Syntax Unsupported；sealed Choice
  独立收集，不借旧定义表或放宽旧 observer。CaptureDrop 的精确 edge 防御保持。
- Choice 准入只有一个 Boolean 参数及唯一 literal/Group If 绑定，任何 capture 或额外
  statement 使 begin 返回 None。因此 capture slot cleanup 不会成为 Choice Drop；
  此组合不是 Choice×Capture 同 callable 的生产接线。
- 无新增生产修复、功能或回归；冲突只做字段/初始化双保留和文档拼接。现有测试分别
  覆盖 Choice→普通 sibling 与 capture/拒绝→普通 sibling，没有同一 Unit 中 Choice
  与 Capture callable 交替的专用回归；该组合隔离仍需官方 CI 与后续明确测试任务。
- 未运行本地 Cargo/build/test/Clippy，未推送、发布或创建 PR。精确组合头仍待获准后
  的官方 fmt、Clippy、composition/conformance、相关 frontend lib 与适用 native 门禁。
  静态检查不能代替 Rust 编译或行为验收，D10/E2 继续 in-progress。

## 96. 同一 Unit 的 Choice / Capture callable 隔离回归候选（2026-11-04）

基于 §95 组合头 `541cf747`，仅新增一项内部回归及模块注册，不改生产、准入或公开 API。
同一 source unit 分别排列 C1/K1/C2/K2 与 K1/C1/K2/C2；C 保留单 Boolean 参数、唯一
literal/Group If 的 val，K 保留既有顶层 move capture 和非对称 If 释放源码形状。
每份源码含四个普通函数及两个 lambda；按真实 AST callable 身份唯一定位观察，不依赖
lambda 与普通函数混合的产物下标，所有符号与表达式文本在输入中唯一。

- C 精确核旧 Unsupported 的本函数 If / Syntax 和空产物、V1 的本函数 catalog 与两个
  literal origin、绑定及 AST 声明 AfterStatement 清理；K 核 Complete、恰两个定义、
  本函数 lambda 的 environment 与 own String 参数 input、真实 If 身份，并要求 V1 None。
- 对每个函数保留自身 token 正向查询，并在全部其他函数的旧表与可用 V1 catalog 中
  拒绝这些 token。两个真实 lambda 各恰一次且保持 LambdaEntry / Callable Unsupported，
  空产物及 V1 None。每种排列继续比较 observation 开关下完整旧 Analysis 的身份与顺序。
- 此回归只约束不同 callable 的事务/能力隔离，不开放同一 callable 的 Choice×Capture。
  Fixture 既有名称/类型/ownership 前置保留，基础 validate 不额外冒充最终类型准入。

静态检查：直接 Rust 1.96.0 rustfmt、文档结构检查（838 Markdown）、尺寸守卫
（2106 Rust 文件、39 项既有超限）和 diff 检查通过。未运行 Cargo/build/test/Clippy；
单项新回归在两个排列上的断言尚未动态验证。正式验收仍须精确候选头的 frontend `--lib`
及既有双宿主门禁；静态可构造不等于实际执行通过，D10/E2 仍 in-progress。

## 97. 闭合循环体 owner 测试模型候选（2026-11-04）

基线 main `d51c2cc7`。本片只增加私有测试模型与真实循环拒绝回归，不依赖尚未合入的
control-tail/生产 Choice 变更，不修改公开 owner 产物、准入、cleanup 或 Single witness。

- `owner_transport_tests::loop_closed_model` 通过真实直线 Unit producer 取得 String 的
  Result/Binding/Move/Pending/Replace/Drop 序列，模型将该闭合序列作为独立迭代模板。
  它不是从真实 loop 导出的事件，也不是 CFG/phi。仅接纳无参数 owner、无分支/capture、
  且全部定义均经真实 Drop 结束的空入/出接口；禁止将 Result 视作隐含析构或返回交付。
- 静态定义 token 不改；动态实例使用本次模型回放的轮次与定义共同区分。每次回边前
  所有位置必须 Dropped，下一轮从新实例开始；Replace 先验证两输入再原子提交，不能
  以旧轮实例满足本轮 drop 或 replacement。模型不支持跨轮 owner 或外部 live owner。
- 0/1/2/7 仅为有限前缀回放长度，不证明源码终止、条件取值或正常耗尽。零轮输出没有
  创建/析构事件，但仍静态检查模板合法性；静态检查的临时 trace 不计入动态结果。
- 七项模型测试核当前 producer 序列、AST literal 身份、新旧 replace 定义、逐轮守恒，
  并拒绝旧轮实例、缺 Drop、重复构造序列、缺 Pending、replace 新旧互换、外来 token、
  未清理 Result 与 parameter/capture/If catalog。另显式记录 return 无 terminator 事件时，
  闭合运输仍可回放的局限：不能从此模型证明真实回边可达，不能拿它作生产循环准入门。
  模型的守恒检查不代替生产合法性或 continuation 证据。
- 真实循环拒绝矩阵追加 return 与修改外层 root 两源；复用已有精确首个 Unsupported、
  全前缀回滚、sibling 隔离、loop-local drop 和完整 planner 开关 Analysis 比较。
  while/loop/for、跳转与跨轮 phi 仍不开放。
- 本轮不运行本地 Cargo/build/test/Clippy，不推送或合并。静态检查结果单独记录，
  Rust 断言尚未动态验证；D10/E2 与完整循环 owner 运输仍 in-progress。

本候选作者静态检查：直接 rustfmt、文档结构检查（838 Markdown）及 `git diff --check`
通过；Rust 尺寸守卫保留 39 项既有超限，无生产文件增长。独立审查与精确头官方 CI
尚待完成，不能把模型断言或静态检查计为已运行 Rust 验收。

## 98. 循环 continuation/terminator 私有模型候选（2026-11-04）

基线 main `d51c2cc7`；独立于闭合循环体模型和未合入的 control-tail/Choice 候选。
本片仅新增 tests-only `loop_continuation_model`，不修改生产 CFG、API、cleanup 或准入。

### 已确认的前置缺口

- Unit drop planner 的 bool continuation 把 return/break/continue 都折叠为 false；
  checker 的 `Flows` 分别汇总 next/breaks/continues，但不公开逐前驱 owner snapshot。
  现有 liveness 的 header 固定点是 symbol 集合，不是 owner 入边证明。
- 当前 owner observation 没有显式 Return terminator。事件序列中的 owner 已全部 Drop
  不代表控制仍可继续；不能凭有限前缀、AST 形状或现有 `is_consistent()` 建立真实回边。
- 新模型使用真实已检查源码的 source-qualified loop/jump 身份，拓扑和待清理快照由测试
  独立声明。它是合同设计实验，不是生产导出的 CFG，也不能证明清理深度对应真实 owner。

### 候选合同与隔离测试

- 显式区分 Entry、while 条件真/假、无限 loop 无条件 body 边、body fallthrough、
  nearest-loop continue/break 与 return；每条 witness 持有 product/callable/index 身份，
  所需 witness 必须各出现一次。条件假才到 while 正常出口；无限 loop 无伪造零轮出口。
- 每条边恰有一个最后的 Transfer。fallthrough/continue 到 header，break 到最近 loop
  的 AfterLoop，return 直接到 FunctionExit。nested jump 保留外层 loop 的义务；return
  通过真实 AST Return.value 绑定 payload 节点，不用 return 节点冒充值或手写 bool。
  有 payload 时要求该节点 PayloadCompleted→Deliver，再按快照逆序清理全部义务；
  无 payload 不虚构完成或交付动作。完成标记是模型期望，不是真实 producer 运行证据。
- 清理编号只表示模型 obligation，不替代 owner identity；重复 obligation、错误父 loop、
  错误 stack、错 source/product/callable、缺边/重复边、提前 Transfer、return 晚交付与
  虚构 payload 均属负控；另拒绝缺完成、交付早于完成、把 return 节点当 payload、
  错完成节点和借用另一 return 的 payload。跨 source 负控取同一次 Fixture 内另一
  文件的真实 SourceUnitId 并保留原节点下标，要求拒绝；不是在两个独立 fixture 猜 ID。
  真实源码只核首个 normal while 的精确 Unsupported 点和无前缀，后续构造不据此
  声称获得独立拒绝证据，它们仅提供模型所用的源码身份。
- 零轮测试仅检查 Entry→Header→ConditionFalse→AfterLoop 的可能路径，不计算 flag，
  不宣称源码实际取该路径或正常终止。无限 loop 的 body 边不等于存在正常 exit。

### 后续生产接入条件（本片未实现）

实际 producer 必须在现有求值转移处发布 source-qualified edge 身份、所属 callable、
真实 active-loop 栈和 pending cleanup 快照。return operand 成功交付后才允许其 return
terminator；operand 自己终止时只记录内层退出，不伪造外层完成。跳转 scope unwind 完成
后保存尚存 owner，再发布唯一 successor；条件求值的失败/终止不得添加 true/false 后继。
header/exit phi 只消费真实可达前驱，不能把手工 witness 清单、合并 Flows 或静态 liveness
当作完整 predecessor 清单。接入后必须补真实 producer 缺边/错边变异，以及 observation
开关下全部旧 Analysis 的身份和顺序相等，才可讨论准入扩展。

本轮不运行本地 Cargo/build/test/Clippy，不推送或合并；模型断言尚未动态验证。
静态检查与独立审查分别记录，D10/E2、真实 continuation 和跨轮 phi 仍 in-progress。

作者静态检查：直接 rustfmt --check、文档结构检查（838 Markdown）、Rust 尺寸守卫
（39 项既有超限）和 cached diff-check 通过。八项 Rust 模型测试尚未执行；独立审查与
精确头官方 CI 待完成，不把本记录升级为生产 CFG 或生命周期验收。

### 两模型在当前主干上的组合检查（2026-11-04）

组合基线为 main `d7021580`，保留上述两份模型的原断言与各自边界，不扩展生产准入。
共同模块注册包含两模型，真实循环拒绝矩阵保留原项并增加 return / 外层 root 替换两源。
独立静审未见阻断；补强 incoming 旧轮负控，从全新双轮 trace 只替换 incoming，
避免 previous 已损坏时遮蔽 incoming 检查的缺失，原双旧轮用例保留。
直接 Rust 1.96.0 rustfmt 检查、文档结构检查（838 Markdown）、完整 Rust 尺寸守卫
（2099 文件、39 项既有超限）与 diff 检查通过；未运行 Cargo/build/test/Clippy。
精确组合的正式验收应覆盖两个模型的 7 + 8 项测试及 `loop_boundaries` 两项测试，
再由既有双宿主 `check_core.sh` 中的 frontend `--lib` 验证完整共享调用方。
这些选择均尚未动态执行；原候选的有限回放不升级为真实循环、终止或 CFG 证明。

## 99. Choice / Capture 与 loop 候选的静态组合（2026-11-04）

基于 §96 精确头 `44f9f4cb`，合入两份既有 loop 私有模型、两个真实循环拒绝输入与
模块注册；模型源码字节保留。原 SPEC §87–96 完整保留，loop 原无编号正文仅编号为
§97–98，历史基线与验收限制不变。没有生产代码、API 或循环准入变更。

实际循环拒绝矩阵另加一条 `value_observation_v1().is_none()` 断言，覆盖其全部十份
源码。旧 Unsupported / 空定义事件表及完整 Analysis 相等不含 sealed V1 能力，不能
替代这条断言；现行单 val Choice 准入静态上排除循环，因此这是边界补强而非已复现
生产漏洞修复。原精确拒绝点、Syntax、sibling、loop-local cleanup、污染图负控及
observation 开关差分均保留，没有追加新源码或扩大模型。

组合静态检查通过：固定 Rust 1.96.0 直接 rustfmt、文档结构（838 Markdown）、
完整尺寸守卫（2108 Rust 文件、39 项既有超限）和 diff 检查。未运行 Cargo/build/test/
Clippy；新增 V1 断言、15 项私有模型测试及真实循环边界尚未动态验证。后续须锁定
精确组合头，运行相关 frontend `--lib` 与既有双宿主官方 CI，不能复用分支静态检查
冒充组合行为验收。生产 loop owner、phi、可达回边及终止性不在本片证明范围。

## 100. Capture 组合的真实 producer 回归候选（2026-11-05）

基于 §99 精确头 `be7324b2`，仅补顶层 leaf capture 与显式 Unit If 的三项源码回归，
不修改生产准入、validator 合同或旧 cleanup；不含 branch-local formation、循环 owner、
同 callable Choice × Capture、Mac 消费者迁移或 E3。

- 两个环境各自捕获一个 owned String，两边均使用两个 callback，避免 ASAP 在 fork 前
  释放。按独立 AST lambda/parameter origin 查唯一 token，核两个环境身份与每路径 slot
  恰一次释放、slot 先于环境及真实旧 cleanup 顺序。已使用路径的释放点由独立 AST
  局部声明固定为 AfterStatement，未使用内层边固定为 inner If 的 BranchExit；先核
  slot/environment point，再核旧 cleanup，不以两份被测产物互相印证替代独立期望。
- 单环境捕获两个 owned String；源码首次引用顺序刻意与参数声明顺序相反，先固定
  checked capture 原序 `second → first`，再核每边真实 producer/cleanup 的逆序。
  不要求 validator 拒绝仅交换合法 slot 的 trace，不扩大其现有守恒合同。
- 嵌套 If 的未使用边保留真实 BranchExit 释放；先核三条完整路径，再只将该释放的
  control 改为祖先 If，保留原 branch、slot、environment、owner 与事件位置，要求拒绝。

三份源均加入现有 observation on/off 完整旧 Analysis 差分矩阵。测试位于
`owner_transport_tests::capture_composition`；配套差分仍为
`combined_slices_and_mixed_rejection_preserve_complete_legacy_analysis`。
作者静态验收通过：固定 Rust 1.96.0 直接 rustfmt、文档结构（838 Markdown）、
完整 Rust 尺寸（2109 文件、39 项既有超限）和 diff 门禁。按本轮限制未运行 Cargo、build、test、Clippy 或 rustc；三项新测试
及扩展差分均未动态验证，不把静态准入核对升级为行为通过。独立审查、精确头官方
CI 与 D10/E2 完整验收仍待完成，本片不推送、发布、PR 或合并。


## 101. Branch-local Choice 生产代码静态候选（2026-11-05）

基于 §100 精确头 `e83366c8e440ad654bb5f44514d4d1e8507755da`，按用户“主线ci先跳过，
先继续实施”编写未发布候选。§89 的原 Binding/Drop 单切片与逐边官方 CI 前置验收
依赖和历史结论完整保留；本节不将先编码等同于已动态验收的生产准入，不声明 D10/E2
完成，不修改 Guide、公共 value 类型或 V1 validator。

- 外部仍是普通顶层非泛型 Unit 函数、一个无模式 marker 的 Boolean 参数、唯一
  不可变精确 String/MoveOnly `chosen` 绑定及以该参数为条件的显式 If。每边分别允许
  原 literal/Group 尾，或恰一条不可变精确 String/MoveOnly `val`，initializer 只允许
  无插值 literal/Group，随后唯一消费该局部的 Name/Group 尾。保留双 literal，支持
  双 local 及两个方向的 literal/local 混搭；同拼写不同 symbol 的局部分别运输。
- 独立 producer 事务新增逐边 initializer frame、tail frame 和单项 symbol→value
  侧状态。实际 String 完成登记 definition；每层 initializer 完成后，真实 state.insert
  返回才发布 Definition Bind 并清空 initializer 结果。旧 bound_owner 的旧 Binding
  位置不变，仅抽出 Choice hook；chosen 的 Choice Bind 也使用同一 post-insert hook。
- moved_owner 直接传入真实 take_named 返回的 Some(symbol)，不依赖旧 owner 是否
  Some；失败 take 不发布 Move 并清空事务。成功 Move 从对应边局部槽取走 token，
  清空该槽后交付 Name 结果；Name 与逐层 Group 仍需实际 Consume 完成。edge 出口
  核完整 tail、独立完成计数和零局部残留，双边与 control 完成后才允许外部 Bind/Drop。
  token 不从旧 owner=None、最后事件或 cleanup 重建。旧诊断、cleanup、Unsupported
  空 catalog、product/callable 隔离和 Single 见证均保留。
- 不扩展 var、nullable、常量、额外局部或语句、Return/Abort、When、loop、capture、
  replace、外部 live owner、后续使用、嵌套 Choice、Phase 4/native 或 E3。

原 `choice_boundaries` 双 local 真实 body adapter 的 Binding/Move/Group、零重复局部
清理、scope 恢复与完整 Analysis 差分保留，整体断言升级为独立 V1 能力。原 mixed
拒绝源转入实际正例，不删除其差分。新增 `choice_branch_local` 从 AST 声明/表达式链
独立固定逐边 Create/Forward/Bind/Move 顺序、tail 和 chosen 的精确 AfterStatement，
并检查总 cleanup 只有 chosen/sibling、sibling 隔离及完整 Analysis observation on/off
差分。真实源码包含双边分组 initializer/tail、两向混搭和同名不同 symbol；边界矩阵
继续拒绝 mutable/nullable/额外局部/非 literal/未消费 local/后续语句/嵌套 If/Return。
22 条手工事务 trace（1 条正常、21 条负控）核遗漏、提前、重复、Read、失败 take、
错 symbol/initializer/tail、跨边 Move、局部残留、非继续和错误清理；手工 trace 不代替
真实执行。mutable 拒绝源同时覆盖左、右边，不因边顺序放宽合同。

本轮仅准许运行固定 Rust 1.96.0 直接 rustfmt --check、Python 文档/尺寸与 diff 检查。
Cargo、build、test、Clippy、rustc、本地编译和官方 CI 均未运行；不 push、不创建 PR、
不发布或合并。后续动态验收应覆盖 `owner_transport::tests::choice_admission`、
`choice_boundaries`、`choice_branch_local`，再按共享 hooks 影响验证原直线/capture/If
调用方及双宿主官方精确头 CI。静态结果与独立审查记录在本节后续账本，不替代这些门禁。


静态账本：固定 Rust 1.96.0 对全部七个改动 Rust 文件直接 rustfmt --check 通过（含新增
测试文件）；文档结构检查 838 Markdown 通过；相对上述基线的完整尺寸守卫 2110 Rust
文件、39 项既有超限通过，未修改尺寸政策或扩大旧大文件；git diff --check 通过。
独立静审未发现剩余 P0/P1 或明确类型/借用问题。以上不构成 Rust 编译、22 条 trace、
真实源码断言或完整 Analysis 差分的动态通过证据，全部仍待授权后的精确头验收。

## 102. Unit 多层 Index 的 temporary backing owner 修复候选（2026-11-05）

独立基于 main 对象 `d7021580f0aee33391acab70c43651dc459dc988`，不叠加未合入的
Choice 候选或依赖其协议。保留本基线既有 Spec 全文；本节编号预留给并行切片，未来合批
时协调新增节编号，不能用本节覆盖其他候选的历史记录。

- 静态调用链发现：合法嵌套临时容器的末级 Index 及其中间 Group 都是 Place，既有
  Unit helper 只查询直接 receiver 的 temporary 类别，导致 Borrow 找不到 backing
  而进入 `InvalidUnitArgumentPlace`。这是源码推导，未动态复现。
- 最小生产改动仅在直接 temporary 查询未命中时递归 receiver 的 element owner；每层
  仍经 typed descriptor、Group 透传与 source-unit guard，不改公开 place、loan、drop
  模型或 Single witness，不改 Choice、E3/native、Guide、CI 或测试 target。
- 新增 `ownership_containers::nested_temporary_unit` 五项真实源码回归，复用原
  `mutable_unit::check_each` 的 MemoryDocument/Rooted、parse/name/type 检查、final type
  admission、typed identity 与 owned validate 前置，不改共享 helper 或迁移旧 Single 测试。
  覆盖 Int 普通复制/Value、String Borrow、Array/MutableList Inout、普通 owned read/Value
  的 L0136 及最外层 owner/参数标签、后级 index 的 conditional return。
- 期望从源码 AST 的唯一 Call/Index/Group/Return 身份独立构造，硬比 source-qualified
  loan/delivery target、完整 backing drop 表及 origin：Copy 在末级 Index 的
  AfterExpression，Borrow/Inout 在 CallReturn，后级 return 在 ControlTransfer。
  不允许中间 Index/Group、内层构造或重复 owner drop；Unit target 不冒充 Single
  CleanupOwnerId，也不据 loan 的 end span 推断完整 loan-end 流。
- 本片未涉及根构造外 Group 的 owner 归一、具名多索引 place、Index 后字段投影或
  provider/全域 cleanup。既有 Single 的多索引 owner-table/cleanup witness 原样保留。

验收仅限静态：三份改动 Rust 的直接 rustfmt --check、文档结构检查（838 Markdown）、
明确基线的 Rust 尺寸守卫（2098 文件、39 项既有超限，无新增债）和 `git diff --check`
通过，独立静审未发现剩余 P0/P1。本地 Cargo/build/test/Clippy/rustc
及官方 CI 均未运行，新增源码前置和断言尚待动态验收。未 push、PR、发布或远端合并；
D10/SPEC-0341/E2 继续 in-progress，不以历史 main CI 代替本候选精确头验证。

## 103. Unit 具名多层 Index place 完整路径（2026-11-05）

独立基线为 main `d7021580f0aee33391acab70c43651dc459dc988`；本节编号与并行候选协调，
不代表缺席的 §87–102 已合入，也不覆盖或改写基线全文及 §58 的历史差异记录。
本片按 §1–3 的既有能力保全目标补齐 P3，不引入语言决定，不吸纳其他未验候选。

- `UnitOwnershipPlace` 的完整身份与 overlap 职责移入 `compilation_unit/place.rs`，保持原
  public 导出路径及内部 `ownership_checking` 可见域。root、fields 与 const root/element/
  is_root 查询保持；新增只读 `elements()`，`element()` 仍给末层，不以末层替代完整身份。
- named Index builder 沿原 typed descriptor 核对 receiver/index 来源后递归追加路径；
  overlap 与 Single 一致，逐层 `may_alias`，短路径保留 parent-prefix 重叠；不同已知前缀
  即使末项相同也可分离，同前缀不同已知末项亦可分离。字段前缀及索引后字段拒绝不变。
- `canonical_borrow_place` 仍返回原 origin 整 lease；不改 temporary owner/Group 归一、
  cleanup、类型规则、Choice、refine/E3 或 codegen 生产路径。原 Single multi-index、
  one-index 与 field-prefix 测试及共享 `mutable_unit::check_each` 原样保留。
- 真 Unit 回归复用 MemoryDocument/Rooted 与 final type gate：Copy Value、Shared、Inout
  的 source-qualified 根与索引顺序由原 AST 参数和固定路径校验；known sibling、same、
  两层 Unknown、ancestor 与借用期间移动根覆盖正反例，错误比较完整 L0135 Diagnostic；nested MoveOnly Value 保留 L0136 及原根 label。
  Copy 正例另核具名 receiver Group 透明性；成功 cleanup 仅归原 named owner，不生成嵌套 temporary owner。
- 单项既有 codegen harness 回归先检查真实 P3 与 final gate，再固定 nested Borrow 的
  `UnsupportedNode` 及内层 receiver `xs[0]` Span。`container_owner_operand` 的 expected-root
  Index 拒绝、MoveOnly 中间容器 read、refine 非 Name owner 与 loop 非 root 门禁均保留；
  这不是 native 成功证明，不将旧能力缺口改为语言拒绝。

| 验收项 | 目标 | 本片实际状态 |
|---|---|---|
| 路径、逐层 alias、完整诊断与 cleanup | `ownership_containers::named_index_places` 五测试、八源码，两种 Unit 输入 | 新增断言未运行 |
| P3/P4 边界 | `named_nested_element_borrow_passes_p3_but_keeps_the_p4_receiver_boundary` | 新增断言未运行 |
| 生产模型、可见度、const 与消费者 | 独立只读静审 | 未发现阻断问题；不代替 Rust 编译 |
| 格式、文档、尺寸与 diff | 固定 Rust 1.96.0 rustfmt `--check`、Python docs/尺寸、diff check | 通过：838 Markdown、2099 Rust 文件；39 项历史超限未扩大 |

按用户本次“主线ci先跳过，先继续实施”范围，本地 Cargo/build/test/Clippy/rustc、官方 CI、
push/PR/发布及远端合并均未执行。旧 `compilation_unit.rs` 为 1156 行，本片降至 1082 行，
仍属历史超限；完整 place 模块为 90 行，无机械切片或新增尺寸例外。动态验收仍待后续授权，
D10/E2 和本 Spec 保持 in-progress，不能将静态实现或旧 CI 计为本精确头通过。

## 104. Unit 两类多层 Index 切片静态组合（2026-11-05）

本地组合仅纳入 §102 的 `d510680c5b88245ec127c551ca8d78e317e36087` 与 §103 的
`2754f04b5c0c82f1c009c732e17381bce0038d42`，共同基线为
`d7021580f0aee33391acab70c43651dc459dc988`；双亲提交保留两个独立来源。
基线全文和 §102/§103 逐字保留；前两节的独立范围与验收记录不反向改写。

- production 仅联合原两片：temporary backing 的递归查询与 named place 的完整索引路径；
  named place 仍优先，temporary fallback 沿具名链终止于 Name，不创建具名根的临时 owner。
  不改 canonical origin lease、drop 算法、共享 helper、Single witness 或 P4 生产边界。
- 精确保全两份新 frontend 模块及其注册、原有全部测试、完整 place 模块与 P3/P4 拒绝回归；
  architecture 合流两项实现说明，没有额外生产修复或新增测试。
- 静态格式通过：固定 Rust 1.96.0 直接 rustfmt `--edition 2024 --check --config
  skip_children=true` 检查相对共同基线的全部七份 Rust 改动；文档结构检查通过（838
  Markdown），Rust 尺寸守卫通过（2100 文件、39 项历史超限），没有新增尺寸债。
  diff、逐字节保全和独立临时 index 补丁重建检查通过；不代替 Rust 编译或行为验收。

本轮仍未运行 Cargo/build/test/Clippy/rustc、本地构建或任何官方 CI；所有新增 Rust
断言与组合后实际行为均未动态验证。没有 push、PR、发布或远端合并，不吸纳
Choice/capture/loop、Mac 消费者或其他候选。nested native 仍不受支持，D10/E2 与
本 Spec 保持 in-progress，静态组合不等于最终能力验收。

## 105. 一层临时父字段 Borrow 的 pending 清理修复候选（2026-11-05）

独立基线 `d7021580f0aee33391acab70c43651dc459dc988`。本节保留基线正文完整，
§87–104 为并行工作预留；这些节在本树缺席不代表已合入。本片只补 Phase 3 已准入
字段实参的清理登记，不吸纳其他 Choice/capture/index 候选，不改 Phase 4 或 Group 准入。

- 静态缺口：Unit 对 `read(Holder("field").text, 0)` 已产生 Shared loan，target 为
  temporary Holder parent；但原 pending 登记只识别 element backing 或实参自身的
  MoveOnly temporary。字段为 Place，两者均不命中。此为源码静审结论，未执行泄漏复现。
- 将原 Borrow temporary 登记职责迁至 `drop_planner/pending_call.rs`，保留原两分支
  顺序及行为；只在均未命中时追加 field-only fallback。要求 SharedLoan、精确 argument
  的 typed aggregate projection 为 Field/Expression，排除 borrow-result call 与 range use；
  同 source-qualified call/argument 的实际 Shared loan 必须指向 Temporary，且 target
  与 projection receiver 完整相等、来源属于本 planner、父通过既有 MoveOnly temporary
  判断。父正常求值后才登记，不按字段类型判断析构，也不按 AST 名字猜 owner。
- 只接一层直接临时父，不递归追踪 Rc.value、nested field/element parent 链，不接
  Inout、Value 或 receiver；不加通用 Temporary loan fallback，避免重复 range continuation
  已改挂到外层 call 的 pending owner。`temporary_projection_owner`、
  `temporary_expression_origin` 逐字保留，既有 constructor Group 的父身份和 P4 alias
  补偿不变；本片不声称全部投影 cleanup 闭合。
- 正常 CallReturn、逆序完成、loop depth、提前 return/break/continue 与 Abort 不展开
  全部复用原 pending 算法。`loan.end_span` 仅为源码元数据；`direct_call_loan_ends` 仍
  排除字段，不能用其空表证明字段 loan-end 顺序或声称完整 loan-end/native 闭合。
  原真实 temporary-field Unit 准入/Single MemberReceiver deferred
  witness、pending lifetime、range continuation 及 direct-call 原 oracle 全部保留。
- 新 `unit_direct_call_loan_ends/temporary_field_pending.rs` 复用原 Rooted/MemoryDocument
  双 Unit helper、完整类型 final gate 与 ownership validate。六个测试函数包含有限真实
  源：普通/Group 父、MoveOnly 父的 Copyable 字段、两父逆序、后续 return/break/continue、
  内层 loop 跳转保留外层父、后续 Abort、父构造自身 Abort、Copyable value-class 父。
  期望由独立源码/AST 确定，核 source-qualified call/argument/parent、实际 loan target、
  完整 drop 表的 point/target/value_origin/顺序及其他 drop/观测空表；不由产物生成 oracle。

| 验收项 | 目标 | 本候选状态 |
|---|---|---|
| 字段父 pending 与退出清理 | 新模块六项测试及原 `unit_direct_call_loan_ends` | 全部未运行；尚无动态失败或通过证据 |
| 原 pending/range 邻控 | 原 `multifile_ownership_checking/pending_lifetime`、`ownership_range_construction` 等 | 逐字保全，未运行 |
| 格式与静态结构 | 固定 Rust 1.96.0 rustfmt `--check --config skip_children=true`、Python docs/size、diff/保全 | 静态通过：838 Markdown、2098 Rust，39 项历史超限未扩大 |
| 编译、Clippy、native 与 CI | 受影响 frontend/下游及精确头门禁 | 按用户约束未运行，不触发 CI、不推送或建 PR |

独立静审未发现阻断；原生产分支、pending 完成算法、Single witness 与基线 Spec 正文保全。
D10/SPEC-0341/E2 继续 in-progress；静态修复及测试接线不代表动态资源证据、native 支持
或精确头验收完成。

## 106. Unit Index 与临时父字段候选的静态组合（2026-11-05）

本地组合的第一父为 §104 的 `75b78d750b434d50102e7d30a199625885ac9697`，第二父为
§105 的 `ebf7aa374cb926452f24ff0d8b905254fcf6e68e`，共同基线为
`d7021580f0aee33391acab70c43651dc459dc988`。基线全文与 §102/§103/§104/§105
原文依序逐字保留；各节对独立切片的历史范围和验收状态不反向改写。

- 本片仅组合既有两候选，不新增生产行为、测试或能力准入；生产和测试文件各自完整
  沿用对应父提交。architecture 同时保留多层 Index 与一层临时父字段的现行事实。
- named place 仍先于 temporary fallback；临时链仍按 element backing、直接 temporary、
  精确一层 Field 的顺序选择。Field fallback 保留 source-qualified call/argument、Shared
  loan 的真实 owner 等于直接 typed receiver 及 MoveOnly guards，不泛化为任意 Temporary
  loan，也不为 range continuation 重复登记 pending；既有 pending 完成与退出算法不改。
- Index 后 Field 的 liveness deferred、整函数 skipped gate、ownership validate 挡板
  及 `push_field` 拒绝原样保留。恢复路径可查得 loan origin，不代表可发布 validated P3。
  nested native 仍拒绝；provider、完整 loan-end/cleanup 与新投影形状不在本片范围。
- 原有注册与 helper 全部保留，三个新增 frontend 模块合计 16 个测试函数，以及一项
  codegen P3/P4 边界测试均来自两父，17 项全部未运行。本轮不添加新的 oracle 或源码例。

静态验收：固定 Rust 1.96.0 直接 rustfmt `--edition 2024 --check --config
skip_children=true` 覆盖相对共同基线的全部 11 份 Rust 改动并通过；文档结构检查通过
（838 Markdown），明确基线的 Rust 尺寸守卫通过（2101 文件、39 项历史超限，无新增债）。
完整 diff、20 组保全检查与补丁反向适用通过；独立临时 index 从共同基线加完整补丁可
重建精确组合 tree。原五个源/main 工作树身份与干净状态不变；这些不替代编译和动态验收。

未运行 Cargo/build/test/Clippy/rustc、本地构建、native 或任何官方 CI；未 push、PR、发布
或远端合并。不吸纳 #312/#313、Choice/capture/loop、Mac 或其他独立候选。D10/E2 与
本 Spec 保持 in-progress；本片只是待验输入收敛，不是能力完成或精确头动态通过证明。

## 107. 已知 Choice CI 修复向后继候选的最小传播（2026-11-05）

本片基于 §101 精确头 `b50a97c42077d350759a52d55dad8e35aeed2951`，其语义主线
基底为 `d7021580f0aee33391acab70c43651dc459dc988`。只迁移已在
[PR #312](https://github.com/Halckon/Koven/pull/312) 发布头
`5091317c155879ab1100e532def3dee67341140f` 验证的两个已知修复，不扩展能力。
本地补丁来源分别为 `aac480c528380e07ae9fe6b0a507d05b5c31fc65`（Eq）与
`637943a6ddcc8966f3bc41538dac4be985dc610a`（CallReturn）；它们是内容来源，
不是本候选的动态验收身份。§1–101 及全部历史结论按原字节保留，不复制 #312 账本。

- `UnitOwnerObservation` 使用显式 `impl Eq` 配合原自定义 `PartialEq`，不再要求独立
  sealed V1 能力实现结构相等；保留公开 Eq 契约、编译见证以及移除 capability 后旧
  observation 仍相等而 V1 不再可用的 oracle。`PartialEq` 的旧事实比较不变。
- V1 直线投影只在无 Choice 时接纳 CallReturn：立即前一条已成功回放的投影事件须为
  同 call、同 root、同 resolved incoming owner 的 Replace；保留 source、product 与
  slot 校验。完整回放以空前文开始，legacy 单事件 adapter 传入已提交前缀的最后事件，
  不搜索任意历史或未来 Replace。Choice 的 CallReturn、BranchExit 与退出边界不放宽。
- 携带 nested replace 两个实际 CallReturn 的独立 AST/literal 身份 oracle，及错 call、
  root、owner、非紧邻、缺 Replace 和 Choice 邻接 Replace 的负控。后继 Choice 准入
  AST/drop 断言、branch-local 正例及全部 Capture/branch-local 实现与 oracle 完整保留。
  Capture 不进入 String V1，loop 生产准入仍关闭，旧 cleanup 与 Single witness 不变。

本片只改两个生产文件、四个回归文件并追加本节；Architecture 现有旧事实相等与闭合
直线投影描述仍适用，无需扩展。静态检查与保全结果见下方账本。本轮不运行 Cargo、
build、test、Clippy、rustc 或 native，不触发 CI，不更改 #312/#313、push、发布或合并。
#312 的绿灯不覆盖本后继候选。Eq 编译见证、CallReturn 回归与 Capture/branch-local
共享调用方均尚未在本候选动态执行；后续仍需精确头官方 CI，D10/E2 保持 in-progress。

静态账本：固定 Rust 1.96.0 工具链的直接 rustfmt --check 覆盖相对语义主线基底的
全部 28 个变动 Rust 文件并通过；文档结构检查 838 Markdown、完整 Rust 尺寸守卫
2110 文件（39 项既有超限）及相对父提交/主线基底的 diff 检查通过。五个迁移文件与
CallReturn 来源提交的最终字节相同；`choice_admission` 只添加四行 Eq oracle，原
自定义 PartialEq、SPEC 全文及其余 tracked 文件逐字节保全。未修改尺寸政策或 CI。
这些检查不证明 Rust 编译、借用检查、断言执行或 Capture/branch-local 组合动态通过。
独立静审核对两项修复、五处 replay 调用、独立 cleanup oracle 与负控，以及后继
Capture/branch-local 保全，未发现新增阻断；此结论同样不替代精确候选动态验收。

## 108. Choice API 编译见证的实际测试接线（2026-11-05）

在 [PR #314](https://github.com/Halckon/Koven/pull/314) 的后继候选中，仅向既有
`check_stage_integration.sh` 前端测试列表加入 `ownership_value_api`，不移除原目标、
不改变失败传播或双平台门禁；实数现有组合并纠正文档陈旧总数为 132 个前端 integration
target、23 次 Cargo 调用（此前实际已有 131 个，本片仅新增 1 个）。

此前 workspace check/Clippy 的 all-targets 已包含该文件的类型检查，但既有 test 脚本
未执行此 integration target。唯一测试 `value_api_is_nameable_and_matchable_without_private_modules`
是公开 API 与 Eq 约束的编译见证，不替代真实 Choice 行为回归。后续验收须在双端官方
日志中确认完整测试名及 1 passed、0 failed、0 ignored、0 filtered out，拒绝零命中。
本次仅完成接线，尚未取得新头的执行结果；记录真实 head、base、merge SHA/tree 后再判定。

首轮 [CI 41989629757](https://github.com/Halckon/Koven/actions/runs/41989629757)
在两个宿主均报 E0433：`choice_admission`、`choice_branch_local` 与
`choice_capture_isolation` 的测试使用 `UnitStatementId`，但未导入该类型。
本片仅在三个测试模块补充既有公开类型的导入，保留全部 fixture、断言与生产代码。
首轮未进入动态测试，不能当作通过；补验接线与 import 修复一并等待新头的官方 CI。

接线后的 [CI 41990424491](https://github.com/Halckon/Koven/actions/runs/41990424491)
在组合策略的精确目标计数处失败：排除两个按单测过滤的 target 后，计数由 129 增为 130。
同步该固定期望并显式锁定 `ownership_value_api` 恰好出现一次，保留唯一性、23 次 Cargo
调用及全部失败传播断言；不通过删除或放宽门禁解决。该轮未开始 Rust 编译或测试。

## 109. Container access 与 move diagnostics 消费者迁移（2026-11-04）

基于 main `d7021580`，合并两个已独立静审候选，仅迁移四项测试、五次原 Single
源分析；每份原源码与原环境分别通过 MemoryDocument、Rooted 真实 Unit 入口。
不改生产、公开 API、helper、Guide、CI、target，不涉及 resource、Mac receiver、
循环或 Choice 生产工作。

| 验收项 | 测试目标 / 过滤器 | 实际结果 |
|---|---|---|
| Copyable / MoveOnly element 读取 | `ownership_containers::element_owned_read_copies_copyable_and_rejects_move_only` | 两源；保留正例无诊断，负例精确 L0136×2、primary span 的 list[ 前缀与每条 Label；静态核对，动态待 CI |
| 三容器 borrow/inout | `ownership_containers::borrow_and_inout_cover_each_intrinsic_container_capability` | 保留无诊断、五条 loan 与 Shared×3/Exclusive×2 完整顺序；静态核对，动态待 CI |
| 遮蔽符号 move 来源 | `ownership_checking::shadowed_symbols_keep_independent_move_origins` | 保留两个 L0131 顺序、每个主 Span 与 Label 精确字节边界和原源片段；静态核对，动态待 CI |
| 非拥有 move 与不可变 inout | `ownership_checking::non_owning_move_and_immutable_inout_have_dedicated_diagnostics` | 保留 L0133/L0133/L0134/L0134/L0131 顺序、主 Span 集合及每项 Label；静态核对，动态待 CI |

容器批次复用既有 `mutable_unit::check_each`，要求 final type admission、同轮身份；
正例无 deferred 且 ownership validate 成功，负例精确 code 序列且不能取得 owned
资格。新增前置必须动态通过，不放宽门禁换绿。move 批次复用既有
`borrow_rejection_unit::checked`，保留 parse 两次、原源码字节、names/types 无诊断、
基础 validate 与 typed/owned 身份；基础 validate 不等于 final admission。

两批独立静审未见 P0/P1；组合保持已审测试文件逐字节一致。原源码、测试名、全部
原 oracle 与候选外代码静态保全通过；直接 rustfmt、文档、尺寸、diff 检查通过。
未运行本地 Cargo/build/test/Clippy；Draft 发布后的精确头官方 CI 结果另行核验。
本批不删除旧 helper，不替代 owner、loan-end 或 cleanup graph 的独立完整证据；
D10/E2 继续 in-progress，发布不授权自动合入。

## 110. Map 类型诊断旧消费者退休（2026-11-05）

本片独立基于已合入五批后继 PR 的 main `c36c4e85449df69ff1f5fb553313a7ea2dbb2ead`。
承接 §65 明确保留的十项 Single-only 测试，不重复计入其已退休的十四次分析。

- 十项测试原位使用既有 `checked_entries`：同一 SourceMap/ParsedFile 与显式标准环境，
  MemoryDocument/Rooted 各自产生真实 Unit index、已验证 names 和 raw types；两入口
  均核 source identity；validate 前显式要求 names 诊断全空，保全旧前置。完整保留原源码、
  测试名和断言，只增加双入口循环，不增加或
  放宽 final/ownership 准入，也不改生产、Guide、target 注册或 CI。
- 构造/成员正例仍无诊断；两类非 Hashable key 各为 L0161×1；类型实参数量为 L0091×1；
  readonly mutation 保持 L0129/L0130/L0130 顺序；method 未调用为 L0130×1；六个
  key/value mismatch 全为 L0084；class index/get 与 String get 各为 L0136×1。
- 删除最后一个旧 `checked` helper 和对应 resolve_names/check_types/TypedFile 导入，
  保留 consume 所需 NameResolution。其余五项父文件测试、consume 两项与 parser helper
  逐字节保全；入口 helper 仅补上述名称全空前置。全目标仍十七项，退休十次 Single 执行，改为二十次 Unit
  分析；不是新增二十个测试或生产能力扩张。
- frontend integration tests 中直接 check_types 静态调用由 110 次/55 文件降至
  109 次/54 文件，check_ownership 保持 44 次/33 文件；两类旧 checker 消费文件并集
  由 55 降为 54。这是调用点盘点，不是 D10/E2 完成比例；旧 API 与独立差异见证仍保留。

云端固定 Rust 1.96.0、独立 Cargo target、locked/offline 验收：frontend lib 编译通过；
`cargo test -p lang-frontend --test type_map --no-run` 编译通过；同目标实际执行
17 passed、0 failed、0 ignored、0 filtered out；同目标 Clippy `-D warnings` 通过。
十项原源码与 oracle（仅循环缩进变化）、全部测试名、parser helper 和 consume 的机械保全检查通过。
直接 rustfmt、diff、文档结构（838 Markdown）及基于上述 main 的尺寸守卫通过
（2114 Rust 文件、39 项历史超限，无新增债）；独立只读复审未发现阻断或原 oracle 遗漏。
未运行故障注入、校准、含 Abort 的整项测试、Mac 构建、native 或全工作区门禁。
本地结果不替代精确头官方双平台 CI；本片未发布或创建 PR，D10/E2 与本 Spec 保持 in-progress。

## 111. Copyability 消费者退休与 Box signature 漏诊断修复（2026-11-05）

本片独立基于 main `e1cde85f907553053c1d6ef699ea4ddd81d0a18e`，不重复计入 §110 的 Map 退休。
先迁移原 `type_copyability` 八项，真实云端编译通过后运行得到 7 passed/1 failed：原 Box
测试要求 L0117×4、L0091×2，Unit signature 仅发布两条 L0091。保留失败证据及原 oracle，
不删除该测试或放宽诊断，按 Guide 11 已启用的 Box 实参合同补最小生产漏检。

- 原八份源码、测试名称、33 个 assertion 位置、自定义十六 builtin 与能力/可选 Box
  环境、重复解析和 names 诊断全空前置均保留。八次 Single 分析替换为十六次真实
  MemoryDocument/Rooted 分析；仍观察 raw types，不添加原测试的 final/ownership 前置。
- copyability、精确诊断顺序/数量/primary、Box 与源码同名 class 身份、解构 mode、分量
  类型/真实 symbol、原 AST statement 和 Error/Deferred 恢复全部保全。Unit ID 取实际
  declaration、parameter 或 body facts，并核 source/local identity，不新增公开构造 API。
- SignatureCollector 只在真实 intrinsic Box 且原 arity 合法时接入私有实参检查。
  已预声明 nominal kind 足以接受 value/enum；非法 kind 在实参 TypeRef span 报 L0117
  并返回 Error。原 L0091 路径、其他 intrinsic、Single/body 与构造入口不改。
- Box payload 的局部后序检查按实际参数语法与缓存传播失败，外层 Box 成为 Error。
  名义节点只重新 intern 含 Error 子实参的恢复形状，保留其他实参的独立 bound 检查；
  不修改原共享 interned 项。Interface 的 L0094/声明 label 仍由原后置 pass 发布。
  静态 bound/supertype 仅对真实外部 intrinsic Box 且 arity=1 停止标记 payload 为 static，
  同名源码 Box 与错误 arity 保持原路径；不增加全局状态/去重或遍历 nominal 字段/layout。
  内层 Box 与 Nullable(Box) 均复用已检查的 payload，不逐层重复遍历。
- 新增八项回归，覆盖字段/enum payload/参数/返回/function/nullable、arity 与嵌套根因、
  interface 在 runtime/静态 bound/supertype、多兄弟与自身独立 bound 中的恢复、泛型名义
  payload、nullable 嵌套，以及真实跨文件前向/递归 Box、输入倒序和 signatures-only/full
  Unit。独立 sibling 的 L0115/L0093 先在原 main 对照确认，再保全于修复；首版整体 poison
  的真实失败保留。八项新增回归不计旧消费者退休；目标现十六项，原八项成功标准不变。
- frontend integration 直接 check_types 从 109 次/54 文件降至 108 次/53 文件；
  check_ownership 仍 44 次/33 文件；旧 checker 消费文件并集 54→53。旧 API 与独立差异
  witness 保留；此盘点不是 D10/E2 完成比例，也不宣称关闭其他 signature recovery 差异。

验证在获准云端 Rust 1.96.0 执行，命令均使用 `--locked --offline -p lang-frontend`：

| 验收项 | 命令或选择 | 实际结果 |
|---|---|---|
| 生产与目标编译 | `cargo build --lib`；`cargo test --test type_copyability --no-run` | 通过 |
| 直接行为与邻接合同 | `type_copyability`、`type_map`、`multifile_type_capability_graph` 三完整 target | 16 + 17 + 4 项通过，均零过滤 |
| 安全精确邻居 | `multifile_type_checking` 中 `unit_intrinsic_box_and_rc_publish_complete_value_construction_facts`、`source_box_and_rc_names_never_gain_unit_intrinsic_identity`、`invalid_generic_body_type_refs_keep_existing_codes_and_recovery`，均完整模块名加 `--exact` | 各实际运行 1 项并通过 |
| 静态质量 | frontend `cargo clippy --all-targets -- -D warnings`、格式、docs、Rust 尺寸、diff | 通过；未执行 Clippy 编译出的其他测试 |
| 独立复审与保全 | 原八项/33 assertion/源码与环境；新增回归及三处生产文件 | 无阻断；旧 Spec 完整前缀保留 |

共实际运行 40 项允许范围测试。`signatures.rs` 1021→1025 行，仍在既有 1036 行例外内；
新职责位于 152 行私有模块，仅在父模块接线，未提高额度或新增尺寸债。首轮缺诊断、
首版 sibling 丢诊断、错误的新 span 预期及修正后的真实日志保留于仓库外证据，不改写为通过。
本片未运行故障注入、校准、含 Abort 的整项套件、Mac 本地构建或官方双平台 CI。
候选仅本地保存，未推送或创建 PR；D10/E2 与本 Spec 保持 in-progress。

### §111 官方 CI 的非法路由夹具对齐（2026-11-05）

首次发布为 [PR #319](https://github.com/Halckon/Koven/pull/319)，远端
`d43af0eaecfc89db3deec3c0b57d8617183443ec` 与本地 `fe10b0e5` 的完整树相同。
[官方 run 42043391556](https://github.com/Halckon/Koven/actions/runs/42043391556)
的双平台 Check/Clippy 通过，Targeted Tests 同因失败：旧 codegen
`functions_outside_the_subset_keep_the_per_family_path` 把 `Box<Int>?` 当合法输入，
在进入路由观察前因修正后的 L0117 无法取得 validated types。Ubuntu 为 2611 passed/1 failed，
macOS 为 2610 passed/1 failed/1 ignored；后续证据上传失败是前序中止后产物未生成，不据此重跑或放宽语义。

补片仅调整该测试文件及本账本：原非法源码、package、路径和标准环境原样移至独立
`non_nominal_box_payload_is_rejected_before_refinement`，要求 signature 恰一条 L0117、
primary 精确为 `Int` 且 raw types 无法 validate。其他八个负向 case 及原 `!routed` 断言、
全部正向/顺序/邻接路由测试逐字节保留；不让非法类型进入 lowering，也不撤销 Box kind 检查。

云端 Rust 1.96.0 / LLVM 21.1.8 下，frontend lib 与 codegen lib 测试目标真实编译通过；
定向 `ssa::refine_subset_tests::` 实跑九项全通过（2604 项被过滤），`type_copyability`
十六项全部通过（零过滤）。独立只读复审无阻断，窄范围清点未见第二个同类非法 Box fixture。
codegen all-targets Clippy（`-D warnings`）通过，测试文件现 440 行，没有新增尺寸债。
该补片尚未推送或由官方 CI 运行，首轮失败不改写为通过。


### 顺序容器 signature 的有界 L0125 修复（2026-11-06）

基于 main `c4ba670a76a567e1475deee47378fa791d25f419`，独立分支
`fix/spec-0341-container-signature-storage`，不叠加 #326–328 或修改 #323。
Phase 2 修复现行 Guide 12 已定义的存储限制，不改变语言语义、旧 Single witness 或 codegen。

- 原因：signature 的真实 external intrinsic 分支只检查 arity/Box，没有 body checker
  已有的直接不可存储元素诊断。新增五项回归先在主干生产实现运行，4 failed/1 passed，
  直接、嵌套、字段和返回签名缺 L0125；原混合 case 仅 L0094/L0091，缺两条 L0125。
- 仅在 arity 合法的 Array/List/MutableList 上检查元素为直接或 Nullable 包裹的
  Any/Nothing；在原元素 TypeRef span 报 L0125。真实环境身份判断，不按名字匹配，
  不递归 nominal 字段/类型实参，不把函数签名中的 Nothing 当作函数值不可存储。
  Rc/View/Map/Box、interface/layout/range 后置检查及原 Error/arity 恢复均不改变。
- 首版返回 Error 真实制造了不同非法 overload 的伪 L0097，独立审查指出后用新增回归
  复现。最终沿用现有 Map signature 的诊断策略：保留原 raw 形状和 intern 表，只增加
  错误诊断；typed validate 与实际 final admission 均拒绝且保留原诊断。没有改共享
  shapes 算法，不把 raw 产品存在当作成功准入，也不修改合法邻居或去重不同源码实例。
- 九项新增测试覆盖三容器×四种元素×外层可空性、字段/返回/函数参数与结果、内嵌容器、
  固定诊断顺序、合法函数值/泛型/源码同名 List、跨文件反向输入、arity、顶层/member/
  function overload、重复错误位置与合法邻居。单源均实际双入口，跨文件核源身份和
  完整 signature 确定性；原 type_containers 的两个 Single witness 和 #328 内容不改。
- 主签名文件只加 module 与调用两行，1025→1027，沿用既有增长例外；新私有 helper
  49行，测试模块231行，不提高尺寸额度。未宣称解决全部递归 nominal storage 或 native。

| 验收项 | 检查 / 命令 | 实际结果 |
|---|---|---|
| 失败先行 | 新增初始五项、首版 overload 回归 | 4红1绿；首版 overload 额外L0097真实失败，原日志保留 |
| 直接与邻控 | `multifile_type_signatures`、`type_containers`、`type_copyability`、`type_map` | 17 + 32 + 16 + 17 全通过，均零过滤 |
| 云端编译 | Rust1.96.0、默认debug=2、locked/offline frontend lib build 与测试编译 | 通过 |
| 静态质量 | frontend all-targets Clippy、fmt、docs、尺寸、diff | 全通过；839 Markdown、2120 Rust、39项历史超限，无新增债 |
| 独立复核 | raw shape、错误恢复、身份和scope | 无阻断；首版恢复副作用已修正并动态验证 |

本片未执行故障注入、Abort 整项、Mac 构建、native 或官方双平台 CI；未推送或开 PR。
D10/E2 保持 in-progress，不等待其他 PR 的 CI 才继续工作，也不自动合并。

## 112. Field/Group 临时父 Borrow 的准入与清理修复（2026-11-05）

独立基线 main `e1cde85f907553053c1d6ef699ea4ddd81d0a18e`。按 Guide 现行 Borrow temporary 与 Group 透明规则修复
Phase 3，不改变 Value/Inout、普通借用结果或其他投影族的资格。

- 真实失败回归先行：`read((Holder("field").text), 0)` 完整类型阶段通过后，ownership
  返回内部 `InvalidUnitArgumentPlace`；`read(Outer(Holder("field")).inner.text, 0)`
  ownership 准入且 loan 指向 Outer temporary，但完整 drop 表为空。固定 Rust 1.96.0
  实际编译并分别执行两项测试，得到上述失败，不以静态推测当作动态证据。
- 新 source-qualified 专用查询仅沿 typed Field 与 AST Group 追踪已求值 temporary
  父，每步检查 receiver source-unit，排除普通借用结果与 range continuation。
  普通 Shared argument 使用该查询；通用 projection/receiver/iteration 路径逐字保留。
  pending 清理仍必须与同一 call、原 argument、实际 Shared loan 的完整 Temporary
  target 匹配，并由父类型判断 MoveOnly；不从中间字段类型、语法名字或通用 loan
  fallback 猜 owner。构造 Group 的旧父身份保持不变。
- 正常返回、逆序清理、提前 return/break/continue、内层 loop 保留外父与构造未完成
  清理复用既有 pending 状态。Copyable 父无唯一义务；Inout 与 partial move 负例保持
  L0134/L0132 与原 primary 约定；后续非法实参清空已完成前缀的 executable facts。
  direct-call/exclusive-prefix 观测片仍排除字段，不以空表宣称完整 loan-end 闭合。
- `unit_direct_call_loan_ends/temporary_field_pending.rs` 新增七项实际源码回归，使用
  Rooted/MemoryDocument 双入口、完整 type final gate 与 ownership validate。预期
  call/argument/parent/point 由独立源码 AST 选取；核完整 drop 表与 value_origin 顺序。
  原一层测试只推广其 AST parent 检查以容纳 Field/Group 链，原源码与资源 oracle 保留。
- 下游只读核验发现独立 P4 缺口：既有 `lower_borrow_place` 把 Temporary loan target
  的完整 class parent 当作 String 参数 owner，未建立字段 loan，真实公开 native 探针的
  grouped 正常调用与 nested/return 均在 object emission 前报 `InvalidModel/MissingFact`。
  ADR-0032 明确不采用继续按家族扩充旧 `unit_lower`；本片遵守 §2 与 E3 分工，不改
  codegen/refine 路由。P4 实验不进入本候选，后续须沿既有 refine field/pending 合同独立推进，
  不能将本片的 frontend 资源 facts 验收当作 native 已修复。
- [#315 审查提示](https://koven-hq.slack.com/archives/C0CBTCC2X2R/p1793886950819489?thread_ts=1793885318.134569&cid=C0CBTCC2X2R)：
  `make().f` Shared 实参的 Abort 边仍缺 counted/native 动态证据，目前不是已确认缺陷。
  现行 Abort 不 unwind，不能用正常返回或 return/break/continue 的 deinit 顺序替代证明。
  后续专用 CI 应保留现有 Abort facts 整项，并在 E3 正式支持该族后，以独立 counted/native
  测试核对失败边的既定资源合同与进程结果；本地不执行含 Abort 的整项，不将其列为本片通过。

| 验收项 | 目标 / 过滤器 | 实际结果 |
|---|---|---|
| 失败先行 | grouped/nested 两项精确测试 | 修复前各失败一次，分别为内部错误与空 drop 表 |
| Field/Group 与直接调用邻控 | `unit_direct_call_loan_ends`，排除两个含 Abort 的整项 | 27 passed；禁止执行的两项完整保留、不运行 |
| range continuation 邻控 | `ownership_range_construction` | 14 passed，无过滤 |
| pending lifetime 邻控 | `multifile_ownership_checking::pending_lifetime::unit_pending_borrow_owner_survives_branches_and_nested_calls` | 精确 1 passed |
| frontend 静态检查 | 固定 Rust 1.96.0 `cargo build -p lang-frontend --lib` 与 `cargo clippy -p lang-frontend --all-targets -- -D warnings`，locked/offline | 最终候选均通过 |
| P4 边界核验 | 仓外公开 native API 探针 | P3 修复后两项仍 MissingFact，未生成 object；先前整体 lib-test 编译被 SIGKILL，未进入测试。P4 不在本候选 |
| 文档 / 尺寸 / diff / fmt | 现行本地检查 | 全通过；838 Markdown、2114 Rust、39 项历史超限，无新增债 |

不运行故障注入、校准、含 Abort 的整项测试、Mac 构建或全工作区动态套件；未发布、
不推送，不将本地有界验收冒充精确头官方 CI。D10/E2 与本 Spec 保持 in-progress。

### §112 官方验收与主干组合（2026-11-05）

[PR #321](https://github.com/Halckon/Koven/pull/321) 的原发布头 `85a5b9b5`，基线
`e1cde85f`、合并输入 `598a965f`，已在[官方 run 42052875962](https://github.com/Halckon/Koven/actions/runs/42052875962)
全部通过。Linux/macOS 均实际通过 `temporary_field_pending` 的 13 项测试（含现有 Abort
边界），Preview 生产与独立消费也均通过。这只证明 P3 前端验收，不证明 P4/native 缺口已修复。

本次组合 main `bd036dfd`（含 #319、#320），保留双方原改动，仅解决 Spec 编号冲突，将本节
改为 §112；P3 源码不变。新组合的官方 CI 尚待运行，原发布头的通过记录不替代新组合验收。


## 113. Callable 基础事实消费者退休（2026-11-06）

基于 main `c4ba670a76a567e1475deee47378fa791d25f419`，分支
`feature/spec-0341-e2-progress`。只涉及 Phase 2 测试消费入口，不改生产、Guide、
codegen 或 PR #323 的 E3 退休工作；D10/E2 与本 Spec 继续 in-progress。

- `type_callable` 三项原测试：标准 println 与源码遮蔽、普通成员/函数值调用的稳定映射、
  参数模式矩阵。保留原四份源码、标准或自定义十六 builtin/两能力环境、重复解析、
  测试名称与全部原断言，四次 Single 类型分析替换为八次真实 Unit 分析。
- 同一 ParsedFile 与环境分别走 MemoryDocument/Rooted；helper 检查单源身份、精确
  root/path 与 call 的 source-unit，名称诊断全空后才 validate。仍观察 raw types，
  不增加 final/ownership 前置。Source target 对应明确的 Declaration 或 Symbol；
  External、FunctionValue、返回类型、参数顺序、mode、Place、效果和 L0122×2 保留。
- 仅本文件直接 `check_types` 静态调用 25→21；这不是全仓退休比例，旧 API 与其余
  差异见证仍保留。其他十六项顶层测试体逐字节不变，独立只读复核未发现阻断。

| 验收项 | 命令 / 检查 | 实际结果 |
|---|---|---|
| 目标编译 | `cargo test --offline --locked -p lang-frontend --test type_callable --no-run` | 云端固定 Rust 1.96.0、独立 target、默认 debug=2，编译链接通过 |
| 生产编译 | `cargo build --offline --locked -p lang-frontend --lib` | 通过 |
| 直接行为 | 上述三项原测试名逐一 `--exact` | 各 1 passed、0 failed；每项内双入口，非全 target 动态验收 |
| 静态质量 | `cargo clippy --offline --locked -p lang-frontend --all-targets -- -D warnings`、fmt、docs、Rust 尺寸、diff | 全部通过；839 Markdown、2119 Rust 文件、39 项历史超限，无新增债；主测试948行、helper 51行 |
| 原合同保全 | 三项源码/断言字符串、其余十六项顶层测试体、全部十九项顶层测试名称 | 机械比对与独立静审通过 |

没有执行故障注入、校准、含 Abort 的整项测试或用户 Mac 构建。目标中的其他测试未执行，
本地通过不替代官方双平台 CI；本片未推送、未开 PR、未合并。

### 常量求值消费者双入口补全（2026-11-06）

基于 main `c4ba670a`，仅补齐 `type_constants` 三项既有 MemoryDocument 求值消费者的
Rooted 对照；不是 Single witness 退休，也不依赖其他在途分支。

- 十个 checked arithmetic 失败、四个短路成功、一个非法依赖与五个关联常量整数宽度
  样例均在真实双入口运行。原源码、L0158/空诊断和运算符 `+` primary span 断言保留。
- 私有 helper 参数化既有分析路径，明确检查单源 source_id 和入口 key/root/path；
  仍经无诊断名称解析与 validate 后观察 raw Unit types，不新增 final/ownership 准入。
  其他既有 analyzed 消费者继续默认 MemoryDocument，全部 legacy helpers/witnesses 不变。
- 独立静态审查无阻断；不修改生产、Guide、CI、故障注入或 #323，不据本片宣称
  常量 ownership/native 或整个 D10/E2 已完成。
- 云端 Rust 1.96.0、默认 debug=2、locked/offline 的目标编译、三项定向测试及整个
  `type_constants` 24 项（无过滤）通过，frontend lib build、all-targets Clippy
  （`-D warnings`）、fmt、文档结构、尺寸与 diff 检查通过；官方 CI 未运行，尚未发布或创建 PR。

## 114. D10/E2 消费者批量组合候选（2026-11-06）

按用户要求改为多个逻辑提交、一次 PR 验收。基于已重新核验的 main `c4ba670a`，
独立分支 `feature/spec-0341-consumer-batch` 保留以下八个输入提交的逻辑与顺序：

| 输入 | 原提交 | 范围 |
|---|---|---|
| #326 | `916f731b` | callable 基础消费者 |
| #327 | `81ce6a4a` | Exclusive prefix 回归 |
| #328 | `7e4afbc` | 核心 container 消费者 |
| #329 | `f87f8d48` | signature L0125 有界修复 |
| #330 | `98aa7bbe` | expected move literal 消费者 |
| resource | `876d4e73`、`5d2cfba6` | 资源类型消费者与文档 |
| constant | `4d2988d6` | 常量求值双入口 |

- 上述五项 PR 核验时均 Open/Draft，resource 远端为 `5d2cfba6`；未关闭旧 PR、
  覆盖原分支或合并 main，不包含 #323/#325。各片的原本地/发布状态文字保留其历史时点，
  不作为本组合的动态验收。新的官方 CI 仍须针对组合精确头运行。
- 仅两处文档相邻追加冲突：Architecture 保留 container 与 resource 双方说明，Spec
  同时保留 callable §113 和常量求值小节。原八个提交的源码/测试补丁逐字保全。
- 组合增量不删除旧 witness：原 non-storable/wrong-arity Single 源及四条诊断断言保留，
  再将同一源码与自定义环境送入 #328 双入口，核 #329 修复后的
  L0125、L0125、L0094、L0091 顺序。未迁 deferred callable witness，未扩大生产修复范围。
- 云端 Rust 1.96.0、默认 debug=2、locked/offline，八个目标编译通过；实际执行
  signature 17、container 32、copyability 16、map 17、constant 24、callable 20、
  direct-call-loan 28、resource 6 与原 names-only 1，共 161 项通过。
  callable 排除一项 Abort identity，direct-call-loan 排除两项含 Abort 整项；
  其余 type_checking 不执行。不运行故障注入、校准、Mac 构建或 native。
- frontend lib build、all-targets Clippy（`-D warnings`）、fmt、文档结构、尺寸与 diff
  检查通过；独立只读审查确认原补丁保全与组合增量无阻断。
- 本组合本地验证通过，未推送、未创建 PR。单片历史绿灯不能替代组合 CI；
  D10/E2 保持 in-progress，不自动合并。

### 独立批次：名义构造与接口类型消费者（2026-11-07）

基于 main `11130e16` 的 `feature/spec-0341-nominal-type-consumers`，先完成8项enum/构造
消费者，再将4项静态接口/返回流并入同批，不按小片拆PR；不重复#335/#337/#339/#340。
四项代码提交为 `4260f7f2`、`b71d467d`、`5d08826f`、`d3ff4a40`，不改生产、Guide、loan-end或CI。

- 12项原测试、15份原源码全部保留并走MemoryDocument/Rooted；原重复解析和自定义环境不变。
  先核真实source/key，再读取raw类型事实；不增加final/ownership准入或runtime能力。
- 三项flow保留enum payload、nullable/short-circuit/穷举和泛型替换的原期望，精确核
  projection、non-null use、Nullable→narrowed与null comparison的symbol/type关系。
- 四项construction保留nullable adaptation、bare case的零operand、原L0144/None span、
  泛型实参数及IntrinsicBox身份。bare case按source-unit和真实case value_symbol比对，
  不按名称猜target。最初适配误用私有UnitSymbolId构造器导致E0624；仅改测试使用公开
  source_unit/symbol查询后通过，不改生产可见性，不把该次编译失败算通过。
- identity每入口重复typing复用同一AST、inputs、validated names、TypeEnvironment；
  保留完整construction等值、4项顺序、first/second参数及evaluation_index 1/0、
  零参数、EnumCase、无call与result type关系，另核expression/argument的source-unit。
- 四项接口/返回流保留7份原输入、L0092/L0094的primary与label、L0100/L0099、
  super的Function/名义TypeRef与两个ImplicitThis调用、两份合法receiver例及L0088返回流。
- Mac AArch64、Rust1.96.0/default debug=2、locked/offline：基线8+4项通过；最终
  `type_checking nominal_unit::` 12 passed/70 filtered。相关multifile_type_checking：
  source_construction全7项、intrinsic构造1项、null comparison1项（9 passed/145 filtered），
  super receiver与expression tail两项（2 passed/152 filtered）；multifile_type_signatures
  的interface closure/invalid bounds两项（2 passed/15 filtered），合计25项通过。
- 代码头 `d3ff4a40` 的frontend all-targets Clippy（`-D warnings`）、fmt、尺寸与diff通过；
  主文件2123→1770行，5个新增文件均小于1000行，不增加旧尺寸欠账。type_checking仍82项，
  stage继续完整选择原target，不改过滤或计数；Cargo复用已有target串行执行。
- 代码头静态merge-tree对main、#339、#340无冲突；#335/#337的根测试相邻删除有冲突。
  函数集合不重叠，之后集成须保留双方迁移并重新验证，不能整块选择一边。
- 整个target/workspace、运行时Abort/故障注入/校准、native、Linux与官方CI未运行；
  本批仅本地累计，D10/E2保持in-progress，不将定向通过等同阶段完成，不自动合并。

### §114 官方 selector 覆盖缺口修复（2026-11-06）

#331 原九提交头 `bb769e47` 的 CI 绿灯不能证明新增 signature storage 九项实际执行：
`check_stage_integration.sh` 原来未列 `multifile_type_signatures`，all-targets Clippy 的
编译覆盖也不能替代动态运行。本地 17 项通过记录仍有效，官方动态证据须补跑。

- 在既有 frontend stage batch 增加完整 `multifile_type_signatures`，不带测试过滤器，
  与原目标一起由既有 Linux/macOS targeted job 实际执行；不移除、跳过或改写其他目标。
- 新增 Python 合同核目标恰好一次、正确 frontend 命令、无 positional/exact/no-run
  过滤及 container_storage 模块接线。修复前真实失败 `1 != 0`，补 selector 后该文件
  43 项回归通过。不在本地执行包含故障注入与 Abort 的完整 stage 脚本。
- 最新 main `ea476ad` 与原 base `c4ba670a` 的这两份脚本一致；本片直接追加到 #331
  原九提交链，不重写历史、不改 #323、不丢弃另存的 borrow/string 四提交。
  此追加提交官方 CI 尚待发布与实跑，不能以旧 CI 复用证明新增目标已执行。

本片本地验收：43 项 Python 合同、bash 语法、文档结构与 diff 检查通过，独立静审无阻断。
云端固定 Rust 1.96.0/default debug=2/locked/offline 首次编译因自有缓存 ENOSPC
失败（未进入测试）；获准清理该闲置 incremental 后重新编译成功，完整 signature
17 项通过，含全部九项 container_storage、零过滤。故障日志保留，不将失败尝试算通过。

追加头 `c551d87e` 的官方 run `42231702822` 因独立 composition 合同仍期待 130 个目标、
实际 131 而失败，动态门禁跳过；此前只跑 selector 合同文件漏掉这项计数验证。
本次复现该失败后，明确要求新增 signature 目标恰好一次、剔除它后的原集合仍为 130，
总数为 131，并保留全局唯一性与全部原目标断言。不是放宽重复/过滤资格。
本地相关 45 项 Python（selector 与 composition 成功路径）和完整 signature 17 项均通过；
不运行注入失败的整项门禁，新的官方动态结果仍须精确新头实跑。

### 独立批次：nullable when 类型消费者（2026-11-07）

基于 main `a56e1b17` 的独立 `feature/spec-0341-nullable-type-consumers`，不重复 #335/#337，
不改生产、Guide、loan-end、CI selector 或其它工作树；按 flow、mutation、identity 与验收分提交。

- 12项原测试、15份原 source 迁至 MemoryDocument/Rooted：四项正常域/非空读取与后续 if
  隔离、七项 mutation/capture/field/failed-trial 诊断、一个重复检查稳定身份消费者。
  自定义环境、重复解析、原诊断顺序/span、非空 use 数与空计划断言保留。
- identity 每入口重用同一 AST、inputs、validated names 与 TypeEnvironment 做两次 raw typing，
  比较完整 plan、同一 source-qualified subject 的唯一 call 与两entry；仅将原裸 expression
  查询适配成 qualified subject.expression()，不增加 final/ownership 前置。
- 首组五候选中，`nullable_when_remaining_single_case_keeps_payload_refinement` 在 Unit
  对原 `!is Shape.Point -> x.radius` 报 L0113，原 Single 成功。该函数与原期望完整恢复，
  不放宽 oracle、不扩大生产修复。另三项 native_eligible/body_type 专有字段测试缺少等价
  Unit 描述符，保留原 Single witness，不拿较弱的 source_type/flow 观察替代。
- 原16项 nullable_when 测试全部保留（12双入口＋4原Single），type_checking 总数仍82项；
  stage 仍完整选择该 target，不新增计数或过滤。其它原测试函数不改。
- Mac AArch64、Rust1.96.0/default debug=2、locked/offline：基线16项通过；最终
  `type_checking nullable_when_` 16 passed/66 filtered，相关
  `multifile_type_checking nullable_when::` 4 passed/150 filtered，共20项通过。
  Cargo 使用本任务已有 target 串行执行，不另建并行缓存。
- 代码验收头 `ab55164c`：frontend all-targets Clippy（`-D warnings`）、fmt、846份 Markdown、
  尺寸与 diff 检查通过；独立只读审查确认源/oracle保全、同分析输入的重复检查与完整selector
  接线无阻断。主文件2123→1948行，新增四文件全部小于1000行，不增加旧尺寸欠账。
- 整个 target、Abort/故障注入/校准、native、Linux 与官方 CI 未运行；D10/E2 保持
  in-progress，不将定向通过或静态事实提升为全部 Single 退休与 native 能力，不自动合并。

### Borrow-result 与 String clone 入口补全候选（2026-11-06）

基于 main `c4ba670a`，独立于 #331 源码与 #323；同分支保留两项逻辑提交后批量验收，
不按小片重复创建 PR。不修改生产、Guide、CI 或既有 Single witnesses。

- `type_borrow_result` 三项原 MemoryDocument 测试补 Rooted：四个来源/可空性正例、
  三个无效来源反例及三组调用合同样例，保留原源码、空诊断/L0162 与 Parameter(1)、
  named argument 参数位置断言。检查实际单源 source_id 与 key/root/path。
- String clone 主 helper 本已双入口，本片不改。仅最后一项 Rooted last-use receiver
  测试补 MemoryDocument，保留 class field/Rc payload 原源码、两项 effect、实际
  receiver loan/root 与唯一 CallReturn drop 断言；不将 descriptor 等同独立 loan-end 事件。
- 这是入口覆盖补全，不宣称 Single 退休或新 borrow-return/native 能力。
  云端 Rust 1.96.0、默认 debug=2、locked/offline：borrow-result 3 项、String clone
  14 项全部通过（无过滤）；目标编译、frontend lib build、all-targets Clippy
  （`-D warnings`）、fmt、文档结构、尺寸与 diff 检查通过，独立只读审查无阻断。
  未执行故障注入、Abort 整项、Mac 构建或 native；未推送、未开 PR，官方 CI 未运行，
  D10/E2 保持 in-progress。

#### 同批追加：声明拒绝的严格入口对照

- `type_declaration_frontier` 的原四个声明在 MemoryDocument/Rooted 均检查完整诊断、
  marker span、无 borrow-return、final admission 拒绝、ownership 未启动和 raw facts
  不变。原 Rooted 单源 witness 与固定 expected diagnostics 函数字节保留。
- 云端同工具链与默认 debug=2 下，frontier 2、borrow-result 3、String clone 14，
  合计 19 项重跑全部通过，无过滤；frontend all-targets Clippy（`-D warnings`）通过。
  本片只补拒绝边界覆盖，不删除原 witness，不授予新声明能力；随当前未发布批次积累，
  不新增单独 PR，不把本地结果当作官方 CI。

#### 同批追加：普通借用来源与权限恢复

在保留前四提交的独立分支中无冲突同步 main `6272d3e5`；不等待 #331 CI，不改其分支。
`ownership_borrow_result` 四项原消费者、六份源码由单一 MemoryDocument 补为真实双入口，
检查实际 source_id 与 root/path。保留 wrapper/local scope 的空诊断与 deferred、
foreign/temporary 来源 L0162、source move L0135 和普通 val L0163 原断言。
无生产或新语言能力变化，不移除 Single witness；同批继续累积，不单片创建 PR。
独立静审保全通过；最初编辑的分号错误经 fmt 捕获并修正，动态验收以下列实际结果为准。
云端固定 Rust 1.96.0/default debug=2/locked/offline：ownership_borrow_result 4、
String clone 14、borrow-result 类型 3、frontier 2，共 23 项通过（无过滤）；编译、
all-targets Clippy（`-D warnings`）、fmt、文档/尺寸/diff 通过。未运行故障注入、Abort、
Mac 构建或 native；本批尚未发布，官方 CI 未运行。

#### 同批追加：借用实际来源与原子拒绝 oracle

`ownership_borrow_origins` 五项原测试、18 组原样例补为真实 MemoryDocument/Rooted；
保留原来源数量、空 deferred、L0162/L0163/L0164、错误时空 origins/drops，以及
primary/来源 label 的原文本断言。原两阶段诊断门、typed validate 与实际 ownership
分析均保留，增加 source_id/key/root/path 身份检查；不把两入口通过升级为跨文件/native 能力。
云端同工具链/default debug=2/locked/offline，origins 5 加前批 23，共28项通过（无过滤），
目标编译与 all-targets Clippy（`-D warnings`）通过。未运行故障注入或 Abort，未发布或新建PR。

#### 新 main 上继续：last-use 同作用域与未使用结果

保留本批已有提交，合入已含 #331 的 main `751efd6a`；Rust 无冲突，Spec 仅末尾追加
冲突，双方历史与后续批次记录均完整保留。`ownership_borrow_last_use` 两项原测试补
Rooted：同作用域八份原 source 与无诊断/deferred oracle，及未使用结果的单一 end、
AfterStatement、source_unit 和精确 initializer statement 文本。其余六测试仍走原
MemoryDocument；helper 只参数化入口并加真实 key/root/path 检查，不改生产、源码或 oracle。
本片不声称任意 CFG/NLL 或 native 支持，随借用批次累计，尚未发布、未单开 PR。
云端 Rust 1.96.0/default debug=2/locked/offline：last-use 8、origins 5、borrow ownership 4、
String clone 14、borrow types 3、frontier 2，共36项无过滤通过；目标编译、all-targets
Clippy（`-D warnings`）、fmt、文档/尺寸/diff及独立审查通过。故障注入/Abort/Mac构建/native
未运行，官方 CI 尚未运行，不将本片双入口覆盖表述为 Single 退休。


#### 同批补齐：last-use 其余消费者

余下六测试的14份原输入也运行 Rooted，保留父子 binding/逆序 ends、source 清理唯一性、
独立 statement 身份、L0135 primary/label、错误 bindings 清空、Map失效、call-return
正反例及保守 Block endpoint。前两项双入口不变，删除仅默认 MemoryDocument 的未使用包装。
全部八项现均双入口，不改分析算法，不将 scoped 保守行为升级成任意 CFG/NLL 能力。
云端固定 Rust 1.96.0/default debug=2/locked/offline，六目标36项无过滤重跑通过，
all-targets Clippy（`-D warnings`）通过；同批累计未发布，不单独开 PR。

#### 同批收尾：caller-continuation 关闭边界

仅将共享 `assert_ownership_rejects` 的五项原消费者补 Rooted：capture、conditional
initializer、nested call、temporary source 与 loop binding。原五源、L0162/L0164、
bindings/ends 清空断言保留；其余测试与独立 Single loan-end helper 不改，默认 checked
仍为 MemoryDocument。此补覆盖不开放新来源或 CFG 能力。
云端固定 Rust 1.96.0/default debug=2/locked/offline，continuation 14 加前六目标36，
共50项无过滤通过；目标编译、all-targets Clippy（`-D warnings`）通过。所有七目标本已在
stage selector，未增加目标或修改计数；保留多逻辑提交统一交付，官方 CI 尚未运行。

### 后续事实合同：直接调用事件发布门的非空对照（2026-11-07）

`unit_direct_call_loan_ends/gate_contracts.rs` 用固定字节 oracle 在两入口证明同一
成功前缀恰有一个真实 CallReturn loan-end、原临时 owner 的唯一同点 drop，且事件先于
drop；随后分别追加真实 L0131 或既有 deferred 工作，核对事件清空及拒绝最终验证。
错误样本还核对 drops 原子清空。旧 Single witness 与全部既有测试均保留，避免空表
测试在事件发布从未发生时仍通过；不修改生产算法或提升 slice 为完整 loan-end 流。
调查确认现有 direct-call slice 明确排除借用返回、投影及 receiver/provider。完整退休
continuation witness 仍需另行证明来源交接与控制转移事件合同，不能用 result-end 计数
替代。本片仅加强已存在能力的门禁，不宣称已解除上述依赖。
云端 Rust 1.96.0/default debug=2/locked/offline：新增2项通过；目标允许范围30项通过，
含 Abort 的两整项按约定仅留 CI；编译、all-targets Clippy（`-D warnings`）、fmt、
文档与尺寸门禁通过。本批本地累计，官方 CI 未运行。

#### 有界正常返回来源交接观察片

本片遵循 Guide10 的唯一来源延续，不改变调用/cleanup 语义。新增
`UnitBorrowCallHandoffFact`：仅直接 Declaration borrow-return initializer、无 receiver、
无 parent 的显式 borrow val，来源须为当前存活的 formed-owned 名称根；该调用实际登记
的 loan tokens 全为 Shared 根 place。Value 辅助参数不在此 token 集内，未整体排除。
在正常返回移除 pending tokens 前，以 call/argument/origin 精确匹配同一 pending binding
捕获来源；仅 initializer 正常提交 binding 后发布。公开 source loan、binding 与 CallReturn
身份；首片未发布辅助 loan-end（后续有界扩展见本页“借用返回的辅助实参结束观察”），不将来源交接标记为来源结束。所有权/类型错误与 deferred
沿现有两层门原子撤销；不改变 iteration cleanup action 或 codegen 合同。
Group initializer、parent reborrow、outer-call 消费、forwarded return、receiver/provider、
投影/temporary 来源及提前退出的完整事件流不在首片能力内。旧 Single witnesses 保留；
消费者现在可额外查询独立 handoff，但完整 loan-end 退休尚未达到。
新增6项真实两入口回归覆盖 named反排固定源码oracle、同根不同实参身份、来源交接与
唯一owner drop、提前return、parent/Group/outercall关闭边界以及非空前缀后的error/deferred
撤销。新API缺失先编译失败，接线后6项通过。一次编译因空间耗尽失败，清理已授权的
本任务incremental后重试通过；未删除源码、日志或既有二进制。
最终云端组合：direct-call目标36项、continuation14项、nested-borrow10项，共60项通过；
仅direct-call的两项Abort整项过滤。all-targets Clippy（`-D warnings`）、fmt、文档及尺寸
检查通过；source-unit/key/root/path、精确L0131与parent事件身份纳入回归。原超限文件
仅新增必要接线：unit model 1095行、dataflow1119行、drop planner1491行，既有尺寸例外
范围内，未机械拆文件或压行。独立生产审查未发现阻断；官方CI尚未运行。

#### 同批补强：跨文件 lineage 与兄弟分支绑定隔离

跨文件 provider/caller 在输入正序、反序分别核 handoff call/argument/binding/root
归属 caller source unit，parameter span 归属 provider，并使用固定源码文本核
`view(owner)`、`owner`、`source: String`。兄弟分支核静态事件各自对应不同 binding、
call、initializer/source argument，不宣称两分支运行时同时执行或完整 CFG 运输能力。
本片仅新增测试，未扩大 source-handoff 生产范围，继续保留 Single witnesses。
首次跨文件样本因 package demo 与逻辑路径不匹配触发 L0146；修正为 demo/provider.ko、
demo/caller.ko 后，三目标62项通过，2项Abort按约定留CI。源码oracle与生产实现未更改。
all-targets Clippy首次因磁盘耗尽失败，确认进程结束后仅清理已授权incremental，重试通过；
fmt、文档/尺寸/diff检查及独立审查通过。本批继续保持多个逻辑提交，未发布新PR。
### 新 main 后续：continuation 普通消费者双入口收束（2026-11-06）

基于已含 #332 的 main `8c9778aa`，新分支以两个逻辑提交补余下五项普通 Unit 测试：
nested/alias scope 权限恢复、live-result/child 跨分支冲突、scope/return 逆序清理、
mutation/exclusive 冲突 span，以及 nullable/generic/copyable shared binding。
原源码、L0135/空诊断/deferred、origin/parent/end顺序、owner cleanup、真实span/details
及binding/end数量全部保留。所有普通 Unit 消费者现均走真实两入口；原三项独立
Single loan-end witnesses、对应helper和parser-only测试不改，不宣称其已退休。
未修改生产、Guide、selector或计数，不扩张CFG/NLL/native能力，不单片创建PR。
云端 Rust 1.96.0/default debug=2/locked/offline：首片完整continuation14通过；组合
continuation14、last-use8、origins5、borrow-result4，共31项无过滤通过，目标编译及
all-targets Clippy（`-D warnings`）通过。官方 CI 尚未运行，本批仍在本地累计。

#### 同批补齐：独立 loan-end witness 的 Unit 两入口

三项差异对照保留原 Single 分析、源码与 CallReturn/ControlTransfer/forwarded-source
非终止断言，仅将 paired Unit 分析扩为 MemoryDocument/Rooted 并校验 source/key 身份。
包括 named-source 两份正反输入，所有 Unit bindings/ends/drop/诊断断言原样保留；
不以 result endpoints 替代独立 loan-end stream，不宣称 Single 退休或扩大 CFG 能力。
云端同一工具链配置：完整 continuation14 与四目标组合31项无过滤通过；目标编译、
all-targets Clippy（`-D warnings`）、fmt、文档与尺寸门禁通过。官方 CI 尚未运行。

### 独立批次：构造与控制类型消费者（2026-11-07）

基于 main `a56e1b17` 的独立 `feature/spec-0341-raw-type-followup`，不改 #335 分支、
生产、Guide、loan-end 或 CI selector；按顶层诊断、构造、控制诊断与验收记录分提交。

- 超大整数原 Single 仅 L0090、Unit 另有 L0167 的原因已定位到现行 Guide05 的
  “顶层与关联命名空间的值声明”：普通顶层 val/var 必须拒绝，并允许继续检查 initializer。
  这是现行 Unit 规则，不删诊断或改变源码位置来达成表面等价。新增一个五源码测试：原
  顶层超大整数逐字复用，补 var、const、local 与无溢出邻居；两入口锁定完整诊断码顺序、
  SourceId、精确 primary span 和 raw validate 拒绝。原 numeric_overflow Single witness 原样保留。
- 八项构造测试迁移原 inference/trial、错误原子性、源码 Box/Rc identity、实参与 capability
  诊断；三项控制测试迁移 assignment/capture 的 smart-cast 失效和 when 的覆盖/合并诊断。
  共11项12份原 source、自定义环境和所有原 oracle 保留；UnitConstructionTarget::Nominal
  与 UnitCallTarget::Symbol 只适配原源码目标身份，不改语义。
- 同一重复解析 AST 使用实际 MemoryDocument/Rooted，核 source_id、key/root/path 与
  expression facts 的 source-unit。名称验证后直接检查 raw types，不增加 final/ownership 前置。
  原82项测试未删减，新增一项后目标83项；stage 仍整 target 选择，不改测试过滤或目标计数。
- Mac AArch64、Rust1.96.0/default debug=2、locked/offline，迁移前12候选各 exact 通过；
  最终 raw_followup 12项通过（71 filtered）、resource 邻控6项通过（77 filtered）、
  原 overflow exact 1项通过（82 filtered），合计19项。仅复用本任务 target 串行执行。
- 代码验收头 `4a2ad8c0`：frontend all-targets Clippy（`-D warnings`）、fmt、846份 Markdown、
  尺寸和 diff 检查通过；独立只读审查核原源码/oracle、Unit身份映射、保留witness与selector
  接线无阻断。主文件2123→1902行，新增文件全部低于1000行，旧尺寸欠账仅下降。
- 全 target、Abort/故障注入/校准、native、Linux 与官方 CI 未运行；D10/E2 仍 in-progress，
  本地定向结果不替代精确发布头双宿主 CI，不自动合并。

### 独立后续：nested-borrow 成功消费者双入口（2026-11-06）

基于 main `8c9778aa`，不依赖待发布的 continuation 后续批次。三项原成功消费者的
13份输入扩为真实 MemoryDocument/Rooted，保留原 Single 分析、精确 call-use、
独立 loan-end 断言，新增 Unit source/key 身份检查；拒绝组原双入口不动，另外两项
成功 witness 保留原 Rooted 默认入口。无生产/Guide/selector 修改，不宣称 Single 退休。
云端 Rust 1.96.0/default debug=2/locked/offline，完整 nested-borrow 10项无过滤通过，
目标编译、all-targets Clippy（`-D warnings`）、fmt、文档/尺寸门禁通过；未运行官方CI。

#### 同批收束：精确顺序与 parent/owner-drop witness

余下两项成功测试的三份原输入补两入口，保留 grouped/named 表达式与实参身份、
同 root 链不折叠、parent 跨外层调用存活，以及原 Single/Unit 的唯一 owner-drop
及 CallReturn 断言。删除未使用的默认 Rooted 包装；不删独立 Single 事件对照。
完整 nested-borrow 10项无过滤、目标编译、all-targets Clippy（`-D warnings`）、
fmt、文档及尺寸门禁通过；本批累计本地逻辑提交，未尝试远端写入或官方 CI。

#### 独立 source-prefix 结束观察：有正常分支的 initializer 被 return 放弃

同步 main `a56e1b17` 后保留原六提交。新增 `UnitBorrowCallPrefixEndFact` 与
`borrow_call_prefix_ends()`，只投影 planner 已实际发布的 ControlTransfer(Return)
加 EndCallLoan 动作。要求 direct initializer 的正常 binding 来源证明与动作
call/argument/target 精确一致：Declaration、Borrow result、无receiver/parent、Shared
formed-owned根。正常 binding 仅限制支持范围，不作为退出发生的证据；不修改pending
或cleanup执行，不复用Owned direct-call事件类型，也不发布已建立binding的承诺。
公开原loan/真实point/同点preceding_drops，不提供完整cleanup图。无正常完成分支的
initializer不会产生现有binding证明，因此 `select(owner, return)` 仍在片外；空表
不能推导没有abandonment。保留Single ControlTransfer witness，不改变正常handoff能力。
回归覆盖来源求值前后、同根不同参数来源身份、正常完成后的return不误报、同点owner
drop顺序，以及非空成功对照后的错误/deferred原子撤销。新API缺失先编译失败；新样本
曾因条件值试图move存活来源触发L0135，改用独立临时辅助值和独立坏值构造L0131，
保留精确诊断要求，未放宽原有测试或修改语言行为。
云端 Rust1.96.0/default debug=2/locked/offline：三目标66项通过，2项Abort按约定仅CI。
新增片外空表、入口身份、Shared/formed-owned/root断言通过，独立静审无阻断。
all-targets Clippy（`-D warnings`）首次空间耗尽；确认无活动构建后仅清理本任务已授权\nincremental，重试通过。fmt、文档/尺寸/diff门禁通过，官方CI未运行。

#### 补足无正常 binding 的直接 initializer 提前 return

前片的无正常分支限制由实际求值上下文消除：ValueState 在进入直接 Borrow-return
initializer 时保存(call, typed映射source argument)候选，随分支clone并在表达式返回后
移除；typed映射只选argument identity，不提供origin。实际unwind移除EndCallLoan时
必须匹配活跃候选，核Shared/root/formed-owned、owner仍存活、原operand place与真实
loan target全等且不为borrow binding，然后记录该次iteration_actions下标。查询只投影
这些真实Return动作，不再依赖最终BorrowBindingFact，pending/cleanup执行不变。
原operand核对阻止parent canonicalization伪装成owned根；仍不开放receiver、projection、
carrier或完整CFG事件流。无条件return正例先稳定失败（期待1事件、实际0），再接线；
保留bindings为空、handoff为空以及旧Single对照，绝不把退出标记为结果已建立。
命名反排且无正常binding的退出样本核固定call/argument/parameter文本、source-unit、
Single ControlTransfer等价及owner清理顺序；来源求值前return仍无事件，parent样本先证实
真实loan存在再核排除。三目标69项通过，2项Abort整项仅留CI；独立静审无必须修复问题。
目标编译、all-targets Clippy（`-D warnings`）、fmt及文档/尺寸/diff门禁通过；在Clippy前
确认无构建进程后仅清理已授权incremental恢复空间，未删其他产物。官方CI未运行。

#### 来源提前 return 的跨文件与兄弟边证据

同步主干 `11130e16`（#338）后，原借用事实合同及逻辑提交保留。新增跨文件样本在两种
provider/caller 输入顺序下核 caller 的 call、argument、return 与 owned root 身份，
参数 Span 则仍属于 provider；无最终 binding/handoff 时要求唯一真实 prefix 事件。
兄弟分支样本要求 call/argument/return point 互异、owned root 相同，各边恰好匹配一个
Single ControlTransfer witness，并在对应点唯一清理该 owner，preceding_drops 精确为零。
未修改生产语义或删除原 Single；静态兄弟边不表示同时执行，完整 cleanup 流仍未承诺。
云端 Rust 1.96.0、locked/offline、默认 debug=2：借用事实定向17项通过，三个相关目标
合计71项通过；两项 Abort 按约定仅留正式CI。all-targets Clippy（`-D warnings`）、fmt、
文档、尺寸与diff门禁通过；本批尚未发布，官方CI未运行。

#### 来源 prefix 与辅助 temporary 的清理组合

在主干仍为 `11130e16` 时补充 `select(owner, sink("temporary", return))` 的实际事件
组合。期望 call、argument、temporary 与 return 先由唯一源码 AST 选取，分别核 borrow
source prefix 和普通 direct-call end 的精确身份、Shared kind、原 loan 成员关系及
同一 ControlTransfer。两者 preceding_drops 都为零；同点 drops 必须恰为 temporary
然后 owned root。Single 的两条独立结束见证仍保留；不据此宣称两类切片之间的统一全序。
生产合同和旧断言不变，三个相关目标72项通过，两项Abort仍仅CI；all-targets Clippy
（`-D warnings`）、fmt、文档、尺寸及diff门禁通过。本批仍未发布，官方CI未运行。

### 独立批次：非空断言类型消费者（2026-11-07）

基于 main `11130e16` 的独立 `feature/spec-0341-non-null-type-consumers`，不重复
#335/#337/#339，不改生产、Guide、loan-end、selector或其它工作树。三项代码提交
`2eba1eea`、`1425e432`、`c559162e` 分别迁移诊断、来源类别/条件能力、稳定identity消费者。

- 8项原测试保留21个展开source case（去重后20份源码），全部进入MemoryDocument/Rooted；
  原重复解析与自定义环境不变，核真实source/key及descriptor的source-qualified身份。
- 四项诊断消费者保留trial回滚/唯一选择、原L0085/L0123顺序与span、preexisting计划及
  Error/Deferred不发布计划；六类来源×Int/Node与两项泛型能力保留place/temporary、
  operand/operator span及Copyable/MoveOnly判据，不以类型候选授予ownership权限。
- 单次求值测试保留同一AST、inputs、validated names、TypeEnvironment的重复raw检查、
  完整计划一致性、nullable/inner关系、查询和唯一源码call。error遮蔽仍按真实源码call
  identity查名称与Int返回类型；只观察Abort描述符，不执行源程序或触发进程Abort。
- Mac AArch64、Rust1.96.0/default debug=2、locked/offline：基线8项通过；代码头
  `c559162e` 的 `type_checking non_null_assertion_` 8 passed/74 filtered，相关
  `multifile_type_checking non_null_assertion_` 5 passed/149 filtered，共13项通过。
  复用本任务既有target串行执行，不新增并行Cargo缓存；type_checking总数仍82项。
- frontend all-targets Clippy（`-D warnings`）、fmt、848份Markdown、尺寸与diff检查通过；
  独立只读复审确认全部原输入/oracle、两入口身份与同分析输入重复检查无阻断。主文件2123→1893行，
  四个新文件均小于1000行，不增加旧尺寸欠账；stage仍完整选择原target，不改过滤/计数。
- 静态merge-tree对当前main与#337无冲突；#335模块注册与账本末尾追加、#339相邻
  删除有文本冲突，迁移函数集合无交集。集成时须保留双方迁移并重新验证，不能整块选择一边。
- 整个target/workspace、运行时Abort/故障注入/校准、native、Linux与官方CI未运行。
  D10/E2保持in-progress；本批仅本地累计，不将定向通过等同阶段完成，不自动合并。

### 独立批次：基础与接口 raw type 消费者（2026-11-07）

基于 main `754f6858`，独立于 continuation/loan-end 合同，按基础类型与接口契约两个
逻辑提交集中交付，不修改生产代码、Guide、原 raw type 准入或 CI selector。

- `type_checking::scalar_unit` 承接四项原测试：九种字面量的 source-qualified 类型、
  local/lambda/operator/type-ref 诊断、返回/分支诊断顺序及 Byte overflow 的精确 `128` span。
- `type_checking::interface_unit` 承接五项原测试：接口边与环、alpha-equivalent overload、
  receiver mode、replacement/override 和默认实现冲突，原 L0095–L0102 顺序全部保留。
- 九项15份原始 source 与自定义环境不改；同一重复解析 AST 分别进入 MemoryDocument/Rooted，
  核实际 source_id、root/path 及全部 expression facts 的 source-unit。只验证名称后读取 raw
  typed facts，不增加 final/ownership 前置；其余73项测试与原 parser helper 保留。
- 尝试第十项顶层超大整数时，原 Single 仅 L0090，而 Unit 为 L0167、L0090。该项完整恢复
  Single witness，不改预期或源码位置，不把原 top-level runtime value 限制当作本片待修生产
  回归；仅记录消费表示差异，未声称本批覆盖该迁移差距。
- Mac AArch64、固定 Rust 1.96.0/default debug=2、locked/offline：迁移前十项各 exact 通过；
  最终 `cargo test -p lang-frontend --test type_checking _unit::` 的九项迁移与六项原 resource
  邻控共15项通过、67项 filtered，原 overflow exact 另行通过。目标共82项，未删减测试数；
  stage 原已完整选择 type_checking，不新增 target、不修改计数。
- 代码验收头 `5e9cbaf0`：frontend all-targets Clippy（`-D warnings`）、fmt、846份 Markdown、
  尺寸及 diff 检查通过；独立只读复审确认九项原源码/oracle、其余测试逐字保全、source-qualified
  identity 与 selector 接线无阻断。主测试文件2123→1942行，旧尺寸欠账仅下降。全 target、
  Abort/故障注入/校准、native、Linux 与官方 CI 未运行；本地定向结果不能替代精确发布头
  双宿主 CI，D10/E2 保持 in-progress，不自动合并。


## 115. Raw type 消费者四批集中集成（2026-11-07）

以已含 #340 的 main `88df4632a2e3239fb610166ef9b23dda148c9337` 为基线，在独立
`feature/spec-0341-type-consumer-integration` 保全下列发布头，未改原分支或关闭原 PR：

| 来源 PR | 精确发布头 | 原逻辑提交数 |
|---|---|---:|
| [#335](https://github.com/Halckon/Koven/pull/335) | `5f3dd1c613f1636868bdf5f0887da94b71d66d3c` | 3 |
| [#337](https://github.com/Halckon/Koven/pull/337) | `c611454f86bfdd24e64f48a8b978c4e123fe33ed` | 4 |
| [#339](https://github.com/Halckon/Koven/pull/339) | `980971ed0a7266f55cff8080a28148dbbb4a2579` | 4 |
| [#341](https://github.com/Halckon/Koven/pull/341) | `7427c430e30ec4eb9d2291926d63a8e9199af39f` | 5 |

- 四个 no-ff merge（`7b4fa3d2`、`616cbf83`、`7b9de937`、`10b422b2`）保留全部16个
  原逻辑提交。冲突按迁移函数集合合并根注册/相邻删除，Spec 相邻追加保留双方原文；
  16个新增模块与各发布头逐字节一致，#340 的四个 non_null 模块及原控制/resource 模块不变。
- 独立静审逐项核44项迁移、57份原源码字面量及全部原oracle；根文件剩余22项测试和5个
  helper逐字保留。只删除合并后不再使用的 `Expression` import，首次Clippy所报该告警
  已修复并重跑通过。原82项测试无遗漏，加顶层诊断对照后共83项，未改selector或计数。
- 原 overflow Single L0090、Unit顶层L0167/L0090及四项nullable Single差距对照完整保留，
  不改生产、Guide、原准入或source位置，不宣称Single全部退休或扩大ownership/native能力。
- 组合代码输入为 `10b422b2` 加上述单一导入清理；Mac AArch64、Rust1.96.0、default
  debug=2、locked/offline，复用既有target串行执行，实际验收如下。

| 验收项 | 测试目标/选择 | 实际结果 |
|---|---|---|
| 全部原输入及组合 | `cargo test --locked --offline -p lang-frontend --test type_checking` | 83 passed，0 filtered；导入修复后重跑通过 |
| 相邻 Unit 合同 | `multifile_type_checking` 的 `nullable_when::`、`non_null_assertion_`、`source_construction::`、`unit_intrinsic_box`，以及 `dispatch_receivers::unit_super_uses_the_selected_receiver_mode_and_exact_interface_instance`、`expression_tails::expression_tail_fallthrough_is_stable_under_input_permutation` | 19 passed，135 filtered |
| null flow 邻控 | `nullable_flow::null_comparisons_publish_stable_source_qualified_flow_facts --exact` | 1 passed，153 filtered |
| 接口签名邻控 | `multifile_type_signatures` 的 `cross_file_interface_closure_substitutes_arguments_and_checks_bounds` 与 `cross_file_invalid_bounds_supertypes_and_cycles_keep_existing_codes` | 2 passed，15 filtered |
| 编译和格式 | frontend all-targets Clippy（locked/offline，`-D warnings`）、`cargo fmt --all -- --check` | 通过 |
| 尺寸 | `python3 scripts/check_rust_sizes.py --base 88df4632` | 通过；根文件1893→963行，超1000行旧文件39→38 |
| 文档与差异 | `python3 scripts/check_docs.py`、`git diff --check` | 848份Markdown通过，无空白错误 |

共105项不同测试通过；type_checking只做前端分析，Abort仅事实描述符断言，不执行源程序。
整个workspace、运行时Abort/故障注入/校准、native、Linux与本集成头官方CI未运行；
D10/E2继续in-progress，原各批历史验收不改写，集中PR仍须精确发布头双宿主CI，不自动合并。

## 116. 复审触发的常量映射测试消费者迁移（2026-11-07）

基于 `03807a9e`，`ssa/constant_materialization.rs` 的四项映射测试改由真实 Unit 常量
capability 生成事实，保留原源码、值/字节/SSA类型、单次物化及空诊断合同；生产映射不变。
旧入口消费者分流、独立复核发现与修复、红绿门禁及本机实际结果统一记在
[SPEC-0392 §11](0392-compiler-layered-review.md#11-第三切片真实消费者分流与常量映射测试迁移2026-11-07)。
仅减少一个旧测试helper，不声明旧检查器/lowering整体退役，不继承旧CI为本片通过。


## 117. 顺序容器与混合借用测试消费者迁移

基于 `c260be13`，顺序容器八项 Phase 2 测试与七份混合借用非法源码改走真实 Unit。
迁移暴露顺序容器显式签名漏报，修正及输入、oracle、复核和实跑结果见
[SPEC-0392 §12](0392-compiler-layered-review.md#12-第四切片顺序容器与混合借用消费者迁移)。

## 118. 借用事实批次的主线组合验证（2026-11-07）

将借用事实批次 `3a12ff9d` 与 main `bff9468a` 合并为 `5c415a43`，保留全部原逻辑提交。
唯一文本冲突是本账本相邻追加，双方原文均保留；生产代码无冲突。独立只读复审未发现阻断问题。
云端组合编译因 ENOSPC 中止，三个测试目标未执行，不能沿用旧基线本地结果为新组合通过。
完整提交以校验过的 Git bundle 转入 Mac AArch64 隔离工作区，保留云端与用户现有工作。

Mac Rust1.96.0、locked/offline、默认debug=2，复用空闲target串行执行三个相关目标：
`ownership_borrow_continuation` 14项、`ownership_nested_borrow_arguments` 10项、
`unit_direct_call_loan_ends` 48项，共72项通过；后者两项Abort依既定范围整项留正式CI。
frontend all-targets Clippy（`-D warnings`）、fmt、849份Markdown及diff门禁通过；
相对main的尺寸门禁通过，38个超1000行旧文件不增加。不是frontend全量、native或双宿主官方验收。
公开handoff/prefix仍是有界观察，保留Single独立见证，不承诺完整事件全序或cleanup图。
D10/E2保持in-progress，本批需精确发布头的正式双宿主CI，不自动合并。

## 借用返回的辅助实参结束观察（2026-11-07）

基于 `be42f064`，补充显式 borrow initializer 已提交 handoff 中真实退休的辅助 Shared owned 根
loan tokens。来自实际 pending 队列移除，保留源码求值顺序，并按 call/argument 身份排除继续
交接的来源；同根参数不得混淆。未新增 cleanup action，正常后态与原紧邻 retain 一致。
逐原始 operand/place 核对活跃 owned local，排除 borrow binding、provider、投影和 nested
borrow result；旧 handoff 本身的覆盖范围不缩窄。不承诺完整事件全序，不退休复杂 Single 见证。

新 API 回归先以 E0599 失败，随后生产接线完成；named 反排双辅助、同根不同实参、借用 alias
排除、source-only、return 放弃、非空事实的 error/deferred 原子撤销、跨来源重排均有定向证据。
Mac Rust1.96、locked/offline、默认 debug=2：borrow 定向 26 项、两个 consumer 目标 24 项、
nested L0164 拒绝且不泄漏 handoff 的边界负例 1 项，共 51 项通过；frontend all-targets
Clippy（`-D warnings`）、fmt、Markdown 与 diff 门禁通过。正式双平台 CI 与跨 crate check
待发布头执行；未运行全量、故障注入、校准或含 Abort 的整项测试。

## String 根 swap 的 owner transport 增量（2026-11-07）

从 main `be42f064` 隔离实现完整 String 根 swap 的真实 token 交换，接入现有
replace/move/cleanup 链，保留 cleanup 输出不变。新增独立 literal origin 预期、
遗漏/重复/别名交换负例、Unsupported 后缀原子回滚及全 Analysis 开关对照。
V1 catalog、Resource owner 和相关 Single 见证保持原边界，D10/E2 不据此宣告完成。

验证：Mac Rust 1.96、默认 debug=2、locked/offline，复用已释放 target 串行执行
`cargo test -p lang-frontend --lib owner_transport`，92 项通过、259 项过滤；
frontend all-targets Clippy `-D warnings`、fmt、849 份 Markdown、diff 和尺寸门禁通过。
未运行 frontend 全量、native、Abort、校准或宿主故障注入；正式双宿主 CI 仍待发布头验证。

## 119. Nullable when 剩余单 case 类型修复（2026-11-07）

基于 main `be42f064e348997260710a51792723aff154b5a6`，在独立
`fix/spec-0341-nullable-when-remaining-flow` 修复 Phase 2 raw type producer。
代码验收头 `1b1bc31a2501f9f3fbfb52a445f918c89c201fd8`；不改 Guide、ownership、
loan-end、selector或其它工作树，不将原规范差异靠改源码/删除诊断掩盖。

- 先以原 `nullable_when_remaining_single_case_keeps_payload_refinement` 输入复现：
  Single通过，Unit回归在`radius`的141–147范围报L0113（`3c2af467`保留红灯测试）。
  `!is Point`的原覆盖域含Circle/null，而前一null条件已使实际body域只剩Circle；
  Unit原先只补非空根类型，未用实际body域补case，故没有唯一payload事实。
- 复用既有`facts_for_single_case`，仅在进入when时类型仍为nullable、来源key/generation
  有效且body为有限域时补事实。保留alternatives交集、赋值/inout kill、trial及分支合流；
  不改descriptor准入，三entry的when仍不发布仅支持null/else两entry的nullable descriptor。
- 原Single测试的源码字面量和无诊断oracle逐字迁到同名MemoryDocument/Rooted测试，
  删除的仅是重复旧消费者。原四项nullable Single witness现余三项专有字段对照，未弱化其断言。
- 六项新增测试展开14份源码，核顺序/else、泛型Int替换、null/多case、condition及同entry
  后续inout、body写入/join、字段/调用/capture、未选中body写入；负例锁L0113、source/span
  和失败不发布payload projection。跨文件新增一项用两份源码核field/case来源、泛型实参及
  输入排列下完整expression/projection事实一致，不新增target。
- 首轮通用补事实使旧非nullable `else == root` oracle失败；本片已加nullable入口门控，
  原测试原样通过。非nullable、以及声明T?但进入when前已收窄为T的subject仍保留旧行为，
  是另行处理的边界，不将旧v0.32测试注释提升为现行Guide规则。跨文件新测试最初误要求
  三entry descriptor，已改为检查现有空descriptor边界，未为满足测试扩张生产能力。

Mac AArch64、Rust1.96.0、default debug=2、locked/offline，串行复用既有target；
最终门控落定后按受影响选择验收，避免重跑整target及已完成的其它检查：

| 验收项 | 目标/选择 | 实际结果 |
|---|---|---|
| 原见证与新正反例 | `type_checking` 的原同名Unit测试、`remaining_case_payload_` | 7 passed，82 filtered |
| nullable/enum/control邻控 | 同target其余nullable flow/mutation/identity、三项Single字段对照、`nominal_unit::flow::`、`raw_followup::control_diagnostics::` | 21 passed，68 filtered |
| 跨文件新合同及旧边界 | `multifile_type_checking` 的`control_flow::nullable_remaining_case_payload_keeps_cross_file_identity_and_generic_arguments`与`control_flow::cross_file_when_covers_enum_boolean_nullable_and_statement_contexts` | 2 passed，164 filtered |
| 既有Unit合同 | 同target的`nullable_when::`、`nullable_when_unit::`、cross-file enum/invalid type-test/when诊断三项 | 10 passed，156 filtered |
| 编译/格式/尺寸 | frontend all-targets Clippy（`-D warnings`）、fmt、尺寸门禁（base `be42f064`） | 通过；2144份手写Rust、38份旧超限文件不增加 |
| 文档与差异 | `python3 scripts/check_docs.py`、`git diff --check` | 849份Markdown通过，无空白错误 |

最终共40项不同测试通过，目标注册数为type_checking 89、multifile_type_checking 166；
producer834行、新测试文件212行、既有control_flow822行，根测试963→953行。
独立只读复核确认原输入/oracle、迁移注册及最终门控；初审漏掉的旧非nullable邻控已由实际
测试发现并收窄修复。未运行全target/workspace、故障注入/校准、含Abort的整项测试、
native或Linux；这些不计为本地通过。本批仅本地逻辑提交，未发布/运行官方CI，D10/E2仍in-progress。

## 正常借用返回的固定身份 oracle（2026-11-07）

基于 main `2fd6bfb9`，保持两份正常返回源码字面量不变，以固定源码字节期望验证完整单项
handoff、来源 call/argument/parameter/root、绑定声明以及有序辅助结束向量；同轮 Rooted
与 MemoryDocument 均核 source qualification、Shared、真实 loan 成员与 begin/end 范围。
仅 named 反排双辅助的旧 Single 对照退休，其两条实际结束身份、顺序和 CallReturn 已由
真实辅助 producer 与独立完整向量承接；不改任何生产语义或新增 test target。

独立复核指出 direct binding 的完整 loan-end 流否定不能由有界 Unit 切片证明，因此保留
原 Single source(call,argument) 不结束见证和原 owner 延后 drop 断言；只补固定身份 oracle。
复杂控制流、temporary、错误/deferred 等旧见证不动，不将此片宣称为完整 Single 退休。
本地 Mac Rust1.96、locked/offline、默认 debug=2，两项相关测试及 frontend all-targets
Clippy（`-D warnings`）通过；未运行全量、native、校准、故障注入或含 Abort 的整项测试。

## String swap 的 V1 value 投影（2026-11-07）

基于 main `2fd6bfb9` 将实际双根 swap 接入 V1 守恒投影；producer catalog 保持原 token，
交换不创建 result/producer。连续同 call-return 的两根 cleanup 复用真实 commit 证据，
各自核对交换后的 token 和实际槽位。新增真实双根终结、重复 swap/replace 组合、
缺失/重复/别名/foreign 事件负例。Choice swap、Resource 和其他既有 Unsupported 边界不变。

本切片 Mac Rust1.96 默认debug=2、locked/offline，串行复用已释放target：
owner_transport lib过滤器95项通过、259项过滤，frontend all-targets Clippy -D warnings通过。
fmt、849份Markdown、diff及相对2fd6bfb9尺寸检查通过，38个历史超长文件未增长。
未运行全量、native、Abort、校准或宿主故障注入；正式组合发布头双宿主CI仍待验证。

组合验收：`ddc1193e` 的 owner 95 项、borrow 27 项通过；all-targets Clippy 首次发现
外部 API 编译消费者 `ownership_value_api` 缺少新 Swap 的穷尽匹配，故不能沿用分支 lint
通过声明证明组合。补显式 Swap 两值匹配后，该目标 1 项与最终 all-targets Clippy 通过。
合计 123 项定向测试；fmt、849 份 Markdown、diff及尺寸门禁通过。未以此替代正式 CI。

## Return-prefix 固定 oracle 迁移（2026-11-07）

基于 main `78a36f25`，保持原源码与既有 Unit 断言，迁移
`unconditional_named_return_keeps_actual_source_identity_without_a_binding`、
`sibling_return_edges_keep_distinct_source_tokens_and_cleanup_points` 两项成功消费者。
固定字节期望直接来自原源码，比较完整 prefix 向量的 call、原 argument、parameter、owned
root 声明与 Return；核同轮 source identity、Shared 类型、loan 成员资格、开始/结束 span、
无投影根及真实形成的 owned local。同控制点完整 drop 向量只能是该 owner，preceding_drops=0；
无 borrow binding/handoff/direct-call-end。两入口各自校验，不从任何 checker 产物生成期望。

仅删除上述两项的 Single 存在/计数对照。直接来源全程不结束、混合分支与嵌套 temporary
清理的旧见证保持，不承诺跨事件切片统一全序，也不改变生产、语言或 native 范围。
Mac Rust1.96、locked/offline、默认 debug=2 串行复用已释放 target：两项精确测试各
1 passed、56 filtered；frontend all-targets Clippy（-D warnings）和 849 份 Markdown 门禁通过。
workspace fmt、diff 与相对 78a36f25 的尺寸门禁通过；2149 份手写 Rust，38 份历史超长文件未增长。
未执行全量、含 Abort 整项、native、故障注入或校准；官方 CI 未运行，D10/E2 仍未完成。

后续同合同再迁移 `return_during_later_operand_ends_only_the_established_source_prefix` 和
`nested_auxiliary_return_preserves_source_end_and_temporary_cleanup`，原源码及原 Unit
关联/cleanup 断言保留。prefix 身份完整向量与“无最终绑定、单 owner drop” wrapper 分开；
前两项仍经过同等严格 wrapper。嵌套用既有 direct-call 固定完整向量同时核 temporary
辅助 loan 的参数、调用、Return 与目标；保留 temporary→owner 的 drop 顺序，不推导两个
loan-end 切片间的全序。只替代原 Single 的存在/计数断言，不替代全程 absence 证据。
最终四项精确测试分别 1 passed、56 filtered（共四个不同测试），frontend all-targets
Clippy（-D warnings）通过；没有重复运行全 target。仍不涉及 Abort/native/故障注入/校准。

### Continuation 消费者与保留边界

同批将 `ownership_borrow_continuation::abandoned_initializer_ends_source_call_without_ending_an_unestablished_result`
的原 source(call,argument) ControlTransfer 存在断言迁到真实 Unit prefix 单元素向量；
仍要求最终 result end 仅有一个 AfterStatement，补 Shared、loan 成员资格、原 call/argument/
parameter 文本、origin 与同源 transfer 身份。原输入未改。named-source 测试只有 L0135 的
负例也转 Unit；成功路径的全流唯一结束 Single 证据不动。两入口均保持。
两项精确测试各 1 passed、13 filtered，最终 frontend all-targets Clippy 通过；不运行整目标。

同类审计后明确保留：direct-source/forwarded-source 的全程不结束见证、named-source
成功路径在全流中恰好一个其他实参结束，以及 nested borrow helper 的 outer-end 在
source-end 之前。这些分别需要完整 absence、全流计数及全序；当前 bounded handoff/prefix
观察无法等价替代。没有为扩大退休数量而删断言或将 result endpoint 当作 loan end。

## 同控制点 call-loan 结束顺序观察（2026-11-07）

前述普通 Return 混合辅助/source 切片只证明各自存在，不能证明二者先后。本片为
`UnitDirectCallLoanEndFact` 与 `UnitBorrowCallPrefixEndFact` 增加
`preceding_call_loan_ends()`：同轮分析、同 source-qualified point 下，producer 在当前动作前
实际发布的 `EndCallLoan` 数。两种投影从同一 `iteration_actions` 计数，先计动作再按已有
发布资格过滤，不从 loan descriptor、end_span 或公开切片位置推测。未公开 call-loan 动作
也计入，因此编号可有空隙；不同 point/不同分析的数值不可比较。
仅查询字段扩展；注册、token 退休、unwind、drop 次序、准入与错误/deferred 清空不变。
不含 receiver/result/drop 动作，不是完整统一 cleanup 图、动态执行次数或全局事件流。

已有 nested auxiliary Return 测试先因缺少 getter 编译失败，再以实际动作核
auxiliary=0/source=1；新增双辅助 unwind=0/1/source=2、未公开 Exclusive 动作位于
两观测事件间时 source=2，避免把过滤后切片位置冒充 producer 序号。
sibling Return 各自归零，正常 CallReturn 双实参为0/1；既有 Return/Break/Continue 与
错误/deferred 发布门控保持。前述正常嵌套借用结果 `outer-end < source-end` 的 Single
见证仍不在该范围，本片不退休它或完整 absence/全流计数见证。

本地 Mac Rust1.96、locked/offline、默认debug=2，9项不同普通定向测试通过
（新增prefix_order两项、nested/sibling Return各一项、normal direct一项、三种控制转移
组合一项、direct发布门控两项、borrow发布门控一项）；frontend all-targets Clippy通过。
跨crate workspace all-targets check（LLVM21）、fmt、849份Markdown与diff检查通过；
未运行全target/native/Abort/故障注入/校准。

相对78a36f25尺寸门禁通过：2150份手写Rust、38份历史超长文件未增长；新顺序测试48行。

独立审阅后补 Shared→Exclusive→Shared 的同 Return 用例，公开 direct rank 明确为0、2，
source rank=3，证明 direct 投影也对隐藏动作保留空隙。该精确测试1 passed、59 filtered，
最终frontend all-targets Clippy通过；本片累计10个不同普通定向测试。仅新增测试，
producer未再改动，workspace编译结果仍对应最终producer。

## Callable 原诊断消费者有界迁移（2026-11-07）

基于 main `78a36f25`，`type_callable` 的 named/arity/type/target failures 与 generic
inference conflicts 两项原测试复用既有 MemoryDocument/Rooted helper。原始源码、自定义
环境、完整有序诊断码，以及泛型推导的三个 primary span 均保持不变；不添加 final 或
ownership 前置，不改生产语义。

本次等价性探测发现另两项不能退休：nullable overload ambiguity 的 Unit 结果为
L0123 而原 Single 为 L0124；non-lambda filter 到唯一候选的 lambda L0084 缺原
`callback` label。保留这两项原 Single 测试及所有断言，不用弱化 oracle 换取迁移。
初始迁移不改这两处。后续组合中的 nullable bottom 修复见下一节，callback label 的后续修复与消费者迁移见 [SPEC-0392 §84](0392-compiler-layered-review.md#84-r0r2-调用参数诊断来源与类型身份消费者)。

| 验收项 | 定向检查 | 结果 |
|---|---|---|
| 两项双入口迁移及两项保留见证 | `type_callable` 四个同名过滤器 | 4 passed，17 filtered |
| 受影响测试目标 lint | `cargo clippy -p lang-frontend --test type_callable -- -D warnings` | 通过 |

Mac Rust1.96、locked/offline、默认debug=2，复用空闲target串行执行。未运行含Abort
的整个target、全量、native、故障注入、校准或Linux CI；D10/E2保持in-progress。

## Nullable bottom 的 Unit 诊断一致性（2026-11-07）

基线 main `78a36f25`。原 overload 输入在 Single 报 L0123/L0124，两个 Unit 入口第二项误报
L0123；根因是 Unit 类型适配缺少 Guide 已规定的 Nothing? 到 nullable 规则。
保留原 Single witness，补两个入口的原源码断言及 nullable/nonnullable/不同 nullable 负例，
并锁 deferred-inner 拒绝。完整相容查询移入既有 type_graph 职责模块，driver 不增加超长欠账。

另核实 lambda L0084 并非无 label：Unit 指向整个 callback 参数声明，Single 指向名称。
本片不改该公开参数 span 合同，原 Single 诊断精度见证保留，不宣称该消费者已迁移。

Mac Rust1.96、locked/offline、默认 debug=2，最终 8 项定向测试通过：type_graph 类型关系1项、
type_callable overload过滤5项、nullable正反例1项、ownership_primitives普通normal commit1项。
frontend all-targets Clippy（-D warnings）、fmt、849份Markdown、diff与尺寸门禁通过；
历史38个超长文件数量未增加，checker driver由1182行减至1151行。
未运行全量、native、故障注入、校准或含Abort的整项测试；官方双宿主CI尚未运行，不计为通过。

组合验收（基于 `78a36f25`）：保留四个逻辑提交，仅解决 Spec 追加段落冲突并同步当前事实。
组合源码的 callable 过滤器9项、type_graph过滤器1项通过；frontend all-targets Clippy
（-D warnings）、fmt、849份Markdown与diff检查通过。正式Draft PR的双宿主CI仍待验证。

### 剩余容器普通生命周期消费者（2026-11-07）

基于 main `86430a05`，将 `ownership_containers` 四项原始 fixture 移至 `remaining_unit`：
field-container parent/element overlap、replacement moved-owner 与 commit cleanup、temporary
共享/独占借用及重复分析稳定性、post-index field projection Deferred。保持原始源文本与全部
旧断言，包括 L0135、L0131 的 `list[0]` primary span、AfterReplacement/ReplacedElement、
AfterExpression/CallReturn temporary cleanup、IndexPlace deferred 与空 drop plan。

每项通过 MemoryDocument / Rooted 实际类型最终准入和 P3 分析；验证 SourceUnit、产物同源性，
成功/诊断/Deferred 的 validate 结果分别保留。temporary 在每种入口各自重跑并比较完整 loans/drops
调试快照，不把两入口之间相等当独立 oracle，也不改借用结束 producer 或完整事件见证。

验证：四项 `remaining_unit::` 定向测试通过，frontend all-targets Clippy、fmt、文档及尺寸检查；
未运行 full/native/Abort、故障注入或校准测试。与待合 #350 为不同测试段落，后续保留两批证据整合。
## Map.withValue 泛型借用消费者迁移（2026-11-07）

独立基于 main `78a36f25`，不依赖 callable 或 borrow-prefix 候选合入。
`ownership_map_with::scoped_map_action_generic_borrow_ends_before_owned_map_delivery`
的 String、Packet、Resource、Token? 四份原源码逐字保留，转用既有 MemoryDocument/Rooted
Unit helper。原无诊断、loans≥4 下界、callback 单参数 Borrow/类型等于 value_type、返回
Unit 合同全部保留；还复用同轮 completion/ownership 兼容性与单 descriptor 检查，并核源身份。
删除该文件最后一条 Single name/type/ownership helper 及导入，未改生产或Guide。

这里原测试没有独立 loan-end 事件表、完整动态cleanup或native oracle，不能将本次消费者
退休说成新增上述证明；其他文件中依赖 Single 专有字段或事件顺序的见证保持。
原七种 source invalidation/escape 负例、原 L0087 primary span及普通借用正例不删不改。
最终文件175行（原220行），没有引入新的生产API或改变withValue语义。

Mac Rust1.96、locked/offline、默认debug=2，串行复用释放后的target。四项直接消费者
逐个精确运行各1 passed/3 filtered，包括泛型四实例；frontend all-targets Clippy通过。
fmt、849份Markdown、diff及相对78a36f25尺寸门禁通过（2148份手写Rust、38份旧超长文件未增长）。
不运行全量/native/Abort/故障注入/校准，不以本地测试
替代正式CI，D10/E2仍未完成。

### 容器参数与元素借用消费者迁移（2026-11-07）

`ownership_containers` 的 runtime-length 构造与已知 sibling 元素借用两例，保留原始源文本，
改经现有 MemoryDocument / Rooted 双入口、完成类型准入和实际 P3 检查。前者保留恰好两个
Shared loan；后者保留四个 loan 的 Shared/Shared/Exclusive/Exclusive 顺序、
Known(0)/Known(0)/Known(0)/Known(1) 索引及 named root 的 CallReturn cleanup，
并检查 source-qualified call/root。没有用有界 loan-end 切片替代全流事件表。

验证：上述两项 exact 定向测试各通过；frontend all-targets Clippy、fmt、文档、尺寸与 diff 检查。
本片不运行 full/native/Abort 故障路径；剩余 Single helper 与独立生命周期见证保持原样。

同批后续补迁两项容器 size 消费者：named receiver 的四条 Shared loan、无诊断/无 deferred
与 owner 后续 move，以及 temporary receiver 同步 loan 所指 owner 的恰好一次 CallReturn drop。
两项保留原始源文本，双 Unit 入口新增 call/owner 的 SourceUnit 校验；各自 exact 测试通过。
没有将这两项普通读取路径扩大为完整借用结束事件流或所有 temporary 生命周期的迁移结论。

重传组合验收（2026-11-07）：合入 main `333ba5ec`，仅调和本节与 callable 验收的追加冲突，
双方历史记录完整保留；原 #350 两份测试及 ownership 架构内容未改变。Mac Rust1.96、
locked/offline、默认 debug profile，Map 全4项及上述容器4项 exact 测试通过；两个直接测试
目标 Clippy（-D warnings）、workspace fmt、862份Markdown与diff检查通过。
未运行全量/native/Abort/故障注入/校准，本次精确新头的正式CI须另行确认。

最新主干组合复验（2026-11-07）：合入已含 #349 的 main `86430a05`，双方 Spec 与
architecture 记录完整保留，原 Map/容器测试内容不变。Mac Rust1.96、locked/offline、
默认 debug profile，原8项消费者、ownership_borrow_continuation 全14项、
unit_direct_call_loan_ends 全60项通过，共82项；frontend all-targets Clippy
（-D warnings）、workspace fmt、862份Markdown及diff检查通过。
本次未运行全量/native/故障注入/校准；正式CI须以本次新头单独核验，不沿用旧头绿灯。

后续本地整合 #350 的 `d8125323` 后，容器目标已没有 Single helper 调用，删除其入口和 imports。
原两提交另存已验证 bundle；没有删除或覆盖原分支。45 项容器目标测试全部通过，
frontend all-targets Clippy 与格式/文档/尺寸检查通过；仍不代表其他 ownership 目标全部退役。

## 混合 Return 分支的绑定与终止隔离证据（2026-11-07）

基于 #349 后 main `86430a05`，保留原混合分支源码及 prefix 固定完整向量，补 continuing
分支唯一 binding 的固定字节身份（161–165）、同轮 source qualification、initializer 与
origin 对接；唯一 result endpoint 必须属于该 binding，且为 AfterStatement，不能冒充
Return 上的来源 loan 结束。同控制点的实际 call-loan 前序计数为零。两入口均验证。

审计仍保留 direct/forwarded source 的完整 absence、named-source 全流恰一结束及 nested
outer-end 先于 source-end 的 Single 见证。该混合分支现有 prefix 和 binding 可观察，但
正常 handoff 单元素探针失败（列表为空），因此不把正常调用 handoff 的能力外推到此输入；
本片不扩 producer、不改 Guide、不删除任何 Single，也不宣称完整事件流已实现。

Mac Rust1.96、locked/offline、独立 target、默认 debug=2：
`unit_direct_call_loan_ends` 的
`borrow_handoff::return_during_later_operand_ends_only_the_established_source_prefix` 精确测试
1 passed、59 filtered。保留证据邻控：`ownership_borrow_continuation` 的
`results_continue_original_source_loans_and_never_own_the_payload` 与
`named_source_continues_only_its_loan_and_other_arguments_end_at_call_return` 各 1 passed、
13 filtered；`ownership_nested_borrow_arguments` 的
`direct_and_grouped_results_are_synchronous_borrow_operands` 为 1 passed、9 filtered。
frontend all-targets Clippy（-D warnings）、fmt、862 份 Markdown、diff及相对基线尺寸门禁
通过；2150 份手写 Rust、38 份历史超长文件未增长。未运行全 target、native、Abort、
故障注入或校准；官方 CI 未运行，D10/E2 仍 in-progress，不合并。

## 容器与借用证据集中交付（2026-11-08）

基于 main `60aa67d1`，组合保留 container-remaining `a44a673e` 与 borrow-evidence
`2cb692cf`，代码组合头 `24f5a76c`。仅调和 Spec 追加记录冲突，原分支及历史验收保留；
不包含 #351 的生产修复。独立只读逐项复审未发现可操作 P1/P2，原源码与 oracle 未弱化。

Mac Rust1.96、locked/offline、独立 target、默认 debug profile：初始四目标组合编译在
`unit_direct_call_loan_ends` 链接报 errno=28（No space left on device），exit101，未执行测试。
该宽目标也包含 Abort 源码测试，后续不重跑宽目标；没有把未运行目标计为通过。
复用本轮已编译目标后，`ownership_containers` 全45项、
`ownership_borrow_continuation::results_continue_original_source_loans_and_never_own_the_payload`
与 `ownership_nested_borrow_arguments::direct_and_grouped_results_are_synchronous_borrow_operands`
各精确1项通过，共47项。workspace fmt、862份Markdown、diff及相对main尺寸门禁通过，
2151份手写Rust、38份旧超长文件无新增欠账。

本组合头的 mixed Return 精确测试和 frontend Clippy 因磁盘容量限制未完成；
未运行 native、故障注入、校准、Linux或官方CI。本地历史通过不能替代本组合头验收。
D10/E2仍in-progress，发布只保存工作，不自动合并。

### 合入候选同步（2026-11-08）

#351 已合入后，将当前 main `d8c7393a` 无冲突合入 #352 的 `77b9e727`，保留双方代码与验收。
相对新主干的前端 patch-id 与原 #352 一致，不修改 producer 或扩大 D10/E2 完成范围。
Mac Rust1.96、独立 target、默认 debug profile：`ownership_containers` 全45项和
`unit_direct_call_loan_ends` 全60项通过（合计105项，零失败、零忽略），frontend all-targets
Clippy（locked/offline、-D warnings）、workspace fmt、863份 Markdown 与 diff 检查通过。
旧 [PR CI](https://github.com/Halckon/Koven/actions/runs/42434231041) 已完整通过，但其基线为
`60aa67d1`；合流候选仍须新头正式 CI，不沿用旧绿灯。未在本机追加 native、故障注入或校准。

## Resource 与嵌套 capture 身份消费者迁移（2026-11-08）

基于 main `04583bb2`，测试提交 `cbb02ae0` 将 `ownership_closures` 的两个既有正例
移至 `capture_identity_unit`，原始源码保持逐字一致，两入口逐项执行原 oracle：Resource
与 Copyable 参数的 Shared/Borrow、Owned/Copy/Move 捕获顺序、三种 closure 的
Transferability、资源随 capture 清理且不再由原 named binding 清理，以及 nested lambda
的符号身份与 shadowing 无捕获。新增 MemoryDocument/Rooted、同轮 completion/ownership
兼容性、完整 source-qualified identity 与无 deferred/可验证检查；不改 producer 或 Guide。
独立只读复审指出的预期 lambda 身份反取及 names/types 诊断门弱化均已修复并复核。

Mac、locked/offline、独立 target、默认 debug profile，在最终测试代码上实跑：

| 验收项 | 命令或过滤器 | 实际结果 |
|---|---|---|
| 双入口 Resource / nested capture 原消费者 | `cargo test -p lang-frontend --test ownership_closures capture_identity_unit -- --nocapture` | 2 passed，72 filtered，零失败/忽略 |
| 前端静态检查 | `cargo clippy -p lang-frontend --all-targets -- -D warnings` | 通过 |
| 格式与空白 | `cargo fmt --all -- --check`、`git diff --check` | 通过 |
| 尺寸门禁 | `python3 scripts/check_rust_sizes.py --base 04583bb2` | 通过，2191份手写Rust、35份历史超长文件，无新增欠账 |
| 文档后继 | `python3 scripts/check_docs.py`、`git diff --check` | 通过，863份Markdown |

文档后继不冒称重跑测试。此片不证明动态 Choice/循环 capture 传输、全流 loan-end、
SSA/LLVM/native 或完整复杂所有权；其他 Single owner/loan 见证保留，D10/E2仍进行中。
未运行全量测试、native、故障注入、校准或正式 CI，不创建 PR、不合并。

## 循环共享 capture 的有界安全门禁（2026-11-08）

在 `6dc1ff5c` 上探测原循环 Choice 源码，MemoryDocument/Rooted 均无诊断，却在循环后
`f()` 使用来源之前发布 xs/ys 的 LoopExit drop；break 还出现 BranchExit drop。正确最低
生命周期断言实跑失败，原 Single 三项条件来源见证通过。根因是 If 结果不能进入 planner
的单 lambda 槽，且循环 body_state 未运输到出口。不能靠并集延迟所有来源冒充条件 ASAP。

用户批准先对特定无法证明路径报 L0164。新 `dataflow/loop_capture.rs` 在 for/while/loop
边检查循环外仍 live 的 holder：仅对相对入口新增的环境树中 Shared、MoveOnly、由当前
callable 拥有的来源拒绝；Borrow/Inout 参数、非 owning binding、已建立环境、body-local
且已结束的 callback、无捕获与 Copyable 来源不新增拒绝。失败不发布 cleanup/capture/
iteration 产物。loop 内重新建立且跨边保存的环境仍待完整协议，不提升动态能力。

可独立证明零轮的 literal false（含 Group）及 typed 内建容器 descriptor 确认的零实参
ListForm/EmptyMutableList 保留旧路径；不靠函数名称判断空容器，不新增一般常量折叠、
运行时长度求值或任意条件可达性证明。其他未证明零轮的运输仍可报 L0164，属于明确的
临时精度限制。独立审查发现的 Borrow/Inout 参数误拒已修正并补正控；新模块避免给历史
953行 traversal 再添加整段算法，不扩大尺寸豁免。

### 完整条件 owner/phi 的下一阶段设计边界

此节是待实施合同方向，不是已启用的新产物或能力。环境实例身份必须与静态 lambda
分离，并绑定同次 analysis/source；每个实例持有有序 capture slots、source owner 与有效
条件。Choice 与 loop header/exit phi 必须列全零轮入口、正常/continue 回边及 break/
exhaustion 入边；条件 EndCaptureLoan 与 Drop 共享唯一有序序列，区分仍可用 named owner
和仅由环境保留的 owner。validator 验证入边完整、条件互斥覆盖、slot 活性及恰一次清理。

替换先完整形成 RHS 环境，再退休旧环境并提交新 holder；相同来源仍被新环境借用时不得
出现释放窗口，未选中/旧轮 slot 必须清空。lowering 只消费验证后的 selector、owner、
capture slot 与 phi 边赋值，不从 AST 重推生命周期，不重复执行平面 drop。

后继成功验收须覆盖零轮、一轮 true/false、两轮 TT/TF/FT/FF、continue/break/exhaustion、
RHS 提前 return、shared 同来源、多 holder 与 nested wrapper；证明未选中来源及时释放、
选中来源到最终调用、旧新环境恰一次结束及 loan-before-drop，并补真实 lowering/native。
原 Single 条件 owner 与两轮 replay 全部保留；两个无条件 CallReturn drop 不是等价 oracle。

### 本片验收账本

Mac、locked/offline、独立 target、默认 debug profile；以下均为最终生产代码定向实跑，
不是全量测试或动态运输能力证明：

| 验收项 | 测试目标/过滤器 | 实际结果 |
|---|---|---|
| 双入口 fail-closed 与支持路径矩阵 | `ownership_iteration::loop_choice_unit` | 8 passed，192 filtered；包含五种零轮正控及非空内建/普通函数结果负控 |
| 保留 Single 条件来源与两轮 replay | `ownership_iteration::loop_carried_choices` | 5 passed，195 filtered |
| 首批 Resource/nested capture 消费者 | `ownership_closures::capture_identity_unit` | 2 passed，72 filtered |
| 原 exclusive 回边门禁 | `ownership_closures::exclusive_lifetime::exclusive_capture_loop_backedge_requires_a_proven_loan_state` | 1 passed，73 filtered |
| 工程门禁 | frontend all-targets Clippy `-D warnings`、workspace fmt、diff | 通过 |
| 尺寸门禁 | `check_rust_sizes.py --base 04583bb2` | 通过；2193份手写Rust、35份历史超长文件，无新增欠账 |
| 文档门禁 | `check_docs.py` | 通过，863份Markdown |

未运行其他套件、native、故障注入、校准或正式 CI；未创建 PR、不合并。
D10/E2仍进行中，完整协议只有设计边界，不声称已实施。

## Shared capture 真实来源 slot 前置（2026-11-08）

基于 `9f38c650`，为 conditional owner/phi 提供前置的真实来源输入，不扩展动态 Choice 或
循环准入。原基线 Shared String 本地闭包观测为 Unsupported/Syntax，正确过滤的实际测试
已确认失败；初次误用 exact 导致零项不计通过。新实现仅允许 named environment 的纯
Shared String capture：形成读取当前命名 owner，不移动来源，环境本地 Move 保持实例 token。

真实 EndCaptureLoan action 与观测在同一 helper 发出；关闭观测时仍执行原 action。
结束时读取的 token 必须匹配形成 slot，逆序 End 组完整且紧接同 point 的环境 Drop。
每个环境独立持有 loan 集合，两环境同源不能互相解除保护；活跃 source 不得经 Move、
Drop、Replace、Swap 或 Owned Capture 消费。独立 overlay 和既有位置 replay 互补，
新事件的跨分析事实 Eq 与 V1 投影拒绝同步更新。历史超长 planner 净减少6行，无新尺寸豁免。

真实源码覆盖形成前 replace、两个来源逆序结束、两环境同源、环境 binding Move，以及
closure 结束后的显式 source 使用。借用参数缺 owner、临时/mixed/nested、分支内形成、
活跃 Shared 穿 If、循环的 observation 保持 Unsupported；不把这些拒绝称为语言错误。
原 cleanup/diagnostics/deferred 等全部既有 Analysis 通过同输入 observation on/off 对照，
涵盖准入与拒绝源码。V1 value projection 不接受新事件，循环安全门禁不变。

损坏产品测试已加入 CI：缺/重复/逆序/错point/foreign owner或环境的 End、空/重复 slots、
组内插入和缺环境 Drop；独立 overlay 另测五类消费旁路及只退休一个环境后的源消费。
这些为 CI-only 注入，**本地未执行**，不得把正常源码通过当成这些负控已验。

Mac、locked/offline、独立 target、默认 debug profile；最终代码本地验收如下：

| 验收项 | 目标/过滤器 | 实际结果 |
|---|---|---|
| Shared真实来源槽 | lib `shared_string_environment_has_real_source_owner_slots` | 1 passed，4个真实源码输入 |
| Shared准入边界 | lib `shared_capture_outside_the_named_straight_line_slice_stays_unsupported` | 1 passed，7个真实源码输入 |
| 完整旧产物等价 | lib `shared_capture_observation_preserves_all_existing_cleanup_products` | 1 passed，准入与拒绝11个输入 |
| 原Owned与旧观测邻控 | lib `capture_formation_preserves_replaced_and_forwarded_owner_lineage`、`owner_observation_never_changes_existing_cleanup_products` | 各1 passed |
| 循环保护与首批消费者 | `ownership_iteration::loop_choice_unit`、`ownership_closures::capture_identity_unit` | 8 + 2 passed |
| 工程/消费者编译 | frontend all-targets Clippy `-D warnings`、workspace all-targets check | 通过；LLVM21前缀使用已安装工具链 |
| 格式/尺寸/文档 | workspace fmt、diff、尺寸相对9f38c650、check_docs.py | 通过；2196份Rust、35份历史超长，863份Markdown |

普通定向共15项通过，零忽略；未运行全量、native、CI-only损坏测试或正式CI。
独立只读复审核对真实token输入、loan集合、逆序退休、默认cleanup等价及负控静态逻辑。
本片保存观测协议候选，不宣称 conditional owner/phi 或 D10/E2 完成，不创建PR、不合并。

## Shared slots 跨独立 If（2026-11-08）

基于 `6ba0e742` 的后续片仅放开已有 slots 不变的显式双臂 Unit If。真实源码基线已实跑
失败为 InconsistentTransport；overlay 现在逐臂复制入口 loans，递归保护所有消费路径，
要求出口与入口完整相等且无部分退休，顶层仍从空集合开始并归空。原位置 replay、分支
内形成/退休门禁、cleanup、V1 和循环 L0164 不变；不宣称动态 conditional owner/phi 完成。

| 验收项 | 目标/过滤器 | 实际结果 |
|---|---|---|
| 跨 If 身份与退休顺序 | lib `shared_capture_live_across_independent_if_keeps_source_slots` | 1 passed，4真实输入：独立String、嵌套、双环境同源、无关replace/swap |
| 边界、原槽与全产物等价 | lib `shared_capture_outside_the_named_straight_line_slice_stays_unsupported`、`shared_string_environment_has_real_source_owner_slots`、`shared_capture_observation_preserves_all_existing_cleanup_products`、`owner_observation_never_changes_existing_cleanup_products` | 各1 passed；边界8输入，Shared on/off合计16输入 |
| 循环与消费者邻控 | `ownership_iteration::loop_choice_unit`、`ownership_closures::capture_identity_unit` | 8 + 2 passed |
| 工程检查 | frontend all-targets Clippy `-D warnings`、fmt、相对6ba0e742尺寸、diff | 通过 |

Mac、locked/offline、独立 target、默认 debug profile；普通定向共15项通过。
五种消费旁路追加双臂和嵌套 overlay 负控，属于 CI-only 注入，只编译未本地执行。
未运行全量、native、正式CI；未改变公开产物类型，未追加workspace检查。分支内退休及
单臂return仍拒绝观测；本片仅认证不变的跨边借用身份，不新增语言或lowering执行能力。

## Shared slots 双臂一致退休（2026-11-08）

基于 `50b723aa`，loan overlay 逐臂从相同真实入口状态 replay，仅在完整有序 loan 集合
相等时合流。SharedEnd 不再一律拒绝 edge，改为核对 BranchEntry/Exit 的 control/branch；
位置合流、环境 Drop、逆序完整退休和来源保护仍联合成立。真实双臂退休源码先实跑失败
为 InconsistentTransport，再以最小实现转绿。Shared 形成仍不得进入分支，不实现不等状态
phi、循环运输或新的 cleanup；观察协议不授予 lowering 能力。

| 验收项 | 目标/过滤器 | 实际结果 |
|---|---|---|
| 双臂一致退休 | lib `shared_capture_retires_on_both_if_edges_without_phi` | 1 passed，5输入含嵌套叶边、双环境只退其一、来源后用、多槽逆序 |
| 既有Shared与边界 | lib `shared_capture_live_across_independent_if_keeps_source_slots`、`shared_capture_outside_the_named_straight_line_slice_stays_unsupported`、`shared_string_environment_has_real_source_owner_slots` | 各1 passed |
| 全旧产物等价 | lib `shared_capture_observation_preserves_all_existing_cleanup_products` | 1 passed，20份真实源码开关对照 |
| 循环和消费者 | `ownership_iteration::loop_choice_unit`、`ownership_closures::capture_identity_unit` | 8 + 2 passed |
| 工程与文档 | frontend all-targets Clippy `-D warnings`、fmt、尺寸相对50b723aa、diff、check_docs.py | 通过；2196份Rust、35份历史超长，863份Markdown |

Mac、locked/offline、独立 target、默认 debug profile；普通定向15项。正向直接断言每个臂
及嵌套叶边确实退休原环境，禁止把退休推到合流后；另一仍活跃环境保留独立来源保护。
CI-only 另加单臂丢失/重复/部分End、缺环境Drop、错误分支点及另一环境仍借用时提前消费
来源，编译但未本地执行。未跑全量、native或正式CI，未改公开类型；不新增workspace检查。

## 分支内 Shared 环境完整生命周期（2026-11-08）

基于 `fdae3d3a`，只准入显式 Unit If 内纯 Shared named String 环境的形成与本臂完整退休。
真实源码先实跑失败为 Syntax，最小 planner/replay 修改后转绿。环境定义 created 跨两臂
共同维护，不能复制同一静态环境形成；既有 loans 与 locations 合流约束不放宽，分支产生
的环境不可逃逸。cleanup、语言语义、动态 phi、循环 L0164、V1 与 lowering 能力不变。

真实来源见 `owner_transport_tests/shared_branch_local.rs`：六份准入输入覆盖单臂/双臂、
分支局部String、嵌套、已有外层环境、多来源和同源双环境。逐臂核对形成身份、完整逆序
End组、紧邻环境Drop及来源晚于环境释放。五份边界覆盖逃逸、Owned、mixed、缺else、循环。
静态环境复制到兄弟臂的 CI-only 负控先修正分支point，避免其他校验遮蔽 created 检查；
另有缺End、缺环境Drop、重复End和提前来源Drop。这些注入本地仅编译，未执行。

| 验收项 | 目标/过滤器 | 实际结果 |
|---|---|---|
| 分支内生命周期 | lib `shared_capture_formed_and_retired_inside_if_is_observed` | 1 passed，6份真实输入 |
| 准入边界 | lib `branch_local_shared_capture_does_not_admit_escaping_or_other_kinds` | 1 passed，5份输入 |
| 新片完整产物等价 | lib `branch_local_shared_capture_preserves_full_cleanup_analysis` | 1 passed，11份输入 |
| 原Shared槽/跨If/双臂退休/边界/等价 | lib 对应5个正常源码过滤器，见前述切片表 | 各1 passed；旧Shared等价现为19份输入 |
| 循环与消费者 | `ownership_iteration::loop_choice_unit`、`ownership_closures::capture_identity_unit` | 8 + 2 passed |
| 静态检查 | frontend all-targets Clippy `-D warnings`、fmt、diff、check_docs.py | 通过 |

Mac、locked/offline、独立target、默认debug profile；普通定向18项，独立只读复核未发现漏洞。
未跑全量、native、正式CI或Tree-sitter；未改变公开类型，不追加workspace检查。

## Unit 栈帧候选适配新主线（2026-11-08）

基于已合 PR #351 的 main `d8c7393a`，独立分支 `fix/spec-0341-unit-stack-main`。
旧候选 `341ac068` 保留，不把其旧 expression/flow 实现覆盖到新主线。本片从新
`checker/expression.rs` 提取 Call 分派，保留 `ExpressionExpected` 与成功普通调用后的
`record_pending_call_prefix`；Elvis、逻辑和普通二元分帧，逻辑 left 求值后才进入右侧
pending 处理帧。后半逻辑的 tokens、普通 binary 的原文、求值和错误返回顺序经独立复核。
#351 的 pending edges、rhs_required、bypass、versions、写入失效和诊断/事实发布均保留。

Mac AArch64 / Rust1.96、默认 debug=2、独立target同配置前后反汇编：
main 左结合逻辑链每层为192+16,192+9,776=26,160 B；本片为
192+14,176+480+560=15,408 B（减少41.1%）。96层该链由2,511,360降为
1,479,168 B，未计外层、叶子和后半处理，不是整个程序的栈上界证明。
没有增栈、降输入、改profile或添加ignore；不承诺无限深度。

原Single测试文件未改；新增双Unit `nominal_unit::stress::long_enum_domains_and_deep_smart_cast_conditions_stay_deterministic`
与旧候选逐字一致，保留128 enum cases、96 smart-cast条件、同parsed/names/types重复
检查及enum域128/2断言。此历史Abort输入仅编译，本机明确排除执行；正式
`check_stage_integration.sh` 的 type_checking目标会实际执行，不能把帧缩减当作已实跑转绿。

| 验收项 | 命令或选择 | 实际结果 |
|---|---|---|
| 普通定向邻控 | frontend `type_checking`、`type_callable`、`type_containers`、`type_ownership_primitives`、`control_branch_join`、`while_flow_facts`、`final_type_admission`、`type_iteration`、`ownership_iteration`，locked/offline；统一 `--skip nominal_unit::stress::long_enum_domains_and_deep_smart_cast_conditions_stay_deterministic` | 443 passed；仅新增历史Abort输入1项未执行 |
| 静态门禁 | frontend all-targets Clippy `-D warnings`、fmt、863份Markdown、diff、相对d8c7393a尺寸门禁 | 通过；35个历史超长文件无增长 |
| 压力双入口 | 原128/96，默认线程栈 | 编译通过，正式CI实跑待验 |

没有执行故障注入、Abort、全workspace或native套件；不以旧候选或#351的CI代替本头验证。
未开PR、未合并，D10/E2仍in-progress。

### 栈帧候选与容器借用主干合流（2026-11-08）

#354 原头 `36c0df3c` 的[正式 CI](https://github.com/Halckon/Koven/actions/runs/42438608336)
通过后，合入已含 #352 的 main `4fd948dd`；仅调和本账本追加记录，双方内容保留。
相对新主干的前端 patch-id 与原 #354 一致。Mac Rust1.96、独立 target、默认 debug profile、
locked/offline：原九个普通目标443项、`ownership_containers` 45项及 mixed Return
`borrow_handoff::return_during_later_operand_ends_only_the_established_source_prefix` 精确1项通过，
合计489项。历史Abort压力输入仍在本机精确排除，不修改源码、输入、ignore或线程栈。
frontend all-targets Clippy（-D warnings）、workspace fmt、863份Markdown及diff检查通过。
新组合须重新取得正式PR CI；旧头通过不代替新组合验收，D10/E2仍in-progress。

### PR #365 主线冲突合流（2026-11-08）

保留原合批头 `301dde03`，在独立 worktree 合入 main `d5d3c8f5`。唯一冲突为本账本
追加记录，双方全文保留；所有权生产代码与其原定向测试相对原头无变化。
合流代码在 Mac、locked/offline、原独立target、默认debug profile 下复跑上述18项普通
定向，全部通过；frontend all-targets Clippy `-D warnings`、workspace all-targets check
（已安装LLVM21）、fmt、文档与diff检查通过。未运行CI-only注入、native或Tree-sitter。
新合流头需取得真正 pull_request 事件的完整CI，原push头的skipped不算通过，不自动合并。

### PR #365 旧 Shared 边界 oracle 迁移（2026-11-08）

`df51f7d9` 的正式双平台 CI 在 `capture_boundaries_discard_the_entire_observation_without_language_errors`
失败：[Linux原始失败](https://github.com/Halckon/Koven/actions/runs/42466597814/job/128688333691)。
本机精确复现；根因为 Owned-only 切片仍把纯 Shared String `{ a }` 预期为 Unsupported，
与本批已准入范围冲突，并非本次修改放宽生产行为。

原完整源码逐字保留为 `REPLACED_SHARED_SOURCE`，新正控固定 replacement 后真实 owner、
lambda Unit 身份、唯一 End、相邻同点环境 Drop、来源随后释放及非 Owned 消费/V1拒绝。
该输入也加入 observation on/off 全 Analysis 对照。原负控用仍不支持的 Shared String+Int
混合类型替换，其他四例和整段清空断言不变；不删除输入、不接受多种结果、不添加ignore。

Mac、locked/offline、默认debug、独立target：新正控、原失败边界、两个原Owned真实来源、
Owned分支隔离、Shared全产物等价、分支局部形成、双臂退休共8项普通定向通过。
独立只读复核确认原源码逐字一致且未弱化oracle。未改生产代码或损坏产物负控；
CI-only注入仍未本地执行，新头仍须取得正式双平台CI，不以此次定向代替全量验收。

## 独立修复：名义签名 L0091 来源锚点（2026-11-08）

基线 main `adba71c3`（含 #351）；#356 为 constant while exits，不与本片改动重叠。
Guide 03 的 L0091 允许类型实参表或类型名作为 primary；原 Unit signature 错取首实参，
且用声明 source 解读 use-site TypeRefId。原 `nominal_and_type_parameter_identity_is_known_and_arity_is_exact`
源码的双入口回归修复前实得 `(Int, Left)` 而非 `(Left, Left)`；跨文件回归实得声明文件
`Long` 而非使用文件 `Left`。本片令 emitter 显式接收 Span，nominal 传类型名，type parameter
保持原 argument primary，诊断码、label、错误恢复与 validate 拒绝不变。原 Single 见证完整保留。

| 验收项 | 测试目标/命令 | 实际结果 |
|---|---|---|
| 原源码缺失/多余实参诊断，两 raw Unit 入口 | `type_checking::nominal_arity_spans` | 修复前失败，修复后通过 |
| 跨文件 primary/label source identity、正反输入排列 | `multifile_type_signatures::nominal_arity_primary_stays_in_use_source_under_input_permutation` | 修复前失败，修复后通过 |
| `T<Int>` / `Int<String>` 原锚点邻控与同文件精确位置 | `type_checking::nominal_arity_spans` | 通过 |
| 签名/body/原 Single/诊断共享契约 | `cargo test -p lang-frontend --test type_checking --test multifile_type_signatures --test multifile_type_checking --test diagnostic_model` | 401 项通过，0 失败、0 忽略 |

Mac、Rust 1.96、独立 target、默认 debug profile。frontend all-targets Clippy
（locked/offline、`-D warnings`）、workspace fmt、863份 Markdown、diff 与 Rust 尺寸门禁通过。
独立复审核对 use-site primary、声明 label 与两入口后，补齐邻控和精确字节身份，最终复跑全绿。
仅 Phase 2 诊断锚点修复；不宣称原类型
消费者已整体迁移。enum case TypeRef 的 L0114 差距仍未修复，Single 仍保留；未运行 native、
full suite 或新头 CI，不开 PR、不合并。signatures.rs 原 1036 行旧债，本次改为1033行，
不扩大既有例外或增加职责。

## 独立修复：已解析 enum case 签名位置（2026-11-08）

基线 main `f3d2857e`。Guide 06 的 case TypeRef 仅用于 is/!is；签名、字段、payload、
泛型实参、supertype、bound 等显式位置必须 L0114。原 Single witness
`enum_case_types_preserve_root_payloads_and_are_rejected_outside_type_tests` 保持原样；
同一源码双 Unit 原先无诊断，新增回归已实测失败，再修复为单条 L0114。
本片只识别名称产物中真实 terminal case symbol，不按拼写猜身份，也不依赖 case signature
收集进度；完整 TypeRef 为 primary，root enum 名称为 label，缓存 Error 不授成功资格。

| 验收项 | 测试目标/过滤器 | 实际结果 |
|---|---|---|
| 原 Single 源码双 raw Unit 入口与原 span | `type_checking::enum_type_position::enum_case_signature_rejects_original_single_source_in_both_entries` | 修复前无诊断失败，修复后通过 |
| 字段、返回、nullable、函数参数、泛型、payload、supertype、bound | `type_checking::enum_type_position::enum_case_types_are_rejected_through_nested_signature_positions` | 双入口单条 L0114，无附加诊断 |
| root 签名、is/!is | `type_checking::enum_type_position::enum_root_signatures_and_case_type_tests_remain_valid` | 双入口无诊断、可 validate |
| 默认可见性的双文件来源身份及正反输入排列 | `multifile_type_signatures::enum_case_position_diagnostic_preserves_cross_file_identity_and_input_order` | primary/use source、label/enum source 均正确，拒绝 validate |

Mac Rust1.96、独立 target、默认 debug profile：`cargo test -p lang-frontend --test type_checking
--test multifile_type_signatures --test multifile_type_checking --test diagnostic_model` 全402项
（92/20/281/9）通过，零失败、零忽略；frontend all-targets Clippy（locked/offline、
`-D warnings`）、workspace fmt、863份 Markdown、diff、Rust尺寸门禁及尺寸政策专项47项通过。
最终候选独立复审无阻断。

范围仍有限：探针 `class Holder(val value: Shape.Circle)` 在同文件 `enum class Shape { Circle }`
之前时，名称产物缺 terminal reference，Unit 仍漏 L0114；本片不伪造类型层名称解析。
双文件显式 `public enum class Shape { Circle }` 则在原 main 生产代码与本修复上均报
`MissingDeclarationSymbol`；已用原生产版本复测，不把默认 public 的成功 witness 冒充此形式通过。
这两项继续 open，未删除/修改既有断言，未声称所有 enum 或 Single 消费者退休。

signatures.rs 1036→1041 行仅增加模块及位置门接线，识别逻辑独立子模块；独立复审认可有界
例外并更新尺寸政策。未运行 native、full suite 或新头 CI；不创建 PR、不合并。

## 独立修复：显式可见性的 inline classifier 身份（2026-11-08）

基线 main `6e5934b5`。跨包 `public enum class Shape { Circle(radius: Int), Point }`
合法签名原先实测 `MissingDeclarationSymbol`。inline-layout 读取误把 `Item::Modified`
当 classifier；本片复用既有声明解包，保留原声明/字段/payload 身份，不吞内部错误或绕过布局检查。
不改名称产物、L0114 位置门或可见性政策，不接入其他候选分支。

| 验收项 | 测试目标/过滤器 | 实际结果 |
|---|---|---|
| 跨包 public enum 的 case、payload、来源身份与完整签名排列相等，body 中合法 is | `multifile_type_signatures::enum_identity::explicit_public_enum_preserves_case_signatures_across_files` | 原版 MissingDeclarationSymbol 红测，修复后通过 |
| public/internal/private 的 enum 与 value class 有限布局 | `multifile_type_signatures::enum_identity::explicit_visibility_preserves_finite_inline_components_in_both_entries` | memory/rooted 均无诊断，可 validate |
| 同三可见性的 enum/value class 递归布局 | `multifile_type_signatures::enum_identity::explicit_visibility_does_not_skip_recursive_inline_layout_diagnostics` | 双入口单条 L0116、精确递归 TypeRef 位置，拒绝 validate |

Mac Rust1.96、locked/offline、独立 target、默认 debug profile：`multifile_type_signatures` 22、
`multifile_type_capability_graph` 10、`multifile_type_checking` 281、`type_checking` 89、
`type_copyability` 17，共419项通过，零失败/忽略。frontend all-targets Clippy（`-D warnings`）、
workspace fmt、863份Markdown、diff与相对基线尺寸门禁通过；无新增尺寸欠账。
独立只读复审未发现 P1/P2。
前向 enum case terminal reference 缺口仍 open；已解析 case 的 L0114 修复仍属独立候选，
本片不声称其组合已验。未运行 frontend/workspace 全量、native、Linux 或正式 CI；不合并。

## 独立修复：同文件 enum 前向 case 名称身份（2026-11-08）

基线 main `4ae149a6`。同文件 `Shape.Circle` 使用早于 enum 声明时，局部名称产品尚无
terminal 引用，Unit 遇到已绑定 lexical root 又跳过补全。另已实测 `Shape.Missing` 会被
同名顶层 `class Missing` 接管，错误地消除 L0080。六项定向基线4失败/2通过。
本片仅在 Unit 名称阶段，以 Type namespace 的真实 root SymbolId 及完整 enum case 表
补齐两段 TypeRef 的 terminal，保留 root、scope、symbol/case ID 和其他引用；不在类型层按拼写猜身份。
仅 terminal 的局部引用/旧 L0080 被替换，未知 case 发布单条 L0080 和 Type/Unresolved。

| 验收项 | 测试目标/过滤器 | 实际结果 |
|---|---|---|
| 正反声明顺序、精确 root/case 身份及 Span，memory/rooted | `multifile_name_resolution::forward_enum::local_enum_type_paths_preserve_root_and_case_identity_in_both_orders` | 通过；保留局部 symbol/scope/case 表 |
| 两个 enum 同名 case 由各自 root 选择 | 同模块 `equally_named_cases_are_selected_by_the_bound_root` | 通过 |
| 类型参数遮蔽、值参数同拼写不遮类型 | 同模块 `type_parameter_shadowing_is_not_rebound_to_the_outer_enum`、`same_spelling_value_parameter_does_not_shadow_the_enum_type` | 通过 |
| 缺 case 有/无同名顶层类型，普通裸名不受影响 | 同模块 `missing_local_case_cannot_fall_back_to_a_same_named_top_level_type` | 双顺序/双入口均单条精确 L0080，拒绝 validate |
| 合法前向 is 与完整名称输入排列相等 | 同模块 `forward_enum_type_tests_validate_and_input_permutation_preserves_names` | 名称相等、typed 无诊断并可 validate |

Mac Rust1.96、locked/offline、独立 target、默认 debug profile：`name_resolution` 15、
`multifile_name_resolution` 19、`unit_name_snapshot` 7、`type_checking` 90、
`multifile_type_signatures` 19、`multifile_type_checking` 281，共431项；完整 LSP 56项及
stdio 1项，合计488项通过，零失败/忽略。frontend all-targets Clippy（`-D warnings`）、
workspace fmt、863份Markdown、尺寸与diff检查通过；独立只读复审未发现阻断问题。

额外一次性组合验证：以同基线及本片 qualified.rs，叠加 `f27960e3` 的 L0114 生产/测试差异、
`5d5a68bb` 的声明解包生产差异；3项 enum_type_position 和1项跨文件 case 诊断通过。
嵌套签名探针扩为8种位置×正反声明顺序×默认/public×双入口共64组，均精确 L0114；
跨文件 public enum 正反输入排列保留 primary/use 与 label/enum 来源。这仅为隔离组合证据，
不代表三个分支已集成、发布或取得正式 CI。

空 enum 已由 Guide08/parser L0073 拒绝，本片不追加其恢复诊断；超过两段 TypeRef、
表达式成员路径与 legacy local resolver 不变。未运行 frontend/workspace 全量、native、Linux
或正式 CI；不合并，D10/E2仍in-progress。

## 类型诊断四候选集中集成（2026-11-08）

以已合 #365 的 main `6d87040b69dfc12ae5accacd3a73a2f816d8bb36` 新建本地
`fix/spec-0341-type-candidate-batch`。`git cherry` 核实四源候选均未被主线吸收；
依次保留 `16593a5c`（L0091）、`f27960e3`（L0114）、`5d5a68bb`（修饰器解包）与
`58b6d36c`（前向 case 名称）完整逻辑提交，生产输入归于合批头 `845ed317`。
四笔生产差异的 stable patch-id 分别与源提交相同，不另改语言规则或所有权能力。

冲突仅为测试注册、尾部追加与文档同段/追加位置，生产代码无冲突。原测试源码及断言保留，
不混入主工作区未跟踪脚本或其他候选；原独立验收仍是历史记录，本节才描述组合结果。
已将先前一次性组合探针固化：8种签名位置 × 正反声明顺序 × 默认/public × 双 Unit 入口
共64组，均精确单条 L0114并拒绝validate；双文件默认/public、正反输入排列保留使用处
primary与enum声明label的来源身份。合法is/!is、root签名、遮蔽、缺case与递归布局邻控保留。

首轮扩大到Box邻控发现一个旧oracle不再匹配：原完整 `Box<Point<Int>>` 输入仍产生既有
五个诊断码及五个Error参数，但L0091由旧首实参`Int`转为正确使用点`Point`。
按Guide03合同迁移这一处primary，保留其余四个primary、全部源码与Error断言，补精确
SourceId/字节范围、声明label及validate拒绝。独立复审确认是锚点修复的必要消费者迁移，
未放宽生产行为或接受多种结果。首次475通过/1失败的结果不冒充全绿。

Mac AArch64、Rust/Cargo1.96、locked/offline、默认debug profile、单一独立target、
编译jobs=2，最终组合实跑如下：

| 验收项 | 目标/命令 | 实际结果 |
|---|---|---|
| 名称、签名、body、诊断、能力、原Single与双Unit | frontend `diagnostic_model`、`multifile_name_resolution`、`multifile_type_capability_graph`、`multifile_type_checking`、`multifile_type_signatures`、`name_resolution`、`type_checking`、`type_copyability`、`unit_name_snapshot`；一次`--no-fail-fast`调用 | 9/19/10/281/24/15/94/17/7，共476项通过，零失败/忽略 |
| 名称身份直接下游 | `cargo test --locked --offline -p lang-lsp --no-fail-fast` | 56项及stdio1项，共57项通过，零失败/忽略 |
| 静态检查 | frontend all-targets Clippy `-D warnings`、workspace fmt、`git diff --check` | 通过 |
| 文档与尺寸政策 | `check_docs.py`、`check_rust_sizes.py --base 6d87040b`、`unittest scripts.tests.test_check_rust_sizes` | 863份Markdown、2207份手写Rust/34份历史超长与47项政策测试通过 |

独立只读复审核对真实root/case身份、未知case抑制、Error资格链、修饰器布局及冲突oracle，
最终增量无P1/P2。signatures.rs从main1036行经+5/−3成为1038行，尺寸例外收紧至1038；
architecture合并本片重复段落保持200行，不删除原事实。政策测试初次受只读临时目录限制，
允许创建其测试临时仓库后47项实跑通过，不将初次环境失败算作通过。

`type_checking`精确排除历史Abort输入
`nominal_unit::stress::long_enum_domains_and_deep_smart_cast_conditions_stay_deterministic`
共1项；其源码、规模、ignore与线程栈均未修改，该项只编译，正式CI仍负责实跑。
未运行frontend/workspace全量、native、Linux、损坏注入或新头CI；未改跨crate API，不追加
workspace check。空enum、超过两段本地TypeRef及错误嵌套case实参恢复不新增覆盖。
本批只本地准备，不推送、不创建PR、不合并；D10/E2仍in-progress。

## Unit Return 终止边前置（2026-11-08）

基于已合 #365 的 main `6d87040b`，独立分支 `feature/spec-0341-shared-owner-continuation`。
下一有界合同是显式 Unit If 的单臂 bare Return，另一臂继续。旧真实 Shared 输入本机
先实跑为 Unsupported/Syntax，再引入真实 UnitReturn 事件及两个 replay 的继续/终止状态。
真实 Return 求值、iteration cleanup、drop_all 顺序不变，超长 planner 把该语义块委托到
control helper，净减少行数；观测关闭也执行完整原路径。终止边须清空 owner/loan、无
部分退休与事件后缀；只采用继续臂状态，不求伪 phi。事件事实 Eq 与 V1 拒绝同步更新。

五份 Shared 正控覆盖正反臂、同源双环境、多来源、嵌套。精确核 bare Return 的 AST Unit
身份，终止臂所有 End/Drop 为该 Return 的 ControlTransfer，环境先于来源退休；继续路径
仍实际 Move 环境。原普通 String Return 边界源码完整保留为正控。十二份真实源码做全
Analysis 开关对照（五准入、六边界、原普通返回输入），不把状态变Complete当作cleanup等价。

边界包括带返回值、双终止、return臂新增静态owner、无else、循环和opaque调用，全部仍
拒绝观测。尤其 return 臂新增 owner 尚未归一到另一臂的未形成状态，本片不放宽此项。
CI-only 负控覆盖遗漏End/环境Drop/来源Drop/终止、终止后缀、伪终止、foreign-source Return
及继续臂提前来源消费；本地只编译，不执行注入。无动态条件owner/phi或循环实例交付，
不修改Tree-sitter，不自行外发或合并。

| 验收项 | 目标/过滤器 | 实际结果 |
|---|---|---|
| 终止边与边界 | lib `shared_capture_unit_return_edge_does_not_retire_the_continuation`、`unit_return_keeps_unsupported_terminal_shapes_fail_closed`、`original_plain_return_boundary_keeps_the_surviving_owner_state`、`unit_return_observation_keeps_full_legacy_analysis` | 各1 passed；五Shared正控、六边界、原普通返回输入 |
| 旧分支与Shared邻控 | lib 原分支拒绝、Shared拒绝、双臂退休、跨If、分支局部形成及两种全产物等价过滤器 | 7 passed |
| 循环和消费者 | `ownership_iteration::loop_choice_unit`、`ownership_closures::capture_identity_unit` | 8 + 2 passed |
| 原Return与borrow边 | `ownership_checking::return_cleanup_preserves_declaration_order_after_rebinding`、`ownership_borrow_continuation::children_end_before_parents_and_owner_cleanup_on_scope_and_return_edges` | 各1 passed |
| 工程检查 | frontend all-targets Clippy `-D warnings`、workspace all-targets check（LLVM21）、fmt、diff、尺寸相对6d87040b、check_docs.py | 通过；2203份Rust、34份历史超长，863份Markdown |

Mac、locked/offline、独立target、默认debug profile；普通定向23项通过。独立复核发现并
修复CI-only多文件fixture按索引误选观察的问题，现按source-qualified callable选择，并
核原始consistent及foreign-source差异；最终复核闭合。损坏负控仅编译、未本地执行；
未跑全量、native或正式CI，本片仅本地保存，不推送或创建PR。

## 终止臂局部 owner 静态归一（2026-11-08）

基于本地 `9f84e652`，在原隔离分支推进其明确保留的局部owner缺口。原完整边界源码
先实跑为 InconsistentTransport；最小修复仅处理 continuation=Uncreated、已验证terminal=Dropped
的组合，将其记为静态不可用Dropped。其余继续状态不动，两边Uncreated不变，不伪造
运行时drop、owner或phi。终止完整清理、loan分流、created去重及双终止门禁不变。

四份真实输入保留原boundary并覆盖反向臂、局部Shared环境、嵌套。逐路径确认局部owner
只在终止路径真实绑定/析构，继续路径无其Result/Binding/Move/Drop；外层Shared来源
仍保留至该路径环境结束。十五份源码验证完整Analysis开关等价，含旧准入/拒绝和原
普通Return输入。CI-only负控检查合流后四类token复活、缺局部String Drop/Shared End/
环境Drop、兄弟臂复用静态定义及两臂都未形成的孤立definition；本地仅编译未执行。

| 验收项 | 目标/过滤器 | 实际结果 |
|---|---|---|
| 局部身份静态归一 | lib `retired_return_local_owners_are_unavailable_on_the_continuation` | 1 passed，4源码 |
| 原终止协议和Shared邻控 | 前一切片11个普通lib过滤器（含全Analysis等价） | 11 passed |
| 循环和消费者 | `ownership_iteration::loop_choice_unit`、`ownership_closures::capture_identity_unit` | 8 + 2 passed |
| 原Return/borrow边 | 两个前述原Return/borrow普通过滤器 | 各1 passed |
| 静态检查 | frontend all-targets Clippy `-D warnings`、fmt、diff、相对9f84e652尺寸、check_docs.py | 通过；2203份Rust，34份历史超长 |

Mac、locked/offline、独立target、默认debug profile；普通定向24项，独立只读复核无生产漏洞。
未改公开类型，不重复workspace检查；未跑全量/native/CI-only注入/正式CI或Tree-sitter。
此片仍不实现动态owner phi或循环实例，仅本地保存，不接触借用线补片、不外发或合并。

## Callable expected 参数模式消费者迁移（2026-11-08）

基于 main `f83aff80fd565771eb4873ebf8cfa51b5613ad02`，独立分支
`feature/spec-0341-callable-lambda-mode-consumers`。仅迁移 Phase 2 测试消费入口，
不修改生产、Guide、ownership、lowering 或仍保留的 Single 恢复见证。

- `expected_lambdas_adopt_and_publish_all_parameter_modes` 保持原名称、完整源码、自定义
  十六 builtin/两能力环境及三组固定 oracle。原一次 Single 类型分析由同一 AST 与环境的
  MemoryDocument/Rooted 两次真实 Unit 分析承接；无新增 final/ownership 准入前置。
- 六个命名参数通过真实 source-unit/local symbol 对应唯一 callable signature parameter；
  七个 lambda 参数由同源 body mode 映射取值；七个 lambda function type 使用原 AST
  expression 加真实 unit identity 查询。保留原顺序和 Borrow/Inout/Value 预期，另核唯一
  来源和函数参数 arity，不按参数同名文本配对，也不把缺事实默认为成功。
- 私有 `unit_entries` 提供保留名称结果的窄 helper，原 `checked_entries` 返回结构与检查
  不变；其他顶层测试正文逐字保留。`type_callable` 直接 `check_types` 调用17→16，
  仅说明这一消费者退休，不代表剩余 Single witness 或 D10/E2 已完成。

| 验收项 | 命令或选择 | 实际结果 |
|---|---|---|
| 原入口基线 | 原测试完整名称加 `--exact` | 1 passed |
| 双入口及共享 helper 消费者 | `type_callable`，排除 `standard_error_has_a_stable_abort_identity_and_nothing_result` | 20 passed，1项原Abort witness未执行 |
| 静态质量 | frontend all-targets Clippy `-D warnings`、workspace fmt、docs、尺寸、diff | 全通过；863份Markdown、2203份Rust，34项历史超长无增长 |
| 独立复核 | 原源码与固定 oracle、来源身份、共享 helper | 无阻断，确认原契约完整保全 |

Mac AArch64 / Rust1.96，locked/offline、独立 target、默认 debug profile，编译 jobs=2。
本片没有运行故障注入、Abort、压力输入、frontend/workspace全量、native或正式CI；
测试已编译不代表上述未执行项通过。未推送、未开PR、未合并，D10/E2仍in-progress。


## PR #366 类型、终止边与 callable 合批（2026-11-08）

保留 #366 原头 `cb1de268` 的四项类型修复，在独立
`fix/spec-0341-d10-batch-integration` 分支合入 main `f83aff80`，随后接入
`9f84e652`、`dcb1ffa5` 的终止边/局部 owner 与 `5cde1d3e` 的 expected lambda
参数模式消费者。整合代码头为 `197ebf27d4da43ff79d6b072de6d7948f107fec7`；原逻辑
提交保留，新增切片分别对应 `549a0808`、`27cc5532`、`197ebf27`。

Rust 无冲突，只有本账本的尾部追加冲突，双方完整保留。独立只读复核确认：所有权源码
与 `dcb1ffa5`、callable 测试/helper 与 `5cde1d3e`、四项类型生产路径与 `cb1de268`
逐字一致；parser/type body 保留新 main，实现没有回退 #367 的 callback 来源修复。
原 fixture、诊断/模式 oracle 与 observation on/off 全 Analysis 对照未弱化，未混入其他
未纳入本批的候选。独立复审未发现新的 P1/P2，静态复核不代替执行验收。

Mac AArch64、Rust/Cargo1.96、LLVM21.1.8，locked/offline、默认 debug profile、
单一独立 target、编译 jobs=2，Cargo 门禁串行运行。组合代码实跑如下：

| 验收项 | 目标/选择 | 实际结果 |
|---|---|---|
| 类型、名称与 callable | 前述类型合批9个目标加 `type_callable`，一次 `--no-fail-fast` 调用 | 498 passed，0 failed/ignored；两项历史 Abort witness 精确排除 |
| 所有权普通 lib 见证 | 前述终止边12项普通过滤器，加真实Shared槽、replace来源、分支局部边界及全旧cleanup等价 | 16 passed；其余367项未选中 |
| 循环、消费者与原 Return/borrow | `ownership_iteration::loop_choice_unit`、`ownership_closures::capture_identity_unit` 及前述两个原 Return/borrow 过滤器 | 8 + 2 + 1 + 1 passed |
| 名称身份直接下游 | 完整 `cargo test --locked --offline -p lang-lsp --no-fail-fast` | 56 + stdio 1 passed |
| 跨 crate 与静态质量 | frontend all-targets Clippy `-D warnings`、workspace all-targets check、workspace fmt | 全通过 |
| 文档与尺寸 | `check_docs.py`、尺寸相对 `f83aff80`、尺寸政策47项、diff | 全通过；863份Markdown，2209份手写Rust/34份历史超长 |

Rust 定向共583项通过，不是 frontend/workspace 全量。历史
`nominal_unit::stress::long_enum_domains_and_deep_smart_cast_conditions_stay_deterministic`
与 `standard_error_has_a_stable_abort_identity_and_nothing_result` 只编译未本机执行；
不修改源码、规模、ignore、线程栈或默认 profile。所有 CI-only 损坏产物注入也仅编译，
未跑 native、Linux 或 Tree-sitter。本批继续同一草稿 PR #366 取得新头正式双平台 CI，
旧 `cb1de268` 的 CI 不能代表本组合已通过；不自动合并，D10/E2 仍 in-progress。


## 成功 overload lambda 消费者迁移（2026-11-08）

基于 main `f83aff80fd565771eb4873ebf8cfa51b5613ad02`，独立分支
`feature/spec-0341-overload-lambda-consumers`。与 expected 参数模式切片分别实现，
只修改 Phase 2 测试消费入口；不改生产、Guide、ownership 或 lowering。

- 原 `overload_lambda_trial_commits_only_the_unique_candidate_and_nested_call` 与
  `generic_overload_trials_preserve_named_multi_lambda_modes_and_mapping_order` 的完整
  源码、测试名、重复解析与自定义环境逐字保留。两项各走 MemoryDocument/Rooted；每个
  入口在同一 ParsedFile、inputs、validated names 和环境上连续检查两次，不能用重新
  resolve 替代原 repeated-check 合同。仅观察 raw types，不新增 final/ownership 前置。
- 唯一候选保留两个 Int 返回、calls 相等与唯一 lambda 的 Borrow/Int；泛型候选保留
  源码实参映射 `[2, 0, 1]`、Long 返回与 Inout/Borrow/Value 模式顺序。按真实 source-unit
  和 local symbol 从 modes 表找到唯一原 binding，另核完整诊断、calls、body modes 与
  body symbol types 的重复稳定性，不按同名文本或猜测索引配对。
- 另17项顶层测试正文逐字保留，包含失败/歧义 trial 的 Single Error 恢复见证。
  本文件直接 `check_types` 调用17→14，旧API不删；与其他切片合批后的数量另行清点，
  本变化不代表所有 trial 或 D10/E2 完成。

| 验收项 | 命令或选择 | 实际结果 |
|---|---|---|
| 原入口基线与迁移后邻控 | `type_callable`，排除 `standard_error_has_a_stable_abort_identity_and_nothing_result` | 前后各20 passed，原Abort witness未执行 |
| 静态质量 | frontend all-targets Clippy `-D warnings`、workspace fmt、docs、尺寸、diff | 全通过；863份Markdown、2203份Rust，34项历史超长无增长 |
| 独立只读复核 | 原fixture/oracle、同names重复分析与source-qualified身份 | 无阻断，确认全部原契约保全 |

Mac AArch64 / Rust1.96，locked/offline、独立target、默认debug profile，编译jobs=2。
未运行故障注入、Abort、压力输入、frontend/workspace全量、native或正式CI。
候选只保存在独立本地分支，交给同一PR合批；未自行推送、未合并，D10/E2仍in-progress。


### PR #366 追加消费者与旧 Owned Return 见证迁移（2026-11-08）

在已发布 `e26babd4` 上合入 main `59396543`（#368），追加 `204bdeeb` 的两项成功
overload lambda 消费者，整合为 `0fb8dc50`。仅模块注册、Architecture同段与本账本
追加冲突，双方内容保留；原expected模式helper、失败trial见证与#368全部13文件逐字
保全。`type_callable`直接`check_types`调用由本PR前一头16减至13，不以调用次数代替任务数。

前一头的[正式CI](https://github.com/Halckon/Koven/actions/runs/42478679334)在旧
`capture_release_does_not_admit_noncontinuing_if_edges`断言失败：原完整Owned move
环境单bare Return输入已按新协议Complete，旧oracle仍要求Unsupported/Syntax。
`0fb8dc50`本机精确复现同因，0 passed/1 failed，不能将首次失败算作通过。

修复 `7ea84ebab07c35269df71cb0231081b6d56a3c02` 仅改测试与架构，不再放宽生产。
原 `RETURN_RELEASE` 源码完整保留为正控，固定原参数槽、closure/If/Return的真实Unit
身份，两路径各一次slot→environment释放，独立从AST构造AfterStatement清理点，继续
臂无终止且顶层无重复清理，V1仍拒绝。原负测名改承接完整双return输入，保留精确
Unsupported/Syntax、definitions/events整体清空；两个输入均比较观测开关的完整Analysis。
独立复审确认这是既有准入的oracle迁移，不从当前事件反推预期，不削弱原源码契约。

| 验收项 | 目标/选择 | 实际结果 |
|---|---|---|
| 类型、名称、callable与新main Parser | 前述10目标加 `parser_class_family`、`parser_interface_delegation`，`--no-fail-fast` | 498 + 65 + 15 = 578 passed；两项历史Abort仍精确排除 |
| Owned修复、Shared、终止边与完整cleanup等价 | 前述16个普通lib选择，加新正/负控及全Analysis、既有两项branch release与两项composition邻控 | 23 passed；未选364项，不运行注入 |
| 循环、消费者与原Return/borrow | 前述4目标/过滤器 | 8 + 2 + 1 + 1 = 12 passed |
| LSP直接下游 | 完整LSP及失败stdio的原参数精确复核 | lib 56 passed；stdio首轮10秒响应超时，未改代码/超时阈值的精确复核1 passed（9.57秒） |
| 静态质量与跨crate | frontend all-targets Clippy `-D warnings`、workspace all-targets check、fmt、docs、尺寸相对59396543、diff | 全通过；863份Markdown、2210份手写Rust/34项历史超长 |

Mac AArch64、Rust/Cargo1.96、LLVM21.1.8，locked/offline、同一独立target、默认debug、
jobs=2，门禁串行。最终相关Rust选择累计670项通过，保留上述首次失败/超时记录；stdio
超时原因未定位，不声称已修复或全量通过。类型/外部消费者在`0fb8dc50`实跑，后续仅
私有测试/架构变化，输入等价；23项lib及静态门禁在`7ea84eba`复跑。尺寸政策未改，复用
前一组合47项政策实跑。两项历史Abort、所有CI-only注入仍仅编译，未跑native、Linux、
Tree-sitter或frontend/workspace全量。新头需重新取得正式CI，PR仍草稿，不自动合并。

### 嵌套完成与后参放弃的同根顺序合同（2026-11-08）

基于 `04583bb2`，固定一份原源码：outer borrow initializer 的 source、已正常返回的
`ready(owner)` 与命名参数反排的 `sink(second = owner, first = owner, stop = return)`
共用同一 owned 根。双 Unit 入口锁定完整有界 direct 向量三项及 source-prefix 单项；
已完成 token 只在自身 CallReturn 出现一次，pending tokens 按源码逆序在 Return 结束，
source 的真实同点 rank 为2，owner 只在该 Return 清理一次，未提交 binding/handoff。
不新增全流 API，不改变 Single witnesses 或既有生产语义。

Mac Rust1.96、locked/offline、独立 target、默认 debug：prefix_order 四项、continuation
14项、nested-borrow 10项全部通过；frontend all-targets Clippy、fmt、863份Markdown、
diff及尺寸检查通过。未运行 native、Linux、故障注入或官方CI。

完整流下一实现的真实缺口是正常完成调用后的 call-local pending retain：尚未被有界观察
消费的 token 在此移除；`continue_borrow_call_source` 目前只运输 owner 根保护，不运输
source loan identity。辅助 loan 又存于 handoff 旁表。因此当前 iteration actions 的全集
也不是完整 loan-end 流，禁止拼接现有切片或从 end_span/result endpoint 反推全局事件。
后续应从实际 pending token 建立 continuation 状态，分别闭合同步 consumer、local binding
和 forwarded return 的生命周期及错误原子发布，再逐项退休 Single 证据。

### 真实 source token 的同步 consumer 运输首片（2026-11-08）

同分支在前项合同后新增生产状态运输，不止增加测试。`borrow_transport` 在 inner 正常
返回且同轮 call-use 已验证时，从实际 pending 队列移出唯一 source token，保存进
ValueState；outer 正常返回再匹配并移出真实 consumer token，最后消费所运输的 source。
`UnitBorrowCallTransportFact` 保留两份完整 source-qualified loan 身份、inner handoff 点、
outer return 点及局部 consumer-before-source 顺序。它不从最终 loan descriptor、end_span
或结果 endpoint 生成结束事实；两处移出的 token 原本均会被各自 call-local retain 删除。

首片限无 receiver 的直接声明借用调用、当前存活 owned Shared 根及返回 owned 结果的
直接 declaration/symbol 同步 consumer。Group 只保原参数身份；同根的两份 source 与
命名反排仍按实际 pending 顺序分别运输。辅助 Shared 根资格复用既有检查，排除 parent、
provider、投影和 temporary。新状态随 ValueState 克隆，合流只保全部继续边共有的同一
transport；不修改原 owner 保护、iteration/native cleanup 或调用检查语义。

只有完整正常 consumer 事务发布；未完成事务不代表已释放来源。错误与 deferred 沿两层
原子门清空新产物。此首片不是全局完整流，也不提供跨事务统一顺序或 unwind/source
absence 证明；local binding、forwarded return、receiver/provider 与完整 CFG 运输仍待
后续。既有 direct/forwarded absence、named source 恰一次与 nested 顺序 Single 见证原样保留。

先因新 API 缺失得到编译红测；实现后5项双入口测试通过：direct/Group、同根命名反排的
固定字节身份、未完成 consumer/后置错误、非空 prefix 后 deferred、后参 CFG 关闭边界。
CFG 成功探针实际得到既有 L0164（`view(owner)` 的167–178字节）；改为锁定现行拒绝合同，
双继续与混合 Return 均不宣称已获支持。只读独立审查未发现可确认P1/P2，并核实旧cleanup
不变。dataflow 1123/1248、drop_planner 1557/1669，仅字段/聚合/两处调用接线增长，
算法留独立子模块，既有尺寸额度和拆分计划不变。

Mac Rust1.96、locked/offline、独立 target、默认 debug：direct-call 63项、continuation
14项、nested-borrow 10项，共87项通过。direct-call 明确过滤两个 `abort_` 测试及
`nonreturning_suffix_and_unreachable_calls_publish_no_cleanup`，不计为已通过。
frontend all-targets Clippy（-D warnings）、workspace all-targets check、fmt、文档、
尺寸和diff检查通过。未运行 native、Linux、故障注入或官方CI，不宣称完整流已完成。


### 真实 source token 的 local binding 运输切片（2026-11-08）

接续 `6dbc3c7b`，在同步 main `a884ab8d` 的合流点 `f1c6aea4` 上推进 Phase 3。
直接借用 initializer 正常返回时，从实际 pending 队列移出 source token，放入随
ValueState 克隆的 binding 运输状态；实际 initialized binding 终止时消费该 token，
发布 `UnitBorrowBindingTransportFact`。匹配完整 call、argument 和 origin，保留原 loan
身份及参数 Span，不从结果 endpoint、最终 descriptor 或 end_span 重建 source end。

首片限无 receiver 的直接 declaration initializer、无 parent 的显式 borrow val 及
现存 Shared owned 根；复用辅助来源资格，排除 provider、投影、temporary 与借用父根。
Group initializer 保留关闭边界。已建立的 parent 运输可保留到其子 binding 全部终止；
子来源本身没有因此取得运输事实。未使用 binding 在初始化后结束；既有 if/while 保守
作用域规则不变，同一 token 可在互斥 Return 与继续 scope 路径分别结束，合流只保留
所有继续边共有的 token。未完成 initializer 不发布事务；错误/deferred 清空已完成前缀。

此产物只证明该 source 从真实队列运输到实际 binding 终止，不提供全局 absence、跨事务
统一顺序、完整 loan-end 流或新增 CFG/NLL 能力。既有 child-before-parent/owner cleanup
顺序和 iteration/native 动作不变。forwarded return、receiver/provider、完整同步
consumer unwind 与全部动态路径仍待后续；旧 Single absence/唯一结束见证继续保留。

| 验收项 | 目标/过滤器 | 本组合版本实际结果 |
|---|---|---|
| source 身份、具名同根辅助、父子 binding、同根多 binding、Return、未完成 initializer、Group 边界、原子清空、unused 与 if/while 双出口 | `unit_direct_call_loan_ends::borrow_binding_transport` | 9项双 Unit 入口通过；先以缺失 API 得到编译红测 |
| 直接调用相关回归 | `unit_direct_call_loan_ends`，skip `abort_` 与 `nonreturning_suffix_and_unreachable_calls_publish_no_cleanup` | 72通过、0失败；3项明确未执行，不计通过 |
| 直接调用方 | `ownership_borrow_continuation`、`ownership_nested_borrow_arguments` | 14＋10通过 |
| 公共 API/质量 | frontend all-targets Clippy（-D warnings）、workspace all-targets check、fmt | Mac Rust1.96、locked/offline、独立target、默认debug通过 |

独立只读复审未发现可证实P1/P2，已复核真实队列移除、双层原子门、CFG clone/merge和
最终测试；复审本身未执行Cargo。dataflow 1123→1127/1248、drop_planner
1557→1564/1669，仅字段、聚合和接线增长；算法留92行子模块，原例外额度与拆分计划不变。
文档、diff及相对main尺寸门禁随本提交验证；未运行native、Linux、故障注入或官方CI。
合计96项相关测试通过，不代表D10/E2完成，也不自行合并PR。


### 真实 source token 的 forwarded return 交接切片（2026-11-08）

接续本地 `0adfca46` 的 Phase 3 运输。无 receiver 的直接 declaration 借用调用正常返回时，
从实际 pending 队列移出 Shared 参数根的完整 source token，保存到 ValueState；实际
wrapper Return 或表达式体返回时，再将该 token 交给 `UnitBorrowForwardTransportFact`。
call、argument、origin 与已验证 forwarded/return-origin 同时匹配；Group 保留完整返回
表达式身份。此事实是 live source 的继续转发，不是 source end，也不建立 caller 动态实例
映射。未完成或不可达调用不交接，借用参数不取得 payload owner 或 drop。

边界仍为 Shared 参数根，排除 receiver、投影来源、借用局部、exclusive 来源与活动
iteration provider；合法投影仍可有原 forwarded descriptor，却没有本片运输事实。
同根具名参数不混淆 source 与 auxiliary，多层 wrapper 保持各函数自己的 source 身份；
互斥分支的真实 Return 分别交接。owned auxiliary 的唯一 Drop 仍在 inner CallReturn，
不被推迟到 wrapper Return。原 iteration/native cleanup 不变，旧 Single absence
见证保留，不宣称完整流、跨事务统一顺序或全部 unwind 已完成。

同步 consumer 后参 Return 的成功探针首先得到既有 L0164；本片保留其拒绝合同，没有
放宽 checker 来制造运输事实。forwarded API 缺失时编译红测，补实现后双入口测试通过。
新增 raw Analysis 测试另抓到早期撤销遗漏：外层公开门已保护，但 dataflow finalize 失败
分支未清新表。现已直接清空新表，完整正例前缀后接独立错误/deferred 来源均实测撤销。

| 验收项 | 目标/过滤器 | 本地实际结果 |
|---|---|---|
| 表达式体/Return/Group、同根具名参数、wrapper链与caller隔离、未完成/不可达、投影排除、错误/deferred、分支Return、owned辅助清理 | `unit_direct_call_loan_ends::borrow_forward_transport` | 9项双Unit入口通过 |
| 整族回归 | `unit_direct_call_loan_ends`，skip `abort_` 与 `nonreturning_suffix_and_unreachable_calls_publish_no_cleanup` | 81通过；3项明确未执行 |
| 直接调用方 | `ownership_borrow_continuation`、`ownership_nested_borrow_arguments` | 14＋10通过 |
| raw原子门与邻接表 | `--lib adjacent_borrow_results` | 5通过，含真实非空forwarded前缀撤销 |
| 公共API/质量 | frontend all-targets Clippy（-D warnings）、workspace all-targets check、fmt | Mac Rust1.96、locked/offline、独立target、默认debug通过 |

合计110项相关Rust测试通过，尺寸政策测试47项通过；文档结构、diff和相对main尺寸检查通过。独立只读复审确认raw
清空修复闭环，并核对辅助owner的唯一Drop位置。CompilationUnitOwnership父文件
996→1008重新跨1000软阈值，仅公开导出、字段、successful/deferred门控与getter；
算法留123行子模块。原1156额度、baseline、owner与拆分计划不变，显式补记例外审阅；
dataflow 1127→1132/1248、drop_planner 1564→1572/1669均为有界接线。
未运行native、Linux、故障注入或官方CI；未同步此后推进的main，不宣称新主线组合验收，
也不据此关闭D10/E2。本片仅本地提交，不推送或合并PR。


### 三类运输表的 raw 撤销一致性（2026-11-08）

基于 `6d3885ab`，用同一合法源码真实形成同步 consumer 与 local binding 运输各1条，
再按稳定 source 顺序追加独立错误/deferred 来源。红测直接观察 dataflow::Analysis，
确认旧两张raw表未在finalize失败时撤销；公开层原有门控有效，不是公开产物泄漏。
仅在现有失败分支追加两次clear，使call、binding与forward三类运输表统一撤销。
合法路径与既有条件不变，不改变loan lifetime、cleanup或语言准入。

| 验收项 | 目标/过滤器 | 本地实际结果 |
|---|---|---|
| 两类真实非空前缀、独立错误/deferred与既有邻接表 | `--lib adjacent_borrow_results` | 6通过；新增案例先红后绿 |
| 运输/结束事件相关回归 | `unit_direct_call_loan_ends`，同前片skip3项 | 81通过，3项未执行 |
| 直接调用方 | `ownership_borrow_continuation`、`ownership_nested_borrow_arguments` | 14＋10通过 |
| 质量门禁 | frontend all-targets Clippy（-D warnings）、fmt、文档、尺寸、diff | Mac Rust1.96、locked/offline、独立target、默认debug通过 |

合计111项相关Rust测试通过。独立只读复审确认正例确已形成两条运输，错误/deferred
精确事实与早期撤销均直接验证；typed无效由既有finalize条件覆盖，本片未新增该状态的
非空运输前缀动态用例。dataflow 1132→1134/1248，仅失败门清空接线，无额度增长。
这不是所有raw cleanup表或全局完整流的保证；未运行native、Linux、故障注入、
Tree-sitter或官方CI，未增加workspace检查。仅本地提交，不推送或合并PR。


## 临时父字段 Borrow 的真实结束事件（2026-11-08）

独立基线 main `59396543e2bf29e7a739cd892926586b14ca86b1`。Phase 3 观测切片复用
既有 Field/Group pending-owner 证明，只接普通 owned-result、无 receiver 调用的 Shared
`Temporary(parent)`，并核完整父的 MoveOnly 类别。正常返回移除并发布原 pending token，
return/break/continue 沿原 loop-depth unwind；不新增 token，不改注册、drop 或借用准入。

失败先行实跑：`temporary_field_parent_drops_once_after_normal_call_return` 在未改生产代码
时得到空事件向量，独立 AST 期望为唯一父 token 的 CallReturn；原完整 drop oracle 已通过。
修复后原 13 项临时字段测试保留源码与完整父 drop 顺序，并补完整有序结束向量。另核工厂
返回父、已返回嵌套调用不重复退休、隐藏 Exclusive 的同点序号、for-provider 清理顺序及
错误/deferred 撤销。Rooted/MemoryDocument 双入口与既有 Single MemberReceiver witness 保留。

Copyable 父、普通借用结果、receiver、provider 来源、Rc.value 与其后字段均不扩张；
Index 后字段保持精确 IndexPlace deferred。旧 §105/§112 的空观测描述是当时事实，本节
仅接其 Shared 临时父字段的真实结束事件，不表示完整 loan-end、NLL、Single 退役或 P4/native。

| 验收项 | 命令或选择 | 实际结果 |
|---|---|---|
| 失败先行 | direct suite 的上述精确测试 | 修复前 1 failed，空事件向量与非空固定期望不符 |
| 字段与相邻事件 | `unit_direct_call_loan_ends`，整项排除 `exclusive_prefix::abort_and_normal_return_do_not_invent_prefix_unwind_events` 与 `temporary_field_pending::abort_does_not_unwind_the_temporary_field_parent` | 66 passed；2 项仅编译，未本地执行 |
| range 邻控 | `ownership_range_construction` | 14 passed，无过滤 |
| Unit iteration 邻控 | `multifile_ownership_checking` 的 `iteration::`，整项排除 `iteration::nested_while_jumps_preserve_outer_provider_and_abort_has_no_cleanup_edge` | 20 passed；该 Abort 整项仅编译，未本地执行 |
| pending lifetime | 同 target 的 `pending_lifetime::unit_pending_borrow_owner_survives_branches_and_nested_calls`，exact | 1 passed |
| 静态门禁 | frontend all-targets Clippy `-D warnings`、workspace fmt、文档、相对基线尺寸及 diff 检查 | 通过；863 Markdown、2203 Rust、34 个旧超长文件无增长 |

本地 Mac Rust 1.96.0、locked/offline、独立 target、默认 debug profile，101 项实际通过。
两轮独立只读复审核完整父资格、真实 token 退休、顺序、循环深度和错误/deferred 能力门，
无生产 blocker；公开查询注释已同步，Rc/Index 邻控已补。
未运行全量 frontend/workspace、native、含 Abort 整项或故障注入；本候选只本地提交，
同 PR 组合后仍须精确发布头官方 CI，D10/E2 保持 in-progress。


## Deinit 只读能力双入口拒绝合同（2026-11-08）

基于 main `59396543e2bf29e7a739cd892926586b14ca86b1`，独立分支
`feature/spec-0341-deinit-rejection-contracts`。这是 Phase 3 原合同补全，不是生产修复或
Single 消费者退休；不涉及在途 callable/类型组合、capture Return 边或借用撤销补片。

- 仅在 `ownership_resource_deinit/unit_plans.rs` 追加一项测试，原文件前缀与全部既有
  测试保持原样。两份源码逐字复用 `deinit_cannot_mutate_fields_or_establish_exclusive_reborrows`
  的字段赋值和 Inout 实参，不改变原 Single L0135 / Unit L0134 差异见证。
- MemoryDocument/Rooted 共享同一原 SourceMap/AST/标准环境，各自核入口身份并经过
  原 basic typed gate。独立源码定位完整 L0134：赋值 primary 为 `this.value`，Inout为
  `&`；两者 immutable label 指向 class 字段 `value`，Inout 再保留所选参数 label。
  固定 severity/message/完整 details 及顺序，不从任一分析结果生成另一入口的期望。
- 同时核 typed/owned 兼容、无 deferred、无可执行 drops，以及 owned validate 拒绝后
  完整诊断与空 drop 表保全。不扩大到所有失败程序的 loan-end 或清理撤销合同。
- 首轮新 oracle 错把实际早期 immutable-place 拒绝当成后续 receiver-capability 分支，
  精确测试真实失败；核 assignment/call-contract 及 marker 来源后纠正新预期并转绿。
  没有修改生产以迎合预期，也不把该测试构造错误记为已修复编译器缺陷。

| 验收项 | 命令或选择 | 实际结果 |
|---|---|---|
| 新合同 | `ownership_resource_deinit::unit_plans::readonly_deinit_rejection_keeps_exact_diagnostics_in_both_unit_entries` 精确选择 | 1 passed，两个原输入×双Unit；首轮错误预期失败保留 |
| 普通正反邻控 | `ownership_resource_deinit` 与 `resource_deinit_type_facts` 的 `deinit` 过滤器，先核list，再 `--no-fail-fast` 实跑 | 4 + 6 passed，零失败/忽略；其余27 + 5项未执行 |
| 静态质量 | frontend all-targets Clippy `-D warnings`、workspace fmt、docs、尺寸、diff | 全通过；863份Markdown、2202份Rust，34项历史超长无增长 |
| 独立复核 | 原源码保全、真实早期拒绝路径、独立span与完整诊断oracle | 最终修正版无阻断 |

Mac AArch64 / Rust1.96，locked/offline、独立target、默认debug profile，编译jobs=2。
未运行故障注入、Abort、压力、frontend/workspace全量、native或正式CI。
候选保持独立本地提交，交后续同PR合批；未推送、未合并，D10/E2仍in-progress。


## PR #366 借用运输、临时字段与 deinit 合批（2026-11-08）

以已发布 `d0fb233f` 和 main `59396543` 为基线，在隔离分支顺序接入
`0c4cb6de`、`6dbc3c7b`、`0adfca46`、`6d3885ab`、`baabbe2e` 的完整借用运输链，
再接临时父字段 `af2a3443` 与 deinit 合同 `7de99313`，组合头 `2dec24d2`。
`baabbe2e` 只补撤销，不能单独代替其 API/状态运输前置；旧 main 合流已是当前祖先，
不重复引入。Rust 冲突仅为 direct 测试模块注册，双方全部保留；七份 Spec 追加记录均
逐字保留一次。原候选算法与原 #366 owner Return 实现不变，deinit 仍只是新增合同。

独立只读复核确认：call/binding/forward 三类真实 source token 运输与临时完整父字段
的 direct-end 资格互斥，不会争抢同一 token；`drop_all` 在原清理前交接 forwarded
source，单臂 Return 只合并继续路径。三表 raw finalize、analysis 与公开成功/deferred
门控全部保留。上述有界产物不能拼成全局完整 loan-end 流或跨表统一顺序。

本批另在 `borrow_transport::source_transport_and_temporary_field_end_share_a_completed_consumer`
加入同一 `read(view(owner), Holder("extra").text)` 的双入口组合见证。独立从源码 AST
固定三条真实 loan 身份、各自终点、唯一临时字段 direct end，以及完整父和来源 owner
各在 outer CallReturn 清理一次；不改变原输入或承诺跨 API 顺序。首轮新 oracle 错将
named owner 的 `value_origin` 当成 initializer，0 passed/1 failed；沿 `local_variable`
的 `marker_span(name)` 及公开“诊断位置”合同核实后，仅改为独立 AST 声明名 Span。
临时父仍用完整父表达式 Span，修正后通过；这是新测试预期错误，不是生产清理修复。
最终独立复核无阻断，原候选见证和历史失败记录保留。

| 验收项 | 目标/选择 | 本组合实际结果 |
|---|---|---|
| source 运输与临时字段 | `unit_direct_call_loan_ends`，精确排除下述3项 | 90 passed，含新增交叉见证；原89项邻控保留 |
| 原调用方与范围 | `ownership_borrow_continuation`、`ownership_nested_borrow_arguments`、`ownership_range_construction` | 14 + 10 + 14 passed |
| Unit provider/pending | `multifile_ownership_checking` 的 `iteration::` 与原 pending lifetime，排除下述1项 | 20 + 1 passed |
| raw 撤销与原 owner 合同 | 6项 `adjacent_borrow_results`、前批23个普通 lib 见证及 Exclusive Return 前缀全产物邻控 | 30 passed；359项未选，不运行注入 |
| 原 owner 消费者 | `ownership_iteration::loop_choice_unit`、`ownership_closures::capture_identity_unit`、原 declaration-order Return | 8 + 2 + 1 passed |
| deinit 拒绝与正反邻控 | `ownership_resource_deinit`、`resource_deinit_type_facts` 的 `deinit` | 4 + 6 passed；原 Single 差异合同不变 |
| 静态与跨 crate | frontend all-targets Clippy `-D warnings`、workspace all-targets check、fmt、docs、尺寸、diff | 全通过；863份Markdown、2217份手写Rust、35项超1000，尺寸政策47项重跑通过 |

Mac AArch64、Rust/Cargo1.96、LLVM21.1.8，locked/offline、默认debug、复用同一独立
目标目录且 Cargo 串行。最终相关 Rust 选择200项通过，不累计重复复跑为额外覆盖。
生产头 `2dec24d2` 上199项已通过，随后只追加上述私有测试；最后90项direct族与
Clippy/workspace/fmt复跑。`compilation_unit.rs` 回到1009行，沿用既有1156例外；
独立复核确认仅三类产物接线与临时字段注释，没有父文件算法增长或提高额度，不能把
35项超限写成原34项未增长。

本机整项不执行 direct 中 `exclusive_prefix::abort_and_normal_return_do_not_invent_prefix_unwind_events`、
`temporary_field_pending::abort_does_not_unwind_the_temporary_field_parent`、
`nonreturning_suffix_and_unreachable_calls_publish_no_cleanup`，以及 multifile 中
`iteration::nested_while_jumps_preserve_outer_provider_and_abort_has_no_cleanup_edge`；
全部只编译。前批两个历史Abort/压力见证与所有CI-only注入也未在本轮本机执行，未跑
frontend/workspace全量测试、native、Linux或Tree-sitter。前批类型/LSP数字不冒充本轮重跑。
旧头 `d0fb233f` 的[正式CI](https://github.com/Halckon/Koven/actions/runs/42480287069)
完整成功后才发布本批，避免中断在途CI；该绿灯不代表本批新头已通过。仍保持同一草稿
PR #366，等待精确新头正式CI，不自动合并，D10/E2仍in-progress。


### 发布前同步 #369 的 CaptureIndex 与 callable 主线（2026-11-08）

交叉见证与上述验收记录保存为 `c4a4ce6b` 后，推送前守卫发现 main 刚由 `59396543`
推进至 `6d315265`（#369），因此在任何 push 之前停止。随后无冲突合入新 main，
组合代码头 `0af0949c1716809d9e0188610e7a71d029b08dee`，未反复发布或取消在途CI。

独立只读复核确认13文件主线增量保全：CaptureIndex 按 source-qualified lambda 分组并
保留 descriptor ordinal；dataflow 的三运输表及两层撤销门未回退，owner 全Analysis
helper 只迁索引构造。main 的标准error/external两个消费者与本PR三个callable消费者
均保留，三个helper及模块注册不变；direct `check_types` 调用现为9，不计作任务数。
原标准error虽已迁Unit，仍按既有本机Abort边界整项不执行。

| 验收项 | 新组合实际选择 | 结果 |
|---|---|---|
| 借用/字段与callable直接消费者 | direct-call、callable、continuation、nested-borrow，沿用原Abort排除 | 90 + 20 + 14 + 10 = 134 passed |
| raw撤销、owner全产物和索引 | 前述30项普通lib，加真实两来源32/64 lambda的 `capture_queries_visit_only_the_requested_lambda_in_each_source` | 31 passed；不是注入，未宽选formed_tests |
| 捕获形成/错误/替换与loop诊断 | 两个capture身份、3个closure邻控、1个multifile错误邻控、loop_choice八项 | 5 + 1 + 8 = 14 passed |
| factory身份 | `callable_provenance::contract` 下新source-unit隔离与原capturing-return summary两项，exact | 2 passed；首次错误模块过滤未命中，未把零命中算通过 |
| 静态质量 | 默认frontend all-targets Clippy `-D warnings`、workspace all-targets check、fmt、docs、相对新main尺寸、diff | 全通过；863 Markdown、2217 Rust、35项超1000，原额度不变 |

本次新主线组合实跑181项（134 + 31 + 14 + 2），不把原基线200项与本轮重复选择相加
为381项覆盖。尺寸政策脚本未改，复用本批47项成功结果；未重跑无关类型/Parser/LSP
全套、deinit/range等既有选择，亦未跑全量、native或CI-only注入。两份新后续切片
（借用序号、双终止owner）不在本次冻结批内。旧头旧base的绿灯不能证明本组合新base
通过，仍须同一草稿PR的精确新头正式CI；不自动合并。

## 双 bare Return 的终止闭合（2026-11-08）

基于本地合批 `2dec24d2`，隔离分支 `feature/spec-0341-terminal-owner-edges` 推进 Phase 3
owner observation。原 Shared 双 Return 完整源码先实际得到 Unsupported/Syntax；再接纳
显式 If 的 Nothing 结果，由真实 bare Return 事件闭合两臂。两 replay 各自传播终止并拒绝
后缀；主位置只归一 Dropped/Uncreated，不生成动态 join、phi 或额外清理。

原 Shared/Owned 双终止源码逐字保留为正控。八份真实输入覆盖同源多环境、多来源、嵌套、
外层继续及两侧终止臂局部定义；逐项对照 AST If/Return 身份和每条路径实际形成集合的
恰一次清理。Owned alias 保持原 AfterStatement 槽/环境释放点。四份边界保留带值返回、
隐式 else、循环和臂内 Owned capture 拒绝；完整 Analysis 开关对比不只比较数量。

独立复审指出两 replay 的组合负控可能互相遮蔽，已增加各自直接入口的终止传播、后缀与
缺清理负控并完成静态复核。这些损坏产物测试只编译，未本地执行。事件 gate 不重判 typed
AST；真实 Return 的身份与完整性由 producer 和源码正控约束，不宣称损坏图可反推源码。

| 验收 | 目标/命令 | 本机实际结果 |
|---|---|---|
| 新终止与原合同 | lib 16项普通精确选择：`terminal_return` 三项正控、`terminal_if_observation_preserves_full_legacy_cleanup_analysis`；既有12项单 Return、局部 owner、捕获槽和完整 cleanup 普通合同 | 16 passed；新增8准入+4拒绝源码，零失败/忽略；CI-only负控仅 `--lib --no-run` 编译 |
| 原循环与消费者 | `ownership_iteration::loop_choice_unit`、`ownership_closures::capture_identity_unit` | 8 + 2 passed |
| 原 Return 与借用顺序 | `ownership_checking::return_cleanup_preserves_declaration_order_after_rebinding`、`ownership_borrow_continuation::children_end_before_parents_and_owner_cleanup_on_scope_and_return_edges` | 1 + 1 passed |
| 严格静态检查 | frontend `clippy --locked --offline --all-targets -- -W clippy::all -D warnings`；fmt、docs、尺寸 `--base 2dec24d2`、diff | passed；全部修改 Rust 文件低于1000行，未新增尺寸例外 |

Mac 使用既有 target 串行、jobs=2、默认 debug profile。首轮常规 all-targets Clippy 报现有
borrow transport 三个 API 缺失，源码均存在，`--lib -vv` 却显示 Fresh；显式开启默认
`clippy::all` 并保持严格 warnings 后全目标重核通过。未删缓存或改源码来消除该错误；
首轮失败保留，跨 worktree 复用产物的关联不等于完整根因证明。

本片28项普通定向通过，不代表 frontend 全量；未跑 workspace、native、Linux、正式CI
或损坏注入。主线随后新增的 CaptureIndex 合流须在最终组合头重验，不能复用此旧基底
作为组合证明。V1、动态条件 owner/phi、循环实例及 D10/E2仍未完成；仅本地提交，不推送。


## Exclusive Return-prefix 同点顺序（2026-11-08）

基于 main `f83aff80fd565771eb4873ebf8cfa51b5613ad02` 的独立切片，只为已有
`UnitExclusiveCallPrefixEndFact` 追加 `preceding_call_loan_ends`。从真实 planner 动作
逐 Return point 计数全部 `EndCallLoan`，再筛选原 Exclusive 准入边界；与已有 direct
Shared 和 borrow-prefix 的同点序号直接对应，未公开 token 保留序号空隙。
不改 loan 注册、退休、cleanup、语言准入或 CFG，不把同点序号称为全局动态顺序。

新增 `unit_direct_call_loan_ends::exclusive_order` 保留原 mixed 输入全文，固定三切片的
完整投影顺序；另覆盖隐藏字段 token、双 Return 独立归零、前序正常 Exclusive 调用不
泄漏及已形成非空前缀后接错误/deferred 的撤销。原 prefix_order、Exclusive 边界及
全部 Single witness 保持不变；receiver、result、drop 与完整流仍是后续责任。

旧生产实现配新测试时，精确 target 编译以 E0599 失败，证明原公开 API 不存在；
补充实现后4项新测试通过，不把编译红测称为原有 cleanup 行为缺陷。原 mixed 源码
277字节与既有见证逐字一致；独立期望不从任何分析结果反推，归并前另核各切片原发布
顺序，防止排序掩盖重复或乱序。双入口 key、实参/参数/point Span及原loan身份均核验。

| 验收项 | 命令或选择 | 本地实际结果 |
|---|---|---|
| 新三切片顺序合同 | `unit_direct_call_loan_ends::exclusive_order` | 4通过，Rooted/MemoryDocument双入口 |
| 普通事件整族 | `unit_direct_call_loan_ends`，排除两个 `abort_` 整项及 `nonreturning_suffix_and_unreachable_calls_publish_no_cleanup` | 61通过，含上述4新项；3项仅编译 |
| 直接消费者 | `ownership_borrow_continuation`、`ownership_nested_borrow_arguments`，后者排除含Abort的 `inout_receiver_and_nonreturning_sources_do_not_publish_call_uses` 整项 | 14＋9通过；1项仅编译 |
| API消费者与工程检查 | frontend all-targets Clippy（-D warnings）、workspace all-targets check、fmt | 通过，使用已安装LLVM21 |
| 文档与尺寸 | check_docs.py、相对f83aff80尺寸门、diff | 通过，863份Markdown；34个旧超长文件无增长 |

Mac AArch64 / Rust1.96、locked/offline、默认debug profile、jobs=2，协调后串行复用
既有target，未创建新target或清理用户文件。84个唯一普通测试通过；不把新项单跑重复
计入总数。独立只读复审核真实动作计数、source/point桶、oracle、原边界与旧见证保全，
无阻断；其后仅追加双入口key和原发布顺序断言并实跑转绿。
未运行全量frontend/workspace测试、native、故障注入、Abort、Linux或正式CI；旧Single
见证未退休。仅本地切片，不推送或合并，最终组合与正式CI另验，D10/E2仍in-progress。



## Shared receiver Return 前缀的实际结束观测（2026-11-08）

基于 PR #366 的 `94c1960cda27faa95b57cf516b694b38aa7c781c`，在独立分支
`feature/spec-0341-receiver-prefix-ends` 实施 Phase 3 有界观测。先核已有运输/撤销和
临时父字段已纳入基底；独立 `e836a9fa` 的 Exclusive 顺序片没有重复引入。

新 `receiver_call_prefix_ends()` 只读取 planner 实际 `EndReceiverLoan` 动作，保留
真实 receiver source/call/target identity，不重建 descriptor 结束位置、不修改 cleanup。
仅 Return 放弃调用前缀、Shared receiver、Owned-result 方法和 owned Name/Group 根准入。
同点计数涵盖筛选外真实 call/receiver ends；不同 Return 点单独计数。normal return、
break/continue、reservation、this、temporary、投影、provider、借用绑定和 borrowed-result
方法仍不由本片覆盖。raw finalize 与公开 success/deferred 门共同撤销失败前缀。

先以缺失新 API 的两条 E0599 取得真实编译红测；共享 target 首轮误用另一 worktree
旧 lib 导致的额外缺失 API 不计证据，触碰本树入口 mtime 后已重编正确来源，不清理 target。
追加边界时曾误把 `borrow ... from this` 方法当作已支持输入，8 passed/1 failed；
保留原源码并按既有两条 L0164 的顺序、消息、source/Span/label 固定拒绝 oracle，未改生产。
另加入从参数借用的正常准入方法与放弃 initializer 正控，独立验证 Owned-result 过滤，
不以错误门清空代替过滤证明。上述失败保留，不把首轮当作通过。

| 验收项 | 目标/选择 | 实际结果 |
|---|---|---|
| receiver identity、嵌套/同根、过滤外rank、不同Return、排除与撤销；原direct邻控 | `unit_direct_call_loan_ends`，精确排除下述4项 | 101 passed，含11项新增普通双Unit见证；4项只编译 |
| receiver回边原合同 | `ownership_receiver_backedge` 全目标 | 4 passed |
| raw非空前缀后真实错误/deferred撤销 | `--lib ownership_checking::compilation_unit::dataflow::formed_tests::adjacent_borrow_results::` | 7 passed，384未选；无故障注入 |
| 既有receiver准入与两阶段能力 | `multifile_ownership_checking receiver_admission::` | 10 passed |
| receiver先于provider元素终止 | `multifile_ownership_checking iteration::return_ends_pending_member_receiver_before_its_iteration_element --exact` | 1 passed |
| 静态与跨crate | frontend all-targets Clippy `-D warnings`、workspace all-targets check、fmt、docs、尺寸、diff | 全通过；863 Markdown、2219 Rust、35项历史超1000，原尺寸额度不变 |

Mac AArch64、Rust/Cargo1.96、LLVM21.1.8，locked/offline、默认debug、jobs=2，
复用 `/tmp/koven-d10-batch-target` 且 Cargo 串行。最终行为选择共123项通过，不累加
同一选择的重复复跑。独立只读审查及末项正常准入过滤正控复核均未发现P1/P2。
三个历史超长父文件仅增加导出、存储、聚合与发布接线，分别1009→1022、1131→1136、
1557→1562行，沿用既有上限；筛选算法在独立模块，不提高尺寸额度。

本机整项排除 `exclusive_prefix::abort_and_normal_return_do_not_invent_prefix_unwind_events`、
`temporary_field_pending::abort_does_not_unwind_the_temporary_field_parent`、
`nonreturning_suffix_and_unreachable_calls_publish_no_cleanup` 及新增
`receiver_prefix::abort_does_not_publish_receiver_prefix_ends`，仅编译、交正式CI运行。
未跑frontend/workspace全量测试、native、Linux、故障注入或新头正式CI；原Single完整流
见证不变，不能据此证明全局source absence或关闭D10/E2。本候选只本地提交，后续统一
整合验收，不自行推送、开PR或合并。

## 循环退出消费者的双 Unit 合同（2026-11-08）

基于 main `6d315265`，独立分支 `feature/spec-0341-iteration-exit-contracts`；已核
#366 合批头 `94c1960c` 与本地 `42715890`，本片不重复双终止观察或借用运输实现。
仅改 Phase 3 测试消费入口与事实文档，不改生产、Guide、循环准入或 cleanup 算法。

- 从 `ownership_iteration/control_exits.rs` 退休四个纯 temporary/provider 的 Single 消费者，
  原源码、三种跳转、嵌套清理次序及有序退出 oracle 迁入 `control_exits_unit.rs`。
  每份源码分别经 MemoryDocument/Rooted 的真实 names/types/completion/ownership；同 typed
  重查核完整产物确定性，source-qualified ID 由原 AST 和实际 index 构造。
- 原 helper 对 named drop 的 owner identity 要求不能用 Unit target/Span 替代。因此
  nested Return 的 local/outer 与 named source break 两个 Single 原见证完整保留，只补
  双 Unit 正控。前者另核真实 Return、两 provider 的完整同点 actions 及按点查询；
  “消费一次”只指前端计划合同，不冒称 native 动态执行或析构证明。
- 四个退休用例新增无 named drop 断言，不误删非空 identity 义务。原 conditional source
  owner 表、循环 phi/replay 与其它 Single 恢复见证不变；D10/E2 保持 in-progress。

Mac AArch64 / Rust1.96，locked/offline、默认 debug profile、jobs=2，共享 target 串行；
切换工作树时使当前 frontend 重新编译，核日志中的实际源路径，避免复用另一候选源码。

| 验收项 | 命令或选择 | 实际结果 |
|---|---|---|
| 原基线六项 | `ownership_iteration::control_exits` 六个精确测试 | main `6d315265` 上 6 passed |
| 双 Unit 与邻控 | `control_exits_unit`、`conditional_source_unit`、`loop_choice_unit`，以及两个保留 Single 精确测试 | 最终修正版 6+1+8+2=17 passed，零失败/忽略；不重复累加首轮结果 |
| 静态门禁 | workspace fmt、frontend all-targets Clippy `-D warnings`、docs、diff | 通过；863 份 Markdown |
| 尺寸门禁 | `python3 scripts/check_rust_sizes.py --base 6d315265` | 通过；2203 份手写 Rust、34 项历史超长，无新增债 |

独立只读复审核原 fixture/oracle、Single 保留范围、source-qualified 身份与同点完整清理；
新 helper 遗漏的 names 全诊断空断言已补回，独立复核及修改后 17 项复跑通过。未执行全 frontend/workspace、native、Linux、
损坏产物注入、Abort 或正式 CI；本片只本地保存，不推送、不创建或合并 PR。

## 终止 owner 与借用顺序候选复用整合（2026-11-08）

本轮输入为 main `65eaf1a736b7060416ff25ffc5233eb6ee473855` 上的组合头 `da0f99cd`，
加下述混合交叉测试；复用 `42715890`、`e836a9fa`、`a470460b`、`0f03952f`。
既有 #366 运输/临时父字段和 #369 CaptureIndex 已在基底，不重复实现；类型消费者
另批交付。本轮结果只证明该输入，后继 main 变化仍须重新核对与验证。

Exclusive冲突保留当前 `observes_call_loan`，不恢复旧root-only helper：Shared临时字段
继续准入，Exclusive仍仅owned根；序号先计全部实际EndCallLoan，再筛选公开切片。
新增 `exclusive_order::temporary_field_and_receiver_ends_preserve_the_shared_call_rank`
用固定原源码和期望验证临时父字段、Exclusive、direct、隐藏方法实参、receiver及borrow
source同Return的次序与唯一父owner清理。原borrow/receiver/Single见证和CI-only负控不删。

独立逐候选range-diff及完整逻辑复审未发现P1/P2：CaptureIndex接线、双终止传播与后缀
拒绝、receiver三层撤销门、四项iteration原oracle与两项named Single保留均已核对。
三个历史超长父文件仅增加接线，分别1009→1022、1131→1136、1557→1562行，沿用
原例外、未提高额度；算法仍在各自模块。原工作区及未跟踪脚本、未提交文档保持不变。

| 验收项 | 组合实际选择 | 本机结果 |
|---|---|---|
| 原借用与混合新见证 | direct-call、continuation、nested-borrow、receiver-backedge，保留下述排除 | 106 + 14 + 9 + 4 = 133 passed |
| 终止owner、开关全Analysis与raw撤销 | 23个普通lib精确测试：7个owner全产物、3个terminal源、2个shared-return、3个capture-release、7个raw相邻结果、1个CaptureIndex双来源 | 23 passed，378未选；全部lib负控另以no-run编译 |
| receiver/provider与capture邻控 | multifile的receiver_admission与iteration；capture_identity_unit | 30 + 2 passed |
| 退出迁移及保留Single | control_exits_unit、conditional_source_unit、loop_choice_unit、两个named Single精确测试 | 6 + 1 + 8 + 2 = 17 passed |
| 严格静态与跨crate | frontend all-targets Clippy，locked/offline，`-W clippy::all -D warnings`；workspace all-targets check、fmt、diff | 通过 |
| 文档与尺寸 | `check_docs.py`；`check_rust_sizes.py --base 65eaf1a7` | 863 Markdown；2224 Rust、35项既有超长，未提高额度 |

Mac AArch64、Rust1.96、LLVM21.1.8、locked/offline、默认debug、jobs=2；复用同一target，
跨工作区先刷新当前入口mtime并核编译来源，不清理数据。205项不同普通测试通过，不将
交叉用例单跑和所在目标复跑重复累计。D10/E2与动态条件owner/phi仍in-progress。

本机不执行direct族四项Abort整项、nested-borrow的
`inout_receiver_and_nonreturning_sources_do_not_publish_call_uses`、multifile iteration的
`nested_while_jumps_preserve_outer_provider_and_abort_has_no_cleanup_edge`，均仅编译；
所有损坏产物注入、历史压力、native、Linux与frontend/workspace全量测试未运行。
上述候选历史通过不能替代本组合正式CI；最终发布头双宿主CI仍待运行，不自动合并。

### Owner 复用批同步 #371 后主线（2026-11-08）

原205项验收及交叉测试保存为 `4c741ba0` 后，无冲突合入 main
`d2a8195468c172957eb789f347b6af82831ecd44`，组合头 `562c2ffa`。
独立复核确认main新增10文件逐字保全，包括native object-write错误返回修复；
原owner生产、cross测试及旧账本与4c741ba0逐字一致，不沿用旧基底CI证明新组合。

本次重新运行原205个普通合同，并加入 `type_callable` 20项，共225项不同测试通过：
原借用/receiver四目标133项、callable 20项、普通lib精确23项、multifile receiver/iteration
30项、capture identity 2项、iteration退出与保留Single 17项。lib其他378项未运行；
同一测试二进制已完整编译。沿用原六项Abort整项排除，并排除callable标准error身份
整项；不运行损坏注入或历史压力，不改变测试断言、规模、ignore或native边界。

frontend all-targets Clippy（locked/offline，`-W clippy::all -D warnings`）、workspace
all-targets check、fmt、文档、diff通过；相对d2a81954尺寸门禁为2225 Rust、35项既有
超长，原额度不变。Mac/Rust1.96/LLVM21.1.8、默认debug、jobs=2，共享target串行。
未运行frontend/workspace全量测试、native、Linux或正式CI。文档后继不冒称再次运行
测试；新头正式CI待验证，草稿发布不表示D10/E2完成，不自动合并。

### 双终止负控夹具的正式 CI 修复（2026-11-08）

输入为 #374 的 `51a20256dcd9d036ceba5f23cc3e6166c1783d75`，base 仍为 `d2a81954`。
[正式 run 42494969195](https://github.com/Halckon/Koven/actions/runs/42494969195) 的
[Linux](https://github.com/Halckon/Koven/actions/runs/42494969195/job/128776499617)与
[macOS](https://github.com/Halckon/Koven/actions/runs/42494969195/job/128776499633)均在三个
terminal owner 负控准备阶段 `unwrap(None)`：原 `SOURCES[0]` 的 callback 从未在分支使用，
已合法在声明后 ASAP 退休，分支中不存在所假定的 SharedCaptureEnd。这不是生产 cleanup
缺失；后继 artifact 未生成属于前序测试失败，不通过简单重跑或修改生产语义掩盖。

原八个 SOURCES 逐字保留。新增双臂 alias 使用源并加入完整 Analysis 开关等价测试；
两项普通源码回归分别固定原 unused-prefix 与新 branch-retirement，独立核对 AST alias
清理点、真实来源 owner 和 Return 身份。三个负控改用真实活入分支的夹具，按唯一
环境/来源身份找破坏目标，不再把 `end + 2` 当作来源 Drop；六种总验证、两臂 location-only
删除及两臂 overlay-only 删除/重复拒绝全部保留，生产文件无改动。

安全前提回归首先在原源实际失败（分支 end 0，预期1）；新增源首次运行的重复字面量
AST 查询歧义亦记录为失败，改用唯一比较串后同选择通过。最终普通 lib 精确27项通过，
其余376项未选：原23项加两项新回归、Shared双臂退休及原plain-return邻控，不重复累计
4项先行复跑。全部403项lib已编译；frontend all-targets严格Clippy、fmt、文档、diff与
相对d2a81954的Rust尺寸门禁通过。独立只读复审核原源保全、身份选择与三层负控未发现阻断。

Mac AArch64/Rust1.96、locked/offline、默认debug、jobs=2、共享target串行；损坏注入、
Abort、压力及native本机未执行，未复跑workspace或frontend全量。按本轮范围仅本地提交
并format-patch备份，不推送或合并；修复头正式双宿主CI尚未运行，远端旧头失败仍未解除。

### 独立批次：Rc raw type 消费者双入口（2026-11-08）

基于 main `60aa67d165c7419a596a06c4e69a61e06cc95716`，分支
`feature/spec-0341-rc-type-consumers` 仅迁移 `type_checking` 两项原 Rc 消费者到
`rc_unit`。不依赖 #351/#352，不修改生产代码、Guide、CI selector 或准入条件。

- 两份原始 source 字节保留；原2项构造、3项操作、参数名/value模式、MoveOnly、share顺序、
  payload Borrow，以及三条L0121、唯一有效构造和失败不发布Rc操作断言全部保留。
- 使用既有 `checked_entries`，同一重复解析AST与自定义环境分别经MemoryDocument/Rooted，
  保留真实source_id/root/path校验，补构造与操作/receiver的source-qualified身份。
  只消费raw typed facts，不增加final或ownership前置。
- 作者在单独复核步骤逐项对照原断言及原源码，确认其余87项测试不变、原89项测试总数不变。
  该小范围测试迁移未更改共享helper或生产逻辑。

| 验收项 | 测试目标/过滤器 | 实际结果 |
|---|---|---|
| 原提交基线 | 原main根测试文件，`type_checking rc_` | 4 passed，85 filtered；含原2项与2项邻控 |
| 最终完整消费者目标 | `cargo test --locked --offline -p lang-frontend --test type_checking` | 89 passed，0 filtered |
| Rc共享合同邻控 | `cargo test --locked --offline -p lang-frontend --test multifile_type_checking intrinsic_box_rc::` | 10 passed，156 filtered |
| 编译与格式 | frontend all-targets Clippy（locked/offline，`-D warnings`）、`cargo fmt --all -- --check` | 通过 |
| 文档 | `python3 scripts/check_docs.py` | 862份Markdown通过 |
| 尺寸与差异 | `python3 scripts/check_rust_sizes.py --base 60aa67d1`、`git diff --check` | 通过；38项历史超限不变，新模块87行 |

上述测试在Mac AArch64、Rust1.96.0、默认debug=2、locked/offline、独立Cargo target串行运行。
未运行workspace全量、Abort/故障注入/校准、native、Linux或本分支官方CI；不得替代精确发布头
双宿主CI。D10/E2保持in-progress，不创建PR或自动合并。


### 独立后续：接口闭包与泛型签名消费者（2026-11-08）

基于已合并 #351 的 main `d8c7393a`，分支 `feature/spec-0341-signature-type-consumers`
集中承接 Rc 原发布头 `97ac6070` 的两项提交为 `622da63d` / `cc0f666f`；唯一文档冲突
同时保留主线循环/Elvis流事实和Rc范围说明，原Rc分支不改。不依赖 #352 或CI等待。

- `nominal_unit::signatures` 迁移接口闭包、泛型签名与合法this两项原消费者；两份源码字节
  保留，L0093及Wrong/T双span、Good直接/传递接口数量、identity函数类型/泛型参数、
  顶层与成员归属及Nominal/StaticSelf顺序断言保留。归属读取Unit顶层declaration与
  nominal.members，receiver模式额外核验；不以函数名推断Unit symbol。
- 既有helper提供同一重复解析AST、自定义环境和MemoryDocument/Rooted两入口，查询真实
  source-qualified symbol/expression。不改helper、生产代码、Guide、selector或准入。
- 两项尝试迁移的原见证完整恢复：`enum_case_types_preserve_root_payloads_and_are_rejected_outside_type_tests`
  原Single为L0114，Unit当前无诊断；`nominal_and_type_parameter_identity_is_known_and_arity_is_exact`
  第二条L0091的主span在Single为Left、Unit为Int，label同为Left。这是本机原source探针
  的明确差距，不改oracle或source强行迁移，不宣称已修复或Single退休。
- 单独作者复核逐项核原断言与Unit结构映射，脚本确认两份source字节及其余根测试/helper
  全部保留，type_checking测试数仍89。新增测试模块小于1000行。

| 验收项 | 目标/命令 | 实际结果 |
|---|---|---|
| 集中基线 | main加Rc两提交后的 `type_checking` | 89 passed |
| 最终消费者 | `cargo test --locked --offline -p lang-frontend --test type_checking` | 89 passed，0 filtered |
| 签名共享邻控 | `cargo test --locked --offline -p lang-frontend --test multifile_type_signatures` | 19 passed，0 filtered |
| 编译与格式 | frontend all-targets Clippy（locked/offline，`-D warnings`）、fmt check | 通过 |
| 文档、尺寸与差异 | check_docs、check_rust_sizes（base `d8c7393a`）、diff check | 863份Markdown、尺寸及差异通过；35项旧尺寸欠账不变 |

本机Mac AArch64、Rust1.96.0、默认debug=2，独立Cargo target串行执行；不把历史Rc基线
当作本分支官方CI。workspace全量、Abort/故障注入/校准、native、Linux及本头官方CI未运行。
D10/E2继续in-progress，未创建PR或合并。

## 深泛型与接口图消费者后续（2026-11-08）

基于 main `78a36f25`，独立分支 `feature/spec-0341-type-stress-consumers`，
只迁移 Phase 2 raw type 压力测试，不修改生产实现、共享 helper 或 #351 的复审修复。
原 `deep_generic_and_long_interface_graphs_stay_deterministic` 的源码构造逐字保留，
复用 nominal_unit 的真实 MemoryDocument/Rooted 入口；两入口各自核无诊断、Leaf 身份
与64个继承接口，96层Box输入不降规模。原Single重复消费者退休，target仍89项。

尝试迁移原128个enum case + 96层smart-cast测试时，默认测试线程栈发生stack overflow
及SIGABRT。失败发生于调用双入口helper，未得到两入口完成证据，不能归因到某个具体
producer。已撤销该测试的迁移，原Single源码和全部oracle逐字保留并实跑通过；不调大
线程栈、不缩小输入、不以ignore掩盖，也不在本片竞争修复生产代码。
失败输入身份为本节基线的 `type_checking::long_enum_domains_and_deep_smart_cast_conditions_stay_deterministic`；
复现尝试将其原源码传给 `nominal_unit::checked_entries`，默认debug=2和默认线程栈。

| 验收项 | 命令或选择 | 实际结果 |
|---|---|---|
| 迁移消费者 | `cargo test --locked --offline -p lang-frontend --test type_checking deep_generic_and_long_interface_graphs_stay_deterministic` | 1 passed，88 filtered |
| 名义类型邻控 | 同target `nominal_unit::` | 13 passed，76 filtered，包含迁移项 |
| 保留Single差异见证 | 同target `long_enum_domains_and_deep_smart_cast_conditions_stay_deterministic` | 1 passed，88 filtered |
| 静态检查 | frontend all-targets Clippy `-D warnings`，fmt，docs，diff与Rust尺寸门禁 | 通过 |

本机Mac AArch64/Rust1.96、locked/offline、默认debug=2，独占target串行验证。
共14项不同测试通过。独立复查步骤核原输入逐字保全、Single完整恢复和原oracle对等迁移。
未运行完整target、workspace、native、Linux或官方CI；没有推送或自动合并，D10/E2仍in-progress。

## Signature 与深类型图候选复用整合（2026-11-08）

以远端 main `65eaf1a736b7060416ff25ffc5233eb6ee473855` 为基线，隔离分支复用
`40df03a9` 的 Rc/signature 四提交链及 `cd86f515`，不重复 Rc，不改生产实现。
主线 `nominal_unit/stress.rs` 的128个枚举分支/96层smart-cast及其Single见证逐字保留；
深泛型候选原stress另存 `deep_graph_stress.rs`，96层Box与64接口输入不降规模。
原候选工作区和其未提交文档不变。上述历史signature/Rc验收仅属于各节旧基线；
旧Unit L0114缺失及L0091定位差异已由当前主线修复，不作为当前缺口或新头通过证据。

独立逐项复审核五份原输入、oracle、helper source/analysis身份与两份stress字节保全。
首次整合把stress模块重复注册，实际编译报E0428；修正后两模块各注册一次并重跑。
文档首次超过200行入口限制，已将新事实放回原消费者概览，未删除历史记录或提高额度。

| 验收项 | 本组合实际选择 | 结果 |
|---|---|---|
| 类型消费者 | `type_checking`，排除两个同名 `long_enum_domains_and_deep_smart_cast_conditions_stay_deterministic` | 93 passed，2 filtered；历史压力整项只编译，交正式CI |
| 签名共享邻控 | `multifile_type_signatures` 全目标 | 24 passed |
| 静态检查 | frontend all-targets Clippy，locked/offline，`-W clippy::all -D warnings`；workspace fmt、diff | 通过 |
| 文档与尺寸 | `check_docs.py`、`check_rust_sizes.py --base 65eaf1a7` | 863 Markdown；2221 Rust、35项既有超长，无新额度 |

Mac AArch64、Rust1.96、默认debug，独占复用target且Cargo串行。117项不同普通测试通过；
没有执行历史Abort/长枚举压力、故障注入、frontend全量、native或Linux。当前头正式CI
仍待运行，不沿用原候选绿灯；不自动合并，D10/E2仍in-progress。

### Signature 复用批同步 #371 后主线（2026-11-08）

#371 合入后，main 为 `d2a8195468c172957eb789f347b6af82831ecd44`；本批在
`b3d90ac2` 合入该基底。唯一冲突是名称类型概览第17行：保留main的lambda模式、
泛型实例与Single封存哨兵/§98链接，再接回Rc、signature和deep graph消费者事实。
nullable bottom更新保持main正文，概览仍200行。独立复核确认其余9个main文件
逐字保全，原类型测试和见证未变，不将旧基底CI当作新组合验收。

新组合实际重跑 `type_checking` 93项、`multifile_type_signatures` 24项与
`type_callable` 20项，共137项普通测试通过。两个长枚举/deep-smart-cast历史压力
整项及标准error的Abort身份整项只编译，未执行。frontend all-targets Clippy
（locked/offline，`-W clippy::all -D warnings`）、workspace all-targets check、fmt、
文档和diff通过；相对d2a81954尺寸门禁为2222 Rust、35项既有超长，额度不变。
Mac/Rust1.96/LLVM21.1.8、默认debug、jobs=2，共享target串行；未运行损坏注入、
frontend全量、native或Linux。文档后继不冒称再次运行测试，最终新头正式CI待验证。

### Signature 复用批整合 #377 后主线（2026-11-08）

以已核远端 main `e1b0c2dc656ec5a94f4e4bce26b30fd20029aa46` 建隔离分支
`fix/spec-0341-signature-main-integration`，合入 #372 精确头
`bcd12e1ed14e029a14fd3078e1590e04b6f953fe`，代码整合头 `14992e8a`。
保留既有 Rc/signature/deep graph 提交链，不重新实现或重复 cherry-pick；原 main、
候选工作树及未提交改动未动。唯一冲突为名称类型概览一行，保留新主线的双 Unit
`lambda_completion` 与全部候选事实，概览仍200行。独立只读复审确认 main 新增26个
变更文件中的25个逐字保全，唯一交集即该概览；五项原输入与全部原oracle、两个helper、
主线stress及其Single见证、#372完整历史账本均保留，模块各注册一次，无生产代码差异。

| 验收项 | 本组合实际选择 | 结果 |
|---|---|---|
| 类型消费者 | `type_checking`，仅排除两个同名 `long_enum_domains_and_deep_smart_cast_conditions_stay_deterministic` | 93 passed，2 filtered |
| 签名与Rc共享邻控 | `multifile_type_signatures` 全目标、`multifile_type_checking intrinsic_box_rc::` | 24 + 10 passed |
| callable与新主线消费者 | `type_callable`、`lambda_completion` 全目标 | 21 + 4 passed |
| 静态质量 | frontend all-targets Clippy（`-W clippy::all -D warnings`）、workspace fmt、docs、相对 `e1b0c2dc` 尺寸、diff | 通过；863 Markdown、2223 Rust、35项既有超长，无新增额度 |

Mac AArch64 / Rust1.96.0，locked/offline、默认debug、jobs=2，新独占target串行Cargo。
合计152项不同测试通过。本轮实际执行了 `standard_error_has_a_stable_abort_identity_and_nothing_result`
这一raw类型事实测试，不沿用旧账本的“只编译”描述；没有执行native Abort。两个长枚举/
deep-smart-cast压力整项仅编译，未执行故障注入、frontend/workspace全量、native或Linux；
本片未改公开API/生产代码，未追加workspace check。旧基底或#372 CI不能替代本整合头正式CI。
本地分步提交并保存恢复备份，发布仍待用户后续审批；没有推送、修改PR或合入main，D10/E2保持in-progress。

### Signature 复用批保留 #380 主线后的四项增量（2026-11-09）

基于 main `4243ebe57b7b0e5c821a15876daf944c55d7ca39` 建独立分支
`fix/spec-0341-reconcile-type-consumers-20261109`，只复用 #372 精确头
`22fd4967191dd885adeb8b795d2db89d205ccd5c` 中仍独有的 Rc 两项与 signature 两项。
两个代码切片来自 `580cf21b` 和 `7b1774c0`，当前代码提交为 `25af1ffb`。
唯一代码冲突发生在 generic signature 与已退役的旧 deep graph 相邻区域；保留 main 的
`nominal_unit/stress.rs`，不恢复旧 Single 图测试，也不重复注册 `deep_graph_stress`。
主线逐层验证96层 Box 身份/实参、最内层 Int、64接口集合及双入口顺序的更强 oracle 不变。

独立只读复审确认两个新增模块与 #372 逐字一致，四份原源码与 main 逐字一致，原断言
保留。主线两个 helper、stress 和其余测试文件逐字保全；根测试扣除四项迁移与注册后，
其余非空白 token 完全一致。#372 的150行历史记录原样追加，不把旧基底验收改写成新头
证据。原 main、#372 分支、所有既有工作树及未提交文件保持不变，无生产代码或公开 API 修改。

| 验收项 | 本组合实际选择 | 结果 |
|---|---|---|
| 类型消费者及压力 | `cargo test --locked --offline -p lang-frontend --test type_checking` | 95 passed，0 failed/ignored/filtered；两个长枚举/smart-cast整项及更强deep graph均实际执行 |
| 签名与callable邻控 | 同包 `multifile_type_signatures`、`type_callable`、`lambda_completion` 三个完整target | 24 + 21 + 4 passed，0 failed/ignored/filtered |
| Rc共享合同 | 同包 `multifile_type_checking intrinsic_box_rc::` | 10 passed，273 filtered，0 failed/ignored |
| 编译与格式 | frontend all-targets Clippy（locked/offline，`-W clippy::all -D warnings`）、workspace fmt、diff check | 通过 |
| 文档与尺寸 | `check_docs.py`、`check_rust_sizes.py --base 4243ebe57b7b0e5c821a15876daf944c55d7ca39` | 863 Markdown、2224 Rust、35项既有超长；通过，无新增额度 |

Mac AArch64、Rust1.96.0、默认debug、jobs=2，独立worktree target内串行Cargo，共154项
不同测试通过。此前账本只编译的两个历史压力整项在本组合已实际通过，不沿用旧限制描述。
`standard_error_has_a_stable_abort_identity_and_nothing_result` 也实际通过，但它不是native Abort。
未运行frontend全量、workspace check、故障注入、native或Linux；本片无生产/API变化，
未扩大到无关全量门禁。正式双宿主CI仍须对发布的新头验收，不复用旧#372绿灯。
保留原候选分支，不强推、不关闭旧PR、不自动合并，D10/E2继续in-progress。

### #372 与 #374 在 #382 主线上的保全合流（2026-11-09）

基于实际 main `ac9a7d813a58b37ad9385ae55cbd55fd601279eb` 建独立分支
`fix/spec-0341-merge-type-owner-20261109`，通过普通 merge 保留 #374 精确头
`11c93d62edd7e5f04229dcd5d775bea5b1a26866`、#372 精确头
`22fd4967191dd885adeb8b795d2db89d205ccd5c` 及已推去重头
`b64af788dab8b8c7b24536e2f6b0adb45089de1b` 的完整祖先，组合代码头 `d92c0611`。
原远端分支与既有工作树不改写，不 force，不重复 cherry-pick 借用后续链。

#372 的相邻 signature/deep graph 冲突采用既有四项去重结果：保留 Rc 两项与 signature
两项迁移、原输入及 oracle，保留 main 更强 `nominal_unit/stress.rs` 的逐层96层 Box、
最内层 Int、64接口集合/顺序与原压力见证，不注册重复 `deep_graph_stress`。
独立静态复核确认 crates 和 Architecture 整树逐字等于 main + #374 + 既有去重分支；
最新 liveness、short-circuit 与多文件控制流回归保留。SPEC-0341 的 main、#372、#374
及去重分支四份历史均为当前正文的完整有序子序列，旧结果和限制没有改写。

| 验收项 | 本组合实际命令或选择 | 结果 |
|---|---|---|
| 类型与原消费者 | frontend 完整 `type_checking`、`multifile_type_signatures`、`type_callable`、`lambda_completion` | 102 + 24 + 21 + 4 passed，包含历史压力与 raw Abort 身份 |
| 借用与循环消费者 | 完整 `unit_direct_call_loan_ends`、`ownership_borrow_continuation`、`ownership_nested_borrow_arguments`、`ownership_iteration`、`ownership_receiver_backedge` | 110 + 14 + 10 + 202 + 4 passed，未排除 Abort 事实测试 |
| 内部共享合同 | `cargo test --locked --offline -p lang-frontend --lib` | 407 passed，含终止 owner replay 损坏负控 |
| 跨文件与 capture 邻控 | 完整 `multifile_ownership_checking`、`ownership_closures`；`multifile_type_checking intrinsic_box_rc::` | 162 + 74 + 10 passed；仅 Rc 过滤器有273项未选 |
| 编译与静态质量 | frontend all-targets Clippy，`--locked --offline -- -W clippy::all -D warnings`；workspace fmt、diff | 通过，适用 frontend 代码与全部测试目标实际编译 |
| 文档与尺寸 | `check_docs.py`、`check_rust_sizes.py --base ac9a7d813a58b37ad9385ae55cbd55fd601279eb` | 863 Markdown、2230 Rust、36项既有超长；通过，未提高例外额度 |

Linux x86_64、Rust1.96.0、默认 debug、jobs=2，独占 target 串行 Cargo；共1144项不同
测试通过，零失败/忽略。两次命令的目标名拼写错误在测试启动前已纠正，不计作通过。
本环境缺 LLVM21，未运行 workspace check、native、LLVM 或 frontend 全量测试；
Clippy 全目标编译不等于全部行为测试。上述本机结果与旧 PR CI 不替代新组合精确头的
正式双宿主/Preview 门禁，须全部通过且无未决项才合入。D10/E2 与 SPEC-0341 仍在推进。

### #384 同步 #383 新主线后的组合复验（2026-11-09）

#384 的 `cdf5a4106446aecfe3de9afbc4ed828627e72337` 已完成
[旧组合 CI](https://github.com/Halckon/Koven/actions/runs/42569789437)，但该组合基于
`ac9a7d81`，不能代替 #383 合入后的 main `62ca42cfad329bf92c361af38de64797530cd239`。
本次从 #384 远端头建立独立工作树，普通 merge 新 main，无冲突，组合代码头 `1d12f9b2`，
保留两个 PR、去重分支和 #384 既有全部祖先；原工作树和 #372/#374 分支均不改写。

独立静态复核确认 #383 的24个变更文件中21个与新main blob逐字相同；其余
`compilation_unit.rs`、`dataflow.rs`、`names-and-types.md` 完整保留原main新增/删除行。
新 callable 实际进入事实与 receiver prefix/owner observation 分别传递，互不覆盖；
when selector 新实现原样保留，owner observation 对 When 的保守拒绝没有放宽。

| 验收项 | 本组合实际选择 | 结果 |
|---|---|---|
| 原类型与借用消费者 | 上节九个完整 integration targets | 491 passed；压力与 raw Abort 事实均实跑 |
| 共享内部合同 | 完整 frontend `--lib` | 407 passed，含终止 owner replay 损坏负控 |
| 新main交叉邻控 | 完整 `multifile_type_checking`、`multifile_ownership_checking`、`ownership_closures`、`ownership_resource_deinit` | 286 + 163 + 74 + 31 passed |
| 编译与静态质量 | frontend all-targets Clippy（locked/offline、`-W clippy::all -D warnings`）、workspace fmt、diff | 通过 |
| 文档与尺寸 | `check_docs.py`、尺寸门禁 `--base 62ca42cfad329bf92c361af38de64797530cd239` | 863 Markdown、2231 Rust、36项既有超长；通过，额度不变 |

Linux x86_64/Rust1.96、默认debug、jobs=2，独占target串行，共1452项不同测试通过，
零失败/忽略/过滤；不将上节1144项重复加总。无LLVM21，未跑本地workspace/native/LLVM
或frontend全量；以上检查不能替代发布新头的正式双宿主、Preview及CI Passed。
须以包含62ca42cf的精确新组合验收，不沿用cdf5a410的旧base绿灯；D10/E2继续in-progress。

### #384 保留 #385 调用前缀新主线（2026-11-09）

上轮正式CI执行期间，main 再合入 #385，成为
`415ba2779f0d7588239ac7abbd234d117a9d0000`。从 #384 远端
`de26bdd2a15b30f61bc25651747a187846dc237b` 另建隔离工作树，普通 merge 新main，
组合代码头 `181e1188`，无冲突；全部原PR/合流头与主线祖先保留，不改写已有分支。
独立静态复核确认27个新main文件中25个blob逐字相同，`dataflow.rs` 与
`ownership.md` 的主线新增/删除行完整保留。调用开始求值、正常完成实参和实际dispatch
三种事实保持分离；owner/receiver观察不反向改变callable事实，未发现合流P1/P2。

本组合重新运行上一节的完整frontend lib与13个integration目标，1452项全部通过，
零失败/忽略/过滤，不沿用或重复累计旧结果。frontend全目标Clippy
（locked/offline、`-W clippy::all -D warnings`）、workspace fmt、docs、diff及相对
415ba277的尺寸门禁通过：863 Markdown、2231 Rust、36项既有超长，额度不变。
Linux x86_64/Rust1.96、默认debug、jobs=2、独占target串行；LLVM21缺失，未执行
本地workspace/native/LLVM或frontend全量。须以包含415ba277的新发布头完成正式CI，
62ca42cf组合的进行中门禁不作为本次合并证据；SPEC-0341与D10/E2仍in-progress。

### #384 保留 #386 完成性与 Nothing ABI 主线（2026-11-09）

上轮CI期间 main 合入 #386，成为 `e87de0aca7745d8867404b18d9f0a332b5b0b26d`。
本次从远端 #384 `b282ad05430bfd205fd80512b1821edadc4eaa79` 另建隔离工作树，
普通 merge 新main，代码头 `b8f700d3`。唯一冲突是名称类型架构的一段说明：使用
main整文件原样插回既有Rc/signature/stress两句，完整保留新loop/while/when说明和链接。
独立静态复核确认56个新main文件中55个blob逐字相同，唯一文档扣除上述两句后与main
全文逐字相同。ownership主遍历、DropPlanner、liveness均消费when完成性和实际selector
前缀，旧owner observer/replay保持原样，仍保守拒绝loop/while/when，不扩大观察能力。

本组合重新执行相同完整frontend lib及13个integration目标，1454项通过，零失败/忽略/
过滤；其中multifile_type_checking为287、multifile_ownership_checking为164，其余计数
与上一组合相同，不重复累计。frontend all-targets严格Clippy、workspace fmt、docs、diff
及相对e87de0ac尺寸门禁全部通过：863 Markdown、2233 Rust、37项主线既有超长，额度不变。
Linux x86_64/Rust1.96、默认debug、jobs=2、locked/offline、独占target串行；LLVM21
缺失，未运行本地workspace/native/LLVM或frontend全量。只有包含e87de0ac的精确发布头
正式CI才可用于本次合并，不复用b282ad05的旧base证据；SPEC-0341与D10/E2仍in-progress。

### #384 保留 #387 间接 Nothing ABI 与 loop call-prefix 主线（2026-11-09）

上轮CI期间 main 又合入 #387，成为 `f2bef461bd1ddb6a7a831fe8a60c67835106d2af`。
从远端 #384 `f93fdaedfaa215dd2360aa6a641a46d7ee6df19f` 另建隔离工作树，普通
merge 无冲突，代码头 `0ece41a6`。独立只读复核确认 #387 的35个变更文件与main
逐字相同，整个codegen与main无差异；既有32个crate消费者变更与上轮同blob，
owner observer/replay、receiver-prefix及Rc/signature原样保留，未发现合流P1/P2。
原 #372/#374 及所有既有 #384 历史仍为祖先，不改写任何原分支。

本组合重新实际运行完整frontend lib及13个integration目标，1454项通过，零失败/
忽略/过滤；计数同上一组合，不重复累计。frontend all-targets严格Clippy、workspace
fmt、docs、diff及相对f2bef461尺寸门禁通过：863 Markdown、2233 Rust、37项主线
既有超长，额度不变。Linux x86_64/Rust1.96、默认debug、jobs=2、locked/offline、
独占target串行；LLVM21缺失，未运行本地workspace/native/LLVM或frontend全量。
只有包含f2bef461的精确发布头正式CI才可用于合并，e87de0ac组合旧CI不替代；
SPEC-0341与D10/E2仍in-progress。

### #384 保留 #388 控制实参与 Unit fixture 主线（2026-11-09）

上一精确头 `cbcaeccc` 的正式CI run 42585742695 已通过15项门禁，编辑器按范围跳过。
合入前 main 又推进到 #388 `d193a65fa91950a3b9e9653d98f2281d4b9d238d`，故不以
旧base绿灯合并。本次另建隔离工作树普通merge，代码头 `c31b6827`，无人工冲突。
独立复核确认主线15文件中14个同blob；type_checking.rs完整保留fixture迁移增删行、
双Unit入口与source identity/偏移校验。Rc/signature各迁移仍一份，其余31个既有crate
消费者文件逐字保留，含owner observer/replay；实际Diverged优先判定不改变观察准入。

新组合完整frontend lib（408）及原13个integration目标（1047）实际通过1455项，
零失败/忽略/过滤，不累计旧结果。frontend all-targets严格Clippy、workspace fmt、docs、
diff、相对d193a65f尺寸门禁通过：863 Markdown、2233 Rust、37项主线既有超长，额度不变。
Linux x86_64/Rust1.96、默认debug、jobs=2、locked/offline、独占target串行；LLVM21
缺失，未运行本地workspace/native/LLVM或frontend全量。须等待包含d193a65f的精确发布头
正式CI与最新main再核对；SPEC-0341与D10/E2仍in-progress，原分支和所有祖先历史保留。

### D10/E2：规范类型种子消费者复用到最新主线（2026-11-08）

以 `e1b0c2dc`（#377 已合入）新建独立分支，逐文件比较确认原候选 `e8284750` 的
`canonical.rs` 与 `canonical_type_tables.rs` 在其父提交至本基线没有变化，且主线尚未
覆盖该迁移。因此仅原样复用两文件，Architecture 在当前正文追加事实，不回滚已合入
的 `lambda_completion` 等说明，不重放旧 Spec 账本或重复合入其他候选。

原 `package sample` 空源保持不变。MemoryDocument/Rooted 两入口各检查 signature/raw
表的固定18个 builtin 加 Signed/Unsigned literal 与 Error 共21种种子顺序、完整 kind
反查和总长度，不从另一入口或生产 ALL 动态生成期望。原 local/unit builtin 身份对照
保留在既有 canonical 核心测试，直接检查两表 ID 和 kind，不再运行旧 Single checker。
本片退休一个 integration Single checker 消费者；不改生产类型表实现或公开 API。

本基线实际验收：`canonical_type_tables` 1 passed，精确 canonical 核心单测 1 passed；
frontend all-targets Clippy `-W clippy::all -D warnings`、fmt、docs、相对本基线尺寸与
`git diff --check` 全通过。863份 Markdown、2220份手写 Rust，35项既有超1000行，
未新增尺寸例外或提高额度。独立复审确认原样复用、固定 oracle 与旧合同保全。
Mac AArch64、Rust/Cargo1.96，locked/offline、默认debug、jobs=2，复用独立 target 串行。
本片仅 cfg(test)/integration/docs，不改公开 API，未额外运行 workspace check；未跑
frontend全量、Abort、CI-only注入、native、Linux或远端CI。保持 D10/E2 in-progress，
仅本地提交及经恢复验证的 format-patch/bundle 备份，不推送、合并。


### D10/E2：去重复补强已合入失败 trial 见证（2026-11-09）

逐项比对原本地 `dc9fd259` 与 main `e1b0c2dc` 后确认，#377 的 `efa28997` 已引入
双入口 `failed_trials` 基础见证。本片不再注册原候选的 `rejected_trials`，不重复合入
其同类空表/参数/调用检查，仅在现有 helper 补缺失断言；`type_callable.rs` 原入口与
完整 Single Error 封存见证保持字节不变，不把 Unit 的缺失事实改称 Error 哨兵。

保留 #377 的双分析、两参数、空 calls/modes 和 observed=[2,2] 原断言；新增原两处
callee 的完整 primary Span/SourceId 与 resolve 切片、重复完整类型表，以及所有表达式
类型/类别的一致性。两个 lambda 及其 it/true body 共四项直接查询均为 None，不能用
悬空 TypeId 经表查询变成 None 冒充撤销。先于 `_ => continue` 核所有表达式；原合法
函数体 1 与 "text" 分别保持 Int/String，双分析均验证，防止回滚误删既有事实。
Architecture 只补该事实，保留 main 的 lambda_completion 等现行说明。无生产或公开
API 变更，旧 Single 消费者退休数仍为零。

| 验收项 | 本组合实际结果 |
|---|---|
| 原 callable 全部普通合同 | `type_callable` 20 passed；仅整项排除既有 `standard_error_has_a_stable_abort_identity_and_nothing_result` |
| 最终类型准入邻控 | `failed_contextual_lambda_recovery_preserves_call_and_live_roots` exact 1 passed |
| 前一步规范类型种子 | integration 与 canonical core 各复跑 1 passed |
| 静态门 | frontend all-targets Clippy `-W clippy::all -D warnings`、fmt、docs、相对 e1b0c2dc 的尺寸与 diff 全通过 |

组合共23项实际定向测试，不累计前一步重复运行；863份Markdown、2220份手写Rust，
35项既有超1000行，尺寸政策未改。独立只读复审确认不削弱 #377 或原 Single oracle，
直接 None、四项计数与合法 body 正控完整。Mac AArch64、Rust/Cargo1.96，locked/offline、
默认debug、jobs=2，复用同一 target 串行。测试 helper 变更未要求额外 workspace check；
未跑 frontend全量、Abort、CI-only注入、native、Linux或远端CI。基于上述main线性追加
本地提交，不推送、合并；每步分别导出 format-patch/bundle 并验证精确基线恢复与
私有云盘回读完整性，D10/E2继续in-progress。

## 普通分支 join 消费者迁移（2026-11-09）

独立分支 `fix/spec-0341-branch-join-consumers` 基于 main `e1b0c2dc`，代码
提交 `c85c09a4`。本次 fetch 的远端引用中，目标文件没有未合入提交；不叠加
Choice owner、receiver loop-prefix、String clone、类型候选或容器 signature 后继。

主线 `5ec4ed94` 已修正 bottom initializer 结束 callable 时的旧 Single L0088
差距。迁移前临时给原 Single/Rooted helper 追加 MemoryDocument/Rooted 对照，
原9项测试全部通过；临时代码不提交，日志留备份。

- `same_type_and_bottom_branches_keep_exact_result` 的12份原源码迁至双 Unit，
  保留 Int/Nothing 根类型、raw 无诊断及 final 成功。原分支/return/declaration
  oracle 不变，新增20+4次观察计数。
- `expected_types_keep_original_mismatch_diagnostics` 的6份普通原源码迁至双
  Unit，保留唯一 L0084、原 `"text"` primary 及 final diagnostics 与 raw 完全相等，
  新增12次观察计数。
- `observe_text` 保留同一源码、AST、标准环境，新增唯一 SourceId 和真实
  MemoryDocument/Rooted key/root/path 断言。只删除不再使用的旧包装函数。
- runtime Any 的4份 Single/Rooted 源码与 helper 函数体原样保留：旧入口的
  L0165、26..29范围和后续raw，与Unit双L0084仍分别约束，不声称其等价。

| 验收项 | 命令 / 范围 | 实际结果 |
|---|---|---|
| 迁移前入口对照 | 临时扩展的 `control_branch_join` 全目标 | 9 passed，0 failed/ignored/filtered |
| 迁移及终止流邻控 | `cargo test --locked --offline -p lang-frontend --test control_branch_join --test lambda_completion --test final_type_admission --no-fail-fast` | 9 + 4 + 63 passed，0 failed/ignored/filtered |
| 静态质量 | frontend all-targets Clippy `-D warnings`、fmt、docs、diff、相对main尺寸 | 通过；目标Rust文件921行，不增例外额度 |
| 独立只读复审 | 原源码/oracle、入口身份、观察次数与Any差异保全 | 无代码阻断；其余七项测试正文及保留的两个旧helper函数体逐字未变 |

Linux x86_64、Rust1.96.0、默认debug、离线锁定依赖。本片只改测试消费者与事实
记录，不改生产、语言语义、依赖或CI。未运行frontend全量、workspace
check、native、故障注入、macOS或正式PR CI。仅本地分步提交与私人补丁备份，
不推送、不合并；D10/E2仍in-progress。

## 未声明 component 的 final 消费者迁移（2026-11-09）

独立分支 `fix/spec-0341-structural-consumers` 基于 main `e1b0c2dc`，代码提交
`4f59b9bd`。本次 fetch 的远端引用中，目标文件没有未合入提交；不叠加已发布的
Choice、借用、类型、容器 signature 或 branch join 候选。

只迁移 `value_class_does_not_supply_an_undeclared_component_method` 的一份原源码。
先在同一 SourceMap/AST 上临时用完整 Diagnostic 相等断言核 Single 与两个 Unit
入口，exact 测试通过后撤回临时比较，再退休该 Single 分析。保留重复解析、自定义
环境、两条 L0165、原 `pair.component1` primary、空 details、无 call/projection
及 final 拒绝。新增实际 SourceId 和 MemoryDocument/Rooted key/root/path 检查；
不改生产、不启用自动 component、不把 raw 可观察性当作最终准入。

本轮先尝试字段与 fixture 消费者的完整诊断对照，临时 target 为9 passed/2 failed。
两个首轮 MemoryDocument 负例都额外发布 selected-parameter label，Single 没有；
相应 Rooted 负例尚未到达，不能推断结果。这不是主线原测试失败，也没有修改预期
来求等价。两项原消费者、旧 helper 和 sidecar 全部保留；其余测试正文逐字不变。

| 验收项 | 命令 / 范围 | 实际结果 |
|---|---|---|
| 同源完整 final 诊断对照 | 原目标 `value_class_does_not_supply_an_undeclared_component_method` exact，临时 `assert_eq!(single, unit)` | 1 passed，10 filtered；覆盖两个Unit入口 |
| 迁移与共享final邻控 | `cargo test --locked --offline -p lang-frontend --test ownership_structural --test final_type_admission --no-fail-fast` | 11 + 63 passed，0 failed/ignored/filtered |
| 静态质量 | frontend all-targets Clippy `-D warnings`、fmt、docs、diff、相对main尺寸 | 通过；目标Rust文件564行，不增尺寸额度 |
| 独立只读复审 | 原源码/oracle、双入口身份、非等价Single保全 | 无阻断；从BUILTINS起的helper及其他测试逐字不变 |

Linux x86_64、Rust1.96.0、默认debug、locked/offline。首轮fmt只发现局部import
应为单行，修正后重新通过；首次被fmt阻止的测试不计运行。未执行frontend全量、
workspace check、native、故障注入、macOS或正式PR CI。按本轮授权仅本地分步提交，
不生成补丁备份、不上传、不推送、不合并；D10/E2仍in-progress。
