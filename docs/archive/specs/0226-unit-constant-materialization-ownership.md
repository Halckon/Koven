# SPEC-0226：跨文件常量重新物化与所有权

> **性质**：实施 Spec · **状态**：done · **读取时机**：实施 v0.36 unit 常量 Phase 3 时 · **唯一真源**：本 Spec

| 字段 | 值 |
|---|---|
| 状态 | `done` |
| Goal ID | `KOV-P3-226` |
| 所属 Phase | Phase 3 |
| 语言规范 | [v0.36 §36](../../guide/05-declarations-callables.md#36-无运行时存储的关联常量与封闭求值)、[所有权](../../guide/10-ownership-borrowing-drop.md) |
| 批准依据 | 2026-09-12 用户启用 v0.36 并持续授权按 guide/分阶段 Specs 实施、分阶段提交 |
| 前置 Spec | SPEC-0198/0199/0208/0209/0210 `done` |
| 前置 ADR | ADR-0008/0016/0018/0020 `accepted` |
| 阻塞项 | 无；语言已启用，typed const capability 已交付 |
| 影响范围 | `lang-frontend` unit ownership/capture/liveness/drop 与测试；Architecture |
| 语言语义变更 | 否 |

## 1. Goal

完成后，独立 unit constant ownership 入口消费 `ConstEnabledTypedUnit`，复用已有 unit
所有权分析，发布与该次 typed 分析绑定的标量 inline 和 String temporary materialization
事实；后继 native Spec 可消费该专用产物，旧基础 capability 保持原边界。

## 2. 背景

SPEC-0210 已发布 source-qualified 常量值、依赖与 use；现行 guide 要求跨文件 Phase 3/4
分别通过新 Spec 接入。单文件物化和基础 unit 所有权均已存在，本次不重新定义它们。

## 3. 范围与需求

- 新入口只接受独立 const-enabled typed capability，校验 sources/inputs/names/type environment
  与分析身份；结果保留同一 typed owner，不能将另一轮或另一 unit 的 facts 拼接进去。
- 复用既有 unit call、capture、loan、value delivery、liveness 和 drop 分析；常量路径消费
  Phase 2 已选 target/value/use，不能重新求值或把声明改造成变量 owner。
- 常量初始化器依赖不产生运行时 owner/loan/drop。每次可达运行时 use 都使用 source-qualified
  expression identity：标量 inline，String 按普通 literal temporary 建立独立 owner。
- String 的 Borrow/Value/return、最后使用、临时替换、分支/循环、提前返回和 Abort 清理遵循
  已有规则；静态不可达或分析已确认不执行的读取不发布物化，动态分支保留条件执行位置。错误时原子清除可执行事实。
- closure 对常量的读取不捕获常量声明或 object/type namespace；普通捕获仍按既有分析执行。
- 发布专用 owned capability 及可查询物化 descriptor。基础 `ValidatedCompilationUnitTypes`
  和 `ValidatedCompilationUnitOwnership` 不被改造成可绕过新验证的转换出口。

## 4. 非目标

不实现 SSA/LLVM/native、runtime globals/singleton init、跨 compilation-unit ABI、一般 CTFE、
关联函数或新语法；不提前启用 v0.37。native 后继必须显式消费本 Spec 的完整 owned 产物。

## 5. 验收标准

- [x] 两文件相同局部 AST ID、正逆 inputs 均产生相同 source-qualified 物化/capture/drop 事实。
- [x] 全部 11 种闭合常量类型按 typed value 交付；标量无 owner，String 多次 use 的 owner 不共享。
- [x] 初始化器依赖及分析已确认不可达的读取不发布物化；动态分支保留条件执行事实；常量读取不新增 namespace capture。
- [x] 声明/namespace 不进入 moved/loan/owner/drop 状态；标量重复交付不移动声明，也不新增 loan/drop。
- [x] String temporary/root/drop 绑定具体 use 表达式；每个正常退出路径中，未转移的 owner 恰好清理一次，已转移的 owner 不重复清理。
- [x] String 的 Borrow/Value/return、重叠借用、表达式嵌套和控制流退出与对应 literal 行为一致。
- [x] ownership 错误不发布部分物化/可执行事实；混用分析、改动 inputs 或 typed owner 被确定性拒绝。
- [x] 新旧 capability 不可混用；现有基础 unit ownership 与单文件 constant ownership 最近回归通过。
- [x] Architecture、验证记录与完成状态同步；未运行项如实保留。

## 6. 技术方案与边界

在 unit ownership 门面分离公共 capability 校验与内部既有分析 driver，新入口共享内部算法；
不得为复用而公开将 const-enabled typed 强转为基础 validated 的方法。运行时 traversal/capture
在消费 const use 时区分编译期 identity 与值物化，String 复用 ordinary literal owner/drop 路径。

物化 descriptors 和 owned wrapper 的最终命名由实现遵循现有模块风格确定；其身份校验、
失败原子性与无基础转换出口为硬边界。若已有 unit 图缺少表达某条现行清理路径的事实，
在本 Spec 内补最小事实并以 literal 对照验证，不绕过该路径或修改 guide。

## 7. 实施计划

1. [x] 接独立输入/owned capability 和 identity gate → 验证：错配拒绝与基础接口契约。
2. [x] 接 runtime constant use、capture 与 String owner/drop → 验证：直接正反例、literal 对照。
3. [x] 扩展跨文件/控制流矩阵并同步文档 → 验证：本节与第 5 节逐项关联实际证据。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | 独立输入与私有 driver 复用 | `feat(frontend): add unit constant ownership boundary (SPEC-0226)` |
| 2 | 物化、capture、liveness/drop | `feat(frontend): track unit constant materializations (SPEC-0226)` |
| 3 | 完整验收与完成归档 | `test(frontend): complete unit constant ownership acceptance (SPEC-0226)` |

## 9. 未决问题

无新增语言语义问题。实现不能因 native 尚未接通而将缺失的 Phase 3 事实标为完成。

## 10. 验证记录

按切片复用已验证证据；下表区分历史提交与本次最终检查，不因文档勾选重复启动相同门禁。
`a6ba8d4` 已开放专用常量 owned capability；`4bf45ed` 补齐插值，`7b1f49b` 修复 String 二元求值/退出清理；`6c126f1` 补齐标量与双 source 否定证据；`66d1a39` 接私有短路控制事实；本次公开查询并修复不可达 lambda 清理，完整矩阵已独立复核，最终门禁已通过，本次完成归档。

| 验收项 / 命令 | 实际结果 | 证据边界 |
|---|---|---|
| `cargo test -p lang-frontend --test multifile_constant_ownership` | 本次 20 passed，0 failed/ignored | 新增公开计划 source/顺序/恢复及不可达 lambda；保留短路静态/动态/empty unit/单边 move/外层 pending 路径；保留全部标量重复交付无 owner/loan/drop 与双 source capture/drop 同局部 ID；保留 binary 常量/literal × 三运算符 × 八种退出（48 例）、nested prefix、left Abort 与 named-left 六例；保留插值双 owner、常量/literal × 5 种控制流、嵌套与 live named 隔离；保留跨文件/Group 两次重叠借用、独立 owner、反向 inputs；空常量 unit 来源；六类错配与重分析身份；错误/deferred 不发布 |
| `cargo test -p lang-frontend --test ownership_constants` | `7b1f49b`：16 passed，0 failed/ignored | 单文件常量 ownership 回归 |
| `cargo test -p lang-frontend --doc ownership_checking::compilation_unit::constant` | 本次 4 passed，8 filtered，0 ignored | 基础 typed/owned 隔离；新增 plan 字段不可伪造和修改的编译拒绝 |
| `cargo test -p lang-codegen --doc emit_native_unit_object` | `a6ba8d4`：2 passed，0 failed/ignored | 旧 native 入口分别拒绝专用 typed 与 owned |
| `cargo check --workspace --all-targets` | 本次通过（7m 21s） | 跨 crate API 编译门禁；不运行全量测试 |
| `cargo test -p lang-frontend --test multifile_ownership_checking` | 本次 63 passed，0 failed/ignored | 基础 unit 身份、capture/loan/delivery/drop；`adb760b` 重构前后也各 63 passed |
| `cargo test -p lang-frontend --lib compilation_unit::constants_tests` | `7b1f49b`：5 passed，64 filtered，0 ignored | 基础 owned 出口拒绝、String Name/Group/Member 二元析构、Borrow/Value temporary、无常量 capture |
| `cargo test -p lang-frontend --lib ownership_checking::compilation_unit::` | 本次 17 passed，54 filtered，0 ignored | 当前 unit ownership 内部契约、常量 flow/物化、pending cleanup 与 Group identity |
| `cargo test -p lang-frontend --lib groups_forward_the_same_materialization_owner` | `4afb5e2`：1 passed，68 filtered，0 ignored | Name/Member × 一/三层 Group；loan、delivery、Borrow/未提交 Value/discard drop 均对应物化计划，三个 owner 互不相同 |
| `cargo test -p lang-frontend --lib --test multifile_constant_ownership compilation_unit --no-fail-fast` | `4bf45ed`：lib 21 passed，48 filtered，0 ignored | 15 项 unit ownership 与 6 项 types 内部契约；同命令 integration 零命中不计验收，公开套件另行无过滤运行 |
| `cargo test -p lang-frontend --lib ownership_checking::compilation_unit::materialization_tests` | `4832f55` 最终：3 passed，60 filtered，0 ignored | 11 类型跨 source 精确 descriptor/类别、同局部 ID、正反 inputs；初始化器/return/Abort 排除、动态分支保留、错误/deferred 原子性 |
| `cargo test -p lang-frontend --lib ownership_checking::compilation_unit::pending_temporary_tests` | `b3851d4`：5 passed，63 filtered，0 ignored | 常量/literal × Borrow/Value × 6 种退出/正常调用，共 24 例；内层循环保留、命名 Value、精确析构顺序、return operand Abort；`4afb5e2` 也由上述 15 项覆盖 |
| `cargo test -p lang-codegen --lib ssa::unit_lower_short_circuit_tests` | `66d1a39`：2 passed，400 filtered，0 ignored | 基础 AND/OR CFG 与 carried owner、RHS 单边 move 的 MissingFact 拒绝仍成立 |
| `cargo test -p lang-codegen --lib ssa::unit_lower_string_tests` | `7b1f49b`：4 passed，398 filtered，0 ignored | 基础 print/Abort、嵌套 Abort 借用、String concat/equality/Value 消费；不证明插值 native 支持 |
| `cargo test -p lang-codegen --lib ssa::unit_lower_borrow_tests` | `b3851d4`：2 passed，400 filtered，0 ignored | 最近基础 SSA 借用消费回归，不证明新增常量 native 支持 |
| `cargo test -p lang-codegen --lib ssa::unit_lower_loop_tests` | `b3851d4`：7 passed，395 filtered，0 ignored | 最近基础 SSA 循环/退出消费回归 |
| `cargo clippy -p lang-frontend --lib --test multifile_constant_ownership -- -D warnings`、fmt check | 本次通过 | all-targets clippy 的既有测试 lint 未修改，不以 lib 通过替代 |
| docs check、diff check | 本次通过，353 Markdown | inventory 未变化；合同建立时另有检查器 21 项测试通过 |
| 独立只读复审 | 各逻辑切片无剩余阻断发现 | 私有 driver 机械迁移、常量 flow/物化门禁、pending cleanup/Abort；`4afb5e2` 复核 Group 归一；`a6ba8d4` 复核公开 API；`4bf45ed` 复核插值；`7b1f49b` 复核 binary/named-left；`6c126f1` 复核身份验收；`66d1a39` 复核显式模式、统一短路决定、动态合流与原子门禁；本次复核公开查询与不可达 lambda 的 source-qualified 过滤 |

### 已交付切片与事实边界

`adb760b` 将原公开入口主体及 helpers 机械迁入私有 `compilation_unit/analysis.rs`，身份校验、
binding、contracts/capture/dataflow 和错误清理顺序不变，没有公开强转出口。

`645c3e1` 的私有 runtime traversal 跳过 constant initializer，消费 typed Name/Member use，
不访问 declaration/namespace；liveness/drop 使用同一识别，String 二元 Name 走 temporary，
Group 递归保留内部读取 identity。closure root 排除常量，既有 capture 分类已排除常量和
namespace。provenance 保留常量来源，基础 recovery `validate()` 拒绝该来源。

`4832f55` 登记实际访问的 typed use descriptor 和 inline/String temporary 类别，按 source-qualified
expression 去重、合并排序。只有 typed 常量事实完整、ownership 无诊断且无 deferred 才保留
私有计划。最终矩阵明确断言 expression 与 target 来自不同 source，并保留相同局部 AST ID 对照。

`b3851d4` 将 Borrow temporary 与尚未提交的 MoveOnly Value 实参保存于 ValueState。正常提交时
Value 不重复析构，Borrow 在 CallReturn 逆序析构；放弃前缀的 return/break/continue 清理
pending owner，按 loop depth 保留尚未离开的外层调用。prior symbols 保证“后建 local →
逆序 temporary → 旧 local”。return operand 自身不继续时不生成 return cleanup，Abort 不展开。

`4afb5e2` 复用单文件既有 Group 归一规则，仅穿透 Group 并要求终端是当前 source 的 typed constant
use；loan、Value delivery、temporary origin 与发布的 drop 统一指向实际读取。drop 来源 Span
归一，原实参/调用/清理位置不变。独立复审后补了三个 drop owner 互不相同的断言。

### 失败证据与待完成项

有效失败证据包括：`S + S` 原 temporary drop 为 0（应为 2）、常量 recovery 原能通过基础
validate、合法动态分支原无 runtime plans、后续实参提前 return 缺 pending cleanup，以及
合法非 Unit callable 的 `return stop()` 原错误发布 return cleanup，以及 `(TEXT)` 的 loan
原指向 Group 而未对应物化计划。修复后的结果见表。
package 路径、保留字/分隔、unsigned literal 后缀、deferred 场景、Unit return-value 与
value-origin 断言等夹具曾修正；这些诊断不计作生产缺陷。

`a6ba8d4` 新增 `check_compilation_unit_constant_ownership`、专用 recovery/`ConstEnabledOwnedUnit`
及只读物化计划查询，复用私有 driver 的完整身份门禁，保留 typed owner；即使没有常量声明，
专用来源也不能经基础 recovery validate 绕回旧能力。新增测试先因缺少公开导出报 E0432，
实现后通过。独立复审未发现能力边界漏洞。

`4bf45ed` 以合法跨文件插值复现两个内部 owner 原为 0 drop（应为 2），以及后续 return 原缺少
prefix drop。复用 pending 状态，正常结束显式传入 `AfterExpression`，按所属插值逆序清理；
提前退出复用控制流清理，Abort 丢弃该插值 pending 而不发布清理。嵌套测试断言内层常量先
释放、内层结果与外层常量随后逆序释放，后续仍 live 的 named owner 最后在 CallReturn 释放。
`7b1f49b` 复现右侧 return 缺左 owner 清理、nested 缺两个 prefix 清理、left Abort 后仍发布
不可达 named drop；operand helper 与普通 binary 均传播不继续标志，左 temporary 登记
pending 直到右侧完成。独立复审后另复现 named-left 在右 If 的两个 BranchExit 提前清理；
新增二元期间的 pending borrow 保护，按整个 binary 后继 liveness 清理 named，嵌套外层
保护不被解除。六例覆盖右分支/调用/嵌套与后续再次借用，复审确认该缺口闭合。
`6c126f1` 两项验收先修正参数保留字、lambda 语句形态夹具，再通过全部 13 项公开测试。
10 种标量各两次跨文件交付，明确断言 InlineCopy、use identity、非 Move、无 loan/drop/capture；
双 source 相同局部 ID 的 closure/capture 与常量 drop 分别绑定本 source，反向 inputs 全事实相同。

`6c126f1` 后保留的短路测试先用合法 `false && view(TEXT)` 复现 RHS 仍发布
1 个 StringTemporary（0 passed、1 failed、13 filtered）；当时 `true ||` 及后续 loan/drop
断言未执行。本次同一测试通过，两个分支均确认无 RHS 物化、loan 和 drop。

`66d1a39` 由入口显式开启私有 constant-control 模式，空常量 unit 也启用；基础入口不启用，
不按是否存在常量声明隐式切换。统一主遍历、liveness 与 drop 对 RHS 的执行决定；可靠
静态来源仅为 Boolean literal、typed Boolean constant、Group，其他保留 Conditional。
左侧始终求值；动态 RHS 退出不删除 skip 后继，单边 move 只在 skip BranchExit 清理。
分支编号沿用 If：0=true、1=false，AND RHS 在 0，OR RHS 在 1。实际访问的私有 source-qualified
计划稳定排序，初始化器/未访问尾部不登记；错误/deferred 原子撤销，validate 不接受仅有
materializations 而缺少短路事实的产物。本次补充公开只读列表与 expression 查询，字段仅内部可写；source/三种 RHS 决定/反向 inputs/恢复视图由公开矩阵验证。

独立复审未发现新增阻断，按建议补 Borrow/Value × AND/OR × return/Abort 的外层 pending
组合，验证退出分支与 skip 后正常调用分别清理。既有
`unit_lower_short_circuit_tests::rejects_rhs_only_move_until_frontend_publishes_short_circuit_owner_facts`
仍是基础入口的 MissingFact 契约；后继 SPEC-0227 必须显式消费新计划，不能猜测缺失事实。
下游静态检查发现 unit SSA 当前 temporary 缓存只覆盖 Temporary category/
construction；新增命名 Value operand 的 pending cleanup 必须在 SPEC-0227 显式消费，已加入
该 draft 合同，不将本次 Phase 3 事实描述为 native 支持。

第 5 节行为验收已由最终独立复核确认：不可达路径使用 runtime reads、静态/动态短路及不可达 lambda 测试；
owner identity 与清理使用 Group、双 source、插值、binary、pending prefix 和 skip edge 测试；
literal 对照使用调用前缀、插值与二元控制流矩阵。结论限于分析明确识别的控制退出及短路，
不扩展为任意表达式常量折叠。最终门禁与完成归档已同步。

未运行 frontend 全量测试或 native build/run。本次公开查询涉及跨 crate API，追加 workspace 编译检查；单文件路径未变，复用
`7b1f49b` 的单文件回归及 `a6ba8d4` native 编译契约；下游定向 SSA 检查不证明常量 native 支持。

本次完成审计又复现不可达 lambda 的 orphan drop：`return` 后的 `{ view(TEXT) }` 未登记
物化，但独立 lambda drop 预扫描仍生成 CallReturn Temporary drop。修复以主遍历实际访问的
source-qualified lambda 集合约束专用模式的第二次 liveness 预扫描、drop 及 closure/capture
发布；第一次 liveness 保持完整，基础模式不变。三例覆盖 return 后、Abort 后及静态短路 RHS，
既有双 source 正例验证可达 lambda 保留。独立复审核对嵌套访问与过滤顺序，无新增明确阻断。
初版访问集合使用不实现 Ord 的局部 AST ID 导致 E0277，已改用现有 UnitExpressionId，未扩展 AST trait。

完成归档：2026-09-13。实现最终提交 `18cb692`；复用其 20 项公开、63 项基础、17 项内部测试、
4 项编译拒绝测试及 workspace check（7m 21s）、定向 Clippy、fmt 的通过结果。
本次仅完成生命周期与路由迁移，不重复 Rust 门禁；文档结构、inventory 测试与 diff 检查见本次迁移提交。
