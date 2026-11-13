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

## 120. Choice 分组与逻辑非条件切片（2026-11-09）

基于 main `e1b0c2dc` 的独立分支，不复用未合入的双终止 owner/consumer 候选。
Phase 3 的 V1 Choice 仅扩展条件准入：同一 Boolean 参数外可有分组或逻辑非链，
每层须为精确 Boolean 且不是 constant-use；最终 Name 必须解析到原参数 symbol。
原 planner 仍在分叉前求值条件，真实 then/else 固定为输入边 0/1，不根据参数值或
逻辑非重排 owner。未更改 producer catalog、事件协议、Binding/Drop 或旧 cleanup。

现有唯一 chosen、双 literal/单局部尾、完整消费与清理约束保持。常量、调用、比较、
短路、其它参数、后续 owner 使用、capture 和真实循环不因本片获得 Choice 准入；
不新增循环实例、loop phi、动态分支通用支持、Phase 4/native 或 E3 能力。

红测试提交 `b1d78957` 在旧生产准入上真实执行一项，新正例在 `if ((flag))` 缺 V1
能力处失败；裸参数的四组控制已通过。最小实现 `05197df7` 后，新三项覆盖七种条件
乘四种边形态，含两向混搭和同名不同 symbol；AST 独立期望核完整逐边事件、输入身份
及 chosen 的 Bind/Drop，另核旧 Unsupported 空 trace、sibling 和观察开关完整 Analysis。
拒绝矩阵另含八种条件和四种外围使用/循环源码，不从有限模型推断循环支持。

| 验收项 | 实际选择 | 本机结果 |
|---|---|---|
| 新条件正反例 | `owner_transport::tests::choice_conditions` 三项 | 3 passed，0 failed/ignored |
| Choice 邻控 | admission 两项实际源码、branch_local 两项实际源码、boundaries 三项、capture_isolation 一项 | 8 passed，保持原 oracle |
| 循环/旧 owner 边界 | loop cleanup 一项、If 三项、owned capture 一项、swap 一项 | 6 passed，未开放循环 |
| 独立复验 | 在 `05197df7` 重跑新条件三项 | 3 passed，精确头未变 |
| 静态质量 | frontend all-targets Clippy `-D warnings`、fmt、docs、尺寸、diff | 全通过；863 Markdown、2221 Rust、35项既有超限，无新增例外 |

共17项独立普通测试选择，实际各命中一次；编译目标与其它工作树隔离。没有运行
frontend/workspace 全量、模型/故障注入、native、Linux 或正式精确头 CI。独立只读
复审未见 blocker，确认逐层类型/身份限制和真实边序；不得把本机通过当作完整 D10/E2
验收。本分支仅本地分步提交及 format-patch/bundle 备份，不 push、发布 PR 或合并。


## 122. Choice 结果的一次具名转交（2026-11-09）

本片依赖 §120 本地头 `53dd1599`，其主干基线仍为 `e1b0c2dc`；未合并其它 owner 或
借用分支。Phase 3 只扩一个已明确定义的后继：原唯一 chosen 后允许一条不可变、
精确 String/MoveOnly 的 `val forwarded = chosen` 或 Group 链，不允许第三条语句。
initializer 的叶 Name 必须是原 chosen symbol；原条件、双边完整尾交付和局部边合同
不变。V1 既有 Move/Forward/Bind validator 与公开协议原样复用，不新建 Choice producer。

实际 take_named 返回原 symbol 后，事务把同一 Choice 放入独立结果帧；每层实际
Consume 完成按顺序发布 Forward，post-insert 后绑定目标并清空帧。最终只接受真实
后继声明 AfterStatement 的目标 Drop，chosen 不再被 named cleanup。失败 take、Read、
未完成/重复/错序/提前绑定、结果残留或错误清理均原子清空事务。不从旧 owner=None、
最后事件或 cleanup 重建 owner。只有一次转交，不开放 Pending/Replace、var/nullable、
调用、capture、二次转交、Return/Abort、真实循环实例/phi、Phase 4/native 或 E3。

测试先提交 `3fa1aebf`，首项在旧生产准入缺 V1 处真实失败。实现 `649790c6` 后，
三种条件 × 四种边形态 × 三种 Name/Group 后继共36组合保完整 AST oracle；两条旧
后续使用拒绝源码逐字迁为新正例，未删除覆盖。原 helper 仅拆出逐边部分，原直达 Drop
断言保留，新 helper 独立核每个 Choice ID、所有 suffix 事件、最终声明/cleanup、
零 chosen drop、旧 Unsupported 空表、sibling 和 observation on/off 全 Analysis。
十一种真实拒绝后继包括调用、mutable/nullable、额外转交、binary、capture、控制与
循环。新增单项20条手工 guard trace（1正19负）只编译未本机执行，不作真实生产证据。

| 验收项 | 实际选择 | 本机结果 |
|---|---|---|
| 新真实源码 | `choice_forwarding` 三个普通 exact，不含手工 transaction 项 | 3 passed，0 failed/ignored |
| 已有邻控 | §120 同一17项普通 exact，保留原 oracle及拒绝源码迁移 | 17 passed，0 failed/ignored |
| 独立复验 | 在 `649790c6` 重跑新普通源码三个 exact | 各命中1项，3 passed，无 blocker |
| 静态质量 | frontend all-targets Clippy `-D warnings`、fmt、docs、尺寸、diff | 全通过；863 Markdown、2222 Rust、35项既有超限；手工 guard 仅编译 |

未跑 frontend/workspace 全量、手工 guard/模型/故障注入、native、Linux 或正式精确头 CI。
只读独立复审未见 blocker，逐点核实际 hook 和 fail-closed；本片不 push、发布 PR 或
合并。format-patch 与增量 bundle 按私有备份流程保留；完整 D10/E2 和循环 phi 仍未完成。


## 124. 整个 If 的 Choice 结果分组运输（2026-11-09）

基于已发布 `853249b6`（tree `02b6b99d`，与 §122 本地 `de1b113c` 相同），只扩展
§89 留待独立合同的整个 If 外层 Group，不等候或复用前片的 CI 结果。Phase 3 保持
原条件、两条完整正常尾交付和最多一次具名转交；不修改 Guide、公开 API 或 V1 validator。

准入只剥离精确 String、非 constant-use 的 Group 链，终点必须是原显式 If；每层
保留真实表达式身份，不能把整个 initializer 当作 If control。双边实际完成后，同一
Choice 进入独立结果帧，只有实际 Consume 完成且当前位置/完成序号吻合才发布下一层
Forward。chosen 的 post-insert 只接完整末层 initializer 并清空结果；遗漏、Read、
非继续、重复、错序或提前/错误 Bind 均清空事务。无 Group 时沿长度1的相同合同；
后继 transfer 使用独立帧，不再制造 producer，不从 trace 或 cleanup 重建身份。

红测试 `7201bd9b` 在旧生产准入上实际一项失败，缺口是 `(if (flag) ... else ...)`
没有 V1。实现 `145572c9` 后，32组合（2条件×4边形态×2深度×2转交）全部符合独立
AST 期望：真实 control/逐边源、每层同一 Choice Forward、准确 Bind/Drop 及完整旧
Analysis不变。九种合法源码拒绝边界保留 nullable/var、nested Choice、退出、调用、
binary、二次转交、capture、loop。首轮负例的 sibling 选择错误命中了末尾 lambda，
已改为 source-qualified function 身份，未放宽 Complete 断言，随后两项全绿。

新增16条手工guard trace（1正15负）只编译未本机运行；它们不是32组合的运行证明。
调用、nested Choice、控制退出、真实循环实例/loop phi、二次转交、Phase 4/native 与
E3 均不开放。完整 D10/E2 继续 in-progress。

| 验收项 | 实际选择 | 结果 |
|---|---|---|
| 新真实源码 | `choice_result_groups` 两个普通 exact，不含手工guard | 2 passed，0 failed/ignored |
| 前片邻控 | §122 同一20项普通 exact | 20 passed，0 failed/ignored |
| 独立复验 | `145572c9` 上重跑两个新普通 exact | 2 passed，0 failed，精确头不变 |
| 静态质量 | frontend all-targets Clippy `-D warnings`、fmt、docs、尺寸、diff | 全通过；863 Markdown、2223 Rust、35项既有超限；新增guard仅编译 |

独立只读复审无 blocker，核完整初始结果帧和已有 transfer 隔离。未跑全量、手工
模型/故障注入、native、Linux 或正式精确头 CI。按用户最新方向不新增 format-patch、
bundle 或 Drive 备份；本地分步提交后准备 connector 发布，不强推或合并。

## 126. Choice 根的 checked replace 消费者（2026-11-09）

基于已发布 `01a159a1`（与 §124 本地 `eac11275` 同树），先核远端 Choice model 与
complex-consumers 均为祖先，未重复未合能力。main 后续推进到 `a8657f58`，本片不声称
在该新主线组合验收。Phase 3 依 Guide §10 的既有原子 replace/mutable-root 合同，
不改语言语义、公开事件 API、旧 cleanup 或 Single 见证，不等候前片 CI。

仅开放两句形状：`var chosen = If/Group If` 后跟 `val previous = replace(&chosen, RHS)`。
两根均须精确 String/MoveOnly，首 binding 显式 Val|Var 分流；var 无此后继仍拒绝。
后继必须对应实际 checked String root Replace，非字段/索引，实参与 descriptor 同序；
RHS 仅精确 String、非 constant-use 的无插值 literal/Group。既有 val 与一次 alias 不变。
分支局部、条件/整个 If 分组仍沿前片合同；新返回值不接受 call 外 Group 或额外使用。

私有 replace 子模块用独立 RHS 帧和结果/退休标志：逐层实际 Consume 后，只有真实唯一
transfers_at_call pending tuple 可发布 Pending。checked primitive 完成、root origin
更新且实际 root/pending 仍存在后才发 Replace；不受旧观察 Unsupported 早返屏蔽。
旧 Choice 成为 call 返回结果，新 definition 安装到原根；原根先在真实 CallReturn
释放，新结果经 Consume 完成与 post-insert 绑定 previous，声明后释放旧 Choice。
任意遗漏、错误身份、Read、提前/重复 commit、绑定或清理清空新事务，不从 cleanup、
事件历史或旧 owner=None 重建 token；全部旧 Analysis 字段及序列保持观察开关一致。

V1 validator 只取消 Choice 对 CallReturn 的总禁令，仍要求紧邻同 call/root/incoming
Replace 见证及 Named→Dropped；Swap 保持无 Choice。原模型的匹配邻接正例同步，
错 call/root/value、非邻接、无 Replace 与重复清理六负例保留。图可验证更一般的
Replace，但不授权相应 AST 消费者。新20条手工事务 trace（1正19负）与模型改动仅
编译，未在本机执行，不作真实生产证明。真实循环实例/phi、nested Choice、任意调用、
投影、插值/Name RHS、二次 replace、返回值额外运输、Phase 4/native 与 E3 均不开放。

真实红测 `57e8b96b` 在旧实现缺 V1 上一项失败；实现 `64f97a3e` 后，32组合
（2条件×4边形态×2整个If分组×2 RHS分组）独立核 AST 双边事件、三 producer、
每个 Choice/Definition 身份、Pending/Replace、准确两次 Drop 及完整旧 Analysis。
11种合法源码拒绝保留 sibling，另精确核不可变根原有诊断，不把拒绝观测误作语言错误。

| 验收项 | 实际选择 | 结果 |
|---|---|---|
| 新真实源码 | `choice_replace` 三个普通 exact，排除手工trace | 3 passed，0 failed/ignored |
| Choice及旧边界 | §124 同一22项普通 exact | 22 passed，0 failed/ignored |
| 共享hook邻控 | 旧swap/V1两项、nested replace一项、完整Analysis六项 | 9 passed，0 failed/ignored |
| 独立复验 | `64f97a3e` 上新三个普通 exact，各命中1项 | 3 passed，0 failed；运行前后头及源码tree不变 |
| 严格质量 | frontend all-targets Clippy `-D warnings`、fmt、docs、diff、尺寸 | 通过；863 Markdown、2225 Rust、35项既有超限；新增trace仅编译 |

独立静审无 blocker：确认真实 pending/commit/cleanup 隔离和准入边界；
`drop_planner.rs` 1557→1558 仅新增 checked commit 后一条委托，算法/状态留子模块，
现存 max_lines=1669 不变，本次增长已独立审阅，不批准后续增长。架构同节精简至199行，
保留旧边界与证据依赖。未跑全量、手工模型/故障注入、native、Linux 或正式精确头 CI；
D10/E2 仍 in-progress。按用户最新要求不新增备份，本地分步提交后准备 connector 发布，
不强推、不合并。

## 128. Replace 返回 Choice 的一次具名转交（2026-11-09）

基于已发布 `38f278d9`（与 §126 本地 `a2edabe4` 同树），先核 main `edaaa9b2` 与
远端 Choice 分支；相关生产路径没有额外新主线修改或重复的后续消费者。本片依赖未合
Choice 链，不表示已在最新 main 组合验收，不等待前片 CI。Phase 3 组合 §122 和 §126
既有移动/原子置换合同，不改 Guide、公开 API、V1 validator 或通用 pending/commit hook。

仅在 `var chosen = If/Group If; val previous = checked replace(...)` 后允许恰一条
不可变、精确 String/MoveOnly 的 Name/Group 转交。叶 Name 必须解析到 previous，
不能是安装新值的 chosen；三句入口必须已通过原 checked replace 准入。原两句 val
alias 和 replace 路径保留，普通 val 连续转交不会因三句入口而放开。

Transfer 记录真实 source symbol，复用原独立结果帧。实际 take_named 返回 previous
后仍须确认 replace 已提交、原根 CallReturn 清理完成、调用 Consume 完成且 previous
已 post-insert 绑定、返回帧已清空，才记录同一 Choice Move。各层 Consume/Forward、
最终 post-insert/Drop 继续沿原门禁；失败 take、Read、漏层、错序、重复或早绑定均撤销。
previous 不再 named Drop，最终清理点严格取第三句声明 AfterStatement；原根仍在
真实 CallReturn 释放新 definition。replace 的 Drop 回调仅向已准入最终 symbol 委托
既有 transfer 检查，提前/错误 previous Drop 仍撤销，不能仅按 symbol 合并两个所有者。

真实红测 `bd81859c` 在旧生产缺 V1 上一项失败；实现 `d205529c` 后，64组合
（2条件×4边形态×2整个If分组×2 RHS分组×2末尾转交分组）独立核完整 AST 边事件、
三 producer、准确同一 Choice Move/Forward/Bind、两次清理点与 previous 零 Drop。
11种合法拒绝覆盖四句、var/nullable、concat、capture、调用、新根、If、第二次replace、
循环及旧 val 两次转交；包含独立 sibling 隔离和观察开关完整旧 Analysis 比较。
新增18条手工trace（1正17负）仅编译未本机执行，不能代替上述真实源码证据。

| 验收项 | 实际选择 | 结果 |
|---|---|---|
| 新真实源码 | `choice_replace_forwarding` 两个普通 exact，不含手工trace | 2 passed，0 failed/ignored |
| 前片邻控 | §126 同一34项普通 exact | 34 passed，0 failed/ignored |
| 独立复验 | `d205529c` 上重跑两个新普通 exact，各命中1项 | 2 passed，0 failed；运行前后HEAD/tree一致 |
| 严格质量 | frontend all-targets Clippy `-D warnings`、fmt、docs、尺寸、diff | 通过；863 Markdown、2226 Rust、35项既有超限；新增trace仅编译 |

独立生产静审未见 blocker；两个私有模块分别858/356行，无尺寸例外上限变化。
二次转交、任意调用/投影、返回值复杂消费、真实循环实例/phi、Phase 4/native 与 E3
保持拒绝；未跑全量、手工模型/故障注入、native、Linux 或正式精确头 CI。
架构同节200行，D10/E2 仍 in-progress；不新增备份，分步提交后使用 connector 发布，
不强推、不合并。

## 130. Checked replace 返回 Choice 的整表达式分组（2026-11-09）

基于已发布 `4b7f8132`（与 §128 本地 `977752ee` 同树），核实远端 main `4243ebe5`
及现有 Choice 分支，没有重复消费者或此生产路径的新主线改动。仍依赖未合 Choice 链，
不是最新 main 组合验收。Phase 3 沿用 Guide §10 原子 replace 与既有 Group 运输语义，
不改语言、公开 API、validator、普通 pending/commit hook 或借用线。

仅将 `val previous = checked replace(...)` 扩到整个调用的 Group 链；每个节点须
精确 String、非 constant-use，叶仍通过原 checked String root Replace 与 RHS 合同。
可接 §128 已有的一次不可变 Name/Group 转交；nullable、其它表达式、普通调用、
incoming Choice、二次转交及真实循环/phi 仍拒绝。这里没有新增 String producer。

准入保存由实际 call 到最外 Group 的有序结果链；原布尔 completed 改为消费计数。
每次实际 Consume 同时核位置、前驱 returned 身份、commit 和原根清理完成状态；
非首层才发同一 Choice Forward。末层完成后才允许 post-insert 绑定 previous 并清空帧，
一次具名转交还须确认整链完成。漏层、Read、错序、重复、过早绑定或清理均撤销。
Pending/Replace/原根 CallReturn 仍指向实际 call，不替换成 Group；新 definition
仍先在 call 释放，旧 Choice 在第二声明或唯一转交后的第三声明 AfterStatement 释放。

真实红测在旧生产的一层 Group 源码缺 V1 上 1 failed，随后提交 `88036aa1`；
实现 `aabc4d78` 后新2项普通 exact 全绿。128组合（2条件×4边形态×2整个If分组
×2 RHS分组×2返回Group深度×2是否转交）独立核 AST 双边事件、三 producer、
完整有序 suffix、真实 call/末帧身份、两清理点、sibling 与开关完整旧 Analysis。
原 §126 的单层返回 Group 拒绝样例迁入正例；另8种合法拒绝保留 concat、其它call、
非literal/Choice RHS、四句消费、新根转交、nullable及循环，并核 sibling 隔离。
16条补充手工trace（1正15负）仅编译未本机运行，不充当真实生产行为证据。

| 验收项 | 实际选择 | 结果 |
|---|---|---|
| 新真实源码 | `choice_replace_groups` 两个普通 exact，不含手工trace | 2 passed，0 failed/ignored |
| 前片邻控 | §128 同一36项普通 exact，包含不分组的旧replace与转交 | 36 passed，0 failed/ignored |
| 独立复验 | `aabc4d78` 上新两个普通 exact，各命中1项 | 2 passed，0 failed；前后HEAD/tree一致 |
| 严格质量 | frontend all-targets Clippy `-D warnings`、fmt、docs、尺寸、diff | 通过；863 Markdown、2227 Rust、35项既有超限；新trace仅编译 |

独立静审无 blocker，确认真实 planner 内层 call/外层 Group 回调顺序符合帧合同。
唯一生产改动为 `choice/replace.rs` 356→396行；新增测试327行，无尺寸例外变化。
架构保持200行。不运行全量、手工模型/故障注入、native、Linux 或正式精确头 CI；
这些本机检查不代表主线整合、Phase 4/E3 或完整 D10/E2 完成。状态继续 in-progress。
依用户最新方向不新增备份，分步提交后 connector 仅发布独立新分支，不覆盖或合并。

## 132. Replace 新安装根值的一次继续运输（2026-11-09）

基于已发布 `82e99797`（与 §130 本地 `a48ae39f` 同树），先核 main `4243ebe5`
及现有 Choice 分支，未发现重复消费者；相关生产路径没有额外主线改动。本片仍依赖
未合 Choice 链，不表示新 main 组合验收。Phase 3 沿用现行 replace、move 与 ASAP
cleanup 合同；不改 Guide、公开 API、V1 validator、通用 hook、旧 cleanup 或 Single 见证。

三句形状现在允许 `var chosen = If/Group If; val previous = replace(...);
val forwarded = Name/Group chosen`。最后 binding 仍须不可变、精确 String/MoveOnly；
第二句可用 §130 整调用 Group，RHS 沿原 literal/Group 合同。它是对两个独立输出的
受限消费：旧 Choice 由 previous 接收，新 definition 继续留在原 chosen，不得混同。
原三句转交 previous、两句 replace 与 val Choice 一次转交保持原门禁。

replace admission 保存第二声明的精确 AfterStatement 点。新根继续路径不等待或发出
原根 CallReturn Drop，任何原根提前 Drop 都撤销；旧返回仍须逐层 Consume 完成并
post-insert 绑定 previous。只有 previous 的真实第二声明 Drop 已退休旧 Choice，
actual take_named 又成功移出 chosen 后，才将原 RHS producer 的 Definition 存入
独立 transfer value 槽；不从 symbol、cleanup 或事件历史重建。旧 previous 转交则仍
要求原根已清理且 previous 未退休；裸 Choice 转交仍运输原 Choice。

transfer 的 result/value/completed/bound 各自跟踪表达式位置、值身份、顺序和绑定。
Consume/Forward、post-insert 与最终第三声明 Drop 均消费同一 value；Drop 清空槽，
finish 拒绝残余 value，并保留双边 replay 守恒。漏旧退休、失败 take、错误 source、
Read、漏层、错序、早/重复绑定或 Drop 均清空整个事务，不影响原合法性和 cleanup。

真实红测在裸 `val forwarded = chosen` 缺 V1 上 1 failed，测试提交 `f0d53095`。
实现 `a877c251` 的128组合（2条件×4边形态×2整个If分组×2 RHS分组×2返回Group
深度×2末尾转交分组）逐项核 AST 两边、三 producer、同一 incoming Definition、
完整 suffix、previous 第二声明 Drop 与 forwarded 第三声明 Drop，chosen 零 Drop；
独立 sibling 与 observation 开关完整旧 Analysis 保持。9种合法拒绝保留四句、
var/nullable、concat、capture、调用、If、二次 replace 和循环。旧两条新根拒绝迁入正例。
21条补充手工trace（1正20负）仅编译未本机执行，不能替代真实源码证据。

| 验收项 | 实际选择 | 结果 |
|---|---|---|
| 新真实源码 | `choice_installed_forwarding` 两个普通 exact，不含手工trace | 2 passed，0 failed/ignored |
| 前片邻控 | §130 同一38项普通 exact，含旧返回Choice/裸Choice运输 | 38 passed，0 failed/ignored |
| 独立复验 | `a877c251` 上两个新普通 exact，各命中1项 | 2 passed，0 failed；前后HEAD/tree一致 |
| 严格质量 | frontend all-targets Clippy `-D warnings`、fmt、docs、尺寸、diff | 通过；863 Markdown、2228 Rust、35项既有超限；新trace仅编译 |

Clippy 首轮定位测试同类型 cast，删除冗余 cast 后严格门禁和40项普通 exact 全部重跑
通过，未放宽断言。独立静审无 blocker；生产 choice/replace 分别881/436行，新测试383行，
无尺寸例外变化，架构200行。任意后续、投影、incoming Choice、真实循环/phi、
Phase 4/native 与 E3 仍未开放；未跑全量、手工模型/故障注入、native、Linux 或正式
精确头 CI。D10/E2 继续 in-progress；不新增备份，不覆盖已有分支或合并。

## 134. Checked replace 两个输出的顺序运输（2026-11-09）

基于已发布 `d4e183a0`（与 §132 本地 `3ba0f136` 同树），先核最新 main
`ec8a7b9f` 及远端 Choice 分支，相关 Choice 生产路径没有额外主线改动或重复能力。
依赖未合 Choice 链，不把分支测试算作最新 main 整合。Phase 3 沿用已启用的
replace/move/cleanup 合同，不改 Guide、公开 API、V1 validator 或通用 planner hook。

新增恰四句形状：初始 chosen 接 Choice、previous 接 checked replace 返回值，
第三句不可变精确 String/MoveOnly binding 只接 Name/Group previous，
第四句同类 binding 只接 Name/Group chosen。两个目标须与原根、previous 及彼此不同；
只准旧输出在前、新根在后，不是任意 alias 链。零/单 transfer 的既有路径不变。

准入保存至多两段 source/target/chain/精确 AfterStatement 点，运行时只处理活动段。
实际 take 后保存的 value 沿原 Consume/Forward/post-insert 门禁运输；只有该目标
真正按自身声明点 Drop、result 已清空、bound 成立且 value 仍在，才清空整帧并推进
活动索引。第二段从全新 result/value/completed/bound 状态开始，不能继承上一段进度。
finish 要求所有段均完成；这不是两个同时存活的临时值槽或从历史事件重建身份。

双输出路径不允许原根 CallReturn Drop，也不允许 previous 原 binding 提前 Drop。
旧输出 alias 第三声明真实退休后才建立 previous_dropped，新根 actual take 才能拿到
原 incoming Definition，最后在第四声明退休；每个值恰一个真实清理点，根与 previous
都不再额外 Drop。错误源、Read、漏层、错序、提前/重复绑定和清理均原子撤销。

真实红测在四句源码缺 V1 上 1 failed，测试提交 `3422d4dd`。实现 `825d70a9`
后补全256组合（2条件×4边形态×2整个If分组×2 RHS分组×2返回Group深度
×2旧输出Name/Group×2新输出Name/Group），独立核 AST 两边、三 producer、
全部 suffix 身份/顺序与准确两次 alias cleanup，比较 observation 开关完整旧 Analysis。
9种合法拒绝覆盖新先旧后、同值alias链、concat、var/nullable、调用、第五句与循环；
保留 sibling 隔离及全部旧零/单 transfer 邻控。22条补充手工trace仅编译未本机执行。

独立静审曾发现：切段后旧表达式或尚未开始的第二段表达式落入 unknown callback 路径，
会被忽略而非撤销；普通合法源码绿灯不能证明这条故障边界。`754c94c7` 单独修正为
先识别全部 transfer.chain，非活动段一律 abort，包含全部完成后的退休链；真正无关
condition/callee/inout-root 回调不变。既有退休重复 trace 与新增提前第二段 trace 静态
核对该 guard，但未声称运行过这些trace。独立复审确认缺口闭合，没有其它 blocker。

| 验收项 | 实际选择 | 结果 |
|---|---|---|
| 新真实源码 | `choice_dual_forwarding` 两个普通 exact，不含手工trace | 2 passed，0 failed/ignored |
| 前片邻控 | §132 同一40项普通 exact | 40 passed，0 failed/ignored |
| 修正后重跑 | `754c94c7` 相同42项普通 exact及strict Clippy | 全部通过，未省略邻控 |
| 独立复验 | `754c94c7` 新两个完整名 ordinary exact，各命中1项 | 2 passed，0 failed；前后HEAD/tree一致 |
| 最终质量 | frontend all-targets Clippy `-D warnings`、fmt、docs、尺寸、diff | 通过；863 Markdown、2229 Rust、35项既有超限 |

生产 choice/replace 分别938/453行，新测试422行，无尺寸例外变化，架构200行。
反向双输出、同值反复运输、任意后续、incoming Choice、真实循环实例/phi、Phase 4/native
与 E3 仍未开放。未跑全量、手工模型/故障注入、native、Linux 或正式精确头 CI；
D10/E2 继续 in-progress。不新增备份，分步提交后 connector 创建独立新分支，
不覆盖已有 ref、不强推或合并。

## 136. Checked replace 双输出的新根优先顺序（2026-11-09）

基于已发布 `ab3cfa84`（与 §134 本地 `c75c6bbb` 同树），先核 main `ec8a7b9f`
及远端 Choice 分支，没有重复新根优先消费者。依赖未合 Choice 链，分支验证不等于
最新 main 组合验收。Phase 3 沿用 replace 的两个独立 owned 输出与原 move/cleanup
合同，不改 Guide、公开 API、V1 validator、cleanup 或通用 planner hook。

四句入口现在也允许新根先、旧 Choice 后：chosen 接初始 Choice，previous 接 checked
replace 返回值，第三句不可变精确 String/MoveOnly binding 取 Name/Group chosen，
第四句同类 binding 取 Name/Group previous。第二段必须是第一段尚未消费的另一个
输出，不得取第一段 alias、重复同源或重用目标；仍至多两段，不接纳任意链或第五句。

只有活动索引为0、previous 尚未退休、准入序列恰为 root→previous 两段时，允许新根
在旧 Choice 仍 Named 的情况下实际 move。原 commit、完整返回链 Consume、previous
post-insert 绑定、returned 槽清空及原根 cleanup 前置全部保留，不凭源码形状单独发值。
single-root 路径不满足两段门禁，仍须 previous 实际退休；原旧先新路径继续原合同。

新 definition 沿第一段真实 take/Consume/Group/bind 运输，在第三声明精确 Drop 后
整帧清空、活动索引才推进。第二段才可消费仍 Named 的旧 Choice，在第四声明退休。
索引是实际清理完成证据，不从事件历史重建；原根和 previous 原 binding 的提前 Drop、
非活动/已退休表达式及其它错误回调继续撤销。两输出均无多余 producer 或 named Drop。

真实红测在四句裸 Name 源码缺 V1 上 1 failed，测试提交 `351294cc`；
实现 `c56040cd` 后256组合（2条件×4边形态×2整个If分组×2 RHS分组
×2返回Group深度×2旧输出Name/Group×2新输出Name/Group）逐项核真实 AST 边、
三 producer、完整有序 suffix、first Definition Drop→second Choice Move 和准确
两次 alias cleanup；独立 sibling 与 observation 开关完整旧 Analysis 保持。
原新先旧后拒绝样例迁入正例，另9种合法拒绝覆盖alias链、concat、var/nullable、
调用、第五句、capture与循环。23条补充手工trace仅编译，未本机执行。

| 验收项 | 实际选择 | 结果 |
|---|---|---|
| 新真实源码 | `choice_installed_first` 两个 ordinary exact，不含手工trace | 2 passed，0 failed/ignored |
| 前片邻控 | §134 同一42项 ordinary exact，保留旧先新和单根路径 | 42 passed，0 failed/ignored |
| 独立复验 | `c56040cd` 两个新完整名 ordinary exact，各命中1项 | 2 passed，0 failed；前后HEAD/tree一致 |
| 最终质量 | frontend all-targets Clippy `-D warnings`、fmt、docs、尺寸、diff | 通过；863 Markdown、2230 Rust、35项既有超限 |

独立静审无 blocker，确认单根限制、第一退休证据及所有非活动回调门禁未放宽。
生产 choice/replace 分别942/459行，新测试420行，架构200行，无尺寸例外变化。
同值多次运输、任意消费、incoming Choice、真实循环实例/phi、Phase 4/native 与 E3
仍未开放。未跑全量、手工模型/故障注入、native、Linux 或正式精确头 CI；
D10/E2 继续 in-progress。不新增备份，分步提交后仅发布独立新分支，不覆盖、强推或合并。


## 138. 初始 Choice 的两级具名 alias 运输（2026-11-09）

基于已发布 `86d50628`（§136），核对远端 Choice 与 alias 分支，无重复实现。
最新 main 为 `ac9a7d81`（#382）；其通用 flow/branch 改动尚未与本未合依赖链组合验收，
本片分支测试不能替代最新 main 验收。Phase 3 沿用既有 move/ASAP 合同，只改 Choice
私有事务收集，不改 Guide、公开 API、V1 validator、cleanup 或通用 planner hook。

准入仅新增三句形态：初始不可变精确 String/MoveOnly chosen 接既有显式 If/Group If，
不可变 forwarded 接 Name/Group chosen，再由同类 again 接 Name/Group forwarded。
第三句只能消费第一 alias，不能重新消费初始根；仍至多两级，不开放 replace 输出
alias 链、第四句、普通调用、capture、退出或循环。原三个合法拒绝样例完整迁入正例。

第一段必须由真实 take、逐层 Consume/Forward 和完整 post-insert binding 交付；
同一 Value 才从结果帧移入独立的 symbol/value named 槽，清空帧并推进活动段。
这不是退休：第一 alias 不产生 Drop，第二段必须实际 take 成功且源 symbol 精确匹配，
才唯一取走槽值并沿第二结果帧运输。仅末 alias 在自身声明的精确 AfterStatement Drop。
不从事件、cleanup 或 owner=None 重建身份；未绑定、重复/提前 take、Read、错序、
中间 Drop、非活动/退休回调和遗漏末清理均撤销，abort/finish 覆盖新 named 槽。
replace 路径不进入这条 post-insert 推进，仍按各输出真实 Drop 推进，旧合同保持。

真实红测在裸三句源码缺 V1 上 1 failed，测试提交 `0904158f`；实现 `33c46d05`
后216组合（3条件×4边形态×2整个If分组×3第一alias深度×3第二alias深度）逐项
核真实 AST 双边、两个producer、完整有序 suffix、同一Choice身份、原根/中间alias
无Drop及末alias准确清理。10种合法拒绝覆盖第三次转交、var/nullable、concat、
普通调用、capture、循环和两种replace输出链；sibling及开关完整旧Analysis保持。
26条补充手工守卫trace仅编译，未本机执行，不替代真实planner证据。

迁移旧拒绝样例后，strict Clippy 报单元素循环；`9718dfbe` 单独改为普通块，
没有跳过检查或改变断言，随后重跑全部46项和严格Clippy。独立静审无blocker。

| 验收项 | 实际选择 | 结果 |
|---|---|---|
| 新真实源码 | `choice_alias_chain` 两个 ordinary exact，不含手工trace | 2 passed，0 failed/ignored |
| 前片邻控 | §136 同一44项 ordinary exact，含replace双输出与单alias | 44 passed，0 failed/ignored |
| lint修正后 | 同一46项 ordinary exact及frontend all-targets Clippy `-D warnings` | 全部通过 |
| 独立复验 | `9718dfbe` 两个新完整名 ordinary exact，各命中1项 | 2 passed，0 failed；前后HEAD/tree一致 |
| 最终质量 | fmt、docs、尺寸、diff | 通过；863 Markdown、2231 Rust、35项既有超限 |

生产 choice 为973行，replace未改；新测试332行，架构200行，无尺寸例外变化。
D10/E2 继续 in-progress；真实循环实例/phi、incoming Choice、通用后续消费、Phase 4/native
与 E3 仍未开放。未跑全量、手工模型/故障注入、native、Linux、最新main组合或正式精确头CI。
不新增备份，分步提交后仅用connector发布独立新分支，不覆盖已有ref、不强推或合并。


## 140. 双 Boolean 输入的纯短路 Choice 条件（2026-11-09）

基于已发布 `7b9aa46f`（§138），先核远端 main `14297820`（#384）与 Choice 分支，
没有同范围实现；既有32提交 Choice 链仍未合主线。本片保持该依赖，未做最新main组合
验收。Phase 3 沿用现行 Boolean 短路与 move/drop 合同，不改 Guide、公开 API、V1事件/
validator、cleanup、通用遍历或任何运行期 Choice/replace/alias 守卫。

准入允许顶层非泛型 Unit 函数有一至两个参数，逐一要求无mode marker且精确Boolean，
即使未被条件引用也检查；零参数、第三参数及其它类型/模式仍拒绝。条件仅这些真实
UnitSymbolId 的 Name，以及 Group、Not、LogicalAnd/LogicalOr 树；每节点均检查
非constant-use与精确Boolean。迭代worklist检查两侧全部语法，不只检查当前可能求值
的一侧，不按名称拼写授权；literal、比较、普通call、If及其它节点均不放行。

条件不产生owner、loan、调用或退出，实际短路仍由原planner求值；本片不展开或重写
求值顺序。外层String If的真实then/else继续作为Choice输入0/1，Not或短路结构不改变
owner边序，也不增加producer。旧String If观察仍原子Unsupported，新事务只发布既有
已证完整的有界结果；全部Group、alias及replace输出运输合同保持。

真实红测在 `flag && flag` 条件缺V1上1 failed（`e773c368`）；最小实现为`1cb51b35`。
44种条件/边形态组合逐项核真实AST双边、完整Bind/Drop、两个producer及无参数清理，
含两个原拒绝样例的正例迁移。32种组合覆盖4条件×8既有消费者：无/一/两alias、
replace无后继、旧/新单输出及双输出两种次序，核真实分支事件、Choice/Definition移动数、
精确Drop point/symbol与producer数量。19种合法拒绝覆盖literal、call、comparison、
nested If、nullable/非Boolean、参数模式及超限，包括未引用非法第二参数/第三参数。
全部保留独立sibling与观测开关完整旧Analysis等价。

首轮组合测试误用固定两producer helper，replace真实三producer触发失败；独立静审
也指出同一问题，改用既有完整分支事件helper，再按每种消费者精确核producer数量，
不降低断言。独审建议补的未引用非法参数反例已纳入。未发现生产blocker。

| 验收项 | 实际选择 | 结果 |
|---|---|---|
| 新真实源码 | `choice_predicates` 三个 ordinary exact | 3 passed，0 failed/ignored |
| 既有邻控 | §138 同一46项 ordinary exact | 46 passed，0 failed/ignored |
| 独立复验 | `1cb51b35` 三个新完整名 ordinary exact，各命中1项 | 3 passed，0 failed；前后HEAD/tree一致 |
| 最终质量 | frontend all-targets Clippy `-D warnings`、fmt、docs、尺寸、diff | 通过；863 Markdown、2232 Rust、35项既有超限 |

生产choice990行，新测试245行，架构200行，无尺寸例外变化；没有新增手工trace。
未跑全量、既有手工模型/故障注入、native、Linux、最新main组合或正式精确头CI。
真实循环动态实例/phi、incoming/nested Choice、任意消费者、Phase4/native、Single见证
退休与E3仍未闭合，D10/E2继续in-progress。不新增备份，分步提交后只用connector
发布新的独立分支，不覆盖已有ref，不强推或合并。


## 142. 动态 owner 全链与最新 main 的单 PR 整合（2026-11-09）

按用户合批要求，以 main `14297820` 新建独立整合分支，合入已发布 Choice 链
`d419f567`（§140），复用其35个主线外提交。逐支核对既有 Choice 分支均为该链祖先；
owner/capture/loop 旧切片已在 main，不重新实现。本次不覆盖原分支，不强推或合入主线。
Phase 3 合同与生产准入不扩张，D10/E2 继续 in-progress。

本地组合输入为 `aec1a5fd990960a5ba3d63ee403e9375734e0ca5`，tree
`ce82b231a4cbeb3c0ee98a61e593e0401096fa8d`；有序父级为 main `14297820`、
owner `d419f567`。全部 Rust 自动合并，没有手工生产冲突修正；唯一文本冲突为本 Spec
末尾追加区，保留 main 完整正文及 owner §§120–140 完整追加文本，各一份。
独立复核确认 main 独有190个文件、owner 独有18个文件逐 blob 保全，共同修改的三个
Rust 文件精确等于两侧非重叠改动的并集，main 短路和双 bare Return 逻辑均保留。

Architecture 合并后原页217行超过200行门槛，将完整“V1 value 投影与受限 Choice”
正文迁入 [Choice 专页](../../architecture/unit-choice-transport.md)，原页保留明确入口并
同步目录索引；正文原字保全，仅省去末尾分隔空行。另澄清旧观察及内部结果通道本身的
Unsupported 与独立 V1 有界准入不是同一通道，不改变实现或历史验收结论。

| 验收项 | 实际选择 | 结果 |
|---|---|---|
| owner 全链与主线邻控 | `lang-frontend --lib`，§140 的49项 ordinary exact，加6项 terminal Return / full legacy cleanup 邻控 | 55 passed，0 failed/ignored |
| 短路与公开清理 | `multifile_ownership_checking` 两项 control-result、两项 ordinary short-circuit、声明顺序 rebinding cleanup | 5 passed，0 failed/ignored |
| 循环拒绝边界 | `ownership_iteration loop_choice_unit::` | 8 passed，0 failed/ignored |
| capture 与借用邻控 | `ownership_closures capture_identity_unit::` 及 `ownership_borrow_continuation` scope/return 子先父清理 | 2 + 1 passed，0 failed/ignored |
| 独立复跑 | 三项 `choice_predicates`、installed-first、dual-forwarding、alias-chain、terminal Return 与两项公开 short-circuit，各完整名 exact | 9 passed，各命中1项；前后 HEAD/tree 不变 |
| 质量门禁 | frontend all-targets Clippy `-D warnings`、fmt、docs、Rust 尺寸、diff | 通过；864 Markdown、2245 Rust、37项既有超限 |

以上71项为不同 ordinary 测试，独立9项是重跑其中选项，不重复计入总数。Cargo 使用
`--locked --offline`；手工模型、故障注入和 Abort 类测试仅随 all-targets 编译，未本机执行。
后续仅文档修订不改变受测 Rust；再跑文档与 diff 门禁。精确远端头的正式双平台 CI 尚未
验收，需由一个 draft PR 触发；本地组合绿灯不替代 Linux、native 或完整 CI 证据。

本次未跑全量、native、Linux 或 Phase 4；真实循环动态实例/owner phi、incoming/nested
Choice、任意消费者、Single 见证退休、Phase 4/native 与 E3 仍未闭合。Spec 不归档，
不把有界 V1 观察或图守恒升级为这些能力。


## 144. 真实 While 的闭合 String body 与结构回边（2026-11-09）

先核远端 main `14297820`、owner draft #390 `b0bafd01` 及全部在途 owner/loop 分支，
未有同范围生产实现。新隔离工作树复用本地 `c41a288d`，其完整 tree `2b6583ca` 与
#390 头相同；不重复已发布 Choice 链，不覆盖旧工作。本片保持 Phase 3 只读观察，
不改 Guide、既有 cleanup、Choice 准入或 Single 循环见证。用户已明确批准本批源码、
测试和文档通过 connector 快进追加至 owner #390；发布须先核精确头、有序父链与完整树，
不另开 PR，不强推、覆盖已有工作或合入 main。

合同限定整个顶层非泛型 Unit 函数仅一个 While、一个无 mode marker 的精确 Boolean
参数；条件仅该真实 symbol 的 Name/Group，非 constant-use，原分析保留两结局。
body 只含本地精确 String/MoveOnly val/var，initializer 可为 literal、Name/Group 或
完整根 checked Replace 的递归组合；每层类型、根和操作数 descriptor 均核对。
其它条件、普通调用、capture、分支、跳转、外来/携带 owner、嵌套或多循环均拒绝。

真实 While 只遍历一次 body。入口为空时临时建立 fresh body catalog，保存全部旧 sink，
真实 body Result 保存后先恢复 observer/Choice/depth 再传播错误；不调用普通 function
begin/finish，不额外发布 callable。真实 fallthrough、scope cleanup 及完整 ValueState
空边界成立后复用无 Choice V1 seal，要求每个 producer 最终真实 Drop；LoopExit 后再核空。
新产物保存实际 statement/condition/body 和 true-entry、fallthrough-backedge、false-exit
三条结构边。旧观察仍原子 Unsupported；最终诊断或 deferred 阻止新 sealed 能力发布。

独审发现整份同源码二次分析的 body transport 可替换的身份缺口，已增加独立 body Arc
与 catalog 的绑定；outer Arc 继续绑定原观察，且核 Unsupported statement。未以单 token
内部一致性代替整份 catalog 身份。Body 定义属于每次激活的新实例模板，不是 runtime handle；
不变 Boolean 参数的实际行为为零轮或持续循环，有限前缀不证明耗尽、终止或动态 phi。

真实红测在合法 While body 缺产物处失败，提交 `18fdbae7`；生产与扩展测试提交 `6c9aec66`，
tree `44380f8ea1abf6bf1702399eb687c4bd52c662c9`。24个正例组合核真实 AST、参数/循环/body
身份、构造/移动/嵌套 replace、每个 owner 的唯一 Drop 及实际 drop facts。27个合法拒绝
源码核前缀/后缀原子性，另核跨 callable/跨分析隔离及真实所有权错误清空。负例首轮错选
末尾 lambda 为 sibling，已改为真实函数 item 身份后全组复绿，未降低断言。
公开测试在 MemoryDocument/Rooted 各核正常 body 与 return/break/continue，后者不冒充回边。

| 验收项 | 实际选择 | 结果 |
|---|---|---|
| 新实际生产者 | `closed_while` 四项 ordinary exact，不含损坏产物测试 | 4 passed，0 failed/ignored |
| owner 组合邻控 | §142 同一55项普通 lib exact | 55 passed，0 failed/ignored |
| 公开双入口与循环边界 | `closed_while_owner_unit` 一项、`loop_choice_unit` 八项 | 1 + 8 passed，0 failed/ignored |
| 主线邻控 | 公开 control/drop 五项、capture 两项、scope/return 子先父借用清理一项 | 5 + 2 + 1 passed，0 failed/ignored |
| 保留 Single 见证 | `loop_carried_choices` nullable、exit-call、continue、break 四个实际源码测试 | 4 passed；手工两轮 replay 未运行 |
| 质量门禁 | frontend all-targets Clippy `-D warnings`、workspace all-targets check、fmt、docs、尺寸、diff | 通过；865 Markdown、2249 Rust、37项既有超限 |
| 独立复审 | 生产接线、边界、身份与最终代码静审 | 无 blocker；独立5项二进制复跑因权限拒绝未执行，不计通过 |

合计80项不同普通测试，Cargo 使用 `--locked --offline`；workspace 编译使用已安装
LLVM21.1.8，不运行 native。两个新增损坏/最终 deferred 注入守卫及既有手工模型仅编译，
未本机执行。新生产/API/测试文件均低于1000行，原超限 compilation_unit 不增长，
drop_planner 因提取原 While 路由缩短；不新增尺寸例外。

最终文档不改变受测 Rust。精确发布头正式 CI、独立二进制复跑、Linux/native 尚未验收；
本地80项不代替这些证据。循环 header/exit 携带 phi、跳转合流、多实例 closure/capture
运输与释放、通用消费者、Phase 4/native、Single 见证退休及 E3 仍未闭合，D10/E2 继续
in-progress，Spec 不归档。实现边界见[闭合 While 架构页](../../architecture/unit-loop-owner-transport.md)。

## 121. Shared receiver 循环控制转移前缀（2026-11-09）

基线为 #374 的 `11c93d62edd7e5f04229dcd5d775bea5b1a26866`，生产及测试提交
`a9e9998d66ab4bba2fda914b9a17f4266ad5bece`。已核 main `e1b0c2dc`、#374 的 receiver
Return 切片及独立 String.clone `53d0a75b`；本片以 #374 为前置，不重复这些提交，不改
动态 owner/phi、语言准入或 P4/native，也未宣称已与随后主线或 String.clone 合并验证。

只将 `receiver_call_prefix_ends()` 对实际 `EndReceiverLoan` 动作的 Return 筛选扩至
break/continue。pending 注册、loop-depth 逆序移除、owner/provider 清理均不改变；
owned-result、Shared、owned Name/Group 根的原资格与排除边界保留。计数按每个 source
内的静态控制点重置，含未发布 borrowed receiver 的真实动作，不是动态循环次数或全局
事件索引。完整借用流、正常 receiver CallReturn、reservation、this、投影、temporary、
provider/borrow-binding receiver、borrow-result receiver 及跨事务全序仍未交付。

失败先行：未修改生产时，合法 loop body 的真实 break 前缀应有1条事件而得到0条，精确
测试失败。新双入口合同核完整有界向量、真实 source-qualified call/receiver/root/jump
身份、同根多 token 逆序、内层 jump 不结束外层 pending receiver、continue 保留外层
owner、两个循环及同循环互斥分支的计数隔离、for provider 顺序、边界和原子撤销。
旧 Return-only 测试中的两份 loop 源码逐字转为正例，其他原源码及 Single 见证保留。

独立复核发现新测试把函数参数误期望在普通 loop break 点析构；原算法在 LoopExit
清理外层参数，实跑9通过/1失败确认。仅修正该新 oracle 为 jump 点无 Drop，不改生产
cleanup；再次独立审查无阻断，最终 direct 族116项通过。新增 raw 测试还核对真实非空
break/continue 前缀在独立错误及 deferred 来源加入后撤销。

| 验收项 | 实际选择 | 本地结果 |
|---|---|---|
| 新合同及原 direct 族 | `unit_direct_call_loan_ends`，排除下列4整项 | 116 passed，含10项新 CFG 合同 |
| 借用 continuation、nested 与 receiver backedge | 对应3 targets，nested 排除下列1整项 | 14 + 9 + 4 passed |
| 原 Unit iteration 退出 | `ownership_iteration::control_exits_unit` | 6 passed，196项未选 |
| raw 原子门及相邻产物 | `--lib adjacent_borrow_results` | 8 passed，396项未选，含新增 loop 前缀撤销 |
| 静态与跨 crate | frontend all-targets Clippy `-W clippy::all -D warnings`、workspace all-targets check、fmt | 全通过 |
| 文档、尺寸与 diff | `check_docs.py`、`check_rust_sizes.py --base 11c93d62`、`git diff --check` | 通过；863 Markdown、2226 Rust、35项既有超长，未改额度 |

共157项不同普通 Rust 测试通过，不重复累计筛选复跑。Mac AArch64、Rust1.96、LLVM21.1.8，
locked/offline、默认 debug、jobs=2、独立 target 串行。direct 族的
`exclusive_prefix::abort_and_normal_return_do_not_invent_prefix_unwind_events`、
`receiver_prefix::abort_does_not_publish_receiver_prefix_ends`、
`temporary_field_pending::abort_does_not_unwind_the_temporary_field_parent` 与
`nonreturning_suffix_and_unreachable_calls_publish_no_cleanup`，以及 nested 的
`inout_receiver_and_nonreturning_sources_do_not_publish_call_uses` 均仅编译未运行。
未执行故障注入、压力、frontend/workspace 全量测试、native、Linux 或官方 CI。

本片只本地分步提交并生成 format-patch/bundle 私人备份，不推送、不开 PR、不合并。
恢复包明确以已发布 #374 的 `11c93d62` 为前置；本地回放与 Drive 字节核验分别记录于
备份说明，不将备份成功当作精确发布头 CI。D10/E2 与完整借用生命周期仍 in-progress。

## 123. Shared receiver 正常调用完成（2026-11-09）

基于前片 `084675c8b6b9200b89bbde3d30878829f5157f76`，独立分支
`feature/spec-0341-receiver-call-return`；已核 #374 的 Return 观察、前片 CFG 扩展及
未合的 String.clone `53d0a75b`，不重复其实现，也不改在途动态 owner/Choice 算法。
此片仍以 #374 的 `11c93d62` 为依赖祖先，不宣称已同步随后主线或其他未合候选。

新增 `receiver_call_return_ends()` 与封闭 `UnitReceiverCallReturnEndFact`。正常调用完成
原本通过 retain 丢弃的 call/receiver pending tokens，现以同一谓词的补集 extract_if
完整移出；符合既有 Shared、owned Name/Group 根、owned-result 方法资格的真实 receiver
原值被保存在独立表中，不从 descriptor 或 end_span 重建执行。其余 tokens 的保留顺序、
loop depth 和原退休集合不变。无新 iteration action，不改变 caller owner/drop 或借用准入。

前缀查询继续独立。内层正常完成后，外层 return/break/continue 不重复退休其 token；
内层 loop jump 只清理自己深度的 receiver，外层仍可正常完成。一个 pending receiver
在互斥的 Return/继续分支可以分别产生 prefix 与 CallReturn 事实，不能当作同一执行
路径上的两次结束。新表是静态正常完成见证，不提供参数/receiver/drop 全序、动态次数、
全局 absence 或完整借用生命周期。borrowed/this/投影/temporary/provider/borrow-binding、
reservation、borrowed-result receiver 与 String.clone intrinsic 的独立路径仍不扩张。

失败先行先因 API 缺失编译失败；接空表后，同一合法 owned receiver 源码在运行时再次
失败（实际0条、固定预期1条），再接真实 token 退休 producer 后转绿。新增11项普通双
Unit入口合同覆盖完整向量、独立源码 Span、真实 receiver/root/call 身份、Group、零参数、
嵌套同根、完成后 unwind、嵌套 loop、分支与循环静态点、混合 Return/正常路径、for 原
cleanup、排除边界和公开撤销；额外 Abort 邻控仅编译。新 raw 正例核实际非空前缀的错误/
deferred 清空，并以同unit两份相同 AST 索引源码证明 source-qualified 身份互不混淆。

独立只读复核确认 extract_if 完整耗尽、谓词与旧 retain 互补、正常 continuation 才调用、
原 token 移动和各层撤销齐全；新 helper 不影响原其他调用的 pending。补齐建议邻控及
模块注释后复核无P1/P2。既有 Single、prefix、iteration 和 String.clone 见证均保留。

| 验收项 | 实际选择 | 本地结果 |
|---|---|---|
| 普通正常完成与原 direct 合同 | `unit_direct_call_loan_ends`，排除下列5整项 | 127 passed，含11项新普通合同 |
| 原借用与receiver回边 | continuation、nested、backedge 三 targets；nested排除下列1整项 | 14 + 9 + 4 passed |
| 原 iteration 退出 | `ownership_iteration::control_exits_unit` | 6 passed，196项未选 |
| raw 撤销与跨source | `--lib adjacent_borrow_results` | 10 passed，396项未选 |
| 静态与跨crate | frontend all-targets Clippy `-W clippy::all -D warnings`、workspace all-targets check、fmt | 全部通过 |
| 文档与尺寸 | `check_docs.py`、`check_rust_sizes.py --base 084675c8`、diff | 通过；863 Markdown、2227 Rust、35项既有超长，未改额度 |

共170项不同普通测试通过，不重复累计先行/筛选复跑。Mac AArch64、Rust1.96、LLVM21.1.8，
locked/offline、默认 debug、jobs=2、独立 target 串行。CompilationUnitOwnership
1022→1035/1156、dataflow1136→1141/1248仅新增公开导出、字段、聚合及清空接线；
drop_planner1562→1559/1669，算法留receiver子模块。原例外、owner及拆分计划不变。

沿用 §121 的4项 direct Abort 整项与 nested 的
`inout_receiver_and_nonreturning_sources_do_not_publish_call_uses` 排除，另新增
`receiver_return::abort_after_completed_inner_receiver_does_not_invent_outer_return`
仅编译。故障注入、压力、native、Linux、frontend/workspace 全量测试及正式CI未运行。
本片只本地分步提交，不推送或合并；备份包含从已发布11c93d62起的前片及本片patch链和
增量bundle，避免仅依赖未合本地父提交。恢复/私有Drive字节校验独立记录于包内说明；
不以备份或定向门禁宣称完整事件流或D10/E2完成。

## 125. Borrow 参数 receiver 的正常 reborrow 结束（2026-11-09）

精确基底为已由 connector 发布的 `36b84bd069a63209b82a6d9120df937043e37b37`，与
本地前片 `11c3d796` 的完整树一致；独立分支 `feature/spec-0341-borrowed-receiver-return`。
已核在途 receiver/借用分支，不重复正常完成 producer 或 String.clone，也不触碰 Choice。

依现行 Guide10 的 Borrow 参数能力：参数允许 nested Shared reborrow，caller 保持 owner，
callee 不得析构借入值。此片只让 `OwnershipBindingKind::Shared` 的 `ValueParameter`
Name/Group 根进入已有正常完成查询；真实 Shared receiver token 仍在原 CallReturn 点
退休。owned-result、完整 place identity 及原 provenance 排除继续约束，不新增或重建 token，
不修改队列退休、loop depth、owner/drop、借用准入、阶段或 native 行为。

prefix 查询传入 false，继续 owned-only；Inout 参数、显式 borrow val、provider来源/绑定、
投影、temporary、this、borrowed-result receiver 不扩张。新事件只结束此次 nested
receiver reborrow，不表示调用者参数 loan 已结束，不提供跨事务全序、完整动态生命周期、
caller/callee 动态实例映射或全局 absence 证明。既有 Single 见证保持原样。

失败先行逐字保留原 `receiver_return` 排除数组里的 borrowed 源码，新增非空预期运行
失败（0事件对1事件）；最小资格变更后转绿，并将该原源码从旧空向量测试转为新正控。
首次新增两个fixture误写 `deinit(): Unit` 及 `List.size()`，分别被Parser/类型合同拒绝；
按现行 `deinit() {}` 及真实provider binding源码修正测试，没有改生产去放行错误源码。

9项新双Unit入口测试核原源码、default/显式Borrow、Group、嵌套/重复不同call token、
inner完成后outer return/break/continue、混合CFG、caller后续使用及唯一Drop、资源与被
捕获参数的无Drop义务、Inout/provider排除和公开撤销。新raw案例先形成真实非空表，再
叠加独立错误/deferred来源，要求整表撤销。独立只读复审确认新准入严格限Shared参数、
prefix false与原判定等价、正常continuation门及真实token运输不变，无实现阻断。

| 验收项 | 实际选择 | 本地结果 |
|---|---|---|
| 新reborrow与原receiver/borrow合同 | `unit_direct_call_loan_ends`，沿用前片5项Abort整项排除 | 136 passed，含9项新合同 |
| 原借用与receiver回边 | continuation、nested、backedge三targets，nested沿用1项Abort整项排除 | 14 + 9 + 4 passed |
| 原Unit iteration退出 | `ownership_iteration::control_exits_unit` | 6 passed，196项未选 |
| raw撤销与跨source邻控 | `--lib adjacent_borrow_results` | 11 passed，396项未选 |
| 静态与跨crate | frontend all-targets严格Clippy、workspace all-targets check、fmt | 全部通过 |
| 文档及尺寸 | `check_docs.py`、`check_rust_sizes.py --base 36b84bd0`、diff | 通过；863 Markdown、2228 Rust、35项既有超长 |

共180项不同普通测试通过，不重复累计先行复跑。
Mac AArch64、Rust1.96、LLVM21.1.8，locked/offline、默认debug、jobs=2、独立target串行。
现有6项Abort整项仅编译，未运行故障注入、压力、native、Linux、frontend/workspace全量
测试或正式CI。未增加新的超长文件或提高尺寸额度；父文件仅一行公开合同注释更新。

按用户最新指令不再生成备份；本片分步本地提交，准备通过connector发布，不强推、不合并。
本地定向结果不替代发布头官方CI，也不宣称完整借用流或D10/E2完成。


## 127. Borrow 参数 receiver 的控制转移前缀结束（2026-11-09）

独立分支 `feature/spec-0341-borrowed-receiver-prefix`，精确基底为 connector 已发布的
`bb2fca382f9e67b913a780329a7fea587e1fea4b`，与前片本地 `b06970b9` 的完整树一致。
已核在途 receiver/borrow 分支，不重复正常 CallReturn 或 String.clone，也不改 Choice。
main 后续经 #378 前进到 `a8657f58`；本片验证只证明此明确基底上的候选，不算新主线、
其他未合分支组合或集成后验收。

依 Guide10 Borrow 参数允许 nested Shared reborrow、caller 保有 owner 的现行合同，
本片把 §125 已有 Shared ValueParameter 根资格用于原 `EndReceiverLoan` 前缀投影。
`observes_root_receiver` 移除资格开关，正常返回原 true 路径等价；Return/Break/Continue
仍只观察原 depth-aware unwind 实际移出的 token。没有新增或重建 token，也不修改队列
退休、loop depth、receiver activation、借用准入、owner/drop、provider 或 native 行为。

资格仍要求 declaration/symbol 的 owned-result 方法、Shared receiver、完整 Name/Group
根 identity。Inout 参数、显式借用绑定、provider 来源/绑定、this、投影、temporary、
reservation 与 borrowed-result receiver 排除不变。结束的是被放弃调用的 nested reborrow，
不是原参数 loan 或 caller owner；计数仍含未收录动作，按静态控制点分别重置，不代表
完整流、全局 absence、跨表全序、动态迭代次数或 caller/callee 动态别名。Single 见证未改。

失败先行 `40145ca6` 逐字保留原 Return 排除源码，真实运行得到0事件而固定预期为1；
最小资格变更后转绿。原 break/continue 两份 Borrow 负控源码及混合 owned/Borrow 源码
也原样保留，改用非空固定 oracle；已完成 inner CallReturn 不在 outer 前缀重发。
13项新双Unit入口合同覆盖默认/显式Borrow、Group、同根逆序token、loop-depth、真实for
cleanup动作、后续参数使用、不同静态边计数、Inout/借用结果/provenance边界及公开撤销。
2项新增raw合同先真实形成非空表再叠加独立错误/deferred，并核相同AST index的双source身份。

新增fixture曾错误预期 loop 内 borrow val 准入，真实分析按既有 L0164 拒绝；后续探针
区分了 loop 外 alias 已准入但仍被本表排除、loop 内 alias/borrow-result binding 尚未证明。
最终保留两类固定正负控，拒绝项核精确诊断消息与 borrow marker span；临时探针输出已移除。
未以调整生产消除未支持路径，也未删去失败暴露的源码边界。

| 验收项 | 实际选择 | 本地结果 |
|---|---|---|
| 参数前缀及既有直接借用合同 | `unit_direct_call_loan_ends`，沿用前片5项Abort整项排除 | 149 passed，含13项新合同 |
| 原借用与receiver回边 | continuation、nested、backedge三targets，nested沿用1项Abort整项排除 | 14 + 9 + 4 passed |
| 原Unit iteration退出 | `ownership_iteration::control_exits_unit` | 6 passed，196项未选 |
| raw撤销与跨source邻控 | `--lib adjacent_borrow_results` | 13 passed，396项未选 |
| 静态与跨crate | frontend all-targets严格Clippy、workspace all-targets check、fmt | 全部通过 |
| 文档及尺寸 | `check_docs.py`、`check_rust_sizes.py --base bb2fca38`、diff | 通过；863 Markdown、2229 Rust、35项既有超长 |

Mac AArch64、Rust1.96、LLVM21.1.8，locked/offline、默认debug、jobs=2、独立target串行。
沿用6项既有Abort整项仅编译边界；未执行故障注入、压力、native、Linux、frontend/workspace
全量测试或正式CI。本片不改公开表结构或尺寸额度，不把源码准入升级为native证明。
共195项不同普通测试通过，不重复累计先行复跑。独立fresh-context只读复核完整11文件
diff、Architecture与本节，未发现P1/P2阻断；复审未另行运行Cargo。

按用户最新要求不再备份；红测、实现、文档分步本地提交，准备通过connector发布，不强推、
不合并。发布头与主线集成后的官方CI仍须独立核验，D10/E2与完整借用生命周期仍未完成。


## 129. Inout 参数的 Shared receiver reborrow 结束（2026-11-09）

独立分支 `feature/spec-0341-inout-receiver-reborrow`，基底为已发布
`835c1682115df2cceefa3bb3fc2cf921c6cdcda7`，与前片本地 `f2fbc8bc` 完整树一致。
开工核 main `edaaa9b21e7f9b1a9728db04dd72eabef7aeef51`（#379）尚无这组receiver表，
相关新变化为nullable lowering；已核已发布借用链与在途工作，不重复实现。本片依赖前片
堆叠，不把其测试算作当前main、其他未合分支或集成后的验收；不触碰Choice owner线。

Guide10 允许 Inout 参数建立 nested Shared reborrow，caller 保留完整已初始化的owner。
`observes_root_receiver` 仅将声明端 `ValueParameter` 的非owning binding准入从Shared
扩为Shared或Exclusive；顶层receiver仍必须 `SharedLoan`。正常CallReturn与实际
Return/Break/Continue token沿原路径退休，不改producer、队列、loop-depth、cleanup、
owner/drop、activation或语言准入。Exclusive receiver reservation不是本片准入对象。

owned-result、declaration/symbol、Name/Group完整根与真实target身份要求不变；投影、
this、temporary、provider来源/绑定、显式借用绑定、borrowed-result receiver仍排除。
结束的是Shared子reborrow，不是父Inout权限或caller owner；原计数仍包含被排除动作。
不提供跨表全序、全局absence、动态caller/callee别名、完整生命周期或native执行证明。
既有Single见证及原Borrow/owned切片不退役。

红测提交 `c4deb475` 逐字保留原Inout正常返回及前缀排除源码；两个测试实际在正常返回、
Return首例得到0事件而预期1。最小资格变化后转绿，原Break/Continue与混合Borrow/Inout
源码也原样转为非空完整oracle。新增caller用局部var形成owner，首次测试误用参数binding
查询期待Owned而实际为None；改为核真实 `formed_owned_locals`，不改源码或生产，唯一drop
与callee参数无drop断言保留。

13项新双Unit入口合同覆盖原源码、Group、同根normal/逆序prefix token、inner完成不重发、
loop-depth、for实际cleanup、后续变异准入、caller唯一drop、Exclusive receiver正常激活及
前缀未激活排除、投影/provider/结果边界、L0133禁止移出父参数及公开撤销。2项raw合同覆盖
正常和三类控制转移的真实非空表撤销，以及相同AST index双source不混淆。静态准入与身份
不代表实际执行过native调用或恢复权限。

| 验收项 | 实际选择 | 本地结果 |
|---|---|---|
| 新Inout及已有直接借用合同 | `unit_direct_call_loan_ends`，沿用前片5项Abort整项排除 | 162 passed，含13项新合同 |
| raw撤销与跨source | `--lib adjacent_borrow_results` | 15 passed，396项未选 |
| 原借用与receiver回边 | continuation、nested、backedge三targets，nested沿用1项Abort整项排除 | 14 + 9 + 4 passed |
| Unit iteration退出 | `ownership_iteration::control_exits_unit` | 6 passed，196项未选 |
| 相邻两阶段合同 | `multifile_two_phase_borrows`，排除下述2项Abort整项 | 25 passed，2项未选 |
| 静态及跨crate | frontend all-targets严格Clippy、workspace all-targets check、fmt | 全部通过 |
| 文档及尺寸 | `check_docs.py`、`check_rust_sizes.py --base 835c1682`、diff | 通过；863 Markdown、2230 Rust、35项既有超长 |

Mac AArch64、Rust1.96、LLVM21.1.8，locked/offline、默认debug、jobs=2；复用本worker的
隔离Cargo target串行运行，无其他任务并发使用。前片6项Abort整项及两阶段target的
`terminating_arguments_keep_reservation_without_activation`、
`nothing_returning_method_still_activates_before_entering_callee`仅编译，未执行故障注入、
压力、native、Linux、frontend/workspace全量或正式CI。共235项不同普通测试通过，不重复
累计先行复跑。独立fresh-context只读复核完整10文件diff、Architecture与本节，无P1/P2
阻断；复审未重跑Cargo。不增加超长文件或提高尺寸额度。

遵循用户持续开发、分步提交与connector推送授权，不再备份，不强推、不合并。发布头CI
与后续主线集成须独立核验；本片不宣称D10/E2或完整借用流完成。


## 131. 具体 this 的 Shared receiver reborrow 结束（2026-11-09）

本片接续已发布 Inout 头 `3e452ede5eb3d5a11a6ea14b6a6209b123a63ce4`，在独立
`feature/spec-0341-this-receiver-reborrow` 分支实现。开工核对远端 main `4243ebe5`，
其尚无 receiver 结束观察表；已有 receiver/Inout、String.clone 与动态 owner 分支不重复。
这是一条尚未集成主线的有界链，不把其检查计为最新 main 验收。与并行 owner §130 分开，
不触碰 Choice、共同 pending/commit 生产者或原主工作树。

依据 guide §10 的调用期 loan 与 receiver mode 合同，只有已检查的 Shared receiver、
Declaration/Symbol 方法及 owned result 才继续筛选。具体 nominal 类型声明须等于原 token
的 This target；ImplicitThis 核同一 owner，显式来源核同一 source 的 This/Group。仍从
既有正常 pending 移出或真实 Return/Break/Continue cleanup 读取，不从最终 descriptor
反推执行，不读取事后 callable context，也不新增 token。provider this、StaticSelf、
Exclusive、borrowed-result 保留排除，原具名根及 provider/binding 排除不变。

red 提交 `eb651f08` 保留四份旧 this 源码；两项测试实际在各自隐式正常/Return 首例
得到0事件而预期1，显式子例未在红测失败后继续运行。最小观察资格改动后四份均转绿。
扩展夹具的首次执行为9过3失败：无构造字段 value class 为L0077，同 this 的 Shared
outer/Exclusive inner 为L0135，from-this borrow continuation 为L0164。修正合法构造及
预留 Exclusive outer/Shared inner 正控；原非法借用来源保留为明确拒绝负控。另一次
borrowed-result 前缀仍L0164，也保留该来源而非放宽准入；测试代码的诊断显示API编译错误
在执行前修正。生产修改不因这些夹具失败而扩大语言能力。

15项新双Unit入口合同覆盖隐式/显式/Group、class/value class、Borrow/Inout/own callable、
嵌套正常及逆序prefix、inner完成不重发、内层loop jump保留外层token、for真实cleanup、
两阶段正常激活及前缀未激活、caller后续变异与唯一drop、non-owning this无drop、同一call
互斥正常/prefix点、StaticSelf与borrowed-result排除、L0133/L0135/L0164拒绝。2项raw合同
由真实非空正常和三类退出表验证后置L0134/deferred撤销，并核相同AST index的双source、
不同nominal owner与公开/raw完整相等。既有Single见证不退役。

| 验收项 | 实际选择 | 本地结果 |
|---|---|---|
| this与既有直接借用合同 | `unit_direct_call_loan_ends`，沿用前片5项Abort整项排除 | 177 passed，含15项新合同 |
| raw撤销与跨source身份 | `--lib adjacent_borrow_results` | 17 passed，396项未选 |
| 既有借用与receiver回边 | continuation、nested、backedge三个targets，nested沿用1项Abort排除 | 14 + 9 + 4 passed |
| Unit iteration退出 | `ownership_iteration::control_exits_unit` | 6 passed，196项未选 |
| 相邻两阶段合同 | `multifile_two_phase_borrows`，沿用2项Abort排除 | 25 passed，2项未选 |
| 静态及跨crate | frontend all-targets严格Clippy、workspace all-targets check、fmt | 全部通过 |
| 文档及尺寸 | `check_docs.py`、`check_rust_sizes.py --base 3e452ede`、diff | 通过；863 Markdown、2231 Rust、35项既有超长 |

Mac AArch64、Rust1.96、LLVM21.1.8，locked/offline、默认debug、jobs=2，同worker隔离target
串行执行；本片无native、Linux、故障注入、压力或frontend/workspace全量运行。继承§129
八项Abort整项仅编译，不把未运行项计为通过。共252项不同普通测试通过，不重复累计先行
复跑。独立fresh-context复审完整10文件、Architecture与本节及实际日志，无P1/P2；
复审不重跑Cargo。不改尺寸额度。

本片只观察静态实际结束事实，不承诺跨表全序、动态this/caller别名、完整生命周期或
native权限恢复；完整D10/E2仍未完成。不再备份，不强推、不覆盖已有工作、不合并；
connector发布与精确头CI另外核验。


## 133. 具名字段路径的 Shared receiver reborrow 结束（2026-11-09）

本片基于已发布 this 头 `3cca9843531692280c757953005e203d28189e20`，独立分支为
`feature/spec-0341-field-receiver-reborrow`。开工核 main `4243ebe5`，后续复核已推进
`ec8a7b9f`（#381）；该合并不含本观察模块。本片未整合新主线，以下不是新 main 验收。
已有receiver/Inout/this、String.clone与动态owner分支未重复；与owner §132无生产交叠。

依据 guide §10 调用期 loan 与稳定 root/field place 合同，沿原 receiver operand 的
Name/Group/非 safe Member 语法链检查有界来源，随后对 **完整 operand** 查询 typed place；
保留 root、全部字段符号与 source identity，并要求原 token target 等于该完整 place。
不能将剥离语法后的根当作字段 target。只准入 owned/Borrow/Inout 具名根、无 elements 的
路径；既有 provider、borrow binding、Shared kind、owned-result及静态call target资格
不变。不重建token，不改pending生产、activation、loop-depth、cleanup或native消费者。

红测 `2cc178c5` 的两个测试实际在owned正常/Return首例得到0而预期1，后续Inout子例未在
红测失败后执行。最小实现后四份原源码转绿。全新target首次整族193过1失败，发现原CFG字段排除断言，
将两份原Break/Continue源码原样迁为非空oracle，没有删来源或扩大生产。共六份旧字段源码
保留，其他空观察边界与Single见证不退役。

初版扩展15项为14过1失败，临时owner字段方法 `Holder(Node()).node.touch(...)` 尚不能形成
place。该完整源码独立固定为 `InvalidUnitArgumentPlace`，保留source/expression身份；
safe字段链 `holder?.node?.touch(...)` 在最终type准入报L0165，固定完整诊断码与operand span。
两者各含正常/Return源码及两个Unit入口，在未改生产的独立 `3cca9843` 基线实际复跑2项
通过，明确是基线缺口，不把它们当作已准入后零事件，也不在本片修复/启用。

基线复跑后首次复用旧Cargo target未触发重编译，12项仍得到旧空表；该轮不计验收。
此后使用全新、仅本工作树使用的target重跑，未清理旧目标、改生产或降低断言。不同工作树
即使串行也不复用本片target；最终direct记录为增补后的重编译，不单独声称冷缓存全套
验收，不以首次部分成功替代最终门禁。

18项新双Unit入口合同覆盖原六份源码、Group与多层field完整路径、owned/Borrow/Inout根、
重复token及兄弟字段、正常/CFG/loop-depth、for真实cleanup、后续字段replacement、aggregate
唯一drop及non-owning根无drop、互斥normal/prefix、Exclusive兄弟排除但计序、L0132/L0135、
provider/binding/index/this字段排除，以及上述两项入口失败边界。2项raw合同先生成真实非空
正常和三类控制转移表，再由后置L0134/deferred撤销；三source同AST index用例区分consumer
root与共享provider字段声明，公开/raw完整相等。

| 验收项 | 实际选择 | 本地结果 |
|---|---|---|
| 字段及既有直接借用合同 | `unit_direct_call_loan_ends`，沿用5项Abort整项排除 | 新target 195 passed，含18项新合同 |
| raw撤销和跨文件字段身份 | `--lib adjacent_borrow_results` | 19 passed，396项未选 |
| 相邻借用及receiver回边 | continuation、nested、backedge，nested沿用1项Abort排除 | 14 + 9 + 4 passed |
| Unit iteration退出 | `ownership_iteration::control_exits_unit` | 6 passed，196项未选 |
| 两阶段receiver合同 | `multifile_two_phase_borrows`，沿用2项Abort排除 | 25 passed，2项未选 |
| 静态及跨crate | frontend all-targets严格Clippy、workspace all-targets check、fmt | 全部通过 |
| 文档及尺寸 | `check_docs.py`、`check_rust_sizes.py --base 3cca9843`、diff | 通过；863 Markdown、2232 Rust、35项既有超长 |

Mac AArch64、Rust1.96、LLVM21.1.8，locked/offline、默认debug、jobs=2，全新独占target串行。
继承§129八项Abort整项仅编译；未跑native、Linux、故障注入、压力及frontend/workspace全量。
共272项不同普通测试通过，不重复累计基线及先行复跑。独立fresh-context复审完整12文件、
Architecture、本节及实际门禁日志，无P1/P2，未重跑Cargo。不增加尺寸例外。

不提供跨表全序、动态字段/owner实例映射、完整借用流或native恢复权限证明；D10/E2仍未
完成。不再备份，不覆盖既有分支、不强推或合并；connector发布与精确头CI另外核验。


## 135. 首层直接借用绑定的 Shared receiver 结束（2026-11-09）

本片接续已发布字段头 `d290532fac5716bd6b7f6e139b53ede4908fb2e8`，隔离分支为
`feature/spec-0341-direct-binding-receiver`。开工与验收前核 main 均为 `ec8a7b9f`，
已有 receiver/字段、String.clone 与并行 owner 工作未重复。本片没有整合新 main，以下
检查不计为主线验收；与 owner §136 无生产交叠，不触碰 Choice 或通用 pending/commit。

依据 guide §10 的稳定只读 place initializer、非 owning binding 与父子 loan 合同，
仅扩展既有观察资格：首层 `borrow val alias = root` 的普通 BorrowedStorage、无 parent
与 source loan、initializer 同 source 且完整 place 等于 origin 根。随后仍检查原
owned/Borrow/Inout 根资格、provider 排除及 receiver target 精确相等。只接受 Shared、
静态 Declaration/Symbol 方法和 owned result；alias operand 不变，target 仍为 canonical
source lease，不将 alias 当作 owner、不推测 callee 内的 payload 地址。事件继续来自
真实正常 pending 移除或实际 Return/Break/Continue cleanup，不新增或合成 token。

调用返回来源、父子 binding 链、字段来源或 alias 字段 receiver、carrier、provider、
Exclusive 及 borrowed-result 继续排除。本片不改变语言准入、pending 生产、activation、
loop depth、父 binding 终止或 source drop，也不把静态事件当作动态 NLL/权限恢复证明。

红测提交 `1884748a` 的正常及 Return 两项真实源码均得到0事件而预期1；最小资格扩展后
转绿。扩展14项首次为12过2失败：语句 span 实际不含分号，loop jump 仍需析构已求值的
实参临时值。修正相应断言，改为只限制 source owner 不在 jump 提前析构，原源码保留。
追加 Exclusive 负控首次为15过1失败，误期待 L0134；class alias 经 canonical source
与父 Shared loan 冲突，实际为 L0135。固定实际诊断码、message、primary 与借用来源 label，
不改变生产或删除该负例。provider 元素新 binding 明确在 loop 准入阶段 L0164，固定其
message、marker span 和空 binding，而非将被拒源码当作合法的空结束表。

16项新双Unit入口合同核完整结束向量、token/source/target/span、四种参数 mode、Group、
嵌套正常和逆序 prefix、已完成 inner 不重发、内层 jump 保留 outer、互斥正常/退出点、
for 实际 action 顺序、父 binding 后续使用及原终止点、最后使用后 source 可转移、caller
唯一 drop 与 non-owning 参数无 drop。call-derived/parent-chain/字段来源原有排除与
Single 见证保留。2项 raw 合同先由真实源码生成正常及三类退出非空表，再证明后置
L0134/deferred 撤销；三 source 的相同 AST/symbol index 保持 alias 与来源根分离，公开
和 raw 表完整相等。

| 验收项 | 实际选择 | 本地结果 |
|---|---|---|
| 直接绑定及既有直接借用合同 | `unit_direct_call_loan_ends`，沿用5项Abort整项排除 | 211 passed，含16项新合同 |
| raw撤销及跨source身份 | `--lib adjacent_borrow_results` | 21 passed，396项未选 |
| 相邻借用及receiver回边 | continuation、nested、backedge，nested沿用1项Abort排除 | 14 + 9 + 4 passed |
| Unit iteration退出 | `ownership_iteration::control_exits_unit` | 6 passed，196项未选 |
| 两阶段receiver合同 | `multifile_two_phase_borrows`，沿用2项Abort排除 | 25 passed，2项未选 |
| 静态及跨crate | frontend all-targets严格Clippy、workspace all-targets check、fmt | 全部通过 |
| 文档及尺寸 | `check_docs.py`、`check_rust_sizes.py --base d290532f`、diff | 通过；863 Markdown、2233 Rust、35项既有超长 |

Mac AArch64、Rust1.96、LLVM21.1.8，locked/offline、默认debug、jobs=2，本工作树专属全新
Cargo target 串行运行，不跨树复用。共290项不同普通测试，不重复累计先行复跑；继承§129
八项Abort整项仅编译，未跑native、Linux、故障注入、压力或frontend/workspace全量。
独立fresh-context只读复审完整8文件、Architecture、本节与实际门禁日志，未见P1/P2；
复审未重跑Cargo，已按建议收窄 class alias 负控措辞并锁定诊断来源。
不增加尺寸例外，不宣称跨表全序、完整借用流或D10/E2完成。不再备份，不覆盖已有分支、
不强推或合并；connector发布与精确头CI另核。


## 137. 首层字段来源借用绑定的 Shared receiver 结束（2026-11-09）

本片基于已发布直接根绑定头 `994d916a87eb84802dcb34fd0691c4ee3dc16d79`，独立分支
`feature/spec-0341-field-binding-receiver`。开工核 main `ac9a7d81`（#382），收尾核
main `14297820`（#384）。新主线已有有界 Return-prefix 观察，但未接入本片字段来源
binding；已有 receiver/字段/直接根绑定、String.clone 与动态 owner 工作不重复。
本片没有整合新主线，以下不是最新 main 验收；与并行 owner §138 无生产交叠。

guide §10 允许稳定只读 place 作为非 owning binding initializer。这里仅将首层直接
binding 的 origin 从完整根扩展为具名根的普通字段链；initializer 与 receiver 共用
Name/Group/非 safe Member 语法检查，随后要求完整 typed initializer place 精确等于
binding origin。原 receiver target 也须等于该完整 place，保留每个字段符号与 source。
仍要求 BorrowedStorage、无 parent/source loan、alias operand 是完整根、来源满足原
owned/Borrow/Inout 资格及 provider 排除。Shared、静态 Declaration/Symbol 与 owned
result gate 不变，不把 source lease 当作 callee payload 地址。

正常与 Return/Break/Continue 只观察原 pending/cleanup 实际退休 token。不改变语言
准入、父 binding 终止、source drop、activation、loop depth 或 owner 运输。call-derived、
父子链、alias.field、this 字段来源、index/safe/temporary、carrier、provider、Exclusive
及 borrowed-result 保留边界；不宣称完整 NLL、动态实例映射或 native 权限恢复。

红测提交 `e8c8ad17` 保留前片两份正常/Return 字段来源源码，均实际得到0而预期1；最小
观察资格扩展后转绿。旧排除循环仅移出这两份源码，保留其他负控和 Single 见证。首轮13项
扩展通过；追加至18项时17过1失败，MoveOnly sequential index initializer 产生 L0136，
不是合法空观察。原正常/Return 源码单列为第19项，锁定 message、`nodes[0]` primary、
容器 owner label 和空 binding。没有改生产去放行它，也没有声称单独基线实跑该边界。
后续独立复审建议的两个 L0135 负控已补 primary 与父借用 label，最后整族重新通过。

19项新双Unit合同覆盖完整向量、alias/source/target/span、多层Group/field、四种根模式
与四类完成点、class/value class、同根兄弟 alias 与逆序 prefix、父 binding 后续存活和
唯一 aggregate drop、最后使用后整根转移与真实 ReplacedField 析构、兄弟变异/重叠冲突、
内层 loop 保留 outer、inner 不重发、for 实际动作及互斥 normal/prefix 点。this 来源、
call-derived、parent-chain、alias.field 与 borrowed-result 先证明合法准入，再核空表。
2项 raw 合同以真实正常/三类跳转非空表证明后置 L0134/deferred 撤销；三 source 相同
AST/symbol index 区分 consumer 根/alias 与定义文件的共享字段符号，公开/raw完整相等。

| 验收项 | 实际选择 | 本地结果 |
|---|---|---|
| 字段绑定与既有直接借用合同 | `unit_direct_call_loan_ends`，沿用5项Abort整项排除 | 230 passed，含19项新合同 |
| raw撤销及跨source字段身份 | `--lib adjacent_borrow_results` | 23 passed，396项未选 |
| 相邻借用及receiver回边 | continuation、nested、backedge，nested沿用1项Abort排除 | 14 + 9 + 4 passed |
| Unit iteration退出 | `ownership_iteration::control_exits_unit` | 6 passed，196项未选 |
| 两阶段receiver合同 | `multifile_two_phase_borrows`，沿用2项Abort排除 | 25 passed，2项未选 |
| 静态及跨crate | frontend all-targets严格Clippy、workspace all-targets check、fmt | 全部通过 |
| 文档及尺寸 | `check_docs.py`、`check_rust_sizes.py --base 994d916a`、diff | 通过；863 Markdown、2234 Rust、35项既有超长 |

Mac AArch64、Rust1.96、LLVM21.1.8，locked/offline、默认debug、jobs=2；本工作树专属全新
Cargo target 串行，不跨树复用。共311项不同普通测试，不重复累计先行复跑；继承§129
八项Abort整项仅编译。未跑native、Linux、故障注入、压力或frontend/workspace全量。
完整9文件、Architecture、本节及实际日志经独立fresh-context只读复审，未见P1/P2；
复审未重跑Cargo。
不增加尺寸例外。D10/E2及完整借用生命周期仍未完成；不再备份，不覆盖已有分支、不强推
或合并，connector发布与精确头CI另核。


### D10/E2：String.clone 独立正常完成观察（2026-11-08）

在 `e4d6dbbe` 基线上新增有界 `UnitStringCloneLoanEndFact` 与
`CompilationUnitOwnership::string_clone_loan_ends()`。只在 builtin Clone receiver 的
planner 遍历正常完成后记录原 Shared loan，保留 call、argument、target、kind 与完整
Span 身份，完成点为 CallReturn。独立 vector 不参与 pending loan、iteration actions、
owner/drop 算法，也不改变 native 能力。后源诊断或任何 deferred 在 raw/public 两层
撤销整表；已完成的内层 clone 与随后 return 的未完成外层调用严格区分。

本表仅为静态 planner 观察，不承诺动态执行次数、全 CFG 不可达裁剪或跨其他事件/drop
表的统一总序。MemoryDocument/Rooted 固定新 oracle 核对借用根、具名源、嵌套临时源、
return 后未遍历调用、未完成外层 receiver，以及普通同名方法和 List/Map size 邻控。
原正向 fixture 增加完整事件与 loan 对照；原 Single 入口、原源码及其独立 CallReturn
见证全部保留，本片旧消费者退休数为零，D10/E2 仍为 in-progress。

先记录借用根 expected 1 / actual 0 的真实红灯，再接入正常完成观察。首次新测试的
模块路径/导入问题已修正；17/19 阶段的两处失败分别是新 oracle 错把 Group temporary
当作内层 call，以及把 size 属性写成方法。沿原 AST 分类/语言合同修正测试预期，
未更改生产 target 或 size 语义。四个普通源（借用根、嵌套临时、while/break、外层提前
return）在 owner observation enabled/disabled 下比较完整 Analysis，并固定非空新表
数量 1/2/1/2；原 owner 清理与 Exclusive return 前缀邻控继续保留。

| 验收项 | 本片实际结果 |
|---|---|
| String.clone 原合同与五个新独立见证 | `string_clone` 19 passed，两入口均覆盖 |
| raw 前源成功、后源诊断/deferred 撤销 | `adjacent_borrow_results` 7 passed，含新 clone 事件；未选择 CI-only 注入 |
| owner 开关完整产物邻控 | 新非空 clone、原 cleanup、原 Exclusive return 三个 exact 测试各 1 passed |
| clone/replace/swap typed 交叉合同 | `clone_primitive_integration` 5 passed |
| 静态与跨 crate | frontend all-targets Clippy `-W clippy::all -D warnings`、workspace all-targets check、fmt、docs、尺寸、diff 全通过 |

本片共 34 项定向测试，不累计重复运行。Mac AArch64、Rust/Cargo 1.96、LLVM 21.1.8，
locked/offline、默认 debug、jobs=2，同一独立 target 串行执行。文档 863 份、手写 Rust
2220 份，35 项既有超 1000 行；三个父文件仅新表传递、门控与一个领域调用接线，沿用
原例外额度与拆分计划，不提高上限。独立实现复审与补充邻控复审无阻断。

恢复时核对 main 已为 `e1b0c2dc`，主线新增文件与本片没有修改路径交集；本地候选仍基于
上述原基线，此处结果不冒充最新 main 的组合验收。仅本地提交及 format-patch 备份，
未推送、未合并、未触发本片远端 CI。未运行 frontend/workspace 全量测试、Abort、
CI-only 注入、native 或 Linux。正式 CI 中原 stage 集成选择 `string_clone` 与
`clone_primitive_integration`，core 选择 lib；配置存在不代表本片已获得 CI 绿灯。


## 139. Receiver 与 String.clone 生命周期候选合批（2026-11-09）

本批从已合入 #384 的 main `14297820786476e7d85bf5010ed6861904dae03e` 新建
`fix/spec-0341-borrow-lifecycle-integration-20261109`，仅复用 receiver 已发布链
`4f7549eb691b87df719f3ecbb024becc50fcbb4f` 与独立 String.clone 候选
`8b6dc2e3a9579b130d03647e70acd242723e0446`，不继续拆分新的观察资格。
receiver 的增量为已在 main 的 `11c93d62` 之后24提交，clone仅一个提交；不重放
#372/#374、历史批次或另一条动态 owner 链。准备一个借用生命周期 Draft PR，原分支
不变，不强推或合入 main；此处历史各片的“未推送”描述仍是当时证据，不代表本批状态。

本地三方合流 `44520ecc`（main、receiver 双父）及 `2a33ea05`（前者、clone 双父）
保留两候选祖先。生产代码除已有接线外没有新语义改动。receiver 合流仅 SPEC 追加段冲突；
clone 的 dataflow 清空、owner 开关测试、raw 撤销测试及 SPEC 四处冲突均双侧保全。
main 的 entered/dispatch/completed-argument、CaptureIndex、while/短路与 Nothing
完成性代码保留。27条候选路径是原两增量的并集，不以旧候选整树覆盖新 main。

组合合同提交 `94e03e031b84f22b9fcf7ff28535f8d87eb6c0c6` 追加一个65行 integration
模块及两项 raw/owner 测试；现总差异28文件。双 Unit 入口的 Return/Break/Continue
各格同时产生 receiver normal/prefix/clone 三表1/1/2：已完成 clone 不因 outer receiver
退出丢失，不可达后缀仍有 typed descriptor 但没有完成事件。raw 合同先核三表非空及
公开/raw完整相等，再追加后来源 L0134/deferred，逐表核两层清空。复审建议的公开
receiver 两条空断言已补齐并重跑整个 raw 族。owner 开关仅清 owner observations 后
比较完整 Analysis，保留 terminal-if 与 clone 原合同，不忽略任何新表。

以下是同一组合代码树的实际本地验收，不沿用旧片311项或旧主线CI：

| 验收项 | 选择 | 结果 |
|---|---|---|
| receiver全链及双入口组合 | `unit_direct_call_loan_ends`，继承5项Abort整项不执行 | 231 passed，5 filtered |
| clone与primitive邻控 | `string_clone`、`clone_primitive_integration` | 19 + 5 passed |
| raw及公开原子撤销 | `--lib adjacent_borrow_results` | 25 passed，403 filtered；补强后整族重跑通过 |
| owner开关完整Analysis | `--lib ownership_checking::compilation_unit::dataflow::drop_planner::owner_transport::tests` | 30 passed，398 filtered |
| borrow/receiver邻控 | continuation、nested、backedge、two-phase，沿用3项Abort排除 | 14 + 9 + 4 + 25 passed |
| Unit循环退出 | `ownership_iteration control_exits_unit` | 6 passed，196 filtered |
| main flow与callable邻控 | `while_flow_facts`、`multifile_ownership_checking callable_provenance::` | 12 + 25 passed；后者139 filtered |
| 静态与跨crate | frontend all-targets严格Clippy、workspace all-targets check、fmt | 通过；最终测试补强后均复跑 |
| 文档、尺寸及差异 | `check_docs.py`、`check_rust_sizes.py --base 14297820`、diff | 通过；863 Markdown、2244 Rust、37项超过1000行；不增额度 |

共405项不同普通测试，首轮3项与后续复跑不重复累计。Mac AArch64、Rust1.96、LLVM21.1.8，
locked/offline、默认debug、jobs=2，新的工作树专属target串行运行。继承的8项Abort整项
及callable邻控的 `call_entry_facts_distinguish_an_exiting_prefix_from_a_dead_operand`
（含真实 error/Abort 源码）仅编译未执行；没有修改CI selector或删除这些测试。
未跑 native、Linux、故障注入、压力、frontend/workspace全量或正式双宿主/Preview CI。
独立只读复核完整28文件、三项新合同与日志，未见P1/P2；405项无重名，三方事实与历史
账本保全。复审未另跑Cargo；最终文档结构检查复跑通过，精确远端头CI另核。

该批将已有能力集中交付，不提供统一跨表全序、全CFG/absence/动态实例证明或native权限恢复，
不退休原 Single 见证。call-derived/父子alias/provider等既有边界不扩展；D10/E2保持进行中。

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


## 146. 闭合 While 的真实末尾 Continue/Break 完成边（2026-11-09）

先核全部178个远端分支、9个开放PR及最新main `7de209f7`；旧loop边界/条件路由已在main，
#392借用索引及#393独立消费者没有本片实现。隔离分支从main新建，以 `254988d4` 组合
已发布owner #390 `5cd44ae6`，父级依次为main和#390，无冲突；原#390及其它分支未修改。
按用户合批要求，完成后将本地提交交统一整合，不自行另开PR，不强推或合入main。
本节只扩§144的Phase 3只读观察，不改Guide语义、owner运输validator或普通cleanup行为。

合同允许原本地String声明序列后接唯一末尾裸Continue/Break。至少一个局部声明，精确
Nothing类型、同一body最后statement和实际expression身份均须成立；Group跳转、不可达
后缀、Return、分支、嵌套/多循环、外来或跨轮owner、capture及其它原拒绝范围不准入。

AST预筛仅保存期望。实际jump路径仍依次结束pending loan、drop_deeper_than及
finish_jump_iteration，随后观察hook核对fresh body Arc、精确jump、实际kind、唯一最近
loop boundary、scope及完整空状态，才记录一次性completion。旧body返回false时必须有
同Arc/同kind/同expression见证；单独false加空状态不能生成边。错误传播前恢复所有sink并
清除active标记、消费见证。fallback路径和普通iteration/provider清理不改变。

新增ContinueBackedge从实际ControlTransfer(jump)回condition；BreakExit从实际
ControlTransfer(jump)到该While的LoopExit，不冒充fallthrough或condition-false。
公开三边与私有completion分别保存并核对，保留outer/body双identity封印。原V1投影仍
要求全部真实definition最终Dropped，仍拒绝ControlTransfer Drop；现准入String已按ASAP
在jump前退休，本片不搬动drop点，也不宣称增加跳转点owner析构能力。false条件零轮，
true条件在Continue/fallthrough可持续循环，在末尾Break路径一轮退出；没有运行计数或phi。

真实红测试提交 `1b10670c`：合法Continue源码因缺少循环产物失败（0 passed/1 failed）；
最小生产接线后同测试的Continue/Break两输入通过。完整实现和补强测试提交 `a8a01329`，
tree `b73b83e5de2524fb597abc55a239a881e088215b`。后续文档不改变受测Rust。

| 验收项 | 实际选择 | 结果 |
|---|---|---|
| 真实jump生产者 | `closed_while_transfers` 三项ordinary；32正例、17合法拒绝、跨文件/分析隔离及真实错误清空 | 3 passed，0 failed/ignored |
| 已有owner组合 | §144的59项普通lib exact，连同新3项执行 | 62 passed；独审补强后新3项再次通过，不重复计数 |
| 公开双入口与循环邻控 | `closed_while_owner_unit`、8项`loop_choice_unit`、4项原`loop_carried_choices`真实源码 | 13 passed；公开双入口含8个完成/拒绝组合 |
| 主线邻控 | 5项control/drop、2项capture、1项scope/return借用清理 | 5 + 2 + 1 passed |
| 质量门禁 | frontend all-targets Clippy `-D warnings`、workspace all-targets check、fmt、docs、尺寸、diff | 通过；865 Markdown、2250 Rust、37项既有超限 |
| 独立复审 | 实际cleanup同序、active/witness/restore、封印、测试非空及当前架构 | 无blocker；新3项lib与公开双入口1项独立exact复跑各1 passed，HEAD/tree及二进制SHA256前后不变 |

合计83项不同ordinary测试；Cargo使用 `--locked --offline`、专属target串行，Mac AArch64
Rust1.96、LLVM21.1.8。32正例核实际AST jump/condition/body、边kind/target、move/replace
及真实drop facts，完整Analysis观察开关对照不变。两种旧jump拒绝源码明确迁为正例；原
Return在内的其余25个拒绝输入保持拒绝。跨文件用例在两次validate成功、均恰有两个观察项后
核token隔离，修复独审指出的空集合假通过风险。双公开入口逐expression核实际jump及目标。

新损坏seal守卫（kind/point/target/catalog、伪造ControlTransfer Drop）与既有故障注入、
手工模型、Abort仅编译，未本机执行。原jump完整处理移至所属loop模块后，旧大planner
从1560行降到1546行；新collector465行，无尺寸例外或baseline扩额。完整测试、Linux、
native及统一整合后的精确头CI尚未执行，不能由本片83项替代。

loop-carried header/exit phi、一般跳转合流、动态多实例capture/释放、通用消费者、Single
见证退休及Phase 4/native仍未闭合；D10/E2保持in-progress，不归档。当前准入与边界见
[闭合While架构页](../../architecture/unit-loop-owner-transport.md)。


## 单一 PR 的 owner、借用与消费者保全整合（2026-11-09）

以现有 #390 为统一交付入口，保留 #391、#393 的原分支、Draft 与提交历史。
输入为实际 main `369fff4c0e8cf60b20d2937ef1041f65579a04bd`、已发布 owner
`5cd44ae66104bf046260e3e1620333ba8d880d00`、borrow
`a94a83d7202e7c23c302f66d9efa17e71cb56832`、consumers
`60e24f4042d949ed008763cff310cb316f53da55`，及 §146 的已完成跳转链
`1b10670c → a8a01329 → 4e4c0d90`。全部输入以父链保留，不 cherry-pick 重放、
不强推、不覆盖原工作树、不关闭原 PR，也不合入 main。未提交的一跳 parent-binding
后继不在本批范围内。

独立复审先核四路保全，随后核 §146 和最新 main #392 的合流：单侧文件逐 blob
相同，共同文件保留两侧独立变更；导出及测试注册无缺失或重复，SPEC 原附录完整且
各出现一次。#392 的只读 provenance 索引保留，receiver 的三个根成员查询替换为同一
不可变 checker 的集合查询；Borrow/Inout、字段、具体 this、首层 binding 与真实
正常/前缀退休资格不缩窄。alias 先归一为 canonical source，This provider 原检查保留。

最终源码候选为本地 `5756101f26ffd360a330184346f95c839cdc996f`，完整树
`f57443531e7908bfa19e16a507a36b35030efd06`；下列验收均针对该组合，本文追加仅改文档。
Mac AArch64、Rust 1.96.0、LLVM 21.1.8、locked/offline、debug=0、jobs=2，独占新 target
串行执行；按 target 与完整测试名去重，不累计前两轮旧 main 候选的重复运行。

| 验收项 | 实际范围 | 结果 |
|---|---|---|
| 组合 ordinary lib | §139/144 的原普通选择、§146 新3项及 raw 撤销 | 101 passed |
| receiver 与借用运输 | `unit_direct_call_loan_ends` 原231项加 #392 新3项身份隔离 | 234 passed |
| 其他普通消费者与邻控 | canonical_type_tables、clone_primitive_integration、control_branch_join、final_type_admission、multifile_constant_qualification、multifile_ownership_checking、multifile_two_phase_borrows、ownership_borrow_continuation、ownership_checking、ownership_closures、ownership_iteration、ownership_nested_borrow_arguments、ownership_receiver_backedge、ownership_structural、parser_interface_delegation、string_clone、type_callable、while_flow_facts 的既有普通选择 | 394 passed |
| 静态与跨 crate | frontend all-targets Clippy `-W clippy::all -D warnings`、workspace all-targets check、fmt、diff | 全通过 |
| 独立保全与结构 | 三阶段逐 diff/父链复审、docs、相对 `369fff4c` Rust 尺寸 | 无 blocker；865 Markdown、2262 Rust、37项历史超长，不改额度 |

合计729项不同普通测试通过，20个 target，全部实际命中。手工模型、故障注入和 Abort
整项仅编译，未在本机执行；未运行 frontend/workspace 全量测试、native 或 Linux。
既有 CI selector 不变；本地组合通过不代替更新后 #390 精确头的正式双平台及 Preview CI。
Draft 与 SPEC-0341、D10/E2 的 in-progress 状态保持，不据此归档或宣称一般动态生命周期闭合。


### 直属资源闭包的 Unit 规划前置（2026-11-09）

隔离基底 `369fff4c0e8cf60b20d2937ef1041f65579a04bd`。原
`single_resource_deinit_inside_owned_closure_uses_body_and_environment_cleanup` 先实跑通过；
完整146字节原源经 strict Unit 四阶段零诊断，却在 `Resource()` 的 `[120,130)` 返回
`UnsupportedNode`。原源 SHA256 为
`ed4f27c12d1017ce5df15f917d0c154bbfcde6f52fab97dc4fdb307938aa67b0`；不改源、
不把原成功换为双方拒绝。Legacy 成功见证保留，本片旧入口净退休0。

最小生产改动在 Unit instance planner：消费实际 FunctionValue call 的可派发事实与
callee 的 sealed Lambda 来源，将已进入的同源 lambda 加入当前实例的 resource 集合；
其 deinit 继续使用原 nominal worklist、具体类型及 ABI 验证。不从 AST 名称猜来源，
不放开 unused lambda、后参退出、嵌套 lambda 或任意 closure ABI。

新增 Unit 对照保留 body 一次 Resource allocation/drop、一次 callable invoke 与独立
owned String 环境 drop，并把 thunk、deinit 注册和 capture 类型绑定到真实产物。
另验 Group/alias 的两次调用共用原 thunk，以及无调用和 `callback(return)` 两个
真实源保持精确 Resource construction 拒绝。普通 native 核原源 `keep\n`，及两轮
`keep/body/released` 后 `after` 的完整 stdout、exit 0 与空 stderr；不把此输出当作 allocator 计数。

| 验收项 | 命令或过滤器（`cargo test --locked -p lang-codegen --lib`） | 实际结果 |
|---|---|---|
| 原成功见证、Unit资源清理与边界 | `ssa::lower_frontend_tests::resource_deinit_tests::` | 21通过 |
| frozen callable routes | `ssa::unit_plan_tests::callable_identity::` | 16通过 |
| 直接closure下游 | `ssa::unit_lower_closure_tests::` | 10通过 |
| 原源与重复资源析构native | `native_tests::resource_deinit_tests::direct_resource_closure_native_keeps_original_output_and_repeated_drop_order` | 1测试、2程序通过 |
| 既有owned String环境native | `native_tests::move_closure_drops_or_invokes_its_owned_string_capture_once` | 1通过 |
| 工程门禁 | fmt、codegen all-targets严格Clippy、docs、相对基底sizes与diff | 全部通过；docs863页，sizes2235个Rust文件及37项既有超长，未加例外 |

合计49个唯一Rust测试，baseline、红测与修复后的重复exact不重复累计。Mac AArch64、
Rust1.96.0、LLVM21.1.8，独占本工作树target串行执行，默认debug。完整原始日志留
`.local-artifacts/`，不入Git；本段仅记录稳定输入与简要结果。独立只读复审未发现阻断项，
核对dispatch/provenance、模板作用域、原Legacy与新Unit资源oracle；复审不冒称重跑Cargo。
同函数双lambda只选一个的额外隔离负控为非阻断建议，本片未新增。未运行完整codegen/
workspace测试、Linux、故障/计数/sanitizer或新头官方CI。用户已批准本地提交交统一批次，
本片未推送、未创建PR、未合并；D10/E2与本Spec保持in-progress。

### 统一候选同步 #394 与资源闭包（2026-11-09）

本轮只在已有统一隔离分支推进，不新建 PR。`5724164b2f8fd4186bf1c41f7864a2a33c1c5d1d`
保留原统一候选 `16c7dff7` 与实际 main `a4b72d350f9616115a9d5fbed13ca761828ba728`
为有序父级；后者已合入 #394 的借用结果初始化前缀退出。`607dba546e90d2cd2af41dcf9027bc94a4d01838`
再保留该组合与资源闭包候选 `e1e72006cd027e704f1ad01e88b79b7254233d6e` 为父级。
未重放提交。独立逐文件核查确认所有单侧代码原样保全，唯一共同 SPEC 修改精确保留
公共基底和双方追加正文各一次。资源闭包原 Legacy 成功见证仍保留，旧入口净退休0。

交叉测试提交 `a0ce1d3ecc4222d1601cc65ee288c29c6aa28b98`，完整树
`995400626622b7e4f5a1cd6fc0cf2618a405ff91`。本轮不再修改生产实现，新增三个测试：
已派发资源闭包之后的无条件/单臂借用初始化 return、同函数另一个未派发资源 lambda
隔离，以及两种 flag 的真实 native 输出。SSA 保留真实 thunk 的 Resource drop/deinit；
条件退出另逐 block 核正常分支的 source loan、退出分支同源 loan end 先于对应 owner drop。
未派发 sibling 的 unused 与 `callback(return)` 两例均保持第二个 `Resource()` 的精确拒绝范围。
native 核完整 stdout、exit 0、空 stderr与可观察 Resource 析构顺序，不将输出当作
String 环境/owner 的分配释放计数或无泄漏证明。

| 本轮实际选择（`lang-codegen --lib`，二进制逐全名 `--exact`） | 结果 |
|---|---|
| `ssa::lower_frontend_tests::resource_deinit_tests::` | 23 passed，含2个新交叉 SSA测试 |
| `ssa::unit_plan_tests::callable_identity::`、`ssa::unit_lower_closure_tests::` | 16 + 10 passed |
| `ssa::borrow_result_contract_tests::` | 12 passed |
| `native::unit_tests::borrow_result_tests::native_borrow_initializer*` | 3 passed |
| 原资源闭包输出、原 owned String环境、`resource_closure_native_composes_with_borrow_initializer_return` | 3 passed，含1个新交叉 native测试 |
| codegen all-targets Clippy `-W clippy::all -D warnings`、workspace all-targets check | 全通过，均 locked/offline |
| fmt、diff、docs、相对 `a4b72d35` Rust尺寸 | 全通过；865 Markdown、2263 Rust、37项历史超长，未改额度 |

合计67项不同测试全部实际命中，0 failed/ignored；不重复累计首轮3项运行。Mac AArch64、
Rust1.96.0、LLVM21.1.8，locked/offline、debug=0、jobs=2、独占原统一 target 串行。
完整命令、选择和结果留本机临时验收目录，不入 Git。独立复核确认67个选择与成功项完全一致，
交叉测试无阻断项。frontend、标准库、Cargo/工具链输入相对 `16c7dff7` 未变，复用前节729项
frontend证据，不称本轮重跑，也不把两组相加冒称新组合全量验收。

未运行完整 codegen/workspace 测试、Linux、额外故障注入或 sanitizer；新增资源交叉 native
没有 allocator 计数断言。旧 #390 CI run `42629723592` 的输入是 head `51d8dc72`、base
`369fff4c`，不含本轮组合，不能据此宣称新候选正式 CI通过。本轮未更新远端、取消或重启 CI，
未合并 main、关闭其他 PR或改 Draft；后续发布仍须核最新分支与适用授权，再验精确新头。
SPEC-0341、D10/E2保持in-progress，本片不封闭一般动态生命周期、完整迁移或 Phase 4。


### 普通 codegen frontend 边界消费者迁移（2026-11-09）

隔离基底 `a4b72d350f9616115a9d5fbed13ca761828ba728`，未叠入资源闭包候选或修改
在途 nullable/借用工作。只迁移 `lower_frontend_tests` 的三个真实 frontend 拒绝输入
（整数越界、missing name、非法 break）和两个原成功输入（for-loop、String泛型identity）。
两个原测试中的全部7份源码逐字核对相同；numeric when/Elvis 的原Legacy拒绝及其余
mixed-analysis、guarded-cleanup见证保持。文件内 `lower_scalar_file` 调用21→16，
旧 `analyze` 调用22→17，净退休5个真实消费者；不删除仍有调用方的旧helper或checker。

先在原路线仅追加临时诊断打印，实际运行原两项exact，冻结4条完整诊断；临时打印已移除。
新helper核对severity、code、完整message、source名/原字节、primary和label Span、
全部有序details：两条L0090各带原expected-type标签，L0080/L0142无附加detail。
strict Unit分别在names/types门禁拒绝，不能取得可执行Unit capability；本片不再测试
Legacy lowerer的FrontendDiagnostics枚举，也不冒称拒绝后的阶段能力或后端行为等价。

正例仍用原MemoryDocument及真实声明，不添加wrapper、package或own。for-loop保留
唯一iteration fact、verified SSA并追加LLVM、一份容器owner与一次Drop；String泛型
锁实际identity声明、两个具体函数、owned String参数/返回、唯一调用及无Drop的owner交付。
首轮旧显示名 `identity<String>` 在Unit不相同；改用精确declaration origin与String ABI
验证原实例语义，不制造兼容名字或改源。所有生产代码、共享harness和公开API不变。

| 验收项 | codegen `--lib` exact目标或命令 | 本地实际结果 |
|---|---|---|
| 5个迁移消费者及保留的2个旧边界 | `rejects_out_of_range_integer_literals_before_ssa_construction`、`diagnostics_and_unsupported_bodies_fail_without_partial_programs` | 2项通过 |
| 既有Unit相邻合同 | `lowers_real_scalar_expression_functions_through_verified_ssa`、`lowers_straight_line_blocks_locals_assignments_and_returns`、`non_null_assertion_does_not_register_unreachable_inline_types` | 3项通过 |
| 原插值拒绝门 | `rejects_nested_interpolation_in_error_at_whole_file_boundary`、`rejects_nested_interpolation_in_println_at_whole_file_boundary` | 2项通过 |
| 工程门禁 | fmt、codegen all-targets严格Clippy | 通过；新测试clone告警已修复并重跑 |

以上目标均使用完整 `ssa::lower_frontend_tests::` 前缀、
`cargo test --locked -p lang-codegen --lib ... -- --exact`。合计7个唯一测试，旧基线与重复执行不累加；Mac AArch64、
Rust1.96.0、LLVM21.1.8、独占本工作树target串行、默认debug。原始日志及仅观测diff保留
Git忽略的`.local-artifacts/`而不入Git。独立只读审查与最终docs/sizes/diff结果在本地交付核对；
未跑整个mixed目标、全量codegen/workspace、native进程、Linux、故障/计数/sanitizer或
精确新头官方CI。未推送、新开PR或合并；仅交统一批次，D10/E2及本Spec保持in-progress。

### 普通 codegen 边界批次的统一组合验收（2026-11-09）

实际 main 仍为 `a4b72d350f9616115a9d5fbed13ca761828ba728`。统一合流提交
`439b6d2eceeaf525c5ed0a6f034491e78d7b1241` / tree
`b15ab65d76d3d9725b8c8e0ee2a0a9727905c712` 的有序父级为原资源组合
`610c1a3a5d8b6a693a49891e3958814c3e1f3df7` 与消费者批次
`1426ed1beebdba9f80780d335194e19b00561400`。保留双方父链，不重放提交。
独立核查确认统一侧71个单侧文件、新增侧3个单侧文件逐blob保全；唯一共同SPEC
精确为原统一全文加候选的新增段，各出现一次。未吸纳尚未合并的 #395。

计数口径更正：前段保留的 `analyze` 22→17实际是文本命中数，混入了函数定义与
`UnitSource::analyze`；真正旧Legacy `analyze(...)` 调用为20→15。
`lower_scalar_file` 调用21→16及本批净退休5个真实消费者不变。全部7份原源码逐字保全，
四条完整诊断与原观测相同；shared UnitSource与生产实现均未因本批修改。

本组合在同一codegen lib二进制逐完整名称 `--exact` 运行74个不同测试：
前一资源组合67项（23资源SSA、16 callable identity、10 closure、12借用合同、
3借用native及3资源/环境native）与本批既有7项，实际全部通过，0 failed/ignored。
独立核查选择文件、命令和成功项集合完全相等，既有67与新7无重叠；此前运行不重复累计。
codegen all-targets严格Clippy `-W clippy::all -D warnings`、workspace all-targets
check、fmt、diff均通过；docs865页，Rust尺寸2264文件及37项历史超长通过，未改额度。

Mac AArch64、Rust1.96.0、LLVM21.1.8，locked/offline、debug=0、jobs=2，独占原统一
工作树target串行。命令与逐项日志留本机临时验收目录，不入Git。frontend/标准库与
Cargo/工具链输入仍未变，原729项frontend证据仅复用，不声称本轮重跑。
未运行整个mixed目标、完整codegen/workspace测试、Linux、额外故障注入或sanitizer；
新增资源native输出仍不是allocator计数或无泄漏证据。仅本地合流与验收，不更新远端、
不取消或重跑旧CI、不合并main；发布仍须核当前分支及适用授权并验精确新头。
D10/E2与SPEC-0341保持in-progress，不由本批5项退休推导旧入口全部清除。


## 148. 闭合 While 尾部 If 的真实双臂跳转边（2026-11-09）

启动时先核180个远端分支、9个开放PR及本地成果；当时main为 `a4b72d35`，统一#390为
`734dde83b156c533558efd5060a2a39a110537d2`，当时#395为 `9f55dff2` 的nullable/构造借用
修正，不包含本片。隔离分支从main建立再快进到#390，复用已发布§144/§146，不重做
旧循环见证。本片不改现有PR、不另开PR；按用户最新要求仅本地分步提交，留统一批次发布。

只扩Phase 3闭合body观察的控制前置：一个While，已有String/MoveOnly声明前缀后接唯一
末尾显式If，每臂恰一个裸Continue/Break，四种kind组合均可。两个无mode marker的精确
Boolean参数分别供While和If读取不同真实symbol，条件只接Name/Group；原outcomes必须
都保留true/false。单参数直线完成合同不变。同一参数、其它条件、Return、空臂、Group
跳转、臂内声明、嵌套If、不可达后缀及其它原拒绝范围不准入。

AST只保存候选。原If condition完成后先核完整空ValueState及原V1已封闭owner前缀；每臂
入口锁control、branch和body，实际jump仍先执行原loan/owner/iteration清理。跳转见证
包含fresh body Arc、kind、expression、最近loop和scope；各臂实际返回后、状态合并或
丢弃前，另核tail、false和完整空状态，再消费各自pending见证。两臂都完成且事件都为空，
才只抑制私有body sink的空If事件，保存独立completion。异常先恢复observer/Choice/depth
并清空split/arm/pending标记；不从合并后的空状态推定两臂成功。普通If事件和V1不放宽。

新增source-qualified IfBranch，保存control、condition、branch序号与arm body。公开edges
改为只读slice，原直线三边不变；条件完成六边顺序为EnterBody、then分支及其真实jump、
else分支及其真实jump、ConditionFalseExit，私有completion独立重建并核对。String仍按
ASAP在If前退休，没有ControlTransfer Drop、owner join、carried phi或运行实例。两个参数
在调用内不变，If同次调用选同一臂，不能把六个结构边称为一条已执行轨迹或任意换臂循环。

真实红测试 `4fc18af54c1e7c4e97acf5b94221c66ff6aaa319`：合法Break/Continue条件源码
完成typed/ownership验证，但缺循环产物而1 failed；不是编译失败。最小接线后同测试复绿。
完整实现与来源断言补强提交 `ec515ac871abc82c4f40f3c8c7e08ad359e9fef1`，tree
`c38f20c0b74aca58c7ac684cd1d696666c043c70`；后续文档不改受测Rust。

| 验收项 | 实际选择 | 本地结果 |
|---|---|---|
| 条件完成生产者 | `closed_while_conditional::real_while_tail_if_records_both_completed_jump_edges` | 1 passed，32输入覆盖四kind对、Group/显式Unit及四类owner前缀 |
| 拒绝与原子发布 | 同模块 `conditional_loop_rejects_nonindependent_or_open_arms_atomically`、`conditional_loop_catalogs_are_separate_and_errors_withhold_publication` | 2 passed，18合法拒绝、两文件两次分析及真实ownership错误 |
| owner与全局分支hook邻控 | §146的62 lib exact + 本片3项 + `branch_boundaries`三项、`control_tail::shared_control_body_keeps_when_and_replacement_cleanup_unchanged`、两个`owner_observation_preserves_*`非空loan表对照 | 71 passed，0 failed/ignored |
| 公开入口与循环邻控 | 原`ownership_iteration`13项 + `closed_while_owner_unit::conditional_while_completions_match_both_unit_inputs` | 14 passed；新增MemoryDocument/Rooted共12输入组合 |
| 主线相邻合同 | 5项`multifile_ownership_checking`控制/drop、2项`ownership_closures::capture_identity_unit`、1项`ownership_borrow_continuation` | 5 + 2 + 1 passed |
| 工程门禁 | frontend all-targets严格Clippy `-D warnings`、workspace all-targets check、fmt、docs、尺寸、diff | 通过；865 Markdown、2266 Rust、37项既有超限；尺寸基底为734dde83，未改额度 |
| 独立复审 | 原遍历hook、各臂state丢弃前取证、错误恢复与source oracle | 无blocker；上述新3 lib与公开1项独立exact各1 passed，前后HEAD/tree与二进制SHA256不变 |

合计93项不同ordinary测试，既有结果仅用于选名，本轮对当前Rust全数重跑；10项循环初验
及重复复跑不累加。新增3项lib使用完整 `ownership_checking::compilation_unit::dataflow::drop_planner::owner_transport::tests::`
前缀；其它项保留各自模块路径，全部普通二进制逐完整名 `--exact`。32正例从实际While/If/ControlBody取完整ID，核六边
kind/目标、distinct jump、真实producer/move/replace和唯一Drop，观察开关的完整Analysis
一致。跨文件与重复分析先核两项非空、成功validate，再从fixture文件独立核callable、
String origin和所有边source，foreign token lookup失败。公开入口从index核SourceUnitId
和source_id，不用被测observation作为来源期望；实际While AST及双臂jump均独立对照。

Mac AArch64、Rust1.96.0、LLVM21.1.8，locked/offline、默认debug、jobs=2，原专属target
串行，未删除缓存或工作。测试选择及日志留本机临时验收目录，不入Git。新seal损坏守卫
（缺边、重复臂、交换后继、错误control、foreign catalog、伪造ControlTransfer Drop）及
既有手工模型、Abort、故障注入仅编译，未本机执行；未跑frontend/workspace全量测试、
Linux、native或精确新头官方CI。保持原Single见证与所有未准入拒绝；D10/E2和本Spec仍
in-progress。真实entry/backedge/exit owner快照与phi、通用控制合流、动态实例及释放、
Phase 4消费者仍缺，不由本片闭合模板宣称解决。当前事实见[循环观察架构](../../architecture/unit-loop-owner-transport.md)。

收尾只读复核时，#395已合入，main推进到 `9369cc7279c268385a19f41a10a54c7cc31ea2ce`，
#390仍为734dde83。该增量未改本片owner文件，但涉及共享call-loan产物；本片93项与独立
4项是上述已发布基线加本地切片的结果，尚非9369cc72整合验收。保留现有提交，后续统一
本地合流须验证精确组合，不把旧结果改记为新主线结果。


## 150. 闭合 While 的空臂正常完成与共享 body 回边（2026-11-09）

先核远端main `9369cc7279c268385a19f41a10a54c7cc31ea2ce`、#390 `734dde83`及本地在途
owner链，无重复空臂实现。隔离分支从最新main建立，复用已独立保全审查的依赖合流
`3580baa09b0563e351e81768a1f227505bd32835`（有序父main9369、#390734dde83），避免
重复处理#395注册冲突。随后以 `587d24960878eef485eb56194377d32cafbe3515` 保留父链
合入§148本地头 `d4a8d7a1`，无冲突；该片10个文件逐blob等同原owner头，main构造借用
与注册见证等同3580baa0。本地合流不代表行为验证，以下结果才对应本片精确组合。

本片仅扩Phase 3控制前置：闭合String前缀、两个独立Boolean参数及唯一末尾显式If不变，
每臂允许空ControlBody或一个裸Continue/Break。新增空/Break、Break/空、空/Continue、
Continue/空、空/空五种组合；原四个双跳转组合不变。正常臂存在时If必须精确Unit，否则
仍为Nothing，不能整体放宽jump类型。隐式else、非空正常臂、声明、其它尾值、嵌套、
Return、Group跳转、后缀、同参数/常量条件及外来/跨轮owner继续拒绝。

空臂原控制遍历只恢复scope并返回true/None tail，不执行drop_scope。本片不添加清理，
只在真实drop_branch_exit之后、状态push/merge之前，核actual arm、None tail、true、
无owner result、无pending jump、完整空ValueState及同Arc/control/scope/depth，形成
正常臂见证。跳转仍需要原cleanup之后的kind/expression/Arc pending见证；各臂独立消费，
不从merged空状态推定。split只在两臂完成、事件为空且实际continuation数精确等于空臂
数时完成；最后仍须实际statement(body)返回值匹配私有expected.continues，完整body
空且原V1封闭，才能封存回边。原错误传播前恢复/清空事务的顺序保持。

公开IfFallthrough(arm body, control)表示实际空臂正常返回If，不是Drop或arm scope
cleanup点。有空臂时七条边为EnterBody、两个IfBranch及各自后继、唯一共享body Backedge、
ConditionFalseExit。共享回边只能来自实际body正常完成及scope cleanup，空/空不重复
生成两条body回边。旧直线三边与双跳转六边不变；私有三态arm completion与公开边顺序、
来源身份分别核验，没有不存在的jump ID哨兵。ASAP、V1、Choice及普通callable的
Unsupported行为均不变，没有owner join、header/exit phi、动态实例或Phase 4能力。

真实红测试 `c67be20f710c18d78a87047baef8d10bcfaef088` 使用合法Break/空臂完整源码，
typed/ownership成功后因缺循环观察1 failed。最小实现后同测试1 passed；中途脚本路径
笔误造成三处调用签名未接齐的编译失败，补齐后重新通过，不把编译失败算作行为红证据。
实现与完整测试提交 `ab84893aad720601b5ac8e35bd712abad0c5218c`，tree
`98a1b7d123346b0d329266383a6c5ee72783521a`；后续文档不改受测Rust。

| 验收项 | 实际选择 | 本地结果 |
|---|---|---|
| 正常臂真实生产者 | `closed_while_fallthrough::real_empty_arm_completion_retains_a_separate_body_backedge` | 1 passed，40矩阵输入及原§148完整拒绝源码迁入的1输入 |
| 拒绝、来源与原子发布 | 同模块 `normal_arm_rejects_unproved_tails_and_suffixes_without_partial_capability`、`normal_arm_catalogs_keep_actual_sources_and_error_atomicity` | 2 passed，12合法拒绝、跨文件两次成功分析及真实错误清空 |
| owner与共享hook组合 | §148的71 lib exact + 本片3项 | 74 passed，0 failed/ignored |
| 公开双入口与循环邻控 | §148的14 iteration exact + `closed_while_owner_unit::empty_while_arms_share_one_actual_body_backedge_in_both_inputs` | 15 passed，新入口MemoryDocument/Rooted共14输入组合 |
| 主线相邻合同 | §148的5 control/drop、2 capture、1 scope/return借用清理 | 5 + 2 + 1 passed |
| 无伪造空scope Drop | 上述新3 lib及公开1项补强后复跑 | 3 + 1 passed，重复不累加；actual empty arm的AfterStatement在真实drop facts与transport Drop均不存在 |
| 质量门禁 | frontend all-targets严格Clippy `-D warnings`、workspace all-targets check、fmt、docs、尺寸、diff | 通过；865 Markdown、2268 Rust、37项旧超限，尺寸基底587d2496 |
| 独立复审 | scope早退与branch_exit顺序、逐臂取证、body完成、封印及测试oracle | 无blocker；上述新3 lib与公开1项独立exact各1 passed，HEAD/tree及二进制SHA256前后不变 |

合计97项不同ordinary测试，全部以本片main9369依赖组合重新执行；旧日志仅供选名，
13项初验、3项公开初验与补强复跑不重复累计。新增3 lib使用完整
`ownership_checking::compilation_unit::dataflow::drop_planner::owner_transport::tests::`
前缀，其余保留各自模块路径，均按完整名exact。正例直接读取While/If/ControlBody实际AST
建立三种后继及完整七边期望，核来源、唯一body回边、producer/move/replace、唯一真实Drop
及完整Analysis开关不变。跨文件先核非空产物、两个成功analysis和实际callable，再验foreign
token拒绝。公开入口从index独立核SourceUnitId/source_id；旧空臂输入完整迁为正例，其余
旧17个条件拒绝与四个双跳转组合保留，不把新增能力伪装成旧测试放宽。

Mac AArch64、Rust1.96.0、LLVM21.1.8，locked/offline、默认debug、jobs=2，专属target
串行。测试选择与命令输出留本机临时验收目录，不入Git。新损坏seal守卫（缺失/重复body
回边、交换正常臂、错误control、伪Continue）及既有model/Abort/故障注入仅编译，未本机
执行；未跑frontend/workspace全量测试、Linux、native或精确头正式CI。没有删除缓存、
Single见证、旧分支或改Guide；按用户要求只本地分步提交，留明早统一发布，不推送或改PR。
一般有owner入边的header/exit phi、非空臂合流、动态多实例capture/释放、通用消费者及
Phase 4/native仍未闭合，D10/E2与本Spec保持in-progress。事实见[闭合While观察](../../architecture/unit-loop-owner-transport.md)。


## 152. 单个不变 String owner 的真实 While 非空边界（2026-11-09）

本片在新隔离分支从远端 main `9369cc7279c268385a19f41a10a54c7cc31ea2ce` 起步，
ff-only 复用 §150 `df9ef024a898498feeca3da4db802edada81631f`，保留 §148 与既有依赖父链。
开工核对 #390 `734dde83`、新 #396 `66886a08` 及本地统一队列 `4dfeda9e`：后两者分别为
借用路由/Guide litmus 与构造借用交叉测试，不含本片 owner producer；本片未复做这些工作，
也未把队列的757+74测试算为自身验收。用户主 checkout 的14297820仅为旧本地ref，未改动。

新增独立 `UnitOwnerInvariantLoopObservationV1` 与 getter；旧空入边loop、Choice、普通
V1及Single见证保持。准入为一个Boolean参数、三个精确语句：无插值String literal的
val绑定、直线闭合While、同symbol的Name移入另一个精确String val。body复用闭合本地
String/move/Group/root Replace；不接收输入owner的读取/move/replace、任何If或跳转，
不接受prefix/suffix的Group、var、nullable、额外语句、多循环或其它carrier。

真实前缀被普通While清空前，核Complete、唯一String producer、实际Result与Binding事件
及实际单个OwnedValue；保留其producer catalog，原legacy仍在While原子Unsupported。
condition完成后、body实际Ok(true)和scope cleanup后、实际LoopExit cleanup后三处各自
读取singleton状态，完整比较owner/token、symbol、origin、declaration、scope。ValueState
全部17字段穷尽处理，其它槽为空；新能力不从symbol或drop造owner，不复制entry作为回边。
body有独立Arc，原V1仍要求body-local definitions最终Dropped；公开三条结构边对应实际
statement/condition/body。整个分析成功且无deferred才发布，两种loop getter互斥。

输入catalog只证明真实producer；三份实际快照证明边界身份，不提供suffix事件、最终消费或外层闭合trace。
现有真实retired Drop只在兼容性测试核对，不拼入新能力。此为非空owner入边/phi前置，
不实现phi、一般loop-carried owner更新、动态实例释放、完整CFG合流或Phase 4/native。

红提交 `988771517232c7d5b8436d3bde479cd8aa71181b` 包含未接线只读API框架及真实源码：
typed/ownership成功后因新getter缺能力而1 failed。接线后同测试1 passed。实现/测试提交
`cace500bae741c47ca0b50435e9fd9d56301c73c`，tree
`67a742b91b9f8b1bed6ba5acbadf85959bfb3092`；下表均针对该Rust内容，后续文档不改Rust。
初始API导入及公开测试私有symbol构造器有编译错误，修正后重跑；公开MemoryDocument
误用多输入被既有索引拒绝，改为单源后通过，没有放宽索引。borrow-binding body被原所有权
拒绝，转入真实失败组，不将其计入合法否例。独审发现suffix符号类型未严格核String，已补
声明类型检查及合法nullable拒绝输入；这些开发中失败不计为最终通过或行为红证据。

| 验收项 | 实际选择 | 本地结果 |
|---|---|---|
| 非空边界真实生产者 | `invariant_while::actual_outer_owner_survives_three_while_boundaries` | 1 passed，3种condition Group形态×2种Unit写法×4种body，共24输入 |
| 准入与拒绝 | 同模块 `nonempty_loop_rejects_unproved_carriers_and_control_atomically` | 1 passed，18合法否例，最终观察非空且新/旧loop与普通V1均拒绝 |
| 来源与原子性 | 同模块 `invariant_loop_sources_and_analyses_do_not_share_owner_tokens` | 1 passed，两source、两成功analysis的catalog互斥；实际sibling错误与3种非法/未闭合body拒绝 |
| 公开Unit入口 | `invariant_while_owner_unit::invariant_while_boundaries_match_actual_sources_in_both_unit_inputs` | 1 passed，6片段×MemoryDocument单源/Rooted双源，共12分析、18 source inputs；9准入/9拒绝 |
| owner共享hook组合 | §150的74 lib exact + 本片3项 | 77 passed，0 failed/ignored |
| 循环与主线邻控 | §150的15 iteration + 本片公开1项；原5 multifile、2 closures、1 borrow-continuation | 16 + 5 + 2 + 1 passed |
| 质量门禁 | frontend all-targets严格Clippy `-D warnings`、LLVM21 workspace all-targets check、fmt、docs、尺寸、diff | 通过；865 Markdown、2272 Rust、37项历史超限，尺寸基底df9ef024，大planner物理行数不增 |
| 独立复审及复跑 | before-reject prefix、实际三边界、完整state、catalog隔离、原子发布及测试oracle；新3 lib+1公开 exact | 无剩余blocker；四项各1 passed，HEAD/tree及两二进制SHA256运行前后相同，树clean |

合计101项不同ordinary测试，均在本片main9369依赖组合重新执行。lib新增名带完整前缀
`ownership_checking::compilation_unit::dataflow::drop_planner::owner_transport::tests::`；
其余使用各自完整模块名exact。独立复跑及中途红绿/补强不重复累计。正例精确核四body
producer数1/1/2/3、Replace数0/0/1/2、input与body catalog隔离、真实AST三条边及实际
suffix Drop；on/off比较完整原Analysis。公开source预期从index独立读取并核原source_id，
合法否例先核最终观察数1，避免空集合通过。

Mac AArch64、Rust1.96.0、LLVM21.1.8，locked/offline、默认debug、jobs=2、独占target串行。
新手工损坏seal/deferred守卫及旧model/Abort/故障注入只编译、未本机执行；未跑frontend/
workspace全量测试、Linux、native或精确头正式CI。没有改Guide或删除缓存/旧见证/用户工作，
按用户要求只本地分步提交，不推送、不改现有PR。后续仍须与统一队列组合并验证，不能将
本片101项视为整批验收。D10/E2与本Spec仍in-progress；事实见[While边界观察](../../architecture/unit-loop-owner-transport.md)。

收尾只读复核时 #396 已合入，远端 main 更新为
`36d381e342c195f6c6a4718b098cd9b224f3f549`（父9369cc72+66886a08）。本次18文件差异
不含owner observer/loop producer，本片无重复实现；它含codegen borrow routes、Guide
litmus与SPEC-0392文档，仍须统一组合复验。上述101+4和workspace门禁只证明本片9369
基线，未整合或测试新main，不能替代36d381e3组合验收；保留受测链，交统一批次处理。


## 154. 真实 Continue 清理后的非空输入 owner 快照（2026-11-09）

本片先核最新 main `36d381e342c195f6c6a4718b098cd9b224f3f549`、#390 `734dde83`、
在途 #397 `fe1f47f8` 和本地统一队列 `4dfeda9e`；#397只改constant资格/SCC，未复做其工作。
新隔离分支从main起步，保全合入§152 `ee3a1f8719ed59fe6d4f80d927a6de01d5c6ff59`。
纯依赖合流 `f96c8a43a091e3ce11f7ed04bc7c137d28f82acc` 的有序父为36d381e3+ee3a1f87，
tree `9a8b576ba1c1c2beb248062e6b9d579e8433b6fa`。独立逐blob/mode复审确认共同基底
9369cc72上main18文件与owner83文件交集为空，完整index等于两侧变更并集，无冲突或额外文件。
该结论仅为字节保全，行为验证来自下表；未修改既有工作树或远端。

准入只在§152单个精确String val输入与后缀合同上增加真实末尾裸Continue。非空输入的
Break、If、Group跳转、非末尾/嵌套跳转、空owner body及外层输入读取/move/replace仍拒绝。
不改Guide、liveness、ASAP、cleanup、普通V1或Choice，也不扩parser的最近loop目标语义。

原jump hook在pending loan、deeper-scope owner与iteration cleanup完成后执行。它先核
active Complete body sink、精确expression、scope、最近loop、实际kind与expected，再对
实际state作完整singleton快照；成功后才登记completed。保存的continued含真实body Arc、
真实jump和binding，不能从entry复制。body实际Ok(false)返回后再独立读backedge，并继续
要求completed/expected/collected Arc相同及原body V1封闭。Continue不执行正常body
scope结束路径，不伪造Ok(true)、普通Backedge或ControlTransfer Drop。原condition state
仍经过实际LoopExit清理后另读exit；ValueState共17字段，values之外16槽保持空，完整
OwnedValue身份、来源、声明与scope均一致。

新 `continue_boundary()` 返回source-qualified jump和独立清理后binding；Fallthrough
必须None，Continue必须Some且同sealed completion/body Arc及其它binding。两种loop
getter互斥、input/body catalog隔离和最终successful/complete发布门保持。三个已有边界
加可选第四个jump读取是实际检查器状态证据，不是有限runtime轨迹；不可变true条件可
持续循环，不能据此称suffix已执行。仍无phi、outer suffix闭合、动态实例或Phase 4能力。

红提交 `22049664dfda4ee9dcaa255df01d46eb2dc4b8ca` 保留§152原完整合法Continue拒绝
源码：typed/ownership成功后因缺新能力1 failed。最小接线后同测试1 passed。实现和
最终测试提交 `ecc874bcbb82a5a5bd11cea63b5f663e685e80b6`，tree
`df431de24351e5ea28627741488c8b3a507d3aaa`；后续文档不改变受测Rust。

| 验收项 | 实际选择 | 本地结果 |
|---|---|---|
| Continue真实生产者 | `invariant_continue::actual_continue_retains_the_nonempty_input_owner` | 1 passed，24种condition/Unit/body组合+原完整拒绝源码1例 |
| 不支持的控制与输入 | 同模块 `invariant_continue_rejects_unwitnessed_control_and_open_inputs` | 1 passed，12合法否例，观察数1且三类能力均拒绝 |
| 来源及错误边界 | 同模块 `invariant_continue_keeps_sources_analyses_and_failures_separate` | 1 passed，两source两成功analysis；3错误/未闭合body不泄露能力，真实sibling错误清空全部观察 |
| 公开两入口 | `invariant_while_owner_unit::invariant_continue_has_independent_source_qualified_jump_in_both_inputs` | 1 passed，9片段×MemoryDocument单源/Rooted双源，共18分析、27 source inputs，12准入/15拒绝 |
| 原owner与共享hook组合 | §152的77 lib exact + 本片3项 | 80 passed，0 failed/ignored |
| 循环与相邻合同 | §152的16 iteration + 本片公开1项；原5 multifile、2 closures、1 borrow-continuation | 17 + 5 + 2 + 1 passed |
| 质量门禁 | frontend all-targets严格Clippy `-D warnings`、LLVM21 workspace all-targets check、fmt、docs、尺寸、diff | 通过；865 Markdown、2273 Rust、37历史超限，尺寸基底f96c8a43，未改尺寸豁免 |
| 独立复审与复跑 | 依赖字节保全、生产hook、一次性见证、API seal、测试oracle；新3 lib与公开1项exact | 无blocker；4项各1 passed，HEAD/tree及两binary SHA256前后不变，树clean |

合计105项不同ordinary测试，全部在本片36d381e3依赖组合重新运行；初验和独立复跑不
重复累计。lib新名带完整前缀 `ownership_checking::compilation_unit::dataflow::drop_planner::owner_transport::tests::`。
§152原私有Continue否例和公开完整输入均迁入本片正例，未丢旧源码；其余17私有否例、
5公开片段保留并重跑，旧fallthrough显式核continue_boundary None。新正例核真实AST
jump、三条完整边、四份边界binding快照、producer数1/1/2/3、Replace数0/0/1/2、每producer真实Drop、
跨catalog拒绝及完整Analysis开关等值。普通回边/BreakExit和伪造ControlTransfer Drop
明确不存在。公开source由index及原source_id独立验证，不用产物自证。

独审建议的仅变body Arc守卫已补；包括缺失/foreign snapshot、错expression、错kind、
Fallthrough多snapshot、deferred等手工损坏产物守卫仅编译，未本机运行。Mac AArch64、
Rust1.96.0、LLVM21.1.8，locked/offline、默认debug、jobs=2，独占target串行。未跑frontend/
workspace全量测试、Linux、native或精确头正式CI；不把main原CI或统一队列757+74算为本片
验收。收尾复核main/#390/#397头未变；仍需统一批次组合，不把105项称为整批验收。按用户
要求只本地分步提交，不推送、不改PR、不合main，D10/E2与本Spec仍in-progress。事实见
[While边界观察](../../architecture/unit-loop-owner-transport.md)。


## 156. 不变输入 owner 的真实 Break 出口能力（2026-11-09）

本片先核 main `36d381e342c195f6c6a4718b098cd9b224f3f549`、#390 `734dde83`、#397
`fe1f47f8`、本地统一队列 `4dfeda9e` 与 parser 分支 `2f49f39d` 及其在途 index recovery。
这些在途改动均无本片 owner producer，未重复实现。新隔离分支从main起步，ff-only复用
§154 `dc56394a1eb9bbfcef0c4036037fc6a6ae26132c`，保留其有序依赖父链，不增加合流提交。

仅在原单个精确String val输入、闭合body及精确suffix合同上增加末尾裸Break。新增独立
`UnitOwnerInvariantBreakObservationV1` / `invariant_break_observation_v1()`，其新增专属公开
方法为 `break_boundary()` 与 `body_completion()`，没有 `backedge()`。旧normal/Continue
类型和签名不变，旧getter仍拒绝Break。非空If、Group/非末尾/嵌套跳转、其它carrier及输入
owner读取/move/replace继续拒绝。普通V1、Choice、Single见证、ASAP和Guide均不变。

真实Break清理后独立读取jump binding，body实际Ok(false)后再独立读取body completion，
condition entry和实际LoopExit后exit仍分别读取。实际hook保留精确expression/kind、最近
loop、scope、fresh sink Arc及一次性expected/completed门，body仍须通过原闭合V1。四次
读取均穷尽ValueState的17字段，values外16槽为空，完整OwnedValue身份/来源/声明/scope
等于原真实输入；不从entry复制其它边界，不从AST、symbol或drop虚构owner。

原planner丢弃body state并对原condition state作LoopExit cleanup，未在此改为合流。仅
完整单例状态在上述四处相等的窄合同使该观察可成立，不是一般出口merge、exit/header phi
或更新后的loop-carried owner。三条真实结构边只有EnterBody、BreakExit与ConditionFalseExit，
无Backedge或ContinueBackedge。body locals仍按原ASAP退休，没有伪造ControlTransfer Drop。
私有单槽枚举区分Backedge/Break；共同边界检查不授予控制能力，两种seal分别限定实际
completion，外层getter继续核While Unsupported/callable/analysis Arc和最终发布门。

真实红提交 `75bbc90a0bc992ac97697d1c6f5aacb903f7d77f` 包含独立API框架和合法源码，
typed/ownership成功后因缺Break能力1 failed；最小接线后同测试1 passed。实现与最终测试
提交 `10bc9b8fdfda0c62705461ea7d4d5a4a3d2d0de5`，tree
`e493dbf6f40c39921460c47a7fa0bc78b1cb1f31`；后续文档不改变该受测Rust。

| 验收项 | 实际选择 | 本地结果 |
|---|---|---|
| Break真实生产者 | `invariant_break::actual_break_preserves_the_owner_through_distinct_exit_states` | 1 passed，24种condition/Unit/body组合+§152原完整Break拒绝源码1例 |
| 控制与输入边界 | 同模块 `break_exit_rejects_unproved_control_and_input_shapes` | 1 passed，20合法否例，含独立第二Boolean的If双Break；normal/Continue仍保留旧能力 |
| 来源与错误原子性 | 同模块 `break_exit_catalogs_keep_source_analysis_and_error_identity` | 1 passed，两source两成功analysis；3错误/未闭合body不泄露能力，实际sibling错误清空观察 |
| 公开两入口 | `invariant_break_owner_unit::invariant_break_exits_have_no_backedge_in_both_unit_inputs` | 1 passed，9片段×MemoryDocument单源/Rooted双源，共18分析、27 source inputs，12准入/15拒绝 |
| owner与共享hook组合 | §154的80 lib exact + 本片3项 | 83 passed，0 failed/ignored |
| 循环与相邻合同 | §154的17 iteration + 本片公开1项；原5 multifile、2 closures、1 borrow-continuation | 18 + 5 + 2 + 1 passed |
| 质量门禁 | frontend all-targets严格Clippy `-D warnings`、LLVM21 workspace all-targets check、fmt、docs、尺寸、diff | 通过；865 Markdown、2277 Rust、37历史超限，尺寸基底dc56394a，无新尺寸豁免 |
| 独立复审及复跑 | 真实四边界、body丢弃前检查、互斥API seal及测试oracle；新3 lib与公开1项exact | 无blocker；4项各1 passed，HEAD/tree及两binary SHA256前后不变，工作树clean |

合计109项不同ordinary测试，全部在本片36d381e3依赖组合重新执行；独立复跑和中途红绿、
补强不重复累计。新3 lib带完整前缀
`ownership_checking::compilation_unit::dataflow::drop_planner::owner_transport::tests::`。
正例从实际AST核While/condition/body/Break与完整三边，四份binding、input/body catalog隔离、
producer数1/1/2/3、Replace数0/0/1/2及每producer唯一真实Drop；完整Analysis开关等值。
实际suffix Drop只用于兼容性核对，不成为新能力的suffix运输或最终消费证据。合法否例
先核成功和观察数，公开source预期由index与原source_id独立建立。§152/154原Break拒绝
源码和旧getter断言均保留，新API另加完整正例；normal/Continue正例显式拒绝新Break能力。

独审建议的独立Boolean负控已补并纳入最终109项；完整自洽Break证据不能包装成旧回边、
完整自洽Continue证据不能包装成Break的类型封印守卫已编译。新缺失/foreign snapshot、
错jump/body Arc/catalog/edges、外层identity和deferred守卫及旧model/Abort/故障注入只编译，
未本机运行。Mac AArch64、Rust1.96.0、LLVM21.1.8，locked/offline、默认debug、jobs=2，
独占target串行。未跑frontend/workspace全量测试、Linux、native或精确头正式CI。

收尾复核 #397 已合入，新main `748516795812a1946b2fd9e3ca0f8046dc2e560b`，#390头未变。
main相对36d381e3的7文件是constant资格/SCC及对应测试文档，未触owner producer；本地
parser后续 `8bfc8d8d` 同样不含owner改动。本片保留已验证链，尚未整合或测试74851679，
上述109项与门禁不能代替最新main或统一队列整批验收。未删除缓存、旧oracle、分支或用户
工作；按用户要求只本地分步提交，不推送、不改PR、不合main。仍无一般phi、非空分支
合流、动态多实例capture/释放、外层运输闭合或Phase 4/native，D10/E2与本Spec保持
in-progress。事实见[While边界观察](../../architecture/unit-loop-owner-transport.md)。


## 158. 非空不变 owner 的双 Continue 逐臂边界（2026-11-09）

开工核对 main `748516795812a1946b2fd9e3ca0f8046dc2e560b`、全部远端分支、在途PR，
#390仍为734dde83，#391/#393已包含在其链；没有重复的非空条件边实现。本地统一队列
4dfeda9e与parser批次8bfc8d8d不含本片producer。新隔离分支从最新main建立，保全合入
§156 `532aef8132f5859d31a85aa415e3ef7faad26c38`；纯依赖提交
`1fe70f6dc6a20a2641fc730cdd7ea45748ce3a8b` 的有序父为74851679+532aef81，tree
`9fa63f624ca2ed93895347adea94955e1e055333`。独立逐mode/blob复核：共同基底36d381e3上
main改7路径、owner改88路径，交集仅names-and-types；其三个不重叠hunk机械合并，其余
内容逐项保全，index对基底94路径，无额外差异。此结论是字节保全，行为证据另见下表。

新增独立 `UnitOwnerInvariantSplitContinueObservationV1` 及 getter，只接受单个不变String
输入、原精确三语句prefix/While/suffix，并由两个不同Boolean参数控制While与末尾If。
If两臂各仅一个裸Continue，复用现有闭合本地String/move/Group/root Replace前缀；不接
Break混合臂、空臂、arm-local owner、外层输入操作或其它carrier。旧空入边、normal/
裸Continue、Break和普通V1 getter均保持原合同，所有旧oracle保留。

真实If condition后读取split；实际branch-entry cleanup后读各arm entry，原pending loan、
deeper-scope owner和iteration cleanup后读各jump，evaluator实际返回false后再读各arm
completion。每次复用完整singleton/OwnedValue核验，包括ValueState全部17字段，values
外16槽为空；保留实际control/序号/arm body/jump/body Arc，槽只写一次，第二臂不覆盖
第一臂。原tail/pending/active/scope/最近loop门均保留，双臂齐备才登记completed。

关键区别：原If仅将正常continuation的state放入merge，双Continue实际merge输入为空。
因此整体body只核真实Ok(false)、completed/body Arc和完整空状态，不伪造单个非空body
完成快照；不复制entry或某臂回填。公开两份 `UnitOwnerContinueArmBoundaryV1`，连同
While entry、split及LoopExit后的exit共九份独立binding；没有共同 `backedge()` 或
`body_completion()`。六条结构边精确关联两次实际Continue，无普通body回边或新造Drop。
原branch/merge/jump cleanup、Guide和ASAP均未修改，不实现一般phi或运行实例运输。

真实红提交 `e5d2f2ecac35541300e18fad99475198df9c0ac2` 在typed/ownership成功后因缺
新能力1 failed；同源码最小接线后1 passed，并将精确原红源码保留在最终测试。实现/测试
提交 `469d47b7d650bc624fc7e65e0bb3ed1c8e7b259b`，tree
`3175c048fcc206bab83bc90cd6257dbeac27d104`；后续文档不改变受测Rust。公开测试初次
模块路径遗漏导致编译失败，修正后重跑；不把它计为行为红。Clippy初报新私有enum变体
尺寸过大，仅间接存放该新变体，未加豁免或改变公开协议；最终全部测试及门禁重新通过。

| 验收项 | 实际选择 | 本地结果 |
|---|---|---|
| 双Continue实际producer | `invariant_split_continue::actual_split_continues_preserve_each_nonempty_arm_before_discard` | 1 passed，32种双条件Group/Unit/body组合+精确原红源码1例 |
| 控制与输入拒绝 | 同模块 `split_continues_reject_unproved_paths_and_input_shapes` | 1 passed，25合法新getter否例，包含原normal/Continue/Break能力继续可用的3例 |
| 来源及错误边界 | 同模块 `split_continue_sources_and_failures_keep_capabilities_separate` | 1 passed，双source/双analysis隔离；3输入错误/未闭合body及2非法arm无能力泄露，真实sibling错误清空观察 |
| 公开两入口 | `invariant_split_continue_owner_unit::split_continue_arms_match_actual_sources_in_both_unit_inputs` | 1 passed，12片段×MemoryDocument单源/Rooted双源，共24分析、36 source inputs，12准入/24拒绝 |
| owner及共享hook组合 | §156的83 lib exact + 本片3项；18 iteration + 本片公开1项；原5 multifile、2 closures、1 borrow-continuation | 86 + 19 + 5 + 2 + 1 passed，0 failed/ignored |
| 新main相邻合同 | main #397在multifile_constant_qualification新增的11个ordinary测试，各完整名exact | 11 passed，实际合法/非法常量源码，非手工产物 |
| 质量门禁 | frontend all-targets严格Clippy `-D warnings`、LLVM21 workspace all-targets check、fmt、docs、尺寸、diff | 通过；865 Markdown、2281 Rust、37历史超限，尺寸基底1fe70f6d，无新豁免 |
| 独立复审与复跑 | 依赖字节保全、逐臂真实状态、空merge与API seal、测试oracle；新3 lib与公开1项exact | 无blocker；4项各1 passed，执行前后HEAD/tree、两binary SHA256与clean状态一致 |

合计124项不同ordinary测试，即113项owner相关组合及11项新main常量邻控，均在本片
74851679依赖组合重建后执行；初验、红绿与独立复跑不重复累计。新3 lib完整前缀为
`ownership_checking::compilation_unit::dataflow::drop_planner::owner_transport::tests::`。
正例从index/AST独立核来源及六边、不同jump与全部binding；producer数1/1/2/3、Replace
数0/0/1/2独立给定，每producer核实际AST String位于While body内、唯一Drop及真实drop
fact。输入/body catalog隔离，完整Analysis开关等值；实际suffix Drop仅作兼容性见证，
不表示后缀运行过或新能力提供最终消费。原空输入条件组合及旧normal/Continue/Break非空公开正例均重跑。

两个非法arm源码因arm含额外语句在admission阶段被拒，不能称为实际走到“第一臂记录后
第二臂失败”的撤销测试；中途失败安全性来自生产控制流复审，不借源码失败夸大覆盖。
损坏次序、端点、各类foreign binding/Arc/catalog、deferred与自洽混合Continue/Break
模板的seal守卫均已编译，未在本机执行；同样未跑旧model/Abort/故障注入。Mac AArch64、
Rust1.96.0、LLVM21.1.8，locked/offline、默认debug、jobs=2、独占target。未跑frontend/
workspace全量测试、Linux、native或精确头正式CI，不能将124项称为统一队列整批验收。

收尾main与#390头未变；本地parser已推进a90019c0，其parser/对应文档/CI选择变化不触
本片owner，没有重复开发或吸纳该未验收批次。仍缺非空正常臂合流、混合目的地、可变
loop-carried owner、一般header/exit phi、动态多实例capture/释放与Phase 4/native。
未删除旧工作、缓存或oracle，按用户要求只本地分步提交，不推送、不改PR、不合main。
D10/E2与本Spec保持in-progress；事实见[While边界观察](../../architecture/unit-loop-owner-transport.md)。


## 160. 非空 owner 条件终端边的真实 Break 目的地（2026-11-09）

本片先核 main `748516795812a1946b2fd9e3ca0f8046dc2e560b`、在途#390 `734dde83`、
#398 `8952321a`及本地parser后续a90019c0，owner路径历史没有重复的混合终端能力。
新隔离分支从main建立，仅ff-only复用§158 `c475c2dba6104b309fed8e614abbe803edf1570f`，
不增加merge或重放已交付提交。#398和parser后续不含本片owner producer，未吸纳它们；
不改已完成工作树、远端或PR，§158的完整依赖链与旧oracle保留。

新增 `UnitOwnerInvariantSplitExitObservationV1` 与独立 getter，只接末尾If的三种终端
组合：Continue/Break、Break/Continue、Break/Break。仍为单个精确String val输入、While、
精确suffix三语句，两个不同Boolean参数的Name/Group条件，body前缀复用闭合本地String/
move/Group/root Replace。每臂只能一个裸跳转，拒绝空臂、臂内owner、嵌套、外层输入操作
及其它carrier。双Continue仍只取得§158能力；normal/单Continue/单Break接口范围不变。

私有收集器由split_continue更名为split_terminal，保留九次实际singleton快照与一次性槽。
实际jump cleanup后把真实Continue/Break kind、expression、binding一起保存；最终先据
该实际kind形成中性 `UnitOwnerJumpArmBoundaryV1` 的edge，不从expected补造执行。
新arm公开entry、after_jump、completion及edge：ContinueBackedge的目标为真实While
condition，BreakExit的目标为该While LoopExit。仅实际两条Continue边可转换旧公开
ContinueArm，再经过旧seal；旧公开类型文件字节未改，新旧两getter互斥。

新seal显式要求两臂皆终端且至少一Break，实际edge、closed completion和完整六边三方
对照，同时核control/condition/arm序号/body/jump/source、catalog及analysis/body Arc。
完整OwnedValue与ValueState17字段检查保持，values之外16槽为空。三种组合都没有正常
continuation，body仍真实Ok(false)与完整空merge，不制造共同backedge/body binding。
LoopExit仍清理原condition state，没有合入Break state；窄范围完整不变状态不等于一般
出口运输、header/exit phi或最终消费。原branch、conditional hook、merge与jump cleanup
字节不变，Guide、ASAP和普通V1/Choice均未放宽。

红提交 `99bf845f1c69b0b7f66c024d06e80e57cbcb022a` 使用合法C/B完整源码，typed/ownership
成功后因缺新能力1 failed；最小实现后同测试1 passed，精确原红源码仍在最终正例中。
实现与最终测试提交 `0b08d01f375a825fe57720c0add9329b83ee775e`，tree
`429013ceff3a3f77ac7d3b54a95b7d235209ca58`；下表对应该Rust内容，后续文档不改受测源码。

| 验收项 | 实际选择 | 本地结果 |
|---|---|---|
| 三种真实终端组合 | `invariant_split_exits::actual_split_exits_preserve_nonempty_owners_at_each_real_destination` | 1 passed，3×2×2×2×4＝96矩阵输入+精确原红1例 |
| 拒绝与旧能力保全 | 同模块 `split_exits_reject_unproved_paths_and_preserve_prior_capabilities` | 1 passed，25合法否例，另复核旧C/C能力；normal/单Continue/单Break保持 |
| 来源与错误边界 | 同模块 `split_exit_sources_and_failures_keep_capabilities_separate` | 1 passed，双source/二次analysis隔离；3输入错误/未闭合body、2非法arm不泄露能力，sibling错误清空观察 |
| 公开两入口 | `invariant_split_exits_owner_unit::split_exit_destinations_match_actual_sources_in_both_unit_inputs` | 1 passed，20片段×MemoryDocument单源/Rooted双源，共40分析、60 source inputs，36准入/24拒绝 |
| owner及共享hook组合 | §158的86 lib exact + 本片3项；19 iteration + 本片公开1项；原5 multifile、2 closures、1 borrow-continuation | 89 + 20 + 5 + 2 + 1 passed，0 failed/ignored |
| main常量邻控 | §158保留的multifile_constant_qualification新增11项，各完整名exact | 11 passed，不借用旧main CI |
| 质量门禁 | frontend all-targets严格Clippy `-D warnings`、LLVM21 workspace all-targets check、fmt、docs、尺寸、diff | 通过；865 Markdown、2284 Rust、37历史超限，尺寸基底c475c2db，无新豁免 |
| 独立复审与复跑 | 实际kind/expected/edge封印、旧C/C转换、九快照/空body及测试oracle；新3 lib和1公开exact | 无blocker；4项各1 passed，前后HEAD/tree、binary SHA256和clean状态相同 |

合计128项不同ordinary测试，即117项owner相关与11项main常量邻控，全部从本片74851679
依赖组合重建后执行；红绿、初验和独立复跑不重复累计。新3 lib带完整前缀
`ownership_checking::compilation_unit::dataflow::drop_planner::owner_transport::tests::`。
正例独立从index/AST核jump实际kind与六边、九份binding；B/B无ContinueBackedge，混合
组合恰一条，均无普通body回边。每producer核实际AST String及body span、Create、唯一
真实Drop，producer数1/1/2/3与Replace数0/0/1/2独立给定，输入/body catalog隔离。
单源各正例与合法否例比较完整cleanup Analysis开关等值；共享compare固定source 0，
双source测试另核能力与二次analysis身份隔离，不能称两个source都完成开关对照。

两个非法arm源码含额外语句，admission已提前拒绝，不是第一臂记录后的中途撤销证据。
实际C/C配mixed completion、实际C/B配自洽B/B completion、自洽完整C/C放入新类型、
Break错指另一实际While、Continue错指If condition以及foreign snapshot/Arc/catalog、
重排/重复arm、deferred守卫均已编译，未本机执行；旧model/Abort/故障注入也只编译。
Mac AArch64、Rust1.96.0、LLVM21.1.8，locked/offline、默认debug、jobs=2、独占target。
未跑frontend/workspace全量测试、Linux、native或精确头正式CI，128项不是统一队列整批验收。

收尾main、#390及#398头未变；本地旁线已出现0b5c2b25短路flow修复，仅改类型flow、测试
和对应文档，无owner producer重叠，未吸纳或验证该组合。仍缺非空正常臂合流、可变
loop-carried owner、一般phi、动态多实例capture/释放、外层运输闭合与Phase 4/native。
按用户要求仅本地分步提交，不推送、不改PR、不合main，未删除旧工作或oracle；D10/E2
与本Spec保持in-progress。事实见[While边界观察](../../architecture/unit-loop-owner-transport.md)。


## 162. 不变输入 owner 的真实双正常臂合流（2026-11-09）

本片核 main `748516795812a1946b2fd9e3ca0f8046dc2e560b`、在途#390 `734dde83`、
#398 `8952321a`、所有远端分支与本地owner链后，在新隔离分支从main建立并ff-only复用
§160 `b9b1a9f73786b4b0666424a79ab7d3998b813ddd`；无重复实现、无新merge或提交重放。
统一队列4dfeda9e保持独立，本地类型flow de94e7e3未吸纳；不能把本片结果算作其组合验收。

新增独立 `UnitOwnerInvariantNormalJoinObservationV1`、`invariant_normal_join_observation_v1()`
与 `UnitOwnerNormalArmBoundaryV1`。仅接三语句immutable String literal输入、While、
Name suffix，两个不同普通Boolean参数的Name/Group条件；body前缀复用闭合本地String/
move/Group/root Replace，唯一末尾If两臂都为正常空ControlBody。正常/跳转混合、臂内
owner、其它尾值/嵌套/外层输入操作仍拒绝，不放宽旧空入边、直线/裸Continue、裸Break、
双Continue或含Break终端能力，私有枚举与各公开getter互斥。

While entry、If split、两臂各entry/completion、If join、body backedge、LoopExit共九份
binding分别读取实际state，复用完整OwnedValue相等与ValueState17字段核验，其它16槽空。
空ControlBody实际evaluator返回true，None tail、无owner result/pending jump，随后原
branch-exit cleanup；空臂本身不执行drop_scope。两臂完成门要求精确control/序号/body、
同active Arc、两个continuations和空事件。新增join hook只放在原merge_resource_states
完成后，重新核scope、最近loop、completed及observer Arc再独立snapshot，不复制entry。

原merge只按symbol相交并可能调整origin，不用它或same_owners代替完整输入证明：两臂
completion与join各自完整核验。whole-body正常scope cleanup后独立读取backedge，并须
实际Ok(true)、相同completed/collected Arc及原闭合V1。公开arm仅含entry/completion与
IfFallthrough，七边包含两臂正常完成及唯一body Backedge；不冒充jump或臂内drop_scope。
新seal限定全Fallthrough，核九binding、全部七边和catalog/analysis/split/join/arm身份。
旧terminal的空merge要求、普通If/Choice、merge算法、cleanup顺序与Guide ASAP保持。

红提交 `a37b74d64fe20bca8d2be16ca802c16bb235db42`：合法完整源码typed/ownership成功，
因缺nonempty normal join观察而1 failed；最小实现后同例1 passed，原红源码保留于最终
矩阵。实现、最终测试与架构提交 `16604b87cde3ddffebafabf6ccf03b4e72e4d54f`，tree
`bfb7e80b9649978da0472c094cdb87b9f627d79b`；下表对应其Rust字节，后续仅追加本账本。

| 验收项 | 实际选择 | 本地结果 |
|---|---|---|
| 真实正常臂、join与body回边 | `invariant_normal_join::actual_normal_arms_preserve_owner_through_the_real_join` | 1 passed，2×2×2×4＝32矩阵输入+精确原红1例 |
| 拒绝与旧能力 | 同模块 `normal_join_rejects_unproved_paths_and_preserves_prior_capabilities` | 1 passed，29合法否例；另复核四种旧终端组合，空输入正常If仍取得旧loop能力 |
| 来源与错误边界 | 同模块 `normal_join_sources_and_failures_keep_capabilities_separate` | 1 passed，双source/二次analysis隔离；3输入错误/未闭合body、2非法arm无泄露，sibling错误清空观察 |
| 公开两入口 | `invariant_normal_join_owner_unit::normal_join_matches_actual_sources_in_both_unit_inputs` | 1 passed，15片段×两入口，共30分析、45 source inputs，12准入/33拒绝 |
| owner与共享hook组合 | §160的89 lib exact +本片3；20 iteration +本片公开1；原5 multifile、2 closures、1 borrow-continuation | 92 + 21 + 5 + 2 + 1 passed，0 failed/ignored |
| main常量邻控 | §160保留的multifile_constant_qualification 11项完整名exact | 11 passed，未借用旧main CI |
| 质量门禁 | frontend all-targets严格Clippy `-D warnings`、LLVM21 workspace all-targets check、fmt、docs、尺寸、diff | 通过；866 Markdown、2288 Rust、37历史超限，尺寸基底b9b1a9f7，无新豁免 |
| 独立复审与复跑 | 两臂/真实merge/body三处区分、九次snapshot、旧能力互斥及源码oracle；新3 lib与1公开exact | 无剩余blocker；4项各1 passed，前后HEAD/tree/clean及binary SHA256相同 |

合计132项不同ordinary，即121项owner相关与11项main常量邻控，全部在本片依赖组合
重建后执行；红绿、初验与独立复跑不重复累计。lib完整名前缀仍为
`ownership_checking::compilation_unit::dataflow::drop_planner::owner_transport::tests::`。
正例从index/AST独立核两实际空ControlBody、source、七边与九份binding，body producer
数1/1/2/3与Replace数0/0/1/2独立给定；逐producer核实际String/body span、Create及唯一
真实Drop，输入/body catalog互不接受foreign token，无虚构ControlTransfer Drop。
单源正反例比较完整Analysis开关等值；共享compare只构造source 0 Checker，双source
另核来源及分析身份，不能称两个source均已开关对照。非法arm含额外语句，准入时已拒绝，
不是首臂完成后发生的运行中撤销证明。

损坏封印的join/backedge binding、join/split/arm Arc、catalog、重排/重复arm、错误边、
完整自洽异类completion与deferred守卫已编译，未本机执行。独审发现其中曾用control作
jump导致旧closed seal先拒绝，已改同文件真实terminal Continue ID后静审确认对照成立；
不把编译或静审当作守卫运行通过。旧model/Abort/故障注入同样只编译。
Mac AArch64、Rust1.96.0、LLVM21.1.8，locked/offline、默认debug、jobs=2、独占target；
未跑frontend/workspace全量测试、Linux、native或精确头正式CI，132项不是统一队列验收。

为保持架构页主题边界，将旧页四个不变输入章节保全迁至
[不变输入循环边界](../../architecture/unit-invariant-loop-owner.md)，仅一处上下文“上述”
替换为闭合body链接；脚本逐字比对旧章节保全。旧页仍为闭合body入口并链接新页，目录同步，
两页分别89/146行；本片正常join事实写入新页，不压行绕过页面护栏。

收尾main/#390/#398头未变；另见本地SPEC-0398后续c356d70d，仅4个codegen文件，无本片
producer重叠，未吸纳或验证该组合。仍缺正常/跳转混合臂的非空状态、变更owner、一般
header/exit phi、动态多实例capture/释放、suffix最终消费与Phase 4/native；LoopExit
仍清理原condition state，不是实际body回灌或一般出口运输。本Spec与D10/E2仍in-progress。
按用户要求仅本地分步提交，不推送、不改PR、不合main，保留全部旧链、工作树和oracle。


## 164. 不变 owner 的正常幸存臂与独立跳转臂（2026-11-09）

先核 main `748516795812a1946b2fd9e3ca0f8046dc2e560b`、#390 `734dde83`、#398
`8952321a`及远端/本地在途链，无重复owner实现。新隔离分支从main建立，仅ff-only复用
§162 `b0f73fe82d2ff56bd5ddf38027ad67d01959fd9f`，不新建merge或重放依赖；统一队列
4dfeda9e与类型/codegen旁线保持独立，旧工作树、oracle和完整父链不改。

新增 `UnitOwnerInvariantMixedJoinObservationV1`、独立getter及
`UnitOwnerMixedArmBoundaryV1::{Normal, Jump}`。只接末尾If一个正常空臂和一个裸Continue/
Break的四种排列；§162的三语句immutable String输入/Name suffix、独立Boolean条件和
闭合body-local String/move/Group/root Replace合同不变，不放行臂内owner或其它控制。

私有normal_join扩成joining：arm completion保存实际evaluator的continues与binding，
真实jump cleanup另存kind/ID/binding；只有(true,None)造Normal、(false,Some jump)造Jump，
矛盾或缺失则拒绝，不由expected AST或缺少jump推定正常。两个实际Normal只转换原NN
类型并过旧seal，mixed才进新类型；旧public NN/JumpArm文件及branch.rs字节不变。
原完成门对mixed要求恰1 continuation，NN仍2、terminal仍0；终端空merge要求保持。

正常臂经branch-exit cleanup，跳转臂false后不正常branch-exit cleanup也不push。原merge
只收到正常幸存state，之后才独立读取join，不称两状态合流；whole-body正常scope cleanup
后再读backedge并核实际Ok(true)、completed/collected Arc与原闭合V1。共十份真实binding：
entry/split、正常臂entry/completion、跳转臂entry/after_jump/completion、join/backedge/exit。
每份完整OwnedValue相同，values之外16槽空；空正常臂自身不执行drop_scope。

新seal限定恰一Fallthrough，逐项核实际Normal/Jump变体、expected completion、jump目标、
七边、source/序号/control/body及全部catalog/Arc/binding。Continue组合有一ContinueBackedge
和一普通Backedge，Break组合也有一BreakExit和一普通Backedge，不能称整体终端body。
LoopExit仍清理原condition state，没有回灌Break state；控制图是静态检查证据，不是某次
执行同时走两臂或每轮换臂。普通cleanup/merge算法、Guide ASAP、Choice与旧能力不放宽。

合法N/C原红例在typed/ownership成功后因缺观察1 failed，红提交
`272c82e533fca5a5ba472384961266a22ad4189f`；最小接线后同例1 passed，精确原源码保留。
最终Rust/架构提交 `ebf42b04d2e2e6e90454a0b4db42922e8f26e676`，tree
`3e191a3ff01a5e9e427a14d91484ec3c2fade160`；以下验收对应该字节，后续仅追加账本。

| 验收项 | 实际选择 | 本地结果 |
|---|---|---|
| 四种真实mixed路径 | `invariant_mixed_join::actual_mixed_arms_preserve_the_surviving_join_and_jump` | 1 passed，4×2×2×2×4＝128矩阵输入+原红1例；AST独立核7边/10binding、producer及真实Drop |
| 拒绝与旧能力 | 同模块 `mixed_join_rejects_unproved_paths_and_preserves_prior_capabilities` | 1 passed，28合法否例；另复核四终端组合及NN，旧空输入NN能力保留 |
| 来源及错误 | 同模块 `mixed_join_sources_and_failures_keep_capabilities_separate` | 1 passed，双source/二次analysis隔离，3输入错误/未闭合body、2非法arm无泄露，sibling错误清空观察 |
| 双公开入口 | `invariant_mixed_join_owner_unit::mixed_join_destinations_match_both_public_unit_inputs` | 1 passed，27片段×两入口，54分析/81 source inputs，48准入/33拒绝 |
| owner与共享hook组合 | §162的92 lib exact+本片3、21 iteration+本片公开1、原5 multifile/2 closures/1 borrow-continuation | 95 + 22 + 5 + 2 + 1 passed，0 failed/ignored；包含旧空输入四mixed路径回归 |
| main常量邻控 | §162保留的multifile_constant_qualification 11项exact | 11 passed |
| 工程门禁 | frontend all-targets严格Clippy、LLVM21 workspace all-targets check、fmt/docs/尺寸/diff | 全过；866 Markdown、2291 Rust、37历史超限，尺寸base b0f73fe8，无新豁免 |
| 独立复审和精确复跑 | 实际completion、jump/幸存者分离、旧能力及新3 lib+1公开 | 无剩余blocker；4项各1 passed，前后HEAD/tree/clean、两个binary SHA256一致 |

合计136项不同ordinary，125项owner相关+11项常量邻控；红绿和复跑不重复计数。lib名称
完整前缀为 `ownership_checking::compilation_unit::dataflow::drop_planner::owner_transport::tests::`。
正例producer数1/1/2/3、Replace数0/0/1/2由源码独立给定，输入/body catalog隔离，逐个
String body span/Create/唯一真实Drop对照，无伪造ControlTransfer Drop。单源正反例比较
完整Analysis开关等值；双source helper只对source 0做该对照，另核全部source身份。
两个非法arm在准入前已拒绝，不是第一臂完成后的中途撤销；新空输入对照只测NN，四mixed
空输入由旧回归覆盖。自洽NN/双终端、expected Break对actual Continue、foreign状态/Arc/
catalog、错误边及deferred守卫仅编译。精准Break守卫从真实N/B出发，只改为另一个真实
While的LoopExit目标，保持closed一致再核mixed拒绝；不能把初版同时改kind的case当作
单独目的地证明。所有model/Abort/故障注入未本机执行。

Mac AArch64、Rust1.96.0、LLVM21.1.8，locked/offline、默认debug、jobs=2、独占target。
未跑frontend/workspace全量测试、Linux、native或精确头正式CI，136项不是统一队列验收。
收尾main/#390未变，#398推进至b6cb2bb7，含类型flow/inout与恢复门禁；本地c356d70d为
codegen后续，无owner producer重叠，均未吸纳或组合验证。本片完成单个不变String的末尾
If正常/Continue/Break九种双臂完成组合之最后四种，不代表变更owner、一般header/exit phi、
动态多实例capture/释放、suffix最终消费、完整外层运输或Phase 4/native已闭合。
本Spec与D10/E2仍in-progress；仅本地分步提交，不推送、不改PR、不合main。当前事实见
[不变输入循环边界](../../architecture/unit-invariant-loop-owner.md)。


## 166. 正常不变循环输入的真实 suffix 消费（2026-11-09）

去重核 main `748516795812a1946b2fd9e3ca0f8046dc2e560b`、#390 `734dde83`、#398
`b6cb2bb7`及183个远端分支和本地在途链，无重复owner实现。新隔离分支从main建立，仅
ff-only复用§164 `4aa8eb7645ac84061db845945a2320fa83b27683`；没有新merge或重放旧提交。
统一队列4dfeda9e、codegen c356d70d不混入。收尾新增本地类型/codegen链99e16a82含flow、
nullable/inout与SPEC-0392记录，未触owner producer，本片未吸纳或验证该组合。

新增 `UnitOwnerInvariantLoopObservationV1::post_loop_transport_v1()` optional能力，不替换
原边界getter或改变其is_consistent。只限正常直线While，复用精确三语句immutable String
prefix/Name suffix与body-local String/move/Group/root Replace合同；Continue、Break及
九种末尾If组合仍保留各自原边界能力，但不取得本片suffix消费，不能泛化到changing owner。

prefix实际Complete collector被保留，suffix只在精确StatementId、真实正常循环产物完整、
scope/loop/branch/Choice与旧While/Syntax Unsupported身份门通过时激活。实际singleton再按
完整OwnedValue及17槽检查，保存独立suffix entry，与旧exit相等；prefix取出一次即消费。
临时换回同Arc collector，原statement连同borrow ends执行一次，保存Result后无条件恢复
旧sink再传播错误。只有实际Ok(true)、scope等门保持且17槽全空，才保存optional证据。
原borrow_result method只重命名，正文逐字保全；不改变ASAP、borrow或loop清理次序。

原真实六raw事件Result/Binding/Move/Result/Binding/Drop，经既有V1投影成五项
Create/Bind/Move/Bind/Drop。新seal另核正常Fallthrough、单producer、同input catalog/Arc/
callable、held→retired同token、真实suffix expression/statement与精确AfterStatement Drop，
无Choice或额外suffix events。body catalog隔离，旧ordinary仍Unsupported且定义/事件空。
末态失败不影响旧基础边界合同，最终unsuccessful/deferred则仍清除整个invariant产物。

原红例typed/ownership及旧loop边界均成功，仅optional缺失而1 failed；红提交
`be6bf4306675b45a3308d07bfc4871dece40109f`。接线后同一源码与断言1 passed，原例保留。
最终源码/架构提交 `f913d6dfc0533ecd5bd7d5b920a7db547a90dc0b`，tree
`85cfcb3efb45768f882967474d82386d46527f51`；下列运行对应其Rust字节，后续仅补文档。

| 验收项 | 实际选择 | 本地结果 |
|---|---|---|
| 正常消费 | `invariant_suffix::actual_suffix_closes_the_input_owner_after_normal_while` | 1 passed，24唯一正例含原红；3条件拼写×2Unit拼写×4body，producer 1/1/2/3、Replace 0/0/1/2 |
| 拒绝及旧能力 | 同模块 `suffix_admission_preserves_prior_control_capabilities` | 1 passed，12合法否例、2裸跳转和9条件组合；原能力保留，新suffix不扩张 |
| 来源/失败 | 同模块 `suffix_sources_and_failed_analyses_do_not_leak_transport` | 1 passed，双source/二次analysis隔离，重复消费、body移动输入及sibling错误3例全撤销 |
| 双公开入口 | `invariant_suffix_owner_unit::actual_suffix_consumption_matches_both_public_unit_inputs` | 1 passed，17片段×2入口＝34分析/51 source inputs，12准入/39拒绝 |
| owner/共享hook回归 | §164的95 lib+本片3、22 iteration+公开1、5 multifile/2 closures/1 borrow-continuation | 98+23+5+2+1 passed |
| 常量邻控 | §164原multifile_constant_qualification 11 exact | 11 passed |
| statement wrapper邻控 | unit_direct_call_loan_ends的下列4 exact | 4 passed，保receiver前缀/borrow与clone结束 |
| 工程门禁 | frontend all-targets严格Clippy、LLVM21 workspace all-targets check、fmt/docs/尺寸/diff | 全过；Clippy19.77s/check38.56s，867 Markdown、2295 Rust、37历史超长，base4aa8eb76，无新豁免 |
| 独立复审 | 新3 lib+1公开exact，production/架构/guard静审 | 无blocker；各1 passed、0 failed/ignored，前后HEAD/tree/clean和两binary SHA256一致，borrow wrapper仅rename逐字核对通过 |

合计144不同ordinary＝133 owner相关+11常量；重复红绿/复跑不计新增。lib完整前缀为
`ownership_checking::compilation_unit::dataflow::drop_planner::owner_transport::tests::`。
4个邻控为 `receiver_cfg_prefix::local_receiver_prefix_ends_on_break_and_continue_before_its_owner`、
`borrowed_receiver_prefix::original_borrowed_receiver_return_source_now_observes_its_abandoned_reborrow`、
同模块 `original_borrowed_loop_sources_now_observe_their_abandoned_reborrow`、
`lifecycle_combination::completed_clones_survive_receiver_prefixes_without_visiting_unreachable_suffixes`。
七target先统一--no-run编译，再逐exact核每项1 passed/0 failed/0 ignored；没有依赖宽filter命中。

新单源正反矩阵比较完整Analysis开关等值；双source只核全部来源身份，不称全部源开关对照。
普通输入错误在准入/整体分析门撤销，不伪称测试了suffix激活后的异常注入。损坏封印只编译：
foreign graph/entry、缺失/重复/重排、错误消费点/表达式/退休symbol与deferred；精确点守卫
把Drop换为真实body AfterStatement，通用V1仍一致而新seal拒绝。Continue守卫保原graph/
catalog身份，仅换为同source helper的真实jump及自洽completion/edges/transfer，旧边界
is_consistent成立而新Fallthrough门拒绝。两项早期oracle问题已独审修正，未本机执行守卫。
严格Clippy初轮发现optional大对象及布尔条件问题，Box/等价条件修复后全部144与工程门禁
重跑通过，无allow豁免。其它model/Abort/故障注入同样未执行。

Mac AArch64、Rust1.96.0、LLVM21.1.8，locked/offline、默认debug、jobs=2、独占target。
未跑frontend/workspace全量测试、Linux、native或精确头正式CI，不是统一队列组合验收。
outer graph只含prefix/suffix，须与原三处loop边界、独立第四份suffix entry及闭合body模板
联合使用；条件是body不改变的Boolean参数，不能称为实际运行若干轮后退出。本片只闭合
正常条件false路径的输入转移和最终消费，不证明终止、动态实例、一般header/exit phi、
变化owner或Phase 4/native。当前事实见[后缀消费](../../architecture/unit-invariant-loop-suffix.md)。
本Spec及D10/E2仍in-progress；仅本地分步提交，不推送、不改PR、不合main，保护旧链和oracle。


## 168. 裸 Break 出口后的不变输入消费（2026-11-09）

先核 main `748516795812a1946b2fd9e3ca0f8046dc2e560b`、#390 `734dde83`、#398
`b6cb2bb7`、183远端分支与本地在途链，无重复owner实现。新隔离树从main建立，仅ff-only
复用§166 `baf60fcc39b9c4a466b88d2b85ea3249270b5ca8`，未新建merge或重放旧链。
统一队列4dfeda9e、codegen c356d70d、类型/codegen 99e16a82均保持独立，未吸纳或验收组合。

`UnitOwnerInvariantBreakObservationV1`新增独立optional `post_loop_transport_v1()`，只接
原三语句String输入、body-local闭合前缀与唯一末尾裸Break。原Break基础getter和normal
suffix能力保留；Continue及九种末尾If组合不取得此Break消费，不放行可变或变化owner。

共享 `PostLoopTransport::validate`只核原五事件、单producer、同catalog/Arc/callable、
完整旧边界及精确suffix身份，不单独授予控制能力。normal公开getter仍要求实际Fallthrough
和自身is_consistent；Break getter先过原Break is_consistent，包含真实transfer、body Arc、
post-jump状态和精确BreakExit目的地。内部enum只返回这两种实际且尚无suffix的完整证据，
激活还核transaction expected等于actual closed completion；收尾再次核对应类型。

复用§166一次prefix.take、实际statement连borrow ends求值一次、先restore再判断Result的
路径。Break独立读取第五份suffix-entry，完整singleton/OwnedValue与exit相同；随后原真实
Move/Result/Binding/ASAP Drop使全部17槽为空，再通过原V1投影与精确消费封印。五份分别
为entry、post-Break、body-completion、exit、suffix-entry；不是复制entry或把false当作正常
scope cleanup。LoopExit仍清理原condition state，未把Break state回灌到一般出口合流。

旧loops.rs执行器、公开invariant.rs/boundaries.rs及borrow_result.rs与基线逐字相同；
旧check_break函数仅增加test内pub(super)以复用原断言，函数主体逐字保全。旧损坏Break
构造只补optional=None，不删除或减弱旧oracle。最终失败/deferred沿原门撤销整个产物。

原合法红例通过typed/ownership和旧Break边界，仅新optional缺失而1 failed；红提交
`cd405ab7f708f300771a6a5a20cee892d5c5c08f`。接线后同一源码与断言1 passed，原例保留。
最终Rust/架构提交 `f21e6a6a0a40f5d766d62c2f230d1da20fee68b3`，tree
`4430cbb3e8059c6fff08f1a05bae9942fc9cc1af`；以下运行对应该Rust字节，后续仅补账本。

| 验收项 | 实际选择 | 本地结果 |
|---|---|---|
| 实际Break消费 | `invariant_break_suffix::actual_break_suffix_consumes_the_unchanged_input` | 1 passed，24唯一正例含原红，3条件×2Unit拼写×4body；复用原Break AST/三边/四状态oracle，再核第五份entry与精确五事件 |
| 控制隔离与拒绝 | 同模块 `break_suffix_keeps_normal_continue_and_conditional_contracts_separate` | 1 passed，15合法否例、normal/Continue及9条件组合；normal原suffix仍准入，Continue仍拒绝 |
| 来源/错误 | 同模块 `break_suffix_sources_and_failures_keep_the_original_owner` | 1 passed，双source/二次analysis隔离，重复消费/body移动输入/sibling错误3例清空观察 |
| 双公开入口 | `invariant_break_suffix_owner_unit::actual_break_suffix_matches_both_public_unit_inputs` | 1 passed，19片段×两入口＝38分析/57 source inputs，12准入/45拒绝；独立核实际Break与该While目标、输入身份和唯一退休Drop |
| owner及共享hook | §166的98 lib+本片3、23 iteration+公开1、5 multifile/2 closures/1 borrow-continuation/4 receiver-clone邻控 | 101+24+5+2+1+4 passed |
| 常量邻控 | §166原multifile_constant_qualification 11 exact | 11 passed |
| 工程门禁 | frontend all-targets严格Clippy、LLVM21 workspace all-targets check、fmt/docs/尺寸/diff | 全过；Clippy21.23s/check35.38s，867 Markdown、2297 Rust、37历史超长，尺寸base baf60fcc，无新豁免 |
| 独立复审和复跑 | production/测试/架构静审，新3 lib+1公开直接binary exact | 无blocker；各1 passed/0 failed/0 ignored，前后HEAD/tree/clean、两个binary SHA256一致 |

合计148不同ordinary＝137 owner相关+11常量，红绿/独立复跑不重复计数。lib完整前缀为
`ownership_checking::compilation_unit::dataflow::drop_planner::owner_transport::tests::`。
七target统一--no-run编译后逐exact核1 passed；四receiver/clone邻控精确名称沿§166。
单源正反矩阵比较完整Analysis开关等值；双source验证全部身份，不称全部source开关对照。
输入错误属于准入或整体分析撤销，不称suffix激活后的异常注入。公开normal/Continue否例
早期曾写成两次跳转，已改精确删除或替换末尾Break后实际重跑；未用错误源码替代旧能力对照。

损坏封印仅编译：foreign suffix/entry、缺失/重复/重排消费、错误退休symbol/Drop点、
缺transfer、外来transfer Arc/binding、deferred；缺transfer仅表示缺见证被拒绝，不是模拟
真实运行未执行jump。normal/Break相互套错类型保完整原证据及graph，再核各控制getter拒绝；
Continue以同source helper真实jump重建自洽边和transfer，先套旧Backedge验证完整一致，
保持原suffix/catalog身份再让Break拒绝。目的地守卫只将真实BreakExit改指同source另一
真实While，保jump/kind不变；suffix graph仍一致，原closed控制seal负责拒绝错目标。
Drop点守卫使用通用V1仍合法的body AfterStatement，再核新精确suffix seal拒绝。
初始guard误调用私有common seal已改用上述公开完整控制对照，没有扩大生产可见性。
所有model/Abort/故障注入仍仅编译，未本机执行。

Mac AArch64、Rust1.96.0、LLVM21.1.8，locked/offline、默认debug、jobs=2、独占target。
未跑frontend/workspace全量测试、Linux、native或精确头正式CI，不是统一队列组合验收。
裸Break语法中条件false不激活body，true沿裸Break在一次body激活后退出；这些是静态checker
路径，不是执行测量或有限runtime轨迹。图只含outer prefix/suffix，须与四份实际Break边界
及独立第五份entry联合使用，不证明变化owner、一般header/exit phi、动态多实例或Phase 4。
main/#390/#398及本地旁线收尾未变。本Spec与D10/E2仍in-progress；仅本地分步提交，
不推送、不改PR、不合main，保全旧链和oracle。当前事实见[后缀消费](../../architecture/unit-invariant-loop-suffix.md)。


### 声明根 codegen 消费者迁移（2026-11-09）

从 main `a4b72d350f9616115a9d5fbed13ca761828ba728` 的独立分支实施，仅迁移
`declarative_type_roots_do_not_enter_the_scalar_instance_graph` 中的首个正例。
原182字节源码保留四类未使用声明与唯一entry；本测试及相邻诊断测试的9份Koven输入
逐字核对相同。复用现有strict UnitSource，不改生产实现、共享harness或入口API。

原SSA确定性、唯一函数、LLVM唯一internal定义及四个声明名不出现的断言保留。
Unit采用真实entry的declaration Span取代Legacy显示名；额外锁单MemoryDocument、5个根、
单module、空运行时types、无初始化/清理指令和空Return，重复SSA与LLVM均逐字相等。
该文件真实Legacy `analyze` 调用20→19，`lower_scalar_file` 调用21→20，净退休1个消费者；
不把函数定义或`UnitSource::analyze`文本命中计作Legacy调用，也不累计统一批次已有迁移。

试迁前实际运行原两项exact并记录原错误。numeric when在Legacy是UnsupportedNode/
`input` 37..42，而Unit返回成功；Elvis两侧均拒绝，但Span由整函数0..41变为表达式31..41。
两次试迁的断言失败不计通过，最终完整恢复原诊断/unsupported测试，不修改期待换绿。
全局val拒绝、nominal多根的构造/分配/drop及LLVM malloc/free、无retain/clone，泛型跨根
复用与递归身份、其余旧清理见证均原样保留，不宣称上述边界已退休。

| 验收项 | codegen `--lib` exact目标或命令 | 本地实际结果 |
|---|---|---|
| 原完整测试与拒绝邻控 | `declarative_type_roots_do_not_enter_the_scalar_instance_graph`、`diagnostics_and_unsupported_bodies_fail_without_partial_programs` | 2项通过 |
| 原Unit及多根保全邻控 | `lowers_real_scalar_expression_functions_through_verified_ssa`、`lowers_reachable_scalar_generic_instances_once_and_keeps_recursive_identity`、`generic_overloads_with_the_same_type_arguments_keep_distinct_targets`、`non_null_assertion_does_not_register_unreachable_inline_types` | 4项通过 |
| 工程门禁 | codegen all-targets Clippy `-D warnings`、fmt、docs、尺寸与diff | 全通过；863 Markdown、2235 Rust，37项历史超长额度不增 |

测试均用完整`ssa::lower_frontend_tests::`前缀和`cargo test --locked -p lang-codegen --lib ... -- --exact`。
合计6项不同最终测试，基线与重复运行不累加。Mac AArch64/Rust1.96.0/LLVM21.1.8、默认debug、
独占本树target串行；进程枚举被系统拒绝，未据此声称全机无并发。原日志保留在Git忽略的
`.local-artifacts/`。未执行整个mixed套件、codegen/workspace全量、native进程、Linux、
故障/计数/sanitizer或精确新头CI。独立代码复审无阻断；仅交统一合批，
不新开PR或修改正在验收的远端头，SPEC-0341和D10/E2仍in-progress。


### 借用来源移动诊断消费者迁移（2026-11-09）

本地独立分支基于 main `9369cc7279c268385a19f41a10a54c7cc31ea2ce`（已含#395），
未叠加或修改#390的验收头、其他carrier/while候选。本片只改
`borrow_result_contract_tests::borrowed_result_source_move_stays_a_frontend_diagnostic_before_ssa`
这一测试函数，原PROVIDER与entry合成的281字节源码及`result.ko`名称逐字保留。

先实跑原Single断言并记录同源Unit观察：均为L0135 Error，完整message为
`move conflicts with an active loan`，primary255..261，首label238..244及
`conflicting loan starts here`相同。Unit另有140..158的`selected parameter declared here`
标签。本片不声称两份Diagnostic整值相等，也不保留旧FrontendDiagnostics错误包装的逐字段断言。
原测试只观察frontend在SSA前拒绝；该语义由strict Unit的Diagnostics拒绝和
parse/names/types各0、ownership恰1的完整阶段计数承接。固定完整Diagnostic的Eq同时覆盖
severity、code、message、primary、全部有序details；多余诊断、Label、Note或Help均失败。
三个范围的原片段、完整源码和source身份同时固定，不筛选错误码或接受任意拒绝。

本文件真实Legacy `single`调用3→2，净退休1个消费者路径；helper、其内部
`lower_scalar_file`调用点、其他文件消费者及全部生产代码未改，不能计作旧入口退休。
原Legacy wrapper的FrontendDiagnostics/None与source/analysis/diagnostics优先级仍由
`lowering_entry_contract_tests::legacy_only`的既有独立合同保留，本片未改这些测试。

| 验收项 | 完整测试路径或命令 | 本地实际结果 |
|---|---|---|
| 迁移及同族正常/拒绝邻控 | `ssa::borrow_result_contract_tests::`下`borrowed_result_source_move_stays_a_frontend_diagnostic_before_ssa`、`borrowed_result_single_ssa_uses_returned_loan_and_orders_end_before_move`、`borrowed_result_unit_ssa_uses_returned_loan_and_orders_end_before_move`、`borrowed_result_caller_cfg_and_block_returns_stay_precisely_unsupported` | 4项通过 |
| 仍用同一Legacy helper的独立见证 | `ssa::owned_null_cleanup_tests::single_nullable_factory_planning_boundary_precedes_any_comparison`、`ssa::verify_operation::map::require_tests::map_require_readonly_source_can_return_its_slot_through_ordinary_wrapper` | 2项通过 |
| 工程门禁 | codegen all-targets Clippy `-D warnings`、fmt | 通过 |
| 交付文档门禁 | docs、Rust尺寸、diff | 全通过；863 Markdown、2235 Rust，37项历史超长额度不增 |

以上6项均为`cargo test --locked -p lang-codegen --lib ... -- --exact`，每项恰1通过、零失败/忽略。
初次新断言宏缺说明参数导致编译失败，未产生行为结果；修复后全部6项从头实际通过。
基线及重复运行不累加。Mac AArch64、Rust1.96.0、LLVM21.1.8、默认debug、独占本树target串行。
独立代码复审无阻断；原日志在Git忽略的`.local-artifacts/`，不提交raw dump。
未执行故障注入/verifier改写测试、native进程、codegen/workspace全量、Linux或精确新头CI；
本片不新增运行时能力。仅分步本地提交供下一统一批次，不推送、不新开PR或合并，
SPEC-0341与D10/E2仍in-progress。


### 泛型资源包装拒绝消费者迁移（2026-11-09）

基于 main `9369cc7279c268385a19f41a10a54c7cc31ea2ce` 的独立本地分支，核对在途
#390 `734dde83`、统一候选 `3580baa0` 及已交付声明根和借用诊断候选，未重复实施。
只修改 `lower_frontend_resource_deinit_tests` 中
`single_resource_deinit_rejects_unsupported_resource_wrappers` 的第三条输入路径。
三条原源码均按原字节保留一次；第三条 `Holder<Resource>` 输入为113字节，SHA-256
`a0a50cbcd1b4375d72a74dfaf540bfdd4097bce078a8fa35b87df48ed4483668`。

迁移前在同一源码实际观察 Legacy 与 Unit：第三条均为完整 `LoweringError`
`UnsupportedNode`，Span 为原 source 的93..111，切片为 `Holder(Resource())`。
新断言使用 strict `UnitSource` admission，固定四阶段零诊断、无 deferred、三个 parsed
roots、source 名称与全文、真实 `run` 声明及其完整切片，再要求封闭入口不生成 Program，
并整值比较上述错误。没有调整源码、弱化 span 或回退旧路径；测试原本没有资源计数或
native oracle，其他资源测试及其 SSA/LLVM/清理断言完整不变，生产实现没有改动。

前两条输入的 Legacy 循环与断言逐字保留。直接泛型 `Resource<T>` 的 Legacy span
34..45 指向 `deinit() {}`，Unit span81..92 指向 `Resource(1)`，并非完整错误等价，
本片不迁移、不修正该差异。净退休仅1条 Legacy 输入路径（该测试原3条降为2条）；
本文件静态 `analyze_legacy` 仍5处、`lower_scalar_file` 仍4处，不声称删除调用点、
整个测试或 helper，也不声称支持泛型资源包装。

| 验收项 | 本片实际选择 | 结果 |
|---|---|---|
| 直接拒绝与邻控 | `single_resource_deinit_rejects_unsupported_resource_wrappers`、`single_resource_deinit_rejects_member_calls_and_nested_call_borrows`、`single_resource_deinit_rejects_partial_move_at_the_conditional_owner_origin` | 三个 exact 各1 passed |
| 资源正向邻控 | `single_resource_deinit_body_is_lowered_and_reads_live_fields`、`single_resource_deinit_handles_nested_class_fields_and_body_generic_call_roots`、`single_resource_deinit_supports_direct_sequence_elements_and_resource_value_fields` | 三个 exact 各1 passed |
| 静态质量 | codegen all-targets Clippy（locked，`-D warnings`）、workspace fmt、diff | 通过 |
| 文档与尺寸 | `check_docs.py`、尺寸门禁 `--base 9369cc72` | 863 Markdown、2235 Rust、37项既有超长；通过，额度不变 |

以上6项测试使用完整 `ssa::lower_frontend_tests::resource_deinit_tests::` 前缀及 `--exact`，
均零失败/忽略，每次另有2777项过滤；不将 baseline 观察重复计入。macOS AArch64、
Rust1.96.0、LLVM21.1.8、默认debug、jobs=2，本工作树独占 target 串行执行。
未运行 frontend/codegen/workspace 全量、native 进程、Linux、正式CI或故障/Abort注入套件；
局部测试与静态门禁不能替代统一候选最终组合验收。独立代码审阅未发现阻断项，
SPEC-0341 与 D10/E2 继续 in-progress；本片仅分步本地提交，未推送或修改PR。


### 普通 continue 拒绝输入的严格 Unit 差分合同（2026-11-09）

隔离基底为实际 main `94768a910f11fc2f8426c813b550618f7799895f`。
本片仅修改 `lower_frontend_owned_source_continue_tests.rs` 的两个原测试：
source move、索引写入、MutableList add/removeLast 共四份 L0135 输入，及借用元素移出
的一份 L0133 输入。五份原源码逐字保留，无 wrapper、package、mode 或 fixture 改写。
生产实现、共享 harness、工具链、CI selector 与其他测试合同均不变。

原两项 exact 先实际运行，记录 Legacy 与 strict Unit 的完整诊断。四份 L0135 在 Legacy
均为 `access conflicts with the active iteration source loan`，Unit 分别报告
`move conflicts with an active loan`、`mutation conflicts with an active loan` 与
`new loan conflicts with an active loan`；move 另带选中参数标签。L0133 的原文案不变，
Unit 同样增加选中参数标签。两路保留原错误码、primary 和原冲突标签，但不逐字等价。
因此保留全部 Legacy 见证，旧入口净退休0，不把新增观察算作迁移完成。

`6a32a8b4` 首先把原 Legacy 的弱码检查提升为完整 oracle；`33b0b3c7` 再追加同源 Unit
合同及架构事实，完整源码树为 `db194596fd9b044b95d21f96f98d234c20555619`。
期望独立固定 severity、code、完整 message、源码身份与原字节、primary、每个有序
label；Diagnostic 完整相等同时排除额外 note/help。Unit 必须在 parse/names/types
均零诊断后以唯一 ownership 用户诊断拒绝，意外成功、空集、内部错误及其他分流均失败。
本片不调用拒绝后的 lowering，也不把诊断拒绝当作 SSA/native 行为等价。

| 验收项 | 实际选择 | 结果 |
|---|---|---|
| 原 Legacy 与 Unit 差分及 continue 邻控 | `cargo test --locked --offline -p lang-codegen --lib ssa::lower_frontend_tests::owned_source_continue_tests::` | 19 passed，0 failed/ignored，2780 filtered |
| 严格静态检查 | codegen all-targets Clippy，`-W clippy::all -D warnings`，locked/offline | 通过 |
| 工程门禁 | workspace fmt、docs、相对 `94768a91` Rust尺寸、diff | 通过；863 Markdown、2239 Rust、37项历史超长，不改额度 |
| 独立只读复审 | 完整原源、Legacy保全、Unit资格、诊断顺序及架构边界 | 无 blocker；复审未自行运行测试 |

Mac AArch64、Rust1.96.0、LLVM21.1.8、debug=0、jobs=2，独占本片 target 串行运行。
19项为最终不同测试数；两项基线、强化后的复跑及最终重复运行不累加。完整原始观测及
命令日志留本工作树忽略的 `.local-artifacts/`，固定诊断期望已入上述提交，不依赖临时日志。
未运行完整 codegen/frontend/workspace 测试、native 进程、Linux、故障注入或 sanitizer。
本片仅本地提交待统一队列，不推送、开PR、合并或吸纳其他未发布候选；正式精确头CI待后续。
动态 owner、完整借用全序、全部 Single 消费者退休及 Phase 4 仍未闭合，D10/E2保持in-progress。


### 统一批次发布前组合验收（2026-11-10）

在原统一 `4dfeda9e` 上保留父链合入已完成 owner `7820cc9f`、消费者
`78755fa5` / `79d90070` / `cc3d27da` / `e2d9a02d`，以及实际 main
`6ce85b07f9e36fcd322544c93f33274d6ba15c1b`（含 #404）。`94c1960c` 已在原历史中，
未重复开发或重放；其他未提交工作树不纳入。本次合流没有新增生产实现，所有原分支保留。
唯一架构文案冲突保全双方新增事实，四处 SPEC 冲突均验证共同基底为双方完整前缀后，
逐字保留两侧追加正文各一次。独立复核确认各父链、单侧文件及关键自动合并无遗漏。

测试源码为 `32128e78c93b869c414dd6d42632ca834e94e229`，完整树
`c876c822b1cdde20a3fc1160307df4d1f55a9f5c`；本段为之后的纯文档追加。
Mac AArch64、Rust 1.96.0、LLVM 21.1.8，locked/offline、debug=0、jobs=2，
复用独占统一 target，Cargo 命令串行；先编译目标，再逐完整名称 `--exact` 实际执行。

| 验收项 | 最终组合实际范围与结果 |
|---|---|
| frontend lib | 原 owner 101项、adjacent_borrow_results 25项、angle_group_tests 1项，共127 passed |
| frontend integration | multifile_ownership_checking 5、ownership_iteration 24、ownership_closures 2、ownership_borrow_continuation 1、multifile_constant_qualification 11、unit_direct_call_loan_ends 245、while_flow_facts 16、ownership_borrow_result 12、ownership_nested_borrow_arguments 14、canonical_type_tables 1、control_branch_join 9、string_clone 19、parser_block_line_continuation 24、parser_index_recovery 22、parser_super_recovery 10、type_callable 的 inout/overload 6，共421 passed |
| codegen lib | 声明根/诊断消费者、continue 19项、资源闭包、callable identity、closure lowering、借用合同/调用、nullable temporary、short-circuit及相关 native，共119 passed，其中14项实际 native |
| 静态与结构 | frontend/codegen all-targets Clippy `-W clippy::all -D warnings`、workspace all-targets check、fmt、docs、相对6ce85b07尺寸、diff均通过；867 Markdown、2306 Rust、36项既有超长，未新增豁免 |
| 独立证据核验 | 17个frontend目标与1个codegen目标的选择、COMMAND及成功集合完全相等，无重复、遗漏或零命中；每项1 passed、0 failed/ignored |

合计667项不同测试通过，不累计旧头仅编译或选择器校正，也不重复累计旧切片证据。
完整选择、逐项输出、二进制SHA256和输入身份保留本机临时验收目录，不入Git。
未运行完整frontend/codegen/workspace测试、Linux或额外sanitizer；本地结果不代替正式CI。
发布继续使用既有 #390 的快进更新，不新建PR、不合并main；远端精确头及其双平台与Preview
CI须在发布后另核。SPEC-0341、D10/E2保持in-progress，不据此宣称一般动态生命周期或Phase 4闭合。


## 172. 闭合循环与条件臂的精确 Boolean 取反准入（2026-11-10）

本片基于已发布 #390 `ca322bc558be5336efdc701434cb432e5b7c509e`，其中已含
main `6ce85b07`。开始前核对190条远端分支与本地在途成果，未发现重复 owner 取反实现。
§170 条件合流后缀的未完成工作未纳入。本片结束时 main 已推进至 `144793e1`（#405），
其11个变更路径不与本片 owner 路径重叠；下面结果尚非该新 main 的组合验收。

生产仅扩展 `owner_transport/loops/conditional.rs` 与 `owner_transport.rs`：闭合 While
和终端 If 的条件现在可由同一 Boolean 参数经 Group、Prefix Not 交错包装组成。
各层仍须为精确非 constant Boolean，外层真实 condition outcomes 仍须为 `(true, true)`；
While 与 If 仍须追溯到不同的原参数身份，同一参数的相反极性不被误认成独立输入。
观察记录保留整个取反条件的原 ExpressionId、SourceUnitId 和真实控制边，不把内层 Name
当作条件，也不因奇数层 Not 对调 then/else。循环零次路径取决于完整条件结果。

If 内 Not 的普通观察只在已经准入的实际条件链上放行，并要求当前 Conditional 事务、
新鲜 body 身份、完整 value observation、精确 scope/boundary、尚未进入 split、没有
pending jump/completed/owner choice 等状态同时匹配。查询仅识别该条件链中的原 Not ID，
不写状态、不制造控制事件，也不一般性放行循环外 Prefix。没有改变 owner 状态、phi、
清理规则或 body 遍历。架构事实同步至闭合 While、循环 invariant 和后缀三页。

真实红提交 `73afe1cb` 的原 `while (!flag)` 见证先失败；最小准入实现后同项转绿。
扩展矩阵随后实际暴露 If 观察仍拒绝 Prefix（1 passed、2 failed），补入上述严格活动条件
门后3项全部转绿。测试编译中的 IfBranch 结构体模式与 tuple 更新错误已修正，未放宽
生产类型或协议。最终源码提交 `65b6e3edc38a937cda5bd21ebc591d712587a2b7`，树
`eb4b7946336216c4440648d958e1d5cb732e06ff`；本段为之后的纯文档追加。

| 验收项 | 实际证据 |
|---|---|
| 原条件来源 | `negated_parameter_retains_actual_loop_condition` 固定整个 Not 条件、真实 body producer 与完整分析比较 |
| 控制边矩阵 | `negated_while_and_if_preserve_all_real_completion_edges` 覆盖192组：empty/invariant ×4种条件对 ×12种 normal/continue/break 完成形式 ×2种 body；从真实 AST 固定完整有序控制边，保留原 normal/break 后缀支持和 continue 后缀拒绝 |
| 拒绝与身份 | `negated_conditions_keep_unproved_sources_and_identity_closed` 覆盖17份合法拒绝输入、2源相同 raw ID 的来源隔离、2类非法源码原子清空；包含循环外普通 Not 拒绝，固定 Unsupported(Syntax) 的实际表达式位置；单源合法拒绝完整比较分析结果 |
| 公开入口 | `negated_conditions_match_both_public_unit_inputs` 使用30份片段，经单源 MemoryDocument 和双源 Rooted 输入形成60次分析、90个 source inputs，其中72个准入、18个拒绝；核完整 typed 兼容、真实条件/跳转/分支顺序、原输入与 body catalog 分离及跨源 producer 拒绝 |
| 旧见证保全 | 两条原取反源码仍留原拒绝数组，仅对应完整原输入改为正向断言；其他拒绝、sibling、完整清理比较保留，不删除旧源码或缩窄邻控 |

最终不同普通测试共168项：frontend lib 104、ownership_iteration 25、
multifile_ownership_checking 5、ownership_closures 2、ownership_borrow_continuation 1、
multifile_constant_qualification 11、unit_direct_call_loan_ends 4、while_flow_facts 16。
先编译8个目标，再逐完整名称 `--exact` 执行，每项1 passed、0 failed/ignored；
红绿复跑与独立复验不重复计数。独立审阅建议补入循环外 Not 拒绝与澄清完整条件结果，
两项已落实；复审在同一 clean 源码头独立执行4项新增测试，全部通过。

Mac AArch64、Rust1.96.0、LLVM21.1.8、默认 debug、jobs=2，独占 target 串行执行。
frontend all-targets Clippy（locked/offline、`-W clippy::all -D warnings`）、workspace
all-targets check、fmt、docs及diff均通过；867 Markdown、2308手写Rust、36项既有超长，
相对 `ca322bc5` 尺寸门禁未增加豁免。故障、损坏模型、Abort 套件仅编译未执行；没有运行
完整 frontend/workspace 测试、Linux、native、精确头正式 CI 或新 main 组合。

常量、调用、二元运算、比较、局部 alias、模式/nullable 参数及同一参数的双条件继续拒绝。
本片不证明运行次数、终止性、改变 owner 的循环、一般 header/exit phi、动态实例或
Phase 4；SPEC-0341 与 D10/E2 保持 in-progress。仅本地分步提交，不推送、改PR或合并main。


### §172 与 main cc1e5b41 的本地组合复验

Mac 恢复后先完成上述纯文档提交 `01b5aca143ced5d52444d16b6d8314e1b2ec88c0`。
组合前再次核对，实际 main 已推进至 `cc1e5b41223fabe0248dd4bc2742e95aae9b810a`
（#406），#390 仍为 `ca322bc5`。在新的隔离分支保留原提交链形成本地组合
`ac2bf48a3e4f3163397ed1018f3931bd6fc97210`，树
`4f72887bf0b34c6f5ad7541a85b7e209b75dc115`，有序父为该 main 与 `01b5aca1`。
共同基底 `6ce85b07` 上主线16条、功能侧119条变更路径互不相交；独立审阅逐路径
核对 blob 与文件模式，双方完整保全，无额外修改、冲突或 §170 WIP 吸纳。原树保持 clean。

在该精确组合上重新编译12个 frontend 目标，逐完整名称 `--exact` 执行216项不同普通
测试，每项1 passed、0 failed/ignored。选择为上表原168项，加 `type_callable::inout_storage`
17项、完整 `type_map` 19项、`type_checking::control_join_regressions` 3项和完整
`control_branch_join` 9项；原168已含全部16项 `while_flow_facts`，不重复计数。
新增邻控覆盖主线已合入的 Inout/recovery、Map key、退出路径写回及嵌套循环跳转事实。
这次为新的组合结果，不把旧基线168项或此前4项复验累加成测试总数。

同一 Mac/Rust/LLVM、默认 debug、jobs=2、locked/offline、独占 target 串行执行。
严格 frontend all-targets Clippy（`-W clippy::all -D warnings`，29.94秒）、workspace
all-targets check（47.57秒）、fmt、docs867、相对 `cc1e5b41` 的 Rust尺寸门禁及diff
均通过；2308手写Rust、36项既有超长，未改尺寸豁免。中途连接短暂中断后读取原会话
和日志，未盲目重跑或把未返回阶段记为通过；最终串行命令以0退出。

完整测试选择、逐项日志、二进制哈希保留仓库外临时验收文件，不入Git。未运行完整
frontend/codegen/workspace测试、损坏/Abort套件、native、Linux或正式CI。此组合只为
本地验证和后续单PR整合保留，未推送、修改现有PR或合入main；D10/E2仍in-progress。


## 174. 裸 Continue 的条件 false 出口后缀消费（2026-11-10）

本片在实际 main `cc1e5b41` 上建立独立分支后，快进复用已验收的 §172 组合 `3fe17177`。
已核191条远端分支、#390 `ca322bc5` 与本地在途成果，无重复 bare Continue suffix 实现。
§170 未完成的末尾 If 合流后缀不纳入，原工作树与旧测试保全；未复用该候选的未提交内容。

生产仅修改 `owner_transport/loops/invariant/suffix.rs`。原 Backedge 产物现在允许
实际 Fallthrough 或裸 Continue 获取同一 optional 后缀能力；Continue 仍须通过原
独立 post-jump、body-completion 与闭合 ContinueBackedge 证据，不替换为普通完成。
entry、post-jump、body-completion、exit、suffix-entry 仍来自五次真实读取，沿用全部17槽
及完整 OwnedValue 比对；LoopExit 继续读取原 condition state，不加入出口 phi。

独立审阅发现单纯放宽旧 completion 门会让既有正常证据自洽重包为 Continue 的否例失守。
后缀因此额外封存实际收集时的 completion（含原 jump）与新鲜 body Arc，getter 分别
精确比较与 ptr_eq，不能从后来替换的 evidence 回填。它也约束 normal/Break 的现有
后缀封印，但不改变其已支持源码。原 prefix collector 只取一次、suffix statement 只
执行一次、先恢复旧 sink 再验结果、五项真实消费事件与 ASAP Drop/borrow cleanup 全部不变。

真实原源码的 `actual_continue_suffix_consumes_only_after_condition_false_exit` 先失败，
红提交为 `2d769ce914d3c641aaaf70b4a149ac7296388c6c`；加入最小实现后同项转绿。
最终实现/测试/架构提交 `ffb5fd23053b7ab3b5813fb54a5e059cc226e121`，树
`bebd6756bd7b2ff2df0aafe53a757b44d517dca8`；本段为之后的纯文档追加。

| 验收项 | 实际范围 |
|---|---|
| 真实来源与消费 | 3项新普通lib测试覆盖原红见证、40组唯一正向源码（5种Name/Group/Not条件 ×2种Unit声明 ×4种move/replace body）、12种合法语法拒绝、9种末尾If能力隔离、2类错误发布清空；单源正/合法负例完整比较观察开关前后分析产物 |
| 来源隔离 | 两源与同源二次分析均validate成功；输入token只能在原输入/后缀catalog解析，body与foreign catalog拒绝；Continue跳转保留实际source、完整条件和原三条控制边 |
| 公开入口 | `continue_suffix_consumption_matches_both_public_inputs` 经MemoryDocument与双源Rooted覆盖40正+3负，共86次分析、129个source inputs（120准入、9拒绝）；复用增强后的五事件/唯一退休Drop/原输入不Drop及来源helper，不复制旧测试源码矩阵 |
| 旧见证保全 | normal、Break及9种If原源码保留；仅原精确裸Continue旧否例转正；公开Break专属getter的Continue否例仍保留；旧§172条件见证更新为新optional支持 |
| 仅编译守卫 | 保留旧正常→Continue等损坏模型，新增Continue→正常/另一真实jump/真实Break跨getter重包、同源二次分析整套body/Arc/transfer替换及缺跳转见证；均编译成功，未在本机执行 |

最终逐完整名称 `--exact` 运行220项不同普通测试：lib107、ownership_iteration26、
multifile_ownership_checking5、ownership_closures2、ownership_borrow_continuation1、
multifile_constant_qualification11、unit_direct_call_loan_ends4、while_flow_facts16、
type_callable/inout_storage17、type_map19、type_checking/control_join_regressions3、
control_branch_join9。每项1 passed、0 failed/ignored；原216项无删减，恰增加3lib和1公开。
红绿和中间复跑不累加。独审逐项核12份日志/选择与二进制SHA256，在上述clean提交直接
复跑4项新增普通测试全部通过，前后HEAD/tree/状态和二进制哈希不变；不累计为224项。

Mac AArch64、Rust1.96.0、LLVM21.1.8、默认debug=2、jobs=2、locked/offline，独占target
串行执行。严格frontend all-targets Clippy（`-W clippy::all -D warnings`，40.24秒）、
workspace all-targets check（1分14秒）、fmt、docs867、diff通过。连接中断时尺寸日志为空且
进程已不存在，仅补跑该尺寸检查后通过：相对 `3fe17177`，2310手写Rust、36项既有超长，
未改豁免；没有盲目重跑未知测试。完整选择、日志和哈希留仓库外，不入Git。

不变Boolean参数在条件true时不证明会退出，返回图仅表示condition-false出口后的静态
消费，须连同原Continue边界使用。未运行损坏/Abort、全量frontend/codegen/workspace测试、
native、Linux或正式CI；不证明动态实例、一般changing owner、header/exit phi、条件合流
后缀或Phase4。SPEC-0341与D10/E2保持in-progress。仅本地分步提交，未推送、开PR或合并main。


## 176. 一次真实具名 move 形成不变循环输入（2026-11-10）

在 main `cc1e5b41223fabe0248dd4bc2742e95aae9b810a` 新建独立分支，再快进复用已验收
§174 `f7296f31`。开始及收尾均核 main、#390 `ca322bc5`、#407 与本地在途分支；未发现
相同 invariant 输入能力。未吸纳 §170 WIP 或其它线未提交成果，保全原工作树和旧见证。

本片只接四句：seed 无插值 String literal、held 从 seed 的一次精确 Name move、原直线
While、retired 从 held 的精确 Name move。三个绑定均为精确 String val；normal、裸
Continue、裸 Break 沿用原控制能力，九种末尾 If 的四句形式拒绝，原三句 If 能力不变。
不接受第二层 alias、Group/nullable/var 输入、调用或插值 producer、额外语句及外层输入
的循环内操作。没有更改语言语义、17槽边界、快照算法、body 遍历、jump/LoopExit cleanup。

capture 核实际五条 raw 事件：literal Result、seed Binding、seed Name Move、该 Name
Result、held Binding，且所有 token 来自唯一原 literal producer。预扫 AST 仅准入；
held 的完整 OwnedValue 仍从真实 binding 后 singleton 读取，不能由 seed 改名或重造。
后缀图严格七事件，新增 incoming Move/Bind；单独封存 seed symbol 与 Name expression，
阻止图被改写为直接五事件或另一个同源 alias。直接输入仍严格五事件，completion/body Arc、
input/body catalog 隔离、原 suffix 单次执行及错误分析整体清空均保留。

原 `actual_named_move_supplies_the_loop_input` 确实先失败，红提交
`a306842bbab58377c6637b46a67fecff64eeb77d`；同名、同源码加入实现后转绿。
实现/测试/架构提交 `6cd7e192155735d019240617229ad0ea4830e57b`，树
`fa0355718696aceb17a0ad80219cff804a96ca0e`；本节是之后的纯文档追加。

| 验收项 | 实际范围 |
|---|---|
| 真实来源与控制 | 3项新lib普通测试：原红见证、`named_input_preserves_actual_control_and_source_identities`、`named_input_keeps_exact_admission_and_failed_publication_boundaries`；120组正向源码为5条件 ×2种Unit声明 ×4类body ×normal/Continue/Break，核真实7事件、唯一literal producer及准确控制边 |
| 拒绝与隔离 | 19份合法否例、9种If四句拒绝/旧三句准入、3类失败全撤销；跨文件及同源码二次分析拒绝外来token；单源合法正负例完整比较观察开关前后的Analysis产物，seed/held无Drop、retired唯一AfterStatement Drop |
| 公开入口 | `loop_input_alias_unit::named_loop_input_matches_both_public_routes` 的120正＋18负经MemoryDocument与双源Rooted，共276次分析、414个source inputs（360准入、54拒绝）；核typed兼容、原AST完整来源、七事件和input/body/foreign catalog隔离，不把重复源码算成额外形状 |
| 仅编译守卫 | 新V1损坏模型覆盖擦除incoming move、改seed、改alias expression、缺Move、直接输入伪造alias；与旧completion/body Arc等守卫一同编译未执行。未独立注入raw Result缺失，不能声称raw破损全覆盖 |
| 独立复验 | 非作者审查生产/测试/架构及224项日志、选择和12份二进制哈希；在上述clean提交直接运行4项新增普通测试全部通过，前后HEAD/tree/状态与二进制哈希不变。该复跑不累加到测试总数 |

最终逐完整名称 `--exact` 执行224项不同普通测试：lib110、ownership_iteration27、
multifile_ownership_checking5、ownership_closures2、ownership_borrow_continuation1、
multifile_constant_qualification11、unit_direct_call_loan_ends4、while_flow_facts16、
type_callable/inout_storage17、type_map19、type_checking/control_join_regressions3、
control_branch_join9。原§174的220项全部保留，恰增3lib＋1公开；每项1 passed、
0 failed/ignored/measured。12目标先编译，再按选择逐项运行；日志/选择/二进制SHA256
留仓库外，不入Git，不把中间复跑或独立复验重复计数。

Mac AArch64、Rust1.96.0、LLVM21.1.8、默认debug=2、jobs=2、locked/offline，独占target
串行执行。严格frontend all-targets Clippy（`-W clippy::all -D warnings`，26.30秒）、
workspace all-targets check（40.48秒）、fmt、docs867、diff通过；相对 `f7296f31` 的
尺寸门禁通过，2312手写Rust、36项既有超长，未增加豁免。

未运行损坏/Abort套件、完整frontend/codegen/workspace测试、native、Linux或正式CI。
这仍是静态不变owner的来源与退出消费证据，不证明运行次数、终止性、动态实例、一般
changing owner、header/exit phi、条件合流后缀或Phase4。SPEC-0341与D10/E2保持
in-progress。仅本地分步提交，未推送、改现有PR、新开PR或合入main。


## Exclusive 调用前缀的循环退出与 raw 撤销（2026-11-10）

基于 main `cc1e5b41223fabe0248dd4bc2742e95aae9b810a` 的独立 Phase 3 切片。
去重时核对 #390 `ca322bc5`、#391 `a94a83d7` 和22个 borrow/receiver 本地分支，均只
观察 Exclusive Return；parent-binding 的在途实现和受阻文档保持原样，不重做 receiver
或 String.clone。此片不依赖这些候选，不推送或新建 PR，不将其验收合并计算。

`exclusive_call_prefix_ends()` 新增 Break/Continue，只投影既有深度感知 unwind 真正
发布的 `EndCallLoan`；原 root/call/result/provider 资格、token注册、退休、逆序清理
均不改变。普通循环与 for 退出保留 source-qualified 身份及同点动作序号；内层退出不
结束外层调用前缀，正常完成/未访问后缀不伪造 unwind。这里不是完整生命周期或动态计数。

- 先行红测固定原 `replace(&a, break)` 源码，实际空表与唯一结束事件期望不符；最小
  point筛选扩展后转绿。原 break/continue 见证源码、loan/owner drop/replacement
  断言保全，将旧片空表边界更新为精确唯一事件，不删除旧 Single 见证。
- 独立真实 Return 前缀后追加第二来源 L0134 的 raw 红测暴露原 Exclusive 表残留。
  在既有 finalize 失败分支补一个 clear；公开门原本有效，不是公开能力泄漏。实际
  Return/Break/Continue 正控先证明 raw/public 非空且相等，再核错误及deferred撤销。
- 新双入口固定oracle覆盖混合Shared/Exclusive序号、嵌套逆序、内层loop/while跳转、
  部分退出与正常后继、未访问/投影/借用参数排除。for正控直接查真实iteration动作，
  同token恰一次且 EndCallLoan 先于原owner drop，再于EndElement；不由描述符推测。
- owner开关测试仅移除 owner observations 后比较完整 Analysis，保留新表、身份及顺序。

| 验收项 | 选择 | 本地结果 |
|---|---|---|
| 完整普通事件族 | `unit_direct_call_loan_ends`，排除下述6个整项 | 118 passed，含7个新双入口合同 |
| raw/public原子撤销 | `--lib adjacent_borrow_results` | 8 passed，含1个新三出口合同 |
| owner开关完整产物 | `--lib owner_observation_preserves_exclusive_` | 2 passed，含1个新loop合同 |
| 借用与receiver邻控 | continuation、nested、receiver_backedge；nested排除既有1个Abort整项 | 14＋13＋4 passed |
| Unit循环退出 | `ownership_iteration control_exits_unit` | 6 passed |
| 质量与消费者 | frontend all-targets Clippy（-D warnings）、workspace all-targets check、fmt | 通过 |
| 文档/尺寸/差异 | check_docs.py、相对cc1e5b41尺寸门、diff | 通过；不增加尺寸额度 |

Mac AArch64、Rust1.96、LLVM21，locked/offline、默认debug、jobs=2、独立专属target
串行执行。165个唯一普通测试，包含9个新增测试，不重复累计单跑。事件族6项仅编译：
`nonreturning_suffix_and_unreachable_calls_publish_no_cleanup`、`constructed_borrow_aborts_without_unwinding_and_ends_on_the_normal_path`、
`abort_does_not_publish_receiver_prefix_ends`、`copied_nullable_field_keeps_the_temporary_parent_on_both_call_edges`、
`abort_does_not_unwind_the_temporary_field_parent`、`abort_and_normal_return_do_not_invent_prefix_unwind_events`；
nested的 `inout_receiver_and_nonreturning_sources_do_not_publish_call_uses` 亦仅编译。
未运行native、Linux、故障注入、Abort、压力、frontend/workspace全量测试或正式CI。
独立只读复审无阻断，明确核 dataflow 1157→1158 仅一行撤销接线、保留1248上限；
compilation_unit.rs仍1030行，新领域测试309行。原错误日志保留于仓外；本片不归档D10/E2，
最终组合及精确发布头CI另验。


## 借用观察三表的 raw 原子撤销（2026-11-10）

本 Phase 3 切片基于 main `cc1e5b41223fabe0248dd4bc2742e95aae9b810a`，与独立
Exclusive 循环退出提交 `c718d72ec14199953501470bd70d928bffe0cf3f` 分开验收。核对
#390 `ca322bc5`、#391 `a94a83d7` 及25个既有相关本地分支，均未撤销下述三张 raw 表。
receiver/parent-binding 在途实现、受阻文档、既有公开 API 与原验收正文保持原样。

原拟扩展 borrow initializer 的循环前缀观察；真实 `select(owner, break/continue)`
输入先被既有 L0164 拒绝，未进入事件观察。保留该循环实例边界及旧 Return-only 筛选，
不通过放宽准入伪造新能力。双 Unit 入口新增固定 L0164/borrow marker/空结果合同。

独立真实红测分别证明：成功来源先形成 Shared direct ends、借用 handoff（含一个辅助
实参结束）、借用 Return-prefix ends，再追加另一来源的 L0134 后，dataflow::analyze
三张 raw 表各自残留。公开构造门原已撤销这些表，并非公开能力泄漏。
生产只在既有 finalize 失败分支各加一次 clear；成功路径、token退休、owner清理、
loop/receiver/source资格均不变。真实 diagnostic 与 deferred 后的 raw/public 同时为空。

| 验收项 | 选择 | 本地结果 |
|---|---|---|
| raw真实原子撤销 | `--lib adjacent_borrow_results` | 10 passed，含3个独立新正控/拒绝合同 |
| 两入口与循环拒绝 | `unit_direct_call_loan_ends borrow_publication` | 2 passed；三表同时非空后整体撤销；计入下行 |
| 普通事件族 | `unit_direct_call_loan_ends`，排除下述6个整项 | 113 passed，含上述2个新增 |
| owner开关 | `--lib owner_observation_preserves_all_three_borrow_publication_tables` | 1 passed；只移除观察字段后比较完整drop_planner::Analysis |
| 邻近借用合同 | continuation、nested、receiver_backedge；nested排除下述1个整项 | 14＋13＋4 passed |
| 质量与消费者 | frontend all-targets Clippy（-D warnings）、workspace all-targets check | 通过 |
| 静态提交门 | fmt、check_docs.py、相对cc1e5b41尺寸门、diff | 通过；原尺寸额度不变 |

Mac AArch64/Rust1.96/LLVM21、locked/offline、默认debug、jobs=2、本人target串行复用。
155个唯一普通测试，新增6项；单跑与事件族重跑不重复计数。完整保留但仅编译的6项为
`nonreturning_suffix_and_unreachable_calls_publish_no_cleanup`、`constructed_borrow_aborts_without_unwinding_and_ends_on_the_normal_path`、
`abort_does_not_publish_receiver_prefix_ends`、`copied_nullable_field_keeps_the_temporary_parent_on_both_call_edges`、
`abort_does_not_unwind_the_temporary_field_parent`、`abort_and_normal_return_do_not_invent_prefix_unwind_events`；
nested的 `inout_receiver_and_nonreturning_sources_do_not_publish_call_uses` 亦仅编译。
未运行Abort、native、Linux、故障注入、压力、frontend/workspace全量测试或正式CI。

独立只读代码复审无阻断；dataflow 1157→1160仅三行失败撤销接线，明确复审并沿用
1248例外上限，baseline/owner/split_plan不变；新领域测试96行。红绿日志保留仓外，
不复制运行产物入库。未推送或新建PR；D10/E2仍in-progress，组合验收及发布头CI另核。


## 已完成插值 String 的直接借用结束观察（2026-11-10）

独立 Phase 3 切片基于 main cc1e5b41223fabe0248dd4bc2742e95aae9b810a。核对26个
相关在途分支仍为plain-String资格；与 c718d72e Exclusive循环退出、67f3b97d raw
三表撤销分别验收，不混入receiver/parent-binding工作或其受阻文档，不重复已有清理修复。

真实先行红测中，已完成插值实参有Shared loan及严格的Temporary(argument) target，
但direct结束表为空。现限定原无receiver、owned-result declaration/symbol调用，
String字面量需为原始argument的MoveOnly temporary且target完全匹配；分组保原始身份。
普通factory与binary String结果虽有实际temporary loan/drop，仍不取得该观察资格。

只有完整插值成功才登记外层实参token；正常返回复用既有finish_direct_call_loans
真实移走该call token并记录CallReturn，退出仍来自原depth-aware unwind，不从end_span
推断执行。不修改注册时机、String输入清理、owner/receiver/source准入或循环证明。
部分/未完成插值只能清理已形成的内层输入；外层String不是它的父loan或共享payload。
本片不提供完整事件流、全局absence、动态实例计数或native能力。

| 验收项 | 选择 | 本地结果 |
|---|---|---|
| 普通事件族 | unit_direct_call_loan_ends，排除下述6个整项 | 120 passed，含9个新双入口合同 |
| owner开关 | --lib owner_observation_preserves_interpolated_string | 1 passed；仅移除观察字段后比较完整drop_planner::Analysis |
| 借用邻控 | continuation、nested、receiver_backedge；nested排除下述1项 | 14＋13＋4 passed |
| Unit迭代退出 | ownership_iteration control_exits_unit | 6 passed |
| 原插值输入清理 | ownership_constants interpolated_string_constants_release_inner_owners_before_returning_outer_string | 1 passed，保留旧Single见证，不代表普通Unit常量入口准入 |
| 质量与消费者 | frontend all-targets Clippy（-D warnings）、workspace all-targets check | 通过 |
| 静态提交门 | fmt、check_docs.py、相对cc1e5b41尺寸门、diff | 通过；未改尺寸额度 |

新合同固定完整source-qualified事件向量，核正常/Group、Return/for Break/Continue、
真实token→outer drop→EndElement顺序、部分与未访问、内层输入清理、嵌套已完成call不
重放、named参数逆序及Exclusive序号空隙、内层loop不结束外层token。错误/deferred正控
先证明公开表非空再核撤销；raw撤销候选67f3b97d未混入，组合仍须另验。

Mac AArch64/Rust1.96/LLVM21，locked/offline、默认debug、jobs=2、本人target串行复用。
159个唯一普通测试，新增10项；先行8项单跑不重复累计。完整保留但仅编译的6项为
nonreturning_suffix_and_unreachable_calls_publish_no_cleanup、constructed_borrow_aborts_without_unwinding_and_ends_on_the_normal_path、
abort_does_not_publish_receiver_prefix_ends、copied_nullable_field_keeps_the_temporary_parent_on_both_call_edges、
abort_does_not_unwind_the_temporary_field_parent、abort_and_normal_return_do_not_invent_prefix_unwind_events；
nested的inout_receiver_and_nonreturning_sources_do_not_publish_call_uses亦仅编译。
未运行Abort、native、Linux、故障注入、压力、frontend/workspace全量测试或正式CI。
原错误日志保留仓外；初版const夹具不适合普通Unit helper，最终以内层String literal测
相同输入清理边界，不修改helper或旧合同。新领域测试417行，compilation_unit.rs仍1030行，
未改尺寸政策或扩大额度。独立代码复审无阻断；D10/E2不归档，不推送或新建PR。


### 透明 Group 借用初始化器的真实交接（2026-11-10）

从 main `cc1e5b41223fabe0248dd4bc2742e95aae9b810a` 独立推进 Phase 3 观察合同，
不叠加另三片 Exclusive loop-prefix `c718d72e`、失败原子撤销 `67f3b97d` 或
插值 String 临时借用 `fd846873`。实施前核对 #390/#391 在途头及24个保留旧 Group
关闭见证的相关本地分支，未发现本片实现；不改写、合入或清理其他分支和未提交工作。

既有透明 Group initializer 已产生真实 borrow binding、source loan 和 result endpoint，
但正常交接索引仍用外层 Group，实际 pending token 则属于内层 Call；Return 放弃候选
同样误以外层 Group 查 typed call。本片复用现有 Group-only 原调用查询，仅归一化这两个
私有 lookup，随后仍须通过原 typed/result/source/receiver 资格门。公开 binding 的原
initializer 身份不变，source-qualified key 重新绑定当前 checker 的 source；不把不同
source 的局部 ExprId 当成同一调用，也不把 alias/nested consumer 当作直接初始化器。

正常路径从真实 pending 队列移出原 source token，经 binding state 保留至真实终止；
辅助来源按原求值顺序结束。未完成初始化器仅在 source 已求值且后参实际 Return 时观察
既有 prefix 结束，source 尚未建立时不制造 token。双层 Group、具名反排、未使用 binding
的原声明 endpoint、后续 Return、跨文件来源及错误/deferred 撤销均保持各自实际身份。
owner 观察开关前后的完整分析产物，扣除 owner 观察表后相同；没有新增 cleanup 动作。

本节仅取代历史“真实 source token 的 local binding 运输切片”中 Group initializer
关闭的边界，保留该节原结论和运行证据。两项原 Group fixture 原文继续作为正向见证，
其原 incomplete/nested 反控仍保留；parent 来源、receiver、provider、投影、temporary
等资格与循环中的既有拒绝不放宽。不是完整借用流、CFG/NLL、新语言能力或 native 证明。

| 验收项 | 本片实际选择 | 结果 |
|---|---|---|
| 新真实源码合同 | `grouped_initializer.rs` 的7项 Rooted/Memory 双入口与1项跨文件 Rooted 正反输入顺序；固定身份 oracle、全表 singleton 与实际 drop 关联 | 全部通过，包含在下行119项内 |
| direct-call 回归 | `unit_direct_call_loan_ends`，下列6项过滤 | 119 passed |
| 私有共享合同 | `adjacent_borrow_results` 7项、Group-only lookup 1项、新 owner 等价1项 | 9 passed |
| 旧借用邻控 | `ownership_borrow_continuation`、`ownership_nested_borrow_arguments`、`ownership_receiver_backedge` | 14 + 13 + 4 passed；nested过滤1项 |
| 编译与静态质量 | frontend all-targets Clippy（`-D warnings`）、workspace all-targets check | 通过 |
| 提交门禁 | workspace fmt check、docs、diff、相对 `cc1e5b41` 尺寸检查 | 通过：863 Markdown、2243 Rust、36项既有超长，额度不变 |

Mac arm64/Rust1.96、LLVM21、默认 debug、jobs=2、locked/offline、自有复用 target 串行，
共159项不同普通测试通过，含9项新增；不重复累计单独 grouped 过滤器的12项。
先以真实源码确认旧 binding/source/end 已存在但缺 handoff 的失败；实现后曾因测试用
子串 `(view(owner))` 同时匹配 nested consumer 而失败，改为精确 declaration 子串后通过，
不把该测试选择器失误记为生产缺陷。原始红/绿日志保留仓库外，源码见证保留测试目录。

本地 direct-call 明确过滤 `nonreturning_suffix_and_unreachable_calls_publish_no_cleanup`、
`constructed_borrow_aborts_without_unwinding_and_ends_on_the_normal_path`、
`abort_does_not_publish_receiver_prefix_ends`、
`copied_nullable_field_keeps_the_temporary_parent_on_both_call_edges`、
`abort_does_not_unwind_the_temporary_field_parent`、
`abort_and_normal_return_do_not_invent_prefix_unwind_events`；
nested 过滤 `inout_receiver_and_nonreturning_sources_do_not_publish_call_uses`。
这些原测试及正式 CI 选择均未改动，不计为本片已通过。未运行完整 frontend/workspace
测试、native、Linux、压力、Abort 或故障注入；workspace check 仅证明下游编译。

独立只读代码复核未发现阻塞项，核实每 source 独立 Checker、原 token 搬运/退休、
公开 initializer 身份和旧资格门保全。新文件458行；历史 `compilation_unit.rs` 1030行
未增长，其余变更 Rust 文件均低于1000行，不提高尺寸额度。仅准备本地提交，不推送、
新建 PR 或合并；本片不能替代精确组合头正式 CI，SPEC-0341 与 D10/E2 仍 in-progress。


### 实际临时辅助 owner 的借用返回结束（2026-11-10）

从 main `cc1e5b41223fabe0248dd4bc2742e95aae9b810a` 独立补 Phase 3 正常交接观察。
已核 #390 `ca322bc5`、#391 `a94a83d7` 与91条相关本地分支的 handoff/pending 实现；
原 temporary 辅助关闭门仍在，未重复实施。另四片 `c718d72e`、`67f3b97d`、
`fd846873`、`812ba9b4` 保持独立，不叠加其 loop、raw 撤销、direct String 或 Group
initializer 能力；其他分支、旧 Single 见证及未提交工作不变。

真实红测先证明 source transport、辅助 Shared temporary loan 和 CallReturn drop 均已
存在，但根限定的 handoff 门令正常交接观察缺失。本片仅新增辅助资格查询：loan 的
call/argument 必须属于当前 source，target 必须精确等于 Temporary(argument)，并匹配
实际 pending owner 的同 call/argument 与非 transfers_at_call 状态。此 owner 只会在
operand 正常求值完成后登记；既不按最终类型/AST 猜测执行，也不从 descriptor 重建 token。

正常返回仍从原 pending 队列按既有顺序移出原辅助 token；该调用随后原本就会 retain
移除它们。临时 owner 不移出、不重建，其现行逆序 drop 保留。来源继续严格使用既有
Shared owned 名称根资格；source 的 call/binding/forward transport helper 未改。
独立查询不会把 temporary、借用绑定、provider 或父字段提升为返回来源。parent、receiver、
Exclusive、Group initializer、循环准入及失败 raw 表均未扩展。

实际已登记的完成 String、Group、插值、拼接、factory 返回值、构造值及条件继续结果
均以同一 pending owner 证明准入。具名混合两临时值与 owned 根保持原 pending 顺序，
临时值则逆序清理；Return-before-source 不制造 source token，未完成 initializer 不
发布正常 handoff。原 mixed Return/Name 源码逐字保留并追加固定身份检验：退出前缀与
正常交接分别成立，正常 source 仍到真实 result endpoint 才结束，不能合为同一路径事件。

本节仅扩展历史辅助观察中 temporary 的关闭边界，保留历史结论与运行证据；它不提供
全部辅助来源、Return 临时辅助公开结束表、跨切片全序、动态实例、完整 NLL 或 native
能力。临时父字段、own Value、temporary source 与借用 parent 的原排除见证继续存在。

| 验收项 | 本片实际选择 | 结果 |
|---|---|---|
| 新公开合同 | `borrow_temporary_auxiliary.rs`，8项 Rooted/Memory 双入口及1项跨文件 Rooted 正反序 | 9项通过，计入下行，不另累加 |
| direct-call 回归 | `unit_direct_call_loan_ends`，下列6项整项过滤 | 120 passed；不重复累计原119项及追加1项 |
| 私有邻控 | 新 owner 观察开关完整 Analysis 等价1项、`adjacent_borrow_results` 7项、Group-only lookup 1项 | 9 passed；开关等价不是与旧版本前后差分 |
| 旧借用邻控 | `ownership_borrow_continuation`、`ownership_nested_borrow_arguments`、`ownership_receiver_backedge` | 14 + 13 + 4 passed，含 temporary source 拒绝；nested过滤1项 |
| 编译与静态质量 | frontend all-targets Clippy（`-W clippy::all -D warnings`）、workspace all-targets check | 最终复核通过，含追加原 mixed 见证 |
| 提交门禁 | workspace fmt check、docs、diff、相对 `cc1e5b41` 尺寸检查 | 通过：863 Markdown、2243 Rust、36项既有超长，额度不变 |

Mac arm64/Rust1.96、LLVM21、默认 debug、jobs=2、locked/offline、自有复用 target 串行。
共160项不同普通测试通过，含10项新增；不累计过滤器、单项和回归重复执行。原始行为红测、新 oracle
首次缺类型注解的编译错误及后续绿测日志保留仓库外，真实源码/固定 byte 身份留测试目录。

direct-call 明确过滤 `nonreturning_suffix_and_unreachable_calls_publish_no_cleanup`、
`constructed_borrow_aborts_without_unwinding_and_ends_on_the_normal_path`、
`abort_does_not_publish_receiver_prefix_ends`、
`copied_nullable_field_keeps_the_temporary_parent_on_both_call_edges`、
`abort_does_not_unwind_the_temporary_field_parent`、
`abort_and_normal_return_do_not_invent_prefix_unwind_events`；nested 过滤
`inout_receiver_and_nonreturning_sources_do_not_publish_call_uses`。原测试及 CI selector
均未修改；未运行 Abort、压力、故障注入、native、Linux 或完整 frontend/workspace 测试。
workspace check 仅证明下游编译，不替代精确组合头正式 CI。

独立只读代码与文档复核无未解决 P1/P2，确认来源资格、原 token 与实际 pending owner 匹配、cleanup
不变及旧见证保留；新文件589行，所有增长 Rust 文件均低于1000行，不提高尺寸额度。
本片只做本地提交，不推送、新建 PR 或合并。SPEC-0341 与 D10/E2 仍 in-progress。


### 活动 provider 外部 Shared 来源的循环内转发（2026-11-10）

从 main `cc1e5b41223fabe0248dd4bc2742e95aae9b810a` 独立推进 Phase 3 有界观察。
实施前核已有五片 `c718d72e`、`67f3b97d`、`fd846873`、`812ba9b4`、`3364b51f`
均独立保留，72条相关本地分支的 forwarded producer 仅两版，均仍使用整个 iteration
非空即拒绝的门；#390/#391 的既有头未包含本片，不重放或改写在途成果。

先行 Inout wrapper 候选被现有 P3 L0164 拒绝；已有
`typed_inout_borrow_return_keeps_p3_unsupported_flow_diagnostic` 锁定此边界，不能靠
放宽观察门制造已验证来源。该探针及编译/行为失败日志移至仓库外保留，探针工作树
恢复原样，未提交失败正例，也未修改来源检查器或 Inout 准入。

本片真实红测另证明 Shared 参数在无关 for 内已有两条合法 forwarded/source loan，
但仅循环外一条发布 transport。将 blanket gate 收窄为当前活动 frame 的来源重叠检查：
记录根等于参数来源根，或 frame 对应借用 binding 的已验证 canonical origin 为同根
Place 时，仍排除。只读消费现有 origin lease，不递归 AST、不重推 alias、不要求
origin.is_root()，故两层 alias、字段 alias 与调用结果 alias 同样保留排除。

其他资格不变：ValueParameter、Shared binding、root place、无 receiver 的直接声明
borrow call、真实 Shared token 及 call/argument/origin 精确匹配。只有实际正常 inner
return 才把原 pending token 移到 forwarding state，wrapper 实际 Return 再交付。
逐个检查全部活动 frame；已经退出的同根 provider 不污染后续仍位于无关外层 for 的
交接，无关 alias 不被误拦。没有扩大循环内借用 binding、Inout、receiver、投影来源、
动态 owner、语言语义或 native 能力。

两个嵌套临时 provider 的 Return 固定核八步完整清理：内层 element/provider/source/
临时值，再到外层同序；两份 exit plan 与 iteration_cleanup_at 的完整序列相同，
没有被转发 token 的 EndCallLoan 或参数 payload Drop。provider source 清理与这里
延续的参数 source 是不同 loan。source 求值中的 Return 不清尚未 Acquire 的 provider。
此片不构造新 cleanup 动作；owner 观察开关的完整 Analysis 对照，不是旧版前后差分。

| 验收项 | 本片实际选择 | 结果 |
|---|---|---|
| 新公开源码合同 | `loop_forward_source.rs`，8项 Rooted/Memory 双入口、1项跨文件 Rooted 正反序；完整固定身份向量 | 9项通过，计入下行，不另累加 |
| 直接调用回归 | `unit_direct_call_loan_ends`，下列6项整项过滤 | 120 passed |
| 私有共享邻控 | 新 owner 开关完整 Analysis 等价1项、`adjacent_borrow_results` 7项、Group-only lookup 1项 | 9 passed |
| 原来源与借用邻控 | `ownership_borrow_continuation`、`ownership_nested_borrow_arguments`、`ownership_receiver_backedge`、`ownership_borrow_origins` | 14 + 13 + 4 + 5 passed；nested过滤1项 |
| 既有 provider 清理见证 | `ownership_iteration` 中 `nested_return_plan_orders_each_scope_before_its_provider`、`when_source_return_does_not_clean_up_an_uncreated_provider` | 2 + 1 = 3 passed；首过滤器含 Single/Unit 各1项，原见证保留 |
| 编译与静态质量 | frontend all-targets Clippy（`-W clippy::all -D warnings`）、workspace all-targets check | 通过；check仅证明下游编译 |
| 提交门禁 | workspace fmt check、docs、diff、相对 `cc1e5b41` 尺寸检查 | 通过：863 Markdown、2243 Rust、36项既有超长，额度不变 |

Mac arm64/Rust1.96、LLVM21、默认 debug、jobs=2、locked/offline、自有复用 target 串行。
共168项不同普通测试通过，含10项新增，不重复累计单项与过滤器复跑。
独审提出的 alias probe 实际抓到仅比较记录根的漏拦，补 canonical origin 排除后全部
新增测试通过。原红测、alias 失败和绿测保留仓库外；新增 oracle 首次误用 private
place 构造 API 的编译错误也保留，已改为只读原 root 比较，不伪造事实。

direct-call 整项过滤 `nonreturning_suffix_and_unreachable_calls_publish_no_cleanup`、
`constructed_borrow_aborts_without_unwinding_and_ends_on_the_normal_path`、
`abort_does_not_publish_receiver_prefix_ends`、
`copied_nullable_field_keeps_the_temporary_parent_on_both_call_edges`、
`abort_does_not_unwind_the_temporary_field_parent`、
`abort_and_normal_return_do_not_invent_prefix_unwind_events`；nested 过滤
`inout_receiver_and_nonreturning_sources_do_not_publish_call_uses`。原测试和 CI 选择不变。
未运行 Abort、压力、故障注入、native、Linux、完整 frontend/workspace 测试或正式 CI。

静态 Return site 的事实不保证运行时命中次数、循环终止或动态实例恰一次释放；source
继续转发也不是 source end、caller 动态身份映射或全局 absence/统一顺序证明。旧 Single
见证全部保留，独立代码与文档复审无未解决 P1/P2。新增文件749行，其余增长 Rust
文件亦低于1000行，不提高尺寸额度。仅本地
提交，不推送、新建 PR 或合并；SPEC-0341 与 D10/E2 仍 in-progress。


## 178. 一跳具名输入进入真实条件循环边界（2026-11-10）

以实际 main `cc1e5b41223fabe0248dd4bc2742e95aae9b810a` 新建隔离分支，快进复用
§176 `19d597e81c433bbce6d32823702d95767034b0f1`。开始核192条远端分支、在途PR与
本地候选，收尾再核 main/#390 仍为 `cc1e5b41`/`ca322bc5`；不存在重复能力。§170 WIP
未读取复用或修改，其他线候选不纳入本轮组合；保留旧§176工作树与父链。

本片把已证明的一跳 seed literal→Name move→held 输入接入既有九类末尾 If：
NN→NormalJoin、CC→SplitContinue、CB/BC/BB→SplitExit、NC/CN/NB/BN→MixedJoin。
生产只删除 loops.rs 的 alias＋Conditional 五行拒绝门及 invariant.rs 四行废弃 helper。
五raw事件capture、唯一真实literal producer、完整held OwnedValue、17槽检查、真实分臂
快照、正常merge后读取、双终端空merge、cleanup、suffix门与公开类型均未改变。

四个条件类型依旧没有post_loop字段或getter；私有suffix_boundaries仍返回None。
公开catalog与边界证明真实literal producer→held身份，不提供完整incoming move图或
最终消费图。旧retired Drop仅用于清理兼容对照，不把它或静态边界当成条件后缀运输。

原源码测试 `real_named_input_reaches_the_actual_normal_join` 先以缺少正常合流产物失败，
红提交 `01dc6861db040c91f2df52d75fd1532c43d796ea`，最小改动后同名同源码转绿。
实现/测试/架构提交 `e544586f76f28b9df11acb93ec659dde441c8dc5`，树
`b063b732c47f620e7d8d368f3d2d871cd811b9ec`；本节为其后的纯文档追加。

| 验收项 | 实际范围 |
|---|---|
| 真实分臂与合流 | 新3项lib普通测试为原红见证、`named_input_preserves_each_real_conditional_path`、`conditional_named_input_keeps_unsupported_states_and_atomic_failure`；360组矩阵为5条件对 ×2种Unit声明 ×4类body ×9完成组合。复用原四类完整oracle，精确目标getter唯一命中、AST/edge/每臂快照/实际join/backedge保持；双终端不制造共同body binding |
| 身份与拒绝 | 24份合法否例、九组合跨文件和同源二次分析、3类整体失败清空；相同raw AST索引不能跨source/analysis解析token，输入/body catalog双向隔离，seed/held无Drop、retired唯一AfterStatement Drop；合法单源均完整比较观察开关前后的Analysis |
| 公开入口 | `conditional_named_input_matches_both_public_routes` 的360正＋12负经MemoryDocument与双源Rooted，共744次分析、1116个source inputs（1080准入、36拒绝）。复用增强后的四类helper，新负例明确核全部六invariant getter为空，正例核四条件getter恰一命中 |
| 旧源码保全 | §176私有与公开两处九If生成源码完整保留，精确转为对应能力。非作者逐字重建并比较旧九份a.ko/b.ko；第一源原字节进入分析，第二源只改函数声明。匹配原输入使用完整等值，不用If子串推断准入；旧helper主体与其它拒绝语义保全 |
| 独立复验 | 非作者审生产/测试/架构，指出并修正公开负例/唯一性与旧源码改名问题后复审通过；核228条日志、选择、12目标来源和哈希，再在上述clean提交独立执行4项新普通测试通过，前后HEAD/tree/clean与12份二进制哈希不变 |

最终逐完整名称 `--exact --nocapture` 执行228项不同普通测试：lib113、ownership_iteration28、
multifile_ownership_checking5、ownership_closures2、ownership_borrow_continuation1、
multifile_constant_qualification11、unit_direct_call_loan_ends4、while_flow_facts16、
type_callable/inout_storage17、type_map19、type_checking/control_join_regressions3、
control_branch_join9。原224全部保留，无重复，恰增3lib＋1公开；每项1 passed、
0 failed/ignored/measured。中间红绿、补强复跑与独立4项不累加，测试总数仍为228。
12目标先编译后执行；完整选择、逐项日志和二进制SHA256保留仓库外，不入Git。

Mac AArch64、Rust1.96.0、LLVM21.1.8、默认debug=2、jobs=2、locked/offline，独占target
串行。严格frontend all-targets Clippy（`-W clippy::all -D warnings`，28.27秒）、workspace
all-targets check（45.76秒）、fmt、docs867、diff通过；相对 `19d597e8` 的Rust尺寸门禁
通过，2314手写Rust、36项既有超长，未改豁免。架构页删除重复空行后保持200行上限，未删旧合同正文。

旧损坏封印/Abort仅编译未执行，未运行完整frontend/codegen/workspace测试、native、
Linux或正式CI。本片不证明changing owner、header/exit phi、动态实例、循环终止、
条件后缀消费或Phase4；SPEC-0341与D10/E2仍in-progress。仅本地分步提交，未推送、
改现有PR、新开PR或合入main；新合流仍须另行验证其精确组合。

### Shared 来源借用返回的 Exclusive 辅助结束（2026-11-10）

基底 main 为 cc1e5b41223fabe0248dd4bc2742e95aae9b810a；另建独立分支，
不修改原树或在途六片。逐项核对既有 handoff 实现后，补齐无关 Exclusive owned 根
辅助实参的正常结束观察。Shared temporary 辅助片 3364b51f 保持独立，合批须同时
保留两片准入与测试，不能以此片的 root 门替换其 temporary 证明。未推送或新建 PR。

合法基线为具名 view(source: String, inout aux: String) 从 source 返回普通借用，
caller 以 owned owner 和可变 other 初始化 borrow val。真实源码在 Rooted 与
MemoryDocument 均通过 P3，且已有 Exclusive loan；修改前 handoff 为空，新增测试
实际失败。首次误从主树运行的零命中过滤结果单独保留，不计行为证据或通过数。

只移除 handoff 的全 Shared 实参限制，仍要求全部实际 loan target 为 root Place，
所选唯一来源单独要求 Shared、与 binding origin 相同、formed owned 且当前仍活。
辅助 token 来自同一次正常完成调用的 pending 队列，排除来源 argument，并核真实
operand place、formed owned、当前存活及非 borrow binding/provider。正常返回按原
pending 正序移出该调用的辅助 token，原 call-local retain、source transport、
借用终止、ASAP drop 与 prefix unwind 次序不变，不从 descriptor 或 Span 合成结束。

原 observes_borrow_auxiliary 继续以 Shared 包装共用 root 证明，两个 source transport
调用方不取得 Exclusive 来源能力。公开注释区分 Shared 来源与 Shared/Exclusive
辅助；列表只含有界正常退休，不是 direct/prefix 的统一结束序列，不提供全流或
global absence。Inout 来源与含 Inout 的同步 borrow consumer 仍保持现行 L0164。

新增8项公开合同由7项双入口与1项跨源 Rooted 顺序/倒序组成：

- 完整辅助向量固定两个 Exclusive 加一个 Shared 的具名实参求值顺序、实际
  call/argument、callee parameter、caller owned root 与原始 & marker。
- 辅助正常返回后可再次 replace，来源在 item 使用前仍不可变异；同根重叠
  Shared/Exclusive 实参继续 L0135。直线来源 token 保留至真实 println(item)
  AfterStatement；混合 Return 与前置 for 继续保守结束于整个 body 的 AfterStatement。
  每例固定实际终止 statement 的完整源码，binding end、transport 与唯一正常
  source drop 均锁该点，不把复杂 CFG 的 scope end 冒充 last-use。
- 后续实参 Return 时不发布该路径的完成 handoff；混合 Return 有独立前缀和正常
  路径，Exclusive 前缀动作计入 source prefix 的先行计数。先于所有实参的 Return
  不登记 Exclusive token，不可达调用不产生 handoff。
- Inout 非 owning 参数、Copyable 根、已作为 provider 的根不列入辅助退休；字段
  保持整次 handoff 排除。真实成功前缀后发生错误或 deferred 撤销公开 handoff，
  不要求旧 deferred cleanup 表为空；source transport 的 Shared 门不扩展。
- 跨源例核 caller SourceUnitId、callee declaration/parameter Span 与参数序号，
  输入倒序仍有同一条真实 Exclusive token，而非按列表位置猜测来源。

另新增1项 owner observation 开关的完整 Analysis 对照，包含混合 Return 与正常
handoff；仅清除观察槽后要求完整旧产物相等。这不是 handoff 修改前后全产物等价。
扩展测试中曾误把混合路径两份 source drop 限成一份，并对 deferred 旧 drops
作空表假设；加强终止点时也确认复杂 CFG 仍采用 body scope end，而非直线 last-use。
失败日志保留，修正测试后重验，未改生产清理或旧测试来规避失败。

本机 Mac AArch64、Rust 1.96.0、LLVM 21.1.8，locked/offline、默认 debug、
jobs=2、独占复用既有 target 串行。最终实际通过164项不同普通测试，其中9项新增；
不重复累计红绿探针、独立8项或终止点强化前后的重复运行：

| 本轮验收项 | 实际命令选择（均为 lang-frontend） | 结果 |
|---|---|---|
| 真实 handoff 与全部普通邻接切片 | cargo test --test unit_direct_call_loan_ends，以下6项 skip | 119 passed，6 filtered |
| owner 开关完整 Analysis | cargo test --lib exclusive_borrow_auxiliary_preserves_all_cleanup_with_owner_observation | 1 passed |
| raw 邻接失败门 | cargo test --lib adjacent_borrow_results | 7 passed |
| 原 Group lookup helper | cargo test --lib only_groups_forward_the_original_call_to_the_typed_predicate | 1 passed |
| continuation、origins、receiver | cargo test --no-fail-fast 的 ownership_borrow_continuation、ownership_borrow_origins、ownership_receiver_backedge | 14 + 5 + 4 passed |
| nested borrow | 同批 ownership_nested_borrow_arguments，以下1项 skip | 13 passed，1 filtered |

direct 保持以下6个未运行选择，不改原测试或 CI selector：
nonreturning_suffix_and_unreachable_calls_publish_no_cleanup、
constructed_borrow_aborts_without_unwinding_and_ends_on_the_normal_path、
abort_does_not_publish_receiver_prefix_ends、
copied_nullable_field_keeps_the_temporary_parent_on_both_call_edges、
abort_does_not_unwind_the_temporary_field_parent、
abort_and_normal_return_do_not_invent_prefix_unwind_events。
nested 的 inout_receiver_and_nonreturning_sources_do_not_publish_call_uses 未运行；
未运行项不计通过，普通例不替代 Abort、压力或故障注入合同。

frontend all-targets Clippy（-W clippy::all -D warnings）、workspace all-targets
check（仅编译）、fmt --check、check_docs（863 Markdown）、diff 与相对精确基底的
Rust size guard 全部通过。2243个 Rust 文件、36项历史超长，额度不增；
新增 exclusive_auxiliary.rs 为587行，既有 compilation_unit.rs 的1030行未增长。
代码与合同已独立复审；所有旧源和断言保留，不按此片更改 Single witness。
原始红绿及后续校验日志保留仓外，不入库。

未运行 frontend/workspace 全量测试、native、Linux、额外压力/故障注入或正式 CI，
不推送或自动合并。D10/E2 与 SPEC-0341 仍为 in-progress；此片不实现一般
CFG/NLL、动态 owner 实例、完整借用事件流或 Phase 4/native。


### 普通 continue 嵌套与顺序循环的原源 Unit 差分（2026-11-10）

本片从实际 main `6ce85b07f9e36fcd322544c93f33274d6ba15c1b` 隔离建立，
定点核对 #390 `ca322bc5` 与该文件历史，未重复上一片 L0135/L0133 诊断强化。
只增强 `owned_source_continue_nested_scope_boundaries` 原一个测试，四份源码表逐字保留：
fresh inner、独立 inner、同源 nested 与同源 sequential。不吸纳其他未合入候选，生产、
共享 harness、工具链及 CI selector 均未改；合队须同时保留 #390 的原诊断片，不能整文件替换。

在同一基底实际记录原 Legacy 与 strict Unit 基线：Legacy fresh inner 成功，其余三份均在
原 keys 声明的23..27处 UnsupportedNode；Unit 四份均成功。`0789a421` 首先固定三份
Legacy 完整 kind/source/span，旧成功路径和所有原见证保持；后续代码提交 `093ccdb319e2be7c1ec7dcd5b3d13ddcd28f4bfc`
（tree `9863b344d1d02775004e85f3cffe46c0b93e0cee`）再增加严格 Unit 同源对照，净退休0。
Unit 必须通过最终类型准入、四阶段零诊断及原 basic capability，保留单一 MemoryDocument
和原 main 的完整 source identity；同轮两份 typed/ownership iteration descriptor、原 main
实际 SSA entry、两个 element access 及 verified LLVM 均有断言，不接受空集或替代 root。
不把三项既有能力差异写成语义等价，也不修改 Legacy 拒绝边界来获得绿灯。

| 验收项 | 实际选择 | 结果 |
|---|---|---|
| 原四输入及 continue 邻控 | `cargo test --locked --offline -p lang-codegen --lib ssa::lower_frontend_tests::owned_source_continue_tests::` | 19 passed，0 failed/ignored，2798 filtered |
| 严格静态检查 | codegen all-targets Clippy，`-W clippy::all -D warnings`，locked/offline | 通过 |
| 工程门禁 | workspace fmt、docs、相对 `6ce85b07` Rust尺寸、diff | 通过；863 Markdown、2242 Rust、36项历史超长，本片不改额度 |
| 独立只读复审 | 原源/Legacy、strict admission、实际 entry、两循环和架构边界及旧片差异 | 无 blocker；复审仅核读日志，未自行运行测试 |

原19个测试身份保留；本文件 direct Legacy analyze 7→7、lower_scalar_file 6→6。
19项是最终不同测试数，基线及固定Legacy后的原单项复跑不累加。Mac AArch64、Rust1.96.0、
LLVM21.1.8、debug=0、jobs=2、locked/offline，独占target串行。原始观测及命令日志留本片
忽略的 `.local-artifacts/`，原源码和固定断言入库；不依赖临时日志解释验收范围。
未运行完整 codegen/frontend/workspace 测试、native 进程、Linux、故障注入或 sanitizer。
仅本地提交待统一队列，无推送、开PR或合并，精确发布头CI另验；D10/E2和SPEC-0341仍
in-progress，任意循环 owner/phi、完整借用全序、全部 Single 退休与 Phase4并未闭合。


### 普通 continue 分组、同名声明与 alias 的原源身份对照（2026-11-10）

本片从实际 main `144793e1e594fa041c34b22c2e65baab3a5e7c69` 隔离建立，定点核对
#390 `ca322bc5` 与既有代码提交。只增强 `owned_source_continue_declaration_identity_and_groups`
的三份原输入，不重复已提交的五份 ownership 诊断或四份 nested/sequential 对照。
三份原源码逐字保留，透明 Group、函数边界、Array/List、同名 keys 和真实 first() 调用均未改写。

实际基线中，Legacy 与 strict Unit 均接受分组和双函数输入；alias 仅 Unit 可 lowering，
Legacy 在 keys 声明53..57处 UnsupportedNode。`55ef5f1c` 首先保全两条原成功路径并
固定 alias 的完整 kind/source/span；后续 `89e54f438e4647737325de92ccc69f420bead062`
（tree `d733ff010209b4b8fc6b36e900de97c98d4dc5fa`）增加独立 Unit 对照与架构事实。
全部 Legacy 见证保留，旧入口净退休0，不把已有差异解释为两个入口完整等价。

Unit 通过严格最终类型准入、四阶段零诊断和原 basic capability；原 MemoryDocument 字节、
解析根、真实 main entry 和函数数量均固定。每函数恰一同轮 iteration，provider 分别核对
Array/List，source-qualified Place root 必须匹配原 keys 声明23、24/131或53的精确位置；
两个同名根不能合并。实际 SSA 保留每函数一次 element access，main→first 同时核 caller、
callee、空参数/receiver和原 first() 范围；最后以真实 entry验证SSA/LLVM，不由字符串名称猜callee。

| 验收项 | 实际选择 | 结果 |
|---|---|---|
| 原三输入及 continue 邻控 | `cargo test --locked --offline -p lang-codegen --lib ssa::lower_frontend_tests::owned_source_continue_tests::` | 19 passed，0 failed/ignored，2798 filtered |
| 严格静态检查 | codegen all-targets Clippy，`-W clippy::all -D warnings`，locked/offline | 通过 |
| 工程门禁 | workspace fmt、docs、相对 `144793e1` Rust尺寸、diff | 通过；863 Markdown、2242 Rust、36项历史超长，不改额度；目标测试文件662行 |
| 独立只读复审 | 原源/旧见证、严格准入、声明/provider/call身份及架构边界 | 无 blocker；复审核读日志，未自行运行测试 |

原19个测试身份及该测试文件 Legacy analyze 7→7、lower_scalar_file 6→6保持；最终19项不重复
累计原单项基线和固定Legacy后的复跑。Mac AArch64、Rust1.96.0、LLVM21.1.8，debug=0、
jobs=2、locked/offline，独占target串行。完整基线和命令日志留本片忽略的 `.local-artifacts/`，
原源码与固定合同入库；无生产、共享harness、工具链或CI selector修改。
未运行全量codegen/frontend/workspace、native进程、Linux、故障注入或sanitizer；没有验证
与其他候选的组合。本片仅本地提交待统一队列，不推送、开PR或合并；合队保留同文件的
既有诊断及nested合同，精确发布头CI另验。一般alias/owner phi、完整借用全序、全部Single
退休及Phase4仍未闭合，D10/E2与SPEC-0341保持in-progress。

完成前 main 已推进到 #406 `cc1e5b41223fabe0248dd4bc2742e95aae9b810a`，该批改类型流，
未改本片目标文件。本片仍只在 `144793e1` 基底验收，未合入或验证新main组合；合队后须重验。


### D10/E2 原 return 边界 strict Unit 差分（2026-11-10）

从 main `cc1e5b41223fabe0248dd4bc2742e95aae9b810a` 建立独立工作树；收尾只读复核
main 仍为该头，#390 仍为 `ca322bc558be5336efdc701434cb432e5b7c509e`。目标为
`lower_frontend_owned_source_return_boundary_tests.rs`，#390 相对该 main 未改此文件；
定点核对统一队列及既有 owned-return 工作树未发现该文件在途修改。此次不修改此前
continue 三片文件，不据此推定其他分支无冲突，也未执行统一候选合流或组合验收。

保全八份完整原源：双 return、mixed continue、replacement seed、alias seed、Resource
source、outer owner with inner loop、same-source nested/sequential。Legacy 仍逐项拒绝，
完整 `UnsupportedNode` 与原 source/span 固定：普通六项 `keys` 为23..27，alias为53..57，
Resource为63..67。目标测试文件全部12个旧测试名、原输入字节及另四项见证保留；
直接 raw `analyze` 两处、`lower_scalar_file` 一处均不减少，旧入口净退休0。

同一原源另走 strict basic `UnitSource` 最终准入，前端无诊断且无 deferred；逐项验证
同轮 MemoryDocument/source字节、1或2个唯一真实 For 的 qualified provider 与源码表达式、
原 main 内的 statement范围、原 main entry及相应1或2条 ContainerElementPlace 指令，验证SSA与LLVM。
不以常量函数、其他入口或替换源码代替原控制流；不改生产准入或语义。Legacy固定诊断提交
`20ce07d26eedd216d7ce1c883c307f7f169a73a3`；Unit差分与Architecture代码头
`c1e89963392b8b36881d22f50d46f90112d01bac`，代码树
`4073d53e627305a727bab96b9e4dd8fac2548513`，目标Rust文件264行。

| 验收项 | 实际选择与结果 |
|---|---|
| return全族 | `cargo test --locked --offline -p lang-codegen --lib ssa::lower_frontend_tests::owned_source_return_tests::`；90个不同完整测试名：48 core、12 boundaries、30既有native邻控；0失败/忽略，2727 filtered |
| 固定前后原边界核对 | 原始观测及Legacy精确跨度各12 passed，不与全族90项重复加总 |
| 静态门禁 | codegen all-targets Clippy，`-W clippy::all -D warnings`；workspace fmt与diff均通过 |
| 文档与尺寸 | `check_docs.py`通过863 Markdown；相对cc1e5b41的尺寸门禁通过，2242 Rust、36项既有超长，额度不变 |
| 独立复核 | 原源/旧见证、严格Unit封闭视图及真实循环身份、SSA/LLVM入口、Architecture事实和日志唯一计数均无阻断项 |

macOS arm64、Rust1.96.0、LLVM21.1.8，locked/offline、debug=0、jobs=2，独占target串行Cargo。
30项既有native邻控实际经过目标文件、链接及运行，但不是新八份原源的native运行结果；
新八源只证明strict Unit可准入并通过SSA/LLVM，不证明逐出口清理顺序、精确分配/释放、
两入口完整语义等价或任意动态owner/phi。原始观测与完整日志保留在忽略的本地输出中，
稳定原源和完整Legacy oracle在上述提交可追溯，不把重复源码和日志纳入仓库。
未运行完整codegen/frontend/workspace行为测试、Linux、故障注入或sanitizer；未组合前两片
continue本地成果与#390复验，不借用旧CI当作新组合证据。只本地提交，无推送、合并或新PR；
统一队列应保留各片旧见证、在最新main的精确组合头重新验收。D10/E2与SPEC-0341仍in-progress。

### #390 已完成 D10/E2 切片合批验收（2026-11-10）

本批在既有 #390 `ca322bc558be5336efdc701434cb432e5b7c509e` 上保留父链合流，
main 基底为 `cc1e5b41223fabe0248dd4bc2742e95aae9b810a`。先 fast-forward 至
owner `19d597e8`，再以普通 merge 保留七个借用完成片 `c718d72e`、`67f3b97d`、
`fd846873`、`812ba9b4`、`3364b51f`、`351fb93a`、`3ac51b74`，owner §178 最终头
`1f6cd0cb`，以及 nested/identity/return 对照 `4f0c2499`、`667c83f7`、`82117953`。
不把其他工作树的未提交内容或 §170 在途成果纳入，也不重写候选分支。

共享 handoff 冲突保留 owned root 与实际 pending Shared temporary 的资格并集；
辅助退休保留 Exclusive root 和 Shared temporary，来源与两个 source transport 调用方
仍限定 Shared。新增正序/反序两项真实混合实参测试，逐项核 call/argument/parameter、
原 `&` marker、pending 顺序及 temporary 唯一 CallReturn Drop；来源继续到
`println(item)` 的 AfterStatement，结果 end 与 binding transport 身份一致。
Exclusive 三出口、插值结束、Group 内部索引及全部 raw 失败撤销同时保留。

验证代码头 `fc7f60b2db9df3b3ab8590941a2b182fa15b8c2c`，代码树
`98c749a5e24c87e9cfa2f74503e7d3e2b6fffeab`。先对指定目标 `--no-run --message-format=json`
记录实际测试二进制；同一二进制 `--list --format terse` 固定选择，以 `--exact --skip`
排除补集，每目标一次运行，逐项对比完整成功名称集合、计数及二进制 SHA256。
不以编译成功、零命中或前一候选日志代替本轮执行。

| 本批验收 | 实际范围与结果 |
|---|---|
| frontend 去重组合 | 22目标690项通过，0失败/忽略；原§178的228项全部保留；上一548项中538同名、3项因能力变化重命名并保留旧源，7项按下述边界未运行，按合同身份为541旧项＋149新增 |
| frontend 选择 | lib151（原owner普通合同、邻接raw失败门、七片owner观察及Group lookup）；unit_direct_call_loan_ends293；ownership_iteration36；continuation14、nested13、receiver4、origins5；其余15目标174项为原消费者、条件流、类型、解析恢复及插值输入清理邻控 |
| codegen 去重组合 | 209项通过，0失败/忽略；旧119完整保留，增加 owned_source_return_tests 全族90；owned_source_continue_tests 19已在119内，不重复累计 |
| native 边界 | 上述209中44项实际生成目标文件、链接并运行（旧14＋return族30）；八份新增return边界原源仍只证明strict Unit准入和SSA/LLVM差分，不冒充native证据 |
| 静态门禁 | `cargo clippy --locked --offline -p lang-frontend -p lang-codegen --all-targets -- -W clippy::all -D warnings`、`cargo check --locked --offline --workspace --all-targets`、`cargo fmt --all -- --check`及diff全部通过 |
| 文档与尺寸 | `python3 scripts/check_docs.py`与`python3 scripts/check_rust_sizes.py --base cc1e5b41223fabe0248dd4bc2742e95aae9b810a`通过；867 Markdown、2321手写Rust、36项既有超长，额度不变；dataflow聚合后1171行，仍低于既有1248额度 |
| 独立复核 | 11候选祖先、独占文件blob、原验收正文、注册并集、共享gate、旧continue诊断与新混合测试均无阻断；另核899完整成功集合、输入身份和二进制哈希，未将重复运行累加 |

本机为macOS arm64、Rust1.96.0、LLVM21.1.8；locked/offline、jobs=2、debug=0、
独占既有target，Cargo串行。首次日志汇总使用行首匹配，两个有并发诊断输出的目标被
误报名称缺失；两目标进程实际退出0，保留原日志并按完整精确名称及成功总数复核，
没有修改测试、隐藏真实失败或为计数重跑。此前单独10项混合邻控不重复加入899。

本地沿用七个明确未运行项：direct-call中的
`nonreturning_suffix_and_unreachable_calls_publish_no_cleanup`、
`constructed_borrow_aborts_without_unwinding_and_ends_on_the_normal_path`、
`abort_does_not_publish_receiver_prefix_ends`、
`copied_nullable_field_keeps_the_temporary_parent_on_both_call_edges`、
`abort_does_not_unwind_the_temporary_field_parent`、
`abort_and_normal_return_do_not_invent_prefix_unwind_events`，以及nested的
`inout_receiver_and_nonreturning_sources_do_not_publish_call_uses`。原测试和正式CI选择未改。
owner既有大型压力/破损模型守卫仅随all-targets编译；未运行完整frontend/codegen/workspace
行为测试、Linux或sanitizer。原始命令、选择和日志保留仓外，稳定原源与断言在上述代码头
及各候选提交中可追溯；本批最终发布头仍须由[#390正式CI](https://github.com/Halckon/Koven/pull/390)
验证当前main和精确head，旧 `ca322bc5` 的绿灯不能替代。D10/E2与SPEC-0341保持in-progress。


## 180. 一次根替换后 Break 的协议提案

状态：仅提案与现有源码基线；新协议实施等待
[ADR-0037](../../adr/accepted/0037-replace-break-owner-boundaries.md)正式接受。
用户已确认最小设计/实施范围，但本节不将范围确认当成 ADR 仓库接受。

去重输入为 main `cc1e5b41223fabe0248dd4bc2742e95aae9b810a` 与已发布 #390
`dd3515251821d10a733334359518dd869e26c957`（tree `369d198b2415b37d4d79339dbb7e936a37372802`）。
新隔离分支基于该 main 并快进复用完整 #390；旧分支、已有 oracle 和受阻 §170 WIP 不动。
#407 的 Elvis native 边界与本片不重叠。本节不将父链或其它片 CI 算成本片验收。

候选只观察外层 String root 的真实 `replace` 后立即 bare Break：实际 prefix 输入 P、
fresh body 输出 Q、六项实际运输事件，以及分别为 P/Q 的两条互斥出口。
不接受一般回边/phi、多输入、条件合流、共同出口或后缀消费；尾部直接移出仅用于 liveness。
不修改 Guide、普通 planner 状态传播、cleanup、SSA、LLVM 或 runtime；正式合同见 ADR。

| 验收项 | 目标/过滤器 | 本片结果 |
|---|---|---|
| 真实源码既有原语与 cleanup 基线 | frontend lib `loop_replace_break_baseline::outer_replace_break_preserves_actual_primitive_and_cleanup_baseline` | 1普通exact通过：两source×implicit/explicit Unit；各原source分别完整on/off对照，不等于新能力红绿 |
| ADR 提案、编号及 Spec 链接 | `check_docs.py`、其直接测试、`git diff --check` | docs868、检查器41项直接测试、diff通过 |
| 新能力红绿、身份/边界与公开入口 | 接受 ADR 后实施 | 尚未实施或运行 |
| 新增测试静态门禁 | `cargo fmt --all -- --check`、frontend `--all-targets` strict Clippy、sizes `--base dd351525` | 全通过；2322手写Rust/36历史超限，本片新测试128行 |
| 独立审查与基线复跑 | 本片6文件静审；同一普通exact二进制复跑 | 无剩余阻断；复跑1通过，不重复计作第二种用例 |
| workspace、新头CI及其它宿主/native | 未改变跨crate API，不以旧证据替代 | 未运行；新能力尚未实施 |

在 ADR accepted 与本节实施前置闭合前，不新增跨 catalog 产品或放宽原 validator。
本地提案/基线提交不发布远端，不将 D10/E2、一般动态 owner 或本片能力标为完成。

基线首轮因测试 helper 按文本查找 `held` 命中两个真实 Name 而失败；已改为实际声明
NameMarker 定位 initializer/statement，并补精确 While/Syntax 拒绝点后重跑通过。
该失败不是新能力红测。现只证明 checked Replace plan 与旧 cleanup，不证明跨 catalog
P/Q、实际运输事件、post-Break/condition-false 快照或 native/runtime 释放次数。

测试基线本地提交 `252c0bb1a7c75194838f6684b626f2e01aba0c4f`（tree
`c4bd0b892495deb26f022251e796bc2ec72be783`），唯一父为上述 #390 头。Mac AArch64
Rust1.96/LLVM21.1.8、locked/offline、默认debug、jobs2；复用本线独占的既有 target，未降低
编译档位。精确命令为 `cargo test --locked --offline -p lang-frontend --lib
ownership_checking::compilation_unit::dataflow::drop_planner::owner_transport::tests::loop_replace_break_baseline::outer_replace_break_preserves_actual_primitive_and_cleanup_baseline -- --exact`。
独立复跑前后测试源码与二进制hash不变；没有执行模型破坏/故障注入/Abort用例。
纯提案及本账本后继未变Rust字节，不冒称在文档提交重跑行为；新协议实施仍待ADR正式接受。


### §180 正式接受与实施启用

2026-11-10 用户明确同意正式接受限定范围 ADR-0037 并据此实施。本次已将 ADR 迁至
accepted、同步索引与库存；§180 状态转为 in-progress。上述 proposed 阶段及其基线验收
作为已发生历史保留，不以正式接受反向宣称新能力已实现。当前 main 已合入 #390，推进至
`054f0ee102981de17520cb1c7998a4b290370354`；其 tree 与依赖 dd351525 完全相同。
本片继续在原隔离树开发，不改 main、旧分支或用户文件，不推送。

获准实施合同即 ADR 的最小形状、独立双 catalog/六事件/四实际读取、两条互斥出口与
失败原子性。先新增真实能力红测，再接最小生产路径、独立复审及本地分步提交；
一般 phi、共同出口、后续消费和 §170 受阻工作不在本片范围。

### §180 实施与本地验收

正式接受提交 `03851b2a6fe8695059e2cfa88880759ed019c6ff` 后，先提交真实能力红测
`79b251496dae0a161d83c29e40db0dc478da2236`：合法源码的普通 ownership 校验通过，
独立 getter 因缺实际 producer 接线返回 None，ordinary exact 实际失败。随后最小实现
`a04cfd30121c1ab563593363476326f5cdd61bdf`（tree
`a03aa405da142263a1832202f4c3ec000561c4cc`）使同一输入与期望复绿；保留两原提交父序。
最终只读复核 main 仍为 `054f0ee1`，其 tree 等于本片依赖 dd351525；不把 ancestry 合流
或既有 CI 当成本片已验，本次没有合 main、改 PR 或推送。

生产新增独立 sealed `UnitOwnerReplaceBreakObservationV1` 和只读四状态类型；复用原
While、Pending、Replace、Binding、Drop 与 jump 执行 hook，单独校验 actual prefix 和
跨 catalog 六事件。entry、post-jump、body 完成、原 condition-state LoopExit 四次分别读取，
只允许实际 owner/origin 替换，其余根元数据不变。完成身份/Break、两 catalog、全局撤销
及新旧 getter 互斥分别封闭；旧 closed V1、catalog 工厂和全部旧 oracle 不放宽。
精确当前事实见[根替换后 Break](../../architecture/unit-replace-break-owner.md)。

| 验收项 | 目标/精确选择 | 实际结果 |
|---|---|---|
| 新能力与六事件/四边界 | lib `owner_transport::tests::loop_replace_break::actual_replace_break_preserves_distinct_exit_owners` | 真实 red→green；P/Q各自index0但完整身份互异，源/控制/ASAP Drop逐项核对 |
| 身份与非候选、失败原子性 | 同module `replace_break_keeps_actual_source_and_analysis_identities`、`replace_break_rejects_extra_paths_and_revokes_global_failures` | 2普通通过；双source、重分析、相同字面量内容、两Unit形式；18合法非候选及同/跨source全局失败 |
| 两公开输入入口 | `ownership_iteration` 的 `loop_replace_break_unit::replace_break_matches_both_public_input_routes` | 1普通通过；4正源、8合法负源×MemoryDocument/Rooted，24次分析、36个被分析source |
| 原行为基线 | lib `loop_replace_break_baseline::outer_replace_break_preserves_actual_primitive_and_cleanup_baseline` | 通过；原源码、精确旧cleanup与逐source完整on/off对照保持 |
| 原组合与直接消费者 | §178原228普通exact完整保留，加本片上述5项；逐完整名称 `--exact --nocapture` | 233不同普通全部通过：lib117、ownership_iteration29、multifile_ownership5、closures2、borrow_continuation1、multifile_constant_qualification11、direct_call_loan_ends4、while_flow_facts16、type_callable17、type_map19、type_checking3、control_branch_join9 |
| 独立审查与实际复验 | 所有生产接线/model/测试/架构静审；本片5普通exact独立直接运行实际二进制 | 无剩余阻断；5项逐个命中1并通过；20个Rust文件及两binary前后hash不变，不重复计入233 |
| 工程与消费门 | frontend `--all-targets` Clippy `-D warnings`；workspace `--all-targets` check；fmt、docs、size `--base dd351525`、diff | 全部通过；docs869、2326手写Rust/36历史超限未扩大 |
| 损坏模型与deferred守卫 | lib `replace_break_model_rejects_damaged_states_and_catalogs`、`replace_break_deferred_publication_withholds_the_complete_product` | 仅编译，按既有规则未本地执行；精确发布头官方CI另验 |

Mac AArch64、Rust1.96/LLVM21.1.8、locked/offline、默认debug=2、jobs2，所有 Cargo 串行
复用本线独占的既有 target。严格 Clippy23.31s、workspace53.81s；没有改变工具链或降低
档位。新公开测试从实际 cleanup 取得 qualified symbol 后，与独立 names/source 逐项核对，
没有为测试放宽私有 `UnitSymbolId` 构造权限。补强时的导入/借用生命周期/可见度编译错误
已修正，不计作能力红测。原始执行日志、选择清单与binary绑定留仓外，固定源码及oracle入库。

本片本地实现/定向验收完成，但未运行 native/runtime释放次数、其它宿主或新头官方CI；
未执行损坏模型、注入、Abort或全target测试。静态producer不代表runtime实例，condition-false
仍是P而Break仍是Q；不提供共同exit、header/exit phi、后缀消费或一般动态循环能力。
纯文档后继不冒称重新运行Rust行为；D10/E2与SPEC-0341整体保持in-progress。
### 已批准 owner/loan 并行首片（2026-11-10）

状态：approved，尚未实施。用户接受推荐方向后，ADR-0038/0040 在本分支转 accepted；
本节限定 Phase 3 首片，不关闭 D10/E2、不改变 Guide、不接受 0025/0036，也不启动
动态 capture 的未决阶段映射。依赖基线 main `3fe1920a`；0037 实施 `ef5ded79` 和
借用候选 `d6a21333` 等仍需按真实父链合批、去重并重验，不能凭本节视作已纳入。

**Owner 首片**：复用 0037 的固定 Boolean 参数、String root、一次 Replace 后裸 Break。
仅增加唯一实际尾句 `val retired = held` 的真实 take、bind、最终 Drop 观察，显式关联
condition-false 的 P 与 Break 的 Q。进入实现前先保留合法源码的能力红测。生产 hook
须来自实际状态交接，不能重放分支、回填事件或把旧 condition state 的 P 冒充 Q。
封闭产品须证明两条 incoming 完整、实际消费恰一次、原定义与跨 catalog 身份不变；
只观察双出口仍不足以宣称共同消费。若真实 planner 交接无法满足，应保留红测并报告
具体缺口，而非放宽 validator。排除通用 suffix、header phi、多根、Group/Not、alias、
borrow/capture、Resource 与一般循环能力，保留既有合法输入/旧观察契约。

**Loan 首片**：复用已完成 direct/result/receiver 观察，先为一个明确登记的普通直线
直接调用区域形成真实统一动作序列及覆盖证据。首个成功范围限定无 closure/provider、
无嵌套控制、无 source-error/deferred 的普通 direct call；receiver/result 的已有实际
事件不得遗漏或从旧表排序补造，未封闭的家族须明确 Partial/Unsupported，不得宣称
整个 callable Complete。Begin、handoff、End、权限恢复和 owner Drop 保留真实身份及
顺序，结果/父来源交接不当作 End。Complete-empty 必须由同一覆盖认证器正面证明；
没有事件、仅 observer 未拒绝或删除 unsupported 样例都不能替代覆盖证明。

**共同验收**：双 Unit 输入入口，跨 source/重分析身份，observer on/off 完整旧产物
一致；合法非候选不扩大语言拒绝，非空前缀后独立 source 错误/deferred 全局撤销 raw
及公开新产品。保留旧源码、诊断全文与独立 oracle。Owner 核 P/Q 同 index 不同 catalog、
零轮/一轮真实消费；loan 核正常结束、handoff、零事件完整与未支持空表区分、同点
End-before-Drop。破损模型、注入、Abort 与大型守卫遵守既有 CI-only 约束。

提交前定向行为、相关共享契约、严格 Clippy、workspace check、fmt、docs、size、diff
和独立复审分别登记；不并发争用同一 Cargo target。Phase 4、SSA/native、释放计数、
双宿主正式 CI 另列未运行，不用 Phase 3 成功退休 Single oracle。后续集中同一批次
评估，不重写 §170、parent-binding receiver 脏树，也不自动推送或合并。

本次仅完成接受与首片合同记录；没有新增生产实现或运行 Rust/native 验收。

### ADR-0038 B：唯一尾句的真实两入边 carrier（2026-11-10）

本地 owner 首片实施头 `4ff61d89d0b9c45419632fa112f240e65033fb8c`，代码树
`3aa737972110c1d8768f72ead2199faaaeb6113b`。隔离分支基于 main `3fe1920a`，
通过普通 merge `72858945`、`60578376` 保留 0037 `ef5ded79` 与批准合同 `23d70006`
真实父链；仅合并三处文档尾部/ADR 库存并集，不改原工作树或其历史。

能力红测提交 `c7e7b926` 的合法原源实际失败：测试专用实例局部 probe 看到尾句 Move
只取 condition P，legacy Unsupported 随后清除求值 owner，retired Binding/Drop owner
hooks 为空；原双出口仍有效。新实现不向这些旧 hook 补造 Q。实际完成的 body state 中
Q 移入严格 B carrier，与 condition P 保留完整原 catalog 身份；carrier 经真实 Name take、
`EvaluatedOwner`、真实 binding、实际 ASAP Drop 转移一次。新产品记录一个按边选择值的
Take/Bind/Drop 三动作，incoming 显式保留 condition-false P 与 Break Q，不产生新 producer。

同一原源绿色断言保留旧 scalar hook 的上述实际观察，另核新 carrier 的两入边和三动作；
不把旧三种 scalar 事件要求改成回放 Q 来取绿。此为 Phase 3 有界观察，既有 cleanup 与
observer on/off 完整产物保持；未实现一般 phi、更多后缀、borrow/capture 或下游运输。
事实见[根替换后 Break](../../architecture/unit-replace-break-owner.md#唯一尾句的真实状态交接)。

| 验收项 | 实际范围与结果 |
|---|---|
| 能力 red→green | `loop_replace_break::replace_break_suffix_requires_actual_q_handoff`；原源实际失败后由真实 carrier 提供完整消费，旧 scalar probe 不参与生产产品 |
| 最终普通精确集合 | 精确实施头重新编译并固定 binary/SHA256；lib116＋`ownership_iteration`双入口1，共117不同 ordinary exact 通过，0失败/忽略；早先同名复跑不累加 |
| 身份、边界与撤销 | 原本片真实 P/Q 各index0不同catalog、双source/重分析、相同字面量、两Unit形式、18合法非候选与source-error原子撤销继续通过；新增consumer逐项核incoming/Take/Bind/Drop |
| 旧共享契约 | 上述lib116覆盖Choice、closed/invariant循环、实际借用表及完整on/off对照；没有全量运行frontend或iteration |
| 严格静态门禁 | frontend `--all-targets` Clippy `-W clippy::all -D warnings`通过；`cargo check --locked --offline --workspace --all-targets`通过；fmt与diff通过 |
| 文档与尺寸 | docs878；检查器41直接测试通过；size `--base 3fe1920a`通过，2264手写Rust、36历史超长；planner1578行，原1669额度不提高 |
| 独立复审 | 真实Q交接、单次consumer、严格范围与撤销无剩余阻断；同source错绑漏洞已补独立准入target锚点并复审通过；五行planner接线职责合理 |
| CI-only守卫 | 新7变体consumer破损、原双出口破损及deferred注入已编译，未本地执行；正式精确头CI另验 |

**执行偏差如实保留**：第一次沿用旧 owner 清单运行119个 exact，事后逐源码及helper复审
发现其中两个“普通”名下混有损坏模型：`owner_transport_tests::swap::
root_swap_then_replace_preserves_actual_owners`（删除/重复Swap、别名及foreign owner），
`owner_transport_tests::if_observation::
mutually_exclusive_consumption_replays_from_the_same_incoming_owner`（展平If、清空else、
foreign owner）。两项已经实际执行，不能声称本轮完全未执行破损模型；不计入上述117普通
验收，后续整项排除且未重跑，不修改旧测试来隐藏偏差。原119日志和最终117精确名称/binary
manifest均保留仓外。逐源复核未发现Abort、deferred注入或大型压力守卫混入。
`choice_admission::actual_choice_is_delivered_bound_and_dropped_while_legacy_stays_unsupported`
另含移除可选产品以验证旧投影隔离的断言，未制造不一致运输模型，保留分类说明。

Mac AArch64、Rust1.96/LLVM21.1.8、locked/offline、默认debug=2、jobs2，复用原owner独占
target并串行Cargo，未新建/复制target。Clippy首次测试clone写法告警已修正；文档直接测试
首次只读sandbox不能创建临时fixture，取得正常写权限后41通过，两者均不算能力红测。
未运行Phase4、SSA/LLVM/native、runtime释放计数、Linux、sanitizer或正式双宿主CI。
本地完成owner首片实现及上述验收，不宣称整个D10/E2完成；loan并行片未纳入此树，动态
capture未启动。无push、PR、main合并；§170与parent-binding receiver原脏树不动。

### 原样 B 的既有 SSA/LLVM 兼容证据（2026-11-10）

独立测试提交 `04f12cde479c1b1f365ee1abb1d29cacdbc7e6f3` 基于 owner 完成头 `7f854095`，隔离分支
`feature/spec-0341-owner-b-downstream-20261110` 保留真实父链。仅新增测试及注册，
不修改生产代码、不扩 Phase 4 合同。原样 B 函数逐字保留，外围 entry 调用 false/true；
严格 UnitSource helper 经真实 final admission、Unit ownership、sealed view 与现有
lowering，不从 synthetic facts、Single fallback 或 CI-only 故障 helper 获取结果。

| 验收项 | 精确目标/命令 | 实际结果 |
|---|---|---|
| 原样 B 下游结构 | `lang-codegen --lib` exact `native_tests::boxed_enum_tests::unit_source::owner_replace_break_downstream_tests::bounded_replace_break_original_source_reaches_existing_ssa_and_llvm` | 1普通通过；P true/false转交、唯一RootReplace、Q与false P同一exit参数、detached旧值body Drop及selected尾句Drop，原source span逐项核对；SSA及LLVM verifier通过 |
| 严格静态质量 | `cargo clippy --locked --offline -p lang-codegen --all-targets -- -D warnings` | 通过 |
| 格式、文档、尺寸 | fmt、docs、size `--base 7f854095`、diff | 通过；docs878，2265手写Rust，36历史超长，新测试196行 |
| 独立复审 | 原源码、helper调用链、结构oracle和声明边界 | 只读复审无阻断；核实际1 passed与Clippy日志，未重复执行Cargo |

Mac AArch64，Rust1.96/LLVM21.1.8；locked/offline、jobs2，复用原 owner target，
执行前确认无Cargo/rustc活动并与并行worker分配不同target；全部Cargo串行。
环境无profile/RUSTFLAGS覆盖，实际frontend及codegen test rustc均核得
`-C debuginfo=2`。测试与所有调用helper逐源检查，无Abort、破损、故障注入或压力运行。

首次测试调试打印误用不实现Debug的Program而编译失败；修正后首次exact路径误用
源文件名而非真实注册模块，运行0 tests。两者日志保留且不计能力红测/验收；以binary
`--list`确认上表完整路径后实际运行1测试通过，再补强结构断言重新运行1测试通过。
本次没有生产行为变化，因此没有伪造“能力红→绿”。日志留仓外
`/private/tmp/koven-owner-b-downstream-evidence`供本地复核，不作为持久验收真源；
可重复证据以提交内fixture、完整测试名与结构断言为准。

新 B getter 仍无production codegen消费者；本测试只证明既有下游兼容性，不能称新
carrier已接线。未运行native、释放计数、Linux、sanitizer、CI-only或正式双宿主CI；
未重新运行原117前端测试、workspace check（无公开API/生产变化）。不退休旧oracle，
D10/E2仍in-progress，无push/PR/main合并，其他脏树不动。
### ADR-0040 loan 实际动作源第一增量（2026-11-10）

在 main `3fe1920a` 上独立建立 `feature/spec-0341-loan-action-coverage`，快进接受合同
`23d70006`，保留父链。红测提交 `781b66b1` 证明普通 String temporary direct call
已有实际 End/Drop，却没有 acquisition 记录：预期 Begin=1、实际0，1项失败。
实现及最终代码输入 `65ddd0f3914a2f729102b0fa9f78c4a3a5260b7f` 在真实已完成实参的
pending 注册点追加 Begin；同一 `ActionSource` 保存原 cleanup，旧投影只读 Cleanup，
旧清理下标/顺序不变，不排序旧表，不重推 loan 或增加第二次执行。

本增量只发布 source 范围的 Partial/Unsupported 观察。静态 record 具有封闭 catalog ID，
支持同产品 clone、拒绝重分析/外来 catalog，并保留原 call/argument/target 身份。
SourceUnitId 相同下标不能证明同一分析；独立 SourceMap 的 Span 也不保证结构相等。
开发中这两个错误测试假设分别实际失败后修正，catalog 拒绝负控保留，未放宽生产判断。
receiver/result 既有观察继续保留，但 activation、handoff、权限恢复、callable/path/动态
activation 身份仍未统一认证。空表为 Unsupported，不发布 Complete 或 Complete-empty。
这是已批准首片的基础增量，不是首片完整覆盖完成；D10/E2 与本 Spec 仍 in-progress。

未重做或合并 `d6a21333`、`06d72a7b`、`103ed130` 及四个 result 候选；此步只需主干已支持
普通直线 direct temporary。后续登记区域/动作家族与独立覆盖认证器时仍须保留父链合批、
按精确组合头复验，不能把这些候选的旧测试当成本分支结果。既有脏工作树未改。

| 验收项 | 最终代码输入上的实际结果 |
|---|---|
| direct/receiver/result 既有邻控及新增实际源 | `cargo test --locked --offline -p lang-frontend --test unit_direct_call_loan_ends -j 2` 加下列六个既定 `--skip`；300 passed、6 filtered、0 failed/ignored；其中293既有＋7新增，不累计重复运行 |
| 新增能力 | 双 rooted/memory 输入的 Begin→End→Drop、Unsupported-empty、result/receiver仍Partial、同源独立实参、嵌套顺序、later Return未求值后缀、重分析catalog；ID公开可命名、clone及越界亦有断言 |
| raw/public原子门与observer | `cargo test --locked --offline -p lang-frontend --lib loan_action_ -j 2`；2 passed、547 filtered；真实非空合法前缀后独立source错误/deferred清空raw及公开新表；observer开关逐项比较完整旧Analysis |
| 静态门禁 | frontend `--all-targets` Clippy含 `-W clippy::all -D warnings`、workspace `--all-targets` check、workspace fmt与diff均通过；新增测试修正后重跑相关direct/lib及静态门禁 |
| 文档与尺寸 | `check_docs.py`通过877 Markdown；`check_rust_sizes.py --base 3fe1920a`通过，36项既有超限额度不变；独审确认旧超限文件仅增加注册、传递、撤销和开关接线 |
| 独立只读复审 | 单一源旧索引、真实Begin位置、raw/public撤销、observer全产物差分、封闭catalog与增长边界无阻断；复审未代跑Cargo，不认领Complete或退休 |

六个 direct 过滤项保持：`nonreturning_suffix_and_unreachable_calls_publish_no_cleanup`、
`constructed_borrow_aborts_without_unwinding_and_ends_on_the_normal_path`、
`abort_does_not_publish_receiver_prefix_ends`、
`copied_nullable_field_keeps_the_temporary_parent_on_both_call_edges`、
`abort_does_not_unwind_the_temporary_field_parent`、
`abort_and_normal_return_do_not_invent_prefix_unwind_events`。原测试与正式CI选择不改。

**额外实际运行及选择偏差**：此前误将 `ownership_iteration` 完整目标作为普通本机邻控，
实际218项通过，而非事前按旧账本估计的36项。逐源码复核发现其中4项包含本任务约定
仅CI运行的Abort源码：`control_exits::aborting_while_condition_has_no_iteration_backedge_cleanup`、
`control_exits::source_evaluation_exit_never_creates_a_provider_owner`、
`origin_fixed_point::loop_origin_fixed_point_keeps_jump_edges_and_unreachable_tail_separate`、
`source_loans::source_index_exit_cleans_only_the_evaluated_backing_owner`。这是测试选择偏差，
不将218项计为本次合规门禁或并入上述302项；没有隐去实际执行，也未再运行这些项。
后续iteration选择须逐源码列出普通exact清单，不能只依赖测试名或目标分类。

本机macOS arm64、Rust1.96.0、LLVM21.1.8，locked/offline、jobs=2，复用已有借用target
并串行Cargo。红测、最终direct/lib、上述额外iteration、Clippy与workspace check实际均设置
`CARGO_TARGET_DIR=/private/tmp/koven-exclusive-loop-prefix-target`、
`CARGO_PROFILE_DEV_DEBUG=0`、`CARGO_PROFILE_TEST_DEBUG=0`。执行iteration与其后新增测试、
Clippy/workspace check的批次，以及最终direct/lib和静态重验批次，另显式设置
`LLVM_SYS_211_PREFIX=/opt/homebrew/opt/llvm@21`；更早的frontend红测/定向测试没有在命令中
显式设置该prefix，不能据此推定其继承值。

**配置选择偏差**：debug=0是本轮主动指定的非默认调试信息档位，不是已核实的既有target
或环境固定配置。没有核对原候选默认debug=2档位；上述成功结果仅证明实际debug=0输入，
不能称为默认配置验收，默认配置验收尚未完成。保留全部真实结果，不因补记而重跑Rust。
阶段四行为、SSA/native执行、释放计数、Linux、破损模型/注入、大型守卫与正式双宿主CI
未运行。没有推送、PR、合并或Single oracle退休。
后续具体步骤是给普通直线call登记区域与家族，再对真实Begin/continuation/cleanup义务
建立独立覆盖认证；不得凭本增量非空或空动作表把Partial/Unsupported改成Complete。

### Loan 默认配置基线复验与选择纠正（2026-11-10）

以干净 `fbc0f3b8759132e12047d18caa952ba4860019f3` 为确切输入，逐源码及helper审计
`unit_direct_call_loan_ends`全部306项，保守排除10项后固定296个完整exact名；两个
`--lib loan_action_`项逐helper核为真实分析而非注入，保留2个完整exact名。
相较前节六项排除，新增排除 `receiver_return::abort_after_completed_inner_receiver_does_not_invent_outer_return`
（error/Abort源码），以及各生成32份函数/绑定的
`borrow_binding_transport::repeated_local_binding_names_select_only_their_own_call_and_token`、
`borrow_forward_transport::repeated_wrappers_keep_their_own_forwarded_call_and_return_edge`、
`receiver_prefix::repeated_receiver_names_keep_each_functions_root_identity`（按压力保守排除）。
因此前节300项也包含此次新识别的受限选择；保留其实际执行事实，但不再将300＋2称为
全部符合本地选择约定的结果。此前iteration四项Abort及debug=0偏差均不撤销或隐去。

未设置任何`CARGO_PROFILE_*`、`RUSTFLAGS`或`CARGO_ENCODED_RUSTFLAGS`；核当前环境、
仓库与祖先Cargo配置无profile/debug覆盖。仅以
`CARGO_TARGET_DIR=/private/tmp/koven-exclusive-loop-prefix-target`复用原target，执行
`cargo test --locked --offline -p lang-frontend --test unit_direct_call_loan_ends --lib --no-run --message-format=json -vv -j 2`。
两个真实测试artifact的JSON均为`profile.debuginfo=2`，实际rustc命令含`-C debuginfo=2`，
不是只凭Cargo摘要推定默认档位。无target复制，未重跑iteration或十项排除。

逐源审计35文件的路径/字节摘要为
`2bc9765852050cbc6230b3f7ecb45388c8e47863b2e8fff5ba7187ad2d781c56`，运行前严格匹配。
同次编译的两个二进制先`--exact --list`核名称集合，再以全部白名单完整名一次执行；
实际分别296与2项通过、0失败/忽略，成功名称集合逐项一致，不累计前轮结果。
direct二进制SHA256为`47312d5dcbdddfd72c5e65ad1e5c7766c3faa69c2c5b97cf2c53ca470aef5fe1`，
lib为`09b51741ccab62281773d0ccf560c041542b8be7834aaf948c6d6ceb08b24a55`。
这是默认配置下有界行为基线复验；Complete产品尚未实施，后续实现须以其精确输入重新
验收。前轮默认静态/跨crate门禁缺口仍须补齐，不能将旧debug=0静态结果改标为debug=2。

### Loan 普通 direct call 正常返回区域认证（2026-11-10）

延续同一分支和默认配置基线，不合并其他候选或改动既有脏树。能力红测提交
`96a54527`：真实进入的普通String temporary调用只获Partial，期望区域Complete，
实际1项失败。实现代码输入`613c2aa8394a8824c37d37554bba9224ea14e659`接入独立validator，
同一红测转绿；最终验收覆盖下列完整普通选择，不累计中间单项重跑。

区域protocol 1只覆盖**调用者侧正常返回边**的Shared实参建立/结束及对应String临时
owner Drop，不证明callee动态必然返回、callee内部/Abort路径、整个callable或source。
首批准入限定顶层普通函数的独立call语句（整body只有call语句）或单call表达式body，
callee为普通Name/Declaration、Owned Int/Unit结果、无泛型/receiver，全部实参是无插值
String字面量直接映射到String Borrow参数，也允许零实参。命名参数按源码求值顺序保留；
命名变量/Group、nullable转换、控制、nested call及既有pending/provider/capture义务不准入。

候选仅在真实通用Call入口登记，以全混合record游标定位；所有call尾部清理结束后才记录
静态正常完成。validator不筛选/排序区间，严格核全部Begin、逆序End、逆序Drop与唯一
call/argument身份、同CallReturn点及独立AST字面量span；额外/缺失/错序均不能Complete。
按call一次索引checker事实，避免逐call全量扫描。零参数须有真实planner入口/正常后继、
AST/typed零参数与空区间，才可Complete-empty；没有调用不能凭空造空证书。

`owns_region`以catalog身份核验非空和零动作证书，clone保留归属，重分析即使source/call
下标相同也被拒绝。source总体仍Partial/Unsupported，专用String/Rc/construction等早返回
可能没有region，缺region不证明absence。未准入region为Unsupported，准入但未完成或
未闭合为Partial。原raw/public全局撤销门同时撤销region；加强后的真实错误/deferred
邻控先证明非空合法前缀已有Complete，再核所有source新观察全部消失。

| 验收项 | 最终代码输入的实际结果 |
|---|---|
| 默认普通direct精确选择 | 同次默认编译二进制，`--exact --list`核名后一次执行303个完整白名单：既有安全296＋新增7；303 passed、0失败/忽略；原10项受限选择继续全部排除 |
| 7项区域普通证据 | 单literal、两命名literal、连续call完整record游标、零参数正面Complete-empty、转换/变量/控制/嵌套/插值/receiver降级、非空catalog、零动作catalog跨重分析/clone；双rooted/memory输入 |
| 普通lib精确选择 | 原2项raw/public全局撤销和observer完整旧Analysis对照＋新增cross-source正反输入顺序/caller与callee parameter lineage，3 passed；不会命中CI-only破损测试 |
| 配置证明 | 无`CARGO_PROFILE_*`、`RUSTFLAGS`或`CARGO_ENCODED_RUSTFLAGS`覆盖；原target、locked/offline、jobs=2；两个实际测试artifact JSON均debuginfo=2，实际rustc含`-C debuginfo=2` |
| 默认静态门禁 | frontend all-targets严格Clippy（`-W clippy::all -D warnings`）、workspace all-targets check均成功，frontend artifact JSON debuginfo=2；fmt、docs877、diff通过 |
| 尺寸与独审 | 相对main `3fe1920a`尺寸通过：2264手写Rust、36项既有超限，额度不变；独审核真实边界、完整序列、独立AST span、零证书归属、撤销和普通测试来源，无阻断 |

最终普通测试共306个唯一成功项；前节默认基线298项单列，不与本次重复加总。两项新增
CI-only破损模型守卫覆盖缺/重End、缺/错序Begin/Drop、错点/AST origin/callable source、
额外动作及零参数缺完成/Unsupported/越界/非空区间，只随默认no-run及all-targets编译，
未本机执行；不能把编译成功当作负控通过。十项旧排除及iteration四项Abort没有重跑。
先前debug=0与选择偏差记录均保留。中间脚本曾因zsh只读`status`变量在测试后退出，
未启动后续lib；已改用普通变量并分别执行，最终列表与成功集合独立核对，不将未启动项计入。

最终35文件测试源码摘要为`847b9e563e21e4c141b0c1315e0be8f3733167347aacd9504ed5320127a24150`。
direct二进制SHA256为`8f931a0d3001cbc00153947abe76e23102b5085d248b873aa0715e4ad349998b`，
lib为`fbbdd7bf8e1d72e184f086b89c318417290f712cd445ef081ea23fc1994f3d98`。默认构建仍为
`cargo test --locked --offline -p lang-frontend --test unit_direct_call_loan_ends --lib --no-run --message-format=json -vv -j 2`，
只设置`CARGO_TARGET_DIR=/private/tmp/koven-exclusive-loop-prefix-target`；Clippy/check另设
`LLVM_SYS_211_PREFIX=/opt/homebrew/opt/llvm@21`，无target复制或并发Cargo。

本片建立有界正常返回区域的Complete能力，不关闭D10/E2或Spec。receiver activation、
借用结果handoff/permission restoration、一般CFG/动态activation及整个callable覆盖仍未完成。
Phase 4行为、SSA/LLVM执行、native释放计数、Linux及正式双宿主CI未运行；未退休Single
oracle，未推送、PR或合并。后续先在精确组合头运行正式CI破损守卫，再按已批准合同扩展
真实continuation/receiver家族，不能把当前局部证书提升成整个调用者的完整生命周期结论。

### Loan Shared receiver 正常返回独立下一片（2026-11-10，实施范围）

继续`8bf3f160`的既有分支与真实动作源，在ADR-0040及已批准普通receiver生命周期范围内
新增有界normal-return证据，不改Guide或默认启用新语言能力。上片protocol 1直线String
实参/零实参合同保留；本片另声明Shared receiver协议，不默默扩大原direct-only合同。

首个候选仅为零显式实参、Owned Int/Unit结果、普通非Resource MoveOnly nominal方法，
receiver是当前顶层普通caller的真实Owned参数根、无投影/转换。复用既有flat调用区域，
从真实pending注册记录Shared receiver Begin，从原正常token extract位置记录End；旧
`receiver_call_return_ends`改为同源只读投影。新的正常receiver End不加入旧iteration
cleanup执行流，不增加或双跑后端执行责任。observer关闭时仍保留旧return投影。

只有从独立源码/typed/Owned binding证明的根，且实际receiver End之后同CallReturn
存在唯一Named root Drop，才能认证这个新区域Complete；不得从End反推owner或权限
恢复，不制造Drop。来源仍live时保留Partial；Borrow/Inout参数、Two-Phase、this、
字段/投影、temporary/Rc、provider/capture、borrowed result、泛型与控制范围不准入。
不认证callee必返、内部/Abort路径或整个source/callable；上片source总体状态保持。

先保存普通`run(own node: Node) { node.touch() }`能力红测，保留旧receiver end、Drop及
identity oracle；再补on/off完整旧产物、非空Complete前缀后独立source错误/deferred全局
raw/public撤销，以及source/catalog/协议边界。仅运行逐源/helper核实普通exact选择，
十项旧排除及任何CI-only故障/Abort/压力/破损一律不跑，新破损负控只编译。
保持默认debug2实际profile核验、原target串行，不复制或降低档位；既有偏差保留。
本节只限定实施范围，尚不表示新receiver区域已实现或验收；独审与最终准确结果另记。

### Loan Shared receiver 本片验收（2026-11-10）

范围提交`a2268506`、能力红测`6e8a6b50`、实现与普通回归提交`7d1345ba`，均续接
`8bf3f160`，没有重做前片或合并其他候选。首个红测先核已有receiver正常End与Named
Drop真实存在，再因区域Unsupported失败；实现后同一能力测试转绿。方法typed target
实际为Symbol，准入按真实target修正，protocol 1的Declaration/无receiver门未放宽。

本片protocol 2实现事实见[动作源专题](../../architecture/unit-loan-action-source.md)。
Shared Begin与正常End都来自真实planner位置，旧return表同源投影保留原筛选家族和顺序，
observer off仍保留旧表。正常End不加入旧iteration执行流或旧prefix计数。当前caller的
显式Own参数、Owned binding和实际ValueState共同证明owner；独立参数形成span与
receiver使用span分开。只有完整Begin→End→同CallReturn Named Drop三项闭合才Complete。
同根连续两次调用首项Partial、末项Complete，两Owned根保持各自Drop与形成origin。

最终唯一普通成功项315（309 direct＋6 lib），不是与前节298基线或306结果累加：

- direct保留前片303项精确白名单，逐源核新增6项`action_source::`测试：
  `receiver_region_owned_parameter_requires_actual_begin_end_and_drop`、
  `receiver_region_live_root_is_partial_until_actual_last_use_drop`、
  `receiver_region_two_owned_roots_keep_distinct_formation_origins`、
  `receiver_region_unadmitted_roots_and_reservations_are_not_complete`、
  `receiver_borrow_result_keeps_existing_rejection_and_no_certificate`、
  `receiver_region_record_and_certificate_identity_cannot_cross_reanalysis`。
  最终309 passed、0 failed、10 filtered；十项旧排除不变，所有源/helper均为普通有限输入。
- lib保留前片3项普通exact，新增owner_transport测试
  `loan_receiver_observer_preserves_nonempty_legacy_return_projection`、adjacent_borrow_results测试
  `loan_receiver_complete_prefix_is_revoked_from_raw_and_public_products`、loan_region_validation测试
  `loan_receiver_cross_source_retains_declaration_and_caller_identity`。
  最终6 passed、0 failed、550 filtered；先`--exact --list --format terse`核名单集合，
  再`--exact`运行并逐项核成功集合。新增helper核完整Analysis on/off及独立source
  错误/deferred后的raw/public撤销，不只隐藏公开字段。
- Resource、Borrow/Inout参数、Group、temporary、字段、This及Two-Phase普通对照不认证
  Complete。borrowed-result仍报两条原L0164，精确message、source与primary span分别
  25..31、88..99，所有新观察及旧receiver return表撤销。初轮将该例误放入admitted数组，
  得到307通过/1失败，lib未启动；这是测试预期错误，未改语言能力，拆出真实拒绝断言后
  在最终输入重核完整315项，不把初轮或补强前重复成功累计。
- 三项CI-only破损守卫只随no-run、all-targets编译，未本机执行。新增receiver守卫覆盖
  End-before-Begin、重复End、缺Drop、错owner origin、缺完成、缺caller及未准入；
  不能把编译通过说成负控已通过。旧四项iteration Abort和十项direct排除均未重跑。
- 严格frontend all-targets Clippy、workspace all-targets check通过；精确诊断补强后分别
  0.81s、0.66s，前轮相应34.32s、28.94s。fmt、docs 877、尺寸2265 Rust/36项旧超限、
  diff检查通过。独立只读复审核同源旧表/observer off、参数owner事实、完整区间与
  catalog/全局撤销无阻断项；drop_planner.rs仍1583行，未提高旧额度。

环境为macOS arm64、Rust1.96.0；实际test compiler-artifact profile.debuginfo为2，
rustc命令含`-C debuginfo=2`，没有设置`CARGO_PROFILE_DEV_DEBUG`、
`CARGO_PROFILE_TEST_DEBUG`、`RUSTFLAGS`或`CARGO_ENCODED_RUSTFLAGS`。
全程复用`CARGO_TARGET_DIR=/private/tmp/koven-exclusive-loop-prefix-target`并串行Cargo；
无复制target、clean或降低debug档位。默认构建命令为
`cargo test --locked --offline -p lang-frontend --test unit_direct_call_loan_ends --lib --no-run --message-format=json -vv -j 2`。
Clippy/check另设`LLVM_SYS_211_PREFIX=/opt/homebrew/opt/llvm@21`，分别执行
`cargo clippy --locked --offline -p lang-frontend --all-targets -j 2 -- -D warnings`及
`cargo check --locked --offline --workspace --all-targets -j 2`；LLVM前缀不用于frontend行为执行。

最终35文件direct测试源码摘要（排序后的逐文件SHA256清单再SHA256）为
`ea4c2e2ff4c8b51e5b3f111c37bc554db09765564827ec038923bafcda2d4b78`；
direct名单摘要为`49b4506951069367f1860faac8fb758f176c7223dda37d85672581dd15812847`，
lib名单摘要为`93f068a56dcc316e04fc92abb5202cc2436fb47af47d5e3341abec030e451e33`。
最终direct二进制SHA256为`dfc2b4152d6ab20d99c330b7d46bc3c067ff82c03dddaf40ddff1e1cc19d6f13`，
lib为`a39cc89dbcfd9d055ef909cc4463f22915c143b1dbca44fe3cb7a2584d67b940`；日志和完整清单
留仓库外临时证据，不将原始dump入库。既有debug0与选择偏差账本不变。

本片未认证一般permission restoration、结果handoff、Two-Phase activation、Rc/provider、
一般CFG、callee动态必返或整个source/callable；source仍Partial/Unsupported。未运行
Phase 4、SSA/LLVM行为、native计数、Linux或正式双宿主CI，Single oracle不退休。
Spec与D10/E2继续in-progress；本片局部Complete不得替代整体覆盖证明。未推送、PR或合并。

### Loan 单Named参数源直线区域下一片（2026-11-10，实施范围）

续接`71b85631`，在ADR-0040普通借用生命周期范围内另声明protocol 3；不改protocol 1
字面量或protocol 2 receiver合同，也不接owner Phase 4。首候选仅为flat顶层caller中
普通Name/Declaration direct call、无receiver/泛型、Owned Int/Unit结果，唯一实参为
本caller显式Own String参数的直接Name，唯一callee参数为同类型Shared Borrow。

实际acquisition、正常EndCallLoan与Named Drop已在统一动作源，本片只补独立准入与
完整区间验证，不复制旧End表或新增执行动作。参数Own marker、Owned binding、真实
ValueState及形成span共同证明根来源；typed/contract/loan/place与使用span须一致。
真实Begin→EndCallLoan→同CallReturn Named Drop闭合才Complete；根后续仍live则
Partial。Borrow/Inout或local根、Group/投影/转换、多参数、borrowed result、嵌套/控制、
receiver/Rc/provider不准入；不从End推断通用权限恢复，不宣称source/callable Complete。

先保留现有真实End/Drop oracle的能力红测，再做最小认证与普通安全exact回归、完整on/off、
跨source/catalog与raw/public失败撤销；新破损守卫只编译。默认debug2、既有target串行、
十项旧排除与全部CI-only/Abort/压力边界不变。`33938ade`是独立codegen carrier清理，
本片不修改其树或依赖其未合并内容。此节仅限定范围，尚不表示protocol 3已完成。

### Loan 单Named参数源本片验收（2026-11-10）

范围`61f53a73`、能力红测`d437d67e`、实现`5837ba00`连续提交，复用`71b85631`，
不依赖或修改`33938ade`的codegen树。红测保留已有真实EndCallLoan及Named Drop断言，
在region Unsupported对Complete处实际失败（1 failed、319 filtered），最小实现后转绿。
初次no-run缺测试module path时未执行测试，修正接线后才取得上述真实红测。

protocol 3现从独立当前caller Own String参数、Owned binding、ValueState与形成span
证明Named源，不重推owner/权限，不增加执行动作。独审要求的typed/contract/operand
Place category与argument/loan_begin/call/end Span一致性已补齐后重新编译和运行最终选择。
真实Begin→EndCallLoan→同CallReturn Named Drop完整三项闭合才认证；连续调用首项
Partial、末项Complete，双根的Drop origin不混淆。原protocol 1/2和全部旧投影不变。

本片最终324个唯一普通成功项（315 direct＋9 lib），包含上片315而不重复累加：

- direct沿用上片309精确项，新增`named_action_region::`的6项：
  `named_owned_argument_requires_actual_closed_region`、
  `named_root_stays_partial_until_its_actual_last_call_drop`、
  `two_named_roots_keep_their_own_drop_formation_origins`、
  `named_protocol_does_not_admit_other_sources_or_call_shapes`、
  `named_certificate_and_records_do_not_cross_reanalysis`、
  `named_unit_result_preserves_the_same_closed_obligations`。
  命名实参、Int/Unit、双根和live根均用真实普通有限源码；最后315 passed、0 failed、10 filtered。
- lib沿用上片6项，新增owner_transport测试
  `loan_named_observer_preserves_all_existing_planner_products`、
  adjacent_borrow_results::named_loan_region测试
  `loan_named_complete_prefix_is_revoked_from_raw_and_public_products`、loan_region_validation测试
  `loan_named_cross_source_keeps_parameter_and_caller_identity`。
  最后9 passed、0 failed、551 filtered；on/off核完整Analysis，跨source普通错误/deferred
  在真实Complete前缀之后核raw/public全撤销，非人为注入。
- 所有选择先逐源/helper核普通边界，再用`--exact --list --format terse`核集合、`--exact`
  执行并比对成功集合。十项旧direct排除及四项旧iteration Abort未执行。四项CI-only
  破损守卫仅编译；新增守卫覆盖缺/多/错序动作、错root/owner origin/call span/call identity、
  缺完成、越界及未准入，未把编译成功算成负控执行通过。
- frontend all-targets严格Clippy通过（24.02s），workspace all-targets check通过（20.52s）；
  fmt、docs 877、尺寸2268 Rust/36项旧超限与diff通过。独立只读复审无阻断项，
  drop_planner.rs仅新增模块接线1583→1584行，未提高旧1669额度。

默认配置继续为macOS arm64、Rust1.96.0，实际两个test artifact的profile.debuginfo均为2，
对应rustc含`-C debuginfo=2`。无`CARGO_PROFILE_DEV_DEBUG`、`CARGO_PROFILE_TEST_DEBUG`、
`RUSTFLAGS`或`CARGO_ENCODED_RUSTFLAGS`覆盖。只设置
`CARGO_TARGET_DIR=/private/tmp/koven-exclusive-loop-prefix-target`，串行复用，不复制/clean/降档；
构建命令为`cargo test --locked --offline -p lang-frontend --test unit_direct_call_loan_ends --lib --no-run --message-format=json -vv -j 2`。
Clippy/check另设`LLVM_SYS_211_PREFIX=/opt/homebrew/opt/llvm@21`，命令仍分别为
`cargo clippy --locked --offline -p lang-frontend --all-targets -j 2 -- -D warnings`与
`cargo check --locked --offline --workspace --all-targets -j 2`，不执行LLVM或native行为测试。

36文件direct测试源码清单摘要为`6e1d6d94b117b2412eabd54ee67353766fbeeae5b5c4ec0a5a8678717a7b51f0`；
315项direct名单摘要`944a7073ced03915544cb33fe2890f69d423c7747e15ad81215abbe0ab2bc9b7`，
9项lib名单摘要`8e9d4e0b734569c39b19ba0ff13a79bc07edd727874bb8d0df227e762c0102d3`。
实际direct二进制SHA256为`a0032bd72d09704a41e0037a003d74765333be65c42ad785049d31ade2830dcd`，
lib为`6429b6bc129b845c1ff7dfd9380ce2a0b765922f42232d05ba9bf3c73e3da87a`；摘要方法与上片一致，
原始日志/名单留仓库外临时证据。既有debug0、错误选择和各次真实数量记录不变。

source/callable仍Partial/Unsupported；一般权限恢复、borrowed-result交接、多参数/一般CFG、
Rc/provider与动态activation尚未认证。未接owner Phase4，未执行SSA/LLVM行为、native、
Linux/双宿主正式CI；不退休Single oracle，不关闭Spec或D10/E2。未推送、PR或合并。

### Loan 结果binding交接与终止同源增量（2026-11-10，实施范围）

续接`fad15eab`，按ADR-0040推进source→result关系，不再新增同构Complete协议。
将既有`commit_borrow_call_handoff`在初始化实际继续时的提交记录，以及已初始化borrow
binding从真实状态移除后的End记录，迁入同一ActionSource。旧handoff和borrow_ends
成为同源投影，observer off仍保留它们；pending候选、失败/放弃initializer不发布提交。

handoff保留原真实pending source loan与binding映射，不将来源当End，不推造新parent身份。
其历史CallReturn/辅助loan描述不代表辅助loan在binding提交时结束，不能由复合旧fact
倒放辅助动作或排序旧表。新binding End仅记录实际退休，旧表维持首次出现去重，原始
动作记录不按binding/point相等合并；static记录仍不是动态激活或跨CFG执行轨迹。
新记录不进入旧iteration Cleanup或计数，不增加后端执行责任、不重排原child→parent→Drop。

目标是让已有Begin、真实handoff commit、binding End与owner Drop可在同一catalog中
区分来源延续和终止，先只发布Partial/Unsupported，不给borrowed-result区域Complete。
完整callee return来源映射、parent依赖认证、一般权限恢复、auxiliary retirement独立顺序、
wrapper/consumer/provider/capture覆盖仍待后片；现有protocol1/2/3合同不变。

先用普通成功binding源码核旧handoff/End/Drop存在的能力红测，再补放弃initializer、
同源兄弟/parent-child、历史辅助描述、跨catalog/source、全Analysis on/off与raw/public
错误/deferred撤销。所有选择仍逐源/helper核普通exact；十旧排除及CI-only/Abort/压力
不执行，默认debug2、既有target串行、不复制。与ownerPhase4或0400无依赖。

### Loan 结果binding同源增量验收（2026-11-10）

范围`406f46ae`、能力红测`18a6c1ab`、实现`73d85279`续接`fad15eab`，无候选重复实现或
owner/0400合并。红测先证旧handoff、binding End和owner Drop均存在，再因统一动作源
缺实际commit记录失败（1 failed、325 filtered）；迁移原发布位置后同一测试转绿。

本片不新增协议或Complete认证，只增加`BorrowHandoffCommitted`、`BorrowBindingEnd`
的真实记录。前者仅在原initializer continues提交分支；后者在实际binding移除及原transport
处理后、owner清理前。旧handoff/borrow_ends改同源投影，后者保留首次出现去重，而原始
退休记录不按binding/point合并。observer off仍保存旧表源，新记录不进入Cleanup计数或
执行流。原child→parent→Drop顺序不变；source延续不当End，历史辅助描述不补造退休。

最终333个唯一普通成功项（321 direct＋12 lib），包含前片324，不与前片或补强前重复加总：

- direct保留前片315精确项，新增`result_action_source::`六项：
  `committed_result_and_binding_end_share_the_actual_action_source`、
  `abandoned_initializer_has_no_committed_result_or_binding_end`、
  `child_binding_retires_before_parent_and_actual_owner_drop`、
  `siblings_keep_distinct_handoffs_and_protect_the_shared_source`、
  `historical_auxiliary_description_does_not_replay_an_end_at_commit`、
  `result_action_ids_remain_catalog_bound_across_reanalysis`。
  真实辅助temporary Drop先于commit、parent-child及siblings不误合并、放弃initializer
  保持原Return prefix且无结果提交/End；最终321 passed、0 failed、10 filtered。
- lib保留前片9项，新增owner_transport测试
  `result_action_observer_preserves_nonempty_handoff_and_end_products`、
  adjacent_borrow_results::result_action_source测试
  `result_action_prefix_is_revoked_from_raw_and_public_products`、action_source测试
  `result_action_cross_source_preserves_caller_and_callee_lineage`。
  最终12 passed、0 failed、552 filtered；原始/公开新动作与旧handoff/End在真实独立source
  错误/deferred后全部撤销，公开`borrow_results().ends()`也已显式核空。
- 每项及helper均逐源核普通有限输入，先`--exact --list --format terse`核白名单，再
  `--exact`运行并逐项核成功集合。十项旧direct排除和四项旧iteration Abort未执行。
  本线五项CI-only守卫仅编译；新增人工重复记录守卫核原始双record不同ID、旧投影单项
  及observer off，不计作本机执行通过，未跑故障/破损/压力集合。
- 最终补强输入的严格frontend all-targets Clippy通过（24.79s），workspace all-targets
  check通过（19.88s）；fmt、docs 877、尺寸2271 Rust/36旧超限与diff通过。
  独立只读复审无阻断；drop_planner.rs移除旧向量后1584→1577，未提高历史尺寸额度。

仍为macOS arm64/Rust1.96.0默认配置，两个实际test artifact的profile.debuginfo均为2，
rustc含`-C debuginfo=2`，无`CARGO_PROFILE_DEV_DEBUG`、`CARGO_PROFILE_TEST_DEBUG`、
`RUSTFLAGS`或`CARGO_ENCODED_RUSTFLAGS`覆盖。复用
`CARGO_TARGET_DIR=/private/tmp/koven-exclusive-loop-prefix-target`并串行Cargo，不复制/clean/降档。
实际构建为`cargo test --locked --offline -p lang-frontend --test unit_direct_call_loan_ends --lib --no-run --message-format=json -vv -j 2`。
静态Clippy/check另设`LLVM_SYS_211_PREFIX=/opt/homebrew/opt/llvm@21`，分别运行
`cargo clippy --locked --offline -p lang-frontend --all-targets -j 2 -- -D warnings`和
`cargo check --locked --offline --workspace --all-targets -j 2`，不执行LLVM/native行为。

37文件direct测试源码清单摘要为`b25d587cb3da5c04c676b58cfe9f51ad659ebbf134565bdb2602cadf9b0b7009`；
321项direct名单摘要`5ff03cbd7bcf2f73d5dd4ca098ec71811dde7a256cf967c733b7f806a27f6cf9`，
12项lib名单摘要`e641cb6bfa9c7c46c3bd6fa29d9f48afd25f1e409e5dc8f4ae7464549ad5be94`。
实际direct二进制SHA256为`dd9b1859610e8d303c4b9216936baadba8365899a8f6a1e6a29eb46ef4fdf303`，
lib为`6961b5c37c864103e83db7520699c42502f848366097cb49ca4df1e49b7432d7`。
摘要方法沿用前片，原始日志/名单留仓库外；所有既有debug0与选择偏差记录保持。

本片仍Partial，不认证完整callee返回映射、动态parent/权限恢复、auxiliary独立退休顺序、
wrapper/consumer/provider/capture或整个source/callable。没有接ownerPhase4；未来与owner
合批必须另核owner×loan on/off四组合和跨source全局撤销联合证据，准入若触及replace_consumer
须明确拒绝或处理，不把当前形状隔离当永久保证。本片未做该组合验收，未运行正式CI、
SSA/LLVM行为或native计数，不退休Single oracle、不关闭Spec/D10/E2；未推送、PR或合并。

### Owner 与 loan 固定头本地组合（2026-11-10）

基于 main `3fe1920a` 建立隔离 `feature/spec-0341-owner-loan-combination-20261110`。
普通 merge `c125939d` 纳入 owner `285ac9f8`，再以 `e2addc52` 纳入固定 loan `91649628`，
保留共同合同 `23d70006` 与两个真实父链；未 cherry-pick 重造合同。两处追加冲突逐段并集，
其余共享文件逐 diff 核验，保留全部旧验收、选择偏差和未运行记录。未纳入 Phase 4 候选
`3b0c08b4`、loan 后续新片或 SPEC-0400，原工作树和脏改保留。

联合测试提交 `d393d8ad` 首次运行3项：四开关旧产物相等通过，普通独立 source 错误、真实
closure replace deferred 两项分别因 raw owner 表未清、raw B 能力未剥离而失败。这是旧
owner raw 边界既存缺口，不是自动合流新引入的损坏。修复 `66f5b967` 在 dataflow 返回前
复用原 `publish_observations`：错误/无效typed清表，deferred只剥离replace-break、closed-loop、
invariant-loop能力并保留旧诊断性观察；公开构造仍幂等兜底。不改变合法源码准入或执行动作。

最终 Rust 代码输入 `33d9e61832bab336e20c694605bbe66fbffa1061`，树
`e45d3d7bc38cc4b6b535f4c1ec8b3fe965779aad`；格式化测试helper后重新编译并运行下列精确集合。
全部454个唯一普通项通过，0失败/忽略，重复运行不累加；不是frontend/codegen全量验收。

| 验收项 | 精确选择与结果 |
|---|---|
| owner 旧契约 | 原安全116 lib＋`ownership_iteration::loop_replace_break_unit::replace_break_matches_both_public_input_routes` 1项；两项混合损坏仍整项排除 |
| loan 固定片 | 321 direct＋12 lib，沿用91649628最终普通清单；direct仍10 filtered |
| 四开关联合 | lib `drop_planner::owner_transport::tests::owner_loan_combination::four_switches_preserve_b_consumer_and_all_legacy_products` 1项，四loan族×四owner/loan开关，剥观察后完整旧Analysis相等 |
| 全局撤销 red→green | lib `formed_tests::adjacent_borrow_results::owner_loan_combination::{real_error_revokes_owner_and_loan_after_independent_positive_sources,real_deferred_revokes_owner_capabilities_and_loan_after_independent_positive_sources}` 2项；B、loan正例不变，独立第三source触发精确错误/deferred，raw/public按原策略撤销 |
| 原样B既有下游 | codegen lib `native_tests::boxed_enum_tests::unit_source::owner_replace_break_downstream_tests::bounded_replace_break_original_source_reaches_existing_ssa_and_llvm` 1项；仍非新getter生产消费者 |

上述lib简写共同前缀为 `ownership_checking::compilation_unit::dataflow::`。
组合fixture核P/Q incoming与AST限定Take/Bind/Drop；完整原B oracle在独立fixture保留，
不把额外loan的drop混入原恰两条named drop断言。Result仍Partial，不据非空记录升级Complete；
B与flat loan区域当前是兄弟函数形状，未声称同callable混合已准入或形状隔离永久成立。
ActionSource cleanup计数/迭代索引、receiver observer-off、handoff保序和binding首次去重保全。

Mac AArch64、Rust1.96.0、LLVM21.1.8，locked/offline、jobs2，复用既有独占
`/private/tmp/koven-choice-replace-forwarding-target`，串行Cargo、不复制target、不clean。
无DEV/TEST_DEBUG、RUSTFLAGS或CARGO_ENCODED_RUSTFLAGS覆盖，实际4个test artifact
profile.debuginfo均为2，编译命令含`-C debuginfo=2`；LLVM prefix为`/opt/homebrew/opt/llvm@21`。
编译命令分别为frontend `--lib --test unit_direct_call_loan_ends --test ownership_iteration`与
codegen `--lib`，均用`cargo test --locked --offline -p <crate> --no-run --message-format=json -vv -j 2`；
frontend二进制先`--exact --list --format terse`核全部名字；codegen以唯一匹配过滤器列出
完整名后，再`--exact`执行；两者均逐项核成功集合。

独立只读复审定位450个继承选择、84个源码文件并核相关helper/外部常量，再复审3个新测试、
下游helper和7行raw修复；未发现剩余阻断。36个旧超长额度未提高，领域算法仍在独立子模块。
所有CI-only、人工损坏/注入、Abort和压力集合仅编译未运行；未执行native、runtime计数、
Linux、sanitizer或正式双宿主CI，不退休Single oracle、不关闭SPEC/D10/E2。

本轮格式检查曾报告新公开测试helper签名需换行，已修正且重编复验；不计能力红测。
下游第一次编译被会话中断，核无残余进程后续编成功，不宣称中断批次通过。
测试脚本外壳曾因zsh只读变量`status`报错，但三个binary退出和逐项成功集合已落日志；
后续格式化代码头再次完整运行453前端普通项通过，未隐藏外壳错误。
仓外日志、精确名称及binary摘要位于`/private/tmp/koven-owner-loan-combination-evidence`供本地复核；
长期可重现真源为以上提交、测试源码及选择边界，临时路径不作持久证据。未推送、PR或合main。

最终静态门禁：`cargo clippy --locked --offline -p lang-frontend -p lang-codegen --all-targets
-j 2 -- -W clippy::all -D warnings`通过（43.46s）；`cargo check --locked --offline --workspace
--all-targets -j 2`通过（35.29s）；`cargo fmt --all -- --check`通过。文档检查器41项、
Rust尺寸policy47项直接测试通过。最终docs879、尺寸`--base 3fe1920a`（2281手写Rust、
36旧超长）及diff检查通过；未提高尺寸额度，不以继承头结果代替。

### 已批准：B 产品的 Phase 4 单次消费合同（2026-11-10）

**状态：approved／in-progress。** 用户对限定 B 后端接线、新计划验证授权、旧 emitter 唯一发射及双账本核对的请求回复“可以”。本节限定 ADR-0038 B 的下一片接线合同，
不扩大前述已批准 Phase 3 首片，不改变 accepted ADR 或 Guide，不关闭 D10/E2。
只读评估基底为 `285ac9f81f1263c1222def8c3f6ea0a8fc1a436d`；该头的新 getter 在
codegen 仅由 `owner_replace_break_downstream_tests.rs` 读取，没有生产消费者。
上一节已有 SSA/LLVM 成功不构成本节能力红测，也不能证明产品已接线。

#### 范围、入口和责任

建议复用现有 `ssa::refine`、slot→SSA、verifier 与 LLVM，不新增 owner CFG、runtime
registry 或 `unit_lower` 组合特例。范围仍为固定 Boolean 参数、String root、一次
Replace 后裸 Break、唯一 `val retired = held`。一般循环、更多 suffix、Group/Not、
alias、多根、Resource、持续 borrow/capture 均不纳入。Replace 自身已有的瞬时 Inout
loan 必须保留；“无 borrow 首片”不表示删除原语的独占访问及其结束动作。

只从当前 validated owned Unit 获取完整 boundary 与 consumer，精确绑定当前
callable、source、While、condition、body、jump。P/Q 必须以各自 catalog 与完整 opaque
owner identity 解析到已有 producer，不能按裸 index、Span 或符号拼写配对。
`incoming()` 的发布顺序为 condition-false P、Break Q；CFG 的前驱枚举顺序不承诺相同，
必须按边角色和真实端点对应，不能直接 zip。定义身份不变，Choice 不新建 producer。

责任划分如下：

- Phase 3 决定实际来源、边关联、真实 Take/Bind/Drop、cleanup 点及封闭性。
- refine 将已验证身份映射到当前函数实例的 slot/CFG；产品授权后缀读取、绑定与清理，
  每个动作实际兑现一次。AST 只确定求值顺序和 CFG 形状，不补造 ownership 事实。
- 现有 RootReplace 仍先形成 RHS，再写回 root slot 并交付 detached 旧值；SSA 构造
  将 false P 和 Break Q 对应到同一 exit 参数，不 clone/retain 两个候选。
- 不另写 phi 构造或第二套 lowering；LLVM 与 runtime ABI 保持原合同。

#### 准入与单次消费

建议在发射前完成适用性选择，保持 ADR-0032 的 whole-callable 确定性 route。
合法非候选缺少可选 B 产品时仍走既有能力路径，不新增语言拒绝。选中 B consumer 后，
身份不匹配、缺失动作或重复消费必须结构化失败，不得先试新路径、失败再退回旧路径。
不能在产品缺失时重新识别 AST 形状来补造该产品，也不能把所有 String/While 都纳入。

建议最小实现让产品消费计划授权并兑现 Take/Bind/Drop，复用现有 Drop emitter 作为
唯一物理发射器。`Drop(AfterStatement(suffix), retired)` 必须精确对应一个旧 cleanup
obligation，核对 point、target、当前 selected slot；同一次实际发射后，同时记账新动作
与旧 drop index。禁止以下捷径：

- 新计划先发 Drop，随后普通 `drops(point)` 再发一次。
- 只把旧 index 标为 consumed，没有实际发射。
- 仅调用 getter/assert 或事后检查 SSA，全部授权、目标映射和完整性仍独立依赖旧表。
- 按两个 incoming 各发一个后缀 Drop，或用旧 condition P 代替 selected slot。

body 中 detached P 的 `previous` Drop 仍由原 cleanup 路径负责，仅在进入 body 的路径
执行；它不是共同结果 Drop。后缀只清理实际被选择的 P 或 Q。函数结束时必须同时核验：
所有产品动作恰一次兑现、incoming 精确覆盖、选择值只交付一次、新产品与旧 cleanup
ledger 对账。保留 `check_all_drops_consumed`，不为新产品全局跳过旧事实。

现有 consumer 的 Drop action 已带 suffix statement 身份；不能未经需要先增加冗余
公开 getter。具体私有计划类型与索引结构由实施设计决定，不在本候选中预置通用框架。

#### 源码依据与待审选择

以下是上述基底的定位依据，而非新实现声明：

- `ssa/unit_lower/orchestrate.rs` 调用 `refine::route` 后才调用 `refine::lower`；
  `ssa/refine.rs` 定义确定性 route。
- `ssa/refine/build/container_for.rs::while_loop` 构造既有 CFG，false cleanup 与 Break
  各自完成；`build/inout.rs` 发射既有 RootReplace。
- `ssa/refine/build/field.rs::local_variable` 是现有绑定入口；`build.rs` 的 `drops`、
  `collect_drops`、`emit_drops`、`finish_drops` 和 `check_all_drops_consumed` 管理旧账本。
- frontend `owner_transport/loops/replace_break/consumer.rs` 定义 incoming 和三动作，
  `matches_boundary` 校验其与完整边界一致。

本次已批准严格 B 的 Phase 4 接线及“新产品授权和动作完整性、旧 emitter 唯一发射并双账本对账”的责任转移。实施须证明生产消费真实依赖产品，不能仅保留旧成功结果；不改写 accepted 决策，也不扩大 B 准入。

#### 批准后的验收门槛

1. 先增加真实能力红测：证明生产入口尚未兑现新产品消费计划，不能将既有 SSA 成功
   包装为 red→green。普通路径用实际消费见证核对产品动作与 emitter 的唯一对应。
2. 原样 B 的双 Unit 入口、false/true、P/Q 同 index 不同 catalog、同字面量及
   source/重分析身份；核 Take/Bind/Drop 顺序、selected slot 和旧 obligation 对账。
3. 保留上一节原样 SSA/LLVM oracle，并在新精确头重验；旧头绿灯不能移作新接线证据。
   普通 native 功能执行与释放计数分列，无崩溃不能证明无泄漏。
4. 合法非候选保持既有行为；source-error/deferred 不得获得可消费计划。missing、foreign、
   duplicate 产品/动作、重复 Drop、deferred 注入等负控只编译，本地不执行；Abort、
   破损、故障、计数与压力守卫继续交正式 CI。不得复跑前节列明的两个混合破损 exact。
5. 独立复审重点为装饰性 getter 接线、边顺序误配、双 Drop、漏账及错误回退；定向行为、
   共享契约、静态门禁、正式精确头双宿主 CI 分别记账，不退休原 oracle。

原候选提交仅记录源码核实和接线建议，未运行 Cargo、Rust/native、计数或 CI；不重复上一节
测试。文档提交的实际 docs/diff 检查与结果另在交付报告记录。原工作树与未提交改动保留，
不推送、开 PR 或合并；SPEC-0341、D10/E2 继续 in-progress。

### 严格 B Phase 4 本地接线验收（2026-11-10）

用户批准本节限定的后端接线后，从组合冻结头
`2c50c3a0acb9f79e33f9d7728fd82015c8b62a60` 保真实父链接入候选合同
`3b0c08b46e9ad120e71bce7a80fbfaaef6ae96dd`；合并头 `b10eb7e9`，批准状态头
`ef7dada4`。生产代码与最终测试固定于 `567d7af09a97c5deca88665348a20a3669cbc7aa`，
红测头为 `5052a9b6`。未借入后续 loan 未完变更，未修改原脏树或 0400。

私有 `OwnerPlan` 在发射前选择同 Unit/callable 的封闭 B consumer，用各自 catalog 的
完整 owner 身份解析 P/Q producer；While/Break 按角色接实际 CFG 端点，Take 前核恰两入边。
Name 从计划交付 selected slot，Bind 发前授权、Alias 发后记账；旧 Drop emitter 发前取得
Take/Bind receipt，成功物理发射后才消费。receipt 核旧 cleanup index、target、bound slot，
构造时精确核 point。函数完成同时核产品三动作和原 `check_all_drops_consumed`，不跳过旧账。
原 Inout、RootReplace 和 detached P cleanup 保留，无第二个 Drop emitter、AST 造事实或失败回退。

| 验收 | 精确选择与结果 |
|---|---|
| 真实能力红→绿 | `owner_replace_break_downstream_tests::bounded_replace_break_physical_drop_has_product_authorization` 在红测头 1 failed：旧 SSA 仍合法，但实际退休 Drop 缺 Take/Bind receipt；最终同 exact 通过，另核真实 root/Take/Bind/Drop 顺序及槽身份 |
| 本片普通行为 | 同模块五个 `bounded_replace_break_*` exact 各一次，共 5 passed；包括保留的原样 SSA/LLVM oracle、上述 receipt、双 Unit×同字面量×重分析 8 case、合法 Group 条件非候选，以及双 Unit 普通 native false/true；无 ignore/零命中 |
| 共享路径邻控 | `ssa::refine_tests::{nullable_int_while_reassignment_carries_loop_state,inout_scalar_parameters_replace_the_callers_binding}` 与 `native_tests::{dynamic_strings_cross_borrow_value_and_return_boundaries_with_exact_bytes,move_closure_drops_or_invokes_its_owned_string_capture_once}` 四个逐源/helper 审查的普通 exact，4 passed |
| 负控 | `ci_only_bounded_replace_break_rejects_replayed_actions_and_corrupt_receipts` 的重复 Take/Bind/Drop、错 slot/index、漏旧 finish 六类只经 `cargo test --locked -p lang-codegen --lib --no-run` 编译；未执行，不计 passed |
| 静态门禁 | `cargo clippy --locked -p lang-codegen --all-targets -- -D warnings`、`cargo check --locked --workspace --all-targets`、`cargo fmt --all -- --check` 通过 |
| 文档和尺寸 | `python3 scripts/check_docs.py`：879 Markdown；`python3 scripts/check_rust_sizes.py --base 2c50c3a0acb9f79e33f9d7728fd82015c8b62a60`：2283 Rust，36 旧超长；`git diff --check` 通过。build.rs 993，expression.rs 保持 1003，未增加尺寸例外 |

最终行为合计 9 个唯一普通 exact，逐源/helper 审查后只运行正常输入；未运行故障注入、
计数、预期 Abort 或 stress 测试，不从测试名推定安全。
Cargo 串行复用释放后的原 owner target；实际 test artifact profile 为 debug=2，未复制 target、
降档或清理。组合基底的 453 项 frontend 普通行为未重跑，不能算作新头执行；本次没有 frontend
生产或公开 API 改动。原样 B 下游 oracle 已在本次代码输入重验，不移用旧 SSA 绿灯。

本地普通 native 只证明正常功能/输出，不证明无泄漏或释放次数。完整 codegen 行为、正式
macOS/Linux 双宿主 CI、故障负控执行、runtime 计数与 sanitizer 尚未运行，继续由正式 CI 验收。
不扩通用 phi/capture/Resource，不退休 Single/旧 oracle；SPEC-0341、D10/E2 保持 in-progress。
未 push、开 PR 或合并 main。验收不把接线存在或普通成功等同于全部 owner 组合已完成。

固定代码头已完成新上下文独立终审，无阻断：核真实 root/Take/Bind/Drop 序列、selected 与
root 槽一致、按角色绑定 P/Q、唯一物理 Drop 及双账本、非测试构建不含故障注入路径。
六类负控的编译不等于执行，也不声称已覆盖 foreign/missing/deferred 产品或错入边注入。
这些更宽的产品破损组合与正式 CI 仍是未验边界，不因局部终审而关闭 D10/E2。
### Loan 同步consumer实际依赖边下一片（2026-11-10，实施范围）

续接`91649628`，不依赖待确认ownerPhase4或隔离组合头。本片推进实际transfer→retirement
关系，不新增同构Complete协议：在旧source token从pending移入同步PendingTransport的
真实位置记录transfer，并把不透明catalog句柄随该transport携带；正常完成时从实际
source/consumer token退休记录引用该句柄，不按Span或最终描述符列表反向猜父边。

独立核句柄属于同一catalog、指向先前transfer，source token完整一致、consumer call/
argument、Shared模式、root及实际caller SourceUnitId/SourceId一致；callee parameter_span
可跨文件。失配只降级观察引用为None，保留真实退休及旧projection，不改变语言诊断/清理。
公开retirement快照保持结构Eq，getter才产生catalog-bound ActionId，重分析不得解析旧ID。
旧borrow_call_transports改同源保序投影，observer off保留旧表源，不增加iteration执行动作。

观察metadata不参与PendingTransport原语义Eq/合流资格。同一前缀clone共享句柄；语义
相同但句柄不同/缺失的合流只清空观察引用，不能任选分支或影响旧transport是否保留。
有transfer而无retirement可能是consumer未完成，不补End，不认证wrapper/unwind或动态
唯一消费。source/callable仍Partial；callee返回映射与完整权限依赖认证另待证据。

先普通能力红测，再核Group、命名双实参/同根逆序退休、later Return、跨源/重分析/clone、
非空全Analysis on/off与独立source错误/deferred raw/public撤销；人工引用损坏/合流负控
只编译。沿用default debug2、borrow原target串行与全部十旧项/CI-only排除，本地提交。

### Loan consumer依赖边实证（2026-11-10，已实现Partial）

范围提交`1d0b3113`，能力红测`378f4ddc`，实现`a5ce1722`。原真实transport存在而统一
retirement/父transfer引用缺失，首项红测实际失败；最小实现后该项green。最终生产与普通
测试补强输入通过326 direct＋15 lib＝341项，不与前片333或中间执行重复累计。

- direct沿用321项，新增consumer_action_link中的5项：
  `consumer_retirement_references_its_actual_transfer_record`、
  `grouped_and_named_sibling_consumers_keep_actual_distinct_links`、
  `later_return_keeps_existing_rejection_and_publishes_no_dependency`、
  `consumer_link_survives_snapshot_clone_but_not_reanalysis`、
  `nonowning_parameter_source_does_not_gain_transport_coverage`。
  最终326 passed、0 failed、10 filtered。
- lib沿用12项，新增owner_transport中的
  `consumer_dependency_observer_preserves_all_planner_snapshots`、result_action_source中的
  `consumer_dependency_prefix_is_revoked_from_raw_and_public_products`和action_source中的
  `consumer_dependency_cross_source_keeps_external_parameter_spans`。
  最终15 passed、0 failed、554 filtered。原始及公开新观察、旧transport投影均显式撤销。
- 逐源/helper核普通有限输入，exact list集合与白名单相等后运行exact，并核实际成功集合。
  同根命名兄弟按真实逆序退休句柄关联，保留既有固定身份oracle；跨分析SourceId与catalog
  同时核验，跨文件callee parameter_span允许不同来源。clone仍可解析原句柄。
- 中间新增later Return测试错误期待已有准入，完整direct执行得到325 passed、1 failed、
  10 filtered，lib未开始。实际既有L0164拒绝`consume(view(owner), return)`，修正为精确
  message、SourceId及157..168并核新旧两表全空后，才取得上述完整341结果。未开放语言。
- 两个新增CI-only人工负控覆盖损坏句柄/token/source身份及相同/不同/缺失合流句柄，
  加前片共7项只编译不运行。它们靠exact白名单隔离，不是ignore隔离，不运行全lib。
  十旧direct排除、四旧iteration Abort及所有故障/压力/破损集合均未执行。

最终行为构建build4使用默认debug2，两个实际artifact profile.debuginfo=2，rustc含
`-C debuginfo=2`；未设置`CARGO_PROFILE_DEV_DEBUG`、`CARGO_PROFILE_TEST_DEBUG`、
`RUSTFLAGS`、`CARGO_ENCODED_RUSTFLAGS`。复用
`CARGO_TARGET_DIR=/private/tmp/koven-exclusive-loop-prefix-target`，不复制/clean/降档。
命令为`cargo test --locked --offline -p lang-frontend --test unit_direct_call_loan_ends --lib --no-run --message-format=json -vv -j 2`，
然后直接运行已编译binary的exact白名单。断连前no-run39.68s已完成，未重复该基线；
最终补强build4为4.83s。direct binary SHA256：
`b73a58abfa17860aedc2876f57bc65031d4db9babfeb3400ddea4d73a1ca1295`，lib：
`dc88a6c512f88f92036f74c33db3d9694aa68f535e966e32f6e47d3292acc547`。
326项名单SHA256为`75ef8dab5f7060b2d791aa0eb6555dfe24272b42edfd7979eb464822c961add3`，
15项名单为`399c6364bcc2cb59cf6456ab7362af1c2e2c9d20203f604c97b34cfc55636fd3`。

行为绿后Clippy发现仅CI-only断言clone lint，首次修正多一层引用产生E0277，随后改为
`std::slice::from_ref(transaction)`；生产和普通测试未变。最终该CI-only源码经静态编译，
没有冒称新binary行为重验。严格frontend all-targets Clippy通过9.84s，workspace all-targets
check通过32.80s，均在上述target另设`LLVM_SYS_211_PREFIX=/opt/homebrew/opt/llvm@21`，
使用前片所列locked/offline命令串行完成。fmt、docs877、尺寸2275 Rust/36旧超限与diff通过，
未增加历史额度。独立只读复审无阻断。完整日志与白名单保留仓库外，所有此前偏差仍保留。

本片只补真实transfer→retirement依赖，未证明完整acquisition/parent权限链，不认证wrapper、
unwind、动态唯一消费或source/callable Complete；失配引用降为None仍保留原退休和旧表。
未接ownerPhase4，未认领隔离owner组合结果，未跑正式CI/SSA/LLVM行为，不关闭Spec/D10/E2，
未推送、PR或合并。

### Loan acquisition句柄贯通下一片（2026-11-10，实施范围）

续接`d2e9c1e5`，复用同一record catalog及既有transfer→retirement，不另建ID体系或
Complete协议。真实Begin返回观察句柄，随实际pending token移入consumer transfer；
consumer退休同时携带其实际Begin句柄。独立validator验证source Begin先于transfer、
consumer Begin位于transfer与retirement之间，完整token和同catalog归属一致，才公开
source/consumer acquisition引用。禁止按Span或最终facts搜索补回缺失句柄。

pending旧动作/loop深度与合流资格不变。相同前缀clone保留句柄，不同或缺失句柄的合流
只清观察元数据；observer关闭不额外产生Begin，旧projection/清理不变。接入仅覆盖已有
同步consumer的真实token生命周期，不认领wrapper、动态唯一消费、parent权限恢复、
全CFG或整个source/callable Complete。仍Partial，错误/deferred全局撤销边界不变。
先能力红测，再普通exact/独审；既有7项CI-only和十旧项排除，默认debug2与原target串行。

### Loan acquisition句柄实证（2026-11-10，已实现Partial）

范围`d351b9cc`、红测`38a5bd17`、实现`0e7b5c41`。新普通测试
`consumer_action_link::consumer_acquisitions_reference_actual_registered_begin_records`
实际红测0 passed/1 failed/336 filtered，真实缺口为退休token没有acquisition引用；
最小接线后同项green。source Begin→transfer→consumer Begin→retirement引用来自实际
pending token携带的同catalog句柄，独立验证不按最终facts搜索补回。新增引用失效不抹掉
原transfer/transaction，不新增执行Cleanup，不改变旧projection、observer-off或语言准入。

最终精确输入通过327 direct＋15 lib＝342项，不与前片341、单项green或中间342重复累计。
沿用前片326普通direct和15普通lib白名单，仅加上述一项；全部源码/helper逐项审查，
Group、命名同根兄弟、clone/reanalysis与跨文件helper改用公开acquisition引用并核顺序。
原完整on/off、raw/public全局撤销及旧固定身份oracle继续通过。direct为327 passed/
0 failed/10 filtered，lib为15 passed/0 failed/556 filtered；exact list及实际成功集合
均与白名单逐项一致，未执行十旧排除、四旧iteration Abort或其他CI-only集合。

新增两项CI-only负控仅编译：pending合流核同前缀clone、异record/catalog、None、缺项、
depth变化、重复语义候选及三路先清后遇匹配不恢复；acquisition负控核正常及9种失配，
同时保留旧transfer/transaction。累计9项本线CI-only不认领执行通过。重复语义候选clone
也保守清观察，故“clone保留”仅指无歧义前缀；旧末状态pending资格/顺序不变。

补强build2曾因测试helper把retirement写成retired而编译失败，已修；build4普通342绿后，
Clippy要求等价简化validator的否定布尔表达式。修正后最终build5重新精确执行全部342，
不是拿旧binary顶替。build1 no-run43.56s、最终build5 no-run5.08s；不计作多份通过数量。
实际构建仍为前片locked/offline frontend direct＋lib no-run JSON/vv/-j2命令，使用
`CARGO_TARGET_DIR=/private/tmp/koven-exclusive-loop-prefix-target`且串行，不复制/clean/降档。
实际两个artifact profile.debuginfo=2、rustc `-C debuginfo=2`；四个DEV/TEST_DEBUG及
RUSTFLAGS/CARGO_ENCODED_RUSTFLAGS覆盖均未设置。Clippy/check另设
`LLVM_SYS_211_PREFIX=/opt/homebrew/opt/llvm@21`，前片同命令严格Clippy22.05s、workspace
all-targets check31.88s通过。fmt、docs877、尺寸2275 Rust/36旧超限及diff通过，未增加额度。
独立只读复审无生产阻断，其三路负控顺序建议已采纳；不认领未运行的人工负控结果。

最终direct binary SHA256：`7a71fa12b26f204eb7488a14959651123aeaad388b301fad4d82191daca0a8ef`，
lib：`f3b13914c8f8fc47f86caed8abf8d8ff06ce468d26ffc148c8579b9fed188129`。
327名单：`11cf1e86f37cc69644334a646ee5b9f4d14f8900233b3634cf3ab806d882b1d7`，
15名单沿前片`399c6364bcc2cb59cf6456ab7362af1c2e2c9d20203f604c97b34cfc55636fd3`。
原始日志、名单及逐项核对脚本留仓库外，前片全部失败/配置/选择偏差记录仍保留。
本片没有增加Complete协议，不证明wrapper/动态唯一消费/parent权限恢复或整个source/
callable；不接ownerPhase4、不认领固定owner组合验收，不跑正式CI/SSA/LLVM行为，
不关闭Spec/D10/E2，未推送、PR或合并。

### B 后端与 loan acquisition 固定批次组合（2026-11-10）

在上一组合隔离分支快进到 owner `131c770d41671f0b1f62cad0ad3d038a63d4f3fd`，随后
普通 merge `709ab712` 纳入固定 borrow `5f8445c9ee75a39da341104edf75743518bdf1e6`。
保留两真实父链及已批准严格 B Phase 4 合同，不 cherry-pick 重造；不纳入借用后续新片、
SPEC-0400或其他脏树。model冲突同时保留B carrier禁止一般合流与pending acquisition
句柄只降级观察两项职责；测试及历史账本冲突逐段并集，原失败/偏差/未运行记录不删改。
owner后端与上批raw撤销修复相对固定owner头没有生产diff。

联合证据提交及精确Rust输入为 `3512b4222b118d49d7379e0304b5a7d60754bf5c`，树
`72073e473ec5c657d93a8568f348d76ec3299ccd`。仅增强既有3项联合测试，没有新增生产修复，
不把本次兼容性成功伪称能力红→绿。两输入分支原能力红测及上批raw红测记录继续保留。

四开关测试保原B、temporary/named/receiver/result四族，再加入真实`consume(view(owner))`。
loan开启时从公开同catalog句柄解析source Begin→transfer→consumer Begin→retirement，
核完整token及严格顺序、唯一旧transport和Partial；不通过descriptor搜索补回依赖。
20个开关/源码组合剥离owner与loan观察后，比较完整旧Analysis、身份和顺序。
三source用例保持B与loan正例不变，先证明raw/public依赖非空，再由独立普通错误或真实
closure replace deferred触发全局撤销；错误清owner表，deferred保旧诊断观察并剥离能力，
loan观察及旧transport清空。新依赖嵌在原loan观察内，没有漏清的新独立Analysis字段。

本批frontend精确462个唯一普通项通过：134 lib（116安全owner＋15借用＋3联合）、327
`unit_direct_call_loan_ends`、1 `ownership_iteration`。两项owner混合破损、十项direct排除、
四项旧iteration Abort及所有CI-only不执行。逐源码/helper独审覆盖468个继承选择的88个
源码文件，另复审3个联合测试，共471项普通选择；正常native与人工receipt故障严格分开。
默认不运行全lib或全target行为，all-targets仅供静态check/Clippy编译。

macOS AArch64、Rust1.96.0、LLVM21.1.8，locked/offline、jobs2，串行复用原
`/private/tmp/koven-choice-replace-forwarding-target`，不复制target、不clean或降档。
未设置DEV/TEST_DEBUG、RUSTFLAGS或CARGO_ENCODED_RUSTFLAGS覆盖；test artifact逐项核
profile.debuginfo=2，rustc含`-C debuginfo=2`。LLVM prefix为`/opt/homebrew/opt/llvm@21`。
frontend构建选择`--lib --test unit_direct_call_loan_ends --test ownership_iteration`，codegen
选择`--lib`；均使用`cargo test --locked --offline -p <crate> --no-run --message-format=json -vv -j 2`。
各二进制先`--exact --list --format terse`核白名单，再`--exact`运行并逐项核成功集合。

Mac断连发生在462前端通过后的codegen编译期间；恢复时原日志无build-finished success、
没有9项行为日志，且本任务无残余进程，保留中断日志后仅续未完成工作，没有重复462项。
仓外精确名单、binary摘要与日志位于`/private/tmp/koven-owner-loan-combination-batch2-evidence`，
只供本地复核；持久真源为上述提交、测试源码与本段精确选择边界，不把临时文件当永久证据。

恢复后codegen no-run完成（57.71s），同一Rust输入的9个普通exact全部通过（22.57s，
0 failed/ignored，2813 filtered）；因此本批去重总计471 passed，不与上批454或分支各自
验收重复加总。9项完整选择沿用本页“严格 B Phase 4 本地接线验收”：五个普通
`bounded_replace_break_*`（排除`ci_only_*`）、两个nullable/inout refine邻控，以及两个
String/closure普通native邻控。新receipt、唯一旧emitter及双账本、原样SSA/LLVM、双Unit×
同字面量×重分析、合法非候选及普通native false/true均在本批真实组合输入执行。

普通native只证明正常功能与输出，不证明释放计数或无泄漏；本批仍未运行runtime计数、
sanitizer、Linux、正式双宿主CI、完整frontend/codegen行为或任何CI-only/Abort/人工故障/
破损/压力守卫。后者仅编译，不记为passed。没有扩展一般phi/capture/Resource、完整loan
生命周期或source/callable Complete；保留Single及所有旧oracle，Spec与D10/E2仍in-progress。
未push、开PR或合main；这里只形成后续单PR准备的本地候选，不代表正式CI或发布完成。

本批最终静态门禁：`cargo clippy --locked --offline -p lang-frontend -p lang-codegen
--all-targets -j 2 -- -W clippy::all -D warnings`通过（53.31s）；`cargo check --locked --offline
--workspace --all-targets -j 2`通过（40.25s）；`cargo fmt --all -- --check`通过。
`python3 scripts/check_docs.py`通过879 Markdown；`python3 scripts/check_rust_sizes.py
--base 3fe1920a`通过2287手写Rust、36旧超长，未增加尺寸额度；`git diff --check`通过。
独立只读终审核真实父链、model合流职责、旧投影/撤销及471项精确成功集合/binary/profile，
无剩余阻断；文档只追加本批事实，未把历史分支结果冒充本批。仅文档追加后不重复Rust行为。
### Loan 同步结果链normal-return覆盖下一片（2026-11-10，实证范围）

续接`5f8445c9`，目标不再增加身份包装，而是让已有source Begin→transfer→consumer Begin→
真实consumer/source退休关系参与完整区域闭合验证。先实证单层`consume(view(owner))`，
caller为现有flat普通函数，owner为其显式Own String参数，内外普通direct call均单Shared
参数、无receiver/泛型/控制流，内层borrowed String结果来自唯一参数，外层Owned Int/Unit。
语言准入沿既有检查器，不开放borrowed结果或权限新语义。

成功标准为实际entry/completion之间未过滤动作全部核对，两个acquisition句柄和transfer
引用均指向区间内正确记录，实际正常退休后同点Named Drop及独立owner形成来源匹配。
根仍live、额外/缺失/失配动作不能Complete，未准入shape明确Unsupported；整个source/
callable仍Partial。若实际动作或独立typed/ownership来源映射不足，先保留实证与缺口，
不靠旧表拼接、删动作或扩大准入补造Complete。不能把此nested-result合同混入旧direct-only
protocol1/3；如实证可闭合，使用显式不同的结果延续家族声明，复用同一region与validator。
wrapper/export、一般parent权限恢复、callee内部/Abort/unwind、动态唯一消费与owner后端
均非本片目标。普通逐源/helper exact、默认debug2、原target串行及所有CI-only排除不变。

实证收窄：上述Own参数例实际仅两个Begin及Named Drop，旧source transport要求
formed_owned本地形成根，因此缺transfer/retirement，不能Complete；该红测结果保留。
本片不扩大旧transport gate，改用已支持的单个不可变local String字面量形成根，caller仅
此声明与其后普通直线call。参数例固定为未覆盖边界。认证准入须独立核该声明、Owned
binding/formed_owned及真实ValueState origin/declaration；真实结果链其余条件不变。
显式新protocol4只声明同步borrow-result延续及NamedSourceDrop，不修改protocol1/2/3。

### Loan 同步结果链完整区域实证（2026-11-10，protocol4）

范围`0abc80ed`，literal能力红测及收窄合同`445f0c06`，实现`1476cd10`。
首次Own参数样例0 passed/1 failed/337 filtered是范围假设被实证否定：只有两Begin＋Drop，
旧formed_owned门下无真实transfer/retirement；不能算literal实现的能力红测。随后原样
local literal缺证红测独立得到0 passed/1 failed/338 filtered，完整五项实际动作已存在而
区域未认证。断连前该构建已完成4.48s，恢复后先核日志/树/进程，没有盲重跑；其实际binary
SHA256为`2fb291a460d419d6b2515c02725d32ecd4b70fb6e5191b2d44575cc32a48b906`。
接线后对应literal测试1 passed/340 filtered，未以参数样例假阳性冒充闭合证据。

protocol4只认证明确的同步结果延续及NamedSourceDrop，不修改protocol1/2/3。完整未过滤
区间五项和三条catalog-bound绝对引用全部核验，actual Named Drop必须在正常退休后同点。
local能力由不可变声明、formed_owned与唯一仍存的ValueState正面建立；普通local不存在
参数binding descriptor时不伪造它，若已有descriptor则必须Owned。Drop.value_origin实际
匹配name marker，literal formation_span独立核语法和类型。现存replacement/control与所有
pending/transport/capture状态明确排除；本树未含replace_consumer，未来owner合批必须
显式接入该状态拒绝/处理。未接owner后端，未修改旧source transport门或语言能力。

最终确切输入331 direct＋15 lib＝346项普通exact通过，不与前片342或中间执行重复累计。
新增direct四项为consumer_action_link中的：
`owned_parameter_consumer_stays_uncovered_without_real_transport`、
`normal_consumer_closes_declared_source_chain_and_root`、
`live_consumer_root_requires_actual_later_destruction`、
`consumer_coverage_does_not_admit_mutable_or_nonliteral_formation`。
参数/mutable/多声明核protocol4 Unsupported；单nested后接普通消费使根live，前区间四项
无Drop核Partial；唯一literal完整五项核Complete及明确family。live-root未扩大成多个
nested调用准入，Group、一般alias/字段/控制仍未认证。每项及helper逐源核普通有限输入。

复用15项lib，强化原consumer依赖的全planner on/off差分、跨文件来源/region归属及真实
独立source错误/deferred raw/public撤销，先证明新Complete区域真实存在再核全撤销。
最终direct331 passed/0 failed/10 filtered，lib15 passed/0 failed/557 filtered，exact list
和实际成功集合均逐项匹配白名单。新增一项CI-only人工区域损坏守卫核缺/额外动作、调序、
owner origin、foreign catalog、错误acquisition、缺transfer和错Drop point；本线累计10项
CI-only仅编译，未认领执行。十旧direct排除、四旧iteration Abort与所有故障/压力未运行。

中间完整普通选择330 passed/1 failed（lib未开始），因新增concat形成负例错误假定可准入。
实际既有L0164拒绝`val owner="left"+"right"; consume(view(owner))`，最终保精确code/
message/SourceId/152..163与新旧表全空，不放宽原拒绝，修正后才重验上述最终346项。
最终build3 no-run1.54s；实际`cargo test --locked --offline -p lang-frontend --test unit_direct_call_loan_ends --lib --no-run --message-format=json -vv -j 2`，
随后运行已编译binary exact名单。两个artifact profile.debuginfo=2且rustc含`-C debuginfo=2`；
`CARGO_PROFILE_DEV_DEBUG`、`CARGO_PROFILE_TEST_DEBUG`、`RUSTFLAGS`、
`CARGO_ENCODED_RUSTFLAGS`均未设置。复用
`CARGO_TARGET_DIR=/private/tmp/koven-exclusive-loop-prefix-target`，串行、不复制/clean/降档。
严格frontend all-targets Clippy16.09s、workspace all-targets check16.49s通过，沿前片命令
另设`LLVM_SYS_211_PREFIX=/opt/homebrew/opt/llvm@21`，不执行LLVM/native行为。fmt、docs877、
尺寸2277 Rust/36旧超限与diff通过，未增加旧额度；独立只读复审无剩余阻断。

最终direct binary SHA256：`8a8def93d91772186f45e59b9b4160db695df89563e16347c4ff69d182743197`，
lib：`18a30458e43dc3fca648539bf09ceab53b8ad2282be65929a58f50261c446ecd`。
331名单：`ff88610eb76fd23a6b812aa27bab7a79335a48f95fbb5b3704f29d65fd6ada60`，
15名单仍`399c6364bcc2cb59cf6456ab7362af1c2e2c9d20203f604c97b34cfc55636fd3`。
日志/名单/脚本留仓库外，所有旧偏差仍保留。新caller AST准入扫描尚未缓存，未声称压力或
全量性能验收。Complete只适用于此正常返回区域，不证明callee内部/Abort/unwind、wrapper、
动态唯一消费、通用parent权限恢复或整个source/callable；不退休Single oracle，不关闭
Spec/D10/E2，未运行正式CI，未推送、PR或合并。

### 严格 B 与 protocol4 显式状态门组合（2026-11-10）

在隔离组合冻结头`23fb0a2e`上以普通merge `ed191a36`合入固定borrow
`2e9f93770d61ab3525e904496a895f02a862bb40`，保留真实父链；仅本Spec追加冲突逐段并集，
无cherry-pick、历史删除或其他候选合入。borrow后续新片与0400不纳入，不改其原树。

精确Rust输入`d98b0096633831066c6b4092e369b6bad5944f2b`、树
`f5b7376bc2ce0b2eeca332517438c655e4b5d449`。合流后production最小补齐为protocol4的
flat-state门显式要求`state.replace_consumer.is_none()`；其余control/replacement/pending/
transport/capture门不变。内部gate改为`pub(super)`供同planner测试，不开放公共API。
相对上一冻结头，owner后端、真实P/Q carrier、唯一旧emitter与双账本没有production修改。

当前普通语法交集经独审确认为空：严格B为var literal、while replace/break和唯一Name尾句
绑定；protocol4要求首句val literal、后续直接call并排除活动控制帧。补显式状态门是防御合同
补齐，不能冒充普通源码红→绿，也不把当前形状互斥当永久安全保证。
`ci_only_active_b_carrier_rejects_flat_consumer_region_gate`从真实B快照/target人工构造
carrier，核otherwise-flat状态插入拒绝、移除恢复；仅编译未执行，不计普通可达性或passed。
不改变已有语言准入，也不以伪造普通输入制造红测；各分支既有能力红测原记录保留。

原3项联合普通测试强化：四开关的consumer正例先证明唯一protocol4 Complete，使用公开
region全部action_ids核连续未过滤的五项：source Begin、transfer、consumer Begin、retirement、
同CallReturn的根Drop；三条handle精确指向区间位置。两个End由retirement transaction的
`[consumer, source]`保序承载，不增造两个动作。Drop origin匹配实际声明marker，literal
formation span仍独立核准入，source整体保持Partial。五族×四开关仍比较完整旧Analysis。

多source正例保留原mixed Partial consumer，另增独立合格sibling，核两笔旧transport一一
映射、不重复且仅sibling为Complete；raw/public均先证明非空。随后独立普通错误或真实
closure replace deferred撤销全部loan观察、完整region及旧transport，owner保持原错误/
deferred策略。没有把mixed函数放宽为Complete，也没有漏清新增观察字段。

精确frontend普通466项通过：134 lib（116安全owner＋15loan＋3联合）、331 direct和1
iteration。331包含新protocol4四项，参数/可变/多声明Unsupported、live-root Partial和
完整literal区域Complete及已有concat L0164边界均保留。十direct旧排除和两owner混合破损
不运行；新增CI-only门负控未加入原3个exact名单，不用整个owner_loan_combination过滤器执行。
独立逐源码/helper复审前两批与新清单，去重475项普通选择、90个继承源码文件，另核增强
联合模块；旧借用10项CI-only与本次新门负控均仅编译，人工状态不冒称普通验证。

沿用Mac AArch64、Rust1.96.0、LLVM21.1.8、locked/offline/jobs2及原owner独占target，
串行、不复制、不clean、不降档。无DEV/TEST_DEBUG、RUSTFLAGS或CARGO_ENCODED_RUSTFLAGS覆盖，
实际test artifact profile.debuginfo=2，rustc含`-C debuginfo=2`；LLVM prefix不变。
构建及执行命令沿上一批：frontend三target/codegen lib分别no-run JSON/vv，再以精确名单
`--exact --list --format terse`核全部命中后`--exact`执行，逐项对成功集合，不累加重复运行。
本批仓外名单、binary摘要和日志在`/private/tmp/koven-owner-loan-combination-batch3-evidence`，
临时路径仅供本地复核，持久真源为上述提交、测试源码及明确选择边界。

本批最终475个唯一普通exact全部通过，0 failed/ignored：466前端＋9后端。后端9项沿用
前批已逐源审查的五个B普通测试、两个refine邻控和两个String/closure普通native邻控；
本次codegen重新编译28.86s、实际执行5.37s，真实receipt、P/Q槽身份、唯一旧emitter/双账本、
原样SSA/LLVM和双Unit普通native通过，不把旧分支结果移作本批，也不累加重复验收。

最终`cargo clippy --locked --offline -p lang-frontend -p lang-codegen --all-targets -j 2
-- -W clippy::all -D warnings`通过29.15s；`cargo check --locked --offline --workspace
--all-targets -j 2`通过21.47s；fmt、docs879与diff通过。尺寸`--base 3fe1920a`通过，
2289手写Rust、36旧超长，policy及额度未增加。独立只读终审核精确头/tree、完整名单、
逐项成功集合、binary/profile和文档边界，无剩余阻断。仅追加文档后不重复Rust行为。

CI-only carrier门、借用人工损坏及所有Abort/故障/压力守卫未执行，仅编译；普通native
不证明runtime释放计数或无泄漏。Linux、sanitizer、正式双宿主CI、全量行为、整个source/
callable完整覆盖均未验。保留Single和全部旧oracle，Spec/D10/E2不关闭；无push/PR/main
合并，后续单PR仍需正式精确头CI。原main及其他工作树未修改。

### 严格 B 剩余验收准备与入边角色防御（2026-11-10）

按“优先完成动态 owner 和借用线”继续既有严格 B，不扩形状或 general phi。
从第三批组合冻结 `dcd8204df22210db6893f3149e35f5490066f0c4` 新建隔离 Spec 分支，
不改原组合、借用后续工作或0400。代码固定 `ec11a0a03d1b49ab960567d71ac7ac9bafe6cd43`。

静态核查发现 Take 原核前驱集合/数量，若人为交换已记录的 P/Q 端点，集合仍相同。
正常 While/Break 钩子没有产生该状态的源码反例；本片不声称普通能力 red→green。
最小加固保存 While 已有 `finished` 块身份，并在 Take 时要求 condition-false 角色与之相同。
原构图、producer/catalog、Name/Bind、唯一 emitter 和双账本均不变。

新增 `owner_replace_break_ci_tests` 注册到现有 codegen lib，正式
`scripts/check_core.sh` 的完整 `cargo test --locked -p lang-codegen` 将选择这些测试，
没有忽略或另加省略开关。以下均为 CI-only，本地只编译、从未执行：

- 两套真实 Basic/Const Unit 分析，以同名/不同名 source 的外国 owned 产品尝试 sealed view
  构造，覆盖跨 analysis 错配，不手造 opaque owner。
- 已选计划缺失前驱、无关前驱、P/Q 端点角色对调，核结构化拒绝且不回退。
- 保持 run 函数严格 B 原样，caller 分别调用 false/true，两种 Unit 各独立链接/运行计数程序。
  在真实 String destructor 记录字节标签/长度/容量，预期每程序 P Drop一次，Q Drop为flag；
  literal容量0且malloc/free0。它只准备 literal 析构调用次数证明，不证明一般heap或Resource。
- 在测试生成的 LLVM 中复制/删除真实两处 Drop call作校准，不改生产 IR emitter；要求失败
  同时为SIGABRT和计数oracle专属marker，不能把任意崩溃、编译/链接失败当检测成功。

既有 frontend `loop_replace_break` 的 `replace_break_model_rejects_damaged_states_and_catalogs`、
`replace_break_deferred_publication_withholds_the_complete_product`、
`replace_break_consumer_rejects_damaged_targets_and_actions` 继续承担 foreign catalog、deferred
撤销、missing/duplicate action及consumer身份破损守卫，不复制或执行它们。合法非候选没有
可选 B 产品仍可 lower，不能把“产品可选”改成全局拒绝；普通无产品邻控继续保留。

本片不运行任何 CI-only、预期Abort、故障、计数、压力或sanitizer；Rust no-run与C的
`clang -fsyntax-only`不证明生成LLVM可链接或计数程序行为正确。正式双宿主CI仍未运行，
未推送、开PR或合main；Spec/D10/E2继续in-progress。实际本地门禁结果补于下方。

| 本地验收 | 精确选择与实际结果 |
|---|---|
| 普通行为 | 沿严格 B 后端账本同9个逐源/helper审查的exact，9 passed、0 failed、2817 filtered、0 ignored；含原样SSA/LLVM、真实receipt、双Unit/同字面/重分析、非候选、普通native与四邻控，未跑整个target |
| 负控编译 | `cargo test --locked --offline -p lang-codegen --lib --no-run --message-format=json -j 2`及对应`-p lang-frontend --lib --no-run`成功；实际两artifact debug=2，任何负控未计passed |
| 计数C静态检查 | Clang21对同一counter以`-fsyntax-only -Wall -Wextra -Werror -DEXPECT_Q=0`及`=1`通过；无LLVM/C链接、无计数或校准子进程执行 |
| 静态门禁 | `cargo clippy --locked --offline -p lang-codegen --all-targets -j 2 -- -D warnings`、`cargo check --locked --offline --workspace --all-targets -j 2`、`cargo fmt --all -- --check`通过 |
| 文档与尺寸 | docs879；`check_rust_sizes.py --base dcd8204df22210db6893f3149e35f5490066f0c4`：2290 Rust/36旧欠账；diff通过，无新增尺寸例外 |
| 独立终审 | 新上下文只读终审固定ec11a0a0相对dcd8204d六文件差分，无阻断；确认生产最小角色防御、test-only隔离、真实sealed错配与物理计数/校准范围，未代跑行为 |

本轮串行复用原 owner target，不复制、clean或降debug；实际manifest/日志留仓库外，仓库仅保
上述固定输入和选择。第三批组合475普通未整批重跑，不把其旧绿灯计为本片执行；未纳入借用
正在进行的后续证据。此处完成的是严格 B 剩余 CI 验收准备及最小防御加固，正式验收仍待，
不把所有动态 owner 或借用工作宣称完成。
### Loan 覆盖证书到真实SSA的跨阶段实证（2026-11-10，范围）

续接`2e9f9377`。当前codegen没有loan_action_observations/region证书消费者；Phase3区域
Complete不能代替真实SSA/native义务或退休Single。下一步先以已认证local literal同步
consumer为原样固定输入，双Unit入口走现行Unit lowering/refine和独立SSA verifier，检查
source/result/consumer依赖、实际End与Named Drop的身份及正常顺序，不新增第五种shape。

这是无预设能力实证：不强求frontend每个Begin都变成同名SSA操作，借用结果可能通过
BorrowCall产生Loan并直接交付consumer；必须按实际LoanId/源依赖映射核对，不能只数
操作或按函数名/Span猜关系。先报告现有能力/拒绝点，再决定是否需要具体接线合同；
不擅改lowering、ABI、owner未合代码或语言，不将初次绿称为新实现，不认领native释放计数。
只运行逐源/helper审计的普通有限SSA测试；人工破损/Abort/压力均不运行，既有borrow
target、defaultdebug2与串行Cargo不变，LLVM prefix仅用于codegen工具链构建需要。

后继生产接线最小合同（本次只整理，未实施）：

1. 建议先接已具真实SSA实证的protocol4，再以同一个consumer扩展1/2/3，不另起形状协议。
   backend只能查询已认证region/callable/family，不重新从AST推覆盖；声明region在对应
   normal-return lowering入口被真实消费，并留可验证receipt，不能只是读取数量或打印日志。
2. 建立同轮catalog record→具体lowering instance/function/call frame→SSA entity关系，
   source/receiver/result角色分别映射；source位置不是跨阶段LoanId。protocol4的consumer
   Begin映射已有BorrowCall result，不能再生成一条BorrowBegin。prototype中的callee
   参数Loan与caller Loan保持分离；多实例不能仅以callable或Span作key。
3. Complete范围内由唯一producer承担执行责任；验证或迁移原End/Drop生成点，禁止新证书
   消费者与旧清理双发。protocol1 temporary Drop、2 receiver retirement、3 named root、
   4 source/result链各自保持旧顺序，整个未过滤区间逐项消费。不能将当前测试中的手写
   八指令oracle移作生产重放器，必须绑定真实正在形成的SSA实体。
4. Partial/Unsupported或缺席不授absence，不扩大语言拒绝。迁移期间明确哪些仍由既有
   validated facts承担，哪些Complete范围已接管；同轮已声明消费却丢record、错catalog/
   instance/role的内部不一致须结构化失败，不能悄悄从Span补边。observer-off等价验证与
   正式接管后必需产品的缺失行为需在实现前定清，不能依赖关闭观察来绕过证据。
5. 先在精确头补四协议各自真实SSA/LLVM身份/完整顺序及Partial/Unsupported边界，再核
   原始/公开错误deferred撤销与owner×loan组合；含owner replace_consumer时显式拒绝或
   消费其真实关系。本分支未合owner产品，当前证据不能替代组合验收。
6. retirement仍须逐项追溯旧Single的身份、完整顺序、absence及资源oracle；原源码等强
   Unit证据、相关SSA/LLVM/native资源义务、CI-only破损/Abort守卫与精确头双宿主CI/独审
   满足后才可删对应旧witness。当前只有protocol4固定源码的并列Phase3/SSA/LLVM证据，
   没有生产receipt、统一实体映射/执行责任切换、其余三协议跨阶段账本或native释放计数，
   也未执行正式CI，因此本轮不退休任何旧oracle。

跨阶段实证验收：范围`8902cb1d`，普通测试及专题页`9f912755`。固定原样源码为
`fun view(source: String): borrow String from source = source`、
`fun consume(item: String): Unit {}`、
`fun run(): Unit { val owner = "kept"; consume(view(owner)) }`。
唯一普通exact为`ssa::loan_region_ssa_tests::certified_consumer_region_has_real_unit_ssa_lifecycle`，
最终1 passed / 2816 filtered，同一test内rooted与memory document两入口均完成；不按两个
入口重复累计。逐源检查unit_lower_test_support::parsed/analyze、refine observer及verified
LLVM helper：均正常有限输入，不含故障注入/Abort/压力或native资源执行。
测试核同catalog原始record IDs、transfer依赖、typed callee、实际形成/marker spans，
以及真实Unit/refine SSA的完整八指令、source/result独立Loan身份、consumer参数、逆序
End与原owner Drop；最后通过SSA与LLVM module verifier及真实instance限定定义检查。
生产lowering没有读取新证书，因此这里只建立并列事实，不是消费receipt或接线完成。

过程保留：首次较弱probe已1 passed；这不是新增backend能力红测。强化build2曾因
引用/值比较类型E0277编译失败，未运行；build3与build4各0 passed / 1 failed，均在
SSA及首入口LLVM verify后，失败原因分别是定义文本忽略internal属性、忽略实际
`f{instance}.`前缀。依adapter真实命名规则修正测试，没有修改生产ABI或SSA。最终build5
no-run9.42s，实际命令为`cargo test --locked --offline -p lang-codegen --lib --no-run --message-format=json -vv -j 2`，
随后先核binary --list包含目标，再直接运行该binary加上述名称、`--exact --nocapture`。
日志分别留`/private/tmp/koven-loan-ssa-probe.log`、`koven-loan-ssa-proof3.log`、
`koven-loan-ssa-proof-final.log`及`koven-loan-ssa-proof-final-green.log`，后三者也位于
`/private/tmp/`；build5 JSON/stderr同目录`koven-loan-ssa-proof-build5.*`。

始终复用`CARGO_TARGET_DIR=/private/tmp/koven-exclusive-loop-prefix-target`，codegen构建
及Clippy设置`LLVM_SYS_211_PREFIX=/opt/homebrew/opt/llvm@21`；
`CARGO_PROFILE_DEV_DEBUG`、`CARGO_PROFILE_TEST_DEBUG`、`RUSTFLAGS`、
`CARGO_ENCODED_RUSTFLAGS`均未设置，实际artifact debuginfo=2、opt_level=0、test=true，
rustc为`-C debuginfo=2`。最终binary SHA256为
`6fe48ec51b6f5c0fd661fe8cfcf5225696deb62a1a96e1a45d0c880f4f86c660`。
最终严格codegen all-targets Clippy13.92s、fmt、docs877、diff及尺寸2278 Rust/36旧超限通过；
尺寸工具首次缺少--base报用法错误，补`--base HEAD`后通过，没有改额度。独立只读复审
无剩余阻断。本次不重跑既有frontend346基线，不运行CI-only、native、正式双宿主CI或
workspace行为集合；这些未验收项不以LLVM verifier替代。既有偏差账本仍完整保留，
本轮不删除Single、不称source/callable Complete、不关闭D10/E2，未推送或合并。

### 第四批 owner 后继与 loan SSA 证据组合（2026-11-10）

复用隔离组合分支，从`dcd8204d`快进到冻结owner
`74f130499140d7e18afac76c65637cc679c70fb0`，再普通merge固定borrow
`efbe58c7e4a9a35e2f69638cf70c9c732830ec00`；精确代码头为
`409cd71519d079bfe3bca0a71157ce4b964b7f9a`，树为
`8cc6ce4f2fd1364d015b3d8297aa4204431d8ed5`。两个文档冲突逐段保留旧验收历史，
专题页保留已接入的活跃replace_consumer拒绝，再加入SSA事实；没有重新制造共同合同，
没有纳入borrow后续开发或0400。相对固定owner仅加borrow普通测试及注册；相对第三批
确实继承owner的false_block角色失败关闭加强，不能称生产零变更。既有emitter仍唯一
发射，B双ledger、显式carrier门、四开关和raw/public全局撤销测试保持不变。

本轮精确普通集合为第三批475项加唯一普通
`ssa::loan_region_ssa_tests::certified_consumer_region_has_real_unit_ssa_lifecycle`，共476项。
134 frontend lib、331 direct、1 iteration、10 codegen逐项成功，均0失败/忽略；filtered
分别450、10、218、2817。每组先对binary使用`--exact --list --format terse`核集合，
再`--exact`执行并核实际成功名称集合，不使用模块过滤。新增SSA项真实源码/helper已独审，
只核双Unit入口SSA/LLVM，并非生产证书消费、native计数或能力红测。旧9 codegen含普通
native实证继续执行；不是新增故障或计数执行。普通行为耗时依次1.46s、0.30s、0.05s、5.44s。

新增4个owner CI-only身份串用、incoming角色破损、物理Drop计数与故意Abort校准全部只编译；
此前所有CI-only、十项direct排除、四项iteration Abort及两个混合破损owner测试仍排除。
没有以代码命名替代helper审核，独审确认故障/计数路径不进入普通调用链。普通helper中的
观测只记录，故障状态默认None。本机未运行CI-only、正式CI、Linux、sanitizer或压力；
普通native成功不证明释放计数或内存泄漏义务。

串行复用既有owner target，Cargo为`--locked --offline -j 2`，默认debug2，四项
`CARGO_PROFILE_DEV_DEBUG`、`CARGO_PROFILE_TEST_DEBUG`、`RUSTFLAGS`、
`CARGO_ENCODED_RUSTFLAGS`均未设置；实际artifact debuginfo=2和rustc参数已核。
Rust1.96.0/macOS AArch64，LLVM_SYS_211_PREFIX为`/opt/homebrew/opt/llvm@21`。
frontend命令为`cargo test --locked --offline -p lang-frontend --lib --test ownership_iteration --test unit_direct_call_loan_ends --no-run --message-format=json -vv -j 2`，4.82s；
codegen命令为`cargo test --locked --offline -p lang-codegen --lib --no-run --message-format=json -vv -j 2`，1m04s。
本轮外置manifest、逐组名单/二进制hash、构建与行为日志位于
`/private/tmp/koven-owner-loan-combination-batch4-evidence`；临时目录不是永久验收真源，
可重建的固定代码头、第三批名单来源及本段唯一增量共同界定精确输入。
独立只读复核确认476名称集、成功集、binary hash、实际profile一致，无合流阻断。

静态门禁：`cargo clippy --locked --offline -p lang-frontend -p lang-codegen --all-targets -j 2 -- -W clippy::all -D warnings`通过（46.63s）；
`cargo check --locked --offline --workspace --all-targets -j 2`通过（20.32s）；
`cargo fmt --all -- --check`、文档879、diff检查通过；
`python3 scripts/check_rust_sizes.py --base 3fe1920a`通过，2291手写Rust、36既有超千行，
无额度/政策修改。all-targets只编译，不扩大行为执行集合。验收后仅追加本段文档，
无Rust变化；未推送、PR或合main，Spec/D10/E2保持in-progress，不退休Single或其他旧oracle。

### 2026-11-10 严格 B 同 analysis 多 callable 普通隔离证据

继承第四批冻结头`d8e062de`，测试代码固定`792a7fcc`；仅增加
`owner_replace_break_downstream_tests::bounded_replace_break_same_analysis_callables_keep_products_and_receipts_local`，
无生产实现变化或新增消费形状。既有双 Unit/重分析测试每个 analysis 只有一个 B 函数，
本项补同一 analysis 内两个同形函数：相同局部名和字面量，Basic/Const 两种 Unit，
各自原样严格 B 函数体，caller 交错调用 false/true 两边。

普通正向断言核两个真实目录的 owner 身份不同、各自合法目录归属、完整 retired 符号与
旧 Drop index 对应唯一授权 receipt、两个函数各自通过原 P/Q SSA oracle；真实 LLVM
验证及对象/链接/native 运行输出`done`。没有跨目录错误输入、故障注入、计数或预期 Abort。
此项是缺失回归证据的补充，不是生产缺陷红转绿，也不能替代 CI-only 或 heap 资源证明。

本地只执行上述一个普通 exact（1 passed），实际 Cargo test artifact debuginfo=2；
codegen lib no-run 静态编译成功。外置日志与 binary hash 在
`/private/tmp/koven-owner-multi-callable-evidence/manifest.json`；固定代码和 exact 名称为可重建输入。
codegen all-targets 严格 Clippy、workspace all-targets check、fmt、文档879及尺寸护栏通过（2291 手写 Rust、36 既有超千行，
本测试文件485行），无额度修改。CI-only 全部未执行；未推送或触发远端 CI。
### Loan nominal receiver跨阶段身份实证（2026-11-10，protocol2）

续接`efbe58c7`，测试与当前事实提交`667bd7d8168f8620b3041203aae58512481f30c1`。
固定原样源码为`class Node { fun touch(): Int = 1 }`及
`fun run(own node: Node): Unit { node.touch() }`。双Unit入口实际走Compatibility，refine
observer均为空；普通class Shared receiver尚无migrated准入，不能继承protocol4的refine
结论。首probe通过仅为已有能力证据补齐，不是新backend能力；未加anchor或改源码/生产。
后续protocol2 refine准入是独立实施决策，本片不处理。

唯一新增普通exact为`ssa::loan_region_ssa_tests::certified_receiver_region_has_real_unit_ssa_lifecycle`。
最终1 passed / 0 failed / 0 ignored / 2817 filtered，同一测试内rooted与memory document
两入口均完成，不按入口累计。逐源核parsed/analyze、routing observer、SSA/verified LLVM
helper均为正常有限输入。核同catalog全部三项ReceiverBegin/ReceiverEnd/Named Drop、
逐index原action ID、region caller/call/root、真实end/drop及Owned参数binding；typed
receiver来源/类型/模式与method symbol一致。完整参数声明44..58与名称marker48..52
分别锁定，不混成同一Span。SSA从Node Value入口建立RootPlace与Shared loan，真实
DirectCall接收该loan，End同loan后Drop原owner；Int结果与owner分开，callee独立
function-qualified Shared receiver不拥有payload。caller结果、callee常量/返回均核同
signed32 Int；完整五操作、block顺序、来源及callee路径另经独立SSA与LLVM verifier。

过程偏差全部保留：build1因测试试图Debug打印Program触发E0277，未执行；改用已有
render_program后build2成功23.65s。build2尚在编译时曾提前运行旧binary，实际0 passed /
2817 filtered，未命中不计成功。随后build3完成后从Cargo JSON取binary并先--list核名，
probe实际1 passed / 2817 filtered，双入口routes为空，揭示上述Compatibility边界。
强化build4因引用比较E0308及误写常量枚举I32（实际Integer）E0599编译失败；build5因
测试切片试图move非Copy end触发E0507；均未执行，后改借用slice。build6编译通过但未
运行，等待独审补强。build7编译后exact为0 passed / 1 failed / 2817 filtered：测试误将
完整参数declaration span等同Drop名称marker。依真实角色改为分别断言，生产不变；
这些测试假设/编译失败不是语言能力红测。build7失败binary SHA256为
`c31394768d4a052e4c1e4e182b735803043c454d972d44cc5e93b82b2ea1ce22`。

最终build8 no-run19.46s，实际命令为
`cargo test --locked --offline -p lang-codegen --lib --no-run --message-format=json -vv -j 2`；
完成后从JSON取唯一executable，以测试全名--list核名，再直接binary运行该名加
`--exact --nocapture`。唯一target为`/private/tmp/koven-exclusive-loop-prefix-target`，
Cargo串行、不copy/clean/降档。codegen构建与严格Clippy设置
`LLVM_SYS_211_PREFIX=/opt/homebrew/opt/llvm@21`，实际LLVM21.1.8；
`CARGO_PROFILE_DEV_DEBUG`、`CARGO_PROFILE_TEST_DEBUG`、`RUSTFLAGS`及
`CARGO_ENCODED_RUSTFLAGS`均未设置。manifest debuginfo=2、opt_level=0、test=true，
rustc含`-C debuginfo=2`。中途另一个Cargo进程经只读环境核实属于owner的不同target，
未干预。最初sandbox内ps受限后按授权升权只读检查成功，没有绕过操作拒绝。

最终binary SHA256为`a3212950ec1ef5ff60aed37930faf71c31c443dd95404032294eb63e9e8f07fc`；
exact名单SHA256为`01919d6fa46f1411a8a325b92c81d0f7fbfabaeff3e3840d1dfb5e5d20b28444`。
原始编译/失败/probe/最后exact日志保留在仓库外`/private/tmp/koven-receiver-ssa-*`，包含
`build1`至`build8` JSON/stderr、`probe.log`零命中、`probe3.log`、`final.log`失败以及
`build8-exact.log`和`build8-selection.log`；临时目录不保证长期留存，稳定证据为上述
提交中的原样测试及本账本，不将原始日志入库。最终严格
`cargo clippy --locked --offline -p lang-codegen --all-targets -- -D warnings`通过11.29s；
fmt、docs877、diff与尺寸`--base HEAD`通过，2278手写Rust文件/36旧超限，测试文件629行，
不增额度。独立只读复审与最终身份补强复核均无剩余阻断。

本片没有生产receipt/证书consumer、执行责任切换、新shape或语义变化；不重跑既有
frontend基线、protocol4 exact、workspace行为集合；所有CI-only人工破损、Abort、故障
注入、压力、资源计数及native行为均未运行，仅随目标编译。未运行正式双宿主CI，LLVM
verifier不代替native释放计数；不退休Single、不称source/callable Complete、不关闭
Spec/D10/E2，未push、PR或merge。

### 第五批双 callable 与 P2 Compatibility 证据组合（2026-11-10）

从干净`d8e062de`快进到冻结owner`59360f694676281bd04aca55bbd09ab0b32bfd79`，
再保父链merge固定borrow`0ca221db49f658a9abfcab2384f75ef655202a37`，精确代码头
`7b635a1c870432f4c2930b7395fb630b84edf048`、树
`05ed47c85c6a484487effa40df6848426f1e48a1`。唯一Spec尾部冲突保留两侧全部历史；
不纳入其他待批片，不改原worker树，不cherry-pick重建合同。相对第四批仅测试与文档变化，
无production变更，原carrier门、B单emitter双ledger、四开关和全局撤销保持原样。

精确普通选择沿用第四批476项，仅增加以下两项：
- `native_tests::boxed_enum_tests::unit_source::owner_replace_break_downstream_tests::bounded_replace_break_same_analysis_callables_keep_products_and_receipts_local`。
- `ssa::loan_region_ssa_tests::certified_receiver_region_has_real_unit_ssa_lifecycle`。

独立逐源/helper审计确认前者同轮双callable×Basic/Constant独立身份、receipt与普通native
输出，不含人工故障或释放计数；后者双Unit输入的P2完整区间与真实SSA/LLVM，仅为
Compatibility证据，显式断言refine路由为空，不宣称已迁移refine。两者均非生产证书接线，
无新语言能力红测。所有新旧CI-only故障/计数/校准/Abort/压力、十项direct、四项iteration
Abort及两个混合破损owner条目仍排除，不使用模块过滤以免误选CI-only。

本轮串行复用原owner target，Cargo `--locked --offline -j 2`，默认debug2，无
CARGO_PROFILE_DEV_DEBUG、CARGO_PROFILE_TEST_DEBUG、RUSTFLAGS、CARGO_ENCODED_RUSTFLAGS
覆盖；LLVM_SYS_211_PREFIX仍为`/opt/homebrew/opt/llvm@21`，Rust1.96.0/macOS AArch64。
frontend和codegen使用第四批同一no-run命令，均按实际compiler-artifact定位binary，
先`--exact --list --format terse`核名，再`--exact`运行，核实际成功集合及二进制hash。
外置选择/manifest/日志留`/private/tmp/koven-owner-loan-combination-batch5-evidence`；
临时路径不是永久真源，固定提交测试、原集合来源及以上两项增量界定可重建输入。

实际478普通全部成功：134 lib（1.13s）、331 direct（0.27s）、1 iteration（0.06s）、
12 codegen（7.33s），0失败/忽略，filtered分别450、10、218、2817。
frontend no-run4.11s，codegen no-run1m04s。过程偏差：codegen构建已success，但`-vv`
触发llvm-sys构建脚本文本混在JSON流，最初外置脚本JSONDecodeError而未启动后端测试。
保留原始日志，脚本改为只允许JSON行或已逐行核实的`[llvm-sys 211.1.1] `前缀，仍严格
核build-finished成功和实际profile，然后复用该精确输入刚构建的binary完成12项；不重跑
前端466项、不重编、不改Rust。这是验收解析修正，不是行为红测。所有artifact实际
profile.debuginfo=2，选中binary名称集合及成功集合相等。

独立只读复核固定HEAD/tree、478选择与实际成功集合、四个binary hash、构建成功及
默认debug2均一致；确认103行llvm-sys前缀文本及严格解析处理，无阻断。
严格`cargo clippy --locked --offline -p lang-frontend -p lang-codegen --all-targets -j 2 -- -W clippy::all -D warnings`通过40.70s；
`cargo check --locked --offline --workspace --all-targets -j 2`通过19.31s；
`cargo fmt --all -- --check`、docs879、diff及尺寸`--base 3fe1920a`通过。
尺寸2291手写Rust、36既有超千行，无额度变更；两新增测试所在文件485/629行。
最终仅追加本账本文档，无Rust再变动。未执行正式CI、Linux、sanitizer、压力、故障与
计数校准；普通native不证明释放计数义务。未push、PR或合main，Spec/D10/E2仍in-progress，
不退休旧oracle，不把P2 Compatibility或P4并列实证称为证书生产消费完成。

### SPEC-0400 main 基线上的独立组合准备（2026-11-10）

本地main/origin/main实际为`a775568fcbf1cf219460e0b6ec57dad1e7c80772`（#409），
对象已在本地，无fetch。在此固定main上另建隔离分支
`feature/spec-0341-owner-loan-main-20261110`，保父链merge冻结`c27d9ae0`，零冲突。
原组合树保持不变。实际Rust代码头`212b2c2a1e28f2d59b9ab27edbfff6c654817457`，
树`b242a2fe44e8ae2c3506ff256fdcf1cd36641874`。共同祖先仍3fe1920a；两边文件差分
没有同路径重叠，但#409确实退休Compatibility生产载体，不能由无文本冲突推断行为等价。

独立只读准入复核原478普通名单不需调整：P2 nominal receiver不走被删RangeView Carrier
分支，实际receiver Begin/DirectCall/End/Named Drop路径保留；仍为Compatibility，
不宣称进入refine。B/P4 refine、frontend撤销策略、显式replace_consumer门和三个联合
测试未改。迁入result_tests的人工Fixture不在选择集、不进入普通helper链。没有新增
相似测试或为合流制造生产补丁。所有CI-only故障/计数/校准/Abort/压力仅编译，旧排除
保持；不按模块filter执行。

本轮在新树新输入重编，同一478精确普通名单先执行两个loan_region_ssa_tests及B的
physical_drop_has_product_authorization、same_analysis_callables_keep_products_and_receipts_local、
absent_optional_product_keeps_ordinary_lowering五项（完整名称沿用第五批）；通过后执行
剩余7 codegen，再134 lib、331 direct、1 iteration。五项不在后续组重复计数。
Cargo命令及精确集合来源沿用第五批，默认debug2、四项覆盖变量未设置，串行复用
原owner target，LLVM_SYS_211_PREFIX为`/opt/homebrew/opt/llvm@21`。外置manifest、
逐组日志和实际binary hash位于`/private/tmp/koven-owner-loan-main-evidence`；临时目录
不保证永久留存，固定提交及此前精确名单来源界定可重建输入。旧478绿灯不充作本轮结果。

新合成头实际478普通全部通过，0失败/忽略：codegen关键5项1.98s、其余7项6.06s，
前端134 lib 1.20s、331 direct 0.27s、1 iteration 0.06s；前端filtered450/10/218，
后端两组filtered2824/2822，选择集合互斥并集为12。codegen no-run1m24s。
这不是沿用旧binary结果，构建源路径为新隔离树，实际artifact debuginfo=2；无语义失败、
无生产修复。P2完整五指令及Compatibility边界、P4完整链与B授权/隔离/fallback均有
本次运行正证；不能据此推断完整正式CI或资源释放计数已验收。

frontend no-run39.23s。独立证据复核478集合完全等于第五批，5+7互斥无遗漏，四个
binary hash均匹配manifest且不同于旧组合；两次构建成功、实际debug2，无CI-only执行。
严格双crate all-targets Clippy（`-W clippy::all -D warnings`）通过54.75s，
workspace all-targets check通过35.50s，均`--locked --offline -j 2`；fmt通过。
docs880、diff及尺寸`--base a775568f`通过：2288手写Rust、36既有超千行，无额度变更。
本轮原始构建日志均纯JSON，无新增脚本解析偏差。最终仅追加本账本文档，没有Rust再改动。
原冻结组合c27d9ae0仍clean；不改main，不push/PR，正式CI、Linux、sanitizer、压力及
全部CI-only故障/资源计数仍未运行。Spec/D10/E2保持in-progress，不新增旧oracle退休声明。

### Protocol 4 可选生产核验 receipt（2026-11-10，实施中）

用户批准在冻结 `f51edf929084a05b99d93d1889d0c34c24688c54` 上接入受限生产核验。
范围仅同轮已认证 Complete protocol 4：绑定 catalog action 身份、当前 callable/call
实例、真实 source/result Slot 与实际 SSA instruction/entity，正常返回必须完整闭合。
既有 producer 仍唯一生成 Begin/BorrowCall/End/Drop；不重放测试八指令，不按 Span
事后拼配。statement 标记随直线块合并运输；Alias 不伪造 SSA instruction。

核验按实例工作，兼容已准入 outer Unit/Int 及目标调用前后 plaincall，不把整个函数
限定为八指令。内部错配返回结构化 lowering error。observer-off、Partial、Unsupported
及缺席继续旧 facts 路径且无 receipt，不授 absence、不新增语言拒绝。B owner ledger
保持独立；不迁 protocol 2、不扩 shape、不宣称 source/callable Complete、不退休 oracle。

原验收计划：先普通缺 receipt 红测，再实际构造接线；精确执行普通正例、Partial
边界与 Int/邻接 plaincall。observer-off 受现有私有入口限制，端到端未运行，详见后文。所有人工破损、Abort、压力与资源计数只编译。
固定唯一 target、默认 debug2 与 LLVM21 配置沿用委托约束，串行构建，先从本轮
Cargo manifest 核 binary 与测试名再 exact。实际结果随后补充；本段不声称已实现或验收。

首个普通红测沿用原样 `certified_consumer_region_has_real_unit_ssa_lifecycle`，只增加
生产 receipt 观察断言。no-run 成功后从本轮 Cargo JSON 取唯一测试 binary，先 exact
list 核名，再 exact 实跑：0 passed、1 failed、2828 filtered；实际 lowering 成功，但 receipt
为0，预期1。这是真实缺接线红测，不是故障注入或测试源码能力假设失败。binary SHA256
`b81dd3d9834752ec520d96edab687064531ce485079e7bbdfa275564e9cfcb96`。
命令为 `cargo test --locked --offline -p lang-codegen --lib --no-run --message-format=json -vv -j 2`，
实际 artifact debuginfo=2、opt_level=0；LLVM21.1.8。唯一 target 为
`/private/tmp/koven-exclusive-loop-prefix-target`，构建前无活动 Cargo/rustc，四项覆盖变量
未设置。原始日志在仓库外 `/private/tmp/koven-p4-receipt-red.{json,stderr,log}`。

实现接线保留现行 refine 路由，Compatibility 仍走旧行为，不自动迁移所有 frontend
Complete4。静态核实旧 Compatibility 的 nested call-use 仍会原样拒绝；不为 receipt
放宽准入。正常返回指 consumer 的局部续点，不要求后续整函数 terminator 是 Return。
生产 `Plans` 核同 catalog 全记录、三项 retired 引用、source/callable/实例映射及原 Drop
index；RStmt 标记由原 producer 当场形成，emit 对真实 append 身份逐步冻结 source/result
与 owner，最后逐计划恰一次闭合。B 原授权/提交/consumed_drops 顺序未替换。

中间过程如实记录：build1 仅编译失败（模块可见性、引用比较及 Drop 字段名），未执行；
build2 编译通过，同一普通红测1 passed/2828 filtered，binary SHA256
`fd54e0be94f578780414579b26e8c02f56a845de58a8baa35e07b0e668773286`。
build3/4 分别因新增测试误用 Program.function、Operation.operands API 编译失败；build5
仅因 CI-only 子模块路径错误编译失败，均未运行测试，不记作语言能力红测。
build6 编译34.43s，实际 debug2，五个普通 exact 均1 passed/2835 filtered：原P4、原P2、
Int consumer 加前后 plaincall、Partial live root、同轮双P4 callable；binary SHA256
`3e220d72d128592a947956f202d185122783551b6464fd36c0bba447f2836fb2`。
四项 CI-only 破损测试只编译，覆盖错catalog receipt、重复/遗漏角色与stage、Slot重绑/
错误callee、外来函数instruction。其 fixture 捕获成功生产产物，只复核原实际instruction
身份，不再生成SSA操作。初次严格Clippy因提取helper后的 needless_borrow 失败，已修。
最终新增 B/P4 sibling 普通测试与身份接点精简后的统一验收另记，不复用中间binary绿灯。

observer-off 开关目前只在 frontend 私有测试路径；本片不新增公共开关、不模拟删除必需
证据。保持既有 frontend off 全产物证据及生产 empty-plan 分支，未声称本次实跑端到端
codegen observer-off 等价。Partial 不替代此未运行项。所有 native、CI-only、Abort、
压力、资源计数、正式双宿主 CI 未执行；不关闭 Spec/D10/E2，不push/PR/merge。

最终代码固定 `c81259c3377e413c66b6f3dbc1cb77d1382fc1c7`；本地分步提交为范围
`2bdfeea5`、普通缺 receipt 红测 `04079b80`、生产与测试 `c81259c3`。最终重新编译29.12s，
使用前述同一 no-run 命令、唯一 target、LLVM21.1.8 和默认 debug2；Rust1.96.0/macOS
AArch64。四项覆盖变量均未设置，实际 artifact debuginfo=2、opt_level=0、test=true，
rustc 含 `-C debuginfo=2`。构建完成后逐项从该 binary exact list 核名再运行，不提前跑旧binary。

最终普通精确选择（每项1 passed、0 failed、0 ignored、2836 filtered）：

| 验收项 | exact 测试（均在 `lang-codegen --lib`） | 实际结果 |
|---|---|---|
| 原样P4同catalog到实际SSA receipt与旧oracle | `ssa::loan_region_ssa_tests::certified_consumer_region_has_real_unit_ssa_lifecycle` | 通过，rooted/memory两入口 |
| P2保持旧Compatibility边界 | `ssa::loan_region_ssa_tests::certified_receiver_region_has_real_unit_ssa_lifecycle` | 通过，未迁P2 |
| outer Int及前后plaincall | `ssa::loan_region_ssa_tests::receipts::int_consumer_with_neighboring_plain_calls_has_local_receipt` | 通过，不限制全函数八指令 |
| live root Partial | `ssa::loan_region_ssa_tests::receipts::partial_live_source_preserves_legacy_lowering_without_receipt` | 通过，无receipt、旧SSA/LLVM有效 |
| 同轮双P4 callable | `ssa::loan_region_ssa_tests::receipts::sibling_callables_keep_catalog_and_ssa_receipts_local` | 通过，catalog action/callable/SSA实体互异 |
| 同轮B/P4 sibling | `native_tests::boxed_enum_tests::unit_source::owner_replace_break_downstream_tests::owner_loan_receipt_sibling_tests::bounded_owner_and_protocol4_siblings_keep_receipts_and_drops_separate` | 通过，Basic/Constant两入口，B授权及P4物理Drop/receipt分离；仅SSA/LLVM，无native执行 |

最终 binary SHA256 为 `08971c5cb86e4152e7dc495cb8a0a8596ce24c60facd87666e1739e735870597`，
六项完整名称（逐行、末行换行）SHA256 为
`dfb827090641f8aaf32cbc613427a1b8bccd1abd646f02c7c279f62574a6abd5`。
仓库外 `/private/tmp/koven-p4-receipt-final-manifest.json` 保存实际artifact、六项返回码和
成功集；同前缀 `.json/.stderr/-exact.log/-selection.txt` 保存原始编译与逐项日志。
临时路径不保证永久保留；固定代码提交、上述exact集合与摘要为可重建验收身份，不入库dump。

严格 `cargo clippy --locked --offline -p lang-codegen --all-targets -j 2 -- -D warnings`
通过26.50s；`cargo fmt --all -- --check`、文档880、diff和尺寸 `--base HEAD`
（实施比较基线 `04079b80`）通过：2296手写Rust文件、36既有超千行，无额度改动。
`build.rs` 988行、`emit.rs` 980行；两个原statement helper按生成/发射职责提取，未迁公共IR。
前述四项新CI-only及所有既有人工破损/Abort/资源计数仅随目标编译；六项白名单不含它们。
未执行workspace全量、native、Linux、sanitizer或正式CI，既有oracle与Spec状态均保留。

本轮共6个唯一普通测试通过；每项内部双入口不另累计。未重跑旧478组合名单，
不以其历史绿灯替代本轮六项，亦不将本轮六项外推为全量兼容性验收。

独立只读复审确认精确代码头 `c81259c3` 无Rust漂移或剩余阻断；另行重算最终binary与
名单SHA256、核本树Cargo artifact/debug2/build-finished以及六条实际成功记录，均一致。
复审未执行测试，未把静态负控或文档检查当作运行证据。最终文档提交仅记录本轮结果，
不声称在文档头重跑Rust；未push、PR或merge。

### Optional P4 receipt 与既有 owner/loan 普通集合联合验收（2026-11-10）

另建隔离验收分支`feature/spec-0341-p4-receipt-validation-20261110`，固定输入
`1ef893491245cede3d6bed9e3101ac341bd9bf3c`（Rust代码`c81259c3`），树
`b5f21cde061677f33b8db8558083588a11f734c1`，祖先含f51edf92与main a775568f。
原开发树与旧冻结树未改；不纳入observer-off后续开发。本次只验证固定产品，不增加
类似测试、生产补丁或另一个receipt合同。

独立逐源/helper准入复核：旧478与前述六项取并集，重复恰为P2/P4两项，准确总数482，
其中前端466（134 lib、331 direct、1 iteration），codegen16。新增四项为Int consumer
相邻plaincalls、Partial live root、双P4 callable、B/P4 sibling，完整exact名称取前段
六项表排除旧P2/P4；不将双入口重复计数。B/P4 sibling虽然注册在native_tests，实际
仅SSA/LLVM，无native执行。旧集合中的普通native仍按旧白名单验收，不扩大为计数测试。

新receipt observe只收集成功产物；CI snapshot默认None，人工构造/破损仅由四项
ci_only开启，均禁止执行。保持所有旧CI-only、十direct、四iteration Abort及两项混合
破损owner排除，不使用模块filter。Partial/空计划保持旧路径，P2 Compatibility不迁移，
旧effect producer、B授权与Drop消费顺序不替换。当前缺席的端到端observer-off新证据
不以Partial或本次普通集合替代，后续片独立验收。

本轮Cargo沿用`--locked --offline -j 2`、默认debug2、原owner target，不copy/clean；
四项profile/RUSTFLAGS覆盖均未设置，LLVM_SYS_211_PREFIX为`/opt/homebrew/opt/llvm@21`。
从本验收树重新no-run编译，按实际artifact定位binary，先exact list核名再exact执行，
核成功名称集合/二进制hash。外置manifest、选择与分组日志留
`/private/tmp/koven-p4-receipt-combined-evidence`，临时目录不是永久真源；固定提交、
前轮478名单来源及前段六项并集为可重建输入。既有旧478或单独六项绿灯不冒充本轮结果。

本轮实际482普通全部成功，0失败/忽略：codegen两组8+8互斥（22.75s/27.25s，
各2829 filtered），前端134 lib 1.16s、331 direct 0.28s、1 iteration 0.06s，filtered
分别450/10/218。codegen no-run1m25s、frontend no-run40.12s。每组成功集合与所选
精确名称相等。未发现普通兼容失败，无生产修复；既有B普通native确实重跑，新B/P4
sibling仅SSA/LLVM，不能把其路径名误读为native或物理释放计数证据。

独立证据复核HEAD/tree无漂移，482名单、分组成功集合及四binary hash一致，实际
artifact debuginfo=2/opt_level=0/test=true，build-finished成功；无CI-only执行。
严格双crate all-targets Clippy（`-W clippy::all -D warnings`）通过58.29s；
workspace all-targets check通过1m01s，均`--locked --offline -j 2`；fmt通过。
docs880、diff及尺寸`--base a775568f`通过，2296手写Rust、36既有超千行，无额度修改。
本轮最终只追加本账本文档，无Rust再改动。未运行observer-off后继代码、正式CI、Linux、
sanitizer、压力或任何CI-only故障/资源计数；不把482普通通过外推为这些义务完成。
不push/PR/合main，Spec/D10/E2状态与旧oracle保留。
### Protocol 4 真实 observer-off 端到端补证（2026-11-10，实施中）

续接本地 `1ef89349`，按已批准范围增加默认关闭的窄测试支持 feature
`loan-observation-test-support`。frontend 提供隐藏的同步 scope API，codegen 同名 feature
经既有 normal 依赖转发；不增加 dev-dependency 或修改四逻辑边检查器。原 dev-dependency
建议会触发现有 duplicate direct declaration 门禁，故采用此路线。显式启用会把测试支持
编入 frontend 库，`doc(hidden)` 不是安全边界；默认生产/发布图不启用，不读取TLS。

scope 只接真实 planner 的 loan observation bool，不改 owner 开关、不删已生成产品或关闭
receipt 核验。以同 sources/names/typed 做两次真实 ownership 分析，off 原产品直接传
codegen；只读比较 helper 仅在独立 clone 上去掉新增 catalog 用于旧产品全量对照。
私有RAII保存并恢复前态，不公开可跨线程移动的guard；普通嵌套与线程隔离精确执行，
panic恢复负控仅编译。先普通缺off红测，再接真实scope，最终核P4 receipt有/无及旧SSA/
LLVM/B产品等价。默认CLI check与feature测试构建分别核各实际artifact.features。

正式CI仅在 `check_core.sh` 的codegen测试命令显式启用，精确选择合同同步；不在本地
运行该全量命令。发布 `package_preview.py` 的CLI release命令不启用features；所有
显式all-features构建仍可能编入hook，不能称为cfg(test)隔离。此前全部target、debug2、
CI-only不执行与本地不push/PR/merge约束继续有效。

普通能力红测先以未接线scope（直接执行closure）运行：真实二次ownership仍有catalog，
`real_frontend_observer_off_preserves_products_ssa_and_owner_receipts` 在明确off断言失败，
0 passed/1 failed/2837 filtered；不是清表或禁用receipt的模拟失败。feature no-run成功后
从Cargo JSON取binary并exact list核名再运行；frontend/library与codegen/test实际features
均仅 `loan-observation-test-support`，debug2。binary SHA256为
`a114dfd016cc430d9120ef1002d0bb67623051142358275e3d4942224f19eb73`。
本轮先做feature红测，默认CLI图对照后置串行执行，不声称先验证了默认图。

真实scope接线代码为 `242691c63954a4cf7cb7ec513f4058deaa47978e`，红测提交
`1a8d445b`。首个实现构建27.19s成功；同红测及普通嵌套/线程隔离各1 passed、0 failed、
2839 filtered，真实off原产品无catalog且直接进入codegen，无P4 receipt；on有receipt，
完整旧ownership副本、SSA/LLVM文本和B授权/物理Drop相等。没有中间Rust编译或行为失败。
只读比较沿现有产品Eq语义，不修改传给lowering的任一原产物；B分离另由实际receipt检查。

最终同代码feature no-run完成0.25s，复用经Cargo确认的新feature artifact；frontend与
codegen的features均精确为 `["loan-observation-test-support"]`，实际debug2/opt_level0，
前者test=false、后者test=true。从该轮JSON取binary、逐名exact list后运行前轮六项普通
测试，再按顺序追加以下两项，共8个唯一测试全部通过，每项1 passed/0 failed/0 ignored/
2839 filtered，内部双输入不另累计：

| 验收项 | exact测试（前缀 `ssa::loan_region_ssa_tests::observer_off::`） | 实际结果 |
|---|---|---|
| 同names/typed真实on/off ownership到SSA/LLVM/B receipt | `real_frontend_observer_off_preserves_products_ssa_and_owner_receipts` | 通过；Basic rooted/memory两输入 |
| 正常嵌套恢复与单新线程默认on隔离 | `observation_scope_restores_nested_state_and_does_not_cross_threads` | 通过；无故障注入 |

feature binary SHA256：`6a5e0316b2554c4711021db9ea98b645f5629bd962bf04fa7788b3e248c5b8ef`；
八项逐行完整名单SHA256：`64dddd6dbd0baf77d36011bc3b055d06761313ac3b9bd63dfbc05d8da491811f`。
最终manifest、选择和日志为仓库外 `/private/tmp/koven-p4-off-final-manifest.json` 及同前缀
`-selection.txt/-exact.log/.json/.stderr`；红测和首绿日志分别在 `koven-p4-off-red`、
`koven-p4-off-build1` 前缀。临时路径不保证永久保留，固定提交/测试/摘要界定重建输入。

默认图单独执行 `cargo check --locked --offline -p lang-cli --message-format=json -vv -j 2`，
13.95s通过；从本树实际artifact核 `lang_frontend`、`lang_codegen`、`kovenc` 三者features
均为 `[]`、debug2/test=false，不只看CLI自身。原始日志为
`/private/tmp/koven-p4-off-default-cli.{json,stderr}`。初次按纯JSON解析失败，随后前缀
校验误限llvm-sys再次失败；实际逐行核11条 `[serde_core 1.0.229] cargo:` 构建脚本输出后
窄允许该前缀，复用同次已成功构建完成artifact核验，未重编或把解析失败当作能力红测。
发布脚本只静态核既有CLI release argv无feature；没有执行release构建或发布。

严格feature门禁为
`cargo clippy --locked --offline -p lang-frontend -p lang-codegen --features lang-codegen/loan-observation-test-support --all-targets -j 2 -- -D warnings`，通过50.85s；
默认 `cargo check --locked --offline --workspace --all-targets -j 2` 通过37.01s。
原依赖边检查仍为5成员/4允许边，未修改checker、未新增dev edge，Cargo.lock无变化。
仅执行两个逐源核验的纯读Python exact：`CheckCiResultsTests.test_unit_migration_routes_and_exact_regressions_run_on_both_hosts` 与
`CheckCiResultsTests.test_loan_observation_support_is_explicit_test_only_selection`，均通过。
恢复脚本故障stub的精确新命令预期只作源码更新，不执行；CI完整测试命令亦未本地运行。
fmt、docs880、diff与尺寸 `--base 1ef89349` 通过：2298手写Rust文件、36既有超千行。
旧 `compilation_unit.rs` 仅+3行feature模块注册（1078行），`drop_planner.rs` 仅+4行净增长
（1592行）接现有bool；算法与scope在独立模块，既有额度及拆分政策不变。

唯一target、LLVM21.1.8、Rust1.96.0/macOS AArch64与默认debug2保持，四项环境覆盖仍未设置；
全部Cargo串行。新增 `ci_only_observation_scope_restores_after_panic` 及前轮全部CI-only
破损/Abort/压力/资源计数仅编译，不执行。未重跑旧478、native、sanitizer、正式双宿主CI。
本片补齐上述Basic普通输入的真实off证据，不外推为常量capability off或所有形状覆盖，
不迁Compatibility、不修改执行责任、不退休oracle、不关闭Spec/D10/E2，未push/PR/merge。

独立只读复审精确 `242691c6` 无阻断，重算binary/选择hash与八条实际成功日志一致；
默认三crate无feature、feature图debug2、11条serde_core前缀及0.25s均另行核实。
两旧超千行文件的本轮有界注册/选择接线增长经独审认可，不修改1156/1669既有额度。
最终仅补文档，不在文档头重复声称执行Rust；所有未运行边界保持如上。

### P4 receipt 联合账本与真实 observer-off 最终合批（2026-11-10）

另建隔离分支`feature/spec-0341-p4-off-combination-20261110`，保父链合入冻结
`2dc022e8d41cd45f6154de7bb2d2211881a32732`与
`852c0e98b1db219e6468140be65174be99a9d593`，共同祖先1ef89349；原冻结树不动。
唯一Spec尾部冲突保留两侧真实历史。实际合成代码头
`4be8b6db135dbe126c2b7c13d272386dd5bc0aca`，树
`584882edee87f36c1ea7d027dde0e295b8421c01`，没有额外生产修改。

独立准入复核旧482与前段off八项名单去重，准确484：前端466默认图，后端18显式
`loan-observation-test-support`图。仅新增真实off等价及正常嵌套/单新线程隔离两项，
不存在panic或压力执行；panic恢复ci_only继续只编译。旧helper默认on，RAII恢复前态，
真实off产品未清表直接lowering；只读比较helper只改独立clone。不得把feature隔离
称为cfg(test)：显式all-features会启用hook。本轮另跑默认CLI并核frontend/codegen/
kovenc三crate artifact.features=[]，不以CLI自身或旧代码头结果替代。

串行复用原owner target、默认debug2、四项profile/RUSTFLAGS覆盖未设置，
LLVM_SYS_211_PREFIX为`/opt/homebrew/opt/llvm@21`；不copy/clean/降档。
后端no-run显式`--features loan-observation-test-support`，前端no-run不加feature；
其余选项为`--locked --offline --message-format=json -vv -j 2`，原测试target不变。
每组从本轮artifact取binary，exact list核名后exact运行并核完整成功集合/hash。
外置证据为`/private/tmp/koven-p4-off-combined-evidence`；固定合成头与原482/八项
并集界定可重建输入，不把前片或旧组合绿灯冒充最终合成。

实际484普通全绿，0失败/忽略：feature后端18分两组10+8互斥（25.20s/29.85s，
filtered2830/2832）；默认前端134 lib 1.09s、331 direct 0.36s、1 iteration 0.06s，
filtered450/10/218。codegen no-run1m28s、frontend no-run48.12s。后端构建实际
frontend/library与codegen/test features均精确为`["loan-observation-test-support"]`，
默认前端三测试artifact均features=[]；全部debug2。新增两项确实列出且执行，不能
用不启feature时的零命中过关。所有CI-only包括panic恢复守卫均只编译。
两个已逐源核纯读Python exact（CI路由及显式测试feature选择）通过，未执行恢复故障stub
或check_core全量行为命令。尺寸以a775568f为base通过，2298手写Rust/36旧超千行，
无额度变更；旧注册/bool接线增长范围与前片相同。

默认CLI实际新跑`cargo check --locked --offline -p lang-cli --message-format=json -vv -j 2`
12.42s成功，frontend、codegen、kovenc三者features=[]、debug2/test=false。
原始流含11条已逐行检查的`[serde_core 1.0.229] cargo:`文本，其余JSON的artifact及
build-finished成功单独核实，没有重编或将解析处理当能力红测。独立复核484实际集合、
四binary hash、默认/feature两图与HEAD/tree完全一致，无CI-only执行或Rust漂移。

严格feature Clippy命令为`cargo clippy --locked --offline -p lang-frontend -p lang-codegen --features lang-codegen/loan-observation-test-support --all-targets -j 2 -- -W clippy::all -D warnings`，通过42.97s；
默认`cargo check --locked --offline --workspace --all-targets -j 2`通过35.28s。
fmt、docs880、diff通过；依赖边及release命令未改变，未执行release构建或发布。
最终仅追加本账本文档，无Rust再修改。原2dc022e8冻结树仍clean；未push/PR/合main。
全部CI-only、正式双宿主CI、Linux、sanitizer、压力/资源计数仍未执行；off实证限此Basic
rooted/memory输入，不外推为常量capability或所有shape覆盖，不退休旧oracle，不关闭Spec/D10/E2。

### 已批准：变化 owner 两出口向唯一 own 调用移交（2026-11-10）

用户对动态 owner 与 Shared receiver 两项受限后继回复“是的”。本片只实施 owner 侧，
基于已发布`2d79c4d6`新建隔离分支`feature/spec-0341-owner-own-call-20261110`，
不等待或改写在途 #410 CI，不推送；借用侧独立并行。

原固定 Boolean 参数、String 字面量 root、一次 Replace 后裸 Break 保持；唯一尾句允许
`sink(held)`，sink 为同 Unit 已解析顶层非泛型普通函数，唯一 own String 参数、Unit 返回。
首个普通源为 `fun sink(own item: String): Unit {}`，原 B 的 `val retired = held` 改为
该直接调用，entry 调用 false/true 两边。新片不改原 B 的 Take/Bind/Drop 合同。

Phase 3 必须从真实 Name take 与实际调用交付记录发布独立封印的 Take/Call 消费证明，
保原 P/Q catalogs、不透明身份与 false/Break 角色。不得用 AST 匹配或旧 P 状态投影
补造 Q 的交付。Phase 4 只核同轮、同 callable 产品并授权实际参数槽和目标调用；
旧 emitter 唯一发出调用，caller 不再 Drop 已移交的选中值，callee 沿原 own 参数清理，
true 路径 detached P 仍按旧规则清理。任一已选产品失败不得回退。

不含多参、receiver、泛型/间接调用、Group/Not、任意后缀、owner 返回、loan/capture、
Resource、一般循环或新 runtime。既有 main/旁线 owned-call-temporary 是 Owned 返回值
被借用的结束规则，invariant-break-suffix 是不变输入尾句；均不等同本片变化 P/Q 移交。

实施状态：in-progress。先建立真实普通生产 consumer 红测，再分步实施 Phase 3/4；
Basic/Const 双 Unit、两出口实际 SSA/LLVM/native、精确调用参数与 caller/callee 清理为
普通验收。错误封印/动作重放/计数校准等 CI-only 仅编译，正式双宿主 CI 仍另行验收。
保留旧 oracle，不关闭 SPEC-0341/D10/E2；本地提交后集中交付，不单独新开 PR。

### 受限 own 调用实施与普通验收（2026-11-10）

合同头`f0574ac2`后，真实红测试头`368444f1`保留普通源码及旧SSA/LLVM验证，
观察唯一旧emitter成功push的实际sink调用；当时无Call产品授权，实测receipt=None，
与实际参数槽Some(2)不符。不是空collector或人工破损输入。首次构建与前端并行落盘
交叉，不作红输入证据；精准恢复前端后重新静态构建稳定红头，随后执行同名ordinary exact。

实现固定`b8fe8d604d130fe7e63987f5df4306a52b9f1032`。Phase 3新增独立
`UnitOwnerReplaceBreakCallConsumerV1`及Take/Call动作；真实carrier经Name Take进入
同一个PendingTemporary，仅在旧transfers_at_call分支完成移交后封印。旧Unsupported
观察的scalar owner仍为None，不回填P事件；P/Q由已核Take的carrier持有。目标来自同Unit
checked declaration、签名和精确symbol，不按名字。原B getter/动作/清理保持。

Phase 4共享原P/Q producer与两出口角色核验，Call产品另解实际dispatch FunctionId，
核Name Take槽、唯一参数及旧deliver已标记的root状态。只有原Unit DirectCall成功发射后
提交完成；caller若出现选中root Drop则拒绝。旧B仍要求Take/Bind/Drop双ledger；
调用分支没有伪造Drop index，callee仍走原own参数清理，不新增运行时或语言拒绝。

新增前端3 ordinary核真实pending移交、同Unit跨source解析、合法邻形不发布；共用原
compare核观察on/off完整旧Analysis，不外推为新Call后端off端到端证明。frontend最初
两fixture缺函数间换行造成L0047，已仅修源码分隔符；不计生产红转绿。
后端新增3 ordinary核真实调用授权、Basic/Const两Unit完整SSA的P/Q入边与callee参数Drop，
及正常native输出outer/next。caller只Drop detached旧P；callee空体/打印体沿旧规则清理。
native输出不证明析构计数、heap或Resource无泄漏。

普通选择为前端既有已审134+新3，后端旧6 B+B/P4 sibling+新3，共147唯一exact。
只用名单运行，全部CI-only（新frontend破损/deferred、后端重放/错槽/漏提交）仅编译。
外置构建、选择、binary hash及结果在`/private/tmp/koven-owner-own-call-evidence`；
固定代码头、精确测试文件及名单来源`2d79c4d6`的134普通集界定可重建输入。

固定代码头最终重建并复核147普通全部通过（前端137、后端10），0失败/忽略；
实际test artifact debuginfo=2，target沿用原owner目录，Cargo以jobs=2串行且locked/offline。
前端/codegen lib no-run均成功，含CI-only仅静态编译。两crate all-targets严格Clippy
`-W clippy::all -D warnings`、workspace all-targets check、fmt、docs880与diff通过；
尺寸base2d79c4d6通过，2303手写Rust、36旧超千行，原expression.rs1003未增，build.rs989。
旧drop_planner.rs1598仅+6实际交接hook，算法在新模块，不改额度或拆分政策。
正式双宿主CI、负控执行、sanitizer/资源计数仍未运行；未推送、PR或合main，旧oracle不退休。

## 已批准的有界 Shared receiver caller 路由（2026-11-10）

用户批准限定形状 Shared receiver caller 进入 refine，保留原清理规则。此段覆盖
SPEC-0358 对本有界形状未触发迁移的旧限制，不改写历史账本或 ADR。仅显式 own
非 Resource 普通 class 参数根、零实参 Shared receiver、整个 caller 单一直线正常
返回调用；完整 signature/body/drop subset 仍须通过。trigger 只读 typed receiver、
ownership 事实及精确 AST 形状，不依赖 protocol 2 Complete 或观察开关。
caller 与 callee 独立选择；原样 Node/run 只迁 run，不强迁 touch。旧生产者唯一
生成 Begin/End/NamedDrop，不增加 P2 receipt、不退休旧生命周期 oracle。隐式 this、
temporary、projection、two-phase、borrow-result、控制流与 replacement 不在新增范围。

先执行原样普通 receiver 生命周期测试的 caller route 红测，保留完整实体与顺序断言；
后续核 rooted/memory 双 Unit 和真实 frontend on/off 重分析等价。仅逐源审计普通
exact；所有 CI-only 只编译。本机默认 debug2、独占 borrow target，不运行全量、
native、故障或资源计数，不修改其他隔离树；本地分步提交，不 push。

首片进一步限定非泛型 caller、无类型实参的 receiver class/call instance，方法 owned
结果仅 Int/Unit，caller 为 Unit block。此限制与完整 subset 共同守门，不反向关闭
其它已迁移原因。型参边界由独立 AST/typed predicate 静态核验，非 Complete2 授权。

### 有界 receiver 路由实施与本地验收

隔离输入为 main `2d79c4d642a9a51d46a3ca4f74d43088442a59a5`，分支
`feature/spec-0341-bounded-receiver-route`；Spec 与原样能力红测提交9ed0fc19，
生产/测试代码头`09743f1df1d8da9e83322e24a24a818a2feaf128`。
新`refine/subset/receiver_route.rs`仅提供只读路由谓词，原完整signature/body/drop
检查之后与既有迁移原因作OR。它不读取观察表、coverage或protocol，不改任何
Begin/End/Drop生产者、receiver通用准入或callee路由，不连接P2receipt。

| 验收项 | 精确选择与输入 | 实际结果 |
|---|---|---|
| 原样能力红测 | `ssa::loan_region_ssa_tests::certified_receiver_region_has_real_unit_ssa_lifecycle`；9ed0fc19 | lowering成功后实际routed=[]、期望[koven.run.d1]；0通过/1失败/2839filtered |
| 原样红转绿 | 同一exact，独立窄trigger初稿 | 1通过/0失败/2839filtered；双入口原五指令实体/顺序oracle未改 |
| 最终普通回归 | 前述真实off实施八项，加下列receiver_route两项；09743f1d | 10个唯一exact全通过，各1通过/0失败/0忽略/2841filtered；双入口不另累计 |
| 新off路由等价 | `ssa::loan_region_ssa_tests::receiver_route::receiver_route_is_independent_of_real_frontend_observation` | 同names/types真正重分析off，完整旧facts副本比较；原off产品直接lower，on/off均只有caller迁移、无P2receipt，完整SSA/LLVM相同 |
| 新普通边界 | `ssa::loan_region_ssa_tests::receiver_route::neighboring_receiver_shapes_keep_existing_routes` | Boolean结果、Borrow参数、两次调用、非零实参四份普通源，rooted/memory均保既有Compatibility并验证SSA/LLVM |
| 严格featureClippy | `cargo clippy --locked --offline -p lang-codegen --features loan-observation-test-support --all-targets -j 2 -- -W clippy::all -D warnings` | 最终通过26.95s；全部CI-only仍仅编译 |
| 静态门禁 | fmt、docs、diff、尺寸`--base 2d79c4d6` | 通过；880Markdown，2300手写Rust/36旧超千行，无额度变化；新模块166/128行，subset960行 |

no-run统一命令为`cargo test --locked --offline -p lang-codegen --features loan-observation-test-support --lib --no-run --message-format=json -vv -j 2`。
仅用`CARGO_TARGET_DIR=/private/tmp/koven-exclusive-loop-prefix-target`与
`LLVM_SYS_211_PREFIX=/opt/homebrew/opt/llvm@21`，Rust1.96.0/LLVM21.1.8、macOS AArch64。
DEV_DEBUG/TEST_DEBUG/RUSTFLAGS/ENCODED覆盖均未设置，实际artifact为本隔离树src/lib.rs、
opt0/debug2/test=true、feature精确`[loan-observation-test-support]`，build-finished=true。
每轮完成编译后从manifest取binary，逐项exact list确认名称，再执行exact；不运行旧binary。

红测no-run1m34s；初稿build1 32.03s后原样绿，build2 24.75s后10项绿；独审提出
泛型与结果范围需更窄，父确认后收紧，build3 11.52s后同10项绿。首次严格Clippy
只发现新增测试name断言的cloned_ref_to_slice_refs，改为等价只读slice，未改生产；
Clippy2通过后最终build4 20.63s、10项全部重验。此前各次绿仅为阶段结果，不替代最终。
泛型排除静态核AST type_parameters、实例substitutions、nominal/call type_arguments；
未执行泛型负例，不把其它既有独立迁移原因反向关闭。初次普通sandbox ps被拒，随后
require_escalated只读进程核查成功；borrow target锁空闲后才编译，0401任务原样保留。
仓库根与docs下均未找到.agents技能目录；没有假称读取不存在的技能。

最终binary SHA256为
`cda49b080db87d479f1c5d2b9f5f3c731e6a8819ebc56ab76637c82f7a684fd6`，
十项换行名单SHA256为
`336e3083594f07a5c2363ad778419197dcf58d8ccc8833b7196005725830aac6`。
本机临时证据前缀`/private/tmp/koven-p2-route-final4`保manifest、selection与exact日志，
构建证据为`/private/tmp/koven-p2-route-build4.json/.stderr`；临时目录不承诺长期留存，
固定代码头和上述精确选择可重建证据，不将raw入库。

本轮只改codegen私有路由和测试，没有新增frontend API/feature/依赖或共享产物。
没有重跑旧484组合、workspace/默认CLI check、release、native、Linux、sanitizer、
资源计数、压力或任何CI-only（包括panic/人工破损）。真实off证据限Basic双Unit，
不外推常量capability；Int方法有原样实体oracle，Unit结果仅由既有subset及新谓词静态
准入，不声称新增Unit方法实跑。正式双宿主CI留父级集中PR；未push/PR/合main，
不退休任何旧oracle，不关闭Spec/D10/E2。

独立只读终审精确09743f1d无阻断：复核完整subset后的窄OR、泛型/结果边界及旧producer、
P4/frontend无diff；独立重算final4两个SHA并逐一核十条1pass/2841filtered日志、本树
artifact/debug2与20.63s、Clippy2 26.95s。审计没有代执行测试，不外推未运行范围。

### selected-owner Call 与窄 receiver route 联合普通验收（2026-11-10）

发布前只读核本地main/origin及远端main均仍a775568f，无新基线需要额外合入。
本批仅本地准备：从2d79c4d6另建隔离分支
`feature/spec-0341-own-call-receiver-combination-20261110`，保父链合入
owner `dee59c0c96ff2be51a122ab663bd34843af44ef5`与borrow `b4181cbc`。
实际合成代码头`4a1e10292101b32cf9d2912550ce7ea3aa894d34`，树
`0e98af9e84c5e3af563e6114973269d7ebff9200`。唯一Spec尾部冲突保留两侧真实历史，
无生产冲突、无额外修补；原冻结树不动，不单独发布或重复创建PR。

独立选择/helper准入复核：旧484加owner六项及borrow两项去重，准确492，
默认前端469（137 lib、331 direct、1 iteration），显式feature后端23。
新增exact来自前段owner147及borrow十项完整名单，全部仅执行普通有限源码；
邻接continue只作frontend分析，不执行循环。owner新native确实验证outer/next，
其余新增普通为真实分析/SSA/LLVM，无故障或计数校准。新旧CI-only、panic恢复、
人工破损/deferred、重放/错槽/漏提交及所有旧排除均只编译，不按模块filter执行。

P2本轮已是窄caller refine迁移，不能沿用旧Compatibility结论：完整subset仍必经，
真实on/off均要求此窄route，且不产生P2 production receipt；邻形保持旧路由。
owner Call真实pending移交、P/Q边角色、callee own清理与B原双ledger共同验收，
不把新Call产品当原B Bind/Drop产品，不宣称所有owner消费或receiver形状已迁移。

本轮默认debug2、四项profile/RUSTFLAGS覆盖未设置，原owner target串行，不copy/clean。
codegen no-run显式`--features loan-observation-test-support`，默认前端469不加feature；
其余Cargo选项仍`--locked --offline --message-format=json -vv -j 2`。编译后exact list
核名，再执行同binary exact并核成功集合/hash；默认CLI三crate features=[]另行验证。
外置证据`/private/tmp/koven-own-call-receiver-evidence`仅供本机追溯，固定提交及上述
名单来源界定可重建输入，不复用旧头绿灯。

本轮实际492普通全绿，0失败/忽略：feature codegen两组12+11互斥（19.39s/27.36s，
filtered2834/2835）；默认前端137 lib 1.94s、331 direct 0.32s、1 iteration 0.05s，
filtered451/10/218。codegen no-run1m36s、frontend no-run54.06s。旧484与新8全部在
新合成输入重编后执行，成功集合与精确选择相等；未发现联合生产兼容失败，无生产修复。
新P2原样实体oracle和真实off已核refine路由，不产生P2 receipt；新owner native普通输出
与P/Q入边/caller-callee清理均通过，不外推析构计数或资源无泄漏。

默认CLI新跑`cargo check --locked --offline -p lang-cli --message-format=json -vv -j 2`
8.44s成功，frontend/codegen/kovenc三个实际artifact features=[]、debug2/test=false。
独立证据审核492名单、分组成功集及四binary hash一致，feature图前后端仅启指定feature、
默认前端不启用，HEAD/tree无Rust漂移。codegen构建原始流的103条llvm-sys前缀文本
由既有窄允许解析处理，其余JSON及build-finished成功已核，无本轮解析失败。
两个纯读Python exact通过，未执行check_core全量或恢复故障stub。

最终严格feature双crate all-targets Clippy（`-W clippy::all -D warnings`）通过1m05s，
默认workspace all-targets check通过1m01s，均`--locked --offline -j 2`；fmt通过。
docs880、diff及尺寸`--base a775568f`通过：2305手写Rust、36既有超千行，未改额度。
最终仅追加本账本文档，无Rust再改动；原冻结树保持，owner target本批工作完成后释放。
本轮不push、不更新#410或另开PR，不合main，集中发布方式另定。
正式双宿主CI、所有CI-only负控/panic/Abort、sanitizer、压力/资源计数未本地执行；
真实off仍限已测Basic输入，不外推新Call端到端off、常量capability或所有shape；
P2无production receipt，不退休旧oracle，不关闭Spec/D10/E2。

### 已批准：变化 owner 两出口向唯一 String Return 移交（2026-11-10）

用户对“动态 owner 增加 P/Q 选中值直接作为 String 返回的受限路径；借用扩展限定
protocol2生产核验”回复“是的”。本片只做owner侧，基于已发布`a3bdf8d1`另建
`feature/spec-0341-owner-return-20261110`，保真父链，不修改发布头或借用树。

原固定Boolean参数、String字面量root、一次Replace后裸Break保持，函数返回String，
唯一尾句为`return held`。Phase3从真实Name Take与Return payload交付发布独立封印，
保持真实P/Q两个catalog及边角色；Phase4核返回表达式、payload槽及归属，旧Return
emitter唯一发出，当前函数不Drop选中值，调用方沿既有规则接收。原B/Call不变。
排除多根、一般循环、任意suffix、借用/捕获返回、Resource及语言/ABI扩展。

状态in-progress：先普通真实Return缺产品授权红测，再分步实现；Basic/Const双Unit、
两边SSA/LLVM/普通native及旧能力回归。故障/破损/计数等CI-only仅编译，不本地执行。
最终固定头独审与门禁后本地交付，集中合流PR，不自行发布、不退休旧oracle。

### 受限 String Return 实施与普通验收（2026-11-10）

合同`aa81bb2f`、真实红头`03849bfb`，基底为已发布`a3bdf8d1`。红源码已通过旧frontend、
SSA与LLVM，并有唯一实际String Return；旧emitter成功完成后观察的授权receipt为None，
而真实payload槽为Some(2)，同一普通exact据此失败。没有空collector、伪造产物或故障注入。

实现`5a294b69`，最终Rust输入`4e7f17b5`只追加两个公开动作字段的文档注释。
新增独立`UnitOwnerReplaceBreakReturnConsumerV1`及Take/Return动作；真实Name Consume
保留P/Q carrier，核root已移出state，按原Return cleanup完成后封印。仅完整Return
suffix匹配时允许owned String函数结果，原B/Call及其它loop的Unit门保持。
后端核同轮callable、Return expression、已选payload槽及旧deliver状态，旧finish_return
唯一发射成功后提交；禁止当前函数Drop已移交的选中root，caller沿旧规则接收和清理。

新增前端3 ordinary覆盖真实payload、同Unit跨source callable隔离、合法邻形不发布；
共用原compare保观察on/off完整旧Analysis，非Return后端off端到端声明。新增后端3
ordinary覆盖原红、Basic/Const双Unit完整SSA的P/Q入边到实际Return payload、返回函数
只清理detached P以及caller两次结果分别唯一Drop，另有普通native输出outer/next。
原B/Call和B/P4 sibling回归保留。native输出不是析构计数或heap/Resource证明。

选择集由上一own-call片已审147 ordinary追加本片6项组成，共153唯一exact：前端140，
后端13。新frontend破损/deferred与后端重放/错payload/漏提交守卫均为CI-only，只编译。
外置构建、名单、binary hash与结果位于`/private/tmp/koven-owner-return-evidence`；
固定代码头、三个测试模块及上一147名单界定可重建输入。不推送、不新开PR或合main，
正式双宿主CI、负控执行、sanitizer及计数仍待；旧oracle及SPEC-0341/D10/E2不退休。

最终Rust头重新no-run后153普通全部通过，0失败/忽略；两binary的实际debug2、名单与
完整成功集逐项核实。沿用独占owner target，Cargo jobs=2串行locked/offline，普通binary
另以test-threads=1运行。两crate严格all-targets Clippy（-W clippy::all -D warnings）、
workspace all-targets check、fmt、docs880、diff及尺寸basea3bdf8d1均通过：2310手写Rust、
36旧超千行，原expression.rs1003未增、build.rs990，无预算或政策修改。

固定`4e7f17b5`独立只读终审无阻断：产品/实际Return授权消费、旧唯一发射及B/Call
排他不变，manifest的codehead/tree、两binary SHA、实际debug2/features空集和140+13
成功集合均复核一致。审阅者未运行Cargo或行为测试。本片本地实现与普通验收完成；
上述正式CI与计数边界保持，不据此宣称其已执行或通过。


## 已批准的 protocol 2 receiver 生产核验（2026-11-10）

用户同意将既有生产核验从 protocol 4 扩到已进入 refine 的受限 protocol 2 receiver。
范围为前段已迁非泛型、显式 own 非 Resource class 参数根、零实参 Shared receiver、
Int/Unit owned 结果及单一直线正常返回 caller，且同轮 catalog 发布 Complete2。
不改 route，不以 Complete2 取代完整 subset。off/Partial/Unsupported/缺席保旧行为，
不新增语言拒绝、不授 absence、不退休 oracle。P4、B/Call owner 证据保持独立。

从真实 receiver producer 绑定 call、参数 owner Slot 和新 loan；稳定标记随 RStmt
移动，实际 SSA append 处核 RootPlace→Begin→DirectCall→End→原 NamedDrop 身份与
顺序，每个选中实例必须唯一闭合。callee entry loan 与 caller loan 各自局部，不强套
P4 source/result 角色、不回放手写五指令、不按 Span 事后补配；旧 producer 唯一发动作。
先执行原样合法 P2 缺 receipt 红测，再最小生产实现及 Basic 双 Unit 真 on/off、
边界、P4/B/Call 邻控。新增破损守卫仅 CI 编译、本机不执行；独占 borrow target、
默认 debug2、有限普通 exact，不跑全量/native/资源计数、不 push。

### Protocol 2 生产核验实施与本地验收

新隔离分支`feature/spec-0341-receiver-production-receipt`从
`a3bdf8d14d0db07ff3ba007755170819d45638e5`起步；范围与红测提交02088225，
代码头`fa8c547cdace757b8f2fa766137e2c294bfc0d89`。不改前片冻结树。
`receiver_receipt`独立准入/生产状态/SSA核验，RStmt的私有LoanTag显式区分
Receiver/Consumer，双归属为错误，P4模块本体未改。窄shape查询复用原predicate，
仅机械提取type_demands的无状态Checker初始化；原route条件和所有动作生产者不变。

| 验收项 | 实际选择及输入 | 结果 |
|---|---|---|
| 原样缺receipt红测 | 02088225；`ssa::loan_region_ssa_tests::certified_receiver_region_has_real_unit_ssa_lifecycle` | 实际lowering成功，receipt 0而要求1；0通过/1失败/2845filtered |
| 原样生产红转绿 | build2同一exact | 1通过/0失败/2845filtered；生产finish成功后才被observer看到，原五指令oracle保留 |
| 最终14普通exact | 下述精确选择、fa8c547c | 每项1通过/0失败/0忽略/2851filtered；14唯一测试，双入口与on/off不另累计 |
| Int身份与真off | 原样P2及既有receiver_route真off测试 | 同catalog原ID→caller/call→原参数owner/place/loan→真实InstructionId；同names/types真实重分析off，原产品直接lower，旧facts/SSA/LLVM相同，off无receipt |
| Unit结果 | `ssa::loan_region_ssa_tests::receiver_receipts::unit_receiver_receipt_closes_real_slots_and_observer_off_is_transparent` | Basic rooted/memory、真on/off均通过；Unit DirectCall无result，原owner唯一Drop，callee entry loan独立 |
| 同analysis隔离 | `ssa::loan_region_ssa_tests::receiver_receipt_siblings::receiver_consumer_and_owner_siblings_keep_production_receipts_separate` | P2/P4 action IDs、callable、call、SSA实体互异；B仅一次Drop授权，Call仅一次own String交付授权；SSA/LLVM通过 |
| 原邻控 | 前片四合法route边界、P4直接/Partial/sibling、B/P4及两项owner Call普通测试 | 未扩大route，无P2/P4角色混用，无重复Drop或callee授权变化 |
| 严格featureClippy | `cargo clippy --locked --offline -p lang-codegen --features loan-observation-test-support --all-targets -j 2 -- -W clippy::all -D warnings` | 通过50.93s；不执行测试 |
| fmt/docs/diff/尺寸 | `cargo fmt --all -- --check`、docs、diff、尺寸`--base a3bdf8d1` | 通过；881Markdown、2311手写Rust/36旧超千行，build997/emit983/subset992，无增长豁免变更 |

最终名单为前段有界receiver route的10个普通exact，加上本表Unit与同analysis隔离两项，
再加`native_tests::boxed_enum_tests::unit_source::owner_own_call_tests::`下的
`selected_owner_actual_call_requires_product_authorization`及
`selected_owner_call_both_units_preserve_path_values_and_callee_cleanup`。
后两项及既有B/P4虽处native_tests模块，逐源/helper核实只分析、lower、SSA/LLVM验证，
没有native执行、计数或注入。四个新增ci_only仅编译：外来catalog receipt、重复/缺角色、
重绑slot/错误callee、外来function instruction；snapshot仅其显式capture启用，普通
helper不触发。核验的是已产生的真实指令ID，不重造五条SSA操作。

每轮命令均为`cargo test --locked --offline -p lang-codegen --features loan-observation-test-support --lib --no-run --message-format=json -vv -j 2`。
唯一target为`/private/tmp/koven-exclusive-loop-prefix-target`；
`LLVM_SYS_211_PREFIX=/opt/homebrew/opt/llvm@21`，Rust1.96.0/LLVM21.1.8，macOS AArch64。
无DEV_DEBUG/TEST_DEBUG/RUSTFLAGS/ENCODED覆盖；实际本树artifact为opt0/debug2/test=true、
feature仅`loan-observation-test-support`且build-finished成功。先核进程/target锁空闲，
串行编译，每次从已完成manifest取binary、exact list核名再exact运行；不copy/clean/降档。

过程偏差如实保留：红测no-run1m21s；首次实现编译有两处admission引用层级错误，
不是能力红测；修复后build2 59.04s且原样绿。build3 30.60s后直接四项绿，
build4 31.22s后最终14项绿。一次编译尚未开始产出JSON时的只读检查未找到文件，
随后等待完成产物才执行测试，未运行旧binary。首次docs因receiver入口218行超200失败，
拆页后又暴露入口202行及README漏索引；将生产核验独立成职责页、复用原段链接并补索引
后881页通过，未压缩Rust或改额度。最终仅追加文档，不再改已测Rust。

最终binary SHA256：
`8c1a5972ee87d5035c2ec407887869a7630e13fe21c7aec082c6c8baf9a7d878`。
14项换行选择SHA256：
`fd2db2ec955231586fed63ff74f871681c562b272915e62b29dfc13d27ed9ca3`。
本机临时证据前缀`/private/tmp/koven-p2-receipt-final`包含manifest/selection/exact日志，
构建为`/private/tmp/koven-p2-receipt-build4.json/.stderr`；临时目录不承诺长期保留，
固定代码头与精确选择界定可重建输入，不将raw提交到docs。

没有运行任何CI-only、panic、Abort、故障、native、资源计数、压力、Linux、sanitizer、
全量CI、旧组合全表、workspace/默认CLI或release。未改变frontend API、feature或依赖。
P2真off实证限Basic双Unit；静态Complete2过滤与off空计划不等于完整absence。
本机绿不替代父级集中PR的双宿主CI；不push/PR/合main，不关闭Spec/D10/E2或退休oracle。

精确fa8c547c独立只读终审无阻断，确认P2/P4角色与原owner授权独立、窄shape未扩大，
重算两SHA与profile及14条普通exact日志逐项一致；同analysis sibling证据与代码相符。
审计未代执行测试。其后仅提交本账本与架构页面，无Rust漂移。

### Return 与 Protocol 2 receipt 集中合流验收（2026-11-10）

新隔离分支`feature/spec-0341-return-p2-combination-20261110`从已发布`a3bdf8d1`
保真合入owner`e70132f789ce4755b6ede618ac7b2ff8bdb1406f`与
P2`5b2bd01324f94c7a85bb97cac17bcc858102d91d`。main/远端main实核仍`a775568f`。
精确Rust合成头`b61e59572e2dedbf4cc2e10ff7a51f5f0160d2b4`，tree
`49cab56131d02d9d60e7e35c5f627b35c7a2ae0a`。refine.rs与build.rs自动合并保留双方
不同区域接线；仅Spec追加冲突，双方历史逐字保留。没有生产修补或额外测试。

旧492加Return六项与P2两项，去重为500普通exact，全部在本合成输入重编执行。
默认frontend为140lib＋331direct＋1iteration＝472，codegen显式
`loan-observation-test-support`为28。前端名单是上一批137lib加Return三项；
后端为上一批23加Return三项、P2 Unit真off及receipt sibling两项。
新项/helper已逐源审查：continue邻控仅分析，native仅常规两出口输出；
所有CI-only人工破损、错误payload、重放、漏提交、panic及旧十direct/四iteration
Abort和两项混合破损排除继续有效，未运行全lib或故障测试。

环境Rust1.96.0/macOS AArch64/LLVM21.1.8，沿用既有owner target，默认debug2；
无DEV/TEST_DEBUG或RUSTFLAGS/ENCODED覆盖、不复制target、不降档。
全部Cargo串行`--locked --offline -j 2`，LLVM前缀为系统llvm@21。
codegen命令`cargo test -p lang-codegen --features loan-observation-test-support --lib
--no-run --message-format=json -vv`加上述公共参数；编译1m26s，先关键20项23.51s，
再余8项25.30s。frontend命令`cargo test -p lang-frontend --lib --test
ownership_iteration --test unit_direct_call_loan_ends --no-run --message-format=json -vv`
加公共参数，编译1m10s；140/331/1分别1.15s/0.29s/0.06s。先exact list核真实名称，
再exact运行，成功集合、四binary SHA、实际profile/features均独立只读复核一致。
103条非JSON构建输出均为已限定llvm-sys前缀，解析器没有把其他文本吞作成功。

默认`cargo check -p lang-cli --message-format=json -vv`加公共参数通过8.52s，
实际frontend/codegen/kovenc三artifact均features=[]、debug2、test=false。
严格feature双crate all-targets Clippy（`-W clippy::all -D warnings`）54.31s、
默认workspace all-targets check45.37s、fmt、diff通过；均不执行测试。
docs881、尺寸`--base a775568f`通过：2316手写Rust、36旧超千行，无额度变更。
两项纯读Python exact核CI路由和显式测试feature选择通过。独审确认三处交集保全、
500唯一选择与实际成功集、20＋8互斥分组和四binary hash无阻断；审计未代执行测试。
临时证据前缀`/private/tmp/koven-return-p2-evidence`含manifest、选择、build与分组日志，
不承诺长期保留；固定代码头及各片名单界定可重建输入。

本批P2已有受限production receipt，旧“无P2 receipt”仅属前批历史；不扩宽route。
Return由旧emitter唯一发射，P2/P4及B/Call账本独立。真off仅已测Basic双入口，
不外推Const或Return所有形状；普通native不证明计数/泄漏义务。正式双宿主CI、
sanitizer、资源计数/压力及全部CI-only仍待新头正式验证，不沿用#410旧头绿灯。
本轮仅本地冻结，不push或更新#410，不合main，不关闭Spec/D10/E2或退休oracle。

### P/Q 条件透明 Group 实施合同（2026-11-10）

基于合流冻结`9de0bc94`，按用户继续并行本地开发的授权，补既有语言 Group 的透明运输，
不改变语言、ABI或ADR架构。仅原唯一Boolean参数Name的纯Group链可作为ReplaceBreak
条件；保留实际外层condition身份和真实false/Break边角色，原B/Call/Return唯一suffix、
一次Replace及bare Break、P/Q catalog和旧唯一emitter不变。不扩Not、短路、调用、一般
循环、多根、任意suffix或Resource策略。共享boolean_parameter同时支持Not，不能以
直接删除Name门的方式扩大范围。

状态in-progress：先以合法有限源码证明实际生产receipt缺失，再实现最小接线；覆盖
Basic/Const、两边SSA/LLVM/native和旧能力邻控。CI-only仅编译，不本地执行；所有
静态门禁、精确头独审及分步本地提交后交付，不推送、不触发远端CI、不改既有发布头。

### Group 条件实施、选择纠偏与验收（2026-11-10）

合同`2e9120da`，backend普通真实红`2fa5ebcb`，frontend测试`86e19477`，实现固定
`18599f8c`。红源码是正常有限的嵌套Group条件，旧SSA/LLVM已经成功、真实Return的
payload为Some(2)，但授权为None；同exact据此失败。新增产品不从AST补造P/Q，唯一
生产行为差异是Group链Name终点门，完整外层condition仍由旧Boolean helper核类型和
真实两分支，旧实际遍历、catalog、false/Break边与B/Call/Return emitter不变。

新增前端4普通覆盖1/2/4层Group、三种消费者、实际外层ID、原P/Q目录归属、旧Analysis
on/off对照及Not混合邻形；新增后端3普通覆盖真实receipt、Basic/Const两Unit实际入边
和Return payload/Drop、普通native输出outer/next。旧Group排除项迁为Group-Not或Not。
没有更改Guide、ABI、通用Boolean helper、旧清理或任何资源策略。

本轮发现并纠正旧名单的安全分类错误，不能把已发生的执行写成未执行：复用旧名单的
144项frontend批已执行完成，其中loop_replace_break模块的
`actual_replace_break_preserves_distinct_exit_owners`与
`replace_break_keeps_actual_source_and_analysis_identities`调用含foreign catalog查询
的旧helper。原16项backend批在终止前已有15项完成，其中
`bounded_replace_break_dual_units_same_literal_and_reanalysis_keep_receipts_local`
也含此查询。最后一项旧Return native被终止，不能声称整批通过；原日志全部保留。
这些是当时已定位的至少三项，不是完整误分类数；完整受影响范围由专项审计补充。
旧批整体不纳入本片纯普通验收，后续不复跑未重新审定项。public iteration的
`loop_replace_break_unit::replace_break_matches_both_public_input_routes`亦含原有foreign
查询，在本片执行前排除，只编译。以上查询均早于本片，未修改事实来掩盖旧分类失误。

重新独立逐helper复审的闭合集合仅16项：新Group前端4项、后端3项，旧own-call3项、
Return3项及B的original_source/physical_drop/absent3项。使用独立safe名单/日志，不覆盖
原批记录；未选择命名CI-only、故障注入、Abort或计数测试。本地计数、sanitizer、正式
双宿主CI仍未执行，不以native输出证明资源次数，不退休旧oracle。

固定代码`18599f8c`的上述16项已全部通过，0失败/忽略；实际defaultdebug2、features空集、
build-finished和两binary SHA/完整成功集合均由独立runner逐项核对。两crate严格
all-targets Clippy、workspace all-targets check、fmt、docs881、diff和尺寸门禁均通过：
2318手写Rust、36旧超千行，无预算或政策修改。原批及纠偏后的safe结果、构建与manifest
保存在`/private/tmp/koven-owner-group-condition-evidence`，owner target已释放。

固定`18599f8c`最终独立只读复核无生产阻断，16项safe名单与独立选择完全一致，
codehead/tree、两binary SHA及成功集合精确匹配。本片本地实现与纠偏后的受限验收
完成；旧批误分类专项审计不由此宣称完成，正式CI与资源证据边界保持。

## 2026-11-10 普通参数 P3 caller 路由与生产核验

用户授权持续本地开发后，本片分两个独立能力阶段；不以 nullable anchor 替代原样调用。
第一阶段仅新增非泛型、唯一显式 own String 参数、Unit caller 单一直线调用的路由触发：
无 receiver 的 Declaration 普通调用，唯一 Borrow 根实参，正常 owned Int/Unit 结果。
完整 subset 仍须通过，触发只读 typed/ownership，独立于 Complete3 与 observer 开关；
callee 独立分流，非目标既有 trigger 保留。先执行原样 route 红测，再最小实现、原测转绿并提交。
第二阶段才新增同轮 Complete3 的可选生产 receipt，绑定真实 owner Slot、Shared loan、
RootPlace/Begin/DirectCall/End/原 NamedDrop 指令与 catalog 身份；独立执行缺 receipt 红测。
沿用唯一 producer，不改变语言/ABI，不退休旧 oracle；off、Partial、Unsupported、缺席维持旧路径。
真实双 Unit 与 off 重分析、P2/P4/owner 邻控采用逐源审定 ordinary exact；所有 CI-only 破损仅编译。
本地保持 debug2、既定独占 target，不运行全量/native，不发布。验收结果后补，不由设计宣称通过。

### P3 两阶段本地验收账本

隔离树 `koven-named-loan-production-receipt-20261110`，分支
`feature/spec-0341-named-loan-production-receipt`；基线 `9de0bc94d3a8854350c1c5e18cd1053ad51bbadb`。
先只读确认原样普通 P3 走 Compatibility，因此未用 nullable anchor 冒充原样能力。
在本轮持续开发授权下明确增加精确 caller route，再独立接 receipt；无新语言/ABI决定。
Spec/route 红测 `4746a2d3`，路由实现 `b667a84d`，receipt 红测 `9174390f`，
生产与普通/CI-only测试代码冻结 `21f945bd03717ef99cb05bfafd58d95d25c2e1ae`。

| 阶段 | 实际结果 |
|---|---|
| route 红 | no-run 1m37s；原样 Complete3 分析、实际 lowering、SSA verifier、LLVM 成功后，routing=[] 与 [koven.run.d1] 不符；0 passed/1 failed/2856 filtered |
| route 绿 | no-run 26.42s；同 exact 1 passed/0 failed/2856 filtered，精确只迁 caller，不迁 callee；独立提交后才开始下一阶段 |
| receipt 红 | no-run 38.06s；实际 refine/SSA/LLVM 成功后，空观察 stub 返回 0 而期望 1；0 passed/1 failed/2857 filtered，不把 route 红重复算作 receipt 红 |
| 首次实现构建 | build1 因 helper 缺 LoanKind 导入、named predicate 可见性及两处引用层级共5编译错误失败；修正并移除旧 unused import，不是能力红测 |
| 初步 receipt 绿 | build2 1m22s；两个新能力 exact 各1 passed/0 failed/2857 filtered |
| 普通回归补齐 | 新 sibling 首次 fmt 因闭括号缺失失败，修复后 build3 30.66s；最终7个唯一 ordinary exact 各1 passed/0 failed/0 ignored/2864 filtered |
| 静态检查 | feature-enabled codegen all-targets 严格 Clippy 首次33.15s通过；fmt、diff、docs882通过；尺寸 --base 9de0bc94 通过，2325手写Rust、35历史超千行，无提高例外额度 |

生产 identity 闭合在独立 `named_receipt`；同 catalog 完整 Begin/EndCallLoan/Drop、
唯一原 loan/end/drop、argument contract 与 caller/callee 实例均核验。原 ordinary Borrow
producer 仅机械移到 `build/named_argument.rs`，保 adapt→RootPlace→Begin 顺序；
P2/P4 的生产实现模块不变。共用 statement tag 新增 Named 判别，跨家族冲突拒绝；
所有 loan finish/drop hook 在 `build/statements.rs` 依原顺序调用，owner授权顺序不改。
近限文件 subset996/build988/emit992/expression988；原expression1003降至988，
不通过压行或新增额度规避尺寸合同。

最终选择的5个新增 ordinary（前缀 `ssa::loan_region_ssa_tests::`）：
- `named_route::named_borrow_caller_uses_refine_without_migrating_callee`
- `named_route::named_borrow_requires_real_production_receipt`
- `named_route::neighboring_named_shapes_preserve_legacy_routes_without_receipts`
- `named_receipts::named_int_and_unit_receipts_preserve_real_observer_off_products`
- `named_siblings::named_receiver_consumer_and_all_owner_siblings_keep_receipts_separate`

另重新逐源/helper审阅并执行同前缀 `certified_consumer_region_has_real_unit_ssa_lifecycle`
与 `certified_receiver_region_has_real_unit_ssa_lifecycle`，保留原精确生命周期oracle。
七项只调用普通 parsing/analysis、真实 lowering、SSA verifier、LLVM rendering 和只读
receipt observer；无 native、计数、故障开关、foreign-ID 查询或改写分析产品。
同analysis sibling直接包含P2/P3/P4/B/Call/Return来源，不复用旧owner测试白名单。
Int/Unit各在Basic rooted/memory两入口真实重analyze off，原off产物直接lower；旧facts
比较仅副本归一，SSA/LLVM逐字比较。入口/返回型/on-off分支不另累计测试个数。
四个邻形为Borrow根、两次调用、Boolean结果、额外Boolean参数，保旧路由且无P3 receipt。
泛型、投影/临时值、控制等排除保持静态守门，不宣称已逐例执行。

构建命令固定 `cargo test --locked --offline -p lang-codegen --features loan-observation-test-support --lib --no-run --message-format=json -vv -j 2`；
严格命令 `cargo clippy --locked --offline -p lang-codegen --features loan-observation-test-support --all-targets -j 2 -- -W clippy::all -D warnings`。
全部 Cargo 串行使用 `/private/tmp/koven-exclusive-loop-prefix-target` 与
`LLVM_SYS_211_PREFIX=/opt/homebrew/opt/llvm@21`（实际21.1.8）；未设DEV_DEBUG、TEST_DEBUG、
RUSTFLAGS或ENCODED。最终JSON确认当前隔离树lang_codegen test artifact、opt0/debug2、
feature仅loan-observation-test-support且build-finished=true；先按实际binary --exact --list核名再运行。

最终binary `lang_codegen-2ac0a1105ce1786c` SHA256：
`8ad7a6e1f69aa29af930d08af8f85da1294396a97144beff2846baf0420e340b`；
七项selection SHA256：`66c84329128670792083921718811121ce1265f2b36eeb6b48373a8c69e823a4`。
临时证据 `/private/tmp/koven-p3-final-{manifest.json,exact.log}` 与
`/private/tmp/koven-p3-final-selection.txt`、`koven-p3-receipt-build3.{json,stderr}`，
两红证据分别 `koven-p3-route-red-*`、`koven-p3-receipt-red-*`；原始输出不入库，
这些临时文件不承诺长期保留，固定提交源码与本账本摘要为可重做依据。

新增4项CI-only（foreign catalog、重复/缺阶段、重绑/错callee、foreign instruction）
仅编译，未执行。没有继承旧含foreign查询的owner ordinary名单；本轮没有执行任何
旧owner exact、全量、native/resource、Abort/panic/人工破损、release、Linux、Constant off，
不宣称全借用覆盖/absence/旧oracle退休或发布验收。只做本地提交，不push/PR/合并；
0401其它树构建未改动。本轮普通源没有新增语言拒绝。

独立只读终审对精确代码头 `21f945bd` 无阻断：核路由 typed/own guard、同 catalog
唯一 loan/end/drop 与 contract、原 Borrow producer 机械抽取、三族 tag 互斥、真实
SSA 冻结身份和五角色、callee 独立及 shared owner 顺序；重新审阅普通 helper 无
foreign 查询或故障注入。独审重算 binary/selection 哈希与七条日志、本树 debug2
artifact 均一致；审阅者未编辑或执行测试。代码冻结后仅追加文档，不重跑无变化 Rust。

## 2026-11-10 Group 与 P3 联合验收及旧名单分类纠正

本地隔离合流代码头 `3d18f6b5310130ef13dd3f202ae760a0f4788d02`，双亲为 Group
`fd4179d4ab5152f4dc1f6cb2e62117e906e4190a` 与 P3
`6d11c3059579c2a151eb7233bbb7e25c8399409d`，共同基底 `9de0bc94`。
本地 main、origin/main 与远端 main 均核为 `a775568f`。唯一冲突是本 Spec 尾部追加，
保留两父完整原文；没有 Rust 冲突或语义修补。独立只读复核确认双父、干净代码头、
两段追加逐字保全与无冲突标记，未代称独审运行测试。

### 旧行为事实与分类分离

专项逐源/helper 重审旧 500 加 Group 新 7 的 507 个唯一 exact，不以旧审核标签或
总数减法推安全：443 项可作普通候选（frontend lib90、direct323、codegen30），
54 项含真实 foreign catalog/analysis 句柄查询而排除（lib44、direct8、codegen1、
public iteration1）。另10项有限笛卡尔矩阵已读完源码/helper、未见同类注入，但其
本地执行范围仍待核，不在本轮执行；不自行用某个组合数定义压力阈值。

旧 500 执行成功是历史事实，但不能再称全部纯普通。按原成功日志与新排除集精确
求交，Group 原被中止批的144个 frontend 成功中有44项此类混合测试，15个 backend
成功中有1项，共45项实际执行；末项旧 Return native 中止。先前至少三项只是当时
初定位范围，不是最终完整数。保留所有旧日志，不将混合项改称未运行，不拆旧测试
凑普通计数，也未为此次分类修正改动生产。

审计产物为 `/private/tmp/koven-selection-reaudit-final.json`，SHA256
`5baa46a3d4dedf8b32348251fb3ee298ac328dd9be20b55493f46bb9093051c7`；
逐项包含来源、排除位置或独立 helper 记录及源码 hash。direct331 与 choice31 分项
报告、普通白名单保存在同目录。它们是临时复核材料，不承诺长期保存；固定源码和
本节摘要是可重做依据。未选项没有因出现在同一源文件而获得执行许可。

### 本次合成输入的窄普通集合

不重跑旧大集合。采用前节独立重审 Group16 加 P3七项，去重后23项：frontend4、
codegen19。P3七项为上节五个新增 exact 加原 P2/P4两生命周期 exact，与 Group16
无重名。所有新 helper 在合成源码复读；Group 原 native/source helper 未变，P3
观察器仅记录真实生产结果，off 使用真实重分析产品；没有 foreign 查询或人工故障。

两crate按 `cargo test --locked --offline -p <crate> --lib --no-run --message-format=json -j 2`
串行重编；codegen显式加 `--features loan-observation-test-support`。实际编译耗时
1m21s、31.41s，JSON核 build-finished、test artifact和 debug2；前端 features=[]，
后端仅该测试 feature。随后 binary `--test-threads=1 --exact <完整23项分组名单>`：
frontend4于0.09s、codegen19于69.76s全部成功，0失败/忽略，逐项成功集合与选择完全
匹配。正常 native 只核实际输出，不声称资源次数；P3仍不宣称 native/Constant off。

固定 tree `d1dd8d6ce6bc981c6cd9abf5a3694c903f636fec` 的构建、选择、binary SHA与
精确成功日志保存在 `/private/tmp/koven-group-p3-evidence/manifest.json` 及邻接日志。
本轮未执行54项混合、10项范围待核矩阵、任何命名 CI-only、人工损坏/故障、panic
恢复、Abort、资源计数、压力或 sanitizer。所有这些守卫仅随 no-run/静态门禁编译。
未执行剩余普通候选，不以443候选数冒充本轮行为通过数；未运行正式CI或Linux。

静态门禁同代码头串行通过：默认 `cargo check --locked --offline -p lang-cli
--message-format=json -j 2` 8.23s，frontend/codegen/CLI三crate features=[]且debug2；
双crate显式 `lang-codegen/loan-observation-test-support` 的 all-targets Clippy
`-W clippy::all -D warnings` 46.84s，workspace all-targets check 34.73s，fmt、
docs882与diff通过。尺寸以main `a775568f`为基底，2327手写Rust、35历史超千行；
未调整profile/预算或清理target。全部Cargo复用既有owner target并串行；四项
DEV/TEST_DEBUG、RUSTFLAGS/ENCODED覆盖均未设置。保持active与Partial边界，未push、
新建PR或合main；本地结果不代替下一精确发布头的正式CI。

## 2026-11-10 单临时 String P1 路由与生产核验

持续本地开发授权下，首片仅无参数、非泛型 Unit caller 的单一直线普通 Declaration
Borrow调用，唯一实参为原始纯文本 String literal，正常 owned Int/Unit 结果。
Complete-empty、多临时借用、插值、控制、投影、borrow-result及其它形状不由本片新增路由。
原样尚走Compatibility，先独立typed/ownership精确trigger route红→绿，再独立缺生产
receipt红→绿；完整subset及既有非目标trigger保留，不靠nullableanchor，不强迁callee。
第二阶段只消费同轮非空Complete1；真实形成→RootPlace→Shared Begin→DirectCall→End→
原TemporaryDrop六角色核验，原producer唯一，不重复发动作、不按Span后扫。
off/Partial/Unsupported/缺席保原路径，不发布absence，不扩大语言ABI或全callable覆盖。
普通来源/helper重新审核，双Unit真off及少量必要邻控；所有CI-only负控仅编译。
实际红绿/门禁与偏差后补，保持默认debug2/既定独占target，仅本地提交不发布。

### P1 两阶段本地验收账本

隔离树 `koven-temporary-loan-production-receipt-20261110`，分支
`feature/spec-0341-temporary-loan-production-receipt`，基线 `1744661eef2eeb770284812cf0688349a40b95c6`。
原样 `inspect("temporary")` 的无参 Unit caller 经只读核实仍走 Compatibility；
依持续开发授权先新增精确 typed/ownership 路由，不以 nullable anchor 代替原样。
Spec/route 红测提交 `485753ee`，route 独立转绿提交 `8da8c3ce`，receipt 独立红测
提交 `cc04d6fb`，最终代码冻结 `22d2489a6019312d9a23c025cbbb42cf9c278fa2`。

| 阶段 | 实际结果 |
|---|---|
| route 红 | no-run 1m30s；原样 Complete1、lowering、SSA verifier、LLVM 均成功后 routing=[] 与 [koven.run.d1] 不符，0 passed/1 failed/2868 filtered |
| route 绿 | no-run21.96s；同 exact 1 passed/0 failed/2868 filtered，精确只有 caller迁移、callee保持独立；先提交再开始receipt |
| receipt 红 | no-run20.59s；真实refine/SSA/LLVM成功后观察stub为0、期望1，0 passed/1 failed/2869 filtered；不把route红重复计算 |
| receipt 初绿 | build1 46.88s首次编译成功，两个能力exact各1 passed/0 failed/2869 filtered |
| 普通回归候选 | build2 28.47s；8唯一ordinary全部通过，含Int/Unit/空字符串真off及七族sibling |
| oracle加强 | build3 24.78s只编译、未执行；补明确Complete-empty0action/双literal6action后，再按独审建议补固定literal字节和实际String Shared/Int/Unit类型 |
| 最终输入 | build4 22.97s；8唯一ordinary各1 passed/0 failed/0 ignored/2877 filtered，双入口、三来源和on/off分支不另累计 |
| 静态门禁 | feature-enabled codegen all-targets严格Clippy首轮47.87s通过；fmt/diff/docs883通过；尺寸--base1744661e通过，2335手写Rust、35历史超千行，无新额度或例外 |

本轮无非预期Rust编译/测试失败；route红首版注释误写P3，在route实现提交修为P1，
未改变原样输入或失败断言。Complete1的“非空”指三action区间，不要求literal bytes非空。
零action Complete-empty与六action双literal均保留原route、无receipt，未把空表当absence。
普通Boolean结果与额外Boolean caller参数也保原路径；其它泛型/投影/插值/控制等
排除保持静态guard，不宣称已逐例运行。Partial不选为静态守门，未造人工Partial运行。

production在原decode_plain后按实际expression ID激活，在原StringLiteral push取得owner
Slot；adapt后核同Slot和temporary表，原RootPlace→Shared Begin→普通Call→End→Drop
唯一生成动作。独立同catalog三action、唯一旧loan/end/Drop index、完整argument contract、
source token/callable/call实例→FunctionId均核验；actual append冻结六InstructionId及
owner/place/caller-local loan，callee entryloan独立。Temporary/Receiver/Named/Consumer
四族tag互斥；LoanReceipts仅聚合独立tracker，避免共享statements参数持续增长，不合并
角色计划。Alias/current/defined、owner B/Call/Return授权顺序与唯一cleanup均保持。
近限文件subset998/build989/expression989/emit980；新增主体独立模块，不机械压行。

最终选择（统一前缀 `ssa::loan_region_ssa_tests::`）：
- `temporary_route::temporary_borrow_caller_uses_refine_without_migrating_callee`
- `temporary_route::temporary_borrow_requires_real_production_receipt`
- `temporary_route::non_target_temporary_shapes_preserve_routes_and_no_receipts`
- `temporary_receipts::temporary_int_unit_and_empty_string_preserve_real_observer_off_products`
- `temporary_siblings::temporary_named_receiver_and_owner_siblings_keep_receipts_separate`
- `certified_consumer_region_has_real_unit_ssa_lifecycle`
- `certified_receiver_region_has_real_unit_ssa_lifecycle`
- `named_receipts::named_int_and_unit_receipts_preserve_real_observer_off_products`

八项/helper均在本轮重新阅读：ordinary parsed/analyze调用真实FE，后续只SSA verifier、
LLVM rendering与只读receipt观察，无foreign查询或产品破损，未激活故障注入，未执行native。
新sibling在同analysis直接包含P1/P2/P3/P4/B/Call/Return源码，未复用旧owner测试集合。
Int/Unit及空字符串在Basic rooted/memory双入口真正重新analyze off；原off产品直接
lower，legacy比较只归一化副本，完整SSA/LLVM逐字相等。固定literal字节、StringOwner、
caller/callee Shared String target和Int结果实际类型同时核对，Unit无结果。
未继承旧331的323safe8excluded等名单，也未执行旧owner exact。

构建命令固定 `cargo test --locked --offline -p lang-codegen --features loan-observation-test-support --lib --no-run --message-format=json -vv -j 2`；
严格命令 `cargo clippy --locked --offline -p lang-codegen --features loan-observation-test-support --all-targets -j 2 -- -W clippy::all -D warnings`。
Cargo串行使用 `/private/tmp/koven-exclusive-loop-prefix-target` 与
`LLVM_SYS_211_PREFIX=/opt/homebrew/opt/llvm@21`（实际21.1.8）；启动前锁空闲，
DEV_DEBUG/TEST_DEBUG/RUSTFLAGS/ENCODED均未设置，未复制/clean/降档target。
最终JSON确认当前隔离树src/lib.rs、opt0/debug2/test=true、feature仅
loan-observation-test-support、build-finished=true；逐项按该binary --exact --list核名后执行。

最终binary `lang_codegen-2ac0a1105ce1786c` SHA256：
`432c0ead9b663acacb85c37b8d9e48cd6535e56702305900626b90e040bcda92`；
八项selection SHA256：`f04f4f375d85b150686dfd310d2f158aff688b23ca33eeace084087fa109dea8`。
临时证据 `/private/tmp/koven-p1-final-{manifest.json,exact.log}`、
`/private/tmp/koven-p1-final-selection.txt`、`koven-p1-receipt-build4.{json,stderr}`；
两红为 `koven-p1-route-red-*`、`koven-p1-receipt-red-*`，build2候选结果保留phase2前缀。
原始输出不入库；临时文件不承诺长期保留，固定提交源码/本账本摘要为重做依据。

新增5项CI-only：foreign catalog、重复/缺阶段、重绑/错callee、foreign instruction、
缺失/变更formation，只编译未执行。没有全量/native/resource/Abort/panic/人工破损/
压力/release/Linux/Constant off验收，不宣称全借用覆盖、全callable Complete或旧oracle退休。
ConstEnabled下raw String不是constant selection的静态证据已核，未由此宣称实跑。
没有语言ABI或新清理责任变化；0401及其它树进程保持不动，仅本地提交，不push/PR/合并。

精确代码头 `22d2489a6019312d9a23c025cbbb42cf9c278fa2` 经独立只读终审无新增阻断；
25个相关源码/helper字节与冻结提交一致，八项普通来源及helper重新核验，实际build4
artifact、debug2/feature、八条exact日志及binary/selection两SHA均匹配。测试编译包含
默认未激活的故障钩子；普通helper未设置capture或fault，新五项CI-only仅编译。
独审未执行测试；终审后仅修正文档措辞与补充本账本，不改Rust或重复执行测试。


### P/Q 条件纯 Group/Not 链实施合同（2026-11-10）

基于合流冻结`1744661e`，按持续并行本地开发授权，补既有Boolean取反语义的受限P/Q
运输。原唯一Boolean参数Name外只允许Group/Not链，保每层精确Boolean和原实际最外层
condition身份。产品false出口仍对应完整条件求值false时的P，Break仍对应实际进入body
完成Replace后的Q；不按参数真假或Not奇偶交换catalog，不改语言/ABI或旧emitter。
原唯一B/Call/Return suffix、一次Replace后bare Break、原清理/瞬时Inout不变；不扩
短路、调用、一般循环、多根、Resource、任意suffix或未定语言设计。

状态in-progress：先普通实际Return receipt红测，再最小生产接线；Basic/Const、奇偶
取反两边SSA/LLVM/native及重新逐helper审定邻控。旧污染白名单不继承，foreign查询、
损坏/故障、Abort、计数、压力只编译。精确源码hash、固定头独审、静态门禁与本地分步
提交后交付；不推送、不触发远端CI，不绕过真实工具审批。

### 纯 Group/Not 条件实施与受限验收（2026-11-10）

合同`0eaf6c31`，普通真实红`aaefbeee`，frontend红源`9fe211d9`，生产固定`a5554f6e`。
原合法`while(!flag)`已通过旧SSA/LLVM，实际Return payload为Some(2)，但receipt为None，
同exact据此失败。唯一生产变化是移除ReplaceBreak独立Group-only门，仍调用既有
boolean_parameter核每层精确Boolean、仅Group/Not链到原唯一Name参数及两真实outcomes。
未改条件求值、observer/while/If权限、P/Q catalog、backend emitter、cleanup或ABI。

旧While标记Unsupported前已保存P前缀，随后实际Read完整条件；Copyable参数/Not不动
held，entry再核原P。进入body完成Replace/Break才给Q，完整条件false出口仍给P，
不按Not奇偶交换角色，也不从条件AST补造状态。普通证据保实际外层condition Span/ID。
相邻`!!`是一个非空断言token，不能冒充两个Not；本片偶数前缀使用`! !flag`或`!(!flag)`。

新增frontend3覆盖B/Call/Return的7种实际Not/Group链，复用参数化Group helper而不调用
旧污染check；负邻形迁至逻辑/比较非候选，原断言保留。新增backend3核实际Return
receipt、Basic/Const双Unit完整false/Break入边和payload/Drop、1/2/3个Not的普通native
输出：奇数next/outer，偶数outer/next。输出不代表析构计数或heap/Resource证明。

本片重新逐helper核并显式列22项：frontend7，backend15（Group3、Not3、ownCall3、
Return3、B original_source/physical_drop/absent3）。不读取旧污染名单构建选择集；旧
public iteration、B dual-unit mixed、loop_replace_break污染check及所有foreign查询、
故障/破损、Abort、计数、压力路径仅编译。真实源码/配置2985文件SHA、helper闭包说明、
精确名单、构建及结果存于`/private/tmp/koven-owner-negated-condition-evidence`。

首轮22项中7前端及14后端通过，1项复用Return SSA oracle失败，原日志保留于attempt1。
这暴露的是旧oracle把最终SSA when_true固定当body的假设：既有condition_internal对Not
递归交换两个目标，最终branch读取的参数truth不等于源码完整条件truth。`6aa11488`
仅修正测试：要求两个edge中恰有一个target为真实唯一RootReplace所在block，以此定位
body/false，不用Not奇偶或catalog顺序猜角色；原P/Q入参、实际owner槽、join/payload、
本地/caller Drop断言全保留。所有Not奇偶native首轮已绿，未因此修改生产或删除失败项。

`6aa11488`重建后22项全绿。严格Clippy随后要求将同一Boolean helper的None早返回写为
`?`；`7ee70464`仅作此等价传播，不屏蔽lint、不改变门。前头22项成功及Clippy原报告
保留于pre-clippy-style；最终代码重新按同一22名单构建验证，不混用前头binary/hash。

最终Rust输入`7ee70464`重新构建并执行22项全部通过，7前端+15后端，0失败/忽略；
精确名单与完整成功集合一致，两binary实际features空集/defaultdebug2及SHA已核实。
2985源码/配置hash再次逐文件对照真实bytes全部匹配。严格两crate all-targets Clippy、
workspace all-targets check、fmt、docs882及diff通过。正式双宿主CI、故障/foreign/计数、
sanitizer和资源证明未在本片执行；前端on/off对照不外推后端off端到端，不退休旧oracle。

最终`7ee70464`独立只读终审无阻断：生产及oracle差异、代码tree、源码清单SHA、两个
binary实际SHA、7+15成功集合和debug2/features空集均复核匹配；审阅者未运行测试。
尺寸基于`1744661e`通过，2329手写Rust、35旧超千行，无预算变化。本片本地实现与
受限验收完成，owner target已释放，不推送、不发布新PR或改main，正式CI边界保持。

### Not + P1 保父链本地合流验收（2026-11-10）

持续本地开发授权下，将冻结P1 `bd52245283ff39e5f4fcca6bc610aa4ea411c771` 与
Not `d4351ce8cc1c25cac306e70ac32c8550f32b3a1c` 合于独立分支，公共祖先
`1744661eef2eeb770284812cf0688349a40b95c6`。代码冻结双父merge为
`d32a03b720adfcd099a8536571c599764460669a`，不squash/cherry-pick、不丢父链。
唯一冲突是本Spec两端追加段：核两父均保共祖完整prefix后，保P1全文与Not新增尾段，
另加一个分隔空行。Rust无冲突或合流后改写，架构入口及两线事实保留各自实际并集。
原两树clean；main已有proposal/docs未提交工作保持原样，没有纳入或清理。

本轮重新逐source/helper核联合30唯一exact，不继承旧500或331安全标签：
- frontend7：`replace_break_grouped_condition`四项与`replace_break_negated_condition`三项。
- codegen Not15：`owner_group_condition_tests`、`owner_negated_condition_tests`、
  `owner_own_call_tests`、`owner_return_tests`各三项，以及B下游的
  `bounded_replace_break_original_source_reaches_existing_ssa_and_llvm`、
  `bounded_replace_break_physical_drop_has_product_authorization`、
  `bounded_replace_break_absent_optional_product_keeps_ordinary_lowering`。
- codegen P1八：上一P1账本逐项名单原样，无重名；含真正P1/P3 off与P2/P4实际lifecycle。

FE闭包只复用loop原SOURCE/declaration，不调用旧污染check；Fixture从同轮实际源码
构造，compare只归一化比较产物。B旧dual/reanalysis、public iteration及foreign查询
未选。UnitSource使用本轮原始products，四项普通native只链接运行有限输出程序：
Group2、Not6、ownCall2、Return2，共12次程序，不计作额外test或析构/资源证明。
P1 helper只核真实SSA/LLVM与生产receipt，off重新analyze后原样lower，比较只修改clone。
普通helper未激活capture/fault；所有CI-only、人工破损/注入、Abort、counter、压力和
范围待核矩阵仅编译不执行。独立审阅者逐源复读30项闭包并核两父非交集字节保全。

| 本轮门禁 | 实际结果 |
|---|---|
| 默认CLI依赖图 | `cargo check --locked --offline -p lang-cli --message-format=json -j 2`，8.48s成功；本树frontend/codegen/CLI实际artifact features均[] |
| feature no-run | `cargo test --locked --offline -p lang-frontend -p lang-codegen --features lang-codegen/loan-observation-test-support --lib --no-run --message-format=json -vv -j 2`，1m26s成功 |
| 精确普通执行 | 从上项JSON取两binary，各项先`--exact --list`核名再单项执行；frontend7各1pass/598filtered，codegen23各1pass/2880filtered，全部0failed/0ignored |

测试两artifact均来自本合流树，opt0/debug2/test=true、feature仅
`loan-observation-test-support`。owner target独占串行为
`/private/tmp/koven-choice-replace-forwarding-target`，全程LLVM prefix为
`/opt/homebrew/opt/llvm@21`；未设DEV_DEBUG/TEST_DEBUG/RUSTFLAGS/ENCODED，未复制、
clean或降档target，未用borrow target，0401进程38543及其它进程未触碰。

过程偏差：默认sandbox的只读ps被OS拒绝，随后按已授权执行前核查用途以
require_escalated重试同一未执行查询成功，没有绕用另一入口。默认CLI检查本身成功，
初版证据脚本误期望target名`lang_cli`而assert失败，实际CLI target为`kovenc`；按
三crate的本树source路径重新核features[]，保留错误说明，不冒充编译或能力红测。
该shell用分号，assert后feature no-run已开始；只编译、没有未经审定执行测试。
本轮无Rust编译或普通exact失败，没有重跑旧全量集合。

frontend binary SHA256：`12dabf70ab12986f4f78abc71564fc1ea41660241e8a826fe3d2622dd767f0cd`；
codegen binary SHA256：`4254d0d26d992ea84f40aad730430e2165cc4f7431c98500b7fe77e08de85efd`。
frontend selection SHA256：`9f6cacf6c3e59a3be25c1a04b7dce078970a0d1f12a0d5715385da6dbcef329f`；
codegen selection SHA256：`ac5173db585a774d3f3e29e005aa66d95b091210b5cb0c936aff8f3f6802694f`。
临时证据目录`/private/tmp/koven-not-p1-combination-evidence`含manifest、两selection、
逐项日志、默认CLI/feature JSON及2344源码/配置hash；原始输出不入库，不承诺长期保留。
固定双父源码和上述选择边界为重做依据。本次不运行正式CI、Linux、sanitizer或release，
不退休旧oracle、不扩语言ABI或释放职责，不push/PR/发布。

冻结代码后静态门禁均成功：两crate feature-enabled all-targets严格Clippy（显式
`-W clippy::all -D warnings`）42.15s；默认`cargo check --locked --offline --workspace
--all-targets -j 2`35.81s；`cargo fmt --all -- --check`、docs883与diff通过。
尺寸`--base1744661e`通过，2337手写Rust、35历史超千行，不增例外或额度；仅两父
原有注册/接线增长，无合流新增Rust。静态门禁后两test binary重算hash仍与manifest相同。

精确`d32a03b720adfcd099a8536571c599764460669a`独立只读终审无阻断：2344源码/配置
bytes、两binary/selection SHA、manifest与原始feature JSON、默认CLI三个features[]
均匹配；两日志7+23逐项等于审定名单，无额外test。四native exact内12次普通运行仍
只计四tests，静态日志和实际耗时一致。终审者未运行测试；终审后仅追加本账本，
不改Rust、不重编。owner target锁空闲，原两树与main工作区保持原样。

### 2026-11-10 双临时 String P1 路由与生产核验合同

基于冻结Not+P1合流`c956f3bf`，持续本地开发授权内，将原单literal首片精确扩至两个
独立raw Text-only String literal。无参非泛型Unit caller、唯一普通Declaration Borrow
call、无receiver/控制/替换/borrow-result、正常Owned Int/Unit及完整subset门保留。
两个typed argument_index与parameter_index均等于源码位置；命名实参重排、三及更多
literal和Complete-empty仍排除，不反禁其它既有route。typed/ownership触发不看证书/off。
同字节literal仍是不同expression、temporary owner、place及loan，不得以字节合并。

先原样双literal route真实红→最小绿并提交，再receipt缺证真实红→生产核验绿。
旧producer已证实按源码顺序逐个形成StringLiteral/RootPlace/Begin，再唯一DirectCall；
Refine按实际loan列表逆序End，Drop仍按原owned.drops发布顺序。FE原pending loan和
pending temporary分别逆序结束，既有Complete1验证器要求Begin源码顺序、End逆序、
Drop逆序；核验遵循这些既有路径，不为receipt重排任何动作。

同轮非空Complete1六action归一份call-level计划，含两个独立operand状态，保留原ID、
完整旧loan/contract/Drop index与callable/instance身份。稳定语句tag核实际形成及
共享call完整参数向量；一份receipt闭合十一真实指令、六实体，callee参数loan独立。
单literal作为原有N=1路径继续六指令/三实体，不新造protocol或手写指令回放。
observer-off/Partial/Unsupported/缺席保旧产品，不造absence，不退休oracle或改语言ABI。

普通有限双入口测试覆盖不同字节、相同字节、空字节、Int/Unit、真实重新分析off与原
single/其它家族少量邻控；每项/helper重新审查。foreign/故障/Abort/counter/压力及待核
矩阵只编译，绝不本地执行。默认debug2、借用target独占串行、精确binary/profile/名单
账本、独审及分步本地提交，不push/PR；不将main新头或其它人的dirty docs带入。

### 双临时 String P1 实施与受限验收（2026-11-10）

基线`c956f3bf`；Spec及route红输入`b9b2154a`，独立route实现`d761777f`，receipt红输入
`6d8de21e`，生产初绿`73d22163`；最终源码冻结
`03e110f5370f53907fd46ef29e8d24df8a9708b1`。原单literal无须重做能力红，本片两红分开：

| 阶段 | 实际结果 |
|---|---|
| 原样双literal route红 | no-run1m23s；FE Complete1六action、Compatibility lowering/SSA/LLVM成功后，routes=[]与[koven.run.d1]不符；0pass/1fail/2881filtered |
| route绿 | no-run25.54s；同exact1pass/2881filtered，caller独立迁移，callee不强迁；提交后才增加receipt能力测试 |
| receipt缺证红 | no-run16.20s；实际refine/SSA/LLVM与route断言通过后receipt0!=1；0pass/1fail/2882filtered |
| receipt初绿 | 首版生产no-run52.75s成功；同exact1pass/2882filtered，没有改旧producer |
| 最终普通集合 | 新no-run29.84s；12唯一exact各1pass/0fail/0ignored/2886filtered；双入口、on/off及多source不额外累计 |
| 静态门禁 | feature-enabled codegen all-targets严格Clippy首轮31.93s、fmt、docs883、diff通过；尺寸--basec956f3bf通过，2339手写Rust/35历史超千行，无额度变化 |

route只增加恰双、source/parameter索引相同的raw纯literal，完整subset与所有旧trigger
保留；原双literal非目标测试准确改成三literal九action，Complete-empty仍零action。
route阶段的receipt admission临时保单literal选择，避免把未接能力变内部拒绝；receipt
实现提交中移除该过渡guard，最终single/pair分别闭合，未把过渡空receipt当最终完成。

P1改为一call-level Plan+Vec<Operand>，保三/六同catalog原IDs，各operand核完整原loan、
argument contract、独立expression/Temporary、唯一旧Drop index。stage为usize，映射
实际Literal/Root/Begin交错与唯一Call、分别逆序End/Drop，不改变任何发动作或清理顺序。
固定原producer顺序：F0,R0,B0,F1,R1,B1,Call,E1,E0,D1,D0；single保持F,R,B,Call,E,D。
共享Call只一个稳定tag，核完整参数向量；六实体按实际append冻结，后续current映射也
须相同，同字节也不能复用owner/place/loan。两loan属于caller，callee两个参数loan独立。
原frontend、build/emit producer、其它三tracker及owner授权逻辑无diff。
严格同call Drop全集仅用于此无参Unit caller/唯一纯literal call形状，不外推更宽上下文。

本轮12项均逐源/helper核普通真实来源，选择为上一P1八项（其中非目标改成三literal）及
统一前缀`ssa::loan_region_ssa_tests::multi_temporary::`下四项：
- `two_literal_caller_requires_real_refine_route`
- `two_literal_call_requires_real_production_receipt`
- `products::two_literal_int_unit_same_bytes_and_empty_preserve_true_off`
- `products::reordered_named_literals_preserve_existing_route_without_receipt`

新products使用不同字节Int、同字节Unit、双空字节Int，Basic rooted/memory双Unit真正
重新analyze off，原off产品直接lower，完整旧facts只比较clone，SSA/LLVM逐字一致。
同时核六action的Begin/End/Drop对应、十一实际instructions、六独立实体、String类型、
Int实际类型/Unit零结果及callee双参数独立身份。命名颠倒来源本身合法且Complete1六action，
仍由旧Compatibility成功lower且无receipt，不宣称全Complete1可消费。既有single真off、
P2/P3/P4及同analysis B/Call/Return只读邻控保留，无旧污染owner集合或foreign查询。

新增两pair CI-only（交换call参数/复用owner，错End顺序/错Drop索引）与旧五P1负控均只
编译，未执行。capture仅在这些显式CI-only helper打开，普通helper不激活故障注入。
没有全量/native/resource/Abort/panic/压力/待核矩阵/Linux/sanitizer/release验收；未改
公共类型/依赖，未额外重跑前轮workspace或默认CLI图。不退休oracle或造第五protocol。

构建均固定`cargo test --locked --offline -p lang-codegen --features loan-observation-test-support --lib --no-run --message-format=json -vv -j 2`，随后从该JSON取实际binary，逐项
`--exact --list`核名再执行。实际本树src/lib.rs、opt0/debug2/test=true、feature仅support，
build-finished=true；未设置DEV_DEBUG/TEST_DEBUG/RUSTFLAGS/ENCODED，未复制/clean/降档。
独占`/private/tmp/koven-exclusive-loop-prefix-target`，LLVM prefix为
`/opt/homebrew/opt/llvm@21`。启动时另有owner target cargo27881，保持不动；本任务不用
owner target，也不带入main后来9e7fed92或其它人dirty docs。无非预期Rust编译/ordinary
测试失败；依赖llvm-sys的既有non_snake_case提示不伪报为本crate失败或被修复。

最终binary SHA256：`eaa406473264a83aa124d1e1802e6f68507943a08000be83e0adb4c0e1eafd6a`；
12项selection SHA256：`0011f0c36c434f4b287f2cebba276b3026d1e5a3ac89c09b71f10307cea0e483`。
临时证据`/private/tmp/koven-multi-p1-evidence`含两红/阶段绿、final manifest/selection/
exact/build JSON与2346源码/config hash，原始输出不入库，不承诺临时文件长期保留。
固定提交与上述精确选择为重做依据；本地分步提交，不push/PR/发布。

精确`03e110f5370f53907fd46ef29e8d24df8a9708b1`独立只读终审无阻断：2346源/config
bytes、清单SHA、实际binary/selection、原始JSON的debug2/feature/test与manifest均匹配；
12unique日志逐项等于名单，各1pass/2886filtered，无native或CI-only执行。73d22163后
仅已审普通/CI-only测试，无生产漂移；严格Drop全集检查只在当前窄shape适用。终审者
未执行测试；其结论后仅补文档，不改Rust或重编。借用target释放，仅本地交付。


### P/Q 单次 Boolean 比较条件实施合同（2026-11-10）

基于冻结`c956f3bf`，按持续并行本地开发授权，只补现行Boolean相等/不等语义的受限
P/Q运输。条件恰好一次`==`或`!=`，一端原Group/Not链到唯一Boolean参数Name，另一端
裸Boolean literal，左右皆可；比较整体可原Group/Not包装。拒绝双Name、双literal、
嵌套比较、literal包装、短路/调用/多参数，不引入语言、ABI或Resource决定。

P/Q专用条件准入保存实际参数symbol供本LoopTransaction使用，其他loop继续原共享
boolean_parameter；不放宽共享helper或body If观察。完整条件false仍是P，实际body
Replace/Break仍是Q，不按比较、literal或Not方向交换catalog。原B/Call/Return唯一
suffix、瞬时Inout、旧唯一emitter和清理不变。

状态in-progress：实际普通Return receipt红→最小实现→Basic/Const两Unit与两边
SSA/LLVM/native。每项新/邻控重新逐helper审定、记录实际源码hash，仅小集合exact；
foreign、故障/损坏、Abort、计数、压力及待定矩阵只编译。分步本地提交、固定头独审
与门禁，不推送、不触发远端CI，不改main或他人docs脏树，不绕过真实工具审批。

### 单次 Boolean 比较实施与受限验收（2026-11-10）

合同`f443766a`，普通真实红`36693a6d`，frontend红源`84be554e`，最终Rust输入
`f4768221`。红源码`while(flag == true)`已通过旧SSA/LLVM并完成真实Return，payload为
Some(2)但receipt为None，同exact失败。没有伪造产物、空collector或故障注入。

新P/Q私有条件准入只收单Eq/Ne、一裸Bool和一原参数链，沿用exact_type的非constant-use
及精确Boolean检查，保完整condition两种outcomes。保存实际parameter供已准入replacing
分支读取；其他loop仍原boolean_parameter。旧真实求值、body If观察权限、P/Q角色、
consumer和backend唯一emitter均未改，不按比较/literal/Not方向交换catalog。

新增前端4普通覆盖8种比较组合×参数/整体包装、真实AST/op/literal/参数symbol、
B/Call/Return产品及合法边界；另用正常closed-loop/body-If比较源证明共享门未扩张。
共享check_transport提取保旧Group/Not结构及全部P/Q/Drop断言。旧比较拒绝项迁为
双Name或嵌套比较，原断言保留；污染check/public iteration只编译，不调用。

新增后端3普通覆盖真实receipt、10条件的Basic/Const完整SSA角色/slot/payload/Drop，
以及同analysis10个有限sibling callable的真值表native输出。每Unit一个正常可执行，
不是压力矩阵、析构计数或heap/Resource证明；返回函数仍只清理detached P，caller
沿旧规则清理选中值。所有原B/Call/Return及Group/Not受限邻控保留。

显式重审29项仅11前端+18后端，全部通过且0失败/忽略；不读旧污染大名单生成选择集。
完整成功集合与exact选择一致，实际defaultdebug2/features空集及两binary SHA已核，
2995源码/配置SHA逐文件对照真实bytes全匹配。严格两crate all-targets Clippy、workspace
all-targets check、fmt、docs883、diff及尺寸base`c956f3bf`均通过：2339手写Rust，
35旧超千行，无预算或政策修改。证据位于`/private/tmp/koven-owner-boolean-comparison-evidence`。

本片不执行foreign查询、故障/损坏、Abort、计数、压力、sanitizer或正式双宿主CI；
前端on/off旧Analysis对照不外推后端off端到端。严格继承指定c956f3bf，不夹带当时新main
9e7fed92或修改其脏树，不宣称本片门禁已证明该新主干合流。owner target已释放，不推送。

固定`f4768221`独立只读终审无阻断：生产/测试diff、代码tree、源码清单SHA、两实际
binary SHA、11/18成功exact集合及debug2/features空集均复核匹配，审阅者未代运行。
本片本地实现与受限验收完成，正式CI、资源证明及新main合流边界保持，不退休旧oracle。

### Boolean 比较 owner 与双 literal P1 本地合流（2026-11-10）

冻结代码合流 `c627accc4ac2ffafecd90964a2f64c2397e67a62` 保留双父
`c8887287606e8fceff9d6756b17bfae84591095d` 与
`af25d5a70596e1c68d202b255c95bd682ec12d71`，共同基线 `c956f3bf`。
仅本 Spec 尾段冲突，按共同正文、P1 完整尾段、分隔空行、owner 完整尾段并集保留；
其余文件逐字等于相应父，没有新生产修改或新语言/ABI/释放责任决定。

本批不合入 main `9e7fed92`。只读比较发现该主干退休 Compatibility 的 shared-field
及 nullable Rc 旧路径，与本批增量无文件交集，但调用/field/nullable 共享行为仍须另行
整合验收；本批绿色不证明新主干组合。main 及原工作树均未改动。

两父名单重新逐源/helper 审核后执行 41 个唯一 ordinary exact：frontend 11、codegen 30
（owner 18 加 P1 12，无重名）。前端只复用旧 SOURCE/declaration，不调用含 foreign 查询
的旧 check；真实 off 重新分析原 ownership 并原样交给 lowering，归一化只用于 clone
对照。观察器未激活故障注入。5 个 native exact 内共 14 次有限普通可执行运行单独计账，
不累计为额外 tests，不作为资源计数、压力或析构证明。所有 foreign、故障/损坏、Abort、
计数、压力、待定大矩阵与 CI-only 均未执行，仅随测试构建编译。

使用独占 owner target，LLVM 21.1.8，未设置 debug 或 RUSTFLAGS 覆盖，未复制/清理 target。
默认 CLI check 成功 8.81s，实际 frontend/codegen/kovenc 三 artifact features 均为空；
feature no-run 成功 1m25s，两实际测试 artifact 均 opt0/debug2/test=true，且仅显式启用
loan-observation-test-support。每个 exact 先由新 binary --list 确认唯一名称，再单项执行；
41 项全通过、无失败或忽略。本批未运行 release、正式双宿主 CI 或全量测试。

证据目录 `/private/tmp/koven-comparison-multi-p1-evidence` 保存 manifest、两名单/日志、
原始 Cargo JSON 与 2348 源/config 哈希；临时文件不承诺永久保存，固定提交与本段合同
为重做依据。frontend binary SHA256
`0a7493979959b317a17480f231f931c64ca8ef113e7d2ef0d25cbbaf67fbbd6c`，selection SHA256
`0c899beadccddd010bba75d4d29b142b356869a315413ffeae0833f5cb35bb54`；codegen binary SHA256
`9234b899d0c1296aa883db81f1def2ac614d34a0f5e019200baecef9b397aed9`，selection SHA256
`dc1f6bcc48dab0ca02d40058caf4defdb1b8b6866795819c22063e9bb6256180`。

过程偏差：一次只读 Python heredoc 摘要因受限 shell 无法创建临时文件而未执行，改为
不创建临时文件的 Python -c 只读摘要成功；没有重试被拒写入或绕过审批。这不是 Rust
失败或能力红测；合流本身没有新增能力，不复跑两父红测，不将原红测算本批验收。

严格 feature 两 crate all-targets Clippy 41.65s、默认 workspace all-targets check 34.37s、
fmt、docs 883、尺寸 2341 手写 Rust/35 旧超限（base c956f3bf）及 diff 全部 exit0，
未调整尺寸预算。精确 c627accc 独立只读终审无阻断：2348 源/config 与两 binary 重算
SHA 全匹配，41 成功名称/名单/实际 profile/Cargo JSON 一致；frontend 每项 1pass/
602filtered，codegen 每项 1pass/2889filtered。审阅者未执行测试或修改文件。
终审后仅补此文档，未重编 Rust；owner target 已无占用。仅本地提交，不 push/PR/发布。


## 已批准的双 literal 命名参数映射增量（2026-11-10）

用户已要求今晚持续并行本地开发、本地提交；本片按既有语言和 ADR-0040 核验合同，
从固定 main 合成头 `4f72af76` 独立推进，不改变 ABI、参数求值或清理责任。
仅扩展现有两个普通 String literal Borrow 实参的已验证命名重排；保非泛型无参 Unit
caller、单 Declaration 调用及 Int/Unit 结果等原边界。Complete-empty、三 literal、
插值、投影、receiver、borrow-result 与控制转移不因本片开放。

源码 argument_index 决定实际形成和 Begin 顺序；parameter_index 是独立的参数映射，
必须构成两个参数的精确置换。唯一 DirectCall 按参数序核完整向量，End 和 Drop 分别
沿同轮旧事实及真实 producer 顺序核对，不重排、不重复发出动作。相同 bytes 不合并
argument identity、temporary Slot 或 SSA 实体。仍只选择同轮 Complete1；off、Partial
及 Unsupported 保持原路径，覆盖声明不升级为整个 callable Complete，也不退休 oracle。

实施分两阶段保留真实失败证据：原样命名重排 caller 路由红到绿；随后已迁 caller
缺实际 receipt 的独立红到绿。两阶段都以普通源码正常分析和真实路由为准，不加 anchor
或强制路径。验收包括不同/相同 bytes、Int/Unit、双 Unit 及真实 observer-off；测试
逐项与 helper 重审，所有 foreign、破损、注入、Abort、计数和压力仅编译，不在本机执行。
此处仅记录范围，尚未执行本片红测、实现或验收。

### 命名双 literal 映射实施与验收（2026-11-10）

基线4f72af76、范围合同852ecad7；实现全在独立named-literal树，不夹带单源0401 probe
或后续main合流账本。两个能力分别真实红绿，未用同一失败冒充两项能力：
- route红ac503340：原样 `inspect(b = "first", a = "second")` 完成真实FE Complete1六action、
  SSA/LLVM后，实际成功refine名单为空，与caller名断言失败；no-run53.27s。
- route绿9c7ab1c2：只typed完整参数置换扩独立caller trigger，完整subset与旧trigger不变；
  同exact通过，no-run28.33s。此阶段receipt显式保持旧顺序适用域，不使合法新route被旧核验拒绝。
- receipt红3829c432：同源已通过路由/SSA/LLVM，新增独立receipt断言真实0!=1失败，
  no-run16.17s；不是空observer模拟缺证、frontend拒绝或故障注入。
- receipt绿e1d79dfc：移除阶段旧域guard，接实际参数映射核验，同exact通过，no-run29.93s。
  最终普通/CI-only测试输入冻结9027aa930b8e83198e2ad1d0227f5741d462ceb0。

最终route仍只收原1/2 raw Text-only String literal形状，typed argument_index必须等于
源码位置，parameter_index完整覆盖0..n且唯一；不看Complete/observer、不重推签名。
admission独立范围/唯一性核验并把parameter_index与同轮完整contract及parameter_span
关联存入operand。operand不排序，形成/Begin保持源码序，End和Drop沿原真实路径各自核
源码逆序。唯一Call的build与SSA append分别按parameter槽完整核向量，拒重复/缺失/越界。
所有producer、四家族独立身份、owner授权/唯一cleanup、语义/ABI均未改。

最终15unique ordinary exact全部通过，每项1pass/0fail/0ignored/2894filtered：原P1十二项
中的命名重排边界已按批准能力改为 `reordered_named_literals_use_refine_with_production_receipt`，
其余原证据保持；新增named_literal_mapping模块三项分别核route、receipt及source顺序/真实off。
所有source/helper本轮fresh审核，不继承旧safe标签；没有额外失败或测试假设修正。

增强pair helper由原exact与新named exact分别调用，每项3个有限源×2种Unit布局×on/off。
原正序与命名倒序分别核typed映射、原参数声明span和contract index；不同bytes证明顺序，
相同/空bytes仍核两个argument IDs与六个实际实体互不合并。实际十一操作始终
F0/R0/B0/F1/R1/B1/Call/E1/E0/D1/D0；命名倒序Call为loan1/loan0，不能从Call再rev来解释End。
Int真实一结果/Unit零结果，callee Shared入口独立。真实off重新ownership分析，原off产品
直接lower，legacy比较仅clone归一化；on/off的SSA/LLVM相等。P2/P3/P4及同analysis owner邻控
保持，Complete-empty、三literal和其他非目标原路径未放宽。

P1新增两项CI-only（重复/越界参数槽、错误source序Call向量）与原七项都只编译不运行，
普通helper不启SNAPSHOTS或故障开关。未运行foreign、破损、Abort、counter、pressure、
强制路由、native或全量测试，不以本片证明Constant off、资源或旧oracle退休。

独占borrow target、LLVM21.1.8，未设置DEV_DEBUG/TEST_DEBUG/RUSTFLAGS覆盖，未复制/清理target，
不占owner target。最终新artifact opt0/debug2/test=true、仅loan-observation-test-support，
no-run24.58s；严格feature codegen all-targets Clippy31.95s、fmt、docs884、尺寸2341手写Rust/
35旧超限（base852ecad7）、diff均exit0，无预算调整。本片无公共API/依赖变化，未重跑
workspace/defaultCLI/release或正式双宿主CI，不把前批门禁当本批运行。

证据 `/private/tmp/koven-named-literal-mapping-evidence` 含四阶段manifest/build/exact、
final15名单/manifest/log及2348源码/config SHA；临时文件不承诺永久保存，固定提交和本段
精确测试合同为重做依据。binary SHA256
`7ea962559d788fbd8d7e0a59226ec67a0a820f4b8e71b19409868855da0af2a7`；selection SHA256
`9fed64e87f933a4365a0abc760807e9e503cab295251b7c9464e475e74bb3859`。

精确9027aa93独立只读终审无新增阻断：2348源/config重hash、当前binary/selection、
原始Cargo artifact与manifest一致；15唯一名单逐项等于实际成功日志和runner，
两独立红绿的真实前置与失败断言均复核。审阅者未执行测试或编辑源码；终审后只补两份
文档，不重编Rust。borrow target释放，本地分步提交，不push/PR/发布。


### 单层同参数短路 P/Q 运输准备合同（2026-11-10）

基于固定main合成代码`4f72af76`（main9e7fed92与e839f4bd），按持续本地开发授权，
准备严格同一Boolean参数的`flag && flag`/`flag || flag`。两个leaf只允许透明Group，
必须指向原唯一参数；整体可Group/Not。暂不放开leaf Not、第二参数/Name身份、literal、
比较叶、调用、嵌套短路、一般loop或Resource。原B/Call/Return、Replace及bare Break不变。

短路内部多条条件边必须沿原CFG汇聚至body/finished；owner exit仍严格只有finished→exit
和Break→exit两条入边，不放宽OwnerPlan incoming2，不引一般phi。条件阶段尚未安装
suffix carrier，纯Boolean读取后必须实证P未变；merge_value_states清carrier规则不改。
新的普通SSA oracle沿实际参数/edge追溯P/Q，不假设只有一个Conditional，不按edge顺序
或Not奇偶指定owner角色。原emitter、清理和全部语言/ABI规则保持。

当前仅准备Spec与普通真实receipt缺失测试，owner target由main合成验收独占，尚未编译
或执行本片测试，不声称已红。待明确释放后再审普通helper、运行真实红并实现；所有
foreign/fault/Abort/counter/压力及待定矩阵只编译，不push，不改发布或其他脏树。

### 同参数单层短路实施与受限验收（2026-11-10）

准备合同`7552e6e8`、后端源`d1a05b03`、前端源`b01ca948`，均在target释放前只写未跑。
明确释放后，以`b01ca948`执行真实普通红：原源码旧SSA/LLVM成功并完成Return，payload
为Some(2)但receipt为None，同exact失败。实现`82adbf30`仅新增P/Q专用逻辑分支及
Group-only叶helper32行，双侧真实symbol必须是同一原参数；共享条件门、loop分流、
merge清carrier及OwnerPlan incoming2无改动，未增加新language/ABI或一般phi。

前端新增4普通核真实两处Name/不同Span绑定同参数、outer condition及B/Call/Return，
排除leafNot/literal/比较/调用、第二参数、嵌套logical和其他loop族。旧负例迁为仍未准入
的leafNot/比较叶，原断言保留，污染check/public只编译。条件实际Read后的P边界仍由
旧entry检查证明，没把快照灌回状态，也没放宽普通merge规则。

后端新增3普通含真实receipt、新CFG oracle及正常native。oracle沿实际BlockParameter的
完整incoming arguments递归溯源，拒绝循环；以真实初始P及实际Replace安装Q为源，另核
Replace owner来自P、RHS就是Q literal。所有pre-exit String参数仍来自P，最后恰两条
Branch分别携P/Q，Return payload来源集合为两者，detached P与caller清理断言保留。
不假设只有一个Conditional，不按顺序/Not奇偶指定角色。Basic/Const、&&/||及整体Not
均通过；native只证明正常结果，短路边证据来自真实SSA，不冒充计数或资源证明。

新2项receipt/CFG预检先绿，随后显式重审36项（15前端+21后端）全部通过；预检属于同一
unique集合，不另计数量。首次fmt门发现新test模块注册排序，`dc85eaa5`仅移动2行path/mod，
不改生产或测试语义；前头36成功及fmt失败保留于pre-format。最终代码重新完成全部
门禁和同36集合，不能用前头binary代替：0失败/忽略、成功exact集合精确、实际debug2、
features空集及两binary SHA已核，2998源码/配置SHA逐文件匹配真实bytes。

最终严格两crate all-targets Clippy、workspace all-targets check、fmt、docs884、diff及
尺寸base4f72af76均通过：2342手写Rust，35旧超千行，无预算改变。证据位于
`/private/tmp/koven-owner-short-circuit-evidence`。本片没有运行foreign、fault/损坏、Abort、
计数、压力、待定矩阵、sanitizer或正式双宿主CI；旧oracle保持，前端on/off对照不外推
后端off端到端。基底含#411代码但只认领本片受限验证，未推送或改发布头；target已释放。

最终独立只读复核以dc85eaa5/tree5d9a133e为输入：排序提交仅移动test module登记，
源码清单与两binary SHA、15+21成功exact集合、exit0/features空集/debug2实际匹配；
结合生产和helper闭包审核无阻断。审核者未代执行测试，本账本冻结不改变Rust输入。

### 短路 owner 与命名双 literal P1 本地合流（2026-11-10）

冻结代码merge `e5a16d304c5da43a2e6b5da7e6c628764ede720a` 保留双父
`e3c639bd26bc66aff0ce8d451a1a829e4469a86a` 与
`c5fba0c481d21a9d61f80e41bcdf39592f143ba5`，共基4f72af76。执行前两原树clean，
本地main/origin/main/只读远端ls-remote均为9e7fed92，未自动带入其它头。
仅本Spec新增尾段冲突，程序核两父共同正文prefix后按named全文、分隔空行、owner尾段
完整并集；非交集文件逐字等对应父，无Rust冲突/新生产修改。未合750cd403账本或
8a38242b Bag probe测试，不改任何原树/main脏文档，不squash或丢父链。

两父36+15实际无重名，本合成源码重新逐source/helper审定后只执行51unique ordinary
（frontend15、codegen36）；全部通过、无失败/忽略或额外测试。FE每项1pass/606filtered，
CG每项1pass/2897filtered。6个native exact内共22次有限emit/link/run独立分账，仍只算
6tests，不作为资源计数/析构/压力证明。所有foreign、fault/corruption、Abort、counter、
pressure、强制路由及未审矩阵/CI-only仅编译，不调用旧污染check，也未执行Bag probe。

前端owner compare从同一真实checker分别生成两份局部planner Analysis，只清observed
局部owner_observations用于旧facts相等对照，不回灌lower，不是clone()比较器。
P1真off则重新分析ownership并将原off产品直接lower；legacy比较helper才clone归一化。
两者不混写，观察器未激活故障/发射失败hook。实际条件求值、参数映射和唯一cleanup
沿两父原实现，没有为合流换源或改测试结论。

独占owner target、LLVM21.1.8、默认opt0/debug2，未设置profile/RUSTFLAGS覆盖、未复制/
清理target。默认CLI check8.83s，实际frontend/codegen/kovenc三artifact features=[]；
feature no-run1m24s，两test artifact test=true且仅loan-observation-test-support。
先由新binary --exact --list核唯一名，再逐项执行。严格feature两crate all-targets
Clippy41.40s、默认workspace all-targets34.55s、fmt、docs884、尺寸2343手写Rust/35旧超限
（base4f72af76）、diff全部exit0，无预算改变。本批无Rust编译/测试意外失败；仅预期的
Spec合流冲突，未将两父历史红测重算为本批能力证明。

证据 `/private/tmp/koven-short-named-p1-evidence` 保存manifest、两selection/exact日志、
runner、原始Cargo JSON与4193全部tracked文件hash（含fixture/config/docs）。该hash冻结
于e5a16d30；本账本追加后仅Spec当前bytes不同，生产/config/fixture不变。临时产物不
承诺永久保存，固定双父/merge和51精确选择为复现依据。
frontend binary SHA256 `6495fbbbb7b916c6258121200f31cd730b7f962350d878bc68ea4bed5fbe5f17`，
selection `97d2c63d1dc4350b4f3667a3a7373428841ffce10e7e42491ebbc1c736c92cf0`；
codegen binary SHA256 `6e842884204ad4cec95c93873992f4a3acf04dfdc1b39e275c368fe02ba7c520`，
selection `09db3871261b3bd8b3de495e337f1f908d7ddfdd865457b07dd715e632b9668f`。

精确e5a16d30独立只读终审无阻断：4193输入、双binary/selection重算、实际profile与
原artifact/51日志/runner、默认三CLI[]和全部静态终态均一致；审阅者未执行测试或编辑。
终审后仅补Spec，docs/diff复核后本地提交，不重编Rust。owner target已释放，未push/PR/
发布；正式双宿主CI、资源证明与独立Bag行为对照仍不在本批范围，旧oracle不退休。

### P1 非零有限 literal arity 实施合同（2026-11-10）

在233736f8上扩展既有单call Vec计划，解除恰一/二literal限制，接受非零有限N，
不新增协议或人为arity上限。其余完整subset、无参非泛型Unit caller、单普通Declaration
Borrow String调用、raw Text-only literal、Int/Unit结果边界不变。typed映射独立于
observer/Complete1触发路由；参数槽必须是完整置换。源码序形成Literal/Root/Begin，
唯一Call按参数序，End和Drop分别沿原producer源码逆序，不重排或重复cleanup。

同轮Complete1的3N动作可选核验；Complete-empty、Partial、Unsupported和真off不造
receipt或absence。计划保留N个独立临时身份，即使bytes相同也不能合并。阶段长度
5N+1和3N实体/动作索引以checked预算验证；内部不可表示返回结构化错误，不由receipt
状态增加语言拒绝。现有逐operand身份扫描有二次复杂度，不宣称线性或无限资源，
不运行大arity、压力或资源计数样例。

先原样三literal合法源码真实route红到绿，阶段一receipt暂保旧一/二域；再单独
执行缺production receipt红到绿，不用一次红代表两项能力。普通验收仅固定3/4参数
样例，覆盖不同/相同bytes、非平凡命名置换、Int/Unit与双Unit真off；每项重新审
source/helper。foreign、注入、Abort、counter、pressure、CI-only只编译。
所有生产动作仍来自旧producer，不改语言/ABI/释放责任，不扩其它协议或退休oracle。

#### 非零有限arity实施与验收账本

基底233736f8，新隔离树/分支仅本片。768cb61f提交原样三literal route红：真实同轮
Complete1九action、SSA/LLVM成功后，routed=[]不等于实际caller；新构建1m22s，
单exact退出101、2898filtered。7e8fd5fd仅把完整typed/ownership门的非空arity放开，
receipt暂保旧一/二域；23.46s新构建、同exact通过。1b60cf51另加receipt缺证红，
13.12s新构建，先route及SSA/LLVM成功，再actual receipt0!=1退出101、2899filtered。
dbd213c0最小生产绿，25.45s新构建、同receipt exact通过；没有把同一次红当两项能力。

生产保原Vec计划和唯一Call，不新增协议或producer动作。私有instruction_count检查
非零与checked5N+1，支配admission的3N、2N-1-i/3N-1-i，step的3N/4N/5N、
emitter的3i+2及finish计数。有效step保证stage增量可表示；结构化内部错误不影响
独立typed路由。FE既有checked3N认证及旧求值/End/Drop顺序未改。参数映射与所有
source身份逐项核，未给任意人为arity上限，也未承诺大arity资源或线性复杂度。

最终代码9b2e93da78cc2d9c37eb633b78f7d7d788cd20f7。普通oracle泛化为const N，旧N2
源码保持，新固定N3 Int置换[1,2,0]和N4 Unit置换[1,3,0,2]各双Unit真off，共8次
有限lower；不是无界矩阵。真实9/12action、16/21instruction、9/12entity、同bytes
独立根、空字符串、callee独立loan和完整Call参数序均核。End/Drop分别按source逆序，
不反转Call向量。off重新分析ownership且原off产品直接lower，比较旧facts才clone归一。
原三literal负边界因已批准准入移出，零action/Boolean结果/带参caller边界保持。

精确18唯一ordinary全通过，每项1pass/0fail/2902filtered；包括原单/双literal、
命名映射、P2/P3/P4生命周期与同analysis owner B/Call/Return邻控。总76次有限
SSA/LLVM lower不另累计tests，全部无native。新两项及既有九项P1 CI-only仅编译，
foreign/fault/Abort/counter/pressure/强制路由未执行，普通观察器未激活故障注入。

偏差如实保留：阶段一fmt --check指出临时guard换行，脚本未停而提交，后续rustfmt
修正且最终fmt通过；测试helper泛化两次编译失败E0308、E0507分别涉及引用层级与
误移动UnitLoanFact，原日志保留，最终保借用而非clone。独审发现新true-off测试
缺feature gate的默认test图风险，添加该单项gate并通过默认codegen all-targets check。
这些都不是能力红测。只读ps及一次摘要heredoc因sandbox权限失败，未改变进程或源码；
进程检查经已授权escalated执行，摘要未绕过重试。runner末尾旧15文案执行前改为18。

独占borrow target、LLVM21.1.8、默认opt0/debug2，无profile/RUSTFLAGS覆盖、无复制/
清理target。最终feature no-run27.57s，实际本树test artifact仅loan-support feature；
先新binary逐exact --list核名后运行。feature严格codegen all-targets Clippy36.15s、
默认codegen all-targets check22.99s、fmt、docs884、尺寸2344手写Rust/35旧超限
（base233736f8）、diff均exit0，不增加额度。默认CLI/workspace/release/正式CI未在本片
重做，不沿用先前证明扩称；Constant off/native/资源证明与旧oracle退休不在范围。

证据 `/private/tmp/koven-finite-p1-evidence`：final-manifest.json、final-selection.txt、
final-exact.log、final-runner.log、原始Cargo JSON、2352源码/config hash及分阶段红绿/
helper编译失败日志。冻结2352文件与9b2e93da一致，最终仅两docs追加，不改变源码。
实际binary SHA256 `145175371bc0c83dd6941464779aa5760c4296cf7c97e78f58cefb4272195b35`；
selection `477d9f22d3840642f15897423a7372b49e464f8d2eddcb2457054a9a89aec33a`；
source清单 `9ed774290d9d7b560e785cec32c13a7bbda17e050b39e0462aa2bbb0dabee8c4`。
临时产物不承诺永久保存，以固定代码头及精确测试源码为复现依据。

精确9b2e93da独立只读终审无新增阻断：2352源码/config、实际binary/selection及
原Cargo artifact/profile、18逐项结果和两阶段红绿、静态终态均匹配；审阅者未运行
测试或编辑。终审后仅补两docs并复核docs/diff，本地提交清树；borrow target已释放，
未push/PR/发布，不将固定3/4代表外推大arity资源或线性性能。


### 同参数短路叶 Not 实施合同

从233736f8隔离继续既定Boolean语义：P/Q条件保持单层`&&`/`||`，两叶允许
Group/Not链到同一唯一Boolean参数，整体Group/Not仍可用。每层exact Boolean、
ordinary outcomes及非constant守卫不变；不放比较叶、嵌套logical、多参数、调用、一般loop
或Resource。不改语言、ABI、原merge清carrier、严格两owner出口或唯一物理emitter。
先以真实普通源码通过旧SSA/LLVM后缺Return receipt作红测，再实现最小准入门；
Basic/Const验证实际沿途P、出口P/Q、caller清理及正常native真值表。所有foreign、fault、
Abort、计数和压力仅编译，不执行；本片仅本地提交，不发布。

### 同参数短路叶 Not 受限验收

基底233736f8保父链隔离；f029de6e先提交合同和普通backend红测/原CFG helper提取，
5af29dc1增加4个frontend普通exact。红binary编译输入f029de6e，运行时HEAD已是仅新增
前端测试的5af29dc1；两者生产相同。唯一实际Return红测通过旧lower、SSA verifier、LLVM，
真实payload为Some(2)而产品receipt为None，exit101，原日志保留，无注入或假造事件。

生产固定f9d617249a95b74d189db1169753aff909b7629d（treea485e66b），只在P/Q私有
logical leaf helper增加Not剥层并改名；普通outcomes、逐层exact Boolean/非constant、
同参数symbol、共享loop门、merge及incoming2均未改。7个旧测试文件中已准入负例改为
仍不准入的比较叶，原断言保留；含混合foreign的旧helper/public测试仍仅编译。

新前端4项核真实左右AST/Span/Group/Not层数/同一参数symbol，B/Call/Return三consumer
与原清理on/off一致，正常邻形拒准入。新backend3项覆盖实际receipt、Basic/Const
SSA/LLVM与正常native；复用完整原CFG断言，沿真实BlockParameter入边追P，核Replace
输入P及真实Q、最终两Branch分别P/Q、返回payload与caller旧Drop，不假定Not边顺序。
混合叶Not在数学上恒真/恒假，但现有typed ordinary结果保守为双outcome；测试明示该事实，
未为产品放宽守卫。native相应验证outer/outer或next/next，不把静态双出口说成两边都在
该具体源运行；同否定及双否定源覆盖两种runtime结果。正常输出不等于析构计数证明。

最终19前端+24后端共43唯一普通exact全部通过，成功集合与显式selection逐项一致；
本轮逐helper重新审过原36及新7，不继承历史大白名单。两binary实际debug2/features空集，
3001源码/配置SHA和artifact SHA实际核对，manifest固定f9d61724。严格两crate all-targets
Clippy、workspace all-targets check、fmt、docs884、diff与尺寸base233736f8通过：
2345手写Rust、35旧超千行，无预算改变。独立只读审核核生产/helper及最终tree、hash、
19+24成功集合无阻断，审核者未代执行测试。

证据目录`/private/tmp/koven-owner-short-circuit-leaf-not-evidence`保留red、build、selection、
helper审计、manifest及所有门禁日志。没有执行foreign、fault/损坏、Abort、计数、压力、
待定矩阵、sanitizer或正式双宿主CI；未推送、未改原main脏树或发布头。owner target已空闲
释放；当前只认领本片受限行为，不外推一般循环、资源或backend off端到端。

### 叶 Not owner 与有限arity P1 本地合流（2026-11-10）

冻结代码merge `4f02015de940bf0f7b5973cf94634ebb300f7c19` 保留双父
`b48437290dc94dfa4172ac98fa05bd11b8a96f65` 与
`3d9fa62f2cfea022b3472534cb99830990aff491`，共同基底233736f8。
新隔离树开始前两原树clean，main/origin/main/只读远端ls-remote均9e7fed92；不自动
夹带其它main、独立Bag probe或旧账本分支，不改变原树用户文档。P1改11路径、owner
改15路径只交集本Spec；共同正文prefix核后保P1全文、分隔空行和owner新增尾段，
其余非交集文件逐字等对应父。仅预期文档冲突，无新生产/ABI/清理或路由改动。

两父43+18无重名，本合成树fresh source/helper审定后执行61unique ordinary：FE19、
codegen42，全通过且无额外选择，分别每项1pass/610filtered和1pass/2905filtered。
FE为323个固定普通fixture；owner17个静态exact含64次正常lower/63次显式LLVM，
physical_drop只核SSA；另外7native exact共34次有限emit/link/run（2/6/2/2/2/8/12），
仅算7tests，不作为资源计数或一般终止性证明。P1十八为76次有限SSA/LLVM、无native；
这些次数不另累计tests。全部foreign、故障/注入、Abort、counter、pressure、强制路由、
CI-only与未审矩阵只编译不执行，普通observer不启capture/fault。源内native循环均
最多一轮裸break，无额外输入/递归；运行成功及精确stdout由实际测试日志证实。

这是同构建回归及原七族sibling证据，不声称新增叶Not与N3/N4同程序交叉验证。
FE compare从当前fixture重新规划两份本地Analysis，只清observed局部owner_observations
作相等对照，不是clone比较器，也不回灌lower；P1 legacy比较才clone归一。P1真off
重新分析ownership并把原off产品直接lower。数学恒真/假与保守双出口不混写。

独占owner target、LLVM21.1.8、默认opt0/debug2；无profile/RUSTFLAGS覆盖、无复制/
清理target。默认CLI check11.03s，原frontend/codegen/kovenc三artifact全features=[]。
feature no-run1m26s，两本树test artifact只loan-support、opt0/debug2/test=true；
先新binary逐--exact --list核名，再执行精确名单。feature两crate严格all-targets
Clippy53.93s、默认workspace all-targets35.63s（含cfg gate编译）、fmt、docs884、
尺寸2346手写Rust/35旧超限（base233736f8）、diff均exit0，无额度增加。
本合流无Rust编译/测试失败；准备hash清单时git ls-files非ASCII转义路径曾使脚本
报FileNotFoundError，尚未启动Cargo，改NUL分隔读取后成功，不记能力红或源码失败。

证据 `/private/tmp/koven-leaf-finite-p1-evidence` 保存manifest.json、frontend/codegen
selection及exact日志、runner、原Cargo JSON、helper审计和4200全部tracked输入hash。
4200清单冻结4f02015d，含fixture/config/docs；追加本账本后仅Spec当前bytes不同，
生产及其它输入仍同冻结，不称全部当前文件零差异。临时产物不承诺永久保存，双父/
merge及精确测试源码为复现依据。FE binary SHA256
`f5e19871ea7946fa81cb20b53e9a118e5bb27533a121f7714e0fa42b5d51b202`，selection
`5fa842f96362a049bffd25e9a279d9cb77daff577de9fc046f60649064c1e5e7`；CG binary
`da00471a118e9edae3af54946b318d8e9c92bcaa61c6f093b1f613891c1fe7af`，selection
`babc51aee80eda84035ef1afba942bdc91ded29c3d03619e4f9b9bab76ae1be3`；输入清单
`3b19f298ddb9ae39f2e2d93b24e5550858b3059f525c76ed65105a0a92f47a9e`。
正式双宿主CI、release、资源证明与大arity压力仍未执行，旧oracle不退休，不push/PR。

精确4f02015d独立只读终审无新增阻断：双父/逐字合流、4200冻结输入、执行后双binary
与selection、原artifact/profile、61逐项结果和全部静态终态均匹配；审阅者未编辑或
代执行。终审后仅补本Spec，docs/diff复核后本地提交清树，不重编Rust；owner target
已释放。native结果不独立证明RHS动态跳过次数，不作计数或一般终止性承诺。

### 单借用binding跨语句生命周期前置实证（2026-11-10）

基底a35f8c22，在新隔离树先核原样本地String根、单borrow val binding、单普通consume
调用的现有语言/路由/SSA与action发布，不先写生产。真实handoff不是End；绑定结束
后的source token退休及owner清理不能由Span或相等旧表拼成引用证据。已有
result_action_source测试依赖故障拒绝helper，本地只编译，不作为普通执行入口。
另建单一普通原样probe：真实FE→观察原action/旧facts→正常lower→SSA/LLVM，
记录实际路由和操作；无force/修改facts/native/foreign/故障/资源计数。若原样不支持，
先报告诊断，不自行扩大语言准入。后继认证/生产消费合同以该实证与真实发布点为前置。

#### 单binding首片因果链接合同

原样probe dd84209c实际成功：view与run走既有refine，consume复用binding结果loan，
八SSA操作为literal/root/sourceBegin/BorrowCall/consume(result)/End(result)/End(source)/
Drop(owner)。FE实际两Begin的canonical root均为owner，不能靠它选择binding；两P3
region Unsupported已实跑观察。source整体Partial此前仅getter静态结论，后续普通测试
明确断言。无需路由/语言准入增量，probe成功不代表完整认证或receipt。

第一阶段新增Partial因果证据，保现BorrowHandoffCommitted/BindingEnd旧payload与全部
旧public projections。BorrowBindingUseReturned记录普通consumer真实pending token正常
移除，回指该consumer Begin、已成功commit的binding及source Begin；这是metadata使用
结束，不结束仍live的binding/result SSA loan。BorrowBindingSourceRetired记录实际binding
移除后的source退休transaction，回指source Begin、commit、唯一use-return与BindingEnd。
句柄从实际pending token move携带，commit实际发布返回当前catalog句柄；consumer必须由
原raw Name operand解析为live binding，而不是canonical owner或Span匹配。退休观察以
共同事务保持BindingEnd(result)→source retirement，再由旧路径Drop，绝不增执行动作。

公开引用仅经同catalog/先后/角色核验后可用，缺引用保None/Partial；不从相等旧描述
补配。新增metadata不进入PendingBindingTransport语义Eq，原合流retain资格不变；不同
句柄或use历史只降级证据，后续use不能把已丢历史重新升级。普通控制邻控仅验证前端成功合流、旧表与on-off等价；其if不在既有lowering子集，
不声称该控制源码有SSA/LLVM成功证据，也不认证控制。首片仍单本地String根/单binding/普通consumer，不泛化
parent/sibling/wrapper。完整跨两statement binding region及可选production receipt另以
独立能力红推进，未闭合之前不发布Complete；不改变P3/P4 call-only范围或授absence。

首阶段普通缺证红在真实on/off重分析、完整旧facts对照、双Unit SSA/LLVM及八操作
身份通过后检查实际因果动作/同catalog引用；API stub无人发布应失败。后续真实发布点
实现再跑同源绿。foreign/破损/Abort/计数/压力与CI-only仅编译，旧result_action_source
及cross_source外来查询helper不执行。

#### 单binding Partial因果链实施与验收（2026-11-10）

冻结代码 `5b3df2af6d20f02aa9fb0f87aae1a9b8f1177a73`，从 `a35f8c22` 独立树实施，
未改变语言/ABI/route及旧cleanup producer。source Begin句柄沿实际PendingLoan move保存，
成功initializer commit返回真实handle，consumer正常return提取原token并从raw operand识别
binding。实际移除先保旧transport投影，再连续发布BindingEnd与SourceRetired观察。
metadata不入旧binding/source语义Eq，合流歧义与重复/不合格use只降证据，后续use不恢复。
公开use_return仅指唯一已观察合格use，不证明all-use覆盖；本片仍Partial，无新region认证、
Complete、absence授权或production receipt，不退休oracle。

原样probe `dd84209c` 显示view/run既有refine、两P3 Unsupported、实际八SSA；它未断言
source Partial，该项由后继能力测试实测。缺证红 `dce5e9b9` 的on/off旧表与SSA/LLVM先成功，
在首rooted布局缺七事件而失败（0/1/2907），不能算第二布局完成。API stub本身没有producer。
`248b2fa6` 接入实际发布后核心同源绿（1/0/2908），双Unit真实off重ownership，原off产品
直接lower，比较helper仅clone归一化；result/source两loan与consumer复用身份均验证。

过程偏差保留：新增merge邻控原假设if可lower，但完成首布局全部FE旧表/retirement断言后，
后端UnsupportedNode 178..227拒绝；这是测试对既有subset的错误假设，不是能力缺证红。
`5074793e` 将其收为frontend-only，最终测试名明确frontend_merge；不扩route、不宣称
该控制源码SSA/LLVM成功。严格Clippy首报question_mark，最终等价改?；原失败日志保留。
只读hash用shell heredoc遇沙箱临时文件限制，改无临时文件python-c完成，不是Rust失败。

静态：feature-enabled前端/后端严格Clippy39.43s、默认workspace all-targets29.34s、
fmt、docs884、尺寸2353手写文件/35旧超限（base a35f8c22）与diff均通过。最终两crate
no-run27.43s，真实opt0/debug2/test、仅loan-observation-test-support，两个新增前端
CI-only破损/合流负控只编译不执行。未运行release、正式CI或额外资源/压力证明。

证据目录 `/private/tmp/koven-binding-lifecycle-evidence` 保留各阶段raw Cargo JSON、
manifest、选择、原日志及4207冻结tracked输入SHA；代码冻结后仅本账本/架构docs可不同。

最终五个唯一ordinary exact全部通过，各1/0/0/2908 filtered：原样probe、因果链双Unit
true-off、frontend-only merge true-off、既有P4生命周期、P3 Int/Unit true-off。只前端merge
不lower；其余固定有限源码验证SSA/LLVM，无native。测试名/顺序与final-selection、
runner、manifest及原log一致，前端test binary仅编译，未执行。

最终CG binary SHA256 `980b794f470ae54b202785a6792ef84e4f1b506aca8f74404812cad5190b3a6e`，
FE `338dc6f4072615c0b26306a3cfc244fa3cb22805333f2adfb3b637b9d6838240`，
五项selection `c3326dff039c4bc0f647c99188f4d4c4a162ed00c779342769ad75091b918937`，
4207输入清单 `87fb475ed1e9ba258b5ccc058f0cc8231dd9e8351bceb82ea28c0e59669ae09b`。
未单独采默认CLI artifact features证据，不把默认workspace成功表述成该实证。

精确 `5b3df2af` 独立只读终审无阻断：五名单/manifest/runner/原log同序，执行后两binary
hash仍匹配；4207冻结对象与清单一致，当前仅两docs不同、非docs无漂移。四项共15次
有限SSA/LLVM lower，一项仅前端合流；不含native或CI-only执行。终审后仅docs补账本，
不重跑或修改Rust，本地提交且不推送。

### 单binding跨statement完整覆盖认证（2026-11-10）

基于 `0b456820` 的真实Partial因果链，本片独立认证一个声明清楚的正常区域，不把因果记录
非空自动升级Complete。保持普通非泛型、无receiver、无参Unit caller的精确三statement：
`val owner = plainString`、`borrow val item = view(owner)`、`consume(item)`；单String根、
单binding、普通单Borrow参数consumer返回Unit，无控制、重绑定、parent/sibling/wrapper。
原语言/ABI/route、cleanup producer及P1–P4 call-only合同不变，production receipt另片。

区域入口在owner声明已经真实正常完成后、borrow声明执行前。正面核同callable/source的
原AST/typed形成、唯一当前OwnedValue的symbol/declaration/origin/scope和形成事实；本区域
不声称覆盖owner形成全生命周期。实际entry cursor至完整正常body处理后的completion cursor
覆盖全部未过滤动作，包含borrow initializer、commit、consumer及原binding/source退休和
清理。entry/exit审全部active pending/transport/capture/owner/loop/replacement状态，不能
以历史事实索引空集替代运行状态。保留原statement外层owner观察wrapper，不绕过旧执行。

独立validator必须逐角色核同catalog及真实引用：source Begin→handoff→consumer metadata
Begin/use-return→binding End→source退休→实际owner Drop；consumer正常返回不是binding
End，不新增第三SSA loan。仅consumer AfterStatement即时退休且实际owner已移除、唯一新
Cleanup Drop append成功才可闭合。retirement句柄沿原drop尝试携带到实际移除及push_fact，
保护早退、去重未append、歧义或缺边均不借旧相等Drop补齐，不按Span/root查最近事件。
完整未过滤区间若有额外/漏动作、entry/completion缺失或活跃义务未清空，保持Partial。

公开为独立binding区域，非第五个call-only协议。区域身份包含同轮source/callable/binding
和声明边界；source/callable仍Partial，off无新区域且旧facts/执行等价，Unsupported/缺失
不授absence。metadata不改变旧Eq、合流资格或cleanup顺序，错误/deferred沿既有撤销路径。

先新增普通原样真实缺覆盖红：旧因果七事件、真实on/off产品、SSA/LLVM均仍成立，但无
独立Complete binding区域。再接实际entry/completion、Drop因果carry及独立验证成为绿。
局部普通邻控按fresh source/helper审后exact执行；foreign/破损/Abort/计数/压力/CI-only
只编译不运行。后端production消费缺证红不混入本片。

#### 独立binding区域认证实施与验收（2026-11-10）

基于clean `0b456820`，Spec `dcde4337` 固定已形成owner为entry前提的两statement区域；
未声称owner形成在区间内。API空stub/普通能力红 `5b3d57a2` 先完成首rooted布局真实on/off、
七因果记录与八SSA/verified LLVM，再于缺独立binding区域失败（0/1/2909）；红未执行完
第二Unit布局。编译1m01s、opt0/debug2/support，区别于上一片缺因果链接红。

冻结代码 `50f1bdf8e06f6f522fa4e61efcc3a99dbc94d255` 新增独立 UnitBorrowBindingRegion，
普通caller三statement准入，入口核唯一真实OwnedValue及typed/raw身份，body正常尾全active
空状态才completion；原item后续drop_all因此无动作。完整未过滤七事件区间逐身份验证，
consumer AfterStatement即时退休的handle沿原drop尝试到实际remove及唯一新Cleanup append；
entry/removed完整owner快照核一致，保护早退、去重未append、重复/错配不借旧相等Drop。
原函数签名与None路径执行顺序不变，未增加End/Drop、未变语言ABI/route/Eq或P1–P4。
source/callable仍Partial；独立区域Complete不代表production receipt或旧oracle可退休。

首green build46.08s，主例1/0/2910，两Unit真实off及原SSA/LLVM保持，公开region身份逐一
核实际caller/root/binding/sourcecall/consumer/entry/body与即时retirement点。

过程偏差：补actualremoved快照的Python精确替换发现两处匹配并assert中止，已部分写入
validator/entry快照和import，尚未写cause/Named push/CI构造；该shell后续fmt/diff仍执行，
不能以shell0判整项成功。只读确认后用set-e和精确struct匹配补齐，再fmt/diff成功。
这是编辑脚本偏差，不是Rust编译或能力红；本轮实际Rust编译与Clippy无失败。

静态feature-enabled前端/后端Clippy38.71s、默认workspace all-targets32.31s，fmt/docs884、
尺寸2359手写文件/35旧超限（base0b456820）及diff通过。主planner1598→1634仅必要wrapper/
接线，保1669既有例外，不提高额度。新增两项CI-only缺/foreign/重复/错Drop及边界破损负控
在两crate test no-run中编译但不执行；无native、Abort、counter、压力、正式CI或release。

最终静态后两crate no-run实际0.13s，六个唯一ordinary exact均1/0/0/2910 filtered：Complete
主例、extra-use/后读owner边界、原causal true-off、前端merge true-off、P4生命周期与P3
Int/Unit true-off。五项共26次有限SSA/LLVM lower，一项仅FE；未执行frontend test binary。
两新增CI-only负控仅编译，不把人工破损用作普通能力见证。

证据目录 `/private/tmp/koven-binding-coverage-evidence` 的final-selection、run-final、
final-manifest及原exact日志同名同序；4213冻结tracked输入保source-input-hashes，
冻结后仅Spec/架构文案不同。CG SHA256
`ffe429cfaf31b6d18645dae45981b3bd299f15424d45aa28b8b26a290a3c31cb`，FE
`90dcc7ee49e4a4f02d182878e4b86d3aa03c9a3892df9fd3d6f230260a196de7`，selection
`77ecad1149ba1b88993b62c2cac9c5b455f99a6b10b8872f4e09444d931d6add`，输入清单
`e9a81350715d08ca843acdf81f603f6b7f762da636e1dce8a8c72f73558839f4`。
实际两test artifacts opt0/debug2/test=true、仅loan-observation-test-support；未另采CLI
artifact features证据，不把workspace成功外推成CLI/release/正式CI验收。

精确 `50f1bdf8` 独立只读终审无阻断：4213冻结对象、两binary、raw artifacts、六项
selection/manifest/runner/原日志均匹配；当前仅两docs变化，非docs无漂移。本次仅审过
旧1669限额内1598→1634必要薄wrapper增长，不推广新额度。最后docs分账复核通过，
随后仅文档本地提交清树，不重跑Rust、不推送。


### 有界 P/Q 双参数 own-call 实施合同（2026-11-10）

从 a35f8c22 隔离实现唯一 own String 和一个显式 own Copy Int literal 的双参数直接 Unit 调用。
保留原 prefix/While/Replace/Break 与唯一尾句；只增加真实实参到形参映射及 selected 槽授权。
允许两种形参顺序、positional 与反序 named；无额外 owner、loan、调用或分支，不改语言/ABI。
真实 pending Value transfer 才封印，原 DirectCall emitter 唯一执行，caller 不重复 Drop。
先用新普通有限源码确认缺 receipt，再按同源绿、双 Unit SSA/LLVM/native 验证；foreign、
故障注入、Abort 与计数负控只编译、不在本地执行。默认 debug2，独占 owner target。
准备提交5577e372之后发现默认Int参数是SharedLoan，故明确限定显式own Int的Value copy，
默认Borrow Int仍无receipt。恢复原准入与模型，在相同own Int源码确认receipt=0（1 exact
失败、611 filtered）；恢复实现后同源四例通过。早期短名exact选0项和临时缺模型字段的
编译失败均不算红；未把默认Borrow样例当最终Value合同红测。

前端保存真实argument/parameter index与arity，并逐项核typed和ownership合同；refine核
同轮typed映射，原DirectCall参数槽只有一次selected值。当前新普通exact四项已通过：
FE四正例、五合法邻控，Basic/Const各四种映射的SSA/LLVM及八次native。callee仅用普通
有限Int比较输出7，caller输出outer/7/next/7，不作析构计数或泄漏证明。最初native fixture
的println(Int)类型失败及Int插值UnsupportedNode已改为上述合法oracle，不修改相关能力。
完整helper审计仅执行同catalog查询、真实普通分析/lower与正常native，无foreign查询、
注入、Abort、压力、计数或强制route。新增CI-only三类call故障×四种映射仅编译。
默认debug2、LLVM21.1.8、独占owner target；日志保留于
`/private/tmp/koven-owner-pair-call-evidence`。clippy/check、最终头独立审查及正式双宿主CI
仍待记录，不认领D10/E2完成，也不push/PR。


该片生产冻结11f418313cf6a30c71204661237620e839e0e541；c27c86fc仅将架构事实合入
原段满足200行上限。两crate all-targets clippy -D warnings、workspace all-targets check、
fmt及docs884通过。独立只读审查该生产头的四生产文件与新FE/backend完整测试，未发现
阻断；不将该审查视为全部历史helper认证。最终manifest逐个选择FE2和CG5共7个exact，
其中新增4exact与旧单参3exact分账；新增内部例数仍为FE4+5、SSA/LLVM8、native8。
CI-only故障仅编译，未运行foreign、注入、Abort、计数、压力或强制route；正式双宿主CI、
sanitizer及一般循环/多owner/loan合运仍未完成。Cargo JSON证实两个test artifact均
opt0/debug2/test=true、features空；二进制SHA、源码hash、选择名单与失败摘要保留在上述
证据目录，不复用旧大筛选集合。不推送，不改变main及借用隔离树。

### 双参数 own-call 与 binding Complete 本地保父链合流（2026-11-10）

本批代码冻结 `0a8945b1d33af5904fa302663e5f2fc1c5947407`，双父为借用
`ff9c1d464c79ee03159a88c0c235f7a31b9908e7` 与 owner
`680215d3017e0947dd8d86b9167a6c71854cd02a`，共祖 `a35f8c22`。
独立 worktree 保留两条父链；唯一冲突为本 Spec 的新增尾段，合并严格保留借用全文、
分隔空行及 owner 新尾段，其余路径逐字等于对应父，不增生产代码或测试。
本地主仓 main/origin/main 仍为 `9e7fed92`，没有另合主干、probe 或改动主仓既存文档。

执行前重新审阅两个父输入的本体及全部共享 helper，去重后 FE2 + CG11 = 13 个唯一
ordinary exact，未继承历史大集合。真实同 catalog 分析、原产品 lowering 与观察器均
未启用 foreign/fault/forced-route；相邻 CI-only 只编译。owner FE compare 对实际 fixture
重新规划后仅在比较用本地结果清 owner 观察；借用 legacy comparator 则克隆两原产品清
loan 观察作比较，真正 off 产品直接 lower，两者不能混称同一实现。

独占 `/private/tmp/koven-choice-replace-forwarding-target`，LLVM21 prefix，未设置 debug
或 RUSTFLAGS 覆盖、未复制/清理 target。默认 CLI check 9.43s，原 Cargo JSON 中 frontend、
codegen、kovenc 三实际 artifact features 均为空；显式 loan-observation-test-support 的
两 crate lib no-run 1m04s，实际 opt0/debug2/test=true，两个 test artifact 仅此 feature。
实际二进制经 exact-list 后逐项执行，FE2 每项1通过/616过滤，CG11 每项1通过/2913过滤。
13项全部通过，无额外最终选择。2个 native exact 单列：双参数8次、原单参数2次，合计
10次有限 emit/link/run，每个程序各执行 false/true 两出口，不额外计作测试数量；没有
资源计数或一般循环终止性证明。借用6项保留26次SSA/LLVM及一个仅前端merge邻控分账。
本批是同构建精确回归，并未新增 owner pair 与 binding region 同一程序交叉形状证明。

严格两 crate feature all-targets Clippy 47.50s、默认 workspace all-targets check 28.28s
通过，后者覆盖默认 test gate；fmt、docs884及 diff 检查通过。尺寸以共同祖先为 base，
沿用现有额度，不新设例外。过程偏差独立记录：只读沙箱 ps/heredoc 临时文件操作被拒，
在既定 require_escalated 环境重新检查后才继续；没有部分源码修改。尾门禁脚本误名为
check_rust_file_sizes.py 导致 exit2，保留失败日志并补运行真实 check_rust_sizes.py；
不是 Rust 或能力测试失败，不因该脚本偏差重复 Rust 构建/执行。

证据保留于 `/private/tmp/koven-pair-binding-coverage-evidence`：manifest.json、两 selection、
原 Cargo JSON、两 exact 日志、runner、门禁日志及4215项冻结 tracked SHA。FE binary SHA
`97f04643494f6f2bc8e621ef26dee0ba1fbf702e18b4bbed9a443fb282468904`；CG binary SHA
`c04f616d432b63450f09506fcff6be2d449cbc666bb715ba89a0fbb95c85f100`。
收尾仅修改本 Spec；冻结对象与当前源码/配置分别核验，不能称收尾文档仍与冻结字节相同。
binding Complete 仍仅已形成 owner 后的指定区域，source/callable Partial、未接 binding
production receipt；不宣称全量/正式双宿主 CI、sanitizer、release 或资源证明，不推送/PR。

精确 `0a8945b1` 独立只读终审无新增阻断：双父/共祖与29+10路径并集、Spec尾段保全、
4215冻结输入、两个实际binary/raw artifact、13项名单/runner/原日志及默认CLI三空feature
均核对一致。收尾仅Spec变化，非docs零漂移；尺寸误名exit2与正确补跑分别保留。
独审确认上述有限native与借用FE-only分账及覆盖限制，随后仅docs本地提交，不重跑Rust。

### 单 binding Complete 受限生产核验合同（2026-11-10）

从 clean9c067f77 新隔离树实施独立 Binding receipt，只选择同轮、当前实际 caller 实例的
Complete binding region。source/callable仍Partial；observer-off、Partial、Unsupported或
缺席维持旧路径，不新增语言、ABI、route或cleanup责任。旧producer唯一执行。
entry_statement前绑定已形成的owner Slot，不声称覆盖StringLiteral形成；核原RootPlace、
source Begin、BorrowCall result、普通consumer复用result、result End、source End、owner Drop
七个真实SSA角色/四实体。consumer前端Begin/use-return不造第三SSA loan，不提前结束binding。
entry和completion须经过真实statement及cleanup；选中计划漏阶段/错实例/不闭合结构化失败。
独立family的RStmt tag随merge移动，在实际append_instruction取证，Alias无新指令；禁止Span
配对、回扫拼链或复用P4角色。保owner授权/push/提交及原Drop消费顺序。
先普通原样Complete+既有八SSA/LLVM成功，再断缺receipt形成独立红，之后最小接线及双Unit
真实on/off绿。普通小集合逐helper鲜审，foreign/fault/Abort/counter/压力等CI-only仅编译。
默认debug2/LLVM21、独占borrow target，不push/PR；production能力和覆盖认证分别留账。

#### Binding production receipt 实施与验收账本

合同头ef055fc2；独立红ca93d85c只新增测试与空观察stub，原样首先完成首Rooted布局的
on/off真实分析、旧facts比较、八SSA/verified LLVM与Complete七IDs/入口退出身份，随后
在binding_receipt_tests.rs154明确receipt0!=1，0通过/1失败/2914过滤。第二布局未达到，
不称双布局红前置均完成。no-run1m02s；该红独立于前批覆盖认证缺口。

生产1c93c68d引入独立Binding Plans/Emitted与tag，原binding初始化和退休职责机械提取到
build/borrow_binding.rs，statement wrapper移至build/binding_receipt.rs；去除新增核验hook
后旧算法保持。当前entry、实际source operand、commit、consumer快路径、retirement、Drop、
completion分别验证；不是七op计数替代正常body完成。emit在实际append取得身份，Alias也
核冻结实体。首green同exact通过1/0/2918，双Unit真on/off全字段完成，no-run39.94s。

最终代码冻结`bb4faa608d2fdc3bcffb7395909c78147f90fb32`，相对首green仅入口增加12行：
dead为空、this_dead=false、真实owner Slot为StringOwner Value。completion没有错误要求
已Drop后的dead为空。该加固让活owner前提显式，不称已发现普通源码bug；最终重新构建验收。

fresh审定8个唯一ordinary exact：新receipt主证/两个有限suffix边界、旧coverage主证、
causal、FE-only merge、P4、P3 Int/Unit真off、owner单参数授权邻控。最后8项均1通过/0失败/
0忽略/2918过滤。7项有SSA/LLVM共31次有限lower，1项仅前端merge；owner项虽位于native_tests
命名空间，本项不执行native。没有资源计数、foreign、注入、Abort或压力运行。新3项CI-only
覆盖catalog/漏completion或stage/实体重绑/consumer或retirement错配，仅编译不执行。

全部写入/build使用既定提权，独占borrow target，LLVM21 prefix、默认opt0/debug2；未设置
DEV_DEBUG/TEST_DEBUG/RUSTFLAGS、未复制target。显式loan-observation-test-support的最终
CG no-run18.91s，原artifact test=true且仅此feature。严格feature all-targets Clippy33.18s、
默认workspace all-targets check31.84s、fmt、docs884、size2368/35及diff通过。无单独CLI
artifact/release/正式CI实证，不沿用前合流结论越称。本片无Rust编译失败，API路径只读探查
有不存在文件的返回，不计能力红。runner最初未锁source清单，独审在执行前要求补HEAD与
4222逐hash、执行后binary/HEAD及manifest输入hash；补好再许可运行，未执行弱版runner。

证据目录`/private/tmp/koven-binding-receipt-evidence`保留两阶段原JSON/exact日志、
final-manifest.json、final-selection.txt、runner及4222冻结tracked哈希。最终binary SHA
`a4cb28c8e82407c5d3c6e28789e16f74efb5227e66345da214f7df6fe6c9af3c`。
收尾只改Spec与架构，冻结对象和当前非docs分别核，不声称收尾文档仍与冻结字节一致。
不改变语言/ABI/route/清理责任，不授source/callable Complete，不退休旧oracle，不push/PR。

精确bb4faa60独立只读终审无新增阻断：原artifact/manifest、执行后binary、8名单与实际
日志、4222冻结输入均核对一致；当前仅两docs不同，冻结docs对象仍匹配，非docs无漂移。
独审确认31次SSA/LLVM与FE-only/CI-only边界、真实红绿及入口加固分账，随后仅docs提交。


### 有界 P/Q 一跳绑定返回合同（2026-11-10）

从680215d3隔离准备，原prefix/While/一次Replace后Break不变；后继仅为
`val forwarded = held; return forwarded`，owned String返回。只扩既有Return产品的一跳
真实binding路径，不造alias producer，不引入新catalog或ABI，不泛化后缀/循环。
实际take_named先移出held；post-insert后记录alias真实OwnedValue；后续take_named必须匹配
该状态并移出alias，随后旧Return交付形成Take(root)→Bind(alias)→Take(alias)→Return。
旧scalar owner不回填P/Q；liveness保留alias跨语句，merge清carrier规则及失败撤销不改。
refine依真实Alias前/后回调核selected槽到alias槽，旧finish_return唯一发射并核两symbol
均dead、caller无额外Drop。原直返两动作和B/Call保持；不含Group、更多alias、借用/capture、
额外使用、分支或一般循环。先普通原样缺receipt红，再双Unit SSA/LLVM/native同源绿。
foreign、注入、Abort、计数及压力负控仅编译。当前仅静态准备，owner target由合流门禁
占用，未调用Cargo/native；本合同不声明实现或验证完成，不push/PR。


准备头d28cf60e的原样FE exact实际失败（receipt0应1，613filtered），raw保留red.log。
前端真实carrier实现58f784ab同源绿；该头fresh codegen exact随后以MissingFact失败
（2894filtered），证明旧两动作授权未接新产品，而不是仅补观察就宣称下游完成。
后端保原root槽与真实Alias槽，依post-insert/第二次take构成独立阶段，Return必须交付alias槽；
现已同源codegen绿。新增普通FE四动作及单P/单Q定义核验、六个合法未覆盖suffix邻控，
Basic/Const两次SSA/LLVM及两次正常native已通过；native输出outer/next，不作释放计数证明。
旧一跳alias拒绝项迁为两跳，完整旧含continue的边界测试未本地执行；新有限邻控独立覆盖。
原merge规则、B/Call和唯一finish_return不变。五类CI-only重取/重绑/Return破损仅编译。
尺寸门禁相对680215d3通过，2350手写Rust、35旧超千行；无新增超限或policy额度变化。
实现/测试未改main、680215d3冻结树或借用树；更全面回归、静态门禁与独立精确头审查待记。
证据保留于`/private/tmp/koven-owner-alias-return-evidence`，不推送或认领D10/E2完成。


生产冻结46336675d5d641126c50faa3804439ee5f61f84c；独立只读审其六生产差异未发现阻断，
完整终态证据仍须独立核验，不把生产审查当历史helper认证。两crate all-targets clippy
-D warnings、workspace all-targets check、fmt、docs884、尺寸及diff检查通过。
最终fresh allowlist为FE4/CG9，共13个exact全部通过（新alias5，旧B/Return/pair8），
每项均独立选择且日志确实运行1项。新alias五项内含FE一原源与六合法邻控、一次直接授权、
Basic/Const两次SSA/LLVM及两次native，不能把exact数量等同内部例数。普通回归只使用
本轮完整核过的helper闭包，不沿用旧大集合；非本清单测试与所有foreign/注入/Abort/计数/
压力/强制route不执行。新CI-only五类破损仅编译，正式双宿主CI和sanitizer仍未运行。
Cargo JSON保留opt0/debug2/test=true、features空及无profile覆盖；manifest包括source与
binary SHA、13项名单/结果、两份原始SOURCE与各自红头字节一致证明。两次实际红的raw
日志未覆盖。早期尺寸脚本名称误写导致文件不存在，纠正为仓库check_rust_sizes.py后通过，
不将该调用失败算尺寸验收。后续仅文档冻结，不重新编译Rust；无push/PR或一般owner完成声明。

### 2026-11-10 alias Return 与 binding receipt 本地双父合流验收

本批仅合流既有能力，不新增语义、ABI、路由或 cleanup producer。冻结合成代码为
`8fb578c8ff1b7f72ebe21c1db87a28a43d17eabf`，双父分别为借用
`38c1179fec7d16dd53ef9a839e4288c9c9592c16` 与 owner
`ceee6a510ff982098e039e36e7a0c455d64537c8`，共祖为
`680215d3017e0947dd8d86b9167a6c71854cd02a`；既有 pair 父链保留，未重复 cherry-pick。
主干及本地 origin/main 仍为 `9e7fed92`，本批未另合 main，未 push 或创建 PR。

两线修改路径仅本 Spec 相交。owner 正文保留共祖前缀，借用正文因此前合法合流并非该前缀；
实际解决为借用全文加一个分隔换行、再追加 owner 相对共祖的完整新增尾段。
其余 42 与 12 个非交集路径逐字保留对应父，生产代码无冲突改写。
4224 个冻结 tracked 输入的路径与 SHA 保存在仓库外证据目录；后续仅本 Spec 账本变更，
冻结对象与当前源码/配置分别核对，不称追加账本后的全部当前字节仍等于旧清单。

证据目录为 `/private/tmp/koven-alias-binding-receipt-evidence`：`manifest.json`、
`source-input-hashes.json`、两份 selection、原 Cargo JSON、exact 日志及 runner 日志保留。
输入清单 SHA 为 `81ec935efccf6c047cfab95ed9fec8c61fa25901ad6f2cfe79e2e34afd289671`。
FE 名单 SHA 为 `038d3c32eac201df0f9cf846f5ea39c6151972d9cbd94218ff28b4624ac3f5c7`，
CG 名单 SHA 为 `a5f7d6f47c01296df8d3960825863883c5b43bade6409c385db1be16d73e8ff6`。
实际 FE binary SHA 为 `a9031ad0bdd415bf8833f395b7cf803de269b86ed77bf88678c63890025d5320`，
CG binary SHA 为 `acfb6c7b0bbbcd71c83745863a4027e49459545765136ffa624da9958ef310f2`。

仅使用 owner target 串行构建，LLVM21、默认 opt0/debug2，无 profile 或 RUSTFLAGS 覆盖。
默认 CLI check 9.27s，原 JSON 中 frontend/codegen/kovenc 三实际 artifact 均 features=[]。
显式 loan-observation-test-support 的两 crate lib no-run 1m05s；实际 test artifact 均
opt0/debug2/test=true，仅该测试支持 feature。默认 workspace all-targets 29.21s，
显式 feature 两 crate all-targets 严格 Clippy 45.71s；fmt、docs884、尺寸2370文件/35项
既有超限及 diff 检查全部通过，未提高尺寸额度。

fresh 审阅后只运行 owner13 与借用8 的去重并集，交集为零：FE4、CG17，共21 unique exact。
每项先 exact-list 再单独 exact；FE各1通过/618 filtered，CG各1通过/2922 filtered，
原名单、manifest、runner 与成功日志同序一致。三项 native exact 分别运行2、2、8个有限程序，
合计12次，不额外累计为测试数；其余14项 CG 中13项合计46次 SSA/LLVM lower，另1项仅
frontend merge，不能声称该控制源码成功 lower。FE4 为固定普通 fixture 验证。
owner FE 比较只归一化重新规划的本地 observed 结果；loan 比较才 clone 两原产品清观察表。
真实 observer-off 重新分析所得原产品直接 lower，未关闭 receipt 或删表伪装 off。

所有选中本体及 helper 均重新审查；foreign、故障注入、Abort、资源计数、压力及 CI-only
均未执行，故障观察器保持默认未激活。此为同构建独立回归，不是新增 alias Return 与 binding
同程序交叉证据；不证明一般资源性质、正式 CI 或 release。binding source/callable 仍 Partial，
原 oracle 未退休。只读摘要命令曾因 sandbox 不允许 shell heredoc 临时文件而未执行，随后
经明确 require_escalated 执行成功；只读 ps 同样经明确授权后确认无 Cargo/rustc 活动。
这两次沙箱工具边界不计 Rust 或能力失败。
独立只读终审核对双父、冲突正文、冻结输入、实际 artifact/profile、两 binary 与全部21项
原始结果及上述分账，无新增阻断；最终仅本 Spec 文档提交，非文档输入未漂移。

### 2026-11-10 parent→child 直接别名生命周期：原样实证首阶段

本阶段从冻结 `6e606a9379443a980a5d8bef968b0dde42662382` 的独立分支开展，
只准备并验证现行语言普通源码，不修改生产或既有语义、ABI、路由、唯一 cleanup emitter。
固定来源为单个本地 String literal owner、`view(owner)` 得到的单个 result parent、
`borrow val child = parent` 的单个 Name child，以及 `consume(child)` 的单个普通调用。
没有 sibling、多层别名、wrapper、投影、控制流或 parent 后续使用；不以近似旧测试替代原样证据。

原样 probe 使用一个 MemoryDocument 的真实 names/types/ownership，正常选择 lowering 路由，
打印同轮 action catalog、handoff、transport、binding/end 事实及真实 SSA；通过 SSA verifier
与 LLVM verifier后，按实际实体核 BorrowCall result→SharedReborrow source、consumer 复用
child，以及 child End→parent End→source End→原 owner Drop。观察 routing 不强制 routing，
不按 Span 或相同 canonical owner 拼接身份，不运行 native、资源计数、故障或 foreign 查询。
本阶段结果待执行，不把 probe 成功等同于新因果链、Complete 覆盖或生产 receipt。

后续若获协调继续，需在真实 child 初始化正常完成处保存 parent activation/commit 身份，
在真实 child 移除处关联其 retirement；parent/source 退休与实际 Drop 继续沿旧唯一发布点。
新观察身份不得进入旧语义 Eq，丢失或歧义只能降级。因果事实仍 Partial，完整覆盖认证和
可选生产 receipt 必须各有独立能力红测；source/callable Complete 与旧 oracle 退休不在范围内。

| 本阶段验收 | 精确目标 | 实际结果 |
|---|---|---|
| 原样 parent/child 生命周期 | `ssa::loan_region_ssa_tests::parent_child_lifecycle_probe::original_parent_child_lifecycle_records_real_dependency_and_ssa` | 冻结d5d57e4f，单MemoryDocument实际通过，1 passed / 2923 filtered；SSA与LLVM验证通过 |

首阶段实际证据保存在 `/private/tmp/koven-parent-child-lifecycle-evidence` 的 probe-build.jsonl、
probe-manifest.json、probe-exact.log 和 runner；冻结头为
`d5d57e4fa7f8b5fe25e652b5636b6d45c4af9119`，生产零差异。仅该普通 exact 经 fresh helper
独审后执行，无额外测试。borrow target 串行、LLVM21，实际 test artifact 为 opt0/debug2、
仅 loan-observation-test-support；no-run 1m01s。执行前后二进制 SHA 为
`55acc019d8d60fe5efd4e0c05ebb0d4a751185c62a01d4eff85630d6a832e87f`。

原样实跑 route 为 view 与 run。run 的十条 SSA 为原 literal owner v0、RootPlace p0、source l0、
BorrowCall result l1、SharedReborrow child l2、consume(l2)、End l2、End l1、End l0、Drop v0。
测试按真实实体和相对顺序核验，不按 Span 拼接、不强制函数指令总数；未产生新的 consumer loan。
前端原 binding 记录为 parent s6 与 child s7，child.parent=Some(s6)、source_loan=None。
原七动作是 source Begin、parent handoff、consumer Begin、child End、parent End、source退休、
Drop；source退休有 source_begin=0、handoff=1、binding_end=4，而 use_return=None。
两个旧 call region 实际为 Unsupported，binding_regions 为空。这是缺失因果关联的现状实证，
不是新的能力红测或 Complete/receipt 认证；source Partial 仍由原产品合同决定，未新增覆盖断言。

后续接点已静态定位：binding.rs 的 initializer 正常完成与真实 live binding 插入；
borrow_result.rs 的实际 child 移除和 End 发布；refine lower_result_place 的 SharedReborrow。
旧 parent transport 只搬运一次 BorrowCall source token，不能为 child 复制 source transport。
同根多 binding 退休时原 Drop cause 被降级，后续需要真实 child→parent 退休依赖证据才可讨论
解除该歧义，不可直接放宽去重规则。本阶段不修改这些生产点，等待协调后再开展独立因果能力。
本轮 codegen all-targets 显式测试支持 feature 的严格 Clippy 36.45s，通过；fmt/diff通过。
独立只读终审核原始 artifact、实际 binary、唯一 exact 日志及真实 FE/SSA 身份，无新增阻断。
未运行其它测试、native、全 workspace/CLI/release 或正式 CI；最终仅补文档，不重跑 Rust。

### 2026-11-10 parent→child 的 Partial 因果发布片

本片续接原样实证冻结6944b4a4，不重复以probe绿替代能力红。仅补既有单String根、单
BorrowCall result parent、单Name child与单普通consumer的实际事件引用；不新增语言、ABI、
路由或执行责任。首个独立红须先通过真实 on/off 旧产品等价与十条SSA/LLVM，再因缺少
child commit/use-return/retirement 的实际因果引用失败。

child initializer正常完成且加入live bindings时，从其真实parent实例携带source Begin及
parent handoff句柄，记录child commit；child不具有source_loan，不新造source transport。
consumer实际正常返回移除pending call token时，按真实operand binding记录child use-return，
不是结束child或parent loan。child真正从live bindings移除时，以实际BindingEnd记录关联
child commit、已观察的qualified use及同parent运输身份。引用必须同catalog、先后正确，
不能按Span、canonical owner相等、最终旧表描述或最近同名binding补造。

观察身份不参与旧语义Eq；merge/重复或失配后的证据只能sticky降级，不得由后续一次use
恢复。原parent/source retirement与同根Drop因果降级规则不变，本片不解除该歧义。
source/callable仍Partial，既有parent-child范围没有Complete证书或production receipt；
缺引用不授absence。完整覆盖认证及receipt须另立能力红，不将三层合并为一次完成声明。
所有foreign/故障/Abort/计数/压力负控仅编译，普通执行限fresh审过的小集合。

独立能力红冻结 `4029f96bd43352ef9791aa70c00834da3bad6f39`，只有三种公共只读payload/variant
空发布stub与普通测试，无实际planner发布改动。`parent_child_requires_actual_partial_causal_links_with_true_off`
经fresh闭包审核后独立执行：no-run 55.13s、真实debug2；首Rooted布局已完成原on/off产品
等价、两次原产品lower/十条SSA/LLVM及Partial边界，随后在测试87行因所需10动作实际仅7
失败，0 passed / 1 failed / 2924 filtered。第二MemoryDocument布局未到达；这与原样probe
成功分账，不把该红写成语言/SSA缺能力。原始causal-red日志及manifest保留，后续实现不覆盖。

Partial生产代码冻结 `666c2d22141f45b24e7b945cbe14de08bf56ab96`；首次green no-run1m14s，
同独立核心exact通过，双Unit各真实on/off、旧产品等价与十条SSA/LLVM均成功。child只有
一个嵌入原parent transport的观察slot，旧Eq仍只比较binding/source；实际新动作由initializer
commit、normal-return pending提取、live child移除后的旧BindingEnd三个位置发布。
当前parent source/handoff与child commit都以同record身份核验，缺引用不从旧commit补回。
重复、future child/parent使用、分支或合流失证后保持Ambiguous/Lost；后一次正常use不能
再发布看似恢复的合格use链。实际child commit/retire可作为Partial事件保留，不声称所有
非目标源码完全没有观察。本原样的parent退休use_return保持None，同根Dropcause歧义规则未改。

最终冻结头 `055e93155562ffd300cfc9daade0875d4fb7bfc2` 仅比666c2d22追加实现说明/红账本，
Rust无漂移。仓库外同一证据目录的 final-manifest.json、final-selection.txt、final-exact.log、
final-runner.log 和原始 Cargo JSON 绑定该头；causal-source-input-hashes.json含4231个tracked
输入，SHA为 `7302810a6b3b5fa537253f28e6f64052548958d8b5cdfc46080a618dec880d61`。
最终名单SHA为 `56586e342775ce46f311baf9e9f2f8c6169ae17ec24e1a555c42fc322c60c647`，
CG binary SHA为 `2913b4676b2ea419ebe2b6131abd2df523ee754ecd2e0992fcaf6454193e6d64`。
两crate测试产物均实际opt0/debug2/test=true、唯一loan支持feature；最终fresh no-run0.14s，
不冒称重新编译耗时。原probe、独立红和首green的原始日志分别保留，不覆盖或合并计数。

最终fresh小集合仅五项，每项独立exact-list后运行：新因果核心、frontend-only merge、
旧单binding receipt、P4生命周期及P3 Int/Unit真off；五项各1通过/2925 filtered，名单、
manifest、runner和原日志一致。四项合计18次SSA/LLVM lower，另一项只验证frontend合流，
无native；未重复运行原probe。合流邻控要求恰1实际child retirement后核use_return=None，
并核旧parent transport恰1及child source_loan=None，不允许空集合冒充sticky失证证明。
两项新增CI-only（错catalog/角色/父/initializer/顺序与metadata Eq/失证恢复）只编译，
从未加入普通选择；foreign、故障、Abort、计数、压力及未审矩阵均未执行。

两crate显式feature all-targets严格Clippy50.36s、默认workspace all-targets30.99s通过；
fmt、docs884、尺寸2377文件/35项旧超限及diff通过。未增加尺寸额度；新公共API注册使
compilation_unit入口增加4行，复杂逻辑与测试保留独立模块。架构页200行，未突破页限。
本轮无Rust编译或普通测试假设失败；独立能力红按预期缺实际因果记录分账。
未单列默认CLI artifact、release或正式CI验收，不外推大范围控制/多child/权限恢复、
Complete或新production receipt；source/callable仍Partial，旧oracle与唯一emitter保留。
独立只读终审核原始红绿产物、4231冻结输入、五项实际日志及文档分账，无新增阻断；
最后仅本Spec提交，非文档源码保持冻结字节，未重跑Rust。


### 共同出口 Replace 结果 Return：受限合同（2026-11-10）

从本地组合 `6e606a93` 开始，保持既有 String P/Q、一次 Replace→Break 与共同出口，
仅研究尾句 `return replace(&held, "last")`。Guide 10 的既有 root Replace/owned Return
语义、ADR-0038 路径身份、语言 ABI 和旧 emitter 不变；不扩大循环、借用、一般 RHS、
alias、Group 或额外后继使用。此段记录实施合同，不宣称能力已经完成。

原样普通 probe 经真实 Basic 准入、SSA verifier 和 LLVM render 通过；旧编译路径可用，
但 owner observation 为 Unsupported，未发布 replace-break consumer。实际 suffix call
的旧 cleanup 是 `CallReturn` 对 held 的 Drop，不是 Return 的 ControlTransfer cleanup。
SSA 共同出口第二个 RootReplace 安装 literal R，第一结果随后 Drop，第二结果作为
Return payload；调用者普通接收与清理照旧。该结果不是 owner receipt 的能力绿灯。

需要由真实 String 完成、Value pending 注册、checked Replace commit、实际 root removal
及 payload evaluation 完成，依次证明 Create(R)→Pending(R)→Replace(P/Q,R)→
CallReturn Drop(R)→Return(P/Q)。P/Q 保留原两个 catalog，R 必须来自真实 producer 完成，
不得由 drop、AST 形状或旧 condition 的 P 投影倒造；Place 求值也不是 Take(root)。
旧通用 owner observation 在该控制边之后不活跃，不能假设它会生成 R identity。
后端必须绑定原 RootReplace 实际产生的 old-result slot，并在实际 Drop 新 installed
slot 后授权原 Return；不能将现在持有 R 的 root slot 当旧值，也不能重发 Replace/Drop。

实施先锁定普通同源 Phase 3 缺 receipt 红，再证明真实回调闭合；随后以 Phase 4 原 emitter
授权缺失红和同源绿证明消费，不通过扩大 admission 单独取得绿灯。故障、foreign catalog、
资源/计数器及强制路由只在官方 CI 执行，本地仅编译；普通 helper 每轮重新审计。


本地实施：Phase 3 `86ef47ba` 在真实回调闭合后使同源 receipt 红变绿；新 catalog 仅保存
R，P/Q 不重编号。Phase 4 同头普通原样测试先取得 MissingFact（1 exact），随后复用原
RootReplace post-push、installed-R Drop 回执与 finish_return 闭合授权。最终普通选择为
7 exact（FE2/CG5），内部包括4合法排除邻居、2种 Unit SSA/LLVM、2 native 程序和4旧
consumer 回归；不是7个源码样例。CI-only四故障仅编译，未执行；不继承历史大名单。
准备期 Rust Debug/字段访问、模块注册、AST marker 与 descriptor 引用编译错误，以及格式
排序失败均保留，不能算能力红；真实能力红只有原 receipt 缺失和后端 MissingFact。
原始日志、严格选择名单、默认debug2编译profile、源码与binary SHA保留于
`/private/tmp/koven-owner-replace-return-evidence`。冻结前后两轮
7 exact均真实每项1测试通过；生产头 `3ae388fa` 重建后的末轮包含旧Analysis on/off一致性，
默认debug2/opt0，未降profile。
两crate all-targets Clippy -D warnings、workspace all-targets check、fmt、docs885、diff与
尺寸门禁通过（2374手写Rust、35历史超限，未新增超限或提升预算）。静态首轮frame enum
体积警告以Box可选frame修正，Architecture索引遗漏也已补齐；原失败日志保留。
此后仅验收文档提交，不改变该生产头已测试的Rust字节。未推送、未跑官方CI。

### 本地合流验收：suffix Replace Return 与 parent-child Partial 因果链

- 本批仅保真合流，不增加语言、ABI、路由、cleanup 或认证范围。冻结合成头
  `422ecb8c21a0cceb73e183e071f91bc68eb30539`，双父为借用
  `6e3b5c7d681ad5353b2068d956d754a8cfd0666a` 与 owner
  `4c298fa12f0c1f06b06eb1f5207bf9e545911b72`，共基 `6e606a93`。
  两原树保持 clean；未合入新的 main，main 与本地 origin/main 仍 `9e7fed92`；主仓既有文档改动未动。
- 唯一冲突为本 Spec：两父均保留共基正文，结果为借用全文、分隔空行、owner 新增尾段。
  其余分支增量逐字保留对应父；没有重复 cherry-pick、squash 或新生产修改。
- 精确名单为 owner 七项与借用五项的无重名并集，FE2 + CG10 共十二项。
  每项及完整 helper 在合成树重新审阅；先核新产物 `--exact --list` 唯一命中，再逐项执行。
  FE 每项 `1 passed / 0 failed / 0 ignored / 622 filtered`，CG 每项相同通过数、`2931 filtered`。
  一个 native exact 包含 Basic/Constant 两个有限程序；其余后端明确 28 次 SSA/LLVM lower，
  另有一个仅 frontend 的 metadata merge 对照。程序数、lower 数不另累计为 tests。
- 真正关闭 loan observation 后重新 ownership analysis，原 off 产品直接 lower；旧产品比较仅在副本归一。
  owner FE compare 则对同 fixture 的本地重新规划结果清观察字段后比较，不是改原产品或借用式 clone helper。
  所有 ordinary 均未激活 foreign、fault、强制 route、Abort、资源计数或压力路径；CI-only 只编译未执行。
- 独占 `/private/tmp/koven-choice-replace-forwarding-target` 串行执行，LLVM21 前缀
  `/opt/homebrew/opt/llvm@21`；未设置 debug/RUSTFLAGS 降档、未复制或清空 target。
  默认 CLI check 9.16s，三个实际 frontend/codegen/kovenc artifact 均 `features=[]`、opt0/debug2；
  显式 `loan-observation-test-support` 双 crate lib no-run 1m04s，两 test artifact 均 opt0/debug2。
  严格双 crate feature/all-targets Clippy 42.99s、默认 workspace/all-targets check 27.15s；
  fmt、docs（885）、尺寸（2381 手写 Rust / 35 旧超限，base 为冻结头）、diff 均通过，未提升尺寸额度。
- 输入、产物及选择证据保存在本机 `/private/tmp/koven-replace-return-child-evidence`：
  `source-input-hashes.json` 绑定冻结头全部 4236 tracked 输入；`manifest.json` 绑定原 Cargo JSON、
  两 binary、精确名单及逐项退出码，原 stdout/stderr 和 runner 保留在同目录供本批复核，不入库。
  FE binary SHA256 `e1b8858843862f1c2fe738d54a3d1614fb2f951a046164ff9bfc616f330cefe4`；
  CG binary SHA256 `1096c52ca92f8042be6ed9063f3b93ebe442ddbd4d62c71e0e32185a86427048`。
  输入清单 SHA256 `5666b617a88c19efc8f04a8933004fa4a90b733381d860c7c72c2e888734d895`。
  最终仅本节文档追加区别于冻结输入；非文档源码、配置和 fixture 不变。
- 过程偏差：只读 SHA 核验的 shell heredoc 被默认沙箱临时文件限制阻止，授权升级后同一只读核验成功；
  另两次只读定位使用旧路径未找到文件，改为实际路径读取。均非 Rust 编译失败或能力红测。
  owner 原 Phase 4 红未在重建前捕获执行时 binary SHA，旧源码提交、artifact 与原日志仍保留；
  本轮最终 binary SHA 不补齐该旧红限制，没有重跑、补造或隐藏它。
- 本批是同构建中的独立回归，未新增 suffix Replace Return 与 parent-child 同一程序交叉源码。
  parent-child 仍 Partial，不授 Complete、Drop 全覆盖或新 receipt；source/callable 也未升级，旧 oracle 未退休。
  不外推资源证明、一般终止性、release 或正式 CI；临时原件仍需交付时按证据保留合同归档。
- 独立只读终审已核双父、合成字节、十二项真实结果、原 artifact/profile、binary 与输入/日志 SHA；
  最终仅本 Spec 偏离冻结输入，其冻结 git 对象及其余 4235 输入仍匹配，无新增阻断。

### 循环体 owned Return 终止边：原样实证与候选合同（2026-11-10）

从本地组合 `bda59963` 新隔离树，仅调查原样
`var held = "outer"; while(flag) { val previous = replace(&held, "next"); return held }; return held`。
本阶段只批准 probe/合同，不实施 endpoint、不放宽 loop validator 或语言准入。
既有 Guide 10 owned Return 与 Replace、ADR-0038 的终止边规则、语言/ABI 不变。

普通原样 probe 经实际 Basic 四阶段零诊断、SSA verifier 与 LLVM render 通过。run 的
SSA 只有条件与两个终止块：两条件边各带 P；true 进入 body 后原 RootReplace 的 installed
Q 直接作为该块 Return payload，detached P 在旧 AfterStatement(previous) Drop 后才
Return；false 块直接 Return 自身 P 参数。没有 body→loop-exit、正常回边或公共 owner join；
两个 Return 的 source span 独立。caller 原两次接收与各一次 Drop 未改变。
当前 owner observation 仍 Unsupported，没有 loop/replace-break 产品；编译绿不是新能力绿。

受限候选仅一个 Boolean 参数、一个 String 根、一次 Replace 后立即 owned Return；不含
Break/Continue、额外分支/后继、alias、borrow/capture 或一般循环归纳。
最小工程方案须先复用真实 P→Q producer/Replace 前缀，再由实际 Return payload evaluation
保存 Q delivery，旧 cleanup 完成、body 实际返回 false 后核终止回执；condition-false P
另在其真实尾句 Return 闭合。Return 不能冒充 Break，也不能把不存在的 Q 加入 exit incoming。
公共 endpoint 表示及封印仍待该实证检查点审议；不新开按组合命名的平行 getter，不修改旧
closed-body“全部 Dropped”validator 来接受已返回的 Q，不从清理事实倒造 producer。

原始 SOURCE、测试字节、运行前 binary SHA、默认debug2/opt0 artifact与完整facts/SSA日志保留在
`/private/tmp/koven-loop-owned-return-evidence`。本阶段未跑 native、未做能力红绿，CI-only仅编译。
### 已批准后继：parent-child 实际 Drop 依赖闭合与独立区域覆盖

- 从 clean `bda599637a45fd6c3136c661bcbade8cb4a0e6f8` 新隔离树实施，不重复原样 probe。
  固定单 local String 根、单 BorrowCall result parent、单 raw Name child、单普通 Unit consumer；
  不新增 siblings、多层、wrapper、控制或 parent/root 后用，不改语言、ABI、路由和原 cleanup emitter。
- 阶段一单独完成实际 Drop 因果关系，仍 Partial：child 的实际 remove/BindingEnd 发布退休句柄，
  沿原 parent transport 携至其实际 remove/SourceRetired；同一退休批次恰 child→该 parent，
  用同 catalog、same_record 的 source Begin/handoff/commit/use/retirement 身份合成依赖，
  不从 canonical root、Span 或 point 相等反查。child 的合格唯一 use-return 也是本片闭合条件。
- 同根去重继续 fail-closed：明确 PendingChild/Closed/Lost 证据状态；错序、重复、第三同根退休、
  缺链或歧义永久 Lost，不能把 None 当可补槽或选择最后一个 parent cause。
  ChildState 的 Retired 若携新句柄，合流亦须 same_record，否则 Lost；旧 transport Eq 仍只 binding/source。
- 最终边只能由实际 removed OwnerSnapshot 核验成功、旧 facts 去重确实新增后取得的 Cleanup handle 闭合；
  Drop 尝试、早退或去重未 append 不能借旧 Drop。只发布独立只读关系，保原十 actions 与旧投影，
  不添加执行动作、不授 absence/Complete、不接新 production receipt。
- 阶段一先独立缺关联红：原样真实 on/off ownership、legacy Eq、十 SSA/LLVM、Partial 前置成功后，
  因没有该关系失败；红 binary 在绿色构建覆盖前保存并独立实读 SHA。绿色再完整双 Unit 布局，
  逐 ID 核 child retirement→parent source retirement→new Drop 及原 source/commit/use 链。
- 阶段一完整终审冻结后才进入阶段二，另做缺 Complete 红→绿，不用同一红覆盖两项能力。
  新 parent-child 区域必须独立于旧七事件 binding_regions，避免旧 receipt 消费十事件而新增拒绝。
  entry 在 parent 声明前，已形成唯一活 owner 为正面前提，不宣称覆盖 owner 形成；
  completion 在完整 root block cleanup 后，scope0、全部 active义务清空，核未过滤完整区间及实际 Drop 关联。
  source/callable 仍 Partial，新区域不授权旧 oracle 退休或本片之外的生命周期。
- 每阶段仅 fresh 审定的小 ordinary exact；foreign/fault/Abort/counter/压力及 CI-only 仅编译不执行。
  borrow target 独占串行、LLVM21、默认 debug2，不降档、不复制 target；本地分步提交，不推送。

### parent-child 阶段一：真实退休到新 Drop 关联验收

- 基线 `bda59963`；缺关联红 `4b9f9511`，生产首绿 `c289ec6a`，最终代码冻结
  `bb20fc2bb93a0fb5b5abb04b3637cc48b5c6908b`。原树及 main 未改，本地分步提交、未推送。
- 实际 source PendingLoan 移除后、parent transport 建立前捕获唯一活 String owner 身份；
  metadata 不入旧 Eq，合流失配后不补采。child 的真实退休句柄随原 parent 运输到其退休，
  同批 PendingChild→Closed，错序/缺链/第三同根退休永久 Lost；原重复 root 的旧 cause 清空仍保留。
  parent 的私有附着只允许一次，重复附着永久清证。实际 removed owner 校验及真正新 Cleanup append
  才发布独立 `binding_dependency_drops`；去重未 append 不借旧 Drop，也不挑选重复关系。
- 原十 actions、十 SSA、旧投影和唯一 emitter 不变；关系逐 ID 核 source Begin/handoff、child commit/use、
  child retirement、parent SourceRetired 与新 Drop。source/callable 仍 Partial，旧 binding_regions 不变；
  本片没有 parent-child Complete、absence 权限或新 production receipt。
- 独立红实际 no-run 1m11s，opt0/debug2/test/support；仅核心 exact 失败 0/1/0/2932 filtered，
  断点是缺独立关系。此前首 Rooted 的 on/off、完整旧事实相等、十 actions/十 SSA/LLVM 已通过，
  第二布局未执行。执行前后及绿色覆盖前已独立实读红 binary SHA
  `fafd635a2dacd34ba61b379cf66a01838ce354aff5fea3c5ce5a4871cb56f56e`，不重演红测。
- 首生产 no-run 41.15s；核心 exact 1/0/0/2933，完整双 Unit 真 on/off 成功。
  最终 no-run 22.75s；fresh 审定六项逐 exact 均 1/0/0/2933，无额外选择。
  五项共 26 次有限 SSA/LLVM lower，另一个歧义 merge 仅 FE；0 native。
  later-parent/later-owner 两普通源保持旧产品且无关系，FE merge 也明确不发布关系；
  旧 single-binding receipt、P4 和 P3 真 off 为邻控。两项新增 CI-only 仅编译，未激活故障注入。
- 最终输入清单包含 4242 个 tracked 文件，SHA
  `b0278c10b46d78bd9a40901c933050f3014d0315d46d8fbae2d713a1106bc2c3`；六项选择 SHA
  `29b9e8ed6d4e44a876c42a5342389fa38606c093a663ad26703023aa4671813e`。
  实际 CG binary SHA `8a25d974eb0918f7787f66cb8f0de2bf80dcb2df9080de3eef124c310918f20c`，
  原 Cargo artifact 绑定本树 opt0/debug2/test=true、仅 loan-observation-test-support。
- 严格 frontend/codegen Clippy all-targets/support 38.55s；默认 workspace all-targets 32.40s；
  fmt、docs 885、尺寸（base 为最终代码，2387 文件/35 项历史超限）、diff 均通过，旧尺寸额度未提高。
  本片未另做 CLI artifact 空 feature、release 或正式 CI 验收，不据 workspace 成功外推。
- 过程偏差保留：红前 fmt 检查仅导入布局失败后已修正；红 runner 完成后外层 zsh 赋只读 status
  变量 exit1，不是第二能力红。首 Clippy 因新增 Option 方法的 question_mark 告警失败，
  等价改为 `?` 后重跑通过；最终六项使用修复后新构建，不沿用首绿 binary。
  只读定位两次旧路径不存在、默认沙箱 ps/heredoc 限制分别以授权进程检查/无临时文件读取完成，
  均非 Rust 或能力失败。完整原件在 `/private/tmp/koven-parent-child-drop-evidence`，
  含各阶段 JSON/log/manifest、源码与选择哈希；交付时仍须按证据保留合同归档。
- 上述阶段一独立冻结后才继续阶段二缺 Complete 红；当前因果关系成功不自动升级覆盖认证。
- 独立只读终审已核真实红/首绿/最终六项、原 artifact 与执行后二进制 SHA、4242 冻结输入及
  26 份原件哈希；最终仅两 docs 偏离清单，冻结对象和其余 4240 输入仍匹配，无新增阻断。

### 已批准阶段二：独立 parent-child 完整区域

- 从阶段一 clean `e5a99113b4230715a510d4c4fb33af388cb53262` 新隔离树实施；
  不改原树、不合 main。独立缺 Complete 红以前一阶段真实 Drop 关联、十 actions/十 SSA/LLVM、
  双产品真 on/off 与完整 legacy Eq 为前置；红二进制在覆盖前独立实读 SHA，不用旧红代替。
- 精确普通非泛型无 receiver/参数的 Unit caller，四个原始 statement：已形成 local String owner、
  BorrowCall result parent、raw Name child、普通 consumer(child)；不扩 siblings、多层、wrapper、
  控制、replacement、parent/root 后用或额外动作。既有语义、路由、ABI 与唯一 emitter 不变。
- entry 在 parent 声明实际进入之前，只把已形成唯一活 owner 及真实 token/声明作为正面前提，
  不声称覆盖 owner 形成。显式核 parent/child 两声明与 initializer、source/consumer 精确 typed 映射。
  exit 在完整 root body drop_scope/borrow-end wrapper 后，scope0 且全部 active 义务为空；
  不在 consumerReturn 提前封口，不绕 owner collector 的保存恢复或后续正常清理。
- 认证必须检查原始未过滤 [entry,end) 十记录、两 binding facts/endpoint 全集与本轮完整 dependency：
  child 无 source_loan，parent 只有原 source transport；source/handoff/commit/use、各退休与新 Drop
  的同 catalog 引用全部在本区间内。不能凭 parent.use_return=None 或局部关系成功推全部覆盖。
- 使用新独立 parent-child region 类型/集合；旧 binding_regions 与七记录 receipt 消费器完全不选新区域。
  缺边、额外动作、未完成或 active 残留不授 Complete；失败/deferred 沿现观察产品全撤销。
  新区域只认证该有限区间，source/callable 仍 Partial；本片不接新 receipt，不授全函数 absence，
  不退休旧 oracle。两阶段红分别保留，阶段一 Partial 产品不自动升级。
- 仅 fresh 审核普通小集合本地 exact，foreign/fault/Abort/counter/pressure 与 CI-only 只编译；
  借用 target 独占默认 debug2/LLVM21，不降档、复制或清理；本地分步提交，不推送。

### parent-child 阶段二：独立 Complete 区域验收

- 新隔离树基于阶段一 `e5a99113`；独立缺覆盖红 `371e356b`，生产 `8cdfcf48`，
  oracle 类型修正后最终代码冻结 `323fa9b7dc87fc8dddc26b8f6ad4dc082f3e20ca`。
  原阶段一树不改、main 不合，本地分步提交，未推送。
- 新 `UnitBorrowChildRegion` 与 `child_regions()` 独立于旧 `binding_regions()`，不触发七记录 receipt。
  entry 是 parent 声明真实进入前的记录游标，唯一已形成 String owner 是前提而非被覆盖形成动作；
  completion 在完整 root body cleanup wrapper 之后，scope0 与全部 active 义务为空。
- 精确四原始 statement、parent/child 两 initializer 各唯一事实与各唯一端点、无入区旧 active、
  出区全部 active 空及完整未过滤十记录联合证明本区域全集；不以两个 symbol 的过滤结果单独推全覆盖，
  也不要求全 source 恰两 binding 而误拒其他 callable。原普通 fixture 直接核全产品两 facts/两 ends。
  validator 逐 catalog ID 核所有依赖引用，并从已验证 cleanup ID 取唯一真实 AppendedDrop.removed_owner，
  与入区 Some owner token、callable、声明等全字段核对；不由 Span/root 相等拼接清理。
- 红 no-run 1m08s 成功、opt0/debug2/test/support；唯一 exact 实际 0/1/0/2934 filtered，
  首 Rooted 的真 on/off、legacy Eq、十 actions/十 SSA/LLVM及阶段一七个依赖 ID 已先通过，
  在缺独立 child region 处失败，第二布局未达。覆盖前独立实读红 binary SHA
  `bad1f7de3214f7bf7195cb1a1d1898e48e18092453a4a41588091af5b03d7f96`；未重演红。
- 首 green 编译成功 27.85s，核心 exact 1/0/0/2935，完整双 Unit 真 on/off 与公开字段 oracle 通过。
  最终 fresh no-run 0.14s，七个 fresh 审定 ordinary exact 各 1/0/0/2935，名单无额外选择。
  六项共 30 次有限 SSA/LLVM lower，另一个 merge 邻控仅 FE；0 native。
  额外第五句 ping 保十 actions/一个真实 Drop 关系但不获 Complete，later parent/root use 与 merge 亦不获；
  旧 single-binding receipt 与 P3 真 off、P4 同源双 Unit 邻控保留。两项新增 CI-only 仅编译，未执行故障构造。
- 冻结输入 4248 个 tracked 文件，清单 SHA
  `232d9603340b936925413d135f2431e1fd744825fb65591f1d2c89dd6fcc4e27`；七项选择 SHA
  `9363b3d8b0ecb8289009eb30a836e918aa17f6e868bab09fe8b82a9f3cb45525`。
  实际 CG binary SHA `297bba32bccba9e153553245c3ab919b478a4c7c5118ab584f9f7c534818e5ac`，
  原 Cargo artifact 绑定本树 opt0/debug2/test=true、仅 loan-observation-test-support。
- 严格 frontend/codegen Clippy all-targets/support 40.86s；默认 workspace all-targets 33.24s；
  fmt、docs 885、尺寸（base 最终代码，2393 文件/35 项历史超限）、diff 通过。
  旧大文件仅模块注册与薄 hook 增长，额度/拆分计划未变；不外推 CLI 空 feature artifact、release 或正式 CI。
- 过程偏差：首 green no-run 的新测试将 BorrowSourceLoan 误与 UnitLoanFact 直接比较，E0308 后仅改为
  完整 call/argument 身份比较，保存失败 JSON/log，生产无改；这是 oracle 编译错误，不是能力红。
  一次只读 exec transport 断连，经环境列表确认 Mac connected、attach 同 ID 后恢复原 dirty 树；
  两次只读旧模块路径未命中后定位实际模块，均未重跑 probe 或覆盖代码。
  全部原件在 `/private/tmp/koven-parent-child-coverage-evidence`，含各阶段 artifact/log/manifest 与输入哈希；
  交付时按证据保留合同归档。本区域 Complete 不升级 source/callable，不新增 receipt 或退休 oracle。
- 独立只读终审已核两阶段边界、真实红/绿/最终七项、raw artifact/profile 与执行后二进制 SHA；
  最终仅两 docs 偏离 4248 冻结输入，其冻结 git 对象及其余 4246 输入仍匹配，无新增阻断。


### Suffix Replace→Return 回执安全回归（2026-11-10）

仅为既有共同出口 replacement consumer 增加 cfg(test) 故障注入：漏/重
post-RootReplace 回执、old 错作 root、Drop 错用 old、漏/重 installed-R Drop 回执。
全部故障用例只在官方 CI 执行，本地仅编译，不能称为拒绝路径已实跑。
普通正控仍使用原样有限源码、同轮实际 Unit 产品与 SSA/LLVM；不改公共产品、存储、
getter、loop validator 或语言 ABI，不实施待决定的开放 loop Return endpoint。

本地测试头 `3b3c1699`：六个新增 CI-only exact 已由 `cargo test -p lang-codegen
--lib --no-run` 编译，全部未执行；原样 `actual_old_slot_requires_authorization` 与
`both_units_authorize_distinct_old_and_installed_slots` 两项普通 exact 各实际通过，后者
覆盖真实 Basic/Const，两者验证 SSA/LLVM，不代表六条故障拒绝路径通过。codegen all-targets
Clippy `-D warnings`、fmt、docs 与尺寸门禁通过；debug=2，无 profile/RUSTFLAGS 降级。
准备时首次 no-run 漏设 target，已停止本任务默认目录编译并改约定独占 target，未执行测试。
本批未运行 native、官方 CI，也未推送；原循环体 Return probe 冻结头保持不变。

### Owner receipt 安全钩子与 parent-child Complete 的本地合流验收

本轮只合并已批准两条本地父链，不新增语义、路由、cleanup producer 或 child receipt。
合成代码头 `43cf8b4592e176e33dd1067a382ee59a21d0b112` 的双父分别为
`6abfda9ad85ad39ffd393c93805d8013d1c5eb33` 与 `1872ef9787ef67290db8dbc91c560c50052492af`，
共祖 `bda599637a45fd6c3136c661bcbade8cb4a0e6f8`。只在本 Spec 尾部冲突；两父均保留共祖正文，
合成精确保留借用全文、分隔空行及 owner 新尾段，其余路径逐字保留对应父内容。
原树不改，main/local origin/远端 main 核为 `9e7fed92`，没有自动合入、push 或 PR。

- Owner 增量为三个 Rust 文件的 test-only 六故障钩子/测试；新增导入及注入点均为 `cfg(test)`。
  普通观察器不设置 `FAULT`，其线程局部默认 `None`；六个新 CI-only exact 仅编译，绝不本地执行。
- Fresh 审核 owner 两个普通正控与借用七项，去重后九个唯一 exact，无交集；每项独立进程
  先 exact-list 验唯一，再 exact 执行。九项均为 `1 passed / 0 failed / 0 ignored / 2941 filtered`。
  Owner 两项共五次 SSA/LLVM lower；借用六项共三十次 lower，另一个仅 FE merge；总计三十五次
  SSA/LLVM lower 加一个 FE-only test，零 native。命名空间含 native_tests 不表示执行本机程序。
- 真 off 为重新运行所有权分析、原 off 产品直接 lower；legacy comparator 仅克隆后归一作比较。
  P4 所选项只有同源双 Unit 正常 lower，不冒称其有 off 对照。额外语句/后用及 FE merge 保留旧路径。
- 本轮是同一合成构建中的独立回归，没有新增 owner suffix 与 parent-child 同一源码交叉证明。
  Child 只有独立区间 Complete，source/callable 仍 Partial；旧七记录 receipt 隔离，无新 child receipt，
  旧 oracle 未退休，不授全 callable absence。

证据目录 `/private/tmp/koven-safety-child-coverage-evidence` 保留原始 JSON、日志、runner、名单及哈希。
`source-input-hashes.json` 绑定合成头的 4248 个 tracked 输入，SHA
`7bbedb2433ba458af4c2fb8c26610d6780553a2aa348af4cf9f78dd0c782b994`；文档收尾后仅本 Spec 改变。
九项 `final-selection.txt` SHA 为 `8326f9dae17749014a5504655ca8f367253045348a01fe0f0b1bdc6ada740868`。
CG 二进制实读 SHA `92c13cdfd2cd50a6afd1973403c955f7d88bfc93b544ec23422e88072ee074f7`；
FE 编译产物 SHA `0301413536786bef61a396895463558887757a099a9aa302e7b93db28051f1e9`，未执行 FE 测试二进制。
实际 test artifacts 均 opt0/debug2/test=true，显式唯一 loan-observation-test-support feature；
默认 CLI 原 JSON 中 frontend/codegen/kovenc 三 artifact 均 features=[]、opt0/debug2。
默认 CLI check 8.91s、feature no-run 1m06s、strict frontend/codegen all-targets Clippy 42.02s、
默认 workspace all-targets check 28.92s 均成功；fmt/diff exit0，docs 885，尺寸 2393/旧超限35通过，
未提高历史尺寸额度。独占原 owner target 串行执行，没有复制 target、降调试信息或运行正式 CI/release。

本轮无 Rust/行为失败。初次普通 sandbox ps 被系统限制，获准的提权只读核进程成功后才使用 target；
旧 skill/模块只读路径未找到后定位真实路径，不改变代码或验收范围。阶段1的首红只 Rooted、外层 zsh
status 偏差与 question_mark 风格失败、阶段2的 E0308 oracle 编译偏差及 owner 初次漏 target 的准备偏差
均保留原父账本和日志，没有重跑红、补造哈希或用本轮最终产物覆盖历史证据限制。
独立只读终审已逐项复核双父、冻结输入、实际 artifact/binary/九项日志与最终文档分账，无新增阻断；
审阅者未执行项目测试。文档收尾不重跑未变 Rust，保留原代码头与最终文档头的区别。


### 已批准：独立 parent-child Complete 区域的生产 receipt

从冻结 `6f8eb32a` 新隔离分支实现，不改原发布头。消费范围仅当前 callable、同 catalog 的
Complete child_regions，旧七记录 binding receipt 保持独立，source/callable 仍 Partial。
Owner 实际形成只是 entry 前提；区域自 parent 声明前到正常根 body cleanup 后。
九个真实 SSA 角色为 RootPlace、source Begin、BorrowCall parent、SharedReborrow child、
consumer DirectCall、End child、End parent、End source、Drop 原 owner；五实体分别是
owner/place/source/parent/child。完整函数仍十 SSA，不把前端 consumer Begin 伪造为第三 acquisition。

准入核原十 action IDs、真实 dependency 七引用、两 binding/end 与 caller/两 callee 的 typed 合同。
Parent 真实 initializer/source operand/commit 及 child 原 Name initializer 各自绑定实际 Slot；
child 无 source_loan，created sources 为空，parent 恰一 source。普通 consumer 复用实际 child Slot，
不依据 canonical root 或 Span 猜配。退休按真实 binding remove 对应 child→parent→source 三 End；
Drop 仍原唯一 producer，核其唯一 index/Named root/owner，completion 必须在真实 body cleanup 后。
独立 tag 随 RStmt 进入 append，冻结九 InstructionIds 与五 EntityIds，拒绝跨家族双认领或 alias 替换。
缺 Complete/off/Partial 不激活新核验；不新增路由、语言、ABI、cleanup 权限或公共 owner endpoint。

独立能力红先以原样 dual Unit/on-off 实际产品通过旧 Eq、十 SSA/LLVM、完整十动作、dependency、
独立 Complete 及公开边界身份，再断生产 receipt on=1/off=0。初始 stub 只返回空观察列表；
若首 Rooted 即缺 receipt，红只记录已完成该布局前置，不称第二布局已跑。红原 artifact/source/binary
须在 green 重建覆盖前独立实读核验。实现后原选择转绿并补九角色/五实体精确 oracle，普通小集合
逐 helper fresh 审查；所有 foreign/fault/Abort/counter/压力/CI-only 仅编译，绝不本地运行。

#### Child receipt 本地验收账本（2026-11-11）

独立红 `2e3796f6e74a391a508814a4467c142036199d9c` 仅新增 Spec、test-only 空观察面与普通测试，
无生产接线。no-run 1m16s 成功，唯一 exact 在第278行因 on receipt `0 != 1` 真实失败
（0 passed/1 failed/2942 filtered）；首 Rooted 的 on/off 两次 lower、旧 Eq、十 SSA/LLVM、
十动作/dependency/独立 Complete 全字段先通过，第二布局及末尾 off receipt 断言未达。
红 CG SHA `e080ccde4fcd50ffed2a088b44df9385d7a5b91b529eef87d515a5b89343d131` 已在执行前及
执行后、green 重建覆盖前由独审实读；原始 Cargo JSON/manifest/log 与七个 source/helper 哈希保留。

生产与强化 ordinary oracle 冻结为 `d97cc3f70b01159b9b9267518b2f1fe2ca37dd46`：
- 独立 child Plans/Emitted 及 Child tag 消费当前 callable 的同 catalog Complete；十原动作与 dependency
  引用、typed caller/callee、两 binding/end 及唯一 Drop index 均核验，不复制或扩大 frontend 认证。
- 真实 parent initializer/source operand/commit、raw Name child initializer/SharedReborrow、consumer
  借用快路径、实际 remove 两 binding、三 End、原 Drop 与正常 body completion 分别提供身份见证。
  Child sources 必为空、parent 恰[source]；consumer 复用 child，无新增 SSA loan/cleanup。
- 实际 append 冻结九 InstructionIds/五 EntityIds，核验同块次序，拒绝跨族双认领和 alias 替换；StringLiteral
  是区域外 entry 前提。旧七记录 receipt、原 producer、frontend、route 与 owner 公共 endpoint 均不改变。
- 原样首 green no-run 42.52s，唯一 core 1 passed/2946 filtered，完整 Rooted/MemoryDocument 真 on/off
  与 receipt 全字段、九指令/五实体 oracle 通过。最终 fresh no-run 0.13s，输入/二进制不变。

最终名单为两个 owner suffix 正控、新 child core、extra ping、later parent/owner、FE merge、旧 binding
receipt、P4 及 P3 off，九唯一 exact 均 `1 passed / 0 failed / 0 ignored / 2946 filtered`。
八项 SSA/LLVM tests 共35次 lower，另一个仅 FE merge，零 native；owner 两项虽在 native_tests
命名空间，实际只做五次 lower。P4 仅同源双 Unit，不宣称其真 off；后端 off 对照均使用原产品直接 lower，
clone 只用于 legacy 比较。Extra/later 无 Complete 时观察新 receipt 为空，FE merge 仅前端证据。
四个新增 child CI-only 负控及既有所有故障项仅编译，capture TLS 默认 None，普通 helper 未激活故障。

证据 `/private/tmp/koven-child-receipt-evidence` 保留 red/green/final 原 JSON、binary SHA、输入及运行日志。
4254 tracked 输入清单 SHA `aead10ece96285e3988af78d022dadf560330e99952ee92b689fd6dc4994d7f6`；
九项名单 SHA `2ee2344239033e3424852391b679f59d1479c399bbc3e73843a4493d7ba46c6f`；
最终 CG SHA `b7a2ded8036c075ef3183f3febd43b73c8096eb2fc48e82481af1ce371aae887`，
FE SHA `55b97b2bf82f41e8013b489b10a92d20c934bdcbb69569d50768073e87373f96`（只编译，未执行FE二进制）。
实际 artifacts 均本树 opt0/debug2/test/support；strict FE/CG all-targets Clippy 44.29s、默认 workspace
all-targets 33.11s、fmt/diff exit0、docs885、尺寸2399/旧超限35通过。body951/build982/expression991，
无新增超限或 policy 改动。本片未单独做默认 CLI artifact 空 features、release 或正式 CI，不沿用上批冒称。

本片无编译/green行为失败。一次普通 sandbox 的只读 heredoc 临时文件受限，获准提权读取后成功；
只读旧模块定位错误随后改用实际路径，不计能力失败。所有写/build/运行均按既定授权提权，借用 target
串行默认 debug2，无复制/降档。红不重演；原冻结发布头不改，本片仅本地提交不push。
独立区间 Complete 与生产 receipt 不提升 source/callable Partial，不退休 oracle、不证明 native资源释放计数。
独立只读终审已逐项核验生产实际接点、冻结输入、原 artifact/binary、九项结果与两文档分账，无新增阻断；
审阅者未运行项目代码。文档收尾仅提交文档，不重跑未变 Rust。

### 开放 loop 的已交付 Return endpoint（2026-11-11）

用户明确批准统一内部公共产品，不改变 Guide 或 ABI。唯一开放 loop 存储采用
`UnitOwnerOpenLoopObservationV1::{BreakLive, ReturnDelivered}`；旧 replace-break getter
只投影 BreakLive，既有 closed-loop getter/validator 仍只证明原封闭 body。ReturnDelivered
不伪装 Break、不发布 live-Q body completion 或共同 incoming，不新增组合专用平行存储。

首片沿已保留原样 probe：唯一 Boolean Name 条件、一个 String 根 P、body 一次 Replace
产生 Q、detached P 仍按原 AfterStatement 清理，然后 body Return(Q)；条件 false 独立
尾 Return(P)。共享原 P→Q admission/capture 与六事件前缀校验，不扩一般循环或表达式。
Phase 3 从真实 Name Consume/交付、旧 Return cleanup 和 body 实际 Ok(false)记录封印；
false 尾 Return 从原条件状态独立交付。两 endpoint 绑定各自 catalog 与源身份，不合并编号。
Phase 4 复核同轮 typed/owned primitive、真实 producer/binding/RootReplace 后槽、detached
P Drop、两个实际 Name delivery 和 Return terminator。仅 false 路径能到 tail exit；body
必须以 Return 终止，不能有 body→exit/backedge。旧 finish_return 仍是唯一发射器。

基线 `71df5df8` 保留 probe `ff85760f` 与已审集成 `6f8eb32a` 双父；原冻结树不修改。
后端独立缺 endpoint 红 `38c1258c`：旧 SSA/LLVM 成功后 body Q 无授权，单普通 exact
实际 1 failed；前端 `90281c46`：旧诊断/cleanup/观察 on-off 保真后开放产品 0≠1，
单普通 exact 实际 1 failed。两轮均执行前保存 SOURCE、test source、artifact 和 binary SHA
及独立 binary 副本。以上是生产前红阶段记录；后续本地验收见下文，正式 CI 未运行。

本地收敛：FE生产 `b9ecf1e3`、字段文档 `45f47326`，后端 `af5de689` 首轮五普通
exact通过；统一enum尺寸lint后改单层Box payload `bb7167b6`（外层不再Box，无lint豁免），
最终代码头 `5dc30937` 保留规范import顺序。最终16唯一普通exact全部实跑通过：

- FE 11：`open_loop_return` 三项（两sealed端、同Unit多source/重分析自有catalog、五个
  合法邻形），`loop_replace_break_baseline` 一项，`replace_break_grouped_condition` 的
  B/Call/Return三项，`replace_break_alias_return` 与 `replace_break_replace_return` 各一项，
  `closed_while::real_while_body_publishes_closed_transport_and_actual_backedge` 和
  `closed_while_conditional::real_while_tail_if_records_both_completed_jump_edges`。
- codegen 5：`owner_loop_return_tests` 的 `loop_owned_return_requires_two_actual_endpoint_receipts`、
  `loop_owned_return_both_units_deliver_two_separate_endpoints`、
  `loop_owned_return_native_preserves_each_path_value`；`owner_replace_return_tests` 的
  `suffix_replace_return_both_units_authorize_distinct_old_and_installed_slots`、
  `suffix_replace_hooks_preserve_existing_ordinary_consumers`。每exact各1 passed；内部
  Basic/Const、两native程序及旧四consumer不另算exact。旧原样probe没有重复选跑。

全部按fresh完整helper审计精确名单运行；fault/foreign/撤销等CI-only只编译，不宣称其拒绝
行为实跑通过。最终两crate lib no-run、两crate all-targets Clippy `-D warnings`、workspace
all-targets check、fmt、docs（886页）、尺寸（2398 Rust、35历史超限、无新增超限）、diff通过。
保留debug2/opt0，不降profile或RUSTFLAGS。未跑正式CI、未推送，也未并入已发布PR。

准备失败单列：首次FE编译6处旧CI-only字段借用冲突，改显式prefix；第二次3处新测试缺
import，补真实类型导入；均仅编译失败无测试执行。首轮strict Clippy的large-enum失败已
保留并以单层Box修复。两阶段预期红、各阶段原始artifact/日志和执行前binary/source SHA
分别保留，最终产物不覆盖红证据。独立终审与后续合流仍由协调任务完成。


### 单 wrapper live-source 原样前置实证合同（2026-11-11）

基底 `c85ac4b9` 包含 main `03d70377`、owner `39835135` 与借用 `11681a0a`，
在独立 `feature/spec-0341-forward-loan-lifecycle` 树准备，不改合流树或生产逻辑。
本阶段只有原样 probe；不因编写测试或本合同宣称已有新因果链、Complete 或 receipt。

固定唯一 explicit Return wrapper：`view(source)` 返回 Shared 参数来源，
`wrap(source) { return view(source) }` 将该 live token 交付正常返回；caller 以
单本地 String owner、单 borrow val 和单普通 consume 使用结果。选择显式 Return
是为了锁住现有 `finish_borrow_forward_transport` 的唯一已知实际发布点，不同时扩展
表达式体、Group、多层 wrapper、控制、provider、receiver、投影或 Inout。

新单 exact 使用已审普通 parsed/analyze helper，双 rooted/memory 原样正常分析；
先核旧 forward 实际 source/call/argument/returned/Return、caller 与 wrapper 来源分离、
Shared 参数无 payload owner/Drop，以及已注册 Begin 不被当作 source End。再经正常
Unit 入口观察实际 route、SSA、verifier 与 LLVM render，不预设路由或操作数量，
不修改 facts、force route、运行 native 或调用 foreign/fault/counter/pressure helper。
新观察 API/生产发布未改，本 probe 不是缺新 API 的能力红。

当前代码的具体缺口：实际 PendingLoan source 移动点丢弃 acquisition handle；
实际 wrapper Return 只写旧 `borrow_forward_transports`，没有同 catalog 的 forward
动作因果引用。probe 完成后再根据实际产物收窄后继合同；handoff 始终交付 live obligation，
不得记作 End，不新增 cleanup、语言/ABI、公共 owner endpoint 或退休旧 oracle。
source/callable 仍 Partial，覆盖认证与生产 receipt 必须分别取得后继证据。

执行前先提交输入并独立审源码/helper；fresh no-run 后绑定 artifact、binary SHA 与输入，
再仅执行这一个普通 exact。复用既有 borrow target，default debug2、LLVM21、串行 Cargo，
构建前重核进程/锁/资源；不复制 target、不降 debug。所有 CI-only 仅编译，未运行项另记。

#### 原样显式 Return 的真实拒绝与 probe 范围纠正

输入 `7a57052e` 的唯一 ordinary exact 经源码/helper独审和原artifact/binary/input绑定后执行：
fresh no-run 91.25s成功，opt0/debug2/test、仅loan-observation-test-support；执行结果
0 passed、1 failed、2951 filtered、exit101。首 Rooted 的旧forward/live非End、caller分离
和source Partial前置均成功；正常lower在测试112行报UnsupportedNode，SourceId0的
87..93原文为wrap声明的borrow marker。Memory布局、SSA verifier与LLVM均未到达。

真实拒绝路径是unit_lower::orchestrate先调用validate_declarations，再由
borrow_result_support::declaration仅接受FunctionBody::Expression；Block wrapper在
任何refine路由或函数SSA构造前已被拒绝。这是原样lower成功假设失败，不是新因果能力红，
不据ROUTED空推定Compatibility，也不通过改生产gate或force route消除拒绝。
旧输入及原始日志保留在 `/private/tmp/koven-forward-loan-probe-evidence`；
binary SHA `6d8339e3c796b8e17a6ca0605fb25a19348769d75811f73e55e69729c1450175`，
执行后4261输入及binary SHA不变，并已经独立实读核验。

经协调后只收窄后继原样probe为单表达式体 `wrap(source) = view(source)`，其余三声明、
caller与普通使用不变，不同时扩展其他语法。新exact名明确为
original_expression_wrapper_records_live_handoff_and_actual_lowering，另存expression阶段
artifact/log及输入绑定；新源码差异/helper审与fresh artifact绑定完成后才执行。
表达式体仍从实际PendingForwardTransport移除处发布live handoff，不能成为source End。
若该新原样成功，只为表达式体后继Partial因果链提供前置，不宣称显式Return准入已修复。

#### 单表达式体 wrapper 原样 probe 结果（2026-11-11）

冻结输入 `a1d35fb6` 无生产改动；经fresh源码/helper审、新artifact与runner/input/binary
绑定后，仅执行original_expression_wrapper_records_live_handoff_and_actual_lowering：
1 passed、0 failed、0 ignored、2951 filtered、exit0。Rooted与Memory两布局均实际完成
普通FE、正常Unit lower、SSA verifier与LLVM render，各LLVM文本1389bytes。
实际refine成功观察为view、wrap、run，不以未观察的其他函数推定Compatibility。

wrap的真实SSA只有一条BorrowCall，从入口loan l0得到result l1，再BorrowReturn l1；
没有额外BorrowBegin、BorrowEnd或Drop。caller run仍八条SSA，consume复用result，
随后result End、source End、原owner Drop。前端旧forward恰一，source Begin存在、
Shared参数不取得payload owner/Drop，caller来源独立，source整体Partial。
此结果证明表达式体原样现路径，不把FE source Begin硬映射成新增SSA acquisition。

已证实后继缺口仍是source acquisition handle在真实pending move时被丢弃，以及实际
wrapper交付只写旧forward表而无同catalog动作引用。本阶段没有实现该因果链，也没有
新增coverage、Complete、production receipt、observer-off、native或资源计数证据。
显式Block的既有拒绝保持；本次成功不倒填修复前一输入或将其失败认作能力红。

fresh no-run的Cargo时间16.89s、外层wall16.94s，实际opt0/debug2/test、仅
loan-observation-test-support，沿用原borrow target及LLVM21，未复制或降档。
4261冻结输入SHA `b7696f89f5440743657c085c219e41f7ee69e3345b191f9810f66606464aad53`；
CG binary SHA `8cc6de35d8f4846a9711eda219b5baaba53f6257a6f2d220a34dcf10f35ebd6f`。
执行后input/binary SHA保持，并已独立实读终核；原日志、JSON、runner、manifest保留在
`/private/tmp/koven-forward-loan-probe-evidence/expression`，前一失败原件未覆盖。
所有CI-only仅编译，未执行；未运行额外Rust测试、Clippy/workspace行为、正式CI或native。
终核后仅本结果账本可不同于冻结输入，不重跑或修改Rust；后继生产须另定精确合同。


### 单表达式体 wrapper 的 live-source Partial 因果合同（2026-11-11）

沿用已冻结成功的唯一 `wrap(source) = view(source)` 与原 caller；本片不改变语言、
声明 gate、SSA 或 ABI，不扩显式 Return、Group、多层 wrapper、控制或 unwind 的因果认证。
旧 forward 全家族的合法事实必须完整保留，窄 scope 只决定附加证据，不能筛掉旧事实。
source/callable 仍 Partial；source Begin 不是额外 SSA acquisition，live handoff 不是 End。
本片不新增 cleanup、Drop、Complete region、跨函数 LoanId 映射、receipt 或旧 oracle 退休。

生产接点固定为真实 PendingLoan remove 的第三 acquisition handle，随同一
PendingForwardTransport 移动到真实 finish remove；不得查找等值 Begin、Span 或旧 fact 补边。
wrapper 身份来自真实 FunctionBody::Expression 进入/交付 scope，与 source call、returned
及 ControlTransfer 的精确 body identity 一致；不是 inner callee 或外层 caller。
独立 validator 核同 catalog、引用早于 handoff、Begin record role、完整 source token、
SourceUnitId/SourceId 与同 wrapper scope。失证只降级，不推断 obligation 已终止。

公开 `BorrowForwardHandoff(UnitLoanForwardHandoff)` 只携原 transaction 及经验证的
source_begin/callable 查询；私有 Arc 与 index 保持 catalog-bound ID，结构 snapshot Eq 不比较
Arc 指针。PendingForwardTransport 的 Eq 仍只比较原 returned/source；合流保留原 retain，
再独立核唯一匹配与同 handle/scope，缺失、歧义或冲突使 metadata sticky None，永不回填。
实际 handoff record 是旧 forward 表唯一来源，无筛选保序投影；observer-off 仍保存全部
原 raw transaction，只关闭新公开观察及因果认证。不改变错误/deferred 的全单元撤销门。

先只提交 API stub 与独立普通 exact
`expression_wrapper_requires_live_source_causal_link_with_true_off`，不接 producer。
两个 Unit 布局各先正常 analyze，再真实 observer-off ownership/validate；核完整旧产物相等、
原 forward/source live/no End/no owner Drop，原 on/off products 各正常 lower、SSA verifier、
LLVM render。明确核 wrapper 一条 BorrowCall 加 BorrowReturn、caller 原八条与实际 route，
双布局四次 lower 的全部前置完成后，才末端断言缺失的新 handoff record 及其 causal link。
这使独立能力红不会掩盖 Memory/off 前置，也不借旧原样 probe 的绿冒充能力实现。

新源码/完整 helper 经 fresh 独审后方可 no-run，fresh artifact/input/binary/runner 绑定后
仅执行此 ordinary exact；原 probe 不改，红 binary 覆盖前先独核。CI-only、foreign/fault、
Abort/counter/pressure 均不本机执行。红实证与生产方案独审成立后才接最小生产逻辑。

首次冻结 `9342f611` 的 fresh no-run 在新测试 helper 252 行遇到 E0308：frontend LoanKind
导入遮蔽 SSA model::LoanKind；这是测试编译错误，没有执行 ordinary exact，不是能力红。
该阶段原 build JSON/stderr、输入表保留在上述证据根目录的 `causal-red/`；后继仅给
frontend 枚举显式别名并重冻输入，新的构建/执行证据另存，不覆盖首次失败。

#### 独立 live-source 因果红实证

修正后的冻结输入 `015585de` 经 fresh 源码/helper 审、成功 no-run、原 artifact 与
4262 输入/实际 binary/runner 绑定，仅执行上述唯一 ordinary exact：0 passed、1 failed、
2952 filtered、exit101，精确停在测试130行新 handoff record 数量0≠1。Rooted/Memory
两布局各 on/off 四次正常 lower、原 wrapper/caller SSA 核对、verifier、LLVM1389bytes，
以及完整旧产品、SSA/LLVM一致性均已完成；实际routes均为view、wrap、run。新
callable/source_begin后置尚未到达，不把缺记录之外的身份验证称已过。

成功 no-run 外层66.23s、Cargo1m06s，默认opt0/debug2/test、LLVM21及仅support feature。
输入SHA `f81bb988d2621d789aa33000804b9cbebc576204851eee094083ceaab720ca07`；
红binary SHA `149aaae62237ea10cf506d3236d5853d327327d1a07a9ea9122998bac264710b`。
执行后及覆盖前已经独立逐输入/实际binary终核；完整原件保留于证据根 `causal-red-fixed/`，
旧编译错误与两原样probe原件仍独立保留，不重跑红或用后继green覆盖。

后继scope只在真实表达式体入口一次建立：raw Name callee、唯一无命名/模式标记的
raw Name Shared参数实参，无显式类型实参、receiver、Group、控制或额外调用。
所有后继helper只能降级，不能重新从AST建立Some；scope与source_begin作为原子cause，
完成scope丢失时一并失证。ValueState的owner私有穷尽解构只显式忽略此纯metadata，
不添加owner资格条件；这不改原数据流或扩大控制认证。

#### live-source Partial 核心接线实证与有限邻控

生产冻结 `49c3cc9e` 经六文件全文独审、新artifact/4263输入/实际binary/runner绑定，
同一原红exact未改字节并实际1 passed、0 failed、2952 filtered、exit0。双布局四次
true on/off旧产品及原SSA/verifier/LLVM1389bytes前置、末端同catalog/先后source_begin、
实际wrap callable与Partial均通过。旧原样probe保持字节不变，显式Block gate未改。

成功no-run外层159.98s、Cargo2m39s，默认opt0/debug2/test、LLVM21及仅support feature；
输入SHA `a79bca24e2b8cbd04bb0660fc22bbff705fc0a0d570b6ea98d24f84ac5615c78`，
binary SHA `ffbc42da5d061316d0c331b912974e431c65995f0fc1e2f93e40ecdb128be579`。
实际结果与前后SHA已独立终核，原件保留在证据根 `causal-green-core/`。源码检查的
size guard以df334ae6为基通过；原大planner仍1638行净零增长，policy与旧例外不变。

后继新增两个ordinary exact仅验证FE旧投影：
`original_block_and_group_forward_facts_survive_true_off` 与
`original_branch_returns_keep_both_raw_handoffs_without_a_cause`。普通parsed/analyze/真实off
核双布局完整旧产物相等，显式Return/Group及分支返回的旧raw事务和顺序完整保留，
窄scope外两个新因果getter均None；不将这些Block输入送入不支持的lowering。
两个新CI-only守卫分别覆盖无效catalog/index/record/token/kind/target/point/SourceId/scope
及构造合流的same-clone/不同或缺失句柄/歧义/丢scope/sticky降级，只计划编译，禁止本机执行。
本小集合尚待fresh源码/helper/名单审及新artifact绑定，不以核心绿代替邻控验收。

有限补强冻结 `ca97505d` 经fresh源码/helper/名单审及FE/CG实际artifact绑定，固定三项
ordinary exact各1 passed、0 failed、2954 filtered，全部exit0/stderr空；两FE邻控不做
Block lowering，原显式Return/Group/分支raw事实保序且因果getter None，核心真off与新引用
仍绿。两个CI-only由FE lib no-run成功编译，未运行；此项不证明负控拒绝行为已实跑。
FE no-run60.30s、CG178.22s（Cargo1m00s/2m58s），默认debug2/LLVM21。4267输入SHA
`b926391863b6fececbb95b60f251e161cc58e1d073c36de661856b82c35ffa45`与实际FE/CG
binary前后均已独审终核，原件在证据根 `causal-validation/`。

本片追加全部静态：ca975的两crate/all-targets显式support strict Clippy通过136.53s，
default workspace all-targets通过62.97s，docs/size/diff通过；workspace fmt唯一失败
是本片新公开导出带来的mod.rs import换行，不涉及其他文件或生产逻辑。原日志保留在
`static-gates/`；后继仅修该排版，须新冻输入并重新验证静态及绑定三项，不能沿用旧头
的行为结果冒充新输入通过。未跑全量行为、正式CI、native或独立CLI artifact。


#### live-source Partial 最终同头验收

最终代码输入 `0e23edac` 仅修上述导出排版；相对前头的Rust去空白内容及全部测试字节
不变，经fresh独审后重新执行全部静态、串行FE/CG lib no-run并绑定固定三项ordinary。
workspace fmt、两crate/all-targets显式support strict Clippy、默认无feature的workspace
all-targets check、docs、size（基线c85ac4b9）和diff六项均exit0；docs核887页，size核
2410个手写Rust文件及35个历史超限文件，无新增超限。Clippy外层42.33s/Cargo42.09s，
workspace check外层25.70s/Cargo25.48s，原件保留在证据根 `final-static/`。

FE/CG no-run外层分别4.96s/15.87s，Cargo分别4.91s/15.82s；各自唯一实际artifact均为
本树fresh=false、opt0/debug2/test、仅support feature，原target/LLVM21/jobs2串行，
无debug或Rustflags覆盖。4267个冻结输入逐hash及聚合SHA
`9ad96915bb3ab1ec3fad58541a5446f9447086c3719f5a52f8296904cf175558` 已独立核实；
FE binary SHA `95d31970e66f8b8f36f4136311c9e6ba57f4ca01ba7c0a69f6cbbfe6c98ca693`，
CG binary SHA `32b2ae34caa3910c8d686eb2d69905c611592f836d627ad4be3a8a685e3dff42`。

新artifact绑定后仅执行上述固定三项CG ordinary exact，各1 passed、0 failed、
2954 filtered、exit0，三份stderr均空。核心双布局四次真实on/off正常lower、原SSA、
verifier、LLVM1389bytes及全部新因果后置通过；两邻控仅FE，旧Group/Block/分支raw事实
保序，scope外因果getter均None。前后输入与实际binary保持，结果及静态已独立终核；
完整原JSON、输入表、runner、list/run日志及manifest保留在 `final-validation/`。
FE binary未执行，两个CI-only仅成功编译，不宣称负控拒绝行为实跑；无全量行为、正式CI、
native或独立CLI artifact。本片仍为单表达式体source live handoff的Partial，不新增
Complete、receipt、End或显式Block lowering。执行输入固定为0e23edac；后继仅本账本
文档提交，不把文档新头冒充已执行输入，合流仍须按真实合流输入单独验收。


### #410 后主干、开放 Return 与 child receipt 本地合成验收（2026-11-11）

- 现场核 main、origin/main 与远端均为 `03d70377fcbb099e0c10ede8977c55124c773d6f`，
  其 tree 与已发布 `6f8eb32a` 相同；只创建隔离树，未由本任务改动原树、推送或发布。
- `f927c836` 保留 main 与借用 `11681a0a` 双父，`72822865` 再保留 owner `39835135`
  父链；最终代码验收头为 `c85ac4b908a2a402229d28dafa20ebf498aaa9f4`。
  两能力相对共同基线的非交集文件为借用18、owner34，逐字保留各父；交集只有
  `refine/build.rs` 的独立模块/计划字段初始化与本 Spec。build 文件985行，未调整尺寸额度。
- 本 Spec 不是双纯尾段：owner另有中段原样probe账本。合成保留该段、owner尾段及borrow尾段。
  初次拼接前缀断言失败后，外层shell未停止，临时merge提交含冲突标记；随后纯文档提交
  `c85ac4b9` 删除三条标记并保全两侧，所有构建/测试均在修复后的clean头执行。
  中间头不算验收，不改写历史掩盖该过程偏差。输入清单首轮因git quoted中文路径读取失败，
  改用 `git ls-files -z` 后保存4260项；该失败发生在Cargo启动前，非Rust失败或能力红。
- fresh逐源/helper审定实际24唯一 ordinary：owner16与borrow9有一个共同dual-slot exact，
  因而FE11、CG13，不按25补测试。每项均先exact-list核唯一，再独立exact进程执行。
  全部通过；仅一个native exact发出/链接/运行两个有限程序，每个都验证false和true路径结果。
  另有44次显式SSA/LLVM lower与一个借用FE-only merge，native不混入显式lower计数。
- 普通observer不启故障capture；所有CI-only、foreign、fault、counter、压力及未审集合均未执行。
  P4项是同源双Unit正常lower，非off；后端off对照使用原产品直接lower，比较副本不回灌。
  FE owner比较重新规划的两个本地结果，仅清观察字段作比较，不称其为后端clone对照。
- 默认CLI check实际12.48s，原JSON中frontend/codegen/kovenc三artifact均features=[]。
  显式loan-support双crate no-run实际1m29s；两test artifact均opt0/debug2/test=true，
  未降档、未设置RUSTFLAGS、未复制target。严格Clippy47.22s、默认workspace all-targets
  33.08s、fmt、docs886、尺寸与diff全部成功；尺寸比较基线为上述main，旧额度不变。
- 原件保存在 `/private/tmp/koven-main-open-return-child-evidence`：两selection、run-exacts.py、
  4260项source-input-hashes、default-cli原JSON/三artifact、feature-build原JSON、manifest、
  两exact日志与runner/static日志。实际CG SHA为
  `4846ae42b72418bbd48ffc9bdae4be9f00a3bfa012e5bfeb244412791405360b`，FE SHA为
  `794041c8e424cb74874666d2f0ac771daf5b76a8f9cd9a97693dcb6c8f4099d2`；执行前后均核一致。
- Child仍仅同轮独立Complete区间的九角色/五实体核验，source/callable仍Partial；旧七角色
  receipt隔离，原清理发出次序保持，oracle未退休。Owner开放Return保持两个独立终止endpoint，
  不伪装共同join或Break。合成本轮证明同构建独立回归，没有新增同一程序混合两shape证据。
  各父历史红与偏差保留，未重跑红、未补造历史缺失哈希；正式CI/release未运行。
- 独立只读终审已核三头父链、合成字节、24项实际结果及全部产物/输入哈希与文档分账，
  无新增阻断；本次收尾只改本Spec，不重跑已冻结Rust。

### Parameter owner 进入 ReturnDelivered（2026-11-11）

仅一个 Boolean Name 条件参数和一个显式 own String 参数，后者经真实 Name Move/Result
与 var Binding 成为 P；body 一次 Replace 的 Q、detached P Drop 和两 Return 路径仍用
既有统一产品。P 只由实际函数入口的既有 Parameter origin 定义，Name 不再定义 producer。
后端从实际 bind_parameters 的 Value 槽取证，不将 Boolean 改为 Value；不扩大旧 Break、
closed/invariant gate，不新增 producer 种类、公共 getter 或并行存储，不更改语言/ABI。

从已审合流 caf1b1db 新隔离树开始。原样 probe `6b550155` 单普通 exact 实际通过：
Basic 合法、SSA/LLVM正常；入口为 Boolean SharedLoan 与 String Value，callee 只构造 Q。
P 的入口值分别进入 body/false，body 原 RootReplace 后 Drop detached P 再 Return Q，
false 独立 Return P，caller 仅清理两个接收结果。没有 body→exit/backedge/common join。
metadata 尚 Unsupported，probe通过不是能力绿。首次 `8b88aa88` 错把Boolean也断言为Value，
原始源码、artifact、binary SHA和日志已保留；只修oracle，未改ABI，不算缺能力红。

本轮局部能力已实证：`0ddc0b1c` 的 FE 原样缺产品红（0≠1）由 `bcd85b99` 转绿。
输入 catalog 保留函数入口的真实 Parameter 定义；Move/Result/Binding 仅运输该 P。
通用 catalog 的 Parameter origin 不授予旧 family 准入：legacy factory、Break 和五类
invariant validator 显式保留 String 条件，Parameter 候选不回落 closed/invariant。
旧公共 getter/storage 与原循环出口/cleanup 合同保持不变。

后端 `4cc1ad59` 先取得原样缺终止回执红；FE 接线后再于 `19a1f520` 的独立测试取得
真实参数入口链为空的红，不把已有 Name result 当 producer。`4da04ddb` 从实际
bind_parameters 的 Value 槽绑定 P；同轮 typed 复核两个参数及 Boolean SharedLoan，
Name result 核对原槽，实际 deliver 后核参数已 dead，再承接旧 Alias root。Q、旧
RootReplace、detached P Drop、两个独立 Return emitter 与终止回执均复用原路径。

测试代码头 `69fe482e`：本轮 fresh-helper 审计的 14 个 ordinary exact 全部通过
（FE 5、codegen 9；测试内部的 10 个旧 String shape 不另计 exact）。包括真实
Basic/Const 两路、参数入口与两 Return 回执、原 SSA cleanup 顺序及有限 native
`outer`/`next` 输出。FE damage/foreign 测试仅编译，未本地运行；正式 CI 尚未验证。
两 crate all-targets strict Clippy、workspace all-targets check、fmt、docs 和相对
`caf1b1db` 的 Rust size guard 均通过，无新增尺寸豁免。保存每阶段原 SOURCE、原始
artifact、覆盖前 binary 副本/SHA、精确名单和逐项日志；首次 probe oracle 错误保留。
本轮是局部 Parameter 输入能力，不代表一般动态 owner/循环归纳能力完成，未自行发布。

### Parameter owner 与 wrapper live-source 本地合成验收（2026-11-11）

- 新隔离树 `feature/spec-0341-parameter-forward-local-combination` 的代码冻结头为
  `8e2a853ad06959616a8e3384177b1f7be6828148`，真双父为 wrapper `d25430ff` 与
  owner `87341a0a`，共同祖先 `c85ac4b9`。现场远端 #412 仍 open/Draft，head
  `caf1b1db`、base/main `03d70377`；新头保留该 PR head 祖先，本任务没有推送或合入 main。
  main 的11份 proposal 脏改及未跟踪 docs/scripts、两原树和另一借用 target 均未修改。
- 唯一文本冲突是本 Spec；本轮实核两父均以共祖正文为前缀，合成严格为 wrapper 全文、
  分隔换行与 owner 新尾段，完整保留已有合流及两能力历史。解决冲突并核无标记后才提交。
  另一个路径交集 `owner_transport/loops.rs` 自动合并，精确为 owner 版加
  `forward_scope: _` 的两行观察元数据说明/忽略，不改变 Parameter 准入与 active 义务。
  其余 wrapper16、owner15条非交集路径逐字等对应父；没有新生产算法或新语义。
- fresh 审定实际17个唯一 ordinary exact：FE5、CG12，两父候选无重名。
  每项均先 exact-list 核唯一，再独立 exact 进程；FE每项1 passed/638 filtered，
  CG每项1 passed/2959 filtered，全部0 failed/0 ignored，runner末总结17。
  两个 native exact 共4个有限程序，每个程序均验证 false 和 true 路径结果；不作资源计数证明。
  另计17次显式 SSA/LLVM lower（owner13、wrapper4），native emit4次单列；
  wrapper两项邻控仅FE，不冒称 Block/Group/branch 正常lower成功。
- wrapper核心在两个 Unit 布局真实重新执行 observer-off ownership，原 on/off products 直接
  lower；legacy comparator 仅对副本归一，不回灌。旧 forward 全家族事实仍保序投影，
  窄表达式体 live-source 因果仍 Partial，不新增 Complete、receipt、cleanup 或跨函数 SSA 身份。
  owner保留实际 Parameter entry→Name运输→root Alias及两个独立Return，旧String-only门不松。
  本轮是同构建独立回归，没有新增同一程序混合 Parameter owner 与 wrapper 的交叉证明。
- 默认 CLI check11.01s，原 JSON 中 frontend/codegen/kovenc 三实际 artifact 均 features=[]。
  显式 loan-observation-test-support 双crate no-run2m28s；两 test artifact 均opt0/debug2，
  不降档、不设RUSTFLAGS、不复制target。严格Clippy1m22s、默认workspace all-targets1m08s、
  fmt、docs887、diff均成功；尺寸相对main `03d70377`核2412手写Rust、35旧超限，额度未改。
  owner target独占串行；另一0403等待日志脚本未触碰，没有停止任何其他任务。
- 原件保存在 `/private/tmp/koven-parameter-forward-evidence`：4269 tracked输入清单、
  merge-inputs、两selection、run-exacts.py、原feature/defaultCLI JSON、默认CLI三artifact摘要、
  manifest、两exact日志及静态日志。输入清单SHA为
  `0d401b50a9bb47d4e28d01639e7bdb1f0b11c509c452d04c0122aa83848b4cee`；
  FE binary SHA `4a2e649b12e7590798e3dad22c48c1d5714e599ffeee695195031fb03283917e`，
  CG binary SHA `65d68ccbe0ffee4bcfe8349af5b6411eb11a614d5b55f95f22d8eb5b6a07d82a`。
  执行前后HEAD及binary一致；收尾仅本Spec可偏离冻结输入，其余源码/配置不变。
- 本轮构建纳入的 CI-only、foreign、fault、Abort、counter、压力项仅编译、未执行；
  其余未审集合未执行，不宣称已全部编译。
  各父原样假设失败、编译错误、能力红绿及历史证据限制保留，不重跑红或补造缺失证据。
  本轮没有Rust失败或能力红；只读定位曾遇旧helper路径不存在，改查实际路径后读审，
  不影响项目输入或门禁。正式CI/release及发布仍由后继流程处理，本任务不宣称完成。
- 独立只读终审已核双父并集、17项实际结果、全部artifact/binary/输入/名单哈希及文档分账，
  无新增阻断。此后仅本Spec收尾提交，不重跑冻结Rust，也不自行发布。

### 正常回边的当前 owner：原样实证与待确认合同（2026-11-11）

从已验 `b331d044` 新隔离树开始。`3174d43f` 的普通原样 probe 通过 Basic 严格准入、
SSA verifier 与 LLVM render；源码使用 `held="outer"`、`again=flag`，body 一次
`replace(&held,"next")` 后 `again=false`，最后 Return held。false/true 分别有限零/一轮。
`6ec42380` 的精确 SSA 正控再通过：entry 向 header 传 P，body 向同一 header 的真实
normal backedge 传 RootReplace installed Q；header true/false 都运输当前 header 参数。
body 在回边前 Drop detached old，false 出口 Return 自己的 owner 参数，未增加公共 join。
同头另一普通 exact 使用既有真实 Return 观察器失败于缺当前 header owner 的授权回执。
原 SOURCE、artifact、覆盖前 binary 副本/SHA、原始日志与精确选择均已保存；这是真缺
consumer 授权红，不是源码不合法，也不是任何有限轨迹的归纳证明。尚无生产实现。

拟议最小方案尚待产品边界确认：在现有统一 open-loop enum/storage 增加 normal-backedge
有限变体，不新增 producer 或组合专用顶层 getter。H 代表本次 header 当前来源，零轮
入口传 P，正常回边传本次激活新 Q，false 出口传 H。body 对任意合法 H 证明先形成 Q、
Replace(H,Q)、旧 H 一次 Drop，真实 body Ok(true) 和 scope cleanup 后仅 Q 存活并回边。
header 不变量为完整 String owner 且无未完成 pending/loan；不能靠 `again=false` 代替
闭包证明。旧共享 prefix 的 false=P 与固定 UnitOwnerId Return endpoint 不适用，不能
伪装成 invariant 或两个独立 Return；需要该变体自己的 header-source 交付端点。
Boolean assignment 也仅能按真实 typed RHS 正常完成接点窄准入，不放开一般 Assignment。
旧 closed/invariant/Break/ReturnDelivered、原 emitters、语言与 ABI 保持原义。

正常回边边界已按 accepted ADR-0038 §2/5/6 确认属于现有架构内的实现选择。
本片仅既有 String root、局部 Boolean 控制、一次 Replace 后 Boolean 赋值及正常回边；
H 是统一开放循环内的来源参数，不是新增 producer 或 P 的同义词。独立产品须核真实
entry/body completion/condition-false/tail delivery，精确收敛任意合法 H 的 Replace、
旧 H Drop 与新激活 Q 回边。旧 closed/invariant/Break/ReturnDelivered 不放宽。
先补 FE 缺产品红；原来 consumer 缺权限红保持原字节，未有生产绿前不声明已覆盖。

正常回边 FE 本地实现：在唯一 open-loop enum/storage 发布 `NormalBackedge`，
保留两个真实 catalog，不生成 H producer；`incoming()` 给出 Entry(P)/Backedge(Q)。
真实 prefix、body 正常完成、Boolean 赋值完成、false 原状态、tail take/delivery/cleanup
共同封闭。完整六事件体规则中旧输入只经 Replace→detached binding→Drop 消费一次，
新 Q 独立形成后仅回到根；规则不按旧输入身份分支，可用任意已初始化 H 替换首遍 P。
当前准入仍限该 String/Boolean 原样，不代表多根、Continue、任意条件或一般循环已支持。
原 FE 缺产品红已转绿；四项普通 exact（原红、来源方程、四合法邻控、十旧族邻控）
各1 passed/642 filtered，严格 frontend all-targets Clippy通过。新增构造损坏项仅编译。
记录在既有 normal-backedge evidence 的 fe-final-manifest 与逐项日志；首次恢复修复的是
旧测试未适配 enum variant 而导致的三处编译错误，未改旧族实际断言。
backend header Return 授权仍未完成，原 consumer 红不宣称已绿；联合门禁、独审、新main
组合及正式 CI 仍待后继验证，不标记此片或 D10/E2 完成。

### 单wrapper正常LiveForward出口覆盖合同（2026-11-11）

以发布冻结b331d044为基线独立分片，只复用已验单表达式体String wrapper原样源码，
不扩同构wrapper、Block lowering、控制、receiver、泛型或ABI。已批准ADR0040允许通过
明确合法live出口闭合已声明范围；本片独立region可Complete，source/callable仍Partial，
没有End、权限恢复、production receipt、旧oracle退休或新的cleanup执行责任。

独立窄准入核wrapper与callee各唯一Borrow String参数、唯一结果来源参数0、raw Call/Name，
以及唯一实际BorrowReturnOrigin的body/root/声明marker；不能缩旧Partial scope或旧raw家族。
真实item表达式入口保存catalog内区域句柄及records.len游标，建立时所有资源边界清洁。
expression正常完成后、drop_all之前保存完整义务快照；唯一允许残留是携本候选及原Begin
句柄的真实PendingForwardTransport。真实remove/append返回确切handoff句柄；原drop_all
仅执行一次，之后核清态和completion游标。不能把drop_all的clear掩盖成覆盖证据。
独立validator核全部未过滤区间恰为原Begin与live Handoff、同catalog/先后/完整token、
入口与前后边界及唯一返回来源；缺边、重复或未完成永久不认证，不从相等事实搜索补链。
旧表保序投影/off/原Eq和全部错误/deferred撤销门不变，P1至P4及binding/child区域独立。

先API stub及独立ordinary exact original_wrapper_requires_complete_live_exit_with_true_off：
Rooted/Memory四次真实on/off analyze/lower先全部完成，核旧产品、原Partial因果链、真实
route、entry loan→BorrowCall→BorrowReturn、caller原SSA及verifier/LLVM一致，再断新区域。
未接producer前只期待缺新region的真实能力红，不重跑旧probe冒充红。fresh源码/helper及
artifact/input/binary/runner独审后只执行该exact；红binary覆盖前独核。所有foreign/fault/
Abort/counter/pressure/CI-only仅编译，不本机执行；后继生产须真实红及方案独审成立。


#### 独立LiveForward缺覆盖红实证

冻结d72c422b经fresh源码/完整helper/API及实际artifact/input/binary/runner独审，唯一
ordinary exact实际0 passed、1 failed、2960 filtered、exit101，在测试112行独立region
数量0≠1失败。Rooted/Memory四次真实on/off原Partial链、原样SSA、verifier及LLVM1389bytes
均先完成，实际routes四次均view/wrap/run；新区域Complete及字段后置尚未执行。
CG lib no-run外层170.41s/Cargo2m50s，原target/jobs2/LLVM21、opt0/debug2/test、仅support。
4271输入SHA `4fd5f027390dd7d7ec258ebbc5afea017be5476d8cb2c68e7727af2c3e7b931f`，
实际CG SHA `943d6287a7c84c9207d0fd6efccbe339c4f9e12243ea686743b7df298ffec3cc`。
原件保留在 `/private/tmp/koven-forward-exit-coverage-evidence/red`；执行前后及覆盖前已独核，
不重跑或覆盖红证据。该阶段仅编译CG test及FE依赖，不声称FE CI-only测试已编译。


#### 单wrapper局部出口生产与验收

生产 `e1bd1d9b` 在原动作源记录真实 entry、pre/post 边界及 handoff 句柄，唯一原始
forward/source 查询消除歧义后才允许 Complete。原红 exact 未修改，首绿实际1 passed。
最终代码 `6d53fd17` 补新region的11类CI-only构造负控（仅编译）及Group/Block/branch
普通旧形状不认证断言；三个ordinary exact各1 passed、2960 filtered，零native。核心
双布局四次正常on/off SSA/verifier/LLVM成立，另外两项仅FE；不把FE Begin当作新SSA acquisition。

最终输入4274 tracked文件，清单SHA `9488e597604caa3ba5ad56daef178c964d4a319268f993ff5871186b28bb713e`；
CG binary SHA `4aec912c81d26be512b5041301f359ec969db34846d0199e0ed5d9d1f6259f85`。
保留原target、LLVM21.1.8、opt0/debug2与显式loan support；执行前后输入及binary一致。
原件在 `/private/tmp/koven-forward-exit-coverage-evidence/{red,green,final}`，包括原Cargo JSON、
tracked清单、绑定manifest、runner、3项原日志及静态结果；本机临时保存不保证长期留存。

FE lib no-run（包括新增CI-only）、fmt、FE/CG strict Clippy和默认workspace all-targets通过。
三项ordinary、全部身份及红绿同源码经独立只读复核；所有其他未审/故障/计数/压力路径未运行。
架构快照最初追加使旧入口超200行，已改为独立主题页并纳入架构索引；保留失败日志，
最终docs通过（888篇）；尺寸相对b331d044通过（2417手写Rust、35旧超限，未改额度）。
source/callable仍Partial，无新End、receipt或旧oracle退休。
此头尚未纳入后续main `e659b66a`，合流后须重验；未推送、无官方CI结论。

### 正常回边与 LiveForward 的 main 合流验收（2026-11-11）

在独立工作树按真实 merge 父链保留 #412 `b331d044`、main `e659b66a`、
owner `87ee2050` 与借用 `75f4c2d9`，合流代码 `e70e5571`。唯一冲突为本 Spec 的两段
追加记录，均原样保留；frontend 公共导出自动合并，经独立逐文件比对没有丢失。
主工作树用户未提交 proposal 与未跟踪文件未动；旧 main/旧 CI 不替代此次组合验收。

owner 后端现有独立 header Return plan 已接实际 entry/header/body/false/exit CFG，
完整定义 census 拒绝额外 H 改写，正常回边传新激活 Q，旧 H 在 Replace 后 Drop，
false 出口按当前 H 授权真实 Return。此前 consumer 缺授权红已转绿；仅支持既有
单 String root、局部 Boolean、一次 Replace 后 Boolean 赋值的 bounded 形状，
不代表一般多轮、多根、任意条件或 Continue 已完成。旧固定 P/Q、Break 与 closed
合同不放宽。借用单 wrapper region 可 Complete，但 source/callable 仍 Partial，
无新 End、production receipt 或旧 oracle 退休。

合流头 fresh FE 4 / CG 10 共 14 项 ordinary exact 各 1 passed，包含真实 Basic/Const
两次 native 输出 outer/next（零/一轮）；不作为任意循环、泄漏计数或压力证明。
新增 CI-only 构造负控只编译，其他未审/故障/Abort/计数/压力路径不执行。
测试 artifact 为 opt0/debug2，FE 无额外 feature，CG 仅显式 loan support。
4279 个 tracked 输入、两测试 binary 与 runner SHA 均独立核对，执行前后保持一致；
原件在 /private/tmp/koven-two-line-integration-evidence，含 Cargo JSON、完整输入、精确
名单与逐项原日志。旧红证据不覆盖或重跑。初始证据脚本的中文 git 路径引号解析曾失败，
在任何构建前修复；不是项目编译失败或能力红。

fmt、FE/CG all-targets strict Clippy、默认 workspace all-targets、默认 CLI build/--help、
docs 与相对 e659b66a 的尺寸门禁均通过；默认 CLI artifact features=[]。
最终 Spec 仅追加阶段结果，不改冻结 Rust；D10/E2 仍在推进，尚未推送、无此组合正式 CI。

### 正常回边多次激活的后继探针合同（2026-11-11）

基于已本地验收的合流头 `0f9c395d` 独立分支，只增加原样源码探针：单 String
根、Int 局部计数器、`index < limit`、一次 Replace 后加一及循环后 Return。
调用输入 0/1/2/7 仅保留有限正常源码，不作归纳或资源计数证明。ADR-0038 §5
已规定有限 header 合同与跨激活实例边界；当前 Boolean 一轮准入保持原样。
先核真实 Basic ownership、SSA/verifier/LLVM，最后独立要求实际 header Return
receipt；缺失应保持能力红，不补造产品、不以正常 lowering 代替消费授权。
本片不执行 native、压力、fault 或计数路径，不扩 Continue、多根或 capture。
若原样分析先拒绝，记录实际阻塞而不修改源码掩盖。后续实现必须单独收敛
不含任意次数展开的有限闭合规则；此记录不宣称实现或验收完成。

#### 计数正常回边原样缺授权红

基线 `0f9c395d` 加上述未改生产探针，经独立源码/helper与实际artifact复核，
唯一 ordinary exact `counter_backedge_requires_actual_header_return_authority`
实际 0 passed、1 failed、2966 filtered、exit101，在末端缺 header Return receipt
断言失败。真实 Basic ownership、SSA lowering、verifier、LLVM 均先通过，说明
下一缺口是当前 header 消费授权，不是该原样程序的语言拒绝或正常 lowering。
输入4279文件与二进制运行前后保持一致；原件在
`/private/tmp/koven-owner-counter-backedge-evidence`，包含源码清单、Cargo JSON、
manifest、冻结binary和原日志。本机临时证据不保证长期保留。
编译 opt0/debug2，仅 loan-observation-test-support，1m24s。首命令误写feature名称
导致Cargo参数错误，保留失败日志；不将此错误算能力红。fmt、docs与diff检查通过；
未运行native、CI-only、资源计数、Clippy或全套CI。红测试尚未修复，不可发布合main。
后续先独立描述Int控制的类型/身份与单步赋值完成事实，复用既有H入边/回边合同；
核真实运算的正常完成和提前终止边，不靠0/1/2/7展开替代归纳，再接同一真实Return消费。

#### 受限 Int header 后继生产验收范围

本后继生产片复用原样红，不改变其断言。独立准入严格 Int 0 起点、raw
`index < limit`（唯一不可变 Int 参数）及 raw `index = index + 1`；其他步长、
比较、起点、Group、额外 Continue 保持不认证，不使 Int 落入旧 Boolean 协议。
FE 保存真实 increment 身份及 assignment/body 完成事实，仍使用原有限 H 守恒。
代码生成保留 checked-add 的真实 success/Abort 两边；实际生产 hook 保存指令位置、
操作数及 block/slot 身份，Return 终态重新核唯一定义、success 回边与无 H 改写。
Abort 不是回边且不插入 unwind；不因数学范围证明移除实际溢出守卫。

此后继验收明确增加 Basic/Const 下 0/1/2/7 的有限普通 native 输出对照，以及
实际 SSA H 入边/回边/false 出口与唯一 Replace/Drop、Abort 独立终结的结构检查。
这些正常有限运行不构成任意次数或泄漏证明；所有 fault、Abort 执行、计数与压力
仍只在 CI。原探针阶段“不执行 native”的历史记录不变。

#### 受限 Int header 生产结果

原样红提交 `204fd86c` 未改源码或断言。FE 准入后曾暴露后端仍假定 body 直接
回 header 的 InvalidModel；保留失败日志并通过真实 checked-add hook 绑定成功与
Abort 两边修复，没有删溢出守卫。新 SSA 测试首跑曾错误要求 Q 跨 success block
保持同一 ValueId；修为完整 Q→success 参数→header 入参链后通过，未删除其他断言。
严格 Clippy 曾报测试中 unit variant 的冗余结构写法，修正后通过，原日志保留。

最终冻结 Rust 的 FE 4 / CG 5 共9项 ordinary exact 全部通过（各1 passed）；含
原缺receipt红、真实CFG、Basic/Const两次native输出 outer/next/next/next、旧Boolean
及8个合法邻例的拒认证。实际 checked-add 的 Abort 分支保留但未运行。
新增 increment 与 assignment 身份混淆负控、旧破损/fault 等 CI-only 仅编译。

最终 FE/CG lib no-run、fmt、FE/CG all-targets strict Clippy（CG仅显式loan support）、
默认workspace all-targets、docs、相对 `0f9c395d` 尺寸及diff检查通过。
opt0/debug2未调整；4281输入、实际binary、runner与名单运行前后哈希一致，
独立源码/生产/测试审查及执行前身份核验通过。原件与失败日志保留在
`/private/tmp/koven-owner-counter-backedge-evidence`，最终 `sealed-*` 清单对应本轮。
仅后续追加验收文案，不改已测Rust；本机临时证据不保证长期保存。

这是单根、单计数器、唯一参数、固定比较/增量的有限合同，不等于一般循环、
Continue、多根、capture、完整D10/E2、资源计数或旧oracle退休。未推送，无此头官方CI；
与其他后继分片合流仍须针对真实组合头重验，不以本分支结果替代。
### LiveForward 真实生产 receipt 后继切片（2026-11-11）

从已验集成0f9c395d独立分支开始，main/origin均e659b66a；保留集成树及主树。
缺口是后端尚未消费Complete forward_regions，现有BorrowCall/BorrowReturn仅证明旧路有效。
新片仅覆盖原单表达式Shared String wrapper，不扩源码形状、End、权限恢复、清理责任、
source/callable覆盖或旧oracle退休。准入须同catalog原Begin/Handoff身份与完整region，
绑定实际source参数、callable及call；禁止Span搜索或完成SSA扫描回填receipt。
生产目标由原BorrowCall producer印记随指令进入真实append结果，BorrowReturn须在真实
set_terminator后绑定block及返回LoanId；不把terminator伪装instruction，不跨函数借LoanId。
当前仅test-feature空观察接缝和ordinary exact original_live_forward_requires_production_receipt_with_true_off，
尚未接生产：先Rooted/Memory四次真实on/off检查原产品、SSA/verifier/LLVM及旧route，再期待
on每份一张receipt、off为零；新测试红不得宣称交付绿，不合入已验集成头。后续生产另做独审。

红输入563a1f58已独审源码、4281个tracked输入及唯一Cargo artifact；opt0/debug2、
loan-observation-test-support、LLVM21.1.8，no-run84.4秒成功，binary SHA
64c9e3876c323d5245e05051855991188fa095a379495e9a191148950a6ffeb0。
唯一ordinary exact实际完成两布局on/off四次原SSA/verifier/LLVM1389bytes及相等性，再于
rooted/on receipt数量0≠1失败（exit101；0pass/1fail/2966filtered）；输入及binary前后不变。
原始日志与绑定清单在仓库外koven-live-forward-receipt-evidence/red；此次没有生产绿，
没有运行native、CI-only或全量。fmt/docs890/diff与尺寸2421files、35历史超限通过；
未跑clippy/workspace，生产接线前还需显式绑定wrap/view身份、实际append与set_terminator，
再独审并跑同一正常输入绿及CI-only负控编译。该红checkpoint不能合流绿色交付头。


#### LiveForward production receipt 正常输入验收

本地候选4c32c821从同一红源码继续：同catalog Complete/唯一原source与typed dispatch准入，
实际borrowed参数槽→原BorrowCall producer tag→真实append instruction/result→真实
set_terminator后出口block/LoanId验证；finish核唯一call/return。无新Begin/End/Drop，
无区域或off无计划；准入后的缺失/重复/身份破损报错。source/callable仍Partial，不认领
旧oracle退休、跨函数义务闭合或更广wrapper形状。独立代码及绑定artifact审查通过。

4284个tracked输入SHA e6242b465c44850b1d97eb86bb2e54ba08afa6dd7794d8caa10903b0935d3985；
默认opt0/debug2、LLVM21.1.8、loan-observation-test-support，no-run49.2秒成功，
binary SHA767bdecc21c3b43f2b2b53f2fba7ef09768adf70558434a180bddba17ed72b43。
新ordinary exact真实通过：两布局四次on/off原SSA/verifier/LLVM1389bytes保持一致，
on恰一张实际receipt、off零张，进一步绑定原caller→wrap及view、entry/call/return身份。
原forward_exit_coverage、两项forward_projection、P4 consumer与receiver普通exact回归
各1通过，共6项普通测试通过；合计12次SSA/LLVM lower，两个形状邻控仅frontend。
测试前后输入及binary SHA不变，逐项列表恰1；原始日志与清单在仓库外
koven-live-forward-receipt-evidence/green。native、全量及CI-only均未本机执行。
新CI-only实产品负控已编译，覆盖跨程序身份、漏/重复call/return、错source/result/callee/
block及额外指令；尚未直接覆盖admission的foreign-catalog/Partial注入，不能称完整故障覆盖。

最终门禁：codegen全targets带feature Clippy -D warnings通过34.52秒；fmt、docs890、
Rust尺寸2424文件/35历史超限、workspace all-targets check及diff检查通过。没有降低
profile、豁免或删除测试；独审核六项日志/计数/边界一致。仅本地提交，尚未推送/合流或跑CI。

### Int 正常回边与 LiveForward receipt 合流验收（2026-11-11）

独立合流代码 `9892ccc4` 保留 owner `d68ac8fa`、借用 `a8dc7995` 双父，
共同基线 `0f9c395d` 与当前 main `e659b66a` 均在父链中。仅本 Spec 双追加冲突，
两段完整保留；其余逐文件与父头字节一致，经独审确认。原工作树和用户修改未动。

合流后重新编译并执行 FE4 / CG11 共15项普通 exact，各1 passed；owner含 Basic/Const
真实有限0/1/2/7轮 native 正常输出，Int 是源码循环控制，不是资源计数实验；Abort仅核
SSA结构，没有执行Abort、故障、计数或压力路径。借用六项覆盖新真实 production receipt、
原覆盖与投影及旧consumer/receiver；不将局部region Complete扩大为source/callable完成。
CI-only只编译。不能据此声称一般循环、跨函数义务、完整故障覆盖或旧oracle退休已完成。

4286个tracked输入与两fresh binary、runner SHA均独核，执行前后未变；artifact为
opt0/debug2，FE features=[]，CG仅loan support。完整证据在仓库外
/private/tmp/koven-counter-receipt-integration-evidence，保留Cargo JSON、清单及15项原日志。
fmt、FE/CG all-targets strict Clippy、默认workspace all-targets、默认CLI build与真实
--help均通过，默认CLI artifact features=[]；docs及相对e659b66a尺寸门禁通过。
末次仅追加本Spec，不改已冻结Rust。复用既有target，无删除数据或缓存；本轮未推送、
未获得新组合正式CI结果，后续发布等待用户批准。D10/E2不标记完成。

### 显式 Continue header 的后继合同（2026-11-11）

基于已验收 `9164af82` 独立分支，复用单String根和受限Int控制；只增加body末
真实Continue。ADR-0038 §1/5要求Continue身份、目标、清理及回边独立闭合，
不能将其伪装成Fallthrough，也不能仅增加一个语法放行。无条件外形扩样、
新ABI、多根、capture或任意Continue路径；共同Return仍消费当前H。
先原样源码经Basic ownership、SSA/verifier/LLVM，最后检查真实header Return
授权；未改旧测试、不注入facts。探针阶段不执行native、Abort、fault、计数或压力；
若前置拒绝，记录实际阶段不改源码掩盖。生产需真实jump发生点及同轮源/目标
绑定，不能按最终相似Goto搜索补造边角色。

后继生产边界经只读独审：共享私有有限H证明核心，Normal与Continue分别封印公开产品。
Continue不得公开Deref或getter退化为Normal，旧Normal仍强制Fallthrough且无jump。
在原finish_owner_loop_jump的真实loan/owner/iteration清理后，记录同body catalog
jump身份与快照；保留原Ok(false)、expected Continue及witness身份，body完成再次
核同快照与恰一次完成。当前jump_boundary对normal会落empty_boundary，需独立分派。
后端须在原loop_jump清理后绑定实际source/header target，terminate成功后封印，
终态核完整边角色；不能通过扫描相似Goto补造。Break、nested/seq/map不进入此片。
原“无NormalBackedge”邻例不改为正常正例；另加Continue产品正例和角色/目标/清理/
重复完成的CI-only负控。此段为实施合同，不声称上述生产已实现。

#### Continue 原样缺授权红实证

基线9164af82上唯一定向探针经独立源码及artifact核验后，实际0 passed、1 failed、
2971 filtered、exit101；原样Basic ownership、SSA lowering、verifier、LLVM先通过，
最后在缺真实Continue header Return授权断言失败。未改旧测试、未改生产门禁。
no-run为opt0/debug2、仅loan support，6m12s；4287输入与保存binary运行前后一致，
原件在 `/private/tmp/koven-owner-continue-header-evidence`，含manifest、输入清单、
冻结binary及原日志。本机临时证据不保证长期保存。fmt、docs和diff检查通过。
没有运行native、fault、Abort、计数、压力、Clippy或正式CI；未推送。

本次编译后Mac可用磁盘约3GiB，停止追加大构建，不清理缓存或删除文件。先封存
真实红及上述独审实施合同；生产尚未接线，红分支不可并入待发布集成头。
后续需恢复足够构建余量后，实现独立Continue封印和真实jump消费，再做红绿回归。

#### Continue 后继生产与普通验收范围

磁盘余量经用户批准清理指定增量缓存后恢复，继续生产。复用已有Boolean赋false
和Int零起点/小于唯一参数/加一两种控制，均仅新增body末精确Continue；Boolean
另验真实无increment的lowering/receipt，不以Int路径替代。私有HeaderCore只承载
共同H守恒，两公开wrapper分别强制Fallthrough或Continue(jump)，无Deref或转换。
FE在真实jump清理后记录独立catalog快照，经Ok(false)、expected role和body完成
再核一致才封印；CG在真实loop_jump清理后及terminate成功后绑定source/header，
终态核jump身份、唯一receipt、清理消耗和真实边，不从最终Goto反推。

后继普通验收纳入Int双Unit有限0/1/2/7 native、真实Continue terminator origin及
SSA H链、Boolean无increment真实receipt，并保留旧Normal/Break/Return回归。
这些普通运行不等于任意循环、泄漏或计数证明；所有fault/foreign/重复receipt/
错target/缺清理/Abort执行仍CI-only，仅编译不本机运行。原探针阶段不执行native
的历史记录保留，不改红源码或末端断言。

#### Continue 恢复后验收（2026-11-11）

用户手动清理期间旧临时 Continue、counter 与集成 raw 目录丢失；当时被中断的
FE 编译退出101但日志已不可读，不归因为代码失败，也不算新增红。原始探针
`f82a7fc2` 的源码与历史结果仍保留；本段不宣称复核旧 raw。恢复后复用该头及
保全的生产修改，新建持久 `Koven/.local-artifacts/owner-continue-recovery-20261111T060154Z`
保存新输入清单、真实 Cargo artifacts、实际 binary SHA、runner 与逐项原日志；
保留至此切片进入 main 且对应远端证据可用，不清理既有文件。

| 验收 | 本轮实际结果 |
|---|---|
| FE lib no-run，默认 feature / opt0 / debug2 | 通过，1m08s；CI-only 仅编译 |
| FE normal_backedge 的五项普通 exact | 5/5，每项1 passed；含正常/计数回边边界及独立Continue |
| CG lib no-run，仅 loan-observation-test-support / opt0 / debug2 | 通过，10m25s；CI-only 仅编译 |
| CG Continue 四项普通 exact | 4/4；原探针末端授权断言转绿，真实SSA Continue来源与H链、Boolean无increment、Basic/Const有限0/1/2/7 native均通过 |
| CG旧Normal Boolean/Int、Break共同Return、body Return两端点、suffix Replace Return | 五项exact均1 passed，非全量回归 |
| FE/CG all-targets Clippy `-D warnings` | 通过；含loan support |
| 默认 workspace all-targets check | 通过，不是全量测试执行 |
| fmt / docs / Rust尺寸 / diff | 通过；docs891，2426手写Rust，35项旧超限未掩盖 |

独审核4287 tracked输入及实际FE/CG binary SHA一致、14项exact原日志均通过；
源码复审核独立封印、真实jump消费与重复receipt拒绝，无剩余阻断。有限native
只观察输出 `outer/next/next/next`，不当任意轮次或泄漏证明。正式CI、一般多分支
Continue/Break合流、多根、嵌套循环及owner与loan/capture联合合同仍未覆盖。
### 同来源条件借用出口前置诊断（2026-11-11）

从已验9164af82独立树开始，main/origin为e659b66a。下一有价值缺口选择真实条件分支
两个合法同source borrowed returns，而非再添加同构单表达式wrapper。ADR0040允许合法
同来源不同控制路径交付，但仍要求逐路径完整证明；当前FE只保留两条原handoff，不能
将两个互斥路径拼成单线性Complete。第一步只验证既有合法源码的实际lowering前置，
不新增区域/receipt，不放宽单wrapper准入，不改变旧投影或任何End/Drop责任。
ordinary_same_source_branch_requires_real_lowering_with_true_off先核两个真实return和
同source不同call身份、Partial/off旧产品，完成两布局四次真实lower尝试后才验成功结果。
若lower失败须作为独立后端前置缺口记录，不冒充生命周期coverage红或强制route。
本片暂未编译/执行。禁止本机CI-only/native/压力测试；磁盘仅约6GiB，复用专用target。

前置红输入2a409753：no-run723.47秒成功，4287输入绑定独审通过；恢复空间后唯一
ordinary exact实跑，两布局on/off四次ownership均成功、lower均UnsupportedNode且routes空，
首borrow marker102..108（exit101、0pass/1fail/2971filtered）。未到SSA verifier/LLVM，
不能算coverage/receipt缺能力红。日志中的ownership_complete仅指ownership phase成功，
源码scope仍明确Partial；后续将日志改名ownership_phase_succeeded以免混淆。

后继只在Unit验证中为精确Block[If(flag){Return Call(source)},Return Call(source)]
准入；flag与source均为原Borrow参数（Boolean/String），同callee、独立原返回facts和
同root source-qualified IDs逐项校验。其余Block继续旧门，不修改Single declaration gate；
复用已有refine CFG/call/return生产，不新增Complete/receipt/End/Drop或旧oracle退休。

### 同来源动态双出口 lowering 验收（2026-11-11）

后继 `2c7613d2` 已突破原声明门禁，但真实普通入口四组合均在 final verifier 报
InvalidModel。`c1bb61c9` 只读捕获原 verifier（每次恰一次、不替换结果、不重跑），
确认仅 wrap 的两个返回 block 各报 ReturnType0；来源 Span 为142..161、165..184。
根因是返回 parent 链遇到显式 CFG 运输后的 source block 参数，旧检查只认原入口 LoanId。
`80bfacc0` 保留原 alias-root、活跃性、后代与 parent 链检查，仅允许运输身份集合严格
为唯一原入口能力；不使用通用 equivalent 的交集判定、不修改 RangeReturn，不新增 End。
同 root 的 entry/derived 混合身份不能冒充入口。最终代码 `a8ccd392` 只再修正负控断言风格。

| 验收项 | 普通 exact（均为 lang-codegen lib） | 实际结果 |
|---|---|---|
| 动态双出口 | branch_forward_probe::ordinary_same_source_branch_requires_real_lowering_with_true_off | 1 passed；Rooted/Memory × on/off 四次生产lower、verifier、LLVM一致 |
| 三个邻界仍旧门 | branch_forward_probe::ordinary_branch_neighbors_keep_the_unit_declaration_gate | 1 passed；两布局共六次精确拒绝 |
| Single旧约束 | borrow_result_contract_tests::borrowed_result_caller_cfg_and_block_returns_stay_precisely_unsupported | 1 passed |
| 原单表达式receipt | forward_receipts::original_live_forward_requires_production_receipt_with_true_off | 1 passed；四次SSA/LLVM |
| 分支原投影 | forward_projection::original_branch_returns_keep_both_raw_handoffs_without_a_cause | 1 passed；不把FE投影算SSA绿 |

除Single测试前缀为 `ssa::` 外，表中其余前缀为 `ssa::loan_region_ssa_tests::`。
新测试核原flag Read、显式edge参数运输、两条不同BorrowCall/Return，wrapper内无额外Begin/End/Drop；
Partial/off旧产品及分支receipt为空均保留。新增两个手写SSA CI-only运输正负控已编译、
未本机执行，负控要求目标返回位置ReturnType；不能宣称CI负控实跑或完整故障覆盖。

最终17.02秒no-run绑定4289输入，默认opt0/debug2，仅loan-observation-test-support。
五项exact均核list恰一项，执行前后输入及binary身份不变。当前证据保存在主仓忽略目录
`.local-artifacts/borrow-branch-recovery-20261111-1415/attempt-6`，含完整Cargo JSON、
输入/binary/runner清单及原日志；attempt-4保留真实verifier红。此前临时目录日志在用户
手动清理后已缺失，原2a409753红仅会话与本Spec留存，不能称已复核原日志；本轮为新建证据。
本片仍不新增分支region Complete/production receipt，不实现一般循环/多出口覆盖或旧oracle
退休；下一片须逐互斥出口证明原因与完整性，不能把两条handoff拼成单线性协议。未推送。

最终归属调整 `59e1888d` 将本片新增singleton helper放回borrow_result私有领域，入口
1858行，不提高原1860例外；语义守恒经独审。`attempt-7` 为最终代码新证据：66.41秒
no-run、同五项ordinary各1 passed、all-targets feature strict Clippy21.63秒通过；
fmt、docs891、尺寸2428文件/35历史超限及diff均通过。未运行native、全量或CI-only；
最终账本提交仅改本文，不改上述已冻结Rust。后续与owner合流需重新核组合，不以此替代CI。

### Continue header 与双 return 窄 lowering 合流验收（2026-11-11）

隔离合流代码 `438ab3ce` 保留 owner `913bd6b8` 与借用 `871cd939` 双父，
基线 `9164af82` 和最新 main `e659b66a` 在祖先链中。唯一 Spec 追加冲突两段均保留；
其他文件逐一与各父头核对相同，未改原工作树及主树用户修改。

新编译 FE5 / CG14 共19项普通 exact 均各1 passed。Continue native仅在Basic/Const
执行有限0/1/2/7正常样例；Abort只核SSA结构，不执行故障/Abort/计数/压力测试。
借用核双return真实最终verifier及两条独立loan运输，没有新增分支region Complete或
production receipt，不宣称一般多出口、混合Continue/Break或D10/E2完成。CI-only仅编译。
4290个tracked输入、fresh artifact、运行与保存binary和runner身份均独审吻合，执行前后
未变；opt0/debug2，FE无额外feature，CG仅显式loan support。

fmt、FE/CG all-targets strict Clippy、默认workspace all-targets、默认CLI build/--help、
docs与相对e659b66a尺寸门禁通过；默认CLI artifact features=[]。原始证据唯一主目录为
主仓忽略目录 `.local-artifacts/continue-branch-integration-evidence-20261111`，保存完整
Cargo JSON、输入清单、runner和19项日志，以及两份已核SHA的测试binary副本。复用借用
target前先保全其旧verified binary与映射，不碰owner在途target；未删除数据或缓存。
此次末提交仅补本文，未推送、未合main，也没有此组合正式CI结论。

### 第三批默认测试观察器功能门控修正（2026-11-11）

`891e0869` 的默认 workspace check 虽成功，但测试重导出
`observe_final_verifier` 留有 unused import 警告，不能作为默认严格 Clippy 通过。
本补片仅使两层重导出及观察器函数与唯一消费者的
`loan-observation-test-support` 功能门控一致，不改观察逻辑、生产 lowering 或语言语义。

- 独立工作树实际执行默认 `cargo clippy --locked --offline --workspace --all-targets -- -D warnings`：通过。
- 同树显式加入 `--features lang-codegen/loan-observation-test-support` 的 workspace 严格 Clippy：通过，消费者仍通过类型检查。
- fmt、docs、diff 检查通过；三处 cfg 变更独立只读审核无阻断。
- 本补片未重新执行行为测试或 CI-only 测试；此前19项的实测输入仍为 `438ab3ce`，不冒称新头重跑。
- 新头尚无正式 CI，未推送或合入 main。

#### 混合 Continue/Break 单根后继探针范围

基于已验 `913bd6b8` 独立分支，仅探测单Int参数、单String根、零起点/小于limit/
加一的已有控制；Replace与加一后唯一 `if(index < 2)` 分别 Continue/Break，
loop后原样Return。依据ADR-0038 §1/2/5，Continue只供应header回边，不参加exit
join；Break只供应exit，不参加header join。condition-false持当次H，Break持
当轮Replace后的Q，不能用固定P/Q、上次H或最终Goto倒造共同Return授权。
这是最窄两类真实出口与回边共存缺口，不扩多根、嵌套、Resource或capture。

普通探针保持源码与末端断言，经真实FE Basic、lower、SSA verifier、LLVM生成后
断言Return实际receipt。limit0/1/2/7只是源码入口，本阶段不执行native，不称
运行轨迹或参数化证明。任何前置失败都原样保留，不跳过诊断/verifier来制造
末端缺授权红；本阶段不修改生产。独审确认普通exact无fault/foreign/压力/
计数；CI-only不本机执行。取得真实红后再收敛独立sealed混合边合同及生产接线。

混合边探针真实结果：FE Basic、实际lower、SSA verifier与LLVM生成均通过，
唯一exact在第30行缺Return authority断言失败：0 passed/1 failed/2976 filtered，
退出101。没有放宽或删除任何前置检查。首次no-run 4m20s；fmt发现注册字母顺序
问题，保留失败日志后仅排序；重新no-run 9.09s并绑定4288输入及保存binary SHA，
同一exact仍在同一断言真实失败。两轮均opt0/debug2、仅loan support，不执行native。
新证据保存于 `Koven/.local-artifacts/owner-mixed-loop-edges-20261111T063124Z`，
原始与排序后input/binary/log分开保留，至切片合main且远端证据可用；fmt/docs891/
diff通过。独审确认非同构扩样、无生产改动；此提交是已定位缺口，不是完成修复。

后续应复用真实split/arm/jump hooks，却不全局放宽旧闭合value observation门禁。
混合产品须独立核live Q的split快照，两arm实际cleanup及Ok(false)/pending witness/
catalog分别封印；backend在真实If与jump生产点接线，禁止用最后SSA形状替代来源。

#### 混合边生产实施合同（尚待本片验收）

本片以独立Mixed产品和消费plan承接上述原样红，不把Mixed转换成已验Normal或
Continue wrapper。共同header H守恒可私有复用，两个真实arm身份、条件与jump
角色必须另行封闭：同一已消费previous的Q快照进入两arm；Continue cleanup后
回对应header，Break cleanup后到对应exit；两个arm均实际终止，不能假设其中
一臂永不可达。condition-false的H与Break的当前Q共同授权最终Return。

只接真实If/split/arm/jump生产点；旧闭合观察门禁保持原义，Mixed必须走专属
live-state校验。先构造实际RHS再Replace的顺序、detached old的真实Drop、checked
Add正常/Abort边、Return交付先于cleanup均保持。故障注入仅CI-only编译；本机
普通验收将原红、FE封印/邻接形状、SSA角色与输入关联、Basic/Const有限native及
共享hook旧回归分开核验。配置或接线不等同通过，实际结果在完成后补记。

#### 混合边本地生产验收（2026-11-11）

输入基于红探针 `c4349a62`；原SOURCE与原测试函数保持不变。新green证据保留在
前述持久目录的 `green/`：4290输入SHA、真实Cargo artifacts、FE/CG实际与保存
binary SHA、19项逐次exact原日志及结果清单。测试后仅追加此Spec账本。

| 验收 | 实际结果 |
|---|---|
| FE/CG lib no-run，opt0/debug2 | 均成功；FE默认、CG仅loan support；CI-only仅编译 |
| FE normal_backedge七项ordinary exact | 7/7，各1 passed；Mixed实际双角色封印/合法邻形边界及旧Normal/Continue/Int回归 |
| CG mixed三项ordinary exact | 3/3；原缺授权红转绿、SSA真实Continue/header及Break/exit输入关联、Basic/Const有限0/1/2/7 native |
| CG Continue四项与旧Normal/Return五项ordinary exact | 9/9，各1 passed；共享生产hook回归，不是全量测试 |
| FE/CG严格Clippy all-targets，默认与loan support分别检查 | 两次均通过，`-D warnings` |
| 默认workspace all-targets check | 通过，无warning，不代表全量测试执行 |
| fmt / docs / Rust尺寸 / diff | 通过；文档891，旧尺寸欠账保留报告 |

独立审查确认两个新mixed模块均在输入绑定内、真实split/arm/jump与终止证据完整，
旧closed validator不放宽，旧Normal/Continue CFG路径保留。普通native输出仍仅
`outer/next/next/next`，有限运行不证明泄漏/计数或所有激活；任意条件形状、多个
根、嵌套/顺序循环及loan/capture联合合同尚未覆盖，正式CI与后续合流待验证。
### 同来源动态双出口 Partial 因果证据（2026-11-11）

本片基于871cd939，仅给已准入的动态if真出口与唯一false尾return建立独立实际原因链；
不发布branch Complete/region/receipt，也不增加End/Drop。声明计划不等于实际到达；
真实臂入口须独立登记路径，false续接须确认true实际终止且只有唯一继续状态。Return
求值前绑定call/value、control、condition和臂身份；仅沿原pending token传递acquisition。
公开branch身份必须经过独立validator，不能只凭candidate/cause自称已验证。多继续、嵌套、
未准入形状、错call/control/catalog等降为无原因raw handoff，旧语义和原子撤销不变。

真实前置红df9fcf50：22.37秒no-run成功；4290输入与唯一FE binary冻结，普通exact
`ordinary_dynamic_branch_has_two_distinct_actual_acquisition_causes`实际0 passed/1 failed。
Rooted/Memory均ownership成功、两个raw handoff、source_begin/callable链接均零，Partial；
legacy/on-off等价先通过，失败精确为缺少实际acquisition原因。不是lowering、Complete或
receipt失败。证据及独立保存binary在主仓忽略目录
`.local-artifacts/borrow-branch-causes-20261111/red`，独核输入/产物/runner前后身份一致。

最终代码d3eb16aa的普通验收为FE两项与上述codegen五项，各1 passed。FE新增
`ordinary_unadmitted_exit_shapes_keep_raw_partial_projection`覆盖三个邻界；动态正例
逐项核真实AST的callable/split/condition/arm/control/value及两个不同原acquisition。
旧语义on/off等价、两布局Partial、branch region/receipt为空仍成立。独立入口登记与
全记录唯一性校验拒绝重复臂、重复入口或共享acquisition，不以首条候选获胜。
新增CI-only身份负控仅编译未本机执行，不宣称完整故障覆盖或CI已绿。

最终证据在同根green-3与codegen-3：绑定4292输入，输入摘要
`7ed55c6743d7cbea8ece68803b91cedfd0734df4111e88085476f24dd6ba7f76`；
FE no-run 0.079秒复用匹配产物，codegen no-run 10.56秒重新编译，各自保存独立binary、
Cargo JSON、runner和输入清单。七项exact均核list恰一项，执行前后身份不变。
默认strict Clippy 26.90秒、feature strict Clippy 12.41秒、workspace all-targets check
4.48秒均通过；fmt、docs891、尺寸与diff通过。99ed34db默认strict曾因observer未使用
reexport真实失败，保留原日志；d3eb16aa复用已审d5a5722f的三个Rust文件cfg修正，
不以allow掩盖，重新验证以上最终结果。本账本追加仅文档，不改已冻结Rust。
未执行native、全量或CI-only，未推送；合流后必须重新联合验证。

### Mixed owner 与双出口 Partial 因果的最新 main 合流验收（2026-11-11）

隔离代码 `4e79d688` 保留默认observer cfg修复 `d5a5722f`、owner `fc1fb9de`、
借用 `5e7439fa` 与最新main `95cecb86` 的真实父链；#412旧头9164af82仍为祖先。
只双保Spec追加冲突，生产文件自动合并；三个默认observer cfg与d5修复一致，未重复改写。
原工作树及主树用户修改未动，不修改或删除缓存与数据。

26项普通精确回归均各1 passed：owner FE7/CG12，借用FE集成2/CG5。包含已审
Basic/Const有限0/1/2/7正常native，Abort仅核SSA结构；CI-only仅编译，不执行故障、
资源计数、压力或全量。双出口各有真实Partial原因不等于branch Complete/region/receipt；
Mixed有限形状不代表任意条件、多根、嵌套/顺序循环或loan/capture联合合同完成。

唯一持久证据主目录为主仓忽略目录 `.local-artifacts/mixed-causes-integration-evidence-20261111`。
attempt-1保留runner遗漏FE集成support导致exact列表为空的原始记录，未运行该测试，
不是能力红；attempt-2修正feature后26项与全部门禁通过于旧main组合292d6400。
其后main推进#414，故纳入95cecb86并在attempt-3重新运行全部26项与全部门禁，
不能用attempt-2或旧CI代表新组合。4295个tracked输入与实际Cargo artifact、三份保存
binary、runner及精确名单/日志绑定；opt0/debug2，FE lib默认，FE集成及CG显式loan support。
复用匹配产物与重新编译情况以原Cargo JSON为准，执行前后输入及binary保持一致。

默认及loan support两套FE/CG all-targets strict Clippy、默认workspace all-targets、
默认CLI build/真实--help、fmt/docs/相对95cecb86尺寸门禁均通过，CLI features=[]。
尺寸2432手写Rust、35历史超限，未改额度。最终只追加本Spec；仍未推送、未合main，
无本组合正式CI结论，也不将D10/E2标记完成。

#### 参数驱动混合边后继探针

基于已发布集成 `d0f09db0` 新独立分支。相邻真实缺口是固定 `index < 2` 无法由
调用方选择Continue/Break阈值；最窄探针采用两个不同的不可变Int参数limit/stop，
保持单String根、index零起点、while(index < limit)、加一及唯一尾If，split仅为
`index < stop`。不扩普通Normal/Continue控制准入，不接受阈值算式、别名、额外
赋值、捕获、多根或嵌套。现行Int/比较/If语义不变，只补ADR-0038的路径运输观察。

原有有限H、真实split/arm/jump清理和Return证明必须复用，Continue仍只入header，
Break与false H共同到exit。不得以参数个数放宽所有Boolean或普通循环族；阈值
身份必须是当前callable另一Int参数而不是同名文本或外部symbol。参数任意取值
不改index<limit限定的checked Add安全边界，Abort仍保留且不参加正常join。

普通红探针只执行真实FE Basic/lower/SSA verifier/LLVM生成后断言Return receipt，
入口(0,2)/(1,2)/(7,2)/(2,7)仅是原样源码，不执行native。先核全部前置，再记录
最终缺授权红；前置失败不得跳过或改断言制造红。故障/foreign/压力仍CI-only。

参数mixed原样红已实际确认：no-run opt0/debug2、仅loan support成功（14m04s），
4296输入与保存binary SHA绑定。唯一exact依次通过FE Basic、lower、SSA verifier、
LLVM，最终第30行缺Return authority断言失败：0 passed/1 failed/2984 filtered，
exit101，无ignored，不执行native或CI-only。fmt/docs/diff通过；未改生产。
新原始证据保留 `Koven/.local-artifacts/owner-parameter-mixed-20261111T080027Z`，
至切片进入main且相应远端证据可用。此红是已定位缺口，不作为完成代码合流。

#### 参数mixed生产实施合同（待实际验收）

复用唯一MixedBackedge与既有split/arm/jump/Return协议，不新增平行loop wrapper。
仅当前函数恰好两个无mode不可变Int参数可进入新Mixed专用准入；失败不回退到
旧Boolean、owner-parameter、Normal或Continue。旧literal2混合边仍只接受单Int。
私有阈值证据绑定当前callable、精确参数集合、while RHS limit和split RHS stop的
source-qualified表达式及symbol，要求互异、同源、集合完整且不同于index。
生产端只读真实Name解析结果，不能按文本名字辨认参数或把任意scalar表达式当阈值。

CG既有Mixed真实receipt路径应直接消费同轮封闭产品；除非出现真实缺口，不新增
重复consumer。新增普通验收区分参数身份/合法邻形、真实SSA参数至比较及split运输、
Basic/Const有限边界native；损坏私有证明与foreign/fault仍CI-only，仅编译。

参数mixed首轮生产验证保留一次测试oracle失败：FE9全部通过，CG15中14通过
（含原红转绿及六组Basic/Const native），新SSA来源测试在roots helper拒绝参数
读取链，不能记为24通过。源码与独审核实无mode Int参数由Shared entry Loan进入，
Name产生ReadLoan→SSA Read(Loan)；原helper仅识别block参数转发，遗漏合法读取。
修正仅接受唯一def、单结果的Read(Loan)，递归到各自不同Shared Loan<Int>入口，
仍拒Place、算术、调用、Copy/Alias等合成值，不改生产或原红断言。原失败日志保留。
为避免继续检验过时输入，旧本任务Clippy被明确中止（退出255，不算通过），其后
默认Clippy/workspace未执行；修改后将重新冻结输入、编译、运行并完成门禁。

#### 参数mixed最终本地验收（green2）

严格oracle修正后重新冻结4296输入，重新no-run并执行同一白名单24项；原红
SOURCE/authority断言与所有生产代码不因该oracle修正改变。新证据单独保存于
前述持久目录 `green2/`，不覆盖 `green/` 的23/24与取消记录；实际和保存binary
SHA、输入、逐项日志及results清单经独立核验一致。

| 验收 | 最终实际结果 |
|---|---|
| FE9 ordinary exact | 9/9，各1 passed；参数身份/合法邻形以及旧Mixed/Normal/Continue回归 |
| CG15 ordinary exact | 15/15，各1 passed；原参数mixed红转绿、严格Shared Loan<Int>读取来源、Basic/Const六组有限native及旧12项回归 |
| FE/CG no-run | 成功，opt0/debug2；FE默认、CG仅loan support；CI-only仅编译 |
| FE/CG all-targets严格Clippy | loan support与默认分别通过，`-D warnings`；取消旧轮不计入 |
| 默认workspace all-targets check | 通过，无warning；不是全量测试执行 |
| fmt/docs/尺寸/diff | 通过；文档893，2433手写Rust，35项旧超限报告保留 |

六组native为(0,2)/(1,2)/(7,2)/(2,7)/(2,0)/(2,-1)，输出outer后五次next，
只证明这些正常路径，非溢出执行、泄漏计数或所有激活证明。CG生产零改动，复用
已有真实Mixed receipts；本片仍不覆盖阈值算式/别名、多根、嵌套或联合capture。
正式CI与后续最新main合流验证仍待完成，本地验收不代表整体D10/E2完成。
### 同来源动态双出口独立路径覆盖合同（2026-11-11）

本片从d0f09db0起步，仅覆盖已准入Shared String来源的动态Boolean参数Name条件、
无else的if真臂直接return同一callee及唯一false尾return。source/callable仍Partial；
两个Partial原因不能拼成线性Complete，不能扩大旧表达式region或其receipt准入，
不新增End、Drop、权限恢复、跨函数LoanId或后端执行责任。

独立branch产品按callable/source parameter/split/condition声明有限区域；只有整个
声明范围通过独立认证才发布两个认证path，每path绑定原branch身份及本路径全部动作、
实际Begin和live handoff。Partial不得发布有效认证句柄；不公开跨两臂的单一Range，
路径枚举顺序不代表执行时序。旧Partial handoff产品及其getter保持原合同。

认证必须独立对账声明预期两出口、实际split/臂/唯一false续接登记及真实Return生命周期。
实际true终止后才能登记false唯一继续状态；缺失、重复或额外继续状态不能获Complete。
每条路径从实际臂入口至Return完成保留全部record，不按角色筛选；区域入口和条件求值
前后也须证明没有遗漏动作，不能仅凭Boolean Name语法或已有两个handoff推断覆盖。
Return必须在value求值成功后、finish_return_iterations/drop_all之前核未清空义务，
实际handoff及原清理完成后再核post；不能只在outer已清空状态看空表。旧表达式boundary
不放宽；分支单独核真实scope层级与非owning参数，不能剥掉字段调用旧检查来假装闭合。
遗漏pre/post、额外Begin/End/Drop、跨臂区间重叠、删选记录、完成后追加、错身份/跨catalog
或复用acquisition均不认证。错误/deferred沿用原全局撤销边界。

前置红版本只落只读API壳：catalog.branch_forward_regions()初始化为空，生产者和认证器均未
实现，无硬编码证书。ordinary探针复用原源码和原AST/真实cause正控，再完成Rooted/Memory
四次on/off ownership、旧产品等价、两条不同Begin的真实原因、source Partial及旧表达式
regions为空，最后统一期待每布局一个新局部Complete区域。空壳预期是运行时0!=1，
不是缺API编译失败。

真实红78a616d7：FE no-run 270.20秒成功，4296个tracked输入与保存binary绑定；唯一
ordinary exact实际0 passed/1 failed。Rooted/Memory均两模式ownership成功、实际causes=2、
source Partial且branch_regions=0，最后精准0!=1；输入和binary执行前后一致。
持久证据保存于主仓忽略目录的本片red目录，不使用编译失败或旧结果替代能力红。
随后生产接入独立入口、condition、实际臂、Return清理前后、唯一续接、Block seal及
callable close见证；以进入基准加相对scope/branch深度检查状态，不放宽旧边界。
最终生产d5fa31ff已完成本地普通验收；两个path分别核原身份、完整动作区间、Begin/
live handoff及无交叠，两布局仍source Partial，旧表达式region/receipt输入保持为空。

| 验收项 | 测试目标/过滤器 | 实际结果 |
| --- | --- | --- |
| 普通源码独立路径覆盖能力红及后继实现 | unit_branch_forward_causes / ordinary_dynamic_branch_requires_independent_path_coverage | 78a616d7真实0 passed/1 failed；d5fa31ff真实1 passed |
| 漏出口/重复臂/额外continuation、动作区间和状态破损 | branch_forward_regions/tests.rs 的 CI-only 正基线及24类破损 | 默认/feature严格Clippy及workspace编译通过；未本机运行 |
| 原因链与三邻界 | unit_branch_forward_causes 其余两个ordinary exact | 各1 passed |
| 旧表达式region/receipt与双出口lowering不回退 | 上述codegen五项ordinary exact | 各1 passed；不等于新分支receipt |


最终FE no-run 299.39秒、CG no-run 1084.25秒，默认opt0/debug2，仅启用
loan-observation-test-support。绑定4299 tracked输入，摘要
`556075e9140001ef7afa6266022b2a7eecebd208a0009634df010e6ad7e5646c`；
八项ordinary各核exact唯一并实跑1 passed，前后输入及binary身份一致。主仓忽略目录
`.local-artifacts/borrow-branch-coverage-20261111/{red,green,codegen}`保留原Cargo JSON、
输入/runner清单、实际日志及独立保存binary；额外回归runner摘要另存补充绑定。
默认strict Clippy302.03秒、feature strict296.93秒、workspace all-targets61.20秒均过。
最终fmt44.93秒、docs893/16.84秒、尺寸2436文件/35历史超限665.61秒、diff通过，
没有提高尺寸例外。Architecture曾207行超过入口上限，已移除重复合同并更新原事实入口，
恢复200行后真实重验；生产、负控交叉及完整证据均独审通过。最后账本提交仅改本文。

本片不覆盖一般分支/循环/Abort、非该声明形状或分支后端receipt，不改变整source/callable
Partial。24类CI-only只编译未运行；真实error/deferred全局原子撤销、逐字段状态污染与
native尚未本片实测，不宣称完整故障覆盖或正式CI绿。未推送，后续合流须独立联合验收。

### 参数化 Mixed owner 与分支局部覆盖合流验收（2026-11-11）

隔离合流代码 `af3ae95d` 保留 owner `f4a3376e` 与借用 `318b8631` 双父，
基线/#412 `d0f09db0` 及最新 main `95cecb86` 均在祖先链中；验收末再次核main未推进。
唯一Spec双追加冲突两段保留，其余生产自动合并；原树及主树用户修改未动，未删除数据。

32项普通exact均逐项名单唯一、实际1 passed：owner FE9/CG15，借用FE集成3/CG5。
参数化Mixed的Basic/Const native按已审有限输入执行，含不同limit/threshold以及零与负
threshold正常样例；不把有限运行当作一般循环或资源计数证明。Abort仅作SSA结构核验，
CI-only仅编译，无故障/压力/全量执行。分支仅在窄形状内双路径局部Complete，整source/
callable仍Partial，无新分支后端receipt，其他一般分支/循环或联合合同不宣称完成。

完整新证据唯一保存于主仓忽略目录
`.local-artifacts/parameter-coverage-integration-evidence-20261111`。4300个tracked输入
与三份实际Cargo artifact、保存binary、runner、名单和原始日志绑定，执行前后输入及
binary不变；opt0/debug2，FE lib默认，FE集成与CG显式loan support。旧借用binary副本
先保全并核SHA，复用已释放target；不以父片结果或旧CI替代此次真实组合验证。

默认及loan support两套FE/CG all-targets strict Clippy、默认workspace all-targets、
默认CLI build与真实--help、fmt、docs893、相对95cecb86尺寸均通过；CLI features=[]。
尺寸2437手写Rust、35历史超限，未提高额度。此末提交仅追加Spec，不改冻结Rust；
未推送、未合main，无本组合正式CI结论，D10/E2继续保持进行中。

### 单根条件 Break 与隐式正常回边：待验证切片（2026-11-11）

本片从已验收组合 `fa82cfc2` 隔离推进 ADR-0038 的边角色与有限 header 合同。
保持双不可变 Int 参数、单 String root、零初始化计数、严格小于条件、Replace 与
加一；仅考察尾部 `if(index < stop) { break }` 无 else 的合法形状及循环后 Return。
实际 Break 指向 exit；未进入 Break 的分支须经过真实 if/body 正常完成回到 header，
不能伪造 Continue 身份或复用旧 Mixed 两臂终止的 Ok(false) 作为证明。

先保留独立普通探针：FE Basic、lowering、SSA verifier、LLVM 及唯一 Return 收据
逐步通过后，才以实际 payload authority 断言定位缺口；任何前置失败单独记录。
当前尚未执行，不宣称确认红或新增完成能力。生产拟复用私有 H 守恒与参数身份，
新增真实 Break 和正常完成的独立封印；旧 Normal/Continue/Mixed 门禁不放宽。
不覆盖任意条件、多个根、嵌套、capture 或一般后缀；故障及篡改负控仅编译。
本批不推送，后续普通执行、输入绑定和独审结果追加到本节。

本片普通红已真实确认：唯一exact `conditional_break_requires_actual_exit_return_authority`
在最后authority断言失败（exit101，0 passed/1 failed/0 ignored）；前置Basic/真实
lower/SSA verifier/LLVM/唯一Return均已通过，不是编译或前置拒绝。4301输入与
Cargo实际artifact、保存binary SHA及原日志保留于主仓忽略目录
`.local-artifacts/owner-conditional-break-20261111T100154Z`；测试前后输入/binary不变。
本红只确认缺少当前consumer授权，尚未实施生产，也未运行native或CI-only。
格式门禁首次发现新模块注册排序错误，旧失败日志保留；仅修正注册顺序，原探针源码
及断言不改。重新冻结4301输入并编译、重跑同exact，仍在相同authority断言红；
第二次fmt、docs893、diff均通过。最终红证据在同目录red2，非复用旧binary结果。

生产现已接通独立 ConditionalBreak 产品：复用私有 H 核心，真实 split、then入口、
Break清理、then结束、implicit入口/清理、If正常继续、body正常完成共8快照，
后端独立收据与完整 incoming 核验后才授权最终 Return。旧 Mixed/Continue/closed
门禁不放宽，原红 SOURCE/断言不修改。

首轮生产FE为10/11，新正例未形成产品；诊断发现共享表达式门只准有else的If，
新无else在split之前被拒。补门严格绑定当前已准入产品control、Unit、事务身份与
真实body/赋值/未完成状态，临时日志移除。green2真实FE11/11、CG17/18；Mac
断线恢复后核原持久结果，不盲目重跑。唯一结构测试误要求最终SSA保留已被发射器
合法折叠的单前驱join；仅修oracle为严格implicit直接到header、header完整entry+
implicit前驱，其余Break/exit/参数身份不变，未增加宽泛fallback或改生产CFG。

重新冻结green3的4304输入并构建，29 ordinary exact（FE11+CG18）全部实际通过，
各1 passed/0 failed/0 ignored，包含原authority红、最终SSA参数/完整incoming，
及Basic/Const的5组正常native（零轮、Break、2/7轮正常回边）。新负控及原CI-only
仅编译，不执行Abort/fault；有限native不证明一般循环或资源计数。green/red2、
失败green/green2和诊断均保留在本片持久证据目录，保存binary与输入SHA绑定。
当前仍基fa82cfc2，新main4150bbd8须隔离合流重验，不据本片宣称最新main或CI完成。
默认与loan support两套FE/CG all-target strict Clippy、默认workspace all-target check
均实际通过；fmt、docs893、相对fa82cfc2尺寸2441文件/35历史超限、diff通过，
未提高尺寸额度。独审复核29日志、保存binary及真实Cargo artifact（opt0/debug2，
FE默认、CG仅loan support）；实测输入后仅Architecture/Spec文档变化，无Rust漂移。
本片仍只作本地完成切片，正式CI与最新main合流未验，不宣称D10/E2全部完成。
### 有界双出口真实aggregate receipt合同（2026-11-11）

本片基于fa82cfc2，复用已合流的局部branch Complete，不扩语言或声明门禁。
仅同Shared String来源、动态Boolean参数条件、真臂直接return与唯一false尾return。
source/callable仍Partial；旧表达式forward receipt保持独立且本分支样例中仍为空。
不新增Begin/End/Drop、权限恢复、跨函数LoanId，不退休旧oracle，不认领native资源计数。

后继生产必须消费同catalog的唯一Complete区域及两条认证path，核实际typed callee、
原split/condition/臂/control/value与action身份。真实builder原producer只执行一次；
实际append得到instruction句柄、实际set_terminator成功后才记录出口，不扫描最终SSA
凑receipt。entry source经conditional edge operand到arm block parameter再至BorrowCall
source/result和BorrowReturn的完整身份链必须成立，不能仅比较root或强令LoanId相等。
全部两路径和实际condition/边闭合后才能发布一个aggregate，含两个路径receipt。
Partial/off不建计划；已准入后缺失、重复或错身份须报错，不允许退回未观察成功。

前置红版本仅test-only独立branch_receipt观察空壳，无生产consumer/publisher；paths占位仅为
红探针计数，生产实现时必须替换成真实身份记录，不能由计数硬编码证书。
新ordinary exact与旧branch probe共享原源码和原lower/verifier/SSA/LLVM检查，先核FE
唯一局部Complete、两path及真实Begin/handoff、source Partial和on/off旧语义等价。
Rooted/Memory四次真实lower、原最终verifier捕获、独立SSA核验及LLVM一致全部完成后，
才断言on各一个aggregate含两path、off零；预期空壳运行时0!=1，编译失败不算红。

真实红0e60721d：codegen no-run 617.08秒成功；唯一ordinary实际0 passed/1 failed。
四组合原lower、原verifier、SSA与LLVM（1754 bytes）全部先通过，branch aggregate
均为0，最后rooted精准0!=1。输入与独立保存binary执行前后一致，原证据保全。
后继consumer独立于旧表达式Plans：builder登记原四block与false唯一Goto，emitter
在原merge_straight_lines真实折叠处保存false→join映射，不保留额外block改变旧SSA。
普通probe追加真实FE action IDs及两个call/return、condition Read与Conditional
原edge的完整身份断言。生产candidate a481a8fe 已完成源码独审及下列真实验收，
不认领远端CI或native。函数emitter仅暂存完整aggregate；观察批次在原unit最终verifier
成功后提交，提前lowering/verifier错误由guard撤销pending，不新增验证次数。

首次生产候选2f7d6de4的no-run在43.41秒编译失败：ordinary probe引用私有BlockRef；
这是编译接线错误，不替代前置behavioral红。a481a8fe保持内部身份私有，只暴露测试布尔
查询；真实终结事件dispatch归属LoanReceipts，emit.rs回到990行，不提高尺寸额度。
失败与修复证据分别保留。修复后no-run59.60秒成功，4305个输入及唯一实际编译artifact
绑定并独立保存binary；六项ordinary执行前后输入/binary摘要一致，各exact唯一list且
各1 passed。Rooted/Memory各on一个双path aggregate、off零，原SSA/LLVM仍相同。
默认/观察feature FE+CG all-targets strict Clippy分别56.62/57.48秒成功，workspace
all-targets check52.51秒成功；fmt/docs893/diff及尺寸2442文件、35项历史欠账门禁通过。
本片基线仍fa82cfc2；合入更新main的组合须另做联合验证，不外推本片结果。

| 验收项 | 测试目标/过滤器 | 实际结果 |
| --- | --- | --- |
| aggregate receipt能力红及后继consumer | branch_forward_probe::ordinary_branch_requires_actual_aggregate_receipt_with_true_off | 0e60721d真实0 passed/1 failed；a481a8fe后继1 passed |
| 旧双出口lowering/original expression receipt边界 | branch_forward_probe::ordinary_same_source_branch_requires_real_lowering_with_true_off | a481a8fe各1 passed，另核邻界、旧Single门禁、旧expression receipt及raw handoff |
| 缺路径/重复/错edge operand/异catalog等破损 | branch_receipt::emitted::ci_tests 两项 ci_only | 已实现并编译；本机未执行 |

六项ordinary exact为branch_forward_probe的ordinary_branch_requires_actual_aggregate_receipt_with_true_off、
ordinary_same_source_branch_requires_real_lowering_with_true_off、ordinary_branch_neighbors_keep_the_unit_declaration_gate，
borrow_result_contract_tests::borrowed_result_caller_cfg_and_block_returns_stay_precisely_unsupported，
forward_receipts::original_live_forward_requires_production_receipt_with_true_off，
forward_projection::original_branch_returns_keep_both_raw_handoffs_without_a_cause。
CI-only覆盖12类身份变体、6类缺角色、反向/重复fold及3类builder阶段，另模拟pending
遇Err撤销/commit正控；不等同真实最终verifier故障注入或独立mixed-join构造的执行证据。
两位交叉审区分生产作者与注入测试作者，冻结输入、原artifact、独立binary和全部普通
运行证据另经逐项独核。source/callable仍Partial，非此有界拓扑和新End/Drop未由本片扩大。

### 条件 Break、分支 receipt 与主线退役协调验收（2026-11-11）

已测代码 `3e8ca18b` 保留已发布#412 `fa82cfc2`、main `4150bbd8`、owner `3fcd5a4f`
与borrow `a9c9318a` 的真实父链；末次核远端main未推进。main删除旧binding_receipt与
owner_return壳时，保留删除并将PR独立child入口/完成hook迁到borrow_binding，将open-loop/
header Return互斥授权、emitted与observer记录迁到owner_consumer/replacement，不复活旧壳。
condition同插入点冲突完整保留cb_split与branch_plans.split，两者在唯一条件发射前登记，
原emitted顺序不变；Spec双追加原样保全，未纳入任何在途未验成果，源树用户修改未动。

联合35项ordinary加3项binding/parent-child receipt邻域，共38项各1 passed，FE11/CG27。
包含已审有限Basic/Const正常native；Abort仅SSA结构检查，无资源计数、压力或全量执行。
新分支聚合receipt与旧child/binding/Return真实接点均有定向回归，CI-only仅编译。
仍只认领已准入窄形状，不把source/callable Partial或一般循环/借用联合合同宣称完成。

持久唯一证据主目录为主仓忽略目录
`.local-artifacts/conditional-receipt-integration-evidence-20261111`。attempt-1在38项全过后
fmt抓到迁移child_entry后的缺换行；仅格式修正成3e8ca18b，保留原记录。attempt-2重新绑定
4300个tracked输入、实际Cargo artifact、两份保存binary、runner及精确名单，重新实跑全部
38项并通过；执行前后输入与binary一致，opt0/debug2，FE默认、CG仅loan support。

默认及loan support双套FE/CG all-targets strict Clippy、默认workspace all-targets、
默认CLI build/真实--help、fmt/docs897、相对4150bbd8尺寸均通过，CLI features=[]。
尺寸2433手写Rust、35历史超限，无额度豁免。最后仅追加本Spec；未推送、未合main，
没有本组合正式CI结论，D10/E2继续保持进行中。

### 条件直接 Return 与隐式正常回边：待验证切片（2026-11-11）

从已验收条件Break切片 `3fcd5a4f` 隔离推进ADR-0038的Return独立边角色，保留
双不可变Int参数、单String根、零初始化计数、严格小于、Replace及加一，仅考察
尾部 `if(index < stop) { return held }` 无else，循环后仍直接Return当前根。
true边交付当轮Q并退出函数，不进入loop exit/header；false边正常继续到header。
既有无条件循环Return的消费证据与私有H守恒可以复用，但不能把Return伪装成Break
或将已移动状态与live Q补齐。未涉及RHS提前退出、多根、嵌套或capture。

先以独立普通原样probe经过FE Basic、真实lower、SSA verifier、LLVM，核两个实际
Return各自payload授权；前置失败与最终授权缺失分开记录。当前尚未执行，不称确认
红或完成生产。旧ConditionalBreak/Normal/Continue/Mixed的门禁不放宽，本片只在
隔离树本地提交，不推送，后续须结合最新main与其他工作独立合流验证。

独立普通红已真实确认：唯一exact `conditional_return_requires_both_actual_return_authorities`
在第37行owner authority断言失败（exit101，0 passed/1 failed/0 ignored）；前置
Basic、实际lower、SSA verifier、LLVM及两个不同Return收据均已通过。这里只确认
至少一个实际Return缺授权，失败后未检查完其余收据，不声称两个均已分别定位。
4305输入与真实Cargo artifact、保存binary SHA及原日志绑定，测试前后不变；证据
保留于主仓忽略目录 `.local-artifacts/owner-conditional-return-20261111T113054Z`。
此红非前置拒绝；尚无生产接线、native或CI-only执行结论。

独立ConditionalReturnBackedge现已生产接线：H核心的尾部Return与body Return端点
分开，true臂真实Take→交付→空状态cleanup、臂终止及有序Move/Result均封印；
false臂独立六个live快照包含If/body正常完成。精确上下文门只准本片当前事务的
body Return，不全局放宽分支payload Return，旧Break/Continue/Mixed保持独立。
CG独立body-return receipt及taken/emitted状态，两个payload各自验证；true不入
header/loop exit，false携Q正常回header，完整incoming及严格参数来源另行校验。

首次green的FE12/13，新产品正例失败；CG虽编译但未运行且binary保全。定点诊断
证明Take/交付/cleanup及implicit清理已到达，seal误只接受Move；真实Name消费还
生成同payload/owner的Result。修复为严格长度2、顺序Move→Result，拒绝任何额外
或错序事件；public一致性与CI-only换序负控同步，原authority探针未改，临时日志
全部移除。失败及诊断原记录保留，不将其称为通过。

重新冻结green2的4308输入并构建，34 ordinary exact（FE13/CG21）各1 passed、
0 failed、0 ignored，包含两个实际Return各自授权、SSA完整incoming/各payload/
参数来源及Basic/Const五组有限正常native。新13类FE及3类CG损坏负控仅编译，
所有CI-only未执行；有限运行不证明任意循环或资源计数。实测输入、实际Cargo
artifact、保存binary SHA与日志均绑定于本片持久证据目录green2。当前基3fcd5a4f，
正式CI及最新main/其他分支合流仍须独立验收，不宣称D10/E2整体完成。
默认及loan support两套FE/CG all-target strict Clippy、默认workspace all-target check
均通过；fmt、docs893、相对3fcd5a4f的尺寸2445手写Rust/35历史超限、diff通过。
无新增尺寸例外。独审已核34真实日志、默认FE/仅loan的CG artifact及保存binary；
最终账本仅改文档，无Rust漂移。本片只作本地提交，最新组合及正式CI仍待验证。
### 辅助 token 退休的最窄 Partial 原因链

本片复用原样 `view(source, aux)` 返回 source、`view(owner, owner)` 初始化 borrow val
并消费的合法路径；同一 owner 的两个实参必须保持不同 loan/argument 身份。前置
ee91f2e9 已由普通 CG probe 真实验证：no-run 145 秒，唯一 ordinary 1 passed，
Rooted/Memory 各 on/off 原 SSA/LLVM、源保留与辅助先退休顺序通过。

新增只读 `UnitLoanAuxiliaryRetirement` 引用同 catalog 的 source_begin、
auxiliary_begin、原 handoff。9e75a1d9 空 API 的普通 FE 探针完成两布局旧产品等价、
raw aux 恰一及两个真实 Begin 后，实际运行 0 passed/1 failed，关系数 0 != 1；
no-run 15.00 秒成功，非编译错误。原红输入和独立保存 binary 保留。

生产在原 observe 时从仍 live 的唯一 source PendingLoan 捕获 Begin handle，
从原 auxiliary remove 取出其 Begin handle，在 initializer 成功的原 commit 处绑定
handoff。只准入直线、根层、两个 Name 实参的 Shared String 调用：同一 owned root、
不同 argument、source 为第零参数、恰一辅助。独立 finalizer 检查 catalog、顺序、
原 fact、不同身份以及 source/aux/handoff 复用；source 亦不得跨 candidate 重复或与
aux 交叉复用。该最窄片不支持一个 source 关联多个辅助。拒绝只影响新关联，不改变
旧 fact 投影、原退休或 cleanup；不扫描 descriptor 重建“最近 Begin”。source 后续
正常退休仍合法。无新 End/Drop、region、coverage Complete 或 receipt；source/callable
保持 Partial，旧 binding 无辅助门禁不放宽。

6aff5010 的 FE 2 + CG 9 ordinary 均真实通过，但默认 strict Clippy 指出
unnecessary_filter_map。08902db5 仅等价改为 filter，不提高门禁额度；随后新冻结
4310 项输入重新编译 FE/CG（8.96/17.30 秒），两份独立 binary 和实际 artifact 绑定。
FE 2 + CG 9 exact 各唯一、各 1 passed，执行前后输入及 binary 摘要不变，已独核。
FE 正控核两布局各一个原身份关系；三邻界（不同 root、第二参数为 source、两个辅助）
各两布局保持零新关系及原 raw handoff。CG 再核四组合的真实 SSA/LLVM、辅助先退休、
source 保留及原最终 verifier，兼顾旧 branch/expression/binding receipt。
默认/观察 feature 的 FE+CG all-targets strict Clippy 分别 41.36/42.03 秒通过，
workspace all-targets check 39.53 秒通过。失败证据与修复后证据分别保留。

| 验收项 | 测试目标/过滤器 | 实际结果 |
| --- | --- | --- |
| 原样辅助退休合法性 | aux_handoff_probe::ordinary_auxiliary_retirement_preserves_live_source_with_true_off | ee91f2e9 真实 1 passed；08902db5 再验 1 passed |
| FE 原因链能力红绿 | unit_auxiliary_retirement / ordinary_auxiliary_retirement_requires_actual_acquisition_links | 9e75a1d9 真实红；08902db5 真实绿 |
| FE 三邻界 | unit_auxiliary_retirement / ordinary_auxiliary_retirement_neighbors_keep_partial_raw_handoffs | 08902db5 真实 1 passed |
| 缺失/外来/复用/交叉角色候选 | auxiliary_retirements::tests::ci_only_auxiliary_links_reject_missing_foreign_reused_and_cross_role_tokens | 16 类变体已编译，本机未执行 |

CG 九项 ordinary 为上节六项，加 aux_handoff_probe 的合法性探针，
binding_receipts::complete_binding_requires_real_production_receipt 与
binding_causal_links::binding_lifecycle_requires_actual_causal_links_with_true_off。
CI-only 重复 case 是整体重复拒绝，未单独隔离 handoff 复用条件；真实 Err/deferred
撤销、动态循环、跨函数身份、更广类型、native 及完整辅助生命周期不由本片认领。
生产作者与故障测试作者交叉独审；本片仍基于 a9c9318a，更新 main 的组合另行验收。

### 条件 Return 与 auxiliary 因果链联合验收（2026-11-11）

本次隔离组合保留已发布 `d26171d2`、最新主线 `4150bbd8`、owner `6433a1c2`
与借用 `cbdbe053` 的真实父链；实测代码头 `f7660ae1`。条件构建同时注册 owner
条件Return与借用branch计划，真实条件仍仅发射一次；保留主线退役壳迁移，不恢复
已退休文件。新增类型导出取两线并集，没有扩大语言准入或借用Complete范围。

主仓 `.local-artifacts/conditional-aux-integration-evidence-20261111/attempt-1`
保存跟踪输入、runner SHA、3份真实Cargo artifact及独立binary SHA、精确list/run日志。
本组合47普通测试全部通过：owner FE13/CG21、借用辅助因果FE2/CG9，再加去重后
binding/child邻域2项。使用opt0/debug2；不执行CI-only、故障注入、Abort或资源计数。

默认及loan support两套FE/CG all-target严格Clippy、默认workspace all-target检查、
默认CLI构建及实际help（features为空）、fmt、docs897、diff均通过。
相对4150bbd8尺寸2442手写Rust文件、35历史超限，无新增豁免。最后仅追加此Spec；
本地组合验证不替代正式CI或独立审查，不推送、不合main，D10/E2仍有后续范围。

### own String 参数进入正常 Int header：待验证切片（2026-11-11）

本片从已发布组合 `ba025f6f` 隔离推进ADR-0038的真实入口owner身份。最窄合同为
一个不可变Int limit及一个own String incoming，`var held = incoming`，index从0
开始、严格小于limit、body固定Replace及加一，正常回边，循环后直接Return根。
保留Parameter原catalog/owner身份，经真实Move/Binding进入H，Name或Binding不
制造新producer；零轮返回实际入口owner，后续激活Q与入口来源独立。

不同时推广Continue/Break/条件Return、多个根、capture或新控制条件；既有Parameter
无条件loop Return不是本片正常回边的证据。先用普通原样probe经Basic/lower/SSA/
LLVM及唯一Return authority定位真实缺口；尚未执行，不称确认红或完成。后续独审、
真实输入/binary绑定、本地分步提交，未获本批推送授权，不动其他工作树或target。

真实普通红已确认：唯一exact `owned_parameter_normal_header_requires_actual_return_authority`
第32行authority缺失失败（exit101，0 passed/1 failed/0 ignored）；前置Basic、真实
lower、SSA verifier、LLVM、唯一Return及非空payload均通过，不是前置拒绝。
4310输入、实际Cargo artifact及保存binary SHA在执行前后绑定不变，原日志保留于
主仓忽略目录 `.local-artifacts/owner-parameter-header-20261111T125330Z`。
本阶段尚未生产接线或运行native，CI-only仅编译，不称新增能力完成。

本片复用现NormalBackedge而未新增wrapper，Hcore封印initial_origin；只有一个Int
及一个own String且raw参数Name、Int加一、纯Fallthrough可进入参数入口路径。
真实Parameter catalog保留原身份，严格Move→Result→Binding，不创建第二个入口
producer。后端从实际Value ABI绑定保存slot/index/symbol，Name只运输该slot；最终
参数初值无指令定义、唯一ABI引用且原参数已消费，literal路径仍严格核StringLiteral。

首次green的FE编译通过，CG新增SSA测试因StringLiteral模式漏string字段报E0027，
未执行测试；仅修为bytes加其余字段忽略，原红与生产不改，旧编译日志保留。
green2重新冻结4311输入及实际artifact，47 ordinary exact（FE20+CG27）全部
各1 passed/0 failed/0 ignored。其中新5项加旧34项及Parameter open-loop的8项
直接消费者回归，核原Parameter身份、真实entry→header、Q正常回边与H Return；
Basic/Const覆盖0/1/2/7正常轮次及两个不同零轮caller串。新损坏负控与全部CI-only
仅编译，不执行故障/Abort；普通native不证明一般循环或资源计数。
输入/保存binary/runner/日志在本片持久目录green2逐一绑定，不使用旧组合或CI结果
代替当前实测。正式CI、最新main/其他新分支合流仍待验，不宣称D10/E2整体完成。
后续strict Clippy发现两处UnitOwnerOrigin已Copy仍调用clone；未加豁免，只移除
两处clone，保留失败日志。green3重新冻结4311输入并通过默认/loan两套FE/CG
all-target strict Clippy、默认workspace all-target check，再真实重编并重跑原47项，
全部通过。最终交付以green3为准，不把green2的实测当作修正后结果。
最终fmt、docs897、相对ba025f6f尺寸2444手写Rust/35历史超限、diff通过，无提高
尺寸额度。独审重新核green3的47日志、真实artifact与保存binary；实测后仅Spec
账本变化，完整输入最终绑定。本片仅本地完成，不包含正式CI或最新合流结论。

### 正常 header 的本地最终绑定消费：待验证切片（2026-11-11）

本片从已验本地c57c4b48隔离推进ADR-0038的最终消费义务，不再扩控制组合。
保留单Int limit、literal String初值、单根Replace与加一及正常回边；循环后唯一
`val retired = held`，函数返回Unit。当前header仅直接Return；既有Break两出口
绑定消费不能冒充多轮H。须保留真实H来源及Take→Bind→Drop，不创建新producer，
实际清理消费与完整incoming分别验证，不以普通运行成功代替owner授权。

首先仅增加普通原样probe，经FE Basic、真实lower、SSA与LLVM分层定位缺口，再核
实际最终Drop身份及授权。前置失败另记，不提前称authority红；不执行native资源
计数、fault或CI-only。本阶段尚未执行、未完成生产，不推送或改已冻结工作树。

首次probe编译因测试调用私有UnitSymbolId::new报E0624，尚未执行；该失败仅为测试
构造API错误，不作为消费缺口红。保留原编译日志，改用公开原身份查询后重新冻结验证。

red2已真实确认消费授权缺口：唯一ordinary exact
`normal_header_final_binding_requires_actual_drop_authority`在第71行授权缺失失败，
exit101、0 passed/1 failed/0 ignored；此前Basic、原retired Drop fact、真实lower、
SSA verifier、LLVM及精确实际Drop观察均通过。4312输入、实际Cargo artifact与保存
binary SHA绑定；本地证据owner-header-bind-20261111T133204Z保留首编译失败和red2。
此红仅证明最终Drop缺授权，尚未证明H来源或生产完成，不扩大为语言拒绝。

生产候选抽唯一HeaderEquation与CG transport校验，分别保留旧Return及新BindDrop
端点，不复制H守恒、不制造Return身份或producer。首次green编译有FE字段迁移与
旧匹配遗漏错误，green2另遇CG抽模块后RAbort路径错误；各自保留原日志，未执行。
green3真实FE22中21通过，新产品未发布；临时诊断证明准入、H安装及Take均成功，
但原evaluate_owner_expression因旧观察失活清空实际owner，Binding得到None。
修复仅对该BindDrop精确Consume成功完成且原Some owner匹配已Take身份、H方程、
空后状态和未完成旗标的情况保留原结果，不从snapshot回填；Bind须真实完成旗标，
新增损坏负控仅CI编译。临时诊断全移除，原失败输入/日志/二进制均保全。

green4重新冻结4315输入并真实重编，52 ordinary（FE22+CG30）全部逐项通过：
原Drop授权红、真实P/Q到H及H到最终Drop的完整incoming/slot来源、新Basic/Const
0/1/2/7正常native和旧47项回归。saved binary SHA与实际Cargo产物及输入绑定，
没有复用前轮失败或旧构建结果。CI-only不执行，普通native不证明资源计数、一般
控制、参数入口BindDrop、多根或capture；最新main/组合与正式CI仍待验，本地不推送。

随后strict Clippy提示NormalSuffix大枚举；仅为BindInput加Box，未豁免或改合同。
green5重新冻结4315输入，通过默认/loan双strict Clippy及workspace all-target check，
重新编译并真实重跑原52项全部通过。最终以green5为准，不用green4结果替代布局
修正后的实测；原编译失败、21/22及Clippy失败和诊断证据均保留。
最终fmt、docs897、相对c57c4b48尺寸2448手写Rust/35历史超限及diff门禁通过。
独审核52日志、实际binary/artifact及完整生产；最终仅本地切片完成，合流/正式CI另验。

### 辅助绑定 Partial 观察前置独立交付

本交付分支基于 ba025f6f，仅迁移原实际 use-return/source-retirement 的独立观察能力。
不携带辅助 binding region 空 API 或故意失败的区域探针，不实现任何新 Complete 区域或
CG receipt。原区域实验树及其全部失败、诊断证据独立保留，不改写或删除。

前置红 8fd9b376 已真实编译并执行：Rooted/Memory 两布局旧产品等价，原 aux 恰一、
原辅助关系恰一，但实际 use-return/source-retirement 均为零，原记录六条；两布局
先验完成后在 use-return 0 != 1 失败。这是 Partial 观察能力红，不是区域覆盖红。

生产候选采用独立 auxiliary token：原成功 handoff 的真实 source/aux acquisition、
source-live/narrow 见证激活，同 catalog 且两个 Begin 早于 handoff、互不相同。
原 consumer 返回时登记一次实际 use，原 binding 退休时关联该实际 use/source/handoff；
重复激活、重复使用、合流或身份失配使新资格失效，已发生事实不删除。旧 observable、
consumer_region_loan、dependency_owner、child/Dependency 权限均不放宽；新退休观察
不返回 Drop-cause authority 给旧 dispatcher。source/callable 保持 Partial，无新 End/Drop。

| 验收项 | 测试目标/过滤器 | 实际结果 |
| --- | --- | --- |
| 前置 Partial 观察能力 | unit_auxiliary_binding_causes / ordinary_auxiliary_binding_requires_actual_use_and_retirement_causes | 8fd9b376 真实红；c4862b7c 重新编译后 1 passed |
| 原样合法邻界 | unit_auxiliary_binding_neighbors / ordinary_auxiliary_binding_neighbors_preserve_legacy_partial_products | c4862b7c 真实 1 passed（六例、两布局） |
| 同 catalog、先后身份、重复激活及合流粘滞 | auxiliary::tests::ci_only_auxiliary_activation_rejects_foreign_order_duplicates_and_merge_revival | 独立编写并交叉源码审查；c4862b7c 已编译，本机未执行 |

普通邻界包括异 root、来源参数 1、多 aux、Group 实参、重复 consume，以及 child 后
consume(child) 的身份失配。重复 consume 可保留首条 use 事实，不认领后继退休资格；
不把单独 child 初始化或任意额外 owner read 一概判为撤销。默认/feature strict Clippy 分别 47.59/59.40 秒通过，workspace all-targets check
40.23 秒通过；FE 2 + CG 11 ordinary 全部各 1 passed。4313 项冻结输入、原编译 artifact、
独立 binary 与执行前后摘要均经独核，FE no-run 54.16/1.01 秒，CG no-run 62.36 秒。
CG 复用前节九项精准回归，追加 parent_child_drop_dependency 的
parent_child_requires_actual_partial_drop_dependency_with_true_off 与
later_parent_or_owner_use_preserves_old_products_without_drop_dependency。
fmt/docs/diff 通过；尺寸2446手写Rust文件、35历史超限，无新豁免。
原实验 a4199734 另有真实区域能力红：两布局完成8记录后才在区域0!=1失败；它未进入
本交付分支。FE证据manifest的scope沿用旧red模板，实际结果以本头绑定、命令及运行日志
为准，不能解读为Complete或红结果。当前仍不声称native或正式CI验收。

### 参数 Header、最终绑定与 Partial auxiliary 因果联合验收（2026-11-11）

本次以已发布ba025f6f及其主线4150bbd8为基线，保留owner f48eb392与借用42a80a2e
真实父链，实测组合f9504e81。仅Spec附录冲突双保，源码自动合流；未带入独立试验树
中的区域红或空API。参数入口、当前H最终binding/drop权限与原auxiliary UseReturned/
SourceRetired因果共存，仍不扩Complete/region/receipt或legacy Dependency权限。

主仓 `.local-artifacts/header-aux-causes-integration-evidence-20261111/attempt-2`
保存69项普通测试完整重跑（FE26/CG43），保留已发布child/binding及Dependency邻域。
前次attempt-1将两个不同集成测试错映射同binary，严格exact-list断言拒绝空列表；
该失败完整保留，不算产品红或通过。修正后causes与neighbors各自真实artifact，合计
5份保存binary绑定本组合输入及runner SHA，每项精确list唯一并实际1 passed。

默认及loan support两套FE/CG all-target strict Clippy、默认workspace all-target、
默认CLI构建及实际help（features为空）、fmt/docs897/diff均通过；尺寸2452手写Rust
文件、35历史超限，无新增豁免。仍用opt0/debug2，不执行CI-only、Abort或资源计数。
最后只追加本Spec；未推送、未合main，正式CI和独立审查不能由此替代，D10/E2未整体完成。

### Root-qualified edge 身份基础（2026-11-11，本地开发）

依据 ADR-0038，首片将现有 sealed normal header 的结构边绑定同轮 catalog、callable、
control 及 root，不改变 planner、旧 oracle 或 consumer 权限。此为一般多根/嵌套前置
身份隔离，不提供多根 incoming 相关性或嵌套 CFG 完整证明。先以真实源码重分析探针
验证同 AST ID 不等于同轮边。7b75b8b9 的空 API 探针实际编译并执行，1 failed；
实现后实际重编，三个身份用例全部通过：同分析 clone 保持身份、重分析拒绝复用，
跨 source，以及同 source 不同 callable/root 隔离。均核旧 observer on/off 产品等价。
既有 normal_backedge 的 17 项普通回归全部通过；CI-only 损坏未封闭 header 不发布边
用例已编译但未执行。FE all-target strict Clippy 通过。workspace all-target check（LLVM 21）、fmt、docs897、diff 通过；尺寸2454手写文件、
35历史超限，无新豁免。尺寸脚本首次漏传必需 --base，未执行；补传93404d2a后通过。
未运行 native、正式 CI，未完成多根相关 incoming、嵌套 CFG 或旧见证退休。
原始命令结果在本工作树 `.local-artifacts/root-edge-evidence/`；不以这三个源码用例
推导任意根或任意控制形状。只读独立审查未发现此身份封装范围的阻断问题。

### 一般多根与嵌套控制的真实输入基线（2026-11-11）

在16a46213后保留五个真实合法输入：双根逐一Replace的正常/Continue回边，外层根与
内层根共存的嵌套While正常/内层Continue/内层Break。两个general_control_frontier
测试实际运行通过：无diagnostics/deferred，validate成功，原observer on/off产品等价；
现有closed/invariant/open-loop能力均不发布，不把一般输入偷换成单根产品。
这是测试基线，不是新运输实现，也不证明逐路径无泄漏或nested native。

调查确认当前DropPlanner仅持有一个owner_loop槽位，NormalInput严格接收单根及固定
顶层/循环体序列。后续应按真实control identity组织事务与父控制关系，并在同一控制边
上整体记录root-to-owner向量，保留根间相关性，禁止按每根独立Choice再取笛卡尔积。
每条实际回边重新验证该向量；未进入或提前退出的嵌套循环不得伪造incoming，Return/
Break/Continue分别归属真实目标。现有root-qualified edge是身份基础，不替代这些义务。

### ControlRootFrame 内部原子根向量基础（2026-11-11，本地）

从f6bfb4a5继续，将NormalInput的prefix、jump及body completion真实边界采集接入
内部ControlRootFrame。该借用视图保留整个原values向量，不排序、分拆或组合路径；
未初始化owner、重复root、重复owner、root symbol与control跨source或各owner的
callable彼此不一致时向量整体失败。查询
须给出相同source-qualified control，现有single_for仍只接受唯一根，旧准入不扩展。
它不验证owner callable属于该control/root或跨catalog成员关系，不是跨分析capability，
也尚未替换单槽LoopTransaction；
真实owner catalog与incoming封闭验证仍由原HeaderEquation完成。

真实normal_backedge 17项回归已通过。结构模型以实际分析的绑定ID构造二元测试向量，
验证原子保序及控制ID隔离；该测试向量不是实际循环快照，不能用来宣称多根运行支持。
模型1项已通过，CI-only缺owner/重复root/重复owner负控仅编译、不本地执行。
最终重编后23项普通测试通过（normal17、qualified3、frontier2、模型1），FE all-target
strict Clippy、fmt、docs897及diff通过。触及Rust文件均未达1000行，最大900行；
无新公开API或codegen变化，本片未追加workspace/native，未运行CI-only或远端CI。

本片只读独立复审未发现当前三处single_for接线的语义回归；按审查收窄source归属措辞。

### 真实 While 控制栈与活动事务路由（2026-11-11，本地）

以ef2f161a为基础，While真实遍历入口/出口维护source-qualified ControlStack；内层
退出先恢复父控制，再传播Result。活动Return、Break/Continue与开始采集均核当前control
匹配原事务；栈无效或不平衡不得发布loop产品。重复进入同控制、跨source嵌套或错序退出
将栈置为粘滞无效，不能靠后续pop恢复资格。下一callable重新初始化。

f4a78fbf的空栈实现已真实编译并运行，nested父control应为外层却得到None，1 failed；
实现后重新编译并实跑：嵌套正常/内层Continue/Break/Return与顺序循环的探针读取实际
planner hook，不向生产状态回灌。退出后恢复外层、顺序循环不虚构parent，旧observer
on/off完整产物等价。CI-only重复进入及错序退出粘滞负控已编译、未执行。

普通60项通过：frontier4、normal17、closed18、open-return4、parameter5、replace-break8、
qualified3、root模型1。FE all-target strict Clippy、LLVM21 workspace all-target check、
fmt/docs897/diff通过。独立审查无阻断；Err恢复仅源码核实，未做本机故障注入。
drop_planner.rs增长1657→1663仅字段和初始化接线，经独立尺寸复核仍属原职责，保留
既有max_lines1669，不提高额度；控制栈算法留专用子模块。
这是实际控制身份路由基础，仍只有原有单槽LoopTransaction，不发布一般多根incoming、
嵌套owner运输或退休旧oracle；控制遍历轨迹也不等于运行时循环激活轨迹。

### Control-keyed 私有事务存储（2026-11-11，本地）

基于cbf1720b，以ControlTransactions替换DropPlanner的Option事务槽。容器按
source-qualified control索引实际LoopTransaction；While进入选择当前control，退出
在传播Result前恢复旧选择。prefix/suffix仍选择原准入控制；无匹配control时as_ref/as_mut
均不给出其它事务，borrow准入的is_none仍检查全部存储，不能把暂时未选择误算成无事务。
若control进入失败清空事务；父级恢复不再假定事务存在，仍恢复原observation/choice/depth
及legacy错误传播，不因新容器缺项panic。后者为源码审查，本机未注入非法嵌套负控。

真实两函数各发布NormalBackedge且callable/control不同；基于同一checker创建的真实
admitted事务另核选择其它control时无引用、恢复原control时同一事务仍在，take清空状态。
该选择测试是实际准入事务的容器隔离测试，不是两个循环在同一callable同时运输的证明。
现有begin仍只准入一项，take仅接受恰好一项，不从多项中任选；一般多事务采集/发布、
真实多根incoming、嵌套路径归属仍未完成，旧oracle不退休。

最终普通owner suite 219项通过；全部FE lib普通测试638项通过，43项过滤（ci_only），
FE all-target strict Clippy、LLVM21 workspace all-target check通过。当前变更不含推送。
fmt/docs897/diff及Rust尺寸守卫通过（2458个手写Rust文件、35个历史超千行文件，无新额度）；
独立源码复审无阻断，确认恢复顺序和错误传播保留。loops.rs为939行，DropPlanner仍1663行。

### 顺序 closed While 的实际多事务采集（2026-11-11，本地）

以939892c1为基础，函数原AST仅包含至少两个While且每项均满足现有closed合同才整组
准入；每项使用同一checker与真实StatementId走原准入，normal/invariant/replacing任一
命中即不属于本片，任一失败整组拒绝。没有按文本重建AST或伪造body owner。
While真实遍历分别保存独立body catalog与控制产品，共享callable分析身份；退出后恢复
原选择，未放宽复杂控制流、跨循环owner、一般多根或嵌套准入。

719fc5c0的真实已编译红测因first real closed completion缺失失败。实现后真实两循环的
正常、Continue、Break组合均可采到各自产品；test-only探针从真实退出读取已生成产品，
并核一致性、同callable、不同control和body catalog。它不是运行时两循环同时发生的证明，
尤其固定flag可能导致首循环不终止；组合可达性和终止不在本片内。

第二成员Return、嵌套While或函数额外语句等合法源码邻接合同覆盖整组拒绝，不能留下首项
部分准入。初版负向fixture缺少语句分隔符而解析失败，修正后才计入最终验证。
公共batch尚无合同，finish主动标Unsupported且单槽take拒绝多条；不能把抛弃多结果后的
空observation发布为Complete。栈不平衡同样在clear之前拒绝observation，CI-only失衡
finalization守卫只编译、不本地执行。旧witness和legacy on/off完整产物对照保留。

最终FE lib普通测试640项通过，44项ci_only过滤；FE all-target strict Clippy与LLVM21
workspace all-target check通过，fmt/docs897/diff通过。两次独立源码复核无阻断，确认
失衡先拒绝再clear不影响合法平衡序列；本片没有运行CI-only守卫或推送远端。

### 辅助绑定独立局部区域：已发布 Partial 基线后的能力红

新隔离分支基于已发布 93404d2a，保留其中实际 Partial use-return/source-retirement
能力。仅复用早期实验的独立只读 `UnitBorrowAuxiliaryBindingRegion` 空 API 与普通探针，
不覆盖已发布观察实现、不修改旧实验树。此前六记录前置失败不是区域能力红；本次必须在
新基线重新编译执行，不能用旧日志代替。已在 426b811c 上重新编译并取得本次真实能力红。

声明区间从原 borrow val 语句进入、此前唯一 String 文本 owner 已实际形成，直到原
三语句函数 Block 正常完成且原清理后无 pending/owner 义务。准入限无参 Unit 普通
函数、直接 Name callee、两个 Name Shared String 实参同 root、不同 acquisition，
来源参数 0、恰一个辅助；随后仅 consume(item)。排除 receiver、泛型、跨线程、abort、
branch/loop/lambda、多 use、owner 后续读取等超出本声明范围的情形。

完整连续原区间必须恰八条：source Begin、aux Begin、handoff、consumer Begin、
UseReturned、BindingEnd、SourceRetired、Cleanup Drop。唯一已验证 auxiliary 关系
绑定偏移 0/1/2；use/retirement 逐实际 ID 关联，Drop 必须由真实退休、owner 移除及
append 传递证据，不能最终扫描凑齐。入口 owner 完整身份快照与实际移除快照一致；
Err/早退/缺出口/重复闭合/候选冲突/额外记录不能 Complete。局部 Complete 不包括
owner 形成前生命周期，不意味着整个 source/callable 完整，不新增 End/Drop/CG receipt。

新 candidate、活动索引与 AuxRegion Drop cause 独立。原 Region/Dependency 的
`cause.or_else(|| batch.cause())` 结果先保留；已有旧 cause 时仅新 aux candidate 失效，
不抢占、覆盖或改变旧七记录 Region/Dependency 及其消费者。多 binding 同 root 的原
歧义处理保留，新证据也不能由最后一个绑定恢复。旧 binding API/validator 不放宽。

普通探针先完成两布局旧产品和 owner on/off 等价、完整八条原记录、唯一 auxiliary
关系及原 AST entry/completion/consumer 身份，再统一要求一个新局部 Complete 区域。
该能力红阶段 API 保持空；2026-11-11 实跑先通过 rooted、memory 两布局的原产品等价、
八条记录、辅助关系及 AST 身份断言，随后在新区域数量 0 != 1 处失败。
此结果定位为尚未实现独立区域能力，不是已有 Partial 因果回归，也不计完成。

| 验收项 | 测试目标/过滤器 | 实际结果 |
| --- | --- | --- |
| 新基线独立区域能力红 | unit_auxiliary_binding_region / ordinary_auxiliary_binding_requires_complete_interval | 426b811c：编译成功；精确运行 1 项，0 passed / 1 failed，0 ignored / 0 filtered；两布局均 raw_records=8、auxiliary_relations=1、new_regions=0 |

后继独立验证须覆盖缺/多/乱序记录、异 catalog 辅助关系、替换 owner、缺真实 Drop
因果、候选冲突、异常/早退闭合；故障构造仅 CI-only 编译，不执行于本机。生产与后端
消费者均在取得相应真实红及独审后另行推进，不将此合同当作已有实现。

本次命令为 `cargo test --locked --offline -p lang-frontend --features loan-observation-test-support --test unit_auxiliary_binding_region ordinary_auxiliary_binding_requires_complete_interval -- --exact --nocapture`。
默认 unoptimized + debuginfo，未改 profile；未执行 CI-only。证据保存在本机
`.local-artifacts/aux-region-checkpoint-20261111T1505Z/` 的输入提交、退出码和测试日志，
不代替正式 CI 或完整工作区验收。本次仅更新此验收账本，不推送失败 checkpoint。


### 辅助绑定独立局部区域实现（2026-11-11）

在上述 426b811c 真实能力红之后，独立候选由实际 borrow val 进入，实际原 Block
正常完成后封闭；观察入口独立核唯一已形成 String owner 的定义身份、声明、来源和深度。
严格限制来源参数 0、同根两个不同 Shared acquisition、单辅助、单 consumer 的原三语句
普通函数，不扩展多根、嵌套、复杂 CFG 或整 callable/source 的完整性。

真实退休显式分为 legacy 与 auxiliary 两个返回通道，旧 Region/Dependency/Batch
仅消费 legacy；辅助 handle 只进入新区域。真实 owner 移除后验证完整快照，原 Drop
append 时封存退休到 Drop 的对应关系，不从最终 action 内容反推因果。旧 cause 优先，
同根多 binding、候选冲突、重复完成或 owner 不匹配均使新候选失效且不可恢复。
八记录 validator 独立要求唯一已验证 auxiliary 关系、逐记录 catalog/身份/顺序、真实
owner ID、空正常出口与真实 Drop 因果；旧七记录 validator 保持原样。

普通测试已实际证明 rooted/memory 两布局各一个局部 Complete，原 catalog 仍 Partial；
旧单实参七记录区域仍 Complete。六类相邻形状及额外 owner use、嵌套 if、分支 use
均不取得新 Complete，关闭 loan 观察的旧产品等价仍通过。首次扩充 fixture 使用错误
if 语法导致解析断言失败，已修为现行括号语法后重跑；不把该 fixture 错误记为产品缺口。

| 验收项 | 测试目标/过滤器 | 实际结果 |
| --- | --- | --- |
| 独立区域及普通边界 | unit_auxiliary_binding_region | 3 passed，原红转绿，保留旧七记录正控 |
| 相邻辅助运输与取得关系 | unit_auxiliary_binding_causes / unit_auxiliary_binding_neighbors / unit_auxiliary_retirement | 4 passed，未放宽邻域 Complete |
| 伪证据与生命周期 | 新 validator 5 项及 action_source 3 项 ci_only 测试 | 本机只编译，不执行；正式 CI 待验证 |

此批未迁移后端消费者、不新增 production receipt、不退休旧 oracle；需要后续独立消费者
和正式 CI 后才能进一步使用局部证书。全路径借用证明与 D10/E2 整体仍未完成。

最终普通测试共 7 passed；loan support frontend all-target strict Clippy、默认 workspace
all-target check、默认 frontend lib（含上述 8 项 CI-only）no-run 编译通过。首次 Clippy
发现新增 Option 返回后的两处 question_mark lint，已修正后重跑通过。fmt/diff/docs897
通过；尺寸 2459 手写 Rust 文件、35 历史超限，无新增豁免；旧 drop_planner 仅新增一行
模块注册（1657→1658），实现留新文件。独立审阅发现重复闭合失效遗漏，已修复并补
CI-only 测试；该 8 项未在本机执行。最终证据仍保存在上述本机目录。本批未执行完整
workspace 测试、CLI/native、正式双宿主 CI，也未提交后端消费者迁移或退休旧见证。


### 辅助绑定独立生产 receipt（2026-11-11）

后续在前端局部区域提交 64bd1ab3 上新增独立 aux-binding 消费者，不改旧七记录
binding admission、validator 或 receipt。只消费同 catalog 唯一局部 Complete；逐项
核对原 typed calls、两项 Shared acquisitions、handoff/transport 和实际 Drop 索引。
入场/提交/消费/退休/Drop/完成均由原 producer 授权；十条真实 SSA 操作分别为
source RootPlace/Begin、auxiliary RootPlace/Begin、BorrowCall、auxiliary End、consumer
DirectCall、result End、source End、Drop。每次 append 冻结实际 instruction/entity 身份，
不从最终程序扫描反推 receipt；单 block、六个实体、八个前端 action IDs 独立绑定。
测试观察仅在原 unit 最终 verifier 成功后原子发布，失败撤销本 unit pending receipts。

旧辅助 SSA 见证继续保留；新增普通同源码 rooted/memory 与 observation on/off 对照，
检查真实十条指令和六个实体、完整 frontend 身份及关闭观察时 SSA/LLVM 产品相等。
新增五项 ci_only 覆盖错误 catalog/角色/授权/重绑/退休/重复闭合与失败不发布，
本地只编译，正式 CI 才执行。不扩大 Complete 到多根、嵌套或复杂 CFG，
不退休旧 oracle，不声称完成全路径借用证明或 D10/E2。

新普通 receipt、原辅助见证及原七记录 receipt 正控共 3 tests passed；feature all-target
strict Clippy、默认 lang-codegen lib no-run（含上述五项 CI-only）通过。首次编译发现
新增 `from_ref` 参数多余解引用，已修复后重跑通过；不把编译失败计为能力测试红。
fmt/diff/docs897 通过；尺寸检查 2464 手写 Rust 文件、35 历史超限，无新增豁免。
默认 workspace all-target check 通过。未执行 CI-only、完整 workspace test
或 native；未推送、未获得正式远端 CI 结果。证据沿用上述本机目录 receipt-* 日志。


### 辅助绑定来源参数第二位（2026-11-11）

后续切片仍只允许同根、两个 Shared、两 positional 参数、原三语句和唯一 String
owner。来源由 typed borrow-return Parameter(0/1) 确定，不扩大多根、嵌套或路径证明。
真实 source-second 普通源码首先复现局部 Complete 0 != 1；接通 shape/validator 后
再次暴露区域仅六条记录，而非八条，定位到原 auxiliary_retirement_shape 隐含
Parameter(0) 门导致 auxiliary token 无法激活。修复以明确两种参数位置核真实 typed
参数/合同，并保留所有既有局部边界，不通过降低 validator 要求掩盖缺失记录。

前端 Expected 保留 verified source_index，八记录按真实发生顺序保存 aux Begin →
source Begin → handoff，关系、use-return、source-retired 必须引用正确 offset。
后端相应核 actual handoff source 身份与 typed 索引；十条指令仍保持 aux RootPlace/Begin
先于 source RootPlace/Begin，BorrowCall 的 source 仍为后者。receipt 的六实体维持
语义顺序 owner/source-place/source/aux-place/aux/result，append 期间显式核 chronology
与语义映射，不改写任何真实指令。旧来源首位、旧七记录及未覆盖邻域见证保留。

新增 rooted/memory × on/off 普通真源码对照和 frontend/codegen 各一项 CI-only
chronology 故障测试；CI-only 仅编译，不在本地执行。source-second 从旧负控移入
独立正控，其余多根、多辅助、重复消费、Group、子 binding 等边界继续保留。
前端 8 项 ordinary 测试通过，source-second 真红转绿；后端新来源第二位、来源首位、
旧辅助见证 3 项通过，旧七记录 receipt 正控另 1 项通过。新增来源第二位的
distinct-root 与 Group 反例也保持不取得 Complete。双 crate feature all-target strict
Clippy 通过，双向独立代码复审无阻塞问题。默认双 crate lib no-run（含新增 CI-only）
与 workspace all-target check 通过；fmt/diff/docs897 通过，尺寸 2466 手写 Rust 文件、
35 历史超限，无新增豁免。
证据在本机同目录 source-second-* 日志。
本轮不推送，不退休旧见证，不宣称完成全部 D10/E2 或完整逐路径借用证明。

### 夜间双线冻结合流（2026-11-11）

用户授权本次先推送一版，之后并行成果仅本地提交，次日集中交付。此批固定取
owner eaa4be48 与 borrow bbed9ee8，基于已发布 93404d2a 合入为 19032d58；不包含
后续原子 batch 发布或显式 if/else 双 return。唯一合并冲突为本账本尾部追加，双方
历史均保留；生产文件自动合并后重新验证。main 4150bbd8 未改动。

集成验证使用独立 Cargo target 和默认 profile，命令均加 `--locked --offline`；
借用/codegen 测试启用 `loan-observation-test-support`，所有普通 lib 运行明确
`--skip ci_only`。frontend lib 640 passed（53 filtered），五个辅助 integration
目标共 8 passed；`ordinary_auxiliary` 3 passed；codegen `ordinary_` 63 passed
（包含所匹配的 native 正反例，不是完整 native suite）；旧
`complete_binding_requires_real_production_receipt` 精确正控 1 passed。
初次旧正控模块过滤匹配 0 项，已改用实际函数名重跑；不以 0 项计通过。
初次 frontend 命令漏过滤，在编译阶段已停止（143，无测试执行），随后重跑普通集。

双 crate feature all-target strict Clippy、默认 workspace all-target check、fmt、
diff、docs897 及尺寸门禁通过（2473 手写 Rust 文件，35 历史超限，未提高额度）。
原始日志留本机隔离集成工作树的 `.local-artifacts/integration/`；CI-only 仅编译，
未本地执行；未执行完整 workspace test 或 Preview。旧远端 head 的 Preview DNS
失败不是本合流的成功证据；依用户要求不排查 DNS，不改门禁、不声称 CI 全绿。

能力仍按前述切片限定：owner 为控制身份/私有顺序闭合事务采集基础，借用为限定
辅助区域与 receipt；多根 incoming、一般嵌套/复杂 CFG、完整逐路径证明和旧见证
退休未完成。D10/E2 保持 active。远端推送及新头 CI 结果需另核实。
### 独立 closed-loop batch 原子发布（2026-11-11，本地）

在eaa4be48的实际顺序closed采集上，d5aa8066公开空getter红checkpoint已编译实跑，
因atomic real sequence publication缺失失败。现以独立UnitOwnerClosedLoopBatchV1
承接原registry中的完整有序控制manifest与全部已封存产品：至少两项，数量、顺序、
共同callable/analysis身份、各产品完整性、control/body唯一及body catalog独立均核验。
任一result缺失或交易混有normal/invariant/replacing族则整体失败，不选第一个或拼部分。

closed_loop_batch_v1核observation身份与第一control的Unsupported Syntax边界、空旧事件
及与Single/open/invariant/Choice互斥。legacy状态不改Complete，旧投影不能获得batch。
不完整/deferred的总发布会清空batch。真实两循环正常/Continue/Break与三循环Break检验
产品及on/off旧完整产物等价；这是结构目录，不证明多个循环共同可达或终止。
CI-only缺项、乱序、重复、跨analysis拼接/替换、旧产品冲突及deferred清除守卫仅编译，
不在本机执行。一般多根incoming、嵌套root、跨循环owner与旧见证退休均不在本片。

最终641项普通FE lib测试通过，45项ci_only过滤；FE all-target strict Clippy、LLVM21
workspace all-target check、fmt/docs897/diff通过。独立源码复核未发现阻断；公开目录仍
限定closed结构模板，不扩大组合可达性声明。未运行Preview、CI-only故障守卫或远端CI。

### 真实多根控制入口关联（2026-11-11，本地）

基于a454fbde，de320135以真实源码记录缺失入口的红checkpoint。条件求值前从实际
ValueState原子保留control-keyed root→owner向量；先核ControlRootFrame整组约束，
再核当前Complete observation的callable/source、Arc catalog身份及definition成员。
没有按symbol重建owner或用原绑定token替代当前token；同control失败重采集先删旧项，
函数结束清空，借用准入is_none不读取该私有观察数据。

初始记录位置晚于旧While statement precheck，catalog已被该拒绝清空，导致真入口测试
继续失败。修正为在真实statement precheck内、旧拒绝之前采集，不改变原拒绝本身。
真实Int/index两根入口与Boolean循环前实际Replace的新producer关联均覆盖；已有opaque
调用使observation提前Unsupported的前缀则仍不能用残留ID恢复资格。
该快照仅代表条件求值前入口，不证明条件后的存活、body/backedge运输、嵌套语义或一般
多根phi。公开产品保持旧边界，旧见证不退休。
CI-only跨catalog、缺owner、未知definition、失败重采集清旧值和finish清理守卫只编译；
其状态模型不替代真实入口测试。独立源码复审未发现现行私有采集路径阻断。

最终普通FE lib测试644项通过、46项ci_only过滤；FE all-target strict Clippy、LLVM21
workspace all-target check、fmt/docs897/diff通过。入口采集顺序修正另经独立复核，原拒绝
仍执行，不提升已有Unsupported前缀。未推送，未处理Preview。

### 多根条件后与正常落通候选证据（2026-11-11，本地）

基于df5f1da0，fc84d241真实红checkpoint因缺少条件后/落通标记而失败。RootEvidence
保留入口向量，真实condition求值成功完成后核整组root/owner精确相同；body实际Ok(true)
且条件后证据有效、整组仍相同才记normal候选。非正常返回不记正常回边，重采集和失败
清除相应旧标记，不更改ValueState、原admission或公开产品。

真实body Replace负测曾捕获误报：Unsupported后ValueState可残留旧owner ID，单凭向量
相同不足以证明保持。修正为活动控制中实际Call/Assignment求值前、owner active早退之前
独立置sticky tainted；条件后或body后不能靠同样残留ID恢复资格。body污染只撤正常候选，
保留此前已核条件后标记；所有候选一并污染是保守收窄，不是路径敏感别名分析。

真实两根不变循环、Break/Continue/Return、body Replace与condition内赋值覆盖实际hook；
CI-only重采集、缺owner、非正常退出及sticky后不可恢复仅编译。该私有候选不等于完整
header equation、一般多根运输或复杂控制流逐路径证明，亦不表示条件值为真或运行时
正常回边可达。旧见证继续保留，不处理Preview、不推送。

最终646项普通FE lib测试通过，47项ci_only过滤；FE all-target strict Clippy、LLVM21
workspace all-target check、fmt/docs897/diff通过。独立复审确认sticky修正封住此次
真实Replace负测，不能把静态复审替代实测或扩大为逐路径完整证明。

### 实际 root 写入与直接 producer 回执（2026-11-11，本地）

基于caa74a97，7ec4cd4c真实红checkpoint期望两次提交回执却得到0。独立记录直接、
非插值String到达真实完成hook；在checked Replace提交边界核同call唯一pending
transfers-at-call temporary、String primitive单root、当前root存在且属于已验证入口，
再消费对应producer完成记录，生成control/root/call/producer表达式关联。
该hook位于实参成功求值及returns检查后，但在旧owner observer早退前，因此不依赖
残留owner ID。每个call/producer仅一次，不造新的UnitOwnerId、不更新旧owner状态，
上一片sticky污染不因此解除。

真实两根分别写入与同根连续两次不同直接producer均覆盖；group/name RHS拒绝对应
回执（另一个符合合同的实际提交仍可记录），RHS提前return未到提交点所以无回执。
此处只能说遍历观察顺序；互斥分支可能分别被planner访问，回执尚无路径身份，不能拼成
运行时trace、逐路径完整证明或后续cleanup能力。所有数据私有，旧公开能力和见证不变。
独立源码复审未发现提前提交或producer错配阻断；未处理Preview、不推送。

最终649项普通FE lib测试通过，47项ci_only过滤；FE all-target strict Clippy、LLVM21
workspace all-target check、fmt/docs897/diff通过。无新的故障注入本机执行或远端CI声明。

### 静态 header 前驱候选封装（2026-11-11，本地）

在20ffa330私有守恒关系上，将入口与首次body末向量收为只读整组选择；ZeroBody与
FirstBody是静态前驱标签，不是运行时激活编号。同control才可查询，旧新定义须不同
catalog；真实两控制源码覆盖错control拒绝，原多根及逆序矩阵覆盖整组身份与clone稳定。
保持现有产品前置、Unsupported公开结果和旧observer输出；不声称跨迭代归纳或旧见证退休。

本片最终FE lib为656通过、50项ci_only过滤，FE all-target strict Clippy、LLVM21
workspace all-target check及fmt/diff通过。docs898与Rust尺寸2466文件/35项旧债通过，
无新增例外。只读复审发现的control可改写缺口已以私有字段及只读getter封闭并复核。
CI-only故障测试仅编译，未本机执行；本地提交、不推送，未处理Preview。

### 静态逐根转移签名（2026-11-11，本地）

基于8ac8658e，独立事务保存真实drop消费的旧token与fact，构建header候选时按root
关联计划声明、producer定义、返回binding及消费事实，形成封闭的整组只读转移槽。
真实多根正反矩阵与逆序写入覆盖槽/root/producer/drop精确配对、clone稳定和错control拒绝。
这是首次静态body的有限签名，不是第二次激活、运行时trace或跨轮归纳完成；不发布公开能力。

本片定向10项通过；最终FE lib656通过、50项ci_only过滤；FE all-target strict Clippy、
LLVM21 workspace all-target check及fmt/diff通过。只读复审未发现错根、错binding或封装
阻断。CI-only故障用例只编译未本机执行，无远端CI或跨轮运行证明声明。

### header 状态形状闭合前置（2026-11-11，本地）

基于a541b580，在构建私有header候选前核真实body末资源辅助状态全空、活root集合精确覆盖
入口根且无重复；不读取残留owner ID冒充新代际。既有多根/逆序真实源码仍准入；仅CI执行
的模型负控覆盖缺失、重复、额外根以及借用binding、待替换残留。普通回执与公开能力不变，
该前置不单独授权runtime activation或跨轮归纳。

定向10项通过；最终FE lib656通过、51项ci_only过滤；FE all-target strict Clippy、
LLVM21 workspace all-target check及fmt/diff通过。只读复审无阻断；纯观察元数据仍按
boundary_values既有豁免，不把资源辅助状态闭合写成所有字段为空。未本机执行故障注入。

### header guard 与 body 身份（2026-11-11，本地）

基于70787815，将实际While condition/body及解析到的Boolean Name binding封入事务与
候选，核same-source和guard/root分离。真实矩阵补局部val/var guard，逐项回查AST/reference；
不假设所有Name都是参数，不扩大body合同或公开能力，不声称runtime activation已实现。

最终FE lib656通过、51项ci_only过滤；FE all-target strict Clippy、LLVM21 workspace
all-target check及fmt/diff通过，新增guard矩阵断言随普通lib执行。只读复审无阻断；
CI-only只编译未执行，本地保留、不推送。

### 实际 planner body 入口回执（2026-11-11，本地）

基于83cea6f2，将condition成功完成及body入口的真实hook接入独立token事务前置；记录
精确control/condition/body身份且匹配实际While，不能仅凭静态body形状开始事务。
CI-only增加遗漏入口hook故障场景，普通写回执仍在而token/header证据不得建立。这里的
入口仅指首次planner遍历，不等价于runtime激活或跨迭代证明；原公开能力不变。

最终FE lib656通过、51项ci_only过滤；FE all-target strict Clippy、LLVM21 workspace
all-target check及fmt/diff通过。复审发现的旧CI-only模型签名遗漏已修复并编译通过；模型
marker明确不是While条件，不替代真实源码入口验证。遗漏hook负控未本机运行，不推送。

### 已知 header incoming 的整组身份匹配（2026-11-11，本地）

基于49df9d06，新增sealed候选整组反向匹配，绑定control、root顺序与每个原catalog owner；
真实同源码两次分析验证不能互用normal快照，真实另一control拒绝；CI-only覆盖混组/交换/缺根。
它只匹配已知静态entry/normal，不表示新激活或运行时迭代可直接复用该token，公开能力不变。

最终FE lib656通过、51项ci_only过滤；FE all-target strict Clippy、LLVM21 workspace
all-target check及fmt/diff通过。只读复审未发现catalog错配或退化前驱混同；CI-only负控
仅编译未本机执行，未推送。

### 真实证据驱动的有限激活审计模型（2026-11-11，本地）

基于c3b46689，仅测试层建立独立作用域/出生轮次的模型实例，以真实sealed两组incoming
和消费槽审计0/1/2/8个正常前缀、正反root写入顺序、唯一退休和当前实例存活。
模型不生成UnitOwnerId，不接入运行时/公开产品，不以有限回放替代归纳证明。

最终FE lib657通过、51项ci_only过滤；FE all-target strict Clippy、LLVM21 workspace
all-target check及fmt/diff通过。复审建议的非空/等长及退休definition断言已补齐，模型
作用域采用独立身份且clone稳定。普通有限模型已本地执行；CI-only故障注入仍未本机执行。

### 正式 producer catalog 的私有产品接线（2026-11-11，本地）

基于8e196095，将入口完整原catalog及真实body定义catalog接入sealed boundary，复用
既有UnitOwnerProducerCatalogV1一致性与精确token查找；不将根子集改号，不推断历史定义
仍活。真实矩阵补前缀已退休String，验证live roots仍保留原非零index及完整三定义catalog。
该生产接线未新增公开header消费能力，也不将catalog验证等同于跨轮运输证明。

最终FE lib657通过、51项ci_only过滤；FE all-target strict Clippy、LLVM21 workspace
all-target check、fmt/diff、docs898及Rust尺寸2467文件/35项旧债通过。复审无阻断，
提示的历史定义非存活集合与Closure保守边界已明确；CI-only未执行，不推送。

### 公开多根 boundary-only 证据（2026-11-11，本地）

基于e236506a，新增只读UnitOwnerMultiRootBoundaryV1/slot及独立getter，最终发布唯一
完整root候选。产品包含真实sites、完整catalog、首次entry/normal及旧结果drop关联；
精确私有manifest锁定sites和各槽，analysis/callable/status/control及双向产品互斥共同门禁。
失败/有deferred清除；不授予header激活、loop lowering、跨轮归纳或旧见证退休资格。
真实正反矩阵验证公开准入，单独fixture验证完整三定义catalog与旧能力仍None；CI-only
覆盖partial/duplicate/交叉old/drop/错sites/foreign catalog/失败deferred及混合value产品。

复审指出的原关联与互斥缺口已修复并复核。manifest只锁定唯一已验证equation转换的输入，
不是独立CFG证明；该来源链必须保留。最终门禁结果记录于本片收尾。

最终FE lib658通过、52项ci_only过滤；FE all-target strict Clippy、LLVM21 workspace
all-target check、fmt/diff、docs898及Rust尺寸2468文件/35项旧债通过，无新增例外。
CI-only破坏/撤销负控只编译未本机执行；本地提交，未推送，未宣称远端CI通过。

### 参数化多根正常 body 运输（2026-11-11，本地）

基于282bec0b，生产形式校验器以Header(root)/Produced(definition)义务运行真实ordered
Replace步骤，拒绝重复根/producer/结果binding及不完整输出；旧Header恰消费一次且新
producer全保留在根。公开独立normal-body规则不引用固定P充当任意轮次输入，不是快照堆叠。
精确typed线性body与真实hooks提供参数化前提；完整header/exit/lowering仍未实现。

复审发现附接遗漏call/order，已用真实事务actual_steps独立清单锁定并精确附接，禁止覆盖；
CI-only补错call/重排附接及重复义务/缺步骤等负控。真实正反、逆序root及稀疏catalog矩阵
验证新规则准入和实际调用顺序；旧完整loop/value getter不新增资格。

最终FE lib658通过、53项ci_only过滤；FE all-target strict Clippy、LLVM21 workspace
all-target check及fmt/diff/docs898通过。复审确认call/order附接修复；CI-only负控仅编译
未本机执行，未推送、未声明远端CI或完整循环能力。

### 显式双臂返回的独立路径证明（2026-11-11）

本片复用既有 if-return + tail-return 结果，新增严格单个 `if (flag) { return view(source) }
else { return view(source) }`，两 borrowed 参数、同源同 callee、每臂一个原始 Return、
无局部 owner/额外语句/嵌套/循环。真实普通源码先在 rooted/memory × on/off 四组合
均复现 UnsupportedNode（borrow marker），原最终 verifier 未运行且 receipt 为零；
不把此输入的旧合法 frontend raw handoff 当成后端已支持证据。

前端公开 TailContinuation 与 BothArmsReturn 两种明确 topology。旧 tail continuation
状态证明保留，新片分别证明两臂真实 acquisition、live handoff、return 前后状态清空，
真实 then/else 都终止且无存活 continuation 后才接受 terminal join。每路径保存独立
action 区间，不将互斥路径拼成线性记录；catalog 仍 Partial。

后端原 `branch_declaration` 同样有隐含两语句/无 else 门，现只按上述两种精确形状
准入，维持原签名与 owned 来源要求。branch receipt 独立核原 AST 拓扑/return/value
身份；旧 TailContinuation 仍强制 false→join 的真实 Goto 和唯一 fold。BothArmsReturn
必须两个真实 BorrowReturn、空 join predecessors、无 goto/fold，最终无 join block。
普通新测试核真实三个 block、两个 condition edges、两条 BorrowCall/result/return
身份以及对应前端 action IDs；无新增 Begin/End/Drop，on/off SSA 与 LLVM 仍一致。

旧 explicit-else 负控移入独立正控，Grouped return、嵌套/其他未覆盖分支继续拒绝；
没有退休旧 tail/线性/借用生命周期见证。新增 frontend/codegen 各一项 CI-only
漏臂/错拓扑/虚假 continuation 或 fold 故障测试，只编译不在本机执行。
前端新双臂/旧tail/邻域 4 项普通测试通过；后端精确 branch 模块 4 项、auxiliary
旧新 3 项、旧七记录 receipt 1 项通过，真源码红转绿。双向独立代码复审未发现阻塞。
首次回归命令的 `ordinary_` 过滤过宽，误包含无关 native 测试，已主动停止该进程
子树并改为上述精确目标；那次中断运行不计验收，不声称完成 native 全套。
双 crate feature all-target strict Clippy、默认双 crate lib no-run（含新增 CI-only）通过；
workspace all-target check 与 fmt/diff/docs897 通过；尺寸 2468 手写 Rust 文件、
35 历史超限，无新增豁免。
证据在本机同目录 branch-else-* 日志。
此为有限两路径首片，不证明一般嵌套、多源、循环、异常或完整全路径借用体系。


### 同根不同 callee 的独立两路径证据（2026-11-11）

基于 `754f3925`，复用 TailContinuation / BothArmsReturn 的完整逐路径状态证明，
仅去除 frontend planner、Unit declaration 准入与 receipt admission 的跨臂 callee
相等门。每臂仍独立核直接调用、单 Borrow String 参数、Parameter(0) 返回来源、
真实同根输入、typed result_source 与具体 dispatch → FunctionId，所有拓扑清空、
join/fold 与实际 SSA 校验保持不变；旧验证器和见证不退休。

真实红：frontend 新 ordinary 输入具有两条实际不同 callee 的 raw handoff，却无
Complete region；后端两种拓扑 × rooted/memory × on/off 八组合均在借用声明
UnsupportedNode，原 verifier 调用零次且 receipt 为零。新正控逐条核真实
condition edge、BorrowCall 的不同 FunctionId、source/result/return 及 receipt
action 身份，观测 on/off SSA/LLVM 一致。前端 grouped operand 与额外参数
负控保持不准入；新 CI-only 交换/复制 callee（包括同时篡改 plan/receipt）必须
被实际 SSA 身份检查拒绝，该测试只编译不在本机执行。

验收：frontend 新两项、旧双臂一项与旧 cause 三项共 6 ordinary 通过；codegen
精确 branch 模块新两项加旧四项共 6 ordinary 通过。双向独立代码复审无阻塞。
双 crate feature all-target strict Clippy、默认双 crate lib no-run（含 CI-only）、
workspace all-target check、fmt/diff/docs897 全通过；尺寸 2470 手写 Rust 文件、
35 历史超限，无新增豁免。未运行 CI-only、native 全套或远端 CI。
验收日志保存在本机既有检查点目录 distinct-callee-*。
不推送，不证明一般嵌套、多源、循环或完整全路径借用体系。


### 有限三叶嵌套的真实拒绝基线（2026-11-11）

基于 `87b1052a`，先锁定外层 then 内嵌完整 if/else、外层 else 直接 return
的三叶源码：两个独立 Boolean 参数、三个不同 borrowed callee、同一 String 源。
`nested_three_leaf_probe_tests::ordinary_nested_three_leaf_remains_explicitly_unsupported`
实际核三条 raw transport 与 typed callee、Parameter(0)、result_source 及原 wrapper
来源；rooted/memory × on/off 四组合均在精确 wrapper borrow marker 返回
UnsupportedNode，原最终 verifier 未调用，receipt 为零。此普通边界测试通过，
codegen feature all-target strict Clippy 通过；不把 raw 三叶运输当作完整路径证明。

后续需独立父边链与内外终止凭据，现有固定双臂 region/receipt 不放宽。
本基线不改变生产准入、不推送；原始日志保留于既有检查点 nested-three-leaf-*。


### 有限三叶的实际父边 lineage（2026-11-11）

在三叶拒绝基线 `ba5232ad` 上增加独立 Partial 产品，不放宽已有两臂 region。
严格声明计划只接受外层 then 内嵌完整 if/else、外层 else 直接返回，三参数
String/Boolean/Boolean、三个直接 borrowed callee 与同根 Parameter(0) 来源。
计划仅提供预期；实际 condition 完成后，reached-arm hook 才产生带父句柄和
当时 action cursor 的 entry。三叶实际边链分别为 T/T、T/F、F。

独立 nested cause 携带真实 acquisition；实际旧 handoff 追加后才绑定 sidecar。
发布核同 catalog、真实相邻 Begin/handoff、source/argument/control 身份、父边
顺序与 cursor、末级 entry 与 acquisition 同点，且同时检查 sidecar 和底层真实
交易唯一性。旧 handoff.branch、旧 cause/transport PartialEq/cleanup 保持不变；
合流、return 和错误清除新 metadata，旧完整 region 不接受混入的 nested 状态。
公开 lineage 的 coverage 恒为 Partial，不证明内外完整 terminal join、所有路径
关闭或 cleanup，也不构成 codegen receipt 准入权限。

真实红为合法三叶 raw handoff 已存在而 lineage 0 != 3；新普通正控核
rooted/memory × on/off 的旧所有权与 owner observation 一致、三 callee、
catalog/action/control 身份与 2/2/1 条真实边。邻形负控保留缺 else、额外语句、
第三层及 else 侧嵌套；首次负控漏语句分隔符导致解析失败，已改合法换行再测，
未降低解析断言。新增 CI-only 13 类身份/父边/重复交付故障，仅编译不本机执行。
验收：前端新正负两项加旧双臂六项共 8 ordinary、后端旧 branch 六项及三叶
拒绝基线一项共 7 ordinary 通过。独立双向复审补强底层无 sidecar 重复交易
拒绝；新公共 API 缺文档的首轮 strict Clippy 失败已补齐文档并重新全绿。
最终双 crate feature all-target strict Clippy、默认双 crate lib no-run（包括
CI-only）、workspace all-target check、fmt/diff/docs897 全通过；尺寸2476
手写 Rust 文件、35历史超限，无新增豁免。CI-only、native 全套及远端 CI 未运行。
验证结果与证据保留在既有检查点 nested-lineage-*。
本片只本地提交，不推送；下一步仍需独立内外终止证书和后端真实 receipt。


### 有限三叶内外终止与全部叶关闭证书（2026-11-11）

基于 `93c7339e` 的真实 Partial lineage，新独立 nested region 不改变旧 lineage
含义或双臂状态机。先用同一真实三叶源码建立 region 0 != 1 的红测试，原
raw handoff 与 Partial 路径检查仍通过。只覆盖既定外层 then 单个完整 inner-if、
外层 else 直接return、同根三个直接borrowed call；不泛化额外语句或递归树。

新的独立穷尽 ValueState 边界检查要求精确 scope/owner 深度及其他义务为空：
实际 condition 前后、四个 reached-arm entry、三叶 return 前后的完整状态均
正面核验，pre-return 仅允许唯一匹配真实 pending，post-return 必须为空。
18阶段协议要求 inner 两叶各自闭合后才产生 inner_closed；实际 outer false
入口必须消费该凭据，随后第三叶关闭，outer join还须真实两臂终止且无live
continuation。empty states 或 AST 形状本身不构成关闭证明。

函数正常结束必须在原深度空状态 seal，随后 close cursor 恰为 start+6。发布
再次核未过滤的六条原始 Begin/Handoff、catalog身份、call/control/root、每叶
完整父边链与同candidate acquisition cause。缺失、重复、乱序事件保持sticky
invalid，只能给Partial且空paths。catalog仍Partial，仅该有限region可Complete。
旧lineage依然恒Partial，后端普通边界测试明确要求前端新region已Complete，
然后仍在原borrow marker拒绝UnsupportedNode、verifier/receipt为零。

独立双向复审未发现阻塞；新增CI-only覆盖13类关闭/身份故障及错误顺序sticky
invalid，只编译不在本机执行。前端新旧9项ordinary、后端旧branch6项加
新增Complete/Unsupported边界1项通过。双crate feature all-target strict Clippy、
默认lib no-run（含CI-only）、workspace all-target check、fmt/diff/docs897通过。
首轮Clippy的测试快照类型复杂度已改具名类型，不放宽lint。尺寸门禁发现父planner
1667→1674超既有1669额度；独立非作者逐diff确认仅7行接线、算法留子模块，
将已有上限精确锁1674，保留baseline/owner/拆分计划，不机械压行。
复跑尺寸门禁通过：2480手写Rust文件、35历史超限；未新增例外项。
原始日志保留既有检查点nested-closure-*。本片只本地提交，不推送，
CI-only、native全套和远端CI未执行；后端实际receipt仍为下一片。

### 有限三叶实际 codegen receipt（2026-11-11）

基于 `76ef1350` 的独立关闭证书，先以真实源码建立后端红测试：rooted/memory
与观察开关四组均在原 borrow marker 拒绝，原 verifier 和 receipt 都为零。
本片只开放外层 then 含完整内层 if/else、外层 else 直接返回的固定三叶树；
String source 与两个 Borrow Boolean 参数，各叶直接返回同 source 的单参
Borrow call，可用不同 callee；不开放 else-side 嵌套、额外语句、四叶或一般 CFG。

独立 Unit gate 从原始 typed/loan/transport 事实逐叶核声明、来源和 ABI；
观察关闭仍保持同一支持边界。独立 receipt admission 绑定同 catalog 的唯一
Complete region、三个 Partial lineage、六条真实 Begin/Handoff 与完整父边链。
旧单分支事实和验证器均不放宽。producer 在真实 split、return、terminate、
join 和 layout 路径记录事件；两层 join 均需零前驱并从最终 layout 消失，无 fold。
冻结实际五块、两个 Read、三个 BorrowCall/BorrowReturn，逐叶核入口 source
沿全部真实 parent edge 到叶 SSA 参数的对应，另核 inner 条件参数运输。
原最终 verifier 成功后才提交暂存 receipt；任何提前失败由 guard 原子撤销。

新正控及四叶/邻形负控共3项 ordinary 通过，包含四种模式、SSA/LLVM parity、
原 verifier 恰一次及仅观察开启产生 receipt。旧 branch 6项、aux 3项、binding
1项和 frontend 9项回归通过。新增 CI-only 身份/不完整计划/发布撤销变异只编译，
不在本机运行；其中 foreign FunctionId 变异已确认与原 caller 不同，避免无效扰动。
双向独立代码复审通过。后端 builder 1000→1002 仅两行初始化/终止事件接线，
经独立非作者审阅记录本片1002行，保留既有1025额度、owner和拆分计划；
不新增例外、不提高额度或 baseline。
本片不退休旧见证，不宣称一般逐路径完整；native 全套与远端 CI 未执行。
双 crate feature all-target strict Clippy、默认 lib no-run（含 CI-only 编译）、
workspace all-target check、fmt/diff、docs 897 与 Rust尺寸门禁全部通过；
尺寸统计2487手写文件、36个超1000行，历史债务仍显式报告。
日志保留既有检查点 nested-receipt-*；本片只本地提交，不推送。

### 三叶之后的旧见证退休审计（2026-11-11）

基于 `7008e82b` 逐消费者审计后，本轮不退休任何旧见证：Unit 的 borrow_result、
branch_declaration 与 nested_declaration 在 observer-off 仍依赖原 origin/transport/loan；
refine subset/range_borrow 和 build/range_borrow 的真实返回 lowering 覆盖范围更广，
仍读取旧 source/origin。forward、branch、nested receipt admission 也还要逐路交叉
核验 typed/raw 合同。新的三叶局部 Complete 不等于 callable/catalog 完整，更不能
证明全流 absence；projection、观察关闭等价及未支持邻形测试仍保留。
退休需要对应消费者完整输入范围的独立替代合同、缺失与不支持的明确区分，以及
关闭旧读取后的等价验证；不能仅凭新增 receipt 正控删除旧表或旧测试。

### 镜像三叶的有限路径扩展（2026-11-11）

退休审计后选择下一有限形状：外层 then 直接返回，else 中为完整内层 if/else。
先记录前端真实红：raw 三次运输已有，独立 region 却为0；后端真实源码四组
rooted/memory × on/off 均在原244..250 borrow标记 Unsupported，verifier及receipt为0。
显式 NestedThen/NestedElse 区分两棵固定树，镜像路径按真实访问顺序 T、F/T、F/F，
不重排原始动作、不给一般递归嵌套或多根授予完整性。日志为mirror-*-red。

镜像同样18阶段，但第一叶先真实return；外层false入场后才观察内层条件与两叶。
内层join出具关闭凭据，outer join恰消费一次，然后函数seal/close；原Then仍在
外层false入场消费其内层关闭凭据。entry、Partial lineage、Complete region均绑定
同一显式拓扑，return边界深度由实际路径长度决定，原始六条运输及catalog不变。
后端Unit gate仅接受两侧恰一侧嵌套且另一侧为直接return的固定二择一；admission
核拓扑、AST与路径身份。producer要求mirror第一叶已真实Return才允许inner split，
inner join要求全部三叶终止；最终SSA仍五块两Read三BorrowCall/Return，外层false
真实edge运输source和inner flag，各叶完整父边链必须与recipe/receipt一致。

前端4项、后端4项普通新旧三叶/邻界测试通过，含rooted/memory与观察开关的
真实SSA/LLVM等价；旧branch6、aux3、binding1及前端6项回归通过。
双向非作者复审通过，修复了审查发现的CI-only capture辅助函数命名遗漏，
未因测试问题放宽生产逻辑。新增错拓扑、错误false父边、错误flag、提前进入inner、
关闭消费缺失与重复join等CI-only变异只编译，不在本机执行；不退休旧见证。
strict 双crate feature all-target Clippy、默认lib no-run（包含CI-only编译）、
workspace all-target check、fmt/diff、docs897与尺寸门禁全部通过。尺寸2488手写文件、
36历史超限；本片不新增例外、不提高额度。日志mirror-*，只本地提交不推送，
native全套与远端CI未运行。

### 固定平衡四叶的前端关闭证明（2026-11-11）

基于 `c9247d3b`，本片先建立四参数 source/outer/left/right 的固定深度树：
外层两臂各含一个完整内层if/else，四叶直接返回同根borrowed call。
真实前端红在原始四次运输成立后独立region为0；codegen红在rooted/memory×on/off
四组合均保持333..339原borrow marker的Unsupported，verifier与receipt为0。
本片采用先前端证书、后实际receipt的分步边界，不能把前端Complete视作后端支持。
原三参数共享inner flag四叶、不平衡深层树及一般递归嵌套不在本片范围。

FixedNestedTree 显式保存 Three 的2 split/3 leaf或 BalancedFour 的3 split/4 leaf，
不填假叶、不以任意Vec长度授予资格。严格plan要求四参数及outer/left/right各自
原条件身份；split index与真实深度分离，两个inner都在深度1，叶资格比对完整父链。
entry、Partial lineage与Complete region均绑定BalancedBoth及同catalog身份。

独立25阶段协议在左innerjoin11生成左关闭凭据，outer false入场12消费一次；
右innerjoin21须左已闭合并消费，再生成右凭据；outerjoin22消费右，之后seal23、
close24。完成再次核全部8条真实Begin/Handoff、四路径、六个实际entry角色，
任一错序、重复、遗漏或空状态不满足完整性，失败仍Partial且空paths。
后端旧三叶admission/builder/emitted显式拒绝BalancedBoth，真实边界测试要求
前端已Complete同时仍在原借用标记Unsupported，原verifier/receipt均为零。
未注册的后端实际receipt正控保留检查点，供下一片实现，不能算本片已通过。

前端四叶正/负控2项、原三叶4项、codegen四叶精确拒绝/邻界2项、原三叶4项、
双臂6项及前端双臂6项均通过。双向独立复审确认实际关闭消费，另发现并修复
错拓扑/树存储组合后追加事件可能越界的问题，改用受检访问保持sticky invalid，
追加CI-only防御负控。CI-only基于真实producer快照检查两token缺失、错entry、
错路径/数量/游标、重复join等，只编译，不在本机执行。旧见证全部保留。
最后strict feature all-target Clippy、默认lib no-run（含CI-only）、workspace全目标、
fmt/diff/docs897及尺寸门禁均通过，2491手写Rust文件、36历史超限，无新额度。
默认编译首次暴露新观察测试漏feature注册，已按同类模块补齐并重跑通过；
不改生产合同。日志balanced-*，本片只本地提交，native全套与远端CI未执行。

### 固定平衡四叶实际 codegen receipt（2026-11-11）

基于 `86c6d088` 的前端独立Complete，恢复保存的后端真实正控，先确认四组合
仍在333..339原borrow标记Unsupported，原verifier与receipt为0。仅接受固定四参数
source/outer/left/right与三个对应条件，左右各两叶直接返回同一source；不接纳
共享inner flag三参数四叶、缺else、额外语句、更深树或多源。

独立Unit gate保持观察关闭也使用相同严格原typed/raw/ABI；receipt admission另核
同catalog Complete BalancedBoth、四条Partial lineage、8条动作及各callee来源合同。
容器按显式拓扑固定分配(3,4)，无占位叶，不以任意长度开放形状。
producer进入右inner之前必须已观察左两叶实际Return与left join；outer join须
两个真实inner join，均无live前驱，三join在最终layout全部消失且无fold。
冻结7个实际block、3个Read、4个BorrowCall/Return，左右flag分别从entry参数经
真实outer true/false边传递，四叶source沿TT/TF/FT/FF完整父链逐项核验。
原最终verifier及UnitPublication接线不变，成功后提交，任一前置失败撤销暂存。

实际正控和五邻界组共2项、原三叶4项普通测试已通过，含rooted/memory×on/off，
四个callee两两独立、路径动作ID不重复、Read/Call真实所属块和SSA/LLVM等价。
独立双向复审通过；CI-only追加跨子树父边、错flag、foreign身份、缺path/join及
过早right split故障，只编译不执行。旧共享flag四叶和更深树仍在原标记拒绝。
其余后端branch6/aux3/binding1及前端12项回归通过；strict feature all-target
Clippy、默认lib no-run（含CI-only）、workspace全目标、fmt/diff、docs897与尺寸
门禁全部通过。尺寸2491手写文件、36历史超限，无新增例外或额度；测试helper
多余借用在构建前修正，不放宽lint。日志balanced-receipt-*，本地提交不推送，
native全套、CI-only执行与远端CI未运行，未退休旧见证或声称一般全路径完整。

### 共享条件参数的独立路径绑定（2026-11-11）

`8f0b900f`后重新审计，旧见证仍不能退休：Unit观察关闭准入、receipt的原始
source/origin/transport交叉核验，以及更宽Name/Member/Call lowering域尚依赖旧事实。
本片选择平衡四叶共享inner参数，验证输入身份与控制身份可分离，而不是继续增加
叶数。仍保留三个独立split、四条路径和25阶段关闭协议，不因条件值相关剪枝。
固定共享模式入口[source,outer,inner]，split参数绑定明确为[1,2,2]；原独立模式
[source,outer,left,right]仍为[1,2,3]，不允许任意重复或错用outer参数。

真实红已确认：前端原四次运输成立但region为0；后端rooted/memory×on/off均在
308..314原borrow标记Unsupported，原verifier与receipt均0。原三参数共享四叶负例
随支持边界迁入真实正控，同时用错用outer、缺else等实际邻形继续固定拒绝边界。

显式Distinct/SharedInner布局绑定真实entry、plan、Partial lineage与Complete region。
SharedInner只用于三参数BalancedBoth；前端仍从每个原条件typed引用正面核
[outer,inner,inner]，左右split/condition/entry不去重，25阶段双关闭消费不变。
后端Plan将唯一entry_flags与逐split flags分开，完整valid_shape在producer与finish
重验明确映射，Distinct仍核全部输入唯一。实际SSA入口为3参数，左右各自经真实
父edge映射同一入口inner到不同局部LoanId，仍保留3个Read及全部四条路径。
原verifier后原子发布接线未变，旧见证未退休；三叶tail continuation留待独立后续。

共享新前端正控及distinct/三叶回归7项、后端共享实际receipt正控1项与原四叶2项、
三叶4项通过。两入口×on/off核真实7块、3Read、4Call/Return及左右独立局部LoanId；
旧后端branch6/aux3/binding1和前端branch6项也通过。双向独立复审通过，CI-only
新增错layout、重复入口、错误split映射、跨左右局部Loan/Read/condition与父边等
故障仅编译，不在本机执行；没有合并控制点或删除路径。
strict feature all-target Clippy、默认lib no-run（含CI-only）、workspace全目标、
fmt/diff、docs897及尺寸门禁全部通过：2492手写Rust文件、36历史超限，无新增
尺寸例外或额度。日志shared-inner-*；本地提交、不推送，native全套和远端CI未执行。

### 限定三叶真实尾续接证明（2026-11-11）

以`e31ad336`为基线，旧见证仍按前次逐消费者审计保留。本片只增加三参数Distinct
NestedThenTail：outer真臂为完整inner两返回，outer无else，紧邻第三个尾返回。
真实红中typed/raw三条运输成立但独立region为0；不将有活出口当作terminal join。

前端19阶段协议先核inner两叶关闭及真实implicit false entry，再要求唯一存活的
false状态、完整空资源边界和then已终止/else未终止。独立token绑定catalog Arc、
candidate、实际entry、cursor和完整原状态；消费inner关闭凭据后，普通merge仍
默认清除nested metadata，只有merge后真实空状态与精确深度通过才单次恢复。
尾叶pre/post必须发生于resume之后；六条原始动作、全部lineage及seal/close再核，
Complete region提供真实continuation入口与tail control，catalog/lineage仍Partial。
旧terminal join不放宽，非唯一状态、错token/entry/游标、重复恢复、污染资源均失败封闭。

后端明确拒绝新topology，实际rooted/memory×观察on/off均保留原borrow marker的
UnsupportedNode、最终verifier与receipt各0。将来需要真实false Goto/唯一join及
fold证据，不按最终同为五块就冒用终止三叶receipt。一般嵌套、多源、非邻接tail
与旧见证退休均不在本片范围。

首轮实现普通测试4绿1红，定位为topology_matches_tree遗漏新枚举；精确补入Three
对应后复测新tail、原三叶及四叶8项全绿，未放松continuation检查。额外邻形固定
extra statement、inner缺else和outer显式空else拒绝。独立生产因果审查通过。
最终定向验收前端14项、后端18项通过（包含新tail拒绝与旧branch/aux/binding、
三叶、四叶及共享inner实际receipt）。新增18类CI-only continuation故障仅编译，
没有本机执行；strict feature all-target Clippy、默认双crate lib no-run、workspace
全目标检查、fmt/diff、docs897与尺寸门禁通过：2493手写Rust文件、36历史超限，
无新增额度。日志nested-tail-*；本地提交、不推送，native全套与远端CI未运行。

### 限定三叶尾续接实际receipt（2026-11-11）

基于`6e291ecf`完成的前端continuation合同，仅扩展NestedThenTail严格三参数
Distinct形状。真实红核前端Complete已存在，但rooted/memory×观察on/off均在
244..250原borrow标记Unsupported，verifier与receipt各0。新实际正控四组合均
lowering成功、原verifier各1次，观察on各1 receipt、off为0，SSA/LLVM观察开关一致。

准入独立核公开continuation、outer-false入口和tail control，保留typed/raw合同；
producer要求inner两叶终止且inner join无前驱，outer join只有真实false前驱，
随后记录一次指向该join的Goto，尾叶原block必须为该join。真实fold hook只接受
该false→join一次；旧终止拓扑继续禁止fold。最终emitted按实际fold映射到保留
false块，原join必须已消失，按真实父edge映射source/flag，核5块2Read3Call/Return，
receipt同时保存fold及原plan身份。原最终verifier后的原子发布接线没有改变。

合法tail原拒绝测试迁为错outer flag负界，另固定extra-between、inner缺else及
explicit-empty-else三种真实邻形。独立审查发现continuation准入在路径长度核验前
直接索引edge，已改first()缺失即拒绝；修后普通回归无编译警告。旧见证保留，
一般控制流、任意嵌套与多源不在本片范围。
最终普通定向后端20项全部通过，涵盖tail正控/邻形、旧三叶/四叶/共享inner及
branch/aux/binding。新增3项CI-only故障测试覆盖身份替换、fold唯一性及失败发布
回滚，仅编译未执行；未将手动guard回滚冒充源程序触发的最终verifier失败。
strict feature all-target Clippy、默认双crate lib no-run、workspace全目标、fmt/diff、
docs897及尺寸门禁通过：2496手写Rust文件、36历史超限，无新增尺寸额度。
日志nested-tail-receipt-*；本地提交不推送，native全套与远端CI未运行。

### 限定三叶级联尾续接证明（2026-11-11）

基于`f2e308ad`，新增有限NestedElseCascadeTail：outer真臂直接返回，false臂内层
只有真臂返回，函数紧邻尾返回承接FF。固定源第一、3参数Distinct、T/FT/FF，
不是把既有terminal NestedElse或单续接NestedThenTail重新命名。真实红核三条
原typed/raw handoff成立但region为0。

独立20阶段协议核outer第一叶及inner第二叶，inner实际false entry后产生首token，
真实inner merge后恢复；outer只能消费与InnerResumed完整相等的FF存活状态，
再由第二token跨真实outer merge恢复。显式Empty→InnerPending→InnerResumed→
OuterPending→OuterResumed，不用inner_closed伪造继续路径；两个token均绑定
catalog Arc、candidate、split、entry链及cursor，跳级/重放/换层失败封闭。
尾叶执行深度回到函数body，provenance仍保留FF；最终6条原动作、父边、两个实际
false entry及seal/close独立核验，公开证据以inner/outer两步明确顺序。
旧单续接及终止协议未放宽，非cascade必须保持Empty；旧见证继续保留。

后端本片明确Unsupported，实际两入口×on/off保持原borrow marker拒绝、verifier
与receipt为0。后续需独立记录两Goto与两次真实fold；第二次fold前驱已由第一次
折叠替换，不能复用单fold证据或仅凭最终同为5块声称完成。
最终前端15项、后端21项普通定向回归通过，涵盖新级联正控/邻界、旧终止树与
单续接实际receipt及branch/aux/binding。新增3组CI-only故障仅编译未执行。
独立因果复审、strict feature all-target Clippy、默认双crate lib no-run、workspace
全目标、fmt/diff、docs897及尺寸门禁通过：2499手写Rust文件、36历史超限，
无新增额度。日志nested-cascade-*；本地提交不推送，native全套与远端CI未运行。

### 限定三叶级联双fold实际receipt（2026-11-11）

基于`e67a32dd`的有限NestedElseCascadeTail前端双恢复证书，后端实际receipt
严格限定T/FT/FF及3参数Distinct。真实红四组合在244..250原borrow标记
Unsupported且verifier/receipt各0；真实绿rooted/memory×on/off全部成功，
原verifier各1次，观察on各1 receipt、off为0，SSA/LLVM观察开关一致。

独立核inner/outer公开continuation与实际FF路径。producer先记录唯一innerFalse
前驱的innerJoin及Goto(innerFalse→innerJoin)，再记录唯一innerJoin前驱的
outerJoin及Goto(innerJoin→outerJoin)，两条原Goto严格各一次且顺序不可颠倒。
真实fold hook则记录(innerFalse,innerJoin)之后(innerFalse,outerJoin)，第二次
前驱已经替换，不与第二条原Goto身份混淆。tail原block为outerJoin；最终两join
均须从block映射消失，按两条真实fold映射回保留innerFalse，逐实际父edge核
source/flag、callee/Read和返回身份，仍为5块2Read3Call/Return。Receipt独立
保存双fold，旧单fold与全终止拒fold合同保持；原verifier后原子发布接线未改。

合法cascade原拒绝测试迁为错outer flag负界；另固定五种真实邻形拒绝。独立
生产因果审查通过，CI-only新增故障覆盖漏/重/乱序Goto、第二fold错误前驱、
原join/最终映射替换与失败发布回滚。旧见证保留，不表示任意递归、多源或一般
控制流已经完成。
最终普通后端23项定向回归全部通过，覆盖新级联正控/邻界、旧三叶/四叶/共享inner、
单续接及branch/aux/binding。4组CI-only故障仅编译未执行，手动guard回滚不当作
源程序导致的最终verifier失败。strict feature all-target Clippy、默认双crate lib
no-run、workspace全目标、fmt/diff、docs897及尺寸门禁通过：2502手写Rust文件、
36历史超限，无新增额度。日志nested-cascade-receipt-*；本地提交不推送，native
全套与远端CI未运行。

### 固定路径结构去重与退出协议隔离（2026-11-11）

基于`dd90b438`，先审计旧见证：观察关闭路径仍依赖typed/raw/declaration合同，
有限路径证书不能替代更广whole-function或carrier合同，无同合同退休证据，均保留。
本片不新增拓扑、公开API或Complete范围，而将声明结构与实际退出证明分离。

前端私有表集中TT/TF/F、T/FT/FF及TT/TF/FT/FF三种结构的leaf、split父边/深度
及历史entry role；越界返回None，不再把任意非零split都当作深度1。进入实际entry
同时核split索引对应的声明父边，仍保留真实callable/catalog/layout/record/cursor
与最终具体split身份复核。后端独立私有RouteContract集中counts/parents/routes
及明确flag布局资格，删除admission重复路径表与Balanced右子树特判；逐实际边
仍核split/condition/side，SharedInner资格明确只属于Balanced，不按数量猜测。

两阶段各自以literal golden核表，不共享实现作为彼此的正确性oracle。表不包含
已发生事件，也不生成证据；terminal、单续接、级联的原stage/Goto/fold/token
协议不合并不放宽，原verifier后原子发布未改。本片是行为保持去重：旧合法路径
基线与新结构/越界测试均通过，不将此冒充新功能或源程序误编译的真实红绿。
最终前端17项（含2项结构golden）、后端24项（含1项合同golden）普通定向验证通过，
现有全部实际receipt和邻界均保留。新增实际entry错误父边CI-only测试仅编译，\
未执行。双向独立复审、strict feature all-target Clippy、默认双crate lib no-run、
workspace全目标、fmt/diff、docs897及尺寸门禁通过：2505手写Rust文件、36历史超限，
无新增额度。日志nested-route-*；本地提交不推送，native全套与远端CI未运行。

### 动态有界完整终止树合同（2026-11-11）

以`2beb5592`为基线推进声明驱动的有限树，不再为新形状增加topology枚举。
本片预算为split深度最多3、split最多7、叶最多8；函数只有一棵完整if/else树，
每臂恰一子树或直接同根borrowed declaration return。source第一，每个split
独立Boolean参数按声明遍历顺序绑定。缺else、额外语句、tail continuation、多根、
循环、重复/错误flag及超预算输入拒证，不截断后假装完整。

计划用动态节点/路径表达预期结构，但不产生完成证据。实际condition/entry、
叶Begin/Handoff与return清理、两子树终止后的join关闭token、根seal/close须独立
发生且身份/父边/cursor连续，才允许局部Complete；catalog仍Partial。固定合同
已覆盖的输入复用已有observer，首片不重复发布或退休旧消费者。前后端仍独立，
本批先完成FE证书；CG对新树维持原borrow位置Unsupported，后续实际receipt须
证明每层flag/source运输与真实terminal join，不能继承固定树验收结论。
实现记录：初始真实typed/raw已成立但bounded region为0；链状四叶、混合五叶及
完整八叶实际正控均已获得局部Complete。实际observer采用动态DFS状态、两子树
关闭token的身份和连续记录区间，父join逐一消费对应token后才关闭；根seal/close
与全部唯一leaf再次核验。原fixed覆盖的6类输入不重复发布，新5类合法邻界拒证。

声明计划独立核预算、typed/raw同根返回及独立flag；发布器另核有界图结构、
实际entries/父链与token，父链回放最多3步，索引与cursor算术失败关闭。全records
中相同call/argument的Begin与相同call/Return的Handoff各须唯一，区间外无sidecar
重复也拒绝。独立审查提出并已修复空actual访问、父链循环及cursor溢出风险。
这仍非多根、循环、continuation或任意深度证明，旧见证保留。

本片保留明确验收阻塞：未修改的尺寸门禁报告drop_planner.rs为1684行，超过原
1674额度10行，错误已实际复现；增长仅2模块及begin/finish/close接线，算法在
独立模块。调整持久化额度的动作未获授权，未执行；不压行、不机械拆分、不改策略
来规避。compilation_unit.rs的1113行仍在既有1156额度内。待用户明确审批必要的
精确例外，本片仅保存开发检查点，不宣称整体完成或可合并。

验证记录：本片普通FE17项、CG25项通过；新正控先真红后绿，覆盖4/5/8叶、
rooted/memory及observer开关。Clippy（含loan-observation-test-support、所有target、
-D warnings）、默认lib no-run与workspace all-targets check均通过；CI-only只编译，
未在本地执行。fmt、diff及文档897项检查通过。尺寸检查仍失败（2512个手写Rust文件，
36个历史超1000行文件），唯一新增阻塞为上述1684>1674，不改变原额度。
日志为本机bounded-tree-*系列；未执行native全套或远端CI，未推送。

#### 动态有界树实际 SSA receipt（开发中）

基于79d9b36d，下一片保持同根、不同Boolean参数、全叶直接返回及3层/8叶边界。
CG独立重核已验证声明与路径合同，实际producer记录split/arm/join、每叶call/return，
emitted阶段逐边核source与活跃后代flag参数运输、实际指令和callee身份；不使用固定
Topology冒充动态树。只有原final verifier成功后统一发布receipt，失败原子撤回。
observer off仍走原typed/raw合同，旧见证保留。现有尺寸失败不变，不调整尺寸策略。

新正控已真红转绿：原Unsupported marker失败后，4/5/8叶实际SSA成功且原verifier
每次仅运行一次；字面DFS路由独立核每层flag与source逐边运输、callee/return/ID，
rooted/memory及observer开关的SSA和LLVM文本一致。新receipt独立于旧固定topology。
将三种布局观察的生命周期分发统一到既有LoanReceipts::layout，branch→nested→bounded
顺序与错误传播不变；不移动布局算法、不修改尺寸政策。这是同new/terminator/finish
一致的协议生命周期封装，而非压行或机械拆文件。

本片验证：普通CG26项回归通过（新动态正控、新5类邻界、旧nested/branch/
balanced/shared-inner/auxiliary及binding），最终正控复验通过。Clippy -D warnings
通过；默认FE/CG lib no-run、workspace all-targets check、fmt/diff及文档897项均通过。
新增CI-only以真实生产快照覆盖16类receipt与7类plan故障及发布回滚，仅编译未执行；
两轮独立审查修复并复核参数唯一性与向量长度防越界，未改变原verifier。
尺寸复测2520个手写Rust文件、36个历史超1000行文件，唯一失败仍为前序
FE drop_planner.rs 1684>1674；CG本片无新增尺寸错误，策略未改。
原始证据为本机bounded-receipt-*日志。未跑native全套/远端CI，仅本地提交，不推送。
该实现不涵盖多根、循环、continuation或无界树；尺寸阻塞未解除，不宣称可合并。


### 有界单继续出口树：独立前端证明切片（实施中）

在既有动态终止树预算内，支持恰好一个隐式 else 继续出口，其余叶均直接返回同源
借用，树后恰有一个公共尾 return。split 深度不超过3、split不超过7、含继续出口的
叶不超过8；源第一、独立 Boolean flags 按 DFS 参数顺序绑定。继续出口可位于任意
祖先侧，不增加固定拓扑枚举；拒绝多个存活出口、额外语句、循环、共享flag及任意CFG。

声明结构不代替实际事件。独立 continuation token 必须逐层绑定真实entry、candidate、
source catalog、cursor和唯一存活状态；join消费已终止兄弟与唯一继续子证据，merge后
精确核对原存活状态再签发父token。根resume仅一次，尾return在实际深度0执行，但来源
保留hole祖先链。旧Closed/全终止join不放宽，新公开产品与终止树region隔离；旧固定
协议优先，禁止同一函数重复发布。首片仅前端证明，CG仍精确Unsupported，不预告receipt。

验收先真实普通红，再要求rooted/memory与观察开关下旧事实一致、实际路径/尾续接身份
闭合；故障测试仅编译，覆盖跨候选token、跳级/重复resume、先外后内、污染存活状态。
尺寸政策不改，drop_planner.rs既有1684>1674阻塞继续保留，不能宣称全验收。

旧见证审计：observer-off仍依赖typed/raw，RangeView、carrier alias和一般CFG超出有界
合同；fixed仍承担shared flag及既有continuation，原verifier仍是发布前提。本片不退休
任何旧见证，也不以新的Complete局部证书代替whole-flow absence证明。

实际验证：真实typed/raw成功而新region为空的红测试已转绿；3种单hole路径与6类邻界/
fixed优先级案例均成立，FE普通19项、CG普通28项回归通过。后端新3形状仍在原borrow
marker精确Unsupported且verifier/receipt为0；旧完整终止树全部25种有界拓扑的独立
枚举也通过。最终新FE两项再验通过；Clippy -D warnings、默认FE/CG lib no-run、
workspace all-targets check通过。新CI-only真实快照19类final故障及7类resume故障仅
编译，未执行；两轮独立审查核对真实entry、原唯一survivor保留、逐级token消费及
实际depth0尾返回，不从计划重建存活状态，不改变旧终止协议。

尺寸复测2526手写Rust文件、36个历史超1000行文件，唯一错误仍为前序
1684>1674；drop_planner.rs本片完全未改。compilation_unit.rs新公开类型导出+1行，
独立逐diff评审确认职责必要，当前1116仍低于既有1156额度；策略未改。新候选958行。
日志为本机single-continuation-*系列；仅本地保存，未推送、未跑native全套/远端CI。

#### 有界单继续出口树的实际后端收据（实施中）

基于d96306aa，本片消费独立continuation证书而不改变旧terminal region含义；保持
唯一省略else hole、不同Boolean参数、同根、3层/8叶和根后唯一tail return。
准入独立核原typed/raw，producer须记录每层真实entry、唯一存活前驱join、Goto及
实际fold顺序；原join到最终块的身份必须仍可追溯。emitted逐edge核source/后代flags、
每叶callee与tail原块，原final verifier成功后原子发布。不能从最终CFG反推缺失的
Goto/fold事件。先真实Unsupported红，再证明实际receipt；旧见证和尺寸政策保持。

实际Unsupported红已转绿，5个字面路由正控涵盖根hole、深度2及三类深度3hole；
逐层source/后代flag、callee/return/ID、真实Goto/fold次数1/2/3和tail映射均核验，
rooted/memory、observer on/off的SSA与LLVM文本一致。旧合法根hole邻界迁至正控，
新邻界保留显式空else以及双hole、额外语句、重复/错序flag和深4拒绝。

另用独立递归枚举深度≤3完整树的所有false叶位置，共65种单hole程序，全部实际
lowering与原verifier通过、开关产物一致；原25种完整终止树也继续通过。producer保留
原内→外Goto链，fold按实际hook顺序重放“当前相邻组件”，允许先折外join，不将声明
顺序冒充实际fold顺序；完整折叠后root原tail仍映射到原hole保留块。独立审查补强
hole最后一边必须为false，旧terminal和fixed协议均不改。

本片CG普通30项回归通过；Clippy -D warnings、默认FE/CG lib no-run、workspace
all-targets check、fmt/diff及文档897项均通过。CI-only真实快照的身份、原Goto/join、
漏/重/跨链fold、tail原块与canonical映射及发布回滚测试仅编译，未执行。
尺寸复测2534手写Rust文件、36个历史超1000行文件，唯一失败仍为前序FE
1684>1674；本片不改FE、尺寸策略未动，CG无新增尺寸超额。独立审阅确认build新增
两处计划与terminate接线后1008仍在原1025额度内，emit986未越1000。
日志为本机single-receipt-*系列；未执行native全套或远端CI，仅本地提交，不推送。
旧见证保留；多继续出口、显式空分支、多根、循环与无界CFG仍不在此合同内。

### 有界多继续出口：事务与控制路径分离研究（2026-11-11）

基线dcca3994，仅研究与测试，不开放生产准入，不扩单继续出口证书，不退休旧见证。
多hole有实际价值，但不是把single协议的一个survivor改为多个：

```kotlin
if (outer) {
    if (left) { return a(source) }
} else {
    if (right) { return b(source) }
}
return tail(source)
```

这里是TT、TF、FT、FF四条控制路径，却只有a、b、tail三组真实借用调用交易。
TF与FF共用同一个tail acquisition/handoff；不能复制tail记录来满足“一路径一交易”，
不能把单hole的线性remaining_chain用于多前驱汇合。

独立生成器枚举深度≤3的26种完整二叉树（含纯叶），对每棵树的k个false叶选取至少
两个改成省略else的hole。false叶数量分布为k=0/1/2/3/4时各1/3/8/10/4棵，
总计Σ(2^k−1−k)=92个真实程序。逐一原typed/raw成功、observer开关旧事实一致，
实际交易数=结构叶数−hole数+1，tail始终恰好一次Begin和一次Handoff；现fixed nested、
bounded terminal、single continuation均不产生对应资格，CG在原borrow marker拒绝，
final verifier和single receipt均为0。普通研究测试已通过，不是多出口实现完成。

下一实现建议分两片，仍保持同根String、独立DFS Boolean flags、深度≤3/结构叶≤8、
唯一公共tail与无额外语句，不引入固定形状枚举：

1. FE独立多来源汇合证书：继续token携带实际hole身份集合；每个真实join分别消费
   两侧closed/live证据，live集合必须互斥、父集合是精确并集，绑定当次entry/cursor/
   source与merge后状态。根必须覆盖全部实际hole，一次公共tail交易与多条到达路径
   在产品中分离表达。先保留CG Unsupported，不放宽single独有survivor规则。
2. CG多前驱receipt：保留各条真实Goto与join的精确前驱集合，仅折单前驱链。
   多前驱join不得fold消失；同根source需要在两条真实入边各自核block parameter
   位置/类型/LoanId及源链，tail的单个真实call/result/return可由多条控制路径引用。
   保持独立准入、observer-off等价及原verifier后原子发布。

必要故障边界包括丢/重/交叠hole集合、跨候选token、漏survivor/前驱、伪造并集、
错source入边、跨多前驱join折叠、复制tail交易和失败发布。现有旧见证仍覆盖observer-off、
一般生命周期、carrier/alias与fixed专有合同，无同合同完整替代，故本片不退休。
前序尺寸1684>1674仍未解除，策略不改；研究结论不代表可合并或一般CFG完成。

研究复核：独立审阅确认92项枚举无漏项或重复；额外用typed callee声明身份核实
raw末项确为生成的公共tail，而不是仅按顺序自证。最终研究普通测试与既有6类
single邻界普通测试通过，Clippy -D warnings、fmt/diff及文档897项通过。
本片仅新增测试与研究记录，未改生产、原verifier、任何尺寸策略或旧见证；日志为
multi-hole-*。未跑native全套/远端CI，仅本地提交。

#### 多来源集合的独立前端证书（实施中）

基于a64b10cb的92种真实多hole研究输入，本片只实施独立前端证书，不开放后端
多前驱准入。保持深度≤3、结构叶≤8、同根String、DFS独立Boolean参数、至少两个
隐式false hole及根后唯一公共tail；不扩single独有survivor协议或旧terminal证书。

证明单位分离为物理交易与控制来源：每个终止叶保留自身真实Begin/Handoff，公共tail
只允许一个真实交易；多个hole的到达路径引用它而不复制交易。实际分支入口产生来源，
join消费各侧closed/live证据，两个live集合必须互斥，输出必须恰等于输入集合并集。
逐次merge后的状态、candidate身份、cursor、source与真实entry须共同匹配；根必须
覆盖全部实际hole，遗漏、重复、交叠、跨候选或伪造集合均失败封闭。

后端仍在原borrow marker返回Unsupported，不运行最终verifier、不发布新receipt；
observer-off与原typed/raw事实不变。局部Complete不证明任意CFG、循环、多根、callee
内部或权限恢复；旧见证全部保留，既有尺寸1684>1674阻塞及策略不作调整。

实际验收：先在two/rooted原typed/raw成功时观察到新区域0!=1的真实红，现已转绿。
独立枚举92种多hole树，rooted/memory与observer on/off全部核对；新产品的真实入口
路径、每个join两侧集合与精确并集由测试独立树生成器比对，terminal与唯一tail的
动作ID均绑定原raw交易和typed callee。后端仍原borrow marker Unsupported，原final
verifier及single/bounded receipt均为0。另有7类普通邻界与旧协议优先级校验。

FE普通21项、CG普通31项（含92形状联合测试）通过；Clippy -D warnings、workspace
all-targets check、默认FE/CG lib no-run、fmt/diff与文档897项通过。新CI-only真实快照
19类final损坏及9类resume损坏只编译，未执行。两轮独立审阅核对实际then/else顺序、
merge前完整输入保存、原pending精确匹配和tail单交易；未从计划重建live来源。

尺寸复测2540手写Rust文件、36个历史超1000文件，唯一失败仍1684>1674；本片
未修改drop_planner.rs或尺寸策略。新candidate774、独立final334、plan552、hooks274
均低于1000。compilation_unit.rs仅公开新产品导出+2至1118，仍在既有1156额度内。
日志为本机multi-proof-*；没有native全套或远端CI，本片仅本地提交、未推送。

#### 多继续出口的真实后端收据（实施中）

基于3644f9f5的独立前端集合证书，后端以独立准入、实际producer及emitted复核消费，
不放宽single线性链协议。保留有多个真实前驱的join与唯一公共tail；逐入边核同根
source的真实block parameter位置、类型和LoanId。多条控制路径只引用同一个实际tail
call/result/return，不能复制交易或把最终CFG反推为未观察到的Goto事件。

每次真实Goto保留原from/to身份与来源集合；仅允许原优化器实际执行的单前驱fold，
并核fold前后的组件邻接与精确前驱数。多前驱join不折叠；终止叶、每个来源及公共tail
必须全部覆盖。保持原final verifier成功后原子发布、observer-off等价、旧见证及原
深度/叶数/单根/独立flags限额。先真实Unsupported红后验证receipt；尺寸门禁不调整。

实际红绿：FE完整证书先通过，首个rooted/on输入在原borrow marker精确Unsupported，
随后独立multi准入与真实receipt使全部92种形状通过。rooted/memory与observer on/off
均核独立字面路线的Read/flag/source逐边运输、所有hole到同一tail的实际call/result、
保留Goto目标至少两个前驱、原Goto/fold数和receipt身份；SSA与LLVM文本开关一致。
另明确命中多前驱join作为from吸收单前驱后继的实际fold，验证保留原多入边汇合身份。
旧合法two-hole负例已迁入正控，邻界改为额外语句并保留其余5类拒绝。

本片CG普通31项通过，含上述92形状；Clippy -D warnings、workspace all-targets、
默认FE/CG lib no-run、fmt/diff及文档897项通过。新增5项CI-only真实receipt/producer
损坏与发布回滚测试仅编译，未执行。独立非作者复审准入、producer、emitted映射与
publication顺序通过；原final verifier仍是发布前提，未退休旧见证。

尺寸复测2547手写Rust文件、36个历史超1000文件，唯一失败仍为前序FE1684>1674；
本片FE与尺寸策略均未改变。独立逐diff评审build.rs仅Plans创建与terminate两行接线，
1008→1010仍在原1025额度内；emit987未越1000，新emitted843。日志为本机
multi-receipt-*；未跑native全套或远端CI，仅本地提交、未推送。

### 多根辅助与循环借用的下一片研究（2026-11-11）

基线3ffded51，先查合法语义与已有源码证据，不直接扩大有界树或固定shape。
Guide普通借用结果必须声明唯一from，另一根不能成为条件返回来源。因此“多根”下
一个合法目标是唯一source加独立auxiliary owner，而非from来源并集。

现有distinct-roots辅助fixture已通过typed/ownership，raw handoff保存aux调用返回
结束与source延续，但独立同根aux关系/region没有覆盖它。下一最小实现宜先新增
独立双根辅助退休关系：核真实两次acquisition、不同root身份、typed Parameter(0/1)
映射、实际call-return aux End与source仍live；不把同根协议的相等判断简单改成不等。
随后才补从两个owner建立到result/source结束及各自Drop的完整区域和后端receipt。
辅助先退休不等于aux owner已Drop；两个owner释放顺序必须来自真实cleanup，不能猜测。

循环内borrow val目前在两套ownership checker主动L0164拒绝，即使body含立即break。
因此最小循环片不是放宽后端receipt gate，而是先建立动态迭代实例与每轮无carry的
前端闭合合同：正常backedge、continue、break/return分别关闭当轮result/source，
并证明下一轮不会复用前轮token；body-local owner还须核每轮Drop。静态一次源码
Begin/End不能冒充任意迭代次数证明。此项只研究，不调整当前语言准入或诊断。

旧见证仍承担observer-off、独立辅助根与循环尚未认证生命周期；本轮无可安全退休
对象。保持旧尺寸1684>1674及既有策略，研究不代表一般多根/循环能力已完成。

研究实证已完成：独立辅助根在source参数0/1、rooted/memory与on/off均可通过既有
CG及原verifier；不是新的lowering能力。普通测试核两RootPlace确实不同，实际aux
End早于consumer，随后result End、source End及各owner Drop；SSA/LLVM开关一致，
窄同根关系/region/receipt保持空。该既有能力缺的是新的独立双根证明，而非执行支持。

循环普通矩阵覆盖正常回边、continue、立即break、body-local owner四类源码，双入口
与观察开关均typed合法而ownership精确L0164指向borrow，旧事实/诊断一致、无binding
transport，不伪装成可交后端的有效输入。两项新增普通研究测试真实通过，未运行任何
CI-only或native全套/远端CI；日志为distinct-root-research与loop-borrow-research。

本片Clippy -D warnings、fmt/diff及文档897项通过；仅新增测试与研究记录，原尺寸
阻塞不变且policy无修改。未扩大Complete，不退休见证，仅本地提交、不推送。

#### 独立双根辅助退休关系（实施中）

基于8b4ec677，补唯一source与独立auxiliary root的实际call-return关系，不放宽
旧同根协议，不改变既有lowering。真实两次acquisition与同次handoff携带身份，typed
source参数0/1映射和实际owner存活状态共同认证aux退休而source延续；仅有不同symbol
或两个AST实参不够。不能从最终facts重建已经丢失的pending token或owner身份。

关系与完整区域分离：本片不将call-return aux退休声称为owner Drop，也不宣称整个
双根生命周期Complete。实际consumer、result End、source End和两个owner Drop顺序
由原样SSA普通回归继续核对；新的区域/receipt须另有完整边界证明。旧同根资格、旧
见证与observer-off产物不变，尺寸策略不改，先真实红再绿，仅本地提交。

本片实际时序要求纳入ASAP：aux loan退休后、binding commit之前，aux owner可已被
真实CallReturn Drop；不能强求aux继续存活，也不能只放宽动作相邻性。独立pending
窗口在旧owner观察已Unsupported时仍旁听真实owner_event，保留从实际OwnedValue取得
的OwnerId/callable/catalog锚，不重开旧目录。commit必须source原实例仍live；aux要么
原实例仍live且窗口无事件，要么只有精确匹配的aux owner Drop及raw cleanup记录。
独立final再次核事件完整窗口与选定Drop一致，pending仅一次消费；Drop可由新关系
定位，但此局部关系仍不证明后续source或整区域生命周期。

实际红绿：source-index0/rooted先在typed/raw已成功时得到独立关系0!=1；定位到旧
borrow入口故意清空owner观察目录，修正为实际OwnedValue锚与独立窄事件窗口，不改变
旧Unsupported或目录清空。最终source0/1×aux ASAP/后续使用×双入口×on/off共16组合
通过；关系绑定两Begin/handoff、两OwnerId及可选实际aux Drop，原SSA结束/Drop顺序和
开关文本一致。旧同根关系/region/receipt仍按原合同，无新双根完整region或receipt。

FE普通8项、CG普通32项通过；Clippy -D warnings、workspace all-targets、默认FE/CG
lib no-run、fmt/diff及文档897项通过。新CI-only身份、窗口事件增删换、提前结束、
source变更与Drop错配故障仅编译，未执行。独立复审要求final再核完整owner事件窗口，
已落实精确空窗口或唯一aux Drop匹配；未用选择性事件过滤掩盖source Drop/Move。

尺寸复测2552手写Rust文件、36个历史超1000文件，唯一失败仍前序1684>1674。
drop_planner.rs及尺寸策略均未改；compilation_unit.rs仅公开关系导出+2至1120，经
独立职责审查仍低于原1156额度。日志distinct-aux-*，未跑native全套/远端CI；
旧见证全部保留，仅本地提交、不推送。

#### 双根辅助绑定的独立完整区域（实施中）

基于84865d0c的局部退休关系，本片先补独立完整区域，不把关系自动升级为全局能力。
两个真实owner建立、source/aux acquisition、aux调用返回退休及可选ASAP Drop、binding
handoff、consumer、result End、source End与两owner最终Drop均须来自实际producer，
完整未过滤区间与真实入口/出口义务共同核验。source参数0/1映射继续保留，aux保活
的额外使用必须有独立覆盖，不能当作无关动作过滤掉以获得Complete。

旧同根region、旧owner Unsupported/目录清空及原清理顺序不变。新区域先与后端
receipt分阶段验收：普通旧lowering可成功不等于新证书已有生产consumer；真实receipt
须另核同轮SSA身份、两root建立、实际每次End/Drop与原verifier后原子发布。不得凭最终
空表证明从未遗漏义务，不退休旧见证，不改变尺寸门禁，仅本地提交。

首片明确限定ASAP四语句：两个literal owner声明、唯一borrow initializer、唯一
consume(item)，支持source位置0/1。区域入口位于binding前，承接已形成的两个真实
OwnedValue身份和义务，不声称在区域内证明owner创建。独立完整9动作依次含两Begin、
aux ASAP Drop、handoff、consumer Begin、实际use-return、binding End、source退休与
source Drop；两owner事件窗口必须精确auxDrop→sourceDrop。source退休ActionHandle
经独立cause传到实际removed OwnedValue，再核快照后关联Drop，不能由最终空表回填。
aux后续另有consume(aux)的保活情况仅保留局部关系，不获得本片Complete区域。

真实红绿：ASAP在原partial关系和raw事实成功后新region0!=1，现16组合全部通过；
ASAP为Complete且9条原记录完整，保活无region而独立退休关系仍存在。FE普通9项、
CG普通32项回归通过；Clippy -D warnings、workspace all-targets、默认FE/CG lib
no-run、fmt/diff及文档897项通过。7组新CI-only缺失/错序/重复/外来身份、入口出口、
source第二参数、两owner事件与Drop故障仅编译，未执行。独立review确认真实移除
owner因果、完整义务入口出口及旧协议隔离，不从结束空表或最终记录补造证据。

尺寸复测2558手写Rust文件、36个历史超1000文件，唯一失败仍为前序1684>1674。
超额drop_planner.rs与尺寸策略均未改；compilation_unit.rs仅模块注册/导出+3至1123，
经独立职责review仍低于原1156额度。日志distinct-region-*；本片新region尚无新CG
receipt，旧CG通过不等于迁移完成，旧见证保留。未跑native全套/远端CI，只本地提交。

#### 双根ASAP区域的独立真实后端收据（实施中）

基于c199f0db，独立consumer只接受双根ASAP四语句Complete区域，保留source参数0/1，
不扩展aux后续使用或旧同根receipt。真实producer需核两owner建立与RootPlace、source
及aux贷款、borrow call、aux End与其ASAP Drop、结果consumer、result/source End和
source Drop；角色身份分别追踪，不能仅凭指令种类和相对位置声称来源相同。

入参顺序、callee返回来源索引、实际block/slot/ValueId/LoanId必须与同轮typed/source
plan及前端九动作对应。最终原verifier成功后才原子发布，任意缺失/重放/跨owner或
Drop错配撤销本批receipt。旧执行/observer-off产物保持，不退休旧见证，不改尺寸策略，
先真实receipt缺失红再绿；仅本地提交，不推送。

实际红绿：原FE Complete9与既有CG成功后，新receipt0!=1的真实红已转绿；ASAP
source0/1×两入口×on/off核全部9个前端action ID、实际11连续SSA指令、7语义实体、
aux Drop角色6与source Drop角色10。aux后续使用仍无本region/receipt，原SSA/LLVM
开关一致。role0同时冻结两个已有owner，避免两RootPlace之间替换未访问owner而漏验。
独立审阅确认两raw Drop索引/根/slot授权、实际阶段消费和verifier成功后原子发布。

普通loan_region_ssa_tests回归83/83通过；Clippy -D warnings、workspace all-targets、
默认FE/CG lib no-run、fmt/diff及文档897项通过。新4组CI-only身份/角色/owner别名
破坏、producer阶段与发布回滚测试仅编译，未执行。旧同根receipt、FE及旧见证均不变。
尺寸复测2562手写Rust文件、36个历史超1000文件，唯一失败仍前序FE1684>1674；
策略不改。build1010→1011仅Plans创建，经独立职责review低于原1025，emit987无增长。
日志distinct-aux-receipt-*；没有native全套或远端CI，只本地提交、未推送。

#### 双根辅助owner后续使用的独立区域（实施中）

基于fe74b4c9，增加独立五语句合同：两literal String owner、双根borrow initializer、
consume(item)、consume(auxiliary)，source参数支持0/1。不得放宽原ASAP四语句区域，
不得从普通Begin和最终Drop倒推End；需要真实调用返回时消费的acquisition与原
EndCallLoan动作对应，并独立验证source退休/Drop及随后aux实际removed-owner/Drop。
完整未过滤记录与真实入口义务、出口边界共同决定Complete，旧owner门禁及见证保留。

普通SSA研究已确认后续aux是独立第三次Begin，LoanId不等于原source、aux或result；
实际source Drop早于新RootPlace/Begin，后续consume返回后的End早于aux Drop。
source0/1×ASAP/后续使用×两入口×on/off共16组合保持SSA/LLVM一致。
本片只增加独立前端证明，不以旧CG成功冒充新receipt；后续使用的新receipt另行实施。

验收：真实typed/raw成功后新region0!=1的红已转绿；独立区域核11原动作，source
Drop7早于新aux Begin8，真实返回时原acquisition/cursor9对应EndCallLoan9，实际
removed aux owner经一次cause关联Drop10。独立review确认两Drop因果及旧门禁隔离。
FE普通11项、CG普通83项通过；新8组CI-only故障仅编译，未执行。Clippy -D warnings、
workspace all-targets、默认FE/CG lib no-run、fmt/diff及文档897项通过。日志
later-region-*与distinct-aux-later-research.log；没有native全套或远端CI。

尺寸复测2569手写Rust文件、36个历史超1000文件，唯一失败仍前序1684>1674；
超额文件和policy未改。compilation_unit1123→1126仅模块/导出，经非作者职责审阅
低于原1156额度。仅本地提交；旧见证不退休，新后端receipt待后续。

#### 后续辅助借用的独立真实收据（实施中）

基于dc0239db，独立后端协议只消费LaterUse五语句Complete区域，不放宽ASAP或任意
控制流。计划核15个连续真实producer角色与9个实体：两个owner、三个Place与四个Loan。
同一aux owner第一次作为initializer参数、第二次作为后续consumer参数，其Place和Loan
身份分别追踪；旧aux End不等于后续aux End，source Drop后不得再使用source或result。

独立准入需对应同catalog11动作、真实return acquisition/cursor、typed参数来源和原
Drop索引；实际SSA append、alias与Drop授权均保留身份，原verifier成功后才原子发布。
先真实receipt缺失红再绿，旧见证不退休、旧行为与observer-off保持，不改尺寸policy。

真实receipt缺失0!=1已转绿，普通loan_region_ssa_tests共83项通过；source0/1×
ASAP/后续使用×两入口×on/off共16组合保持原SSA/LLVM。LaterUse精确核11个FE
动作ID、15个同块连续SSA指令、9个独立实体，source Drop角色9、aux Drop角色14；
后续Place/Loan不等于原aux或source/result身份。非作者审阅确认真实operand授权、
role0冻结两owner、各append/alias复核、双Drop索引/slot/阶段及最终verifier后发布。

Clippy -D warnings、workspace all-targets、默认FE/CG lib no-run、fmt/diff及文档
897项通过。新4组CI-only覆盖权限/身份、重借用混淆、角色缺失错序和发布回滚，
仅编译未执行。FE及旧见证未改，无native全套和远端CI。日志later-aux-receipt-*。
尺寸复测2573手写Rust文件、36个历史超1000文件，唯一失败仍前序1684>1674；
policy不改。build1011→1012仅独立Plans创建，经非作者职责review低于原1025，
emit987不增长。本片仅本地提交、未推送，不声称任意多根/循环证明或旧见证退役。

#### 双根动态消费序列的真实边界研究（实施中）

基于831cbac4，先审计重复合同再扩展独立证明，不继续堆固定线性shape。令A表示
consume(aux)，B表示consume(item)，普通fixture枚举长度2..4全部22个同时含A/B的词，
再加BA⁸长链；长度仅是测试取样，不是新增语言或生产上限。每词覆盖source0/1、
rooted/memory及观察开关，共184组合，前端与后端分别实测通过。

实际结果：全部typed/raw合法且唯一handoff、binding、end、transport仍存在；原loans
为2+词长。当前新观测始终只记录一次use-return，独立source-retirement链接当且仅当
B出现一次成立；A出现在B前后均不破坏此链接。多个B不能靠“只保留最后一次使用”
扩资格。新Complete及receipt仍仅BA，其他词不冒领原ASAP/LaterUse资格。

真实SSA中每个A重新获得独立Place/Loan并End，所有B复用真实result；最后B之后
result/source依次End再source Drop，最后A之后aux Drop。两个Drop先后跟实际末次
使用次序一致；每组on/off SSA/LLVM相同。因此这些输入的执行支持已经存在，剩余
是独立证明覆盖，不应汇报为新开通的执行能力。新ordinary FE/CG测试固化上述规律。

下片优先以Vec逐次记录单B的动态aux消费序列，可先BAⁿ再覆盖A*BA*；每步须绑定
真实acquisition/return，同一活owner、中间不提前Drop、最终removed-owner因果及
完整区间。多B需另证完整使用集合，不改旧single-use witness。bounded参数返回树
不证明两个本地owner的生命周期交错，不能拼接资格；旧见证仍无同合同安全退役项。

本研究仅新增普通FE/CG fixture与注册，没有生产或门禁修改。FE矩阵1项与CG普通
84项回归、Clippy -D warnings（all-targets）、fmt/diff、文档897项通过；未执行
CI-only、native全套或远端CI，未把前序尺寸1684>1674阻塞算作通过。日志
aux-sequence-*；仅本地提交，不推送。

#### 动态BA序列的独立前端证书（实施中）

基于4bc88a34，首生产片限定单B后接n次A，2≤n≤8；n由唯一局部预算常量约束。
这是新observer证书预算，不是语言或旧执行上限。n0/1继续由ASAP/LaterUse独立
合同优先处理，n9、前置A或多B均不借用本片资格，合法源码及旧事实不受影响。

以Vec声明后缀、单调游标消费每次真实pending返回。每步必须保有原acquisition
Begin、实际End位置、同一个活aux owner；不能按声明次数补造返回链。完整区间
应为8条source/结果闭合前缀、n对Begin/End及唯一最终aux Drop，全部未过滤。
只允许最后真实返回后领取一次Drop因果，再核实际removed-owner身份与原Drop。
共享私有准入/前缀核仅复用相同合同，不放宽旧n1证明；多B集合证据另行实施。

先行为保持回归，再以真实缺失证书红测试推进；全n2..8及n9预算邻界、source0/1、
双入口、on/off独立核对。后端暂不新增receipt，旧执行成功不等于新证书已消费。

验收：私有共享重构先保持旧5项普通回归；n2新证书0!=1真红后，全2..8长度转绿。
最终FE普通7项与CG普通84项通过；CG矩阵扩为28词、224组合，核动态每use的原
Begin/End ID、活owner与完整9+2n记录，n9旧执行合法且无证书，旧receipt仍仅BA。
Single/Sequence按入口声明数量固定分派，Expected缺失仍不借用其他产品资格；
独立审阅确认无过滤或重编号中间动作，非末返回不领取Drop因果，旧n1保持11动作。

逐项游标与Vec处理不枚举固定shape；身份唯一性和planned call匹配包含有界二次
检查，n≤8限定比较预算，不声称无限线性复杂度。Clippy曾指出邻界fixture的固定
vec无必要，改为数组并复测该测试后，全Clippy -D warnings、workspace all-targets、
默认FE/CG lib no-run、fmt/diff与文档897项通过。旧8组及新2组CI-only仅编译，未执行。
日志aux-sequence-fe-*与aux-sequence-single-refactor.log；无native全套或远端CI。

尺寸复测2578手写Rust文件、36个历史超1000文件，唯一失败仍前序1684>1674。
原超额文件/policy零改；compilation_unit1126→1130仅模块和两类型导出，经非作者
职责review低于原1156额度。新动态序列专属CG receipt尚未实施，旧见证保留；仅
本地提交，不推送。

#### 动态BA序列的真实后端收据（实施中）

基于ba5871f3，独立后端协议消费预算内BA^n前端Complete证书，n2..8由实际use Vec
驱动，不逐长度枚举固定shape。预计实际角色为11+4n、实体为7+2n，须逐一核真实
SSA与前端9+2n动作ID；每轮aux使用拥有独立Place/Loan，沿同一个已冻结owner，
每轮正常End后才允许下一轮，只有最后一轮之后才领取并消费最终aux Drop。

同catalog、typed/sourcePlan、每个实际Begin/End引用及两raw Drop索引独立准入；
原producer append和alias逐步维持身份，最终原verifier成功后原子发布，失败撤销
pending。不把源码已可执行当成新receipt完成；先真实缺失红，再全长度绿与n9预算
邻界。旧ASAP/Single/见证及尺寸policy保持，仅本地开发提交。

实际receipt0!=1红已转绿，普通CG84项及224组合通过：预算内全n2..8核每个
9+2n FE action ID、11+4n同块连续真实指令和7+2n独立实体；逐use新Place/Loan与
原source/aux/result及所有先前use不同。n9和其他交错无新receipt，旧Single/ASAP及
on/off SSA/LLVM不变。中间两处引用层级编译错误已修复，不算作行为红。

独立审阅确认共用CG MAX_USES预算、动态index范围、实际operand阶段授权、role0
双owner冻结、append/alias身份、原两Drop索引及final verifier后原子发布。新4组
CI-only覆盖n2..8逐use身份、预算、角色缺漏错序、跨catalog及发布回滚，仅编译。
Clippy -D warnings、workspace all-targets、默认FE/CG lib no-run、fmt/diff和文档
897项通过；格式检查发现的模块排序已修正并复验。日志aux-sequence-receipt-*。

尺寸复测2582手写Rust文件、36个历史超1000文件，唯一失败仍前序1684>1674；
policy不改。build1012→1013仅独立Plans创建，经非作者职责review低于原1025，
emit987不增长。FE及旧见证不变，无native全套/远端CI，仅本地提交未推送。

#### 单B前后辅助消费链的独立前端证书（实施中）

基于0c5cbf0f，新增独立AroundBinding资格：唯一B、总A1..8、至少一个A在B前。
无前置A继续旧Single/Sequence，不双发；总A9或多B仅保持合法旧执行，不借用新证书。
包括AB及全部A都在B前的aux先Drop情况，不套用原source Drop固定前缀。

实际A返回分前B/后B相位：前B必须保存真实source owner和仍未使用的binding
transport义务，其source_begin/handoff锚来自当前活跃pending；后B则要求binding
及transport实际移除、source实际removed-owner/Drop因果完整。仅存两owner或最终
空表不足以证明前B仍借出的source义务，因此只读扩capture到实际binding状态。

按声明顺序的事件Vec核完整原记录，每次A独立Begin/正常End；末A才领取aux实际
Drop，唯一B的返回/退休/source Drop独立闭合。aux Drop可在B之前或之后，必须以
实际事件窗口和两removed-owner因果证明，不过滤/重编号动作套旧prefix8。CG本片
不新增receipt；先真红绿，全44个预算内唯一B位置词、n9全10位置及多B边界实证。

验收：AB的typed/raw/onoff成功后新region0!=1真红已转绿。FE普通9项通过，含36个
新增前置A形状的literal事件顺序及source相位、21个预算/旧资格边界；CG普通84项
通过，67词×source0/1×两入口×on/off共536组合，覆盖44个预算内唯一B位置、n9
全10位置与13个多B词。新FE资格只在36个前置A形状产生，旧receipt仍仅对应原
ASAP/Single/Sequence；合法旧执行及SSA/LLVM开关一致。

新增只读Unused transport快照接口不参与原Eq/reconcile或语言状态。独立review确认
前B唯一活owner/原pending锚、后B已移除binding/transport、全部原9+2n动作、两
removed-owner与实际Drop及owner事件先后对证；预算先验后计算offset，不归一化
剪裁旧前缀。现有10组加新1组CI-only（18故障×5边界形状及删除每原action）仅编译，
未执行。Clippy -D warnings、workspace all-targets、默认FE/CG lib no-run、fmt/diff
及文档897项通过；日志interleaved-*。无native全套或远端CI。

尺寸复测2585手写Rust文件、36个历史超1000文件，唯一失败仍前序1684>1674；
超额文件/policy零改。compilation_unit1130→1134只独立产品/枚举导出，经职责审阅
仍低于原1156。新Interleaved专属CG receipt未实施，旧见证保留；仅本地提交不推送。

#### 单B前后辅助消费链的真实后端收据（实施中）

基于c3e3cf04，为Interleaved独立前端证书补实际CG receipt：总A1..8、唯一B且
前置A非空，动态use Vec及B位置决定事件顺序，不新增逐长度固定shape。初始六
角色之后逐A四角色，B消费及结果/source退休四角色，最终A后插入实际aux Drop；
总11+4n角色及7+2n实体不变，但aux先Drop时B整体后移，不能复用旧role9前缀。

独立准入核同catalog所有9+2n原动作、source相位、逐loan身份与两个raw Drop；
actual producer冻结双owner，逐operand/append/alias对证，原verifier后才原子发布。
旧Sequence/Single资格、语言执行及见证不放宽，尺寸policy不变。先真红，再全
预算内B位置与预算外/多B邻界验证；CI-only只编译，不执行。本片仅本地提交。

实际missing receipt 0!=1真红已转绿；最终普通CG84项通过，67词×source0/1×双
入口×on/off共536组合，全部36个前置A形状逐原FE action/call ID、连续11+4n条
实际指令及7+2n实体核对。逐A不同Place/Loan、初始aux/result/source不混用，B与
两Drop按实际位置对应；n9、多B及旧Single/Sequence不获取新资格，开关SSA/LLVM
一致。实现中曾发现statement hook重复推进，修为真实push仅一次并复验全绿；
临时诊断代码已移除，不把该阶段失败算作通过。

非作者审阅确认entry双owner冻结、前B实际binding/source义务、后B真实退休、
末A先Drop及原verifier后原子发布/失败回滚；补齐producer动态算术前预算守卫。
新增5组CI-only含故障/遗漏错序、身份、发布回滚与最大prefix/source_index防伪，
仅编译未执行。旧FE/Sequence及其他receipt生产零改，旧见证保留；日志
interleaved-receipt-*，无native全套或远端CI。

最终Clippy -D warnings、workspace all-targets、默认FE/CG lib no-run、fmt/diff
与文档897项通过。尺寸2589手写Rust文件、36个历史超1000文件，唯一失败仍为
前序drop_planner1684>1674，原文件及policy零改。build1013→1014只独立Plans
创建，经非作者职责审阅仍≤1025，emit987不增长。仅本地提交，不推送。

#### 多次binding使用集合的独立证书（实施中）

基于cb8ad725，先证明真实多B使用集合到BindingEnd，而非重复单B receipt。现行
直线多B源码合法，无需改变语言；循环借用的L0164与跨merge义务仍不放宽。旧
UseReturn的Unused/Unique/Ambiguous及distinct单次token保持原义，不用最后一次
B冒充完整集合或重新发布旧退休资格。

首片限定两个literal String owner、一个view binding及纯A/B直线调用，A1..8、
B2..8为独立证书预算而非语言限制。声明预期B列表与每次真实pending移除的原
acquisition/返回token逐项对应，同catalog/transport lineage、顺序和唯一性核验；
真实BindingEnd闭合集合，merge、缺漏、重复、跨binding及超预算均失败封闭。
仅独立Partial证书，不证明source Drop/全region，也不新增CG receipt或退休旧见证。

普通FE实际AAAABB的集合0!=1真红转绿；103词中99个预算内集合及4个相邻边界，
逐source0/1、rooted/memory核原B loan/acquisition、返回cursor单调、唯一BindingEnd
与Partial，on/off旧义务一致。初始测试HashSet不支持action ID的编译错误修正为
成对唯一性检查，不计作行为红。长多B CG研究增至73词/584组合，含六个长链及B9；
实测10+4A+B连续实际角色，每B共用同一result loan，仅末B退休，A各自fresh loan，
最后A/最后B决定两Drop先后，未新增receipt或改后端生产。

非作者review确认声明与实际逐次匹配、真实transport携带token及merge清除、原
BindingEnd处独立闭合，旧UseReturn/semantic equality不变。修复连续B边界相等
不是重放、精确A贷款不得误推进B集合、全records拒绝重复Begin/Handoff/BindingEnd
及安全offset运算。没有新语言决定或退休旧见证，完整source生命周期留待下一片。

最终新FE1项、旧8目标9项、CG84项、Clippy -D warnings、workspace all-targets、
默认FE/CG lib no-run、fmt/diff及文档897项通过。新增CI-only一组15候选故障及
额外原记录重复/缺失只编译未执行；日志multi-binding-*。尺寸2594手写Rust文件、
36个历史超1000文件，唯一旧1684>1674失败保留，原文件/policy零改。
compilation_unit1134→1136只模块/产品导出，经独立职责review仍≤1156。无native
全套或远端CI；仅本地提交，不推送。

#### 多B集合退休与实际source Drop的独立证据（实施中）

基于4dcad3e9，先把真实use-set close token经原transport移除/BindingEnd携带到
实际source owner移除与原Drop，仍只发布独立Partial退休证据。不得伪造旧
BorrowBindingSourceRetired，或以最后一个B替代完整返回集合；旧Partial集合成功
与新Drop证明失败隔离，不因新失败丢失已有use-set资格。

专属cause只在旧cause不存在时使用，绑定同catalog/实际source owner全快照、唯一
真实named cleanup与原Drop ID，一次消费。完整A/B进度、末A Drop及整region
Complete紧接后续独立片，当前不授予完整区域或新CG receipt，旧witness不退休。

AAAABB真实source退休证据0!=1红转绿，103词矩阵核同集合、真实source owner与
原source Drop，覆盖aux已先Drop及仍live。独立审查确认非Clone cause由真实token
消费产生，同catalog/末B AfterStatement、完整owner快照及唯一原Drop逐项匹配；
旧cause优先，新失败与旧Partial集合状态隔离。整region与新CG receipt尚未授予。

旧8目标9普通FE回归及CG84项通过；集合/退休两组CI-only仅编译，含新source证明
失败仍保留旧Partial集合的隔离断言，未执行。Clippy发现Option返回可用问号的
等价风格问题，修正后-D warnings通过；未放宽证明条件。日志multi-binding-retirement-*。
尺寸仍2594手写Rust文件/36历史超1000，唯一旧1684>1674失败；原文件/policy零改。
compilation_unit1136→1138只新产品导出，独立职责review确认仍≤1156。

最终103词FE复验、workspace all-targets、默认FE/CG lib no-run、fmt/diff及文档897项
通过；无native全套/远端CI。此检查点本地提交，未推送；下一片继续完整region。

#### 多B动态辅助进度与完整区域（实施中）

基于8b4d945f，在已独立证明的全部B返回及source退休/Drop上，补A1..8/B2..8
直线完整region。声明全A/B事件顺序与实际每A的B返回进度精确对应，不能只拼两份
各自有序的列表。B之间A核活source、实际transport lineage/next进度；末B之后A
核已证source退休/Drop，不复用旧Unused状态或重建pending。

每A由真实pending移除取得Begin/End及aux owner，末A才领取真实removed aux→原
Drop；两owner真实入口、精确owner事件与空义务出口共同验证。全原记录预期
7+B+2A，保留首个旧UseReturned但不伪造多B旧SourceRetired。独立Complete不改变
旧Partial/单B资格，任何新增证明失败与它们隔离；暂无多B新CG receipt。

实际AAAABB完整region0!=1真红转绿，103词矩阵逐literal原事件复核全部7+B+2A
记录、每A真实B计数及Active/Retired相位、两个Drop次序与同一Partial退休链。
空API声明字段接线及子模块可见性编译错误已修，不计作行为红或成功证据。

最终新FE1项（103词）与旧8目标9项普通通过，CG84项通过；新增完整region CI组
25故障×3顺序及全catalog区间外重复A End/首B旧Return拒绝，仅编译未执行。
新region失效不撤销两个旧Partial产品。非作者复审通过真实token.next/source快照、
末A因果、两owner事件及真实空出口；全原事件顺序未裁剪。Clippy -D warnings通过，
新接线格式已修正。尺寸2597手写Rust文件/36历史超1000，唯一旧1684>1674失败；
原文件/policy零改。compilation_unit1138→1142只模块/类型导出，经独立职责review
仍≤1156，不以机械拆分规避策略。日志multi-binding-region-*。

最终workspace all-targets、默认FE/CG lib no-run、fmt/diff与文档897项通过。无native
全套或远端CI，仅本地提交未推送；多B独立真实CG receipt仍为下一片。

#### 多B完整区域的真实动态后端收据（实施中）

基于83c8a0da，独立CG协议消费多B完整区域及原Partial集合/退休证据。沿完整声明
A/B顺序，对全部实际append逐角色验证，不以多个B的FE acquisition虚构多个SSA
result loan。每B的实际call仍消费同一result loan，只有最后B结束result/source并
Drop source；每A独立Place/Loan/Call/End，只有末A之后Drop aux。

预算保持A1..8/B2..8，动态角色10+4A+B、实体7+2A，严格两个Drop实际次序及
source0/1位置。独立同catalog/typed/sourcePlan准入、原7+B+2A动作与每返回身份
对应；原verifier成功后原子发布，任何失败撤销pending。旧receipt/见证/policy不改，
先真实missing-receipt红，再全矩阵绿；CI-only仅编译，本地提交不推送。

实际lowering成功后missing receipt 0!=1真红转绿。最终CG普通84项通过，155词/
1240执行组合含所有FE99个多B资格形状（792组合），保留44个单B及预算外邻界。
逐全FE action/call ID与A/B顺序、连续10+4A+B实际指令、7+2A实体及observer开关
SSA/LLVM等价核验；B9/A9仍合法旧执行但不获新receipt。独立复审确认完整原区间、
每A相位/每B实际return cursor、两raw Drop与typed/sourcePlan；Plan补调用/argument
身份唯一性防伪，不放宽旧资格。新增5组CI-only仅编译，未执行；原verifier后发布
及失败回滚保持。日志multi-binding-receipt-*，FE/旧receipt生产及policy零改。

最终Clippy -D warnings、workspace all-targets、默认FE/CG lib no-run、fmt/diff及
文档897项通过。尺寸2601手写Rust文件/36历史超1000，唯一旧1684>1674失败保留；
原文件/policy零改。build1014→1015仅Plans创建，经非作者review仍≤1025，emit987
不增长；新模块与普通研究测试均<1000。无native全套或远端CI，本地提交未推送。

#### 循环借用L0164的合同与安全邻界研究（实施中）

基于e941a483，循环仍只研究，不修改Guide、两套checker准入或receipt gate。
Guide10普通借用结果节将L0164定义为缺少origin/终止证明的实现边界，并非永久
禁止循环；双轨析构节同时要求backedge保外层owner，owner liveness不能代替loan
证明。现两套borrow_binding::check_borrow_binding按While/For/Loop词法span包含
borrow marker统一拒绝，未区分每轮关闭与跨轮存活；不能直接移除守卫。

建议后续单独审阅实现合同：先限定外部稳定Shared来源、单层while、每轮局部binding
及同步消费，无逃逸/别名/跨轮携带。header须无当轮token；实际acquisition绑定当前
loop frame；normal/continue/break/return逐边消耗当轮result/子loan/source lease，
保留外层source。回边归纳恢复无当轮义务状态，不以一次静态Begin/End冒充任意
运行次数；后端独立核实际CFG/逐边End与身份。准入证明必须属于核心ownership，
observer-off也成立，不借可关闭观察器放行语法。这个no-carry方案不新增语言语义，
但本轮未授权启用实现；条件借用/from并集等仍需另行语言决定，不混入此片。

已有真实binding/transport移除和退出scope cleanup可作观察点，尚不能单独证明
循环归纳闭合。被拒源码不能伪装成有效后端输入或发布实际动态生命周期证书。
本轮无可安全退休的同合同旧见证：直线A/B与有界无环树证据都不包含此回边合同，
旧路径还承担observer-off与其他未覆盖形式。

新增普通实证：循环内绑定原四类加return、while(false)、嵌套，共七形状×双入口×
on/off=28组合，typed均成功，ownership精确L0164指向borrow，无binding/transport
且不能形成有效SSA输入。本组false源码也实际被拒，未用静态不可达假设代替结果。

后端邻界七形状另28组合：循环外binding的break/continue/return/false，以及binding
词法结束后普通loop五类均ownership成功，但CG精确UnsupportedNode、原final
verifier零次；其中return指向return，其余指向while节点。循环内临时view(source)
在ownership L0164指向该调用。普通consume(source) while-break为真实正控：
SSA多块、1 DirectCall/0 BorrowCall，原verifier一次，SSA/LLVM on/off完整文本一致。
因此只删除borrow-val的L0164远不足以形成完整前后端支持。两项普通测试通过，日志
loop-expanded-fe-research与loop-neighbor-cg-research；没有生产代码或Guide变更。

独立复审通过两项研究的阶段与精确span/onoff断言，Clippy -D warnings、fmt/diff及
文档897项通过。本片仅测试/记录，不重跑无关native全套或新增CI-only执行；旧尺寸
阻塞与policy不变。本地提交不推送，后续按用户新授权独立处理自然职责拆分。

## 具名owner销毁职责拆分（行为保持）

按用户明确要求优先按职责拆分，不修改尺寸policy或通过压缩格式规避门禁。
将drop_named、drop_named_with_binding_drop、drop_owned_value、
drop_owned_value_with_binding_drop、has_live_shared_capture、push_fact及
push_fact_with_binding_drop迁入独立owner_cleanup模块。它拥有完整的具名owner
销毁事务：保护判定、实际移除、capture逆序清理、owner事件、removed-owner cause
一次性核验及Drop事实规范化/去重/有序发布。scope/branch/loop退出选择、receiver
清理和liveness仍在planner；非职责相关逻辑不搬迁。

根文件1684→1492行，新模块205行；五个跨模块入口仅pub(super)，内部两helper私有。
七方法规范化必要可见性后签名与函数体逐字节一致，独立复审另逐方法确认body一致，
不改变原guard、状态移除、capture清理、事件和cause消费/发布次序。测试与门禁结果
见下方验证记录；旧见证、语言限制、observer-off及verifier后发布合同全部保留。

验证：前端check及7项普通integration共310测试通过；另显式启用观察测试feature
运行6项aux/loop/multi-binding普通回归全部通过（未启用feature的零测试运行不计）。
CG普通loan-region组85通过；Clippy全targets -D warnings、workspace全targets
check、FE/CG默认lib --no-run、fmt/diff、文档897项全部通过；CI-only仅编译未执行。
尺寸检查以2beb5592为基准通过：2603个手写Rust文件、36个历史超1000行债务仍报告，
本次1684>1674阻塞已由自然职责拆分解除，policy无修改；不宣称其他历史债务已消除。
未运行native全套或远端CI。日志owner-cleanup-*；独立等价性与复审均通过。


### 辅助区域 CI fixture 修复（2026-11-12，本地）

输入 `f1e16d56` 的双平台 [CI 43055725074](https://github.com/Halckon/Koven/actions/runs/43055725074)
各有 6 项 auxiliary validator CI-only 负控在正控准备时失败：fixture 假定最终 owner
目录恰有一个 definition，但旧 owner observation 在借用调用形状下为 Unsupported，
已清空目录。局部辅助区域入口此前形成的 owner 并不因此不存在，不能从最终目录反推。

测试专用、显式启用的 thread-local 只读捕获实际 `Candidate::finish` 输入，准备正控
后立即关闭；不向生产产品增加字段、不重建 owner、不改 coverage 或原六项负控变异。
新增普通正控覆盖来源第一/第二位的真实 candidate，实际先复现 1 failed，再 1 passed。
`cargo test --locked --offline -p lang-frontend --lib -- --skip ci_only` 实跑
641 passed、53 filtered；五个辅助 integration 目标共 8 passed。feature all-target
strict Clippy、fmt/diff 通过；6 项 CI-only 编译并列出，依故障注入仅 CI 约束未本地执行。
普通正控通过不代替负控验收，须明日推送后核对其远端实际结果；未推送、未处理 Preview。
原始日志留隔离工作树 `.local-artifacts/ci-fixture/`，D10/E2 状态与能力边界不变。

### 职责拆分后冻结合流（2026-11-12）

本次用户明确允许职责拆分后推送。冻结 owner 759b3280、borrow 7da4a4bf，并纳入
CI fixture 输入修复 ce3c47ce；在已发布 f1e16d56 上合流，再合入 main 6843d4a6。
后续各开发分支改动未滚动加入。生产冲突保留两条线真实 hook：owner_cleanup 保留
segment_drop 与旧 token Drop；else 分支既恢复 root-arm 状态，也记录真实终止。
main 已退休 statements 模块不恢复，push/value/receipt finish/drop authorization
四方法完整迁至既有 borrow_binding 职责；counter binding 授权/回执保留。

组合首次前端普通测试为 672 passed / 1 failed：新 distinct auxiliary retirement
证书需要真实 owner ID，owner-observer off 时合理不发布，而旧 helper 把新证书误当
旧执行产品比较。修复仅测试口径：先严格比较 source/coverage/actual actions 顺序，
再排除观察证书元数据，对完整旧 Analysis 保持严格 Eq；该 fixture 另明确断言
on 有一项证书、off 无证书。没有放宽 validator 或伪造 owner ID。该修复、冲突迁移、
公共 exports 和实际事件 hook 均经独立只读复审。

最终组合输入 0ef17f59 加上述测试修复，默认 profile 与独立 target，普通测试均
明确 --skip ci_only：frontend lib 673 passed（124 filtered），19 个改动借用
integration 目标合计33 passed；codegen ordinary_90 passed（含其匹配的 native），
旧七记录receipt正控1 passed；owner_multi_root_header_tests 23 passed，包含
counter_boundary六项、SSA/LLVM与有限native执行。另尝试以文件名过滤counter测试
得到0项，不计验收；实际六项已在上述父模块23项中完整执行。

双crate feature all-target strict Clippy、默认及feature workspace all-target check、
fmt/diff/docs897、尺寸门禁通过（2548手写Rust文件，36项历史>1000债务）。policy
与main6843d4a6完全一致，未加入借用历史分支的1669→1674增额；当前
planner1496、loops965、refine build990、borrow_binding966。CI-only只编译，
包括ce3c47ce修复的故障fixture仍待正式CI实跑；未执行完整workspace test。
Preview/DNS按用户要求不排查、不修改、不重跑；不据本地绿灯声称正式CI完成。
日志留隔离集成工作树 `.local-artifacts/integration/`。推送后须核精确远端head。
D10/E2继续active；现行各有界能力不代表一般多根/嵌套/复杂CFG及旧见证退休完成。

### 已批准下一片：嵌套内层 normal-body（2026-11-12）

用户明确同意只开放嵌套循环内层受限 normal-body 权限：绑定实际父/子 control、
callable/analysis 身份和干净资源边界，沿用真实 Replace→返回 binding→Drop 完成
事件与单次消费许可。不得以 AST 计划补造完成事件、借用旧 Complete 或构造入口 token。
本片仅接纳一个直接父子对的现有线性内层 body；缺失/损坏证据不得发布新权限，
未覆盖合法程序保持旧路径。内层出口、外层恢复和任意轮次证明仍走旧路径，
不宣称一般嵌套或跨轮归纳完成。

验收要求：普通真实输入覆盖零/一/多次内外循环并断言新内层 body 消费；
父子身份、资源边界、缺失/错序完成与重复消费必须失败封闭。故障注入负控仅 CI
执行，本地只编译；本片当前实施中，未将未运行检查记为通过。

本片本地实现：实际 ControlStack 父子进入、condition/body 边界及共享 Body 完成事件
封存独立 normal 产品，旧 Complete/其他产品互斥；后端按实际 parent frame 与唯一根
slot 接入已有非 Clone 单次 permit，不重复发射 Drop。未授予入口/退出/外层恢复权限。
只读复审发现原终检会误拒正常 RootPlace 准备链；首有效 native 正控实测 InvalidModel，
修复为逐根唯一真实 place→exclusive loan→对应 Replace 并核整块定义/使用后转绿。
普通 counter_boundary 26/26（59.03s），含内/外 0、1、2 轮九组合及新产品/完成标记；
frontend lib 普通 690/690（17.12s），含合法第二 child/三层嵌套不发布新产品。
三项新增 CI-only 测试覆盖身份/混合权限、漏/错序事件、缺许可/重复提交、错误真实
父 frame、foreign loan 与额外 RootPlace，仅编译，未本地执行，远端实跑待推送后核验。

最终本地门禁：双 crate all-target strict Clippy（5m01s）、workspace all-target
check（3m58s）、fmt/diff、文档915与尺寸2582文件/37项历史超限均通过；未改额度。
共享选择器按职责拆至 body_group/selection，嵌套正控迁到专属测试模块；build 的
共有 Lowered outcome 移至既有 frames，历史超限文件1016→1012，不新增超限文件。
输入为173cf3f7及本片，核 main 仍 f78c8c20；验证日志保留 `.local-artifacts/root-edge-evidence/`
的 nested-body-*（含首红），本片只本地提交，不推送，正式 CI 与上述未覆盖权限仍待后续。

### 嵌套已消费 body 的末端退休一致性（2026-11-12）

在 b7a5f8cc 已批准范围内重读实际 slot IR：按 FE step.header_root 映射根（不把
执行顺序与根序 zip），核 call/producer 与原返回 Drop 事实；要求 Replace < binding
Alias < Drop，并核整个函数 old/binding 唯一定义和消费，无 terminator 逃逸。
这只防止后续 IR 修改绕过已消费 stage，不增加入口、出口、外层恢复或轮次权限。
普通测试按真实调用 span 定位最终 typed-SSA Replace 第二结果并核同块唯一后置 Drop；
新增晚期删除/重复实际 Drop 的 CI-only 负控，仅编译，不在本地执行。

本片验证：普通 counter_boundary 26/26（62.32s，含新最终 SSA 旧值断言），codegen
all-target strict Clippy（1m21s）、workspace all-target check（1m26s）、fmt/diff、
文档915与尺寸2582/37历史超限通过，未增加额度。无 FE 修改，本片未重复运行 FE
全回归（基线 b7a5f8cc 的690项仍仅为该基线证据）。两个新晚期故障仅编译、未执行；
静态最终 SSA 旧值一致性不等于运行时释放次数或跨轮归纳。独立只读复审未发现确定
阻断，native通过确认死局部binding槽在终检仍可查。日志 nested-retirement-*，仅本地提交。

### 嵌套旧路径出口/恢复实证审计（2026-11-12，仅测试，无新许可）

基于0a0d9eb5，仅加强已有九组有限 native 输入的最终 SSA 断言：以原前端 primitive
调用身份定位内层 b Replace、外层恢复 b Replace，核 child body 参数→header 参数；
从 header.false-edge 逐实际 edge.arguments/destination.parameters 的相同索引运输，
每边同根恰一次，visited 防止绕入另一轮。只沿无条件走廊，中途不得读/消费根，
到外层实际 Replace 时核唯一 RootPlace→exclusive loan→Replace 准备链和 owner 身份。
entry/normal 两个静态 header 入边不标成某一运行时实例，不推出任意轮次归纳。

下一出口/恢复权限仍未批准、未实现：需要实际 parent 暂停与 child false-exit 完成
事件、entry/normal 合流来源和根/资源分区、parent 恢复完成及对应消费者；不能把
此测试中的 SSA 参数映射当作现成 owner token 或用有限运行次数代替循环不变量。
本片没有生产代码改动，也不改变多 child/更深嵌套的发布边界。

本片最终普通 native 用例1/1（1.55s，内部外/内0、1、2轮九组输出及上述SSA断言）
通过，codegen all-target strict Clippy（58.78s）通过。仅测试与说明变更，未重跑
完整counter/FE/workspace回归，未执行任何CI-only；独立只读复审未发现确定阻断。
该测试核末块准备链定义、顺序及owner使用，不单独宣称place/loan全函数消费证明。
日志 nested-exit-audit-*；仅本地提交，不推送，不开启新权限。

### 受限嵌套 normal-body 临时准备能力不逃逸（2026-11-12）

保持 b7a5f8cc 的批准边界，终检除 body 内根访问限制外，对实际 RootPlace/BorrowBegin
产生的临时 place/loan 槽核全函数精确唯一定义及消费，所有 terminator 禁止引用两槽。
owner 根仍可走既有出口/外层恢复路径，不把临时能力约束扩大为根的跨块禁令。
新增 CI-only 异块复制实际 BorrowBegin 故障，要求命中一次且不发布 body 完成；
故障仅编译、不在本地执行。独立只读审计确认这是现有消费合同终检缺口而非新权限。

补充第二个 CI-only 直接将 false 块终结器改为携带实际 loan 的 BorrowReturn，
同样要求命中恰一次且无 body 完成。普通 counter_boundary 26/26（47.63s）通过；
之后仅增加该测试故障，未改变生产逻辑。两故障均仅编译，不宣称已运行通过。
codegen all-target strict Clippy通过（26.73s）；最终只读复审无确定阻断。
本片未重跑FE/workspace；不把静态不逃逸等同于任意轮次动态证明。仅本地提交。

### 下一嵌套出口合同选项（2026-11-12，设计待批准，未启用）

当前 Nested 仅保存 conditioned/body_entered/left 与 normal sink；nested_leave 的
success/left 不是独立 false-exit 完成事件，也不包含零轮与正常回边的运行时选择。
因此不能从 left 或现有测试的最终 SSA 映射合成出口 owner token。

建议拆为两个需明确批准的交付边界：

1. **出口事实产品，不授予恢复消费。** 为同一 analysis/callable/parent/child 建立
   独立 child false-exit 完成采集，记录实际 header、false successor、根 symbol 向量
   和干净资源分区。入口与 normal 来源须保持两个有身份的候选，不把它们压成遍历
   最后见到的 producer。缺事件、重复/错 control、根/资源不一致则整项不发布；旧路径
   继续负责发射。后端仅核真实 CFG/参数映射，不将此事实当 Drop 或 Replace 许可。
2. **外层恢复消费迁移。** 只有第一步具备完整实际合流身份后，再为 parent resume
   的实际完成位置、root binding 和下一消费者发一次性许可。必须防止已离开 child
   的旧 permit 重用，防止与 generic 发射双 Drop，独立核零轮/一轮/多轮有限用例。
   不包括多 child、更深嵌套、break/continue/return 或任意轮次归纳。

选项1优先级高于继续叠加normal-body审计，但它仍需用户批准该新增事实合同，当前
不做生产接线。选项2必须另明确是否授予恢复消费，不能把选项1批准解释为一并批准。
验收应包含真实事件丢失/错序、不同control串换、false边根参数串换和脏资源负控；
故障遵循CI-only，本地只编译。有限native输出与静态SSA候选身份均不是动态迭代证明。

### 已批准：内层 false 出口事实采集（2026-11-12，不授权外层恢复）

用户明确批准选项1。实际 drop_root_loop_exit 调用 drop_loop_exit 后，仅使用本次新增
LoopExit facts及清理后state封存可选分区，绑定既有analysis/callable/parent/child/condition。
已完成condition/body/normal事件为前置；首次实际false cleanup、false可能性、完整干净
boundary_values及所有根恰落live或fresh Drop之一才记录。重复事件撤销可选事实，不把
left当出口事件。不更新旧ValueState，不复活Complete，不给后端恢复或新Drop许可。
header根身份与normal producer候选保持分离；没有真实entry token catalog，因此不造
零轮token、不选择运行时代际。此片是前端实际清理点身份/分区，尚无新后端CFG receipt。
缺出口事实保留已批准normal-body权限。CI-only覆盖缺实际cleanup与重复cleanup，仅编译。

本片最终FE691/691（6.75s）、普通counter native26/26（31.23s）、双crate all-target
strict Clippy（38.56s）通过。最初补充测试误期望去掉最终a读就退休a，真实结果live2；
已确认外层下一轮仍需a，将其保留为真实活性正例，未为迎合测试改生产分区。
当前普通输入覆盖全live和父回边仍需根，尚未实证嵌套部分退休正例；不得宣称此矩阵完成。
两新增故障仅编译未执行。独立只读复审无确定阻断；所有结论仅静态清理事实。
workspace all-target check（28.48s）、fmt/diff、docs915及Rust尺寸门禁通过。
仅本地提交，不推送；下一步仍需真实CFG receipt与候选来源关系，恢复消费未授权。

### 已批准出口事实的原清理发射核验（2026-11-12）

真实部分退休fixture将a/b声明置于outer body，使a不跨父回边存活；child false分区
为live b + a的LoopExit Drop。外/内0、1、2轮九组native输出已通过针对测试。
不能用“移除最终读取”替代这个实际生命周期边界。

后端仅在可选false分区存在时复核已捕获header.false→false块几何，把精确Named根
映射到原normal根槽，按原LoopExit fact/consumed_drops/span验证旧通用发射中的唯一
Drop及顺序。不发新Drop，不禁live根后续println/Replace，不禁止槽的新生命周期复用。
这是RefinedBody阶段原清理指令事实，不是late-SSA CFG fold receipt或退出消费许可。
新增CI-only删除实际false块Drop，要求实际命中且无清理/normal完成；本地仅编译。

最终普通counter native27/27（33.50s）、codegen all-target strict Clippy（32.67s）
通过；独立只读复审无确定阻断。本片未改FE、未重跑FE/workspace；CI-only仅编译。
最初fixture声明分隔parse红已修正，日志nested-local-exit-*保留。仅本地提交不推送。

### 部分退休的最终 SSA 独立检查（2026-11-12，仅测试）

在普通Refine/fold完成后的Program中，以原FE root step与call span唯一定位child
Replace，从body根参数找真实header，沿false边的arguments/parameters运输到原
LoopExit fact span唯一Drop。逐边要求根恰一次，走廊无提前使用/重定义，visited防环，
目标Drop必须消费运输后的实际SSA值且该值恰有一个Drop。
这是对558f3ff2前折叠核验的独立测试，不新增生产late-SSA receipt或恢复权限。
不证明动态释放次数，也未单独检查Drop后全部其他用途；完整IR合法性仍由verifier核验。
独立只读复审无确定阻断，有限native九组输入仍不代表任意轮次归纳。
本片普通native1/1（0.27s，内部九组）与codegen all-target strict Clippy（39.63s）
通过；仅测试/说明变更，未重跑全counter/FE/workspace，未执行CI-only。日志
nested-exit-final-ssa-*，仅本地提交不推送。

### 内层 false 清理生产 late-SSA receipt（2026-11-12，不授恢复权限）

新增独立 nested_exit_receipt，不复用旧bridge权限。Builder在原清理事实全部核对后
封存child/header/false块、退休根slot、原Drop指令位置和span；RefinedBody携带独立计划。
straight-line fold同步块号及移入指令prefix偏移（溢出/合并成同块失败封闭）。
layout绑定真实函数、control及header/false的slot→Entity参数快照；实际Drop发射钩子
要求匹配位置、origin、owner且消费原false入口Entity，恰观察一次。
最终重读实际header.false边、根实参、参数快照、Drop操作及块内清单唯一位置和顺序，
全部完成后才记录receipt。不会额外发射Drop，不检查/授权parent后续消费。
空退休集合仅核几何，不声称核验所有live根运输。静态SSA一致性不等于动态跨轮归纳。
新增CI-only实际移除/重复出口块Drop引用；本地仅编译。独立复审发现的块清单/参数
终检缺口已补，不把instruction arena仍有记录当作实际执行指令仍存在。

普通counter native27/27（28.65s）通过，真实完成marker及前片最终SSA独立断言仍通过。
严格Clippy首轮仅两处CI注入collapsible-if报错，已改为等价let-chain，未改变生产代码。
复审终检修正后无确定阻断。两新晚期故障仅编译，不声称本地执行通过。
最终codegen all-target strict Clippy（38.66s）、workspace all-target check（29.30s）
通过；fmt/diff、docs915和尺寸2583/37历史超限通过，不增加额度。本片无FE变更，
未重跑FE全回归。日志nested-exit-ssa-*；仅本地提交不推送。

### false边完整根向量（2026-11-12，仍仅事实）

基于b414093b，生产receipt现保存完整normal根槽而非从退休集合反推；layout要求
向量非空无重复且每根在header/false块参数中恰一次，Drop根必须属于该向量。
finish逐live及retired根核真实false实参与header入口Entity，保留原Drop核验。
全live及部分退休普通输入均断言完整向量数2。此片使空drops不再仅核几何，但不
证明退出后的持续有效性、动态代际或授权parent消费。新增真实false边参数交换
CI-only负控，仅编译。普通counter27/27（13.13s）通过，独立只读复审无确定阻断。
后续恢复权限仍需单独批准，不能因完整边向量匹配直接使用既有normal permit。
codegen all-target strict Clippy（28.11s）通过；新故障只编译，未执行。未重跑FE和
workspace（本片无外部API变化）；fmt/docs/尺寸/diff门禁通过后本地提交，不推送。

### 已批准：受限父恢复的一次性消费合同，第一步真实边界（2026-11-12）

用户明确同意：仅child false证据完整、实际parent及根向量匹配时允许一次性恢复消费；
新恢复链缺项或错配整组拒绝，不扩大一般嵌套/任意跨轮。此提交先落实必要真实边界，
尚不发布恢复消费许可：实际control stack离开child之后，要求当前parent精确匹配、
child正常返回、已记录false分区，重新以boundary_values核干净live根；重复/缺事件
无resume事实，不能用left或AST顺序代替。退休根不得重新加入恢复向量。

后续实现限定紧接child的单条 `val old=replace(&liveRoot,"literal")`，AST/typed primitive
仅识别预期，必须由真实literal→Replace→binding→AfterStatement Drop事件完成独立
状态机，再接非Clone一次性消费者。不套用要求两根及完整while body的BodyPlan，不
将child normal permit复活，不将形式根来源冒充运行时代际。新恢复许可未完成不影响
旧child normal产品；不能把本提交边界事实宣称为已完成恢复授权。
恢复向量按实际symbol集合匹配，不保证与false分区顺序相同；后续消费者必须按symbol
配对，不得直接zip。独立复审无确定阻断；缺实际恢复事件CI-only仅编译，重复恢复/
错parent专用故障矩阵尚待补。FE691/691（6.58s）、普通native27/27（25.89s）通过。
双crate all-target strict Clippy（53.81s）、fmt/docs915/尺寸/diff通过。未重跑workspace；
本片仅本地提交不推送。日志nested-parent-boundary-*。

### 已批准受限父恢复：真实单笔事件与独立一次性发射许可（2026-11-12）

新增独立Resume collector，只识别child后紧接的一条val/direct String Replace声明。
真实literal完成→Replace提交→返回binding→新增AfterStatement Drop→声明成功且clean
状态齐备才封存parent_resume_write；错序/缺项/插入其他语句不发布。完成后相同精确
statement/call/producer/binding/drop重现撤销，后续无关事件可继续。返回值保持存活、
非直接literal或非紧邻语句均不进入新恢复产品。不造旧owner动态token或伪装完整BodyPlan。

CG共享原发射钩子的互斥入口但使用独立resume标记和Ready→Replaced→Bound→Dropped
状态；non-Clone permit绑定parent、恢复块、指令位置/span、真实root/replacement槽。
按root symbol查槽不zip。实际parent frame和资源必须匹配，既有child许可不得复活。
最终重新核原call/literal、Replace→Alias→原Drop及old/binding全函数唯一定义消费，
核实际RootPlace→exclusive loan→Replace和place/loan整函数不逃逸；恢复前缀除对应
RootPlace外不得读写原根，恢复后允许既有合法操作，不额外发射Drop。

实证修正：while的[2]是false清理块，[3]才是恢复join；新许可要求[2]→[3]且唯一
前驱，[2]对恢复live根透明。首两次联合测试因误把恢复literal放[2]拒绝，未放宽保护，
按实际CFG修正后九组native输入通过。最终SSA false-root receipt仍独立全组核验；
这是编译期单次发射许可，不声称运行时只执行一次或一般嵌套/任意轮次归纳。

验收：最终FE 692/692（15.38s）、counter_boundary普通native 27/27（31.73s）
通过，包含受限恢复完成标记与外/内零、一、二轮九组合。双crate all-target strict
Clippy与workspace all-target check均通过（各约1分钟）；fmt、docs915、尺寸
2586手写文件/37历史超限、diff门禁通过。CI-only缺独立permit、真实foreign loan及
FE事件缺失/完成后重放负控仅编译，不宣称本地执行通过。独立只读复审未见确定阻断。
日志nested-resume-final-*；本地提交不推送。尚无独立父恢复late-SSA receipt或
任意跨轮代际归纳，仍沿用通用SSA验证及既有child false根receipt。

### 受限父恢复独立 late-SSA receipt（2026-11-12）

在既有单笔parent许可和原始CFG终检全部完成后，独立封存实际parent/child、
header/false-cleanup/join、root/replacement/old/binding槽及Replace/Alias/Drop精确位置。
straight-line fold同步块和指令偏移，允许false-cleanup与join合并，但不能并入header。
生产发射按实际layout把各块根slot映射到Entity，核child false边与必要的后续Branch
运输。实际Replace输出快照、被消除Alias的current映射及原Drop分别观察；末端再核
真实指令列表唯一有序、操作未改变和第二输出唯一消费且不从块参数/terminator逃逸，
全通过才记录本receipt完成。不额外发射Drop，不扩大父恢复可接纳形状。

receipt在全部函数块发射完成后核验，仍早于整个Program的verify_program；不调整或
绕过原verifier，也不把receipt完成标记说成最终验证已通过。真实SSA负控覆盖删除
block清单中的Drop、改Drop为消费新根、重复Replace清单项，均限CI执行。

本片最终普通counter_boundary native 27/27通过（33.19s，含新receipt标记与九组输入）；
CG all-target strict Clippy通过（28.01s），workspace all-target check通过（38.48s）。
独立复审补齐原RefinedBody三操作精确复核、RootReplace含loan完整操作快照及根槽
恰一次后未见确定阻断。三项新增CI-only负控仅编译，未在本地执行；FE代码未修改，
本片未重跑FE。日志nested-resume-ssa-*；仅本地提交不推送。
静态门禁：fmt、docs915、diff通过；Rust尺寸门禁以7c1f2ae8为基线通过，2587手写文件、
37历史超限，无新增豁免或门禁调整（emit.rs保持1000行上限内）。
## 闭合借用词法块之后的普通while（实施中）

在合流66ee7526上复核原循环研究，既有七邻界结论仍成立。进一步定位发现
validate_bindings经ordinary_local和borrow_result_support::binding的整函数statement
扫描，无条件拒绝While/Loop：即使真实BorrowBindingEnd已经关闭前一独立词法块，
后续普通while也被拒。这与循环内创建binding的L0164是不同层次的缺口。

本片仅考虑已关闭借用块之后、同父块兄弟位置的普通while，独立statement能力不能
混用expression索引；需精确绑定身份、真实唯一End、每个同callable借用分别
满足门禁。循环中的表达式与真实lowering仍走原检查，不能跳过body或将静态身份
当作动态实例。实际loop入口还须复核已授权结果及source义务已被关闭；source参数
本身合法的常驻借用不能与result-source残余混淆。

不涉及活跃loan跨回边、循环内借用、loop/for泛化或祖先块提升；不删除L0164，
不退休旧见证，也不宣称已有循环借用归纳证明。后续以真实红绿和邻界验证记录界定
实际可交付范围；未经验证的设计不计入完成。

真实红诊断进一步修正原假设：正控BindingEnd位于块尾consume(item)的
AfterStatement，而非Block本身。因此本片采用独立的直线末调用合同：块仅含直接
binding声明及普通调用，唯一End精确指向最后简单Call；随后while为同父后续兄弟。
不把源码scope结束假充成事实中的结束点，实际result/source loan移除仍由原
drops/end_result_bindings完成，while入口另复核，未新增动态循环receipt。

实现与独立复审：closed_while私有模块仅为Scoped来源产生独立StatementId集合；
唯一实际End须同源且位于唯一直接Block的末简单调用。块内仅直接binding声明及
参数为Name/Literal的普通非borrow-result调用，拒隐藏控制和closure。while仅同父
后续兄弟、Name/Literal条件且无新binding或嵌套循环；表达式门仍原样逐项执行。
共享门的其他caller保持false或精确MapFor能力；legacy单位路径对source_loan Some
仍先拒，不能绕过refine入口的真实borrowed/result_source_loans/pending检查。

真实红为原lexical-end-before-loop的while整span Unsupported，改后14形状×双入口
×观察on/off=56组合及source已move四组合通过；已move源码仍ownership拒绝。
真正BorrowCall返回LoanId仅有一次BorrowEnd且在entry、先于后续loop；Borrow参数
source为BlockParameter且不得End，owned source的新LoanId则唯一End在resultEnd后。
真实SSA、LLVM及原final verifier一次均通过，on/off文本相同。不新增循环receipt。
普通loan-region回归86通过；宽ordinary首跑的一项失败来自zero循环DirectCall计数
测试假设，修正断言后最终精确回归通过，未为通过测试修改生产语义。

恢复连接后确认原86通过日志与工作树保留，归档closed-loop-*；最终独立复审通过。
Clippy全targets -D warnings、workspace全targets check、FE/CG默认lib --no-run、
fmt/diff及文档912项通过；尺寸以2beb5592为基准通过，2550个手写Rust文件、36个
历史超1000行债务仍报告，policy未改。CI-only仅编译未执行，本片未推送。

## 外层不变量借用while合同（实施中）

基于f80cd4c9独立推进，不纳入尚未合并的其他PR修复。先实测原FE事实：显式外层
[binding,while]词法块的normal/continue/break/false四形状都只有包围Block的
AfterStatement结束；return另有对应真实ControlTransfer结束。五形状×双入口×
on/off共20组合typed/ownership合法且旧事实相等，原始结束点身份已固化测试。
这证明外层binding不是逐轮acquire/retire，但不替代后端实际循环证明。

下一最小合同仅考虑外部一次acquire、Boolean Name/Literal条件、body直线同步
consume(item)，无break/continue/return、分支、嵌套、新binding或source移动。
必须真实唯一外层Block End，保持同一逻辑result/source loan贯穿header/回边与
false出口，最后才结束。Borrow参数来源与owned source新loan分开核身份和结束，
不能错误结束调用者入口loan。现有SSA fixed-point运输可复用但须真实verify后才
计支持；不得修改verifier或用固定源码形状代替独立身份与生命周期核验。

实现的独立准入明确直接Name+Declaration调用、无receiver，source单Borrow映射
index0/from0且String身份一致，consumer同binding的非cross-thread Borrow参数、
Unit结果，无新借用或abort。Block仅[binding,while]且唯一真实Block End。Builder在
entry/header/body-afterleave/false-afterdrops/exit核result/source/root owner或
入口loan逻辑身份不变，并拒pending call/view残余；原verifier未改，旧见证保留。

普通真实SSA断言从原BorrowCall的source/result出发，逐条edge按参数位置传播
lineage，header恰entry和backedge两前驱，true/false都运输结果lease，own另运输
来源与RootPlace所指owner；body不End且acquire只有一次。false出口按真实身份
resultEnd→sourceEnd→ownerDrop，Borrow参数没有伪sourceEnd。独立八类邻界32
组合仍合法FE/拒绝CG，含跳转、嵌套、新借用或额外消费等超合同情形。

最终正控增加true：Borrow/own×flag/false/true×双入口×on/off共24组合，真实
Boolean常量指令和false edge清理目标均核验。true保留结构上的false出口，它在
运行时不可达，不能将该End解释为循环必然终止或必然清理。最终88项普通回归通过；
没有执行可能无限运行的native循环。首Clippy仅测试嵌套if告警，改为等价let-chain，
未修改生产逻辑或降低门禁；独立最终复审通过。

最终Clippy全targets -D warnings、workspace全targets、默认FE/CG lib仅编译、
fmt/diff及文档912项通过。尺寸基准2beb5592通过：2555个手写Rust文件、36个历史
超1000行债务继续报告，policy无改动。新增production职责分为独立准入201行及
实际lease状态核验87行；build根仅新增模块声明，未扩大其调度职责。CI-only未执行。
日志invariant-while-*与outer-loop-fe-final已归档；本片仅本地提交，不推送。

## 外层不变量while直线消费序列（实施中）

基于3da9ea3a，实际运输状态不依赖consumer数量；唯一旧准入限制为单元素body。
本片将合同推广为非空动态AST直线序列，逐调用完整校验原直接Declaration、唯一
Borrow(item)映射、Unit同步返回与无abort，不枚举n或新增无根据形状预算。循环内
依旧不acquire、不跳转或分支，不混入其他root或嵌套调用；唯一真实End仍在外块，
L0164、旧见证与verifier保持。支持须由实际各call同result身份和每条循环edge来源
保持证明，不以通过AST扫描替代SSA验证。

真实count=2红为while整span Unsupported；生产改为非空for逐项执行同一谓词，
Builder/FE/verifier均未改变。正控n=1/2/3/8/17×Borrow/own×flag/false/true×
双入口×on/off共120组合，交替两个实际consumer；逐所有DirectCall核同result
lineage、callee身份与block顺序，BorrowBegin仅own=1/Borrow=0、BorrowCall一次，
原每edge运输与唯一出口End/Drop断言保留。独立12类48邻界核首/中/末混root、
空body、nestedblock及旧jump等拒绝；nested view仍原FE L0164，不假装合法CG输入。
最终88项普通回归通过，独立非作者审查通过。未新增循环receipt或退休旧见证。

最终Clippy全targets -D warnings、workspace全targets、默认FE/CG lib --no-run、
fmt/diff及文档912项通过；size基准2beb5592通过（2555手写Rust文件、36历史超限
债务），policy未改。CI-only仅编译，日志invariant-consumers-*归档。本片本地提交。

## 不变量while尾部continue（实施中）

基于ff9187b6，仅在非空已验证直线consumer序列尾增加一个无条件Continue，
中间/条件/嵌套jump仍不属于合同。原loop_jump先清理再实际Goto(header)并返回
Diverged，因此不能用正常fallthrough回边证据替代continue：需查回精确tail节点、
对应while frame与header，清理前严格loan/view边界，清理后跳转前lease不变。
不将通用min边界容错当作新协议匹配；不支持For frame、break/return或新借用。
真实SSA需证明header恰entry和continue两入边、无额外正常回边，continue块无
result/source End或owner Drop；false出口与true动态不可达出口仍按前合同验证。

真实tail-continue红为count1/Borrow/flag/rooted/on的while span Unsupported。
实现只放行末直接Continue，原consumer全部逐项检查不变；loop_jump新增原清理前
和实际Goto前两个hook，精确tail/while/frame、非For、loan/view长度相等与原header
目标及lease保持均失败封闭，原while快照在Diverged后继续核验，不制造正常回边。

强正控240组合含原120与末continue120，actual terminator源位置精确Continue、
唯一无条件Goto至原header、恰entry+continue两incoming；该块无End/Drop/Begin/
BorrowCall。旧source/result/owner lineage及出口顺序、on/off与LLVM/verifier保持。
独立14类56邻界仍拒条件/嵌套/空consumer continue及break/return等。最终88普通
回归全绿；迁移旧合法continue负控时按真实错误锚锁定条件if整span，未改生产
掩盖失败。独立非作者最终复审通过，未新增receipt或修改FE/verifier/尺寸策略。

最终Clippy全targets -D warnings、workspace全targets、默认FE/CG lib --no-run、
fmt/diff、文档912项及size基准2beb5592全部通过（2555文件、36历史超1000债务）。
container_for仅新增两hook至982行；CI-only仅编译未运行，日志invariant-continue-*
已归档，本片只本地提交、不推送。

## 不变量while尾部break（实施中）

基于9cf0abb0，只在非空已验证consumer序列尾支持一个直接无条件Break。实际jump
token区分Break/Continue，精确尾节点与原frame.exit/header目标不能互换。原
loop_jump的LoopExit named Drop逻辑不跳过或推迟：若它提前清理持有lease的root，
资格或实际cleanup后snapshot必须失败封闭，不能用最终verifier代替提前检测。
FE既有break实证仍只有外围Block End，目标是break与false两条边运输同一lease
到共同exit，再按原事实结束结果/source/owner；header仅entry且没有回边。常量
条件按实际生成边验证，不把结构可达清理声称为运行时必达。L0164与旧见证保留。

真实break红为count1/Borrow/flag/rooted/on的while span Unsupported。实现仅扩
显式JumpKind及token种类/target绑定，保留原consumer准入与所有原cleanup；实际
Goto前核kind/target/lease/dead。首次已通过lowering/verifier后发现旧测试假定
header两incoming不适用于break：原simplifier把无回边header并入acquire块，实际
零incoming；据真实CFG改强断言，不修改simplifier或伪造折前结构。

最终360正控含旧240与break120；break真实源锚Goto及false边各自运输同lease到
同exit，恰两前驱，合流后唯一resultEnd→sourceEnd→ownerDrop（Borrow参数不伪End），
jump块无End/Drop/重新acquire。16类64邻界保留条件/嵌套/空consumer break与return
等拒绝。88项普通回归通过、独立非作者复审通过。未新增通用receipt或退休旧见证。

最终Clippy全targets -D warnings、workspace全targets、默认FE/CG lib仅编译、
fmt/diff、文档912项及size基准2beb5592全部通过（2555文件、36历史超1000债务）；
policy未改，CI-only未执行，日志invariant-break-*归档。本片本地提交不推送。
期间核对新main b1fc7d93已合并#412，与本分支共同基线66ee7526；未覆盖或擅自
吸收其9a8cd2b7测试修复，后续合流需在新main上重新验证。

## 不变量while尾部条件break（实施中）

基于c39b6ec4并核工作树干净。客户端更新短暂断线后重新attach并复核，无未提交
改动被覆盖。本片优先尾if(secondBool){break}无else、then仅一个Break；true
实际进入公共exit、false正常回边，比两个同目标的conditional continue更便于
核最终真实分叉，不假定simplifier一定保留双回边。

只有完整不变量合同admitted后，该binding的精确尾If ExpressionId才获共享门
资格；条件仅typed Boolean Name，不泛放任意if或条件调用。原BranchExit清理和
原break的ControlTransfer/LoopExit清理保持，分别在false回边及true退出核同一
lease，最终break和外while false合流后才结束；不能把AST准入当成生命周期证明。

实现的内层条件可与外层引用同一个Boolean，也可为另一Boolean参数；“second”
指条件位置而非要求不同symbol。不会利用两条件相关性自行剪枝。actual if在condition
及false BranchExit原drops后复核lease，true仍经精确Break token和原CT/LoopExit
清理，再核kind/target/lease。共享Expression权限仅该已admitted binding的尾If，
不放行任意条件。独立非作者生产复审通过；强SSA测试追溯实际原参数身份而不是
只比类型或名称，分别检查回边和退出合流的source/result运输。

最终强正控600组合（旧360+同/异名内条件240），内条件真实SSA来源沿edge追溯
原Boolean参数1或2，true精确break→共同exit、false正常回header，两处各两incoming
逐source/result/owner身份保持，最终合流后唯一End/Drop。原loop研究条件break
负控改为真正超合同的带else形状，精确错误锚按实际表达式核验；不保留失真的旧拒绝
预期。19类76邻界与88项普通回归全绿，无新receipt、FE或verifier改动。

最终非作者review、Clippy全targets -D warnings、workspace全targets、默认FE/CG
lib仅编译、fmt/diff、文档912项及size基准2beb5592通过（2555文件、36历史超1000
债务）。policy不变、CI-only未执行，conditional-break-*日志归档。本片本地提交，
未推送；仍需后续与新main合流验证。

## 不变量while尾部裸Unit return（实施中）

基于506e7f16评估消费序列后直接Return(None)，与break/continue保活合同不同。
准入必须精确同binding双End：该Return的真实ControlTransfer与外围Block的
AfterStatement，拒额外End或返回值；原L0164和verifier不变。已有CT清理刻意保留
borrowed/source逻辑maps给false路径，但真实return路径必须结束result/source并
按原Drop事实清理owner，再发Return。不得以Diverged直接豁免活状态检查或复活
owner；需独立的实际完成凭据，false路径仍依原控制流状态独立核活lease。
实施前须核原状态转换，未经真实路径验证不计入支持。

原false恢复机制已实测：emit_drops即使facts为空也先sync_dead，按进入finished
时的entry_dead恢复该路径；未新增手工清dead或复活。return前捕lease/block/append
cursor，原cleanup/owner_return后核新增区间精确resultEnd、sources逆序End及own
Drop，且精确Return(None)源锚；逻辑maps保留但owner在return路为dead。body结束
仅该已核tailReturn及原body块完整suffix可用退休检查，不泛放Diverged；false仍
要求原活snapshot。独立非作者复审通过，FE/verifier/旧产品未改。

真实return红指向裸return；720强正控含旧600及return120，return真支与外while
false各自实际Return(None)，逐路径各一次resultEnd→sourceEnd→ownDrop，Borrow
参数不伪sourceEnd。无回边CFG据实按原fold结果核，不强造header incoming。21类
84合法CG负组合及2个typed L0087负例分阶段验证，不把类型拒绝输入传CG；88项
普通回归全绿，未新增通用循环receipt、未退休旧见证。

终门禁manual_contains告警以等价contains替换解决，没有放宽检查；重新88普通
回归、Clippy全targets -D warnings、workspace全targets、默认FE/CG lib仅编译
全部通过。fmt/diff、文档912项及size基准2beb5592通过（2555文件、36历史超1000
债务），policy未改、CI-only未执行。invariant-return-*日志归档，本片本地提交。

## 不变量while尾部条件continue（实施中）

基于4b08ac80，尾部精确If(BooleanName){Continue}无else/then唯一jump；复用
条件jump描述但显式kind，Continue必须匹配原header而非Break exit。true经原
jump清理和Goto，false经原BranchExit及正常回边，两路分别核同lease。权限仍只
属于完整admitted的该binding尾If，不泛放控制或新借用。
原emit简化仅合并Goto到单前驱后继；此形预期header有entry和两回边，但必须
以实际最终CFG为准，不为测试禁止优化或伪造边，也不将普通SSA测试称为新receipt。

真实红为尾条件Continue的If span Unsupported；生产只将精确ConditionalJump
携带既有kind，原Break资格与其exit目标不变。最终SSA实际保留两个Conditional和
header三个incoming：entry、true显式Continue、false正常回边。两回边块与原
terminator源锚不同，均指原header，逐source/result/owner及原Boolean参数身份
保持；无需禁止优化或伪造pre-fold证据。此为支持合同实证，尚非独立循环receipt。

最终960强正组合含旧720与条件continue240，24类96合法CG负组合及2个typed
L0087负例全部通过，88项普通回归全绿。独立非作者review通过，L0164/FE/verifier/
policy不变，无循环内新borrow或多根循环支持，旧见证仍保留。

最终Clippy全targets -D warnings、workspace全targets、默认FE/CG lib仅编译、
fmt/diff、文档912项及size基准2beb5592通过（2555文件、36历史超1000债务）。
CI-only未执行，conditional-continue-*日志归档。本片仅本地提交、不推送。

## 循环不变量独立FE关系（实施中，Partial）

基于59e9725d转向独立receipt的前提，不继续堆固定控制形状。首片只观察已合法
外层一次borrow-return binding、普通Boolean参数while及非空直线consumer序列；
无jump、无循环内acquire。计划UnitLoanLoopInvariantLineage永远Partial，分别
绑定同catalog实际source Begin/handoff、entry、被真实遍历body的正常回交、可达
false出口状态及最后原BindingEnd，不把一次静态遍历冒充任意运行次数的Complete。

从真实入口保存独立因果票据，旧PendingBindingTransport reconcile失效不恢复、
旧PartialEq/产品不改变；实际body_state clone和外层state分别核验。常量false
未访问body不造证据，true无正常出口不补End；Borrow参数缺owner合法，不伪造owner。
观察关闭仍由原checker执行，证书不参与语言合法性，L0164保留。独立CG receipt
仍须后续actual block/slot、edge/fold验证与原verifier后原子发布，本片不提前声称。

真实红为原typed/ownership/唯一BlockEnd成功后新产品0!=1。实际探针纠正来源假设：
Borrow和own参数旧记录均无Handoff、transport=0，local literal String则有真实
Begin/Handoff与transport；首片严格仅该local三语句形状发Partial，参数原lowering
仍正常但新资格空。不伪造ID或恢复旧transport观察能力，CG三语句资格尚需后续接入。

producer入口不能提前要求未来尚未发生的BindingEnd。计划仅声明completion位置，
独立非Clone票据由真实入口捕获；body_state clone正常回交及外state退出分别核，
最终actual end_borrow_binding再验证唯一精确End位于真实exit后。每consumer回交
从finish_call_pending_loans实际extract_if移除捕获原loan/Begin/cursor侧记录，
不是从Begin列表推定；完整body原记录逐项回放，额外动作、重复/错序回交均拒绝。
final重核Begin/Handoff/End与票据同catalog及唯一性，旧raw End/Unique资格不改。

普通矩阵local/Borrow/own×consumer1/2/8×normal/continue/break/return/false×
双入口/on-off共180执行组合已绿，只有local normal观察开启发产品。uses逐实际
AST call、同catalog Begin/loan、CallReturn与递增cursor验证，coverage始终Partial。
CI-only真实完成候选变异覆盖身份/边界/动作/回交次序等，只编译不执行。

本片最终验证：10个FE集成目标共12测试及180组合通过，旧CG普通88测试通过；
CI-only组16故障仅编译。Clippy question_mark/clone_on_copy以等价写法修正后，
FE/CG全目标Clippy、workspace all-targets及默认lib no-run均通过。
fmt/diff/docs与相对2beb5592尺寸门禁通过（2561个手写Rust文件、36个既有超千行项），
尺寸policy未改；独立审查确认实际回交侧记录完整、顺序严格，不提前给CG验收资格。

## 局部循环独立CG运输证据（实施中）

1aed13e4的FE Partial来自local literal owner、borrow binding、dynamic while三语句，
旧CG只准入外部参数binding+while两语句。真实普通红先证FE lineage及各consumer
CallReturn存在，再证rooted/memory和观察on/off四种lowering均UnsupportedNode，
原final verifier未运行；不是用编译错误或注入故障代替红。

执行准入独立于观察开关：仅顶层非泛型function三语句、val纯文本String、真实root
symbol及类型、动态非常量Boolean Name、非空直接consumer序列；新片不扩常量、
jump、nested、多根或循环内新借用。旧两语句能力保留。独立receipt额外绑定FE
Partial和实际producer身份，不能提升Complete或拿它代替L0164/旧见证。

CG receipt实证补正：首次严格producer正控在local声明处InvalidModel，定位为原
literal临时槽→binding槽真实Alias。未删除原Alias或泛放别名；分别保留两槽、exact
声明span、唯一entry别名事件，并由emitter actual Alias回调证明同一冻结Value。
仅此别名成立后，RootPlace/sourceBegin/BorrowCall和后续活owner运输可接续。

首片receipt再限定单Borrow Boolean参数与普通callee：own Boolean条件、额外own
String参数各有真实on/off执行对等正控，保留额外Drop但不发新receipt。print/text
primitive不以DirectCall角色伪装。真实CG四项普通测试全绿：consumer1/2/3/8/17
双入口开关共20正组合、12类邻界共48组合，以及ownflag/额外owner各4组合；
正控核8+n实际指令角色、4条实际CFG边、各参数实体、原verifier一次及LLVM对等。

本片最终门禁：CG普通92测试通过；frontend/codegen全目标Clippy、workspace
all-targets、两crate默认lib no-run通过。fmt/diff/docs912通过，尺寸门禁相对
2beb5592通过（2570个手写Rust文件、36个既有超千行项），policy未改。独立审阅
覆盖真实Alias、冻结实体图、条件参数、fold与verifier后发布。新增4组ignored
CI故障只编译未执行，CI需显式include-ignored才运行，不将编译成功记作故障验证通过。

## 循环运输证书自包含与每轮no-carry合同复核（实施中）

54e2a400已在真实producer/emitter校验，但receipt重放仍依赖临时Plans/current，
finish只枚举已映射块内的指令，尚未独立拒绝证书外函数块。本片先补脱离Plans的
有限CFG运输验证：保留实际producer输入、Alias与fold，核whole-function集合，
独立传播owner/source/result实体角色，区分entry建立、body回边保持与false退出。
它证明现有外部lease的有限图运输，仍不把FE Partial升级为完整循环生命周期。

每轮新borrow的下一必要合同经两套checker复核：两处borrow_binding guard均按
While/For/Loop span拒绝；ActiveLoanOwner仅静态Call/Binding身份，merge_state的
loan并集/binding交集不等于迭代实例证明。check_maybe_loop在一次body遍历后合流，
既有check_loop_backedge检查owner move/closure，不能代替普通loan闭合。

真实可用核心闭合点是end_last_borrow_uses：实际remove binding/nonowning/loans
并append End；restore_borrow_scope只静默清除局部项，不能以它之后的空集合推导
已执行End。未来no-carry证书必须在该静默清理前核实际closure，且在merge之前核
normal回边恢复entry外部义务。planner的remove binding/finish transport/End可作
后端因果输入，但不能代替observer-off核心合法性证明。

最小候选仍为单层dynamic while、稳定外部Borrow String、每轮一条borrowval与
非空同步Borrow-Unit消费者、无jump/别名/逃逸：header无当轮token，实际acquire/
return/lastEnd绑定loop frame，normal回边无当轮残留、false零轮不造loan。
两套checker须同合同，CG另证body新结果End位于回边前；不以当前外部carried lease
receipt充当每轮fresh实例。该候选不新增语言语义，仍未启用或移除L0164。

真实红已成立：先把原finish“映射块内指令集合”检查等价抽成validate(function)，
不是空stub；实际lower+原verifier一次+receipt捕获后，脱离Plans验证原Function
成功。克隆该Function并通过真实add_block/set_terminator追加完整Unit Return块，
旧子集检查仍接受，在“必须拒绝额外块”断言失败。不是缺API编译错误、缺terminator
占位或未经过原pipeline的假receipt；生产原verifier与语言准入均未放宽。

独立图验证已补whole-function块、global指令、定义元数据、terminator及可达集合；
仅保留真实producer原边/fold与实际Alias实体seal，不复制Plans，不读Slot/current。
角色从真实entry定义建立，有限单调传播至固定点后核全部incoming。独立审查指出
“同Role的entry旧实体”仍不能替代本块参数，已补每block唯一角色实体、每操作与
每出边精确身份核验；header/body/finished/exit的三资源义务均不允许重复或缺失。

真实覆盖红已绿，五项local普通测试及原新CG合计93项全绿；counts1/2/3/8/17的
所有receipt均可clone后独立validate原Function，不传Plans。Clippy与workspace
all-targets通过，docs912与尺寸相对2beb5592通过（2576个手写Rust文件、36项旧
超千行债务），policy未改。新增4组ignored自包含故障只编译，不执行，覆盖角色
错配、同角色旧实体偷换、Alias/fold缺损及定义/覆盖损坏。

默认lib no-run首次因写incremental query-cache报磁盘无空间失败，非代码断言失败。
保留全部数据、未cargo clean；同target仅设CARGO_INCREMENTAL=0重试，两crate
默认lib no-run于1m42s通过，未变更其余配置。初始失败和成功重试日志均保留。

## 每轮no-carry双checker结束点对照（研究中）

本片不解除L0164、不发布新loop capability。既有unit_loop_borrow_boundary的
rooted/memory属于同一compilation-unit checker的两种入口，不等于旧file-local
checker的独立验证。新增对照必须分别运行两套name/type/ownership链，不能只把
unit入口改名为双checker证据。

同一最小body的循环外普通Block只作闭合原语正控：必须从真实BorrowBindingFact
定位source loan，并核唯一BindingEnd为末consumer的AfterStatement，位置早于
Block退出；不能只检查最终loan集合为空。对应dynamic while及嵌套lexical Block
仍要求精确borrow marker的L0164、无伪造binding/End。正控不证明每轮实例或backedge，
也不将普通Block证明移植为循环准入。

实际普通对照通过：1/3消费者×3种body，共6个file-local执行与24个unit执行
（两布局×观察开关）；原ownership_borrow_result 12项和loop boundary 1项也通过。
本片仅新增测试与文档，没有生产行为变化，故不声称一次产品红绿修复。定向Clippy
-D warnings、fmt/diff、docs912通过；尺寸相对2beb5592为2577文件/36项旧债且通过。
独立只读复审通过。未运行全workspace或CG回归（生产未改），未执行CI-only故障，
未推送；下一实现仍需scope restore之前实际frame闭合及回边义务核验。

## 普通scope frame闭合与外部义务恢复基础（实施中）

本片继续保持循环内L0164，以两套core checker真实普通Block建立研究证据，不把
循环外闭合等同迭代/backedge证明。研究collector仅test/显式test-support编译，
不接旧loan_action产品，不改变默认语言准入、错误、清理和Complete边界。

采证必须来自实际执行：进入scope的外部状态、实际成功binding交接、实际最后使用
移除的前后状态及原AfterStatement End；在check_elements返回后、任何scope退出
清理前封闭。只有本frame非空acquire与一一真实End、正常出口及外部义务恢复可
形成研究证据。外层仍live的binding及source loan必须保留，不能以同根loan统清
误作恢复；scope cleanup造成的空状态不能替代真实结束事件。

首次正控因unit closure_results在调用后增加空集合缓存而结构不等；读取该缓存
对缺项取空，故仅在研究快照两端去空缓存（真实State不改、非空义务不丢），不把
该非目标失败称为产品红。研究资格另严格限制Name直接callee、String Name实参，
拒复杂capture/replacement/pending/nullable/provenance入口；不以OriginState的
共享arena指针相等声称整个可变图的深层证明。

目标真实红随后成立：合法普通Block先关闭借用再take(own source)，原ownership
无诊断且End齐全，外部moved却改变；旧End-only研究见证validate仍true，被“必须
拒绝外部未恢复”的断言抓到。closed与外层live-binding正控以及L0164负控先通过。
这不是语言误接受或空stub，而是新研究frame恢复合同不能只复用End见证。

外部恢复守卫补强后，研究认证要求closed且external_unchanged；实际合法source
move不改变ownership输出。独立审查通过frame唯一身份、真实移除差分和cleanup前
接点。资格再排除非Unit consumer返回、borrow-return消费者及Abort，避免将未纳入
State的丢弃owned临时量清理混入首片；initializer仍限String借用结果。未显式capture
不复制entry/exit或acquire loan Vec，不改默认dependency graph或旧诊断。

最终普通测试16项全通过（新增frame2项、既有borrow result12项、两项loop边界/
双checker证据）。新片含40次实际capture checker执行及20次未capture产品对照，
覆盖closed、外部live lease、同名sibling、End后合法source move，以及loop门禁、
非Unit返回和嵌套调用资格负控。frontend feature all-targets Clippy -D warnings、
默认workspace all-targets、fmt/diff/docs912通过；尺寸相对2beb5592通过
（2579手写Rust文件、36项历史超限，policy不改）。未执行CI-only故障或CG/native
运行测试，本片无CG改动；默认配置及显式feature的编译路径均已核。未推送，
本片不是循环backedge证明，也未退休旧见证。

## 已有外层借用的真实回边义务观察（实施中）

本片沿既有循环研究授权，不改变Guide、L0164或默认检查器算法。两套checker在
condition之后取得实际base，body检查和旧回边诊断完成后、任何origin图回连及
merge_state之前，分别观察normal/continue/break。base不是条件前状态，故不声称
条件求值保持；body已过scope restore，故仅允许无声明、无嵌套、无条件分支的
非空直线Unit消费者，可末尾唯一continue或break，绝不据清理后状态证明fresh实例。

入口要求非空真实carried binding及其place/nonowning/对应loan，观察区间不得新增
binding或End。normal和continue按真实边类别分别核入口义务投影保持；break只记录
退出，不得作为重复边认证，return不凭缺失状态补证。动态非常量Boolean参数Name为首片条件
边界，常量条件两checker可达性不同而不纳入。现有普通Block frame与CG循环图证据
仍各守独立边界；本片不是任意分支逐路径证明，也不是循环内每轮acquire/End。

共享frame快照/直接调用资格归入两checker私有research适配模块；single循环流helper
整体归control，与旧backedge诊断相邻，执行顺序不变，checker.rs降至1352行。
Unit dataflow.rs只新增受cfg保护模块注册，仍在原额度内；尺寸policy不改。

首次编译两处Option<UnitSymbolId>误用copied已修，此不计行为红。随后真实双checker
正控指出Unit无观察而single已通过：误把仅lambda参数事实body_parameter_mode用于
普通具名函数。已改为真实callable参数binding目录与source-qualified名称解析ValueParameter
身份相交，不放宽成任意Name。local Boolean名、常量条件仍不给研究资格。

最终新增4项普通研究测试通过：18组源码分别执行single及unit双布局/观察开关，
共90次capture与90次未capture产品对照，涵盖1/3消费者、normal/continue/break、
return/条件跳转、空入口/fresh门禁/嵌套/局部名/常量，以及双既有来源、兄弟循环
不同票据。消费者实参均Borrow/非跨线程且引用现存binding，不声明独立逐call return
证据或动态任意迭代证明。新旧相关普通测试共241项通过（含for/while 219项），
frontend feature all-targets Clippy -D warnings、默认workspace all-targets、fmt/diff、
docs912及尺寸门禁通过（2583手写Rust文件、36旧超限），独立最终复审通过。
未运行CG/native和CI-only故障；仅本地提交未推送，L0164与旧见证完整保留。

## 已有循环借用逐consumer真实返回关联（实施中）

在前片非空既有carried loan研究范围内，回边状态相等不再充当每次调用返回证据。
两checker在实际call-owned loan移除前后取证，随后原closure/operand清理及Nothing
判断完成，仍有正常next且无诊断/分叉才交付非Clone释放票据。每operand先按真实
source-qualified call/argument ID核唯一原LoanFact，再一一核active Shared loan、
canonical place及origin，不能把同owner的不同调用或同call不同实参合并。

独占loop invocation/session票据保存按AST真实consumer顺序的返回记录；每次只移除
当前call loan的预期状态必须等于入口义务投影，实际移除后和最终normal completion
再核一次。回边认证同时要求原状态保持、完整有序返回集合及无未完成票据。
此处不补造原End、不改变source_origin交接例外，break仍非重复边；默认cfg-off不
接观察。既有L0164、旧检查器和见证保留，不证明fresh实例、复杂分支或CG每轮receipt。

真实缺证据红：保留原有效边认证、先建立尚未连接调用释放接点的事件tape，普通源码
的normal/sibling/two-root三项在实际consumer_returns为0、期望1/2处失败；非法/
fresh负控仍通过。这是新研究合同缺真实事件的断言失败，不是编译错误或语言误接受。
接入真实释放前后与正常完成接点后原四项普通测试通过；完整验收另记下方。

最终普通矩阵扩至1/3/8消费者，并覆盖同call同binding双operand与两binding双operand，
逐实参核原AST/解析symbol，兄弟loop事件不可互认。5项研究测试含125次capture与
125次未capture产品对照；新旧八个定向target共242项全部通过。两组CI-only真实
fixture故障（重放/错序/异源/漏release/漏complete/异loop事件）仅feature lib no-run
编译通过，未执行，不将故障断言称为本地验证。显式SourceId/SourceUnitId防御及
最终完整tape检查经独立复审，无阻断。FE feature all-targets Clippy -D warnings、
默认workspace all-targets、fmt/diff/docs912全部通过，尺寸相对2beb5592通过
（2587手写Rust文件、36历史超限；policy不改）。父文件仅cfg注册与真实接点，
traversal为998行、checker/dataflow保持既有额度；未运行CG/native，未推送。

## 普通frame的acquire/consumer/End完整顺序基础（实施中）

本片保持现行L0164与普通frame原validate合同；新增独立ordinary closure研究证据，
不将其称为循环Complete。原frame已有实际acquisition、最后使用End及cleanup前
外部义务恢复，但尚未统一证明中间每个consumer正常返回与同frame/同binding顺序。
固定loop entry的投影不能用于新frame consumer：新binding在entry不存在，consumer
时必须仍保留该真实lease；不得删去它来伪造状态相等。

研究目标按真实执行链接acquire、逐call实际暂借移除/正常完成与最后End，精确
call/operand/source/binding/frame身份，End前核该binding全部声明consumer返回，
多binding交错不能仅比较三个无序集合。初始化borrow-return仍按原source交接，不
当作普通Unit consumer；最后完整关闭才比较外部entry义务。只复用现有真实核心
接点，不修改诊断、observer-off产品、scope cleanup、旧见证和尺寸策略。

普通frame整链即使通过，循环header零实例、body acquisition归属、回边唯一消费
闭合票据、零迭代路径与后端每轮receipt仍须另证，不从循环外成功推导准入。

新增研究接口complete_closure独立组合旧validate与逐事件tape，原validate不因新证据
缺失而变动。声明计划保存真实名称解析binding、原ordered argument ExprIDs及直接
statement；每次acquire和consumer正常返回各推进一次，consumer必须仍持同frame
真实acquisition快照；End核未来无该binding使用且对应最后真实返回的statement。
每个consumer移除后与final normal next分别核完整预期状态，不以固定loop entry
消去新binding；loop与frame票据独立，未capture不复制新状态。

真实缺证据红为原普通闭合validate通过、完整研究complete_closure失败，原因是
尚未接实际consumer返回事件。此前Expression导入错误不计行为红。接线后首轮
3项新测试通过、1项失败，是测试误要求继承-only空frame发布；既有finish明确
过滤acquired.empty，故修测试为所有观察无完整资格，保留产品行为，未扩资格。

最后5项新普通测试通过：1/3/8消费者、同call同binding双实参、同根多binding交错、
兄弟frame隔离、外层parent存活与inner新binding、继承-only及门禁/复杂/外部move
负控。12组源码各执行single和unit双布局/观察开关，共60次capture+60次原产品
对照；与旧frame/loop三target共12项通过。两组CI-only真实fixture故障仅编译，
未执行，覆盖重放、同binding原operand错序、异源、漏release/complete及异frame；
故障仍要求旧validate有效而新整链拒绝。独立最终只读审查通过。

最终九个普通定向target共247项通过。Clippy首轮仅指出consumer声明tuple过于复杂，
已抽取等价ConsumerPlan名称而非豁免；修复后12项直接回归、FE feature all-targets
Clippy -D warnings及默认workspace all-targets全部通过，独立复审确认无行为变化。
fmt/diff/docs912及尺寸门禁通过（相对2beb5592，2590手写Rust文件、36旧超限），
policy未改。并行编译期间只读确认进程活跃和磁盘空间，不删缓存或中断其他工作。
未执行CI-only故障、CG/native测试，未推送；完整语言准入与每轮回边证明仍未授予。

## 前序普通闭合至真实零资源while header（实施中）

本片不是循环内fresh借用或每轮闭合准入，而是前序普通frame已完整闭合后的真实
零资源边界研究。只有同一真实父frame内紧邻的空body dynamic while，才能在入口
一次性消费前序非空ordinary complete_closure；不将这张入口票据称为body回边票据。
独占身份来自实际frame栈与checker/source域，不靠事后扫描两张公开观察凑证。

真实header必须在原While分支origins.attach后、begin_loop与condition求值之前采集；
condition后base另核相同义务与无binding/active loan，旧回边诊断后、merge前独立
核唯一normal边保持，区间不能新增binding/End或诊断。首片仅Boolean参数Name和
空body，拒常量/调用条件、break/continue/return、新声明与嵌套；空body不能冒充
已经证明非空consumer或新acquisition。此前闭合只在入口消费一次，兄弟loop不得复用。

后续必须另证：循环header没有本轮实例、body acquisition隶属真正loop frame、
cleanup前完整闭合票据由实际回边唯一消费、false零迭代不建实例，以及独立CG
每轮acquisition/End。当前L0164、旧产品、旧见证和尺寸策略均不改变。

真实红：已具备实际ordinary frame闭合和Ready生产、但尚未接While header接点时，
合法源码的names/types/ownership及capture产品对照先通过；新边界观察为0而预期1，
普通断言失败，非编译错误。接入实际condition前header、condition后base和normal
pre-merge取证后，新增3项普通测试与旧frame/consumer/carried共15项通过。
15组源码执行75次zero capture与75次原产品对照，正控另10次真实frame完整闭合
验证；覆盖票据单用与兄弟loop隔离、无前序闭合/非相邻/外层live lease/非空body/
常量或调用条件/嵌套/跳转/fresh门禁负控。未将空body正控描述为每轮新借用。

Unit原check_loop_backedge与check_maybe_loop完整迁入loop_control职责模块，保持
调用时序；独立去空白逐token比对两方法与父提交相等。traversal从998降到887，
checker为1358且在原额度内，policy不变。新增2组8项CI-only故障仅feature lib
no-run编译通过，未执行，覆盖缺header、错父checker/session/checkpoint、缺condition、
错loop身份、缺normal边及非法出口类别。独立静审通过实际父/源/session票据链、
失败封闭和旧carried协议保持；各阶段只消费实际token，不接受公开观察拼接。

最终十个普通定向target共250项全通过；FE feature all-targets Clippy -D warnings、
默认workspace all-targets、fmt/diff/docs912全部通过。尺寸相对2beb5592通过
（2596手写Rust文件、36历史超限），策略未改。独立最终复审还核CI故障仅cfg(test)、
thread-local/RAII恢复及ignored边界；未执行故障或CG/native。仅本地交付，不推送，
不宣称empty body证明了fresh acquire/End、非空body事务或任意动态迭代归纳。


### 空 loop body 的实际 frame 生命周期研究

延续前序普通闭合至空 while header 的独立观察，不扩大既有空 body 与动态 Boolean
参数资格。condition 确认资源保持后产生非 Clone permit；两个 checker 在实际
body scope frame begin 后认领，并在 check_elements 返回后、原 frame.finish 与
scope cleanup 前封闭真实 frame。核实际 parent/frame/session/loop/body 身份、
frame 有效性、零 acquire/End、唯一 normal 出口与完整资源投影及事实检查点保持。
normal pre-merge 独占消费出口票据并核 cleanup 后实际状态，不从清理后为空倒推 End。
当前 loop 完结无论新证据成功与否清理两槽，异常退出仍由 Guard 清理，避免污染兄弟。

旧 validate 不变；body_complete/body_frame 只提供更强的显式研究观察，默认产品
与 L0164 不变。此前普通 closure 只用于一次入口，不作为本轮 acquisition/End。
本片仍是空 body 的静态 checker 生命周期，不证明运行时迭代、非空消费者、fresh
循环借用、false 零迭代实例或独立 CG 每轮 receipt，也不退休旧见证。

断线前红尝试实际为 Capture 初始化缺字段编译错误，不计行为证据；恢复后修正初始化，
普通合法源码的产品对照、真实前序 complete_closure 与旧 validate 先通过，再在
body_complete 断言失败，取得真实行为红。之后才接实际 mint/enter/close 接点。

接点完成后新3项普通测试与旧15项通过；新10组源码覆盖50次capture、50次原产品
对照，正控另10次真实前序frame capture。覆盖实际body身份、兄弟不互认、非空/
fresh/常量/跳转与无前序闭合拒绝。2组8项CI-only故障仅feature lib no-run编译
通过，未执行；故障只破坏首个兄弟，要求旧validate保持且第二个body成功，覆盖
缺claim/close、错session/loop身份、入口检查点、frame checker与出口检查点等
隔离边界。独立生产及测试终审通过。

最终11个普通target共253项通过；FE feature all-targets Clippy -D warnings、默认
workspace all-targets、fmt/diff/docs912通过。尺寸相对2beb5592通过（2599手写Rust
文件、36历史超限），policy不变。未执行CI故障或CG/native；仅本地提交，不推送。


### 非空外部参数 consumer body 的实际 call-loan 研究

独立 capture_borrow_consumer_bodies/BorrowConsumerBodyObservation 不扩大旧
zero-loop 空 body 入口，也不改变已有 carried/local BorrowBinding 适配器资格。
仅动态 Boolean 参数 Name、非空直线普通 source Unit 调用，各 operand 为真实外部
Borrow String 参数 Name；Unit 还要求源域限定 ValueParameter，不能只凭 Shared
目录把迭代 binding 或 receiver 当成参数。拒绝新 binding、分支、跳转、嵌套调用、
borrow-return、abort、非参数 operand 和复杂资源入口。L0164 不变。

证据从 condition 后真实 body-entry 起，不称 condition 前 header。两个 checker
分别在原 apply_argument_contract/apply_contract 前后对实际状态取证：每 operand
恰新增一条 Shared Place call loan，与原 call/argument LoanFact、canonical place
及 origin 一一对应，Unit 排除 reserved。两边原 fact/active 插入顺序不同但均
包围完整原方法；同 root 双参数仍是两个独立 operand，不按 root 合并。
raw LoanFact 游标从入口连续推进，最终核整个未过滤增量等于全部 operand 数。

原 call-loan cleanup 前准备、后核精确删除仅本 call 贷款，原清理及 Nothing 判定后
才确认正常返回；各阶段复核实际 frame、session、loop，按原 call/operand 顺序推进。
cleanup 前真实 body scope 封闭，再由 normal pre-merge 单次消费完整出口，最终
独立复核返回事件集合、唯一性、来源域和计数，失败不得污染兄弟；计数采用checked
运算。有限源码声明驱动，不枚举固定 consumer 数；不声称所有核验均为线性复杂度。

实际合法正控的类型、ownership 与原产品对照先通过，再因未接实际事件观察0≠1
取得行为红；之后接入真实 acquisition/release/return。它证明普通参数暂借的静态
body路径，不证明 fresh循环实例、运行时归纳、condition前header或CG每轮receipt，
不退休旧见证，不改变策略。

新4项普通测试覆盖19源码×5 checker/配置（95次capture及等量baseline），另真实
rooted双源×observer开关2次对照；故意同AST索引但不同SourceUnit的事件不可互认，
同根多operand、1/3/8消费者及兄弟loop均核。多源fixture最初错误请求memory多源，
在index阶段报MemoryDocumentRequiresSingleSource；仅修为受支持rooted双源，
单源memory覆盖保留，生产与构造器未放宽，原失败日志保留。

新增2组9项CI-only故障仅feature lib no-run编译通过，未执行；首兄弟故障后第二
兄弟恢复，漏acquire/释放/正常返回、原fact计数和身份错配失败封闭。独立生产、
实际接点及最终测试复审通过。13个普通target共257项通过，Clippy三处风格项
以等价控制流修正，随后新4项重跑通过；FE feature all-targets Clippy -D warnings、
默认workspace all-targets、fmt/diff/docs912与尺寸门禁通过。尺寸相对2beb5592为
2605手写Rust文件、36历史超限，policy不变。未运行CI故障或CG/native，仅本地提交。

## 已批准有限 loop-local no-carry 核心实施（2026-11-12）

状态：approved／in-progress。用户对“修改 Spec 并实施有限 no-carry”合同明确回复
“是的”：稳定外部 source，循环体内直线同步借用且不逃逸；每条正常回边前真实结束
本轮全部 loan 并恢复外部义务，零轮和退出单独验证，两套 checker 在观察关闭时也
执行相同检查。分支、跳转、嵌套和多根暂不开放，旧见证保留至同合同完整。
本授权推进此前未启用的实现边界，不改 Guide 的借用语义，不接受条件 from 并集、
跨轮携带或动态 capture。此前“本轮未授权”账本仍是其当时事实，不追改历史记录。

### 最小实施边界

首片限定单个稳定外部 Borrow String 参数来源、动态 Boolean 参数 Name、单层 while，
body 为一条直接普通借用结果 binding 及非空直线同步 Borrow-Unit consumer。不得
逃逸、保存别名、混入新 owner、第二来源、分支、jump、嵌套控制或其他未证事务；
不满足范围的已有合法源码保持原边界，未支持 fresh 路径仍 L0164，不全局删除门禁。
核心资格不得依赖可关闭 observer 或 test-support；两套 checker 均须真实消费相同合同。

真实 condition 后 body frame 的准入只授权该精确声明；实际 source-call loan 交接、
binding acquire、各 consumer 暂借/return 及末使用 End 必须关联本 frame，不能由
语法扫描补造事件。在任何 restore_borrow_scope 之前证明全部本轮 binding/loan 已
真实结束，正常回边再核恢复完整外部义务；不可从 cleanup 后为空倒推 End。
零轮/condition-false 边不建立本轮实例，正常退出保持外部 source；一次静态检查只
在已证入口不变量和回边恢复条件下构成有限合同归纳，不宣称覆盖任意循环或路径。
失败须按现行诊断和全局原子撤销约束阻断成功产品；兄弟 frame 不能复用许可或票据。

后端另核实际 body acquisition/consumer/End、normal 回边和 false 出口，保持原
SSA verifier、ABI 与唯一 emitter；FE 通过不等于 CG 通过，也不得以现有外部 carried
lease receipt 替换本轮结果关闭。只有相同实际合同的完整证据成立后再单独审退休。

### 验收与提交

先真实合法源码因旧 L0164／lowering 缺能力失败，再实现至双 checker、Unit 双入口与
observer on/off 产品及诊断对照成立；逐一覆盖 normal、零轮、源退出后可用、连续
迭代结构及兄弟循环隔离。分支/jump/nested/multiroot/alias/escape 等邻界必须保留
拒绝，原 ordinary、carried、zero/consumer-body 合同不回退。跨 source/分析身份、
遗漏 End、错 frame、污染外部义务等破损/注入测试遵守 CI-only，只本地编译不执行。
FE、实际 SSA/LLVM/native 与 receipt 分账；定向回归、Clippy、workspace、fmt/docs/
size/diff 及独立复审分别登记，不并发争用 Cargo target，不改尺寸策略。分步本地
提交，不推送；在真实完成前不把本合同记为已通过或将 D10/E2 标为完成。

### 有限 no-carry 核心与实际 lowering 落地（2026-11-12）

本地实现两套非 cfg Frame/票据协议：真实 acquire、逐 consumer 暂借返回、原末使用
End、cleanup 前恢复及正常回边消费。Unit 私有同实例资格再核原 binding/End，错误
全局撤回；observer-off 同样执行。资源快照按职责从 research 移入 resource_state，
不改空缓存归一化以外的义务。直接函数 body while 外的控制形状保留 L0164。

默认无 feature 合法源码先因原 L0164 真红，再实现为绿；CG 同一合法产品先在原
UnsupportedNode 真红，再接精确资格、实际 body 槽恢复和原 verifier。编译修正及
五个旧 ordinary 研究 fixture 的新合法预期迁移另记，不算行为红测试；owned source
等原拒绝断言保留，未以宽泛 fresh 标签放宽边界。

FE 15 个普通 integration target 共 260 项通过，含 1/3/9/17 consumers、own Boolean
只读条件、兄弟隔离、诊断全局撤回及 14 个有效类型负界。CG loan_region_ssa_tests
普通家族 93 项通过；专门的 SSA 矩阵和 false 原生执行曾独立通过 2 项。实际 source/
consumer 身份逐边断言补强后 SSA/native 两项复跑通过；CI-only 两组六故障仅编译，不执行。

本实现尚不是独立每轮 CG receipt，不退休旧见证。条件固定 true 在此无赋值/跳转
合同下自然无限，不能伪造可终止一轮/多轮 native 样例；false 原生执行证明退出后
仍能消费 source，实际 SSA 证明正常回边恢复。剩余复杂控制流、多根与跨轮合同另议。
当前实现事实与责任入口见 [循环内 no-carry](../../architecture/loop-no-carry.md)。

独立复审核核心生产身份/关闭/撤回与CG边界无阻断，并补 consumer exact result、
source 从入口经header/body/回边的实际参数映射断言。FE+CG feature all-targets
Clippy、默认workspace all-targets通过；fmt/diff/docs913与尺寸门禁通过。尺寸相对
2beb5592为2618手写Rust文件、36历史超限，policy及Guide未改。最后CG断言补强
再作Clippy复核通过；只本地保存，不推送。

### 独立 fresh-loop CG receipt（2026-11-12）

延续已批准有限 no-carry，不扩大语言。独立协议不复用 carried-loop 的入口 acquire/
出口 End：首资格仅函数体一个 while、Borrow source/flag 两参数；own Boolean、兄弟
循环及前后语句保留原 lowering 的执行/拒绝边界，无此证书。参数位置按 typed ABI 解析。

真实 producer 捕获 header ReadLoan、body BorrowCall、逐 consumer、唯一 result End、
原五块 terminator 及实际 fold；Emitted 将标签绑定原 emitter 的真实 ID 和参数映射。
独立最终图复核从 ABI source/flag 传播角色，逐 incoming 对证局部实体、完整函数全集、
body 结果只在本轮使用/关闭，不随回边传出；false 路径没有 fresh 操作。单函数暂存
后仍须整 unit 原 verifier 成功才发布。FE 资格封印的身份与元数据必须对应原准入，
不能用另一次等值重分析事实替换；不冒充最终图复核重新验证整个 FE catalog。

实际缺 receipt 红测试先验证原 lowering/verifier 成功，再发现 receipt 为空；随后
首次真实普通绿通过。最初短名配 --exact 导致零测试的命令不计红，编译缺 receiver
字段也不计行为红。独立复审进一步要求同实例身份对证及 cleanup 前独立 phase 3
封口，按此补强后再作最终验收。CI-only 故障只编译，旧见证不退休，不推送。

独立复审修复两项：新增只读 Rc::ptr_eq 身份 API，真实 layout 将原准入 fact 封存入
私有 Proof，最终同时核身份和完整 metadata；phase 3 前移到实际 leave 前，避免
只依赖旧 lease 的 cleanup 前验证。ordinary 首正控含正反参数顺序、1/3/9 consumer、
双入口与 on/off 共24组合通过。

新增邻界发现兄弟两 while 的 FE 事实存在，但旧 CG 第二 while 仍 Unsupported：
`unit_lower/borrow_result` 每个 binding 仅许可自己的 while，而 borrow_result_support
检查完整函数所有 while。这两处本轮无修改；保留精确原拒绝，不用 receipt 顺带放宽。
CI编译修正原 verifier 模块路径及测试 Debug/type 路径，不记为行为红。

新增接线使 container_for 达1009行触发原尺寸门禁；将完整 while CFG/lifetime boundary
方法等价移至独立 `build/while_loop.rs`，容器循环877行，新模块143行，build注册999行。
不压缩格式、不改policy；迁移后继续最终回归。CI六组只编译，publication rollback
使用真实原verifier错误及guard撤回单测，不冒充实际orchestrate故障注入。

最终验收：新CG两个普通测试（正控24组合及三个邻界）、FE两个target共3项、CG
loan_region家族94项通过；CI六组只作feature lib no-run。Clippy发现重复entry/body
分支，等价合并条件后FE+CG feature all-targets -D warnings通过，并重跑新CG两项；
默认workspace all-targets通过。模块过滤未包含native函数，因此另显式执行真实
false原生测试1项通过，未把空过滤或缺案例当作通过。fmt/diff/docs913、尺寸2626
手写Rust文件/36历史超限通过，policy未改。独立身份修复及while职责迁移复审通过。
此片只本地提交；不声称一般循环、多根、终止性或旧见证退休已完成。

### 已批准同源直接兄弟循环准入（2026-11-12）

前序试改因缺专项授权被权限审核阻止，生产未执行，仅保留普通测试真实红。
用户现直接同意：“同一函数体内的直接兄弟 while，各自在本轮创建并完全关闭借用；
仍不开放分支、跳转、嵌套或多根”。因此只扩同一稳定外部 source、同一 callable 的
直接兄弟 no-carry while；每轮仍各自取得、使用、真实 End 并恢复外部义务。

新增门必须由当前 binding 自身的同 ownership 封印事实启动，目标兄弟也须独立拥有
准确 binding/source loan/initializer/body/唯一 End；两者在真实 Function body 的直接
elements 中且 source/callable/source unit 一致。不能仅凭某个目标有事实就给 ordinary
或 carried binding 放行，不接受不同 source、嵌套、分支或未封印控制节点。

逐循环保留既有 NoCarryLease，实际 SSA 结果不跨本循环回边或进入兄弟循环，false
退出保持 source。现独立 fresh-loop receipt 仍仅 whole-function 单 while 资格；本片
若只完成兄弟执行，不宣称已发布兄弟组合 receipt，不退休旧见证。
复用前序第二 while Unsupported 真红，增加2/3兄弟、独立结果/End、双false原生及
不同 source/未封印控制负界；CI故障仅编译。本地分步提交，不推送。

实施采用精确 peer StatementId 集合而非全体 while 白名单。双方独立admitted、同
owned/callable/source、真实直接子节点；ordinary/carried无自身资格时集合为空。
独立静审通过。原Unsupported真红后，同source 2/3兄弟的真实SSA正控及原单loop
正控转绿。负控发现外部ordinary binding混入在FE即L0164，按真实阶段独立记录，
不修改FE或强造CG反例；不同root与未封印peer保留原CG拒绝，失败日志保存。

强化后五项普通测试通过：新2/3兄弟SSA共8组合，两个CG拒绝邻界8组合，三个FE
精确L0164边界12组合，均含双入口/on-off。独立复审补强actual BorrowCall source须
为body自身唯一source角色参数，CG阴性准确核第二while错误span。双false真实
object/link/run输出kept，未冒充非零多轮或组合receipt。相关CG家族97项与FE三个
target共4项通过，feature all-targets Clippy -D warnings通过；未新增或执行CI-only。
默认workspace all-targets及fmt/diff/docs913、尺寸2627手写Rust文件/36历史超限通过。
policy未改，旧见证保留；本地提交实现，不推送。

### 同源兄弟的动态组合 receipt（2026-11-12）

在已批准兄弟执行合同内扩展独立证据，不扩大语言。首组合资格限函数体全部为同源
直接 while，条件共享一个 Borrow Boolean 参数、另一个参数为 Borrow String source；
数量取真实有序元素，不枚举固定2/3形状。不同Bool参数、own Bool和额外语句保持
原执行/拒绝边界，无本片组合证书。

复用 fresh-loop 协议，动态有序 peer 各自封存原 fact 身份、实际 acquire/consumer/End。
原 term/fold 使用整函数账本：前一 exit 是下一实际 entry，只有最后出口 Return；
标签携 peer 身份，不把单loop五块/末Return规则机械复制。最终整图覆盖只做一次，
核每peer局部关闭、共享source/flag实际逐边身份、精确peer顺序及所有结果不出边。
整组随原 unit verifier 成功才原子发布，任一peer失败撤回，不先发布部分兄弟。

真实红已确认：sameflag两兄弟lowering和原verifier成功，但组合receipt为空；在此
基础实施，不把编译失败计红。计划正控1兼容、2/3/5动态序列、正反ABI和consumer
变化，differentflag保留合法无receipt；CI故障只编译，本地提交不推送。

组合实现与完整独立复审通过：逐peer身份/metadata/角色标签，原边内容、共享出口、
真实fold顺序与唯一前驱，以及整函数集合覆盖均独立核对；fold先删除消费的Goto，
再重写后继entry，未从最终SSA补造历史。首次编译修正空slice类型推断与CI Vec
索引取值clone，不记行为红。真实green2五普通通过：sameflag 2/3/5、正反ABI、双
入口/onoff共24组合发一个组合receipt，逐peer身份/fold数量/实际链接并clone后
独立validate；异flag另24组合执行无receipt。旧单loop正控和阴性边界保持。
双false原生1项真实输出kept；8组ignored故障仅lib no-run编译通过，未执行。
最终CG家族97项、FE三个target共4项通过；FE+CG feature all-targets Clippy -D warnings、
默认workspace all-targets、fmt/diff/docs913及尺寸2627手写Rust文件/36历史超限通过。
独立生产、最终图、测试和文档复审通过。未改尺寸policy，无旧见证退休，本地提交。


### 不同 Boolean 条件的兄弟组合 receipt（2026-11-12）

在既有合法执行范围内扩展独立证据：唯一 Borrow String source，其余 ABI 参数均为
实际使用的 Borrow Boolean，按真实参数索引区分各条件身份。函数体仍只有已封印的
同源直接 while；未使用 Boolean、own Boolean 或额外语句保留执行而不获本片 receipt。
不改变 FE/CG 执行语义、原 verifier 或旧见证，也不开放分支、跳转、嵌套或多根。

实际 producer 冻结完整 ABI 槽并在各阶段复核；每 peer 的条件读取绑定其参数索引。
最终图从 ABI 实体独立传播角色，header/body 保留 source 与该 peer 起的所有未来
flag 需求，false/exit 保留后续需求，末出口为空。共享出口集合、每 incoming 的前驱
局部实体与唯一角色都要匹配。g0,g1,g0 不提前裁掉 g0，g0,g0,g1 在第二 peer 退出后
裁掉 g0；不按同型 Boolean 合并身份。原 fact 独占性、每个真实 End、global fold 和
整 unit verifier 后原子发布保持。

真实缺证红已执行：不同 flag 原 lowering/原 verifier 成功而 receipt 数为0，对1断言
失败；日志独立保留。实现后定向五项普通测试通过：组合14个源码变体、双入口/on-off
共56次 lowering 都核一个组合 receipt、逐 fact 身份与独立最终验证；含2/3/5 peer、
正反及中间 source ABI、重复 flag 和三条件顺序。实际各 Read 与逐块活跃角色集合
另有普通断言；未用 Boolean 保留合法执行及 receipt0。旧单loop与CG/FE阴性保持。
新增3组CI-only同型 flag/read、edge及已死flag保留故障，连同旧8组共11组，只编译不
执行；entry旧flag替代本块Read也在同组检查。

验收：定向普通5项、loan_region_ssa_tests家族98项、FE三个目标共4项、native实际
编译链接运行3项均通过；CI-only 11组仅编译，未执行。双crate feature/all-targets
Clippy通过，最终格式及diff检查通过。尺寸按2beb5592基线通过（2627文件/36历史债务），
policy无修改。首次Clippy的两处测试collapsible_if仅作等价let-chain修复；首次尺寸
命令缺--base是用法错误，均保留原日志，不计作行为红。默认workspace/all-targets检查通过，文档结构检查913篇通过。
本片仅扩独立证据覆盖，无旧见证退休；本地提交，不推送。


### 未使用 Borrow Boolean 的入口消亡证据（2026-11-12）

本片仅扩独立 receipt 覆盖，不改变既有 FE/CG 执行范围。保留唯一稳定 Borrow String
source、同函数直接有序 while 和原 sealed facts；ABI 其余参数仍全部是 Borrow
Boolean，但允许没有实际条件用途的参数。own Boolean、额外语句等仍不获得本片
receipt，不开放分支、跳转、嵌套、多根或计数语义。

真实 producer 和 Emitted 继续冻结完整 ABI 槽及实体。独立 validator 对函数 entry
验证完整参数集合，首 header/body 只接受 source 与实际条件用途的角色；unused
不能进入任何后续参数、边、Read 或 End。后续共享 entry 仍与前一 exit 精确相等，
重复 flag 的未来活性不受影响。不是删去原 ABI 身份，也不为 caller 的 loan 造 End。
原 NoCarryLease、sealed fact 准入、carried-loop 证书等合同不同，均不退休。

真实红：既有 unused 合法源码已 lowering 且原 verifier 成功，但 receipt 为0，
期望1失败；不是编译失败。最小生产修复后定向5项普通全绿。组合矩阵由14增至20个
源码变体，双入口及观察 on/off 共80次实际 lowering；新增24组合覆盖 unused
位于 ABI 首/中/尾、多个 unused、三 flag 与重复用途组合，独立核入口身份、后继
精确活性、真实 fresh consumer/End、逐 fact 身份及 detached validate，LLVM 对等。
旧 own Boolean 无 receipt、前后语句及非法边界保持。

新增1组 ignored 故障，连旧组共12组仅 lib no-run 编译，未执行：unused 偷换条件
Read、proof flag index，以及同步边快照后 unused 或当前 flag 替代未来 flag。边
故障保持真实 arity/global metadata，避免仅快照拒绝的假阳性。原红绿及编译日志
保留在本片 checkpoint。最终 loan_region_ssa_tests（跳过 ci_only）97项、codegen
feature/all-targets Clippy -D warnings、默认 workspace/all-targets、fmt/diff通过。
尺寸按2beb5592基线通过（2627文件/36历史债务），文档结构检查913篇通过。
本片不改变native执行，未另跑native或FE测试；独立复审通过，旧见证与policy不变。
本地提交，不推送。


### own Boolean Value 条件的独立 receipt（2026-11-12）

仅扩既有同源直接 while/兄弟的证据覆盖，不改变 FE、CG 执行合同或原 verifier，
不开放分支、跳转、嵌套、多根或计数。唯一 String source 仍必须 Borrow；Boolean
参数按真实 typed 模式区分 Borrow Shared Loan 与源码 own 对应的 Value，包括混合、
重复用途和 unused 参数。旧 NoCarryLease、sealed fact 准入及其它见证不退休。

actual condition-use 位于原 Name 表达式求值完成、Branch terminate 之前，精确核
UnitExpressionId、span、header、阶段和实际槽，一次封口。own 直接使用 bindings
预冻 Value，无虚构 Condition/Read；Borrow 仍绑定真实 Read 结果。各阶段同时核
完整 bindings/borrowed、互斥归属、槽 EntityType、dead 与外部 ABI 身份。Emitted
保存实际 checkpoint 与 terminator 证据；detached validator 要求 own header 零
指令、条件为该块局部 Value，逐边核精确角色/实体/类型，不用最终图倒造 producer。

真实红：既有合法 own-flag fixture 原 lowering/verifier 成功后 receipt0，对1
失败。首次实现编译发现 typed 枚举实际名为 ParameterMode::Value（不是源码拼写
Own）及空 slice 推断错误，修复后保留原日志，不把编译错误计作行为红。

定向5项普通全绿。组合矩阵28个源码变体、双入口与观察 on/off 共112次 lowering，
新增8变体覆盖 mixed Own/Borrow、ABI换序、重复条件、unused own 与17次消费者。
逐项核 own 无Read、header-local Value、全参数模式/运输类型、事实身份及独立
validate；原Borrow与unused矩阵保留。mixed Own/Borrow native零轮1项真实编译、
链接、运行输出kept；不冒称运行任意非零轮或证明终止。新增1组 ignored 故障，
共13组仅 lib no-run 编译，未执行，覆盖同型错条件、入口旧Value、checkpoint缺失、
ABI模式伪换及同步边快照后的未来own flag丢失。

日志保留在本片 checkpoint。断线恢复后核原进程已结束、日志完整：家族97项、
codegen feature/all-targets Clippy -D warnings、默认workspace/all-targets、fmt/diff
均通过；尺寸按2beb5592基线通过（2627文件/36历史债务），文档结构913篇通过。
无policy变动，旧见证保留；本地提交，不推送。


### 直接外部 source 消费段的组合 receipt（2026-11-12）

本片仅扩既有执行的独立证据覆盖，不改 FE/CG 准入、控制流或原 verifier。函数内
同源直接 while 的前、中、后可有动态直线消费段，n 个 peer 对应 n+1 个有序段。
仅普通 direct、无 receiver、正常同步返回 Unit、单 Borrow String source 实参
调用合格；named、非 Unit、特殊调用、Nothing、借用返回及其它 source 整函数
回退为无本片 receipt，仍保留原执行/拒绝边界。旧见证不退休。

独立准入匹配 typed 与原 LoanFact/参数合同的完整 call/argument/source 身份。
真实 bind_parameters 后冻结 ABI；每段按 operand→实际 DirectCall append→正常
statement completion 顺序捕获，不能从最终 SSA 填造事件。段调用复用外部 source，
无伪 Begin/End。loop 关闭后才许可后段；共享 exit/entry 的指令按序合并。末段非空
时 source 必须运至尾调用后 Return，后续无用途的 flag 仍消亡。最终验证段的局部
source 实体、精确指令全集和真实 fold，原 unit verifier 后原子发布保持。

真实缺证红：before-after 原 lowering 与原 verifier 成功，receipt0 对期望1失败。
实现后定向5项普通全绿，其中组合矩阵34个源码变体、rooted/memory 与 on/off 共
136次实际 lowering，保留旧28变体并新增6个空/多调用段、单/多 loop、混合条件及
ABI换序变体。逐段实际 callee/span/顺序、当前块 source、尾运输与 independent
validate 均核；named/nonUnit/special/other-source 四项合法边界保持 receipt0。

native 零轮1项真实编译、链接、运行，验证前/中/后仍可用 source，输出依次为
pre、kept、mid、kept、tail、kept、kept；不宣称执行任意非零轮。新增2组 ignored
故障，合计15组仅 lib no-run 编译，未执行，覆盖错 source、缺失/重复/乱序段调用、
封口及元数据破坏、同步边快照后的尾 source 丢失。

日志保留于本片 checkpoint。独立生产与 CI 静审无阻断；家族97项、codegen
feature/all-targets Clippy -D warnings、默认workspace/all-targets、fmt/diff通过。
尺寸按2beb5592基线通过（2629文件/36历史债务），文档结构913篇通过。
本片未修改FE，未另跑FE测试；旧见证和尺寸policy保留。本地提交，不推送。

### 受限 owner / borrow 冻结合流验收（2026-11-12）

以远端 main `f78c8c20` 为基线，冻结 owner `005675cb` 与 borrow `1341d0c4`；
不纳入其它工作区未提交内容。保留 main 的退休成果，将 field/inout/assignment 的新增
授权与发射钩子放入现有 value/tagged 职责；两线独立 receipt 的构造、layout、fold、
statement、terminator 与 finish 保留。独立逐方法复审无阻断。

首次尺寸门禁发现合流后的 borrow_binding 1315 行、emit 1002 行超出基线额度，未放宽
policy。将五个聚合值方法移至 aggregate_value，将实际 CFG folding 移至 emit/folding；
六个函数体与拆分前逐字一致。原文件分别降至977/945行；最终尺寸检查通过，2623手写
Rust文件、37项历史超限仍明示，不声称清零。

本地普通验证：FE lib 692通过（12 ignored、146 filtered）；CG owner 家族420通过，
包含实际 native 编译链接运行；loan_region_ssa_tests 99通过。严格双crate feature
all-targets Clippy、feature workspace all-targets、fmt/diff通过，文档922篇结构通过。
清单脚本10项单测通过；真实36故障清单已精确编译列举（FE12、CG24），本地未执行。
该清单在双平台远端 CI 逐FQN以exact/ignored运行，必须每项实际1 passed/0 ignored；
未取得远端证据前不得将其列作通过。完整workspace测试及Preview未运行。

### 受限父恢复后正常回边的可选运输事实（2026-11-12）

此片不授予新消费权限。实际child lowering在已匹配的parent LoopFrame上捕获
continue_target，原恢复终检后仅选择保持新root不读写的Goto/真实checked receipt
success走廊。checked receipt必须唯一并重新匹配真实expression/指令/span，abort
实际终止且不通往parent header；visited防环，到捕获的header即停止，不跨下一轮。
合法但更复杂suffix（如打印、再次Replace或提前退出）不发布可选事实，保持原许可。

选中候选后随fold迁移边和checked位置；SSA发射观察实际checked操作快照，终检从
恢复Replace第一输出沿真实边实参→目标参数运输到parent header，核每段根不被消费、
checked失败标志→abort及success方向，缺项/错配返回InvalidModel，不静默降级。
该事实仅证明受限静态CFG的正常运输，既不把abort算回边，也不证明所有退出路径、
动态代际或任意轮次归纳。另真实正例覆盖child局部根退休、剩余第二根按symbol恢复，
恢复后打印应保留单笔许可但不发布此新回边事实。

本片必要验收：最终counter_boundary普通native 28/28（87.10s）、CG all-target
strict Clippy（35.34s）、workspace all-target check（36.01s）均通过。fmt/docs915/
diff/尺寸门禁通过（2589手写文件，37历史超限，无新增豁免）。独立复审未见确定阻断。
新错根实参与abort错向两项CI-only负控仅编译；普通部分退休恢复正例通过并确认
复杂suffix不发布carry事实、原单笔恢复仍完成。额外nested广域回归提交时仍在运行，
不计为已通过；本片不修改FE，未重跑FE。日志nested-parent-carry-*，仅本地提交。
补充广域回归随后完成：nested_普通测试108/108通过（587.04s），较长用时来自既有
native组合矩阵；无失败。仅执行非ci_only测试，未把故障注入算作本地通过。

### 已批准 child normal-body 的独立生产退休终检（2026-11-12）

同合同审计未发现可直接退休的旧生产权限witness。现有parent恢复receipt不覆盖
inner normal每个Replace的第二输出；Builder原始CFG检查与普通最终SSA测试仍不可
互相冒充。此片将已批准child normal的真实root/producer/old/binding及原三位置/span
封存为独立生产计划，随fold同步，在发射中观察Replace完整操作、Alias真实Entity
映射和原Drop，最终全组核每个旧Value唯一消费、不逃逸及实际指令清单/顺序。
所有step通过才发布完成标记，不新增任何入口/退出/恢复/跨轮权限；不删generic
verifier、FE互斥、原一次性许可或其负控，保留普通最终SSA独立抽查。

验收：最终counter_boundary普通native 28/28（46.39s）、CG all-target strict Clippy
（41.39s）、workspace all-target check（21.85s）通过；fmt/docs915/diff及尺寸门禁
通过（2590手写文件，37历史超限，无新增豁免）。独立只读复审未见生产阻断，并补
缺Alias观察专属故障；三项CI-only负控仅编译，未在本地执行。FE未修改且未重跑。
旧witness审计结论与保留理由记入nested-owner-normal-body实现文档。本片仅本地
提交不推送，日志nested-body-receipt-*；后续集成#420需将新fold hook迁到其folding.rs。

### 既有 nested false receipt 的退休 Value 排他性（2026-11-12）

不新增receipt或权限。已退休String false参数Value在既有finish内必须仅由该exit
参数定义、全函数仅被原Drop使用一次，不得被其他指令读取/重新定义或从terminator
逃逸；原span、顺序和清单唯一检查保留。只约束具体退休Entity，不约束同源码symbol
后续新生命周期，也不收紧live根行为。旧独立最终SSA抽查继续保留，不冒称可退休
generic verifier或其他未等价witness。CI-only针对提前实际Consume、不同ID第二Drop、
真实terminator逃逸；本机仅编译。普通正例验证部分退休后的恢复和同symbol重新赋值。

验收：同symbol新生命周期九组合正例1/1（0.60s）；counter_boundary普通native
29/29（29.71s）、CG all-target strict Clippy（1m10s）、workspace all-target check
（38.33s）通过。fmt/docs915/diff/尺寸通过（2591手写文件、37历史超限，无豁免）。
独立复审未见确定阻断。三项新增CI-only真实SSA故障仅编译，不计本地执行成功；FE
未修改且未重跑。日志nested-false-exclusive-*。本片不触及#420 folding拆分，
不新增receipt、不扩大权限、不退休未证明等价的旧检查，仅本地提交不推送。


### 已批准直接父循环完整入口与 false 出口验证（2026-11-13）

用户明确批准在既有直接父／子 while、String 根和 checked counter 范围内，补充父
header 的整组入口／正常回边，以及父 false 出口资源消费的独立证明。本节是新增
实施范围，不把此前仅到达父 header 的单根可选 carry receipt 升格为跨轮证明。
不改变 Guide，不扩大 Break、Return、额外嵌套层或未闭合资源的接纳范围。

实施要求：从 frontend 已验证事实和实际 lowering 捕获父 control、完整根组、入口、
回边与 false 出口身份；SSA 终检逐个核实际参数、所有前驱和对应边实参，不以类型
相同、根数量相同或单条回边替代整组身份。父 false 路径独立核 live／retired 完整
互斥分区，退休值仅由原 Drop 消费一次且不能逃逸。checked Abort 保持终止，不作为
正常回边。候选被选中后，缺失或失配必须失败，不能静默回落为旧路径成功。

| 验收项 | 目标 | 当前结果 |
|---|---|---|
| 精确父 header 全组入口与回边 | 真实参数／边、丢根、同类型换根、额外前驱 | 待实现、待验证 |
| 父 false 资源闭合 | live／retired 分区、原 Drop 排他消费、无逃逸 | 待实现、待验证 |
| 运行代际覆盖 | 外／内零、一、至少二轮与部分退休后的恢复 | 待实现、待验证 |
| 边界拒绝 | Break、Return、额外嵌套、未闭合资源不新增许可 | 待验证 |
| 原生产验证保留 | 原 verifier、旧 witness 和 child receipt 不被替代 | 待验证 |

新增 fault injection 仅在 CI 执行；本地只报告编译和普通定向测试，不把待运行项
记成通过。实现与验收按小步本地提交，本批不推送。

### 父循环边界 FE 事实切片（2026-11-13）

`parent_boundary_v1` 只封存实际父入口、条件、body-entry、正常回边完整有序根组，
及父 false 出口新生成的清理事实/live 根分区。必须与既有直接子循环和 parent resume
根组一致；不改变原清理或 lowering 许可，不宣称父代际不变量或后端资源消费完成。
普通精确正例覆盖父出口全 live、部分退休、全退休；普通负例覆盖 Break、Return、
条件 Break 和额外 while；原 nested normal 合同精确回归保持。三项精确测试各 1/1
通过，lang-frontend all-target strict Clippy 通过。此前模块过滤误执行 CI-only 故障
测试的偏差不作为合规 CI 验收证据；本轮只执行明确的普通精确测试。
workspace all-target check 在恢复连接后重核通过；fmt、docs915 和 diff 检查通过。
公开 API 仅增加只读父边界事实，原 child/carry 与 generic verifier 均保留。

### 父 false 独立 SSA 消费收据切片（2026-11-13）

基于 FE 父边界事实提交 `4deef005`（owner 集成为 `3d0852c8`），新增独立父
false receipt；父与子持有分开的观察状态，仅共享机械 SSA 核验。不生成新 Drop，
不改变 lowering 权限；实际根完整互斥分为 live 与原 Drop，父出口仅有父 header
false 边这一前驱，边实参按根身份匹配，退休 Value 仅被原 Drop 消费一次且不能
逃逸。选中后父 receipt 缺失在 layout／finish 均失败；原 child/carry/verifier 保留。

普通 native 精确测试 `parent_false_receipt_partitions_live_and_retired_roots` 1/1
通过：全 live／部分退休／全退休三种分区，每种运行外内 (0,0)、(1,1)、(2,2)、
(2,0) 四组合。另四项既有 nested/lifecycle 普通精确测试各 1/1 通过。首次增加
(2,0) 后预期输出漏加第四组而失败，补齐该组预期后重跑通过，不隐去失败记录。
CG all-target strict Clippy、workspace all-target check、fmt、docs915、diff 通过。
本地未执行故障注入；新父路径的 CI 故障覆盖仍待补充和实际运行。独立只读复审
未见确定阻断。仅为边界运输与消费核验，不证明父根跨轮代际归纳，D10/E2 不标完成。

### 父收据独立 CI 故障入口（2026-11-13）

父 header 既有三种真实边故障（同类型换根、缺参、额外前驱）从旧 resume 组合
测试迁至独立 ignored 测试，不在 CI 重复执行。父 false 独立 ignored 测试覆盖
选中计划缺失、不完整分区、false 实参换根、原 Drop 丢失、不同指令二次 Drop、
退休 Value terminator 逃逸及额外出口前驱；SSA 损坏在独立父 receipt 内先拒绝，
不得借通用 verifier 的失败代替收据证明。两个测试各要求 GitHub Actions 环境，
普通 composition 不执行；双平台 Targeted 在 composition 后通过
`scripts/check_parent_receipt_ci.sh` 校验精确测试存在再各执行一次。

本地只编译及 `--list --ignored`：两个精确测试存在，未运行任何故障。普通父
false native 精确回归 1/1 通过；CI 实际故障验收仍待推送后的两宿主运行，不标绿。

### 父边界与借用故障覆盖次日合流（2026-11-13）

以已发布0031abcc为基线，整合owner93382198至c9c2c090及借用710e73f0。
保留双方追加验收记录；folding职责拆分位置接入nested body relocation，
terminator同时保留loop/fresh-loop及parent-header核验，FE导出和双宿主CI入口取并集。
未覆盖原借用事实、未恢复已拆出的旧函数，也未改变Preview或通用verifier。

合流实测：workspace all-target check、FE/CG all-target strict Clippy通过；
五项普通精确native（父false分区、nested root、child false、parent resume、
retired-symbol新生命周期）5/5，父边界普通FE正反例2/2通过。
36项借用故障及2项父receipt故障仅编译列举，未在本地执行；CI结果待推送后确认。
清单脚本模拟runner单测10/10通过，不属于真实Rust故障验收。
fmt、docs922、diff及以0031abcc为基线的尺寸门禁通过；2635手写Rust文件、
37项历史超限继续报告，无新增豁免。独立合流只读审查无确定阻断。
本批只证明受限父边界运输/消费，不宣称完整跨轮代际归纳。

### 父 false 反向部分退休普通 native 回归（2026-11-13）

基于已推送批次 `447b1eb6`，既有普通矩阵增加保留 `a`、退休父 resume 实际写入
的 `b`；断言退休目标等于真实 parent-resume 根且使用父 LoopExit 点，不以一个
String 根的数量代替身份。四种分区各运行外内 (0,0)、(1,1)、(2,2)、(2,0)，并
核 SSA receipt/verifier 及 native stdout。该精确普通测试 1/1 通过（34.96s），
未修改生产或 FE 代码，未执行故障注入；不扩大跨轮归纳证明范围。

### 父 false 收据记录丢失 CI 补片（2026-11-13）

既有 CI 故障覆盖计划缺失和 SSA 损坏，未覆盖实际 layout／statement 观察后记录
丢失。本片在同一 ignored 精确测试增加两项：清空已形成的父 receipt、移除其一个
已观察 Drop 事件；保留原 SSA 不变，直接要求父收据终检失败，不能由通用 verifier
代替。生产逻辑不变，CI FQN 与执行次数不变。仅本地编译并列举两项 ignored 测试，
未执行故障；新增两项与原有七项父 false 故障均须真实 CI 运行后才能验收。

### 已批准：直接父子循环符号代际归纳（2026-11-13）

用户批准不扩展语法，在现有直接父子 while、String 根和 checked counter 范围
增加静态符号代际合同。每个 header 根代表任意当前 owner，不是固定第一轮 token；
每次成功 Replace 以本次激活的独立 String 定义更新该根，并由原 binding Drop
恰好消费旧 owner。先建立 child 的入口假设、单轮保持、任意次数闭包和 false
恒等出口，再将该闭包与 parent resume 的单次更新组合；未更新根保持 owner。
整组回边必须逐根运输当前 owner，父 false 仅消费／保留当前代且分区完整互斥。
checked abort 是独立终止路径，不参与正常回边归纳，也不声称终止性。

继续拒绝 Break/Return、额外子循环或嵌套、动态根集、跨轮 loan、额外前驱和未闭合
资源。FE 合同必须与实际 SSA 的独立消费核验配对；有限轮数 native 测试仅为回归，
不得替代任意激活的参数化论证。分步交付首先建立 child 的符号规则；完成 FE
规则不等于验证了入口的具体 owner、SSA 运输或父组合，旧 witness 和通用 verifier
继续保留。此记录为批准范围，不是完成验收。

### child 条件式符号递推首片验收（2026-11-13）

在原 normal 线性规则上增加 `child_generation_rule_v1()`：按完整 header 根身份
给出 current(0)=entry、current(k+1)=fresh(site, invocation, k) 以及消费 current(k)
的原 binding Drop。要求实际 checked-counter 完成事实，child false 和 parent resume
均为同序全根存活向量；不同父激活中的同一个 child k 不共享 fresh 实例。
这仍是条件式 FE 规则，不证明具体入口实例、SSA 实现或父组合，没有新增 lowering
权限；不以测试有限轮数冒充跨轮归纳，旧 witness／通用 verifier 全部保留。

| 验收项 | 目标／过滤器 | 实际结果 |
|---|---|---|
| 完整根／producer／Drop 身份及交换源码执行顺序；重复根、非正常退出、无 counter／false-reset 拒绝 | FE lib `child_generation_rule`（两个普通测试） | 2/2 通过 |
| 原 nested 消费者未改变 | FE lib `nested_normal_body_uses_real_child_events_without_exit_authority` | 1/1 通过 |
| FE 静态检查 | `cargo clippy -j1 -p lang-frontend --all-targets -- -D warnings` | 通过；首轮 type_complexity 已修复后重跑 |
| 公共产物下游兼容 | `cargo check -j1 --workspace --all-targets` | 通过，26m16s |
| 文档／格式／差异 | docs922、修改文件 rustfmt、diff check | 通过；全仓 fmt 耗时后停止，未将其计为通过 |

未运行任何 CI-only 故障注入。独立审查先发现缺实际 counter 门控，已修复并补普通
负例，复审无确定阻断。剩余真实入口、独立 SSA 代际消费和父组合继续 active。


### D10 child 实际 planner 入口快照切片（2026-11-13，本地已验）

在上一条件式规则上保留真实 child enter 的每根 owner 身份，并在实际 condition / body-entry
确认不变，封闭边界排除活 loan 与 pending resource；缺失或重复 owner 不发布此规则。
不新增 lowering 许可，不把静态 owner ID 当作跨父轮次的实例，也不宣称 SSA 基例已闭合。
独立 SSA 接续须以 pre-fold 的真实 child preheader/header/body/false 身份捕获、正常
checked-success 回边向量以及现有旧值退休 receipt 配对，不能复制退休检查冒充归纳。
本切片只推进 FE 入口观察；SSA 代际消费、checked-abort 配对及 parent 组合仍 active。


| 验收项 | 实际结果 |
|---|---|
| FE lib `child_generation_rule`（两个普通测试） | 2/2 通过；完整根顺序、owner 唯一、child producer 区分及既有拒绝形状 |
| `cargo clippy -j1 -p lang-frontend --all-targets -- -D warnings` | 通过 |
| 修改文件 rustfmt / diff check / docs922 | 通过 |

初轮附加断言错误地假定外层 callable events 保留 Binding；实测该 unsupported
observation 按既有合同清空 events/definitions，已撤除此不成立断言后重跑通过。
不把该外层事件当作入口证明；本片只保留实际 planner 状态快照。独立只读复审无确定
阻断。未运行 CI-only 故障注入；本轮未重跑 workspace、SSA 或 native，因为未改 backend，
也未实现独立 SSA cycle receipt。本地提交不推送。

### D10 独立 child-cycle SSA 收据（2026-11-13，本地普通验收通过）

仅选择已有 checked-counter child 符号规则：builder 在真实 `goto(header)` 前捕获
preheader，按已记录 checked expression 身份捕获 body/success/abort，不从最终 CFG
搜索代替原观察。独立收据在 layout/terminator 保存六角色与完整参数、当前根身份，
终检要求 header 仅 entry/backedge 两前驱、true/false 的逐根运输，以及成功回边完整
传递 Replace 新 owner。checked 指令、flag 极性、独立终止 Abort 均配对。
复用已成功终检的 body retirement 原始 input/new 观察，不重复实现旧值 Drop 证明。
仍不证明父 header 到 child preheader 的完整入口链或父轮归纳；不增加新语法/形状。


| 验收项 | 实际结果 |
|---|---|
| `child_cycle_receipt_connects_actual_current_and_next_root_vectors` | 1/1 通过，包含两根正序/逆序普通源码及实际 receipt marker |
| 既有 `nested_counters_preserve_outer_live_roots_without_flat_batch` | 1/1 通过，含既有普通 native 基线 |
| `cargo clippy -j1 -p lang-codegen --lib -- -D warnings` | 通过 |
| docs922、修改文件 rustfmt、diff check、Rust size guard（base `0f3d13a9`） | 通过 |

初编译 ValueId 解引用问题已修；首轮普通正例还发现 Abort emission 原来跳过普通
terminator hook，以及 abort sibling 活跃槽传播使互斥两边各携新代，均修正后重跑通过。
独立审查发现的 header guard 身份及 Subtract abort divisor 配对亦已修正。新代只能在
成功边和可选同根 Abort 边各运输一次，无其它直接消费；Abort 不作为正常回边。
已选 cycle 的 required 标志与退休计划分开保存，缺计划拒绝静默降级。
未运行任何本地故障注入，新增收据的破坏性负例矩阵与远端 CI 尚未验证；本轮未重跑
全 workspace/all-target 门禁。父入口链、父组合及完整跨父轮次归纳仍 active，不以有限
native 轮数或本片普通正例代替它们。仅本地提交，不推送。

### D10 child-cycle CI-only 破坏矩阵（2026-11-13，待远端验证）

在扩展父归纳之前补充独立收据的破坏覆盖：15 类真实状态变异由两个 ignored 测试
承载，包括 required plan/receipt/观察丢失、entry/true/false/backedge 根向量错配、
错误 header guard、成功边旧代/漏根、checked 极性、abort 回跳/同边重复根、额外前驱
及新代被直接消费。late faults 先验证未变异的退休与 cycle 基线，再修改真实 SSA/观察，
直接断言独立 cycle 终检拒绝并核 hit/rejection；不以通用 verifier 的后续拒绝计通过。
MissingPlan 保留退休计划的 required 标志，在独立 layout 门拒绝。

`scripts/check_child_cycle_ci.sh` 在现有 macOS/Linux test matrix 显式执行两个精确
ignored 测试。默认执行要求 `GITHUB_ACTIONS=true`，测试与注入入口也有环境保护；
本地只允许 `--list-only` 编译并精确列举，不执行任何变异。新增目录单独承担 CI-only
破坏职责，不改变生产 child-cycle 形状，也不宣称父入口链或父归纳完成。

| 本地允许的验收 | 实际结果 |
|---|---|
| `bash scripts/check_child_cycle_ci.sh --list-only` | 两个精确 ignored 目标编译/列举通过；none executed |
| `child_cycle_receipt_connects_actual_current_and_next_root_vectors` | 普通正例 1/1 通过 |
| `cargo clippy -j1 -p lang-codegen --all-targets -- -D warnings` | 通过，只静态检查 |
| 脚本 bash 语法、docs922、修改文件 rustfmt、diff、尺寸门禁 | 通过 |

独立只读复审确认 1 项缺计划、14 项 late faults 与测试清单一致，双宿主入口无单平台
条件。没有本地执行任何故障注入，也未推送或取得远端运行结果。此轮是 child-cycle
证明的负例覆盖建设，不增加父入口链/父组合实现完成度。


### D10 条件式 parent-header → child-entry 链（2026-11-13）

在已选 child-cycle 的六角色之外，保留真实 enclosing loop frame 的 parent header，
共同参与 folding/layout/实际 terminator 观察。独立终检校 parent guard 身份、真实
true 边完整目标槽向量、child preheader 唯一前驱，以及两块每根参数保持且无指令
读取/借用/消费；两条互斥父边每根各运输一次。接上既有 entry→child header 与
child recurrence 后，得到“给定父 header 当前独占向量”的本次 child 调用入口链。

这不是父根初始创建/函数入口基例，也不独立证明 parent header 的两条入边生命周期；
parent resume、false partition 的局部收据不能据此合称完整跨父轮归纳。没有扩大语法、
退出或嵌套支持。父初始 owner 来源与跨父轮组合仍 active。

新增三项 CI-only 实际变异：父 true 根交换、preheader 额外前驱、preheader 根消费，
复用双宿主精确入口，现共 18 类。只读复审核对条件证明范围，未发现确定阻断。
本地验收：普通正例（两根正序/逆序）1/1 通过并断言真实 parent-entry marker；
既有 nested counter 普通/native 基线 1/1 通过；两个 CI-only 精确目标编译列举通过
（none executed）；codegen all-target strict Clippy、docs922、修改文件 rustfmt、
diff check 与尺寸门禁（base `f62b312b`）通过。任何 CI-only 变异均未在本地执行，
远端结果仍待验证；未推送。


### D10 直线函数入口的受限初始 owner 基例（2026-11-13）

真实 parent while lowering 另捕获初始 block。仅当它是函数直线入口，且完整根向量
沿 Alias 来源追踪到 own 参数或 String literal、来源在入口未被其它指令使用时，
选择可选 initial-base 合同；其它 producer/已使用来源不发布基例，不扩大或收紧旧
lowering 权限。独立 required 保存在 retirement 计划，已选基例丢失必须拒绝。

emitted 终检核真实 function entry、参数定义或入口单结果 StringLiteral 定义、每根
Value 唯一、无指令读取/借用/消费、无重复定义/其它块参数冒用/旁路 terminator
使用，以及完整向量唯一边进入 parent header。接上上一片 parent→child 链后，
此受限来源已具有本次 child 调用基例；父回边 resume 组合、任意父轮保持和更一般
初始来源仍未由此证明，不能称完整 D10 归纳。

新增 CI-only 初始计划丢失、初始向量交换、literal 源被消费三例，共 21 类；仍由原
两个双宿主精确入口承载，禁止本地执行。

本地主正例 1/1、既有 nested counter 普通/native 基线 1/1 与已使用根/其它 producer
的普通兼容性回归 1/1 通过；后者只断言
既有 lowering/通用 SSA 合法且不发布 base，不能冒称独立 selector 单测。初版错误
要求这些源码仍发 child-cycle marker，实际 FE 可不选该规则，已据真实范围修正。
codegen all-target strict Clippy、docs922、修改文件格式/diff 与尺寸门禁通过；
CI-only 两个精确目标仅编译列举，none executed。独审发现最初消费反例先破坏
literal 来源，已改为保留 literal、破坏后续 scalar 指令，避免把来源拒绝冒称消费
拒绝；该修正仍只编译列举，远端破坏验证待推送后进行。

### D10 受限父轮符号组合（2026-11-13）

在 child-cycle（含受限初始基例）、parent false partition、parent resume retirement
全部完成终检之后，额外核跨收据 control/header/root 向量身份。限定单次父 String
literal Replace 与一段 checked tail；对子 false→resume→checked-success→parent
header 的完整向量逐槽核真实边参数。未改根始终是本块参数且无指令消费；改根在
resume 是真实新 owner，回边成为同根父 header 参数。正常路径唯一前驱、父 header
仅初始/回边两前驱；父 header 禁止 Place、新建 loan 和 String 根 loan。既有 Shared
Int 函数参数（如每轮初始化 inner 的输入）可按原参数槽保持：类型须匹配实际 checked
整数类型，不是活动 owner-root loan。中间已选 Replace preparation 另核唯一结束链。

父替换输入独立核唯一同块 RootPlace→Exclusive BorrowBegin→RootReplace 实体链、
定义/使用及顺序，Replace 结束 loan，无 place/loan 参数或终结器逃逸；复用原 old
唯一 Drop 收据。replacement 必须是本次 resume 的唯一 StringLiteral 定义与唯一
Replace 使用，新 owner 不得被任何其它指令直接消费或从非 resume 边逃逸。
checked abort 单独核原 operator/divisor、空指令终止块、唯一前驱及根向量；不是回边。

归纳陈述是部分正确性：给定父 header 第 k 轮独占根向量，child 的零轮或任意正常
轮次闭合给出当前向量；父单次替换消费目标旧代并安装本次激活的新代，保留其它根；
checked success 完整映射回同根 header，得到第 k+1 轮不变量。初始基例由前片提供，
false 边按当前代既有互斥消费/保留分区退出；abort 不产生正常归纳后继。不证明终止，
不涵盖更一般 producer、Break/Return、多重嵌套或任意父写入序列。

新增四项 CI-only 真实破坏：父 carry 根交换、未改根额外消费、父借用变 Shared、
新代在其它块被消费，共 25 类；禁止本地执行。

最终普通主正例 1/1（根 Replace 正/逆序）及父 false 部分保留/全消费组合正例 1/1
通过；非发布兼容性和既有普通/native 基线本轮亦各 1/1 通过。最终 codegen all-target
strict Clippy、两 CI-only 精确目标编译列举（none executed）、文档、格式及尺寸门禁
通过。未推送，破坏矩阵仍待远端双宿主验证。

本轮先修复模块可见性和 Clippy 问题；加强后的普通正例曾被“禁止所有 header loan”
错误拒绝。核对真实 SSA 发现是已有 Shared Int 输入，不是 owner-root loan；现只允许
原参数槽、实际 checked 整数类型、raw 全体无重新定义且观察 current 等于同槽参数的
外部只读输入，保留正常读取，不放行新借用。独立审查另外补齐新代全局消费、输入
唯一身份与完整 exclusive loan 链，临时调试输出已移除。

### D10 独立证据审计补强（2026-11-13）

审计确认两处独立核验缺口，而不是已经证明非法源码能绕过最终 SSA verifier：旧 child
retirement 只锁 Replace/new-old/Drop，没有重核真实 literal 来源及 Exclusive 借用全链；
组合选择在终检重新计算后可 early-Ok，不能区别从未选中与选后证据丢失。

选中 child cycle 时，每个 Replace 发射点现记录实际 StringLiteral、RootPlace、BorrowBegin
的 InstructionId/operation 快照。最终按实际 Definition 复核同块 literal 唯一定义、在
Replace 前激活、rhs 唯一交付且无逃逸；输入 owner 唯一参数来源/唯一准备与消费。
child 与 parent 共用同一独立 exclusive loan 链检查，核 place/loan 的定义、使用、顺序、
结束和不跨参数/边逃逸；old 的唯一 Drop 仍复用原收据，不复制实现。

组合资格在 builder 完成局部计划之后封存，分别保存在 resume.selected 与独立 retirement
required；layout 再保存 expected。layout/终检核资格一致性，已选后的 carry 丢失、checked
清空、raw literal 资格漂移不再降级为 Ok；真正从未选中的形状继续保留旧 lowering。

新增八项 CI-only 真实变异：child producer 种类、Shared 借用、额外 loan 使用，以及组合
选后 carry/checked/producer 证据丢失、selected/required 各自单边清除，共 33 类。使用既有两个双宿主精确目标，本地
禁止执行注入。本片普通模块 3/3 通过（两个 ignored 明确跳过），既有普通/native 基线
1/1 通过；all-target strict Clippy、docs922、格式/diff 与尺寸门禁通过。两个 CI-only
目标精确编译列举通过，none executed；新增八例的实际触发与拒绝仍待远端运行。
独审复核确认来源/借用链及失效关闭结构补齐；未推送。
