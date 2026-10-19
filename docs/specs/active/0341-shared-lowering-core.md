# SPEC-0341: D10 共享 lowering 核心与薄入口适配

> **性质**：分阶段实施合同 · **状态**：in-progress · **读取时机**：实施或验收 single/unit lowering 收敛时 · **唯一真源**：本片目标、边界与验收；语言语义以 Guide 为准

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P4-0341` |
| 所属 Phase | Phase 4；消费已验证 frontend facts，不重推语言语义 |
| 语言规范 | 现行 [Guide v0.44](../../guide/README.md)、[阶段交接](../../guide/15-conformance-and-staging.md#phase-边界)与[常量交接](../../guide/05-declarations-callables.md#36-无运行时存储的关联常量与封闭求值) |
| 批准依据 | 用户要求“先合并两条管线、改用通用的代码生成”及“完成F收尾且合并后，后续D10任务交回给云端执行”；本页落实该方向，不称用户已逐条审定本文 |
| 基线 | main `c0c5156a5f28d206b5a87986173f2bbf5bae6611`，tree `cd1041238c466546aa5463b959cee0a2fb3a02d7`；分支 `feature/spec-0341-shared-lowering-core` |
| 前置 Spec | [SPEC-0255](../../archive/specs/0255-neutral-lowering-support.md)、[SPEC-0331](../../archive/specs/0331-map-borrowed-iteration.md)已归档且代码在上述 main；不继承旧头测试结论 |
| 前置 ADR | 无；后续若引入新的长期状态协议或改变既有架构决定，先独立 ADR |
| 影响范围 | `lang-codegen::ssa` 的共享 lowering、single/unit 入口、直接测试及当前事实文档 |
| 语言语义变更 | 否 |

## 1. Goal

将 single 与 unit 已实现的共同 lowering 责任收敛到共享核心，入口仅保留输入、来源、能力与
必要的差异适配；共同语义决策与构造算法不再各维护一份。按领域逐片替换并可独立回退，
保留当前成功、拒绝、诊断和资源合同。仅抽出常量映射不能关闭本 Spec 或宣称 D10 完成。

## 2. 基线与固定边界

unit-basic 与 unit-const 已共用 `lower_unit_from_facts`，single 有独立 driver；它们是可选入口，
不是每次编译依次运行的两套阶段。已有 error/String decoder、`CallableArena`、整数/Range
纯映射、borrow-result helpers、运输证明与顺序 provider builder，不重复发明这些共享能力。
provider builder 有状态，不把它或 ownership/CFG 状态机称作纯函数。

| 责任 | 必须保留的差异 |
|---|---|
| 输入与身份 | single raw products 的校验次序；unit sealed basic/const view 的同轮来源证明；源码相同不代表分析相同 |
| 来源与规划 | local 与 source-qualified ID；single 原根集合与 unit 显式 entry 可达规划；StaticSelf/deinit/callback key、recipe 与预算 |
| validator | schema/diagnostics/deferred 与 backend capability 分层；单文件内校验和 unit factory 的阻断位置不互换 |
| 诊断 | 错误 kind、来源与 Span、None span、多重失败的原优先级；不为配对成功修改拒绝边界 |
| CFG 与 cleanup | 各自 bindings、pending frames、receiver、loan、Group alias、iteration 与条件 drop 事实；正常提交、提前退出与 Abort 分流 |
| 已知能力差 | single 的 receiver/nullable factory 等原限制及 basic/const 分界；共享不表示支持面取并集 |

两入口保留末端 `verify_program`。不得以重新求值 initializer、推断 owner 或重新分析 AST
替代缺失的 frontend fact；输出原子发布和宿主行为不在本片迁移范围。

## 3. 分阶段实施

每一阶段先列出具体生产责任、输入/输出、调用点及普通/CI-only 验收目标，再修改；同一时间
只推进一个未完成步骤。后续阶段可以继续细分提交，不能通过改名、搬两份代码或删除验收来完成。

1. **A：冻结责任与配对合同。** 以固定基线盘点常量/字面量、整数/Range、借用事实查询、
   callable/实例、容器/provider、普通表达式/CFG、loan/drop 七类责任。逐项记已共享、待收敛
   或真实差异，定位当前消费者与测试；source-order/同名 symbol/泛型/const/receiver/drop
   配对按逻辑 source key、Span、definition→use、owner/loan 关系核验，不比较裸 TypeId。
2. **B：常量纯映射首片。** 仅提取已验证 `ConstantMaterializationKind`、`&ConstValue` 与
   `SsaTypeId` 到 `Result<Operation, LoweringErrorKind>` 的选择。保持 Boolean、Integer、Char
   与 StringTemporary 四路及非法配对的 `MismatchedAnalysis`，不新增值、类型或接受能力。
   两适配仍分别查 descriptor/plan、核完整身份、取得 span/type，再调用、append 和登记
   temporary/drop；原顺序与 unit 常量 Group origin 规则不动。
3. **C：事实查询与只读校验收敛。** 从 A 的待收敛清单逐个提取已证等价的责任；每片明确
   原 local/source-qualified 查询及错误映射，先复用现有 helpers。只共享无入口状态的算法，
   不把 Map proof、全套 validator 或来源身份压成宽 context。此阶段不能只清空候选表便称完成。
4. **D：共同 lowering 核心与薄入口。** 按表达式操作、语句/控制转移、driver 骨架逐层
   收敛，生产消费者经共享的事实消费/SSA 构造实现。CFG/cleanup 先建立两入口的语义接口
   与状态角色映射，明确 owner、loan、pending、receiver、source/element 和退出动作，再抽取
   等价状态算法；它们是后续必要收敛责任，不能永远排除。每个小片先独立设计 review，首片
   不提前触及。入口保留上表身份、能力与来源适配，真实不等价的领域策略有明确边界；不能
   用大量可选字段隐藏旧 driver。每域删除被取代副本、补依赖门禁，核全部已支持退出及资源
   路径。未证等价的共同责任继续开放，不能强行让 single 包装 unit 或默默删能力。
5. **E：最终结构与行为审计。** 对 A 的全部条目核对唯一实现/消费者，真实差异逐项给出
   必要性、源位置及固定回归。独立审查确认两入口已是薄适配、没有同一事实解释/构造算法的
   双份生产实现；行为矩阵、资源证据、工程门与精确头双宿主 CI 齐备后才可关闭。

B 通过时只记录“常量映射首片已验”，C/D/E 仍开放。若后续证明既定目标需要语言变更、
新能力或新的长期合同，应停在该责任并提出独立决定，不将未完成项改写成完成的非目标。

## 4. 模块与依赖

首片采用 SSA 同层私有模块，与 `integer_range::operation` 相同职责层；具体模块名由实现
确定。它可以依赖 frontend 的已验证值类型、SSA model 及中立 error kind，不能依赖 driver、
planner、LLVM 或宿主，也不读写 lowerer。没有必要时不增加 trait、crates、公开 capability 或依赖。

不能将返回 `Operation` 的 helper 塞进 `lowering_support`：0255 的该层明确禁止 model。
保留 unit→single 禁边、support 原边界及 IR core 不反向读取 adapter/support；扩展现有词法
门禁覆盖新核心实际生产树，明确允许 model 的单向边，不用新模块逃出扫描。这仍是有界
词法护栏，不声称完整 Rust 依赖图；新增合法边不能成为整个旧门禁的豁免。

## 5. 非目标

- 不改变 Guide v0.44、公开输入能力、frontend 语义、LLVM ABI、标准库或 native 接受范围。
- 不接入或关闭0291、0333–0340；它们的调用、ABI、owner、Never、实参和 nullable 缺口独立保留。
- 不顺带修复0321或重排 append 与交付时点：single 按具体 MoveOnly ValueId 清别名，unit
  消费精确 source-qualified ValueDelivery，两者不是可直接互换的状态更新算法。
- 不以缩短文件、删除行数或一次运行耗时宣称性能收益，不要求两入口生成完全同形 SSA。
- 不将 F 的未评估组合、既有 sanitizer partial 或其他阶段欠账转交 D10 处理或宣称关闭。

## 6. 验收标准

- [ ] A 的有限责任清单、共同合同与真实差异均有生产位置和直接测试；没有仅按函数名判等价。
- [x] B 四路值/字节/Char/SSA 类型映射保持，常量声明无运行时初始化；配对 String use 保留
  独立 owner、原 Group origin、source 顺序与专用 const capability。
- [ ] C/D 每片移除一份真正重复的生产责任，全部消费者调用共享实现；原 diagnostics、identity、
  capability、planner roots、budget、verify 与失败原子性不变。
- [ ] CFG/loan/drop 被触及的片逐条证明借用结束先于 drop、alias/owner 唯一消费、pending
  实参与 receiver 运输、正常/return/break/continue/Abort 分流；未触及者不声称重新验收。
- [ ] E 完整结构审计通过；薄适配与专用策略边界有依据，无开放共同责任被静默排除。
- [ ] 对应普通 SSA/LLVM/native 正反例、完整 stdout/exit/stderr 与必要资源身份分别核验；
  普通 native 成功不证明无泄漏，静态 verifier 与动态资源证据不可相互替代。
- [ ] 工程门、独立审查及精确交付头双宿主 CI 通过；Architecture 只记录实际落地的阶段。

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

在上述专属分支内完成合同、生产片和直接验收，每个提交只属于 SPEC-0341，保持可构建、
可验证；建议按“冻结合同”“共享常量映射”“逐域核心收敛”“最终审计”划界。首片提交后
Spec 继续 in-progress，不归档为 done。完整验收后同步状态/路径、库存、索引与依赖图，
再按仓库 PR/CI/合并要求闭环；发布和合并按用户当前授权执行。

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
