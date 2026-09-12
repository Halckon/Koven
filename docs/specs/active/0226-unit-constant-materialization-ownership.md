# SPEC-0226：跨文件常量重新物化与所有权

> **性质**：实施 Spec · **状态**：in-progress · **读取时机**：实施 v0.36 unit 常量 Phase 3 时 · **唯一真源**：本 Spec

| 字段 | 值 |
|---|---|
| 状态 | `in-progress` |
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

- [ ] 两文件相同局部 AST ID、正逆 inputs 均产生相同 source-qualified 物化/capture/drop 事实。
- [ ] 全部 11 种闭合常量类型按 typed value 交付；标量无 owner，String 多次 use 的 owner 不共享。
- [ ] 初始化器依赖及分析已确认不可达的读取不发布物化；动态分支保留条件执行事实；常量读取不新增 namespace capture。
- [ ] 声明/namespace 不进入 moved/loan/owner/drop 状态；标量重复交付不移动声明，也不新增 loan/drop。
- [ ] String temporary/root/drop 绑定具体 use 表达式；每个正常退出路径中，未转移的 owner 恰好清理一次，已转移的 owner 不重复清理。
- [ ] String 的 Borrow/Value/return、重叠借用、表达式嵌套和控制流退出与对应 literal 行为一致。
- [ ] ownership 错误不发布部分物化/可执行事实；混用分析、改动 inputs 或 typed owner 被确定性拒绝。
- [ ] 新旧 capability 不可混用；现有基础 unit ownership 与单文件 constant ownership 最近回归通过。
- [ ] Architecture、验证记录与完成状态同步；未运行项如实保留。

## 6. 技术方案与边界

在 unit ownership 门面分离公共 capability 校验与内部既有分析 driver，新入口共享内部算法；
不得为复用而公开将 const-enabled typed 强转为基础 validated 的方法。运行时 traversal/capture
在消费 const use 时区分编译期 identity 与值物化，String 复用 ordinary literal owner/drop 路径。

物化 descriptors 和 owned wrapper 的最终命名由实现遵循现有模块风格确定；其身份校验、
失败原子性与无基础转换出口为硬边界。若已有 unit 图缺少表达某条现行清理路径的事实，
在本 Spec 内补最小事实并以 literal 对照验证，不绕过该路径或修改 guide。

## 7. 实施计划

1. [ ] 接独立输入/owned capability 和 identity gate → 验证：错配拒绝与基础接口契约。
2. [ ] 接 runtime constant use、capture 与 String owner/drop → 验证：直接正反例、literal 对照。
3. [ ] 扩展跨文件/控制流矩阵并同步文档 → 验证：本节与第 5 节逐项关联实际证据。

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
当前尚无公开常量 owned capability，第 5 节完整标准保持未完成。

| 验收项 / 命令 | 实际结果 | 证据边界 |
|---|---|---|
| `cargo test -p lang-frontend --test multifile_ownership_checking` | 本次 63 passed，0 failed/ignored | 基础 unit 身份、capture/loan/delivery/drop；`adb760b` 重构前后也各 63 passed |
| `cargo test -p lang-frontend --lib compilation_unit::constants_tests` | `645c3e1`：4 passed，56 filtered，0 ignored | 基础 owned 出口拒绝、String Name/Group/Member 二元析构、Borrow/Value temporary、无常量 capture |
| `cargo test -p lang-frontend --lib ownership_checking::compilation_unit::` | 本次 15 passed，54 filtered，0 ignored | 当前 unit ownership 内部契约、常量 flow/物化、pending cleanup 与 Group identity |
| `cargo test -p lang-frontend --lib groups_forward_the_same_materialization_owner` | 本次最终 1 passed，68 filtered，0 ignored | Name/Member × 一/三层 Group；loan、delivery、Borrow/未提交 Value/discard drop 均对应物化计划，三个 owner 互不相同 |
| `cargo test -p lang-frontend --lib compilation_unit::` | `4832f55`：15 passed，48 filtered，0 ignored | 当时 7 项常量与命中的 8 项 unit 共享契约；非全量 frontend |
| `cargo test -p lang-frontend --lib ownership_checking::compilation_unit::materialization_tests` | `4832f55` 最终：3 passed，60 filtered，0 ignored | 11 类型跨 source 精确 descriptor/类别、同局部 ID、正反 inputs；初始化器/return/Abort 排除、动态分支保留、错误/deferred 原子性 |
| `cargo test -p lang-frontend --lib ownership_checking::compilation_unit::pending_temporary_tests` | `b3851d4`：5 passed，63 filtered，0 ignored | 常量/literal × Borrow/Value × 6 种退出/正常调用，共 24 例；内层循环保留、命名 Value、精确析构顺序、return operand Abort；本次也由上述 15 项覆盖 |
| `cargo test -p lang-codegen --lib ssa::unit_lower_borrow_tests` | `b3851d4`：2 passed，400 filtered，0 ignored | 最近基础 SSA 借用消费回归，不证明新增常量 native 支持 |
| `cargo test -p lang-codegen --lib ssa::unit_lower_loop_tests` | `b3851d4`：7 passed，395 filtered，0 ignored | 最近基础 SSA 循环/退出消费回归 |
| `cargo clippy -p lang-frontend --lib -- -D warnings`、fmt check | 本次通过 | all-targets clippy 的既有测试 lint 未修改，不以 lib 通过替代 |
| docs check、diff check | 本次通过，353 Markdown | inventory 未变化；合同建立时另有检查器 21 项测试通过 |
| 独立只读复审 | 各逻辑切片无剩余阻断发现 | 私有 driver 机械迁移、常量 flow/物化门禁、pending cleanup/Abort；本次复核 Group 归一并按建议补三个 owner 独立性断言 |

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

本次复用单文件既有 Group 归一规则，仅穿透 Group 并要求终端是当前 source 的 typed constant
use；loan、Value delivery、temporary origin 与发布的 drop 统一指向实际读取。drop 来源 Span
归一，原实参/调用/清理位置不变。独立复审后补了三个 drop owner 互不相同的断言。

### 失败证据与待完成项

有效失败证据包括：`S + S` 原 temporary drop 为 0（应为 2）、常量 recovery 原能通过基础
validate、合法动态分支原无 runtime plans、后续实参提前 return 缺 pending cleanup，以及
合法非 Unit callable 的 `return stop()` 原错误发布 return cleanup，以及 `(TEXT)` 的 loan
原指向 Group 而未对应物化计划。修复后的结果见表。
package 路径、保留字/分隔、unsigned literal 后缀、deferred 场景、Unit return-value 与
value-origin 断言等夹具曾修正；这些诊断不计作生产缺陷。

专用 constant owned capability 尚未开放，身份门禁和剩余完整矩阵
仍需继续。下游静态检查发现 unit SSA 当前 temporary 缓存只覆盖 Temporary category/
construction；新增命名 Value operand 的 pending cleanup 必须在 SPEC-0227 显式消费，已加入
该 draft 合同，不将本次 Phase 3 事实描述为 native 支持。

未运行 frontend 全量、单文件回归、workspace check 或 native build/run：本次仅修改 unit
Phase 3 私有实现，无跨 crate API 变更；本次 Group 归一只命中常量 use，未重复下游 SSA 检查，
表中下游结果属于 `b3851d4`。
