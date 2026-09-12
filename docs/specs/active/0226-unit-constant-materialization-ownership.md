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

新增直接目标拟为 `multifile_constant_ownership`；最近共享契约为 `multifile_ownership_checking`
与 `ownership_constants`，按实际改动选择命中测试。公开阶段产物追加 capability 编译拒绝测试与
[分层验收](../../development/testing.md)要求的编译检查；不运行 frontend 全量测试。
每个稳定实现切片只运行一次最小充分验证，后续仅因新增改动、失败或明确缺口扩展。

| 验收项 / 命令 | 结果 | 证据边界 |
|---|---|---|
| Phase 3 实现与 Rust 验收 | 未执行 | 当前仅建立已批准实施合同 |

合同复核与门禁：独立边界审查通过，已明确动态分支与 owner 转移/清理条件；docs check
（352 Markdown）、文档检查器 21 项测试与 diff check 通过。未运行 Rust：本提交仅建立合同。


### 首切片：私有 unit ownership driver

原公开 `check_compilation_unit_ownership` 继续仅接受基础 validated typed capability，委托私有
`compilation_unit/analysis.rs` driver。身份校验、binding 收集、contracts/capture/dataflow、
错误清理与结果构造，以及对应辅助函数机械迁移；没有复制第二套算法或新增公开转换出口。

该切片只完成独立入口复用的前置，不发布 const owned capability，不宣称已有物化/capture/drop
交付；第 5 节标准保持未完成。独立复审逐字比对迁移主体与辅助函数，确认顺序和可见性不变。

`cargo test -p lang-frontend --test multifile_ownership_checking` 在重构前后均为 63 passed，
0 failed/ignored，覆盖身份错配、capture/loan/drop 和失败清理。无行为变化，不另写镜像测试。

门禁：`cargo clippy -p lang-frontend --lib -- -D warnings`、fmt check、docs check
（353 Markdown）与 diff check 通过。未运行 frontend 全量、workspace/native 或单文件回归：
无公开 API/单文件/下游行为变化。未运行 all-targets clippy；此前记录的既有测试 lint 未修改，
不以 lib clippy 通过代替全目标结果。

### 第二切片：私有 constant use flow 与基础出口限制

私有 driver 的 runtime traversal 跳过 constant initializer；Name/Member use 按 Phase 2
descriptor 提前返回，不访问 declaration/namespace。liveness/drop 使用同一识别，String
二元操作数的 Name 走 temporary，Group 递归保留内部读取 identity。closure root 不返回
常量声明；既有 capture 分类已排除 Constant、Classifier 和 ObjectValue。

ownership provenance 保留常量来源，recovery 的基础 `validate()` 拒绝该来源。没有增加公开
入口或 owned wrapper；物化发布、跨文件与完整控制流验收继续留在本 Spec，不能据本切片
勾选第 5 节完整标准。

失败证据：修正 package 路径、保留字与语句夹具后，`S + S` 的 temporary drop 为 0（应为 2），
且常量 recovery 能通过基础 validate；修复后四项直接测试通过。最初夹具诊断不算实现失败。

| 验收项 / 命令 | 结果 | 证据边界 |
|---|---|---|
| `cargo test -p lang-frontend --lib compilation_unit::constants_tests` | 4 passed，56 filtered，0 ignored | 基础出口拒绝、String Name/Group/Member 二元析构、Borrow/Value temporary、无常量 capture；私有单 source driver |
| `cargo test -p lang-frontend --test multifile_ownership_checking` | 63 passed，0 failed/ignored | 基础 unit 身份门禁、capture/loan/delivery/drop 共享回归 |
| `cargo clippy -p lang-frontend --lib -- -D warnings`、fmt check | 通过 | lib lint；不代表 all-targets lint 通过 |
| docs check、diff check | 通过，353 Markdown | 未变更 inventory，无需重跑检查器测试 |
| 独立只读复审 | 无阻断发现 | 检查 typed use 短路、temporary 与 Group、基础路径不变和 provenance；不代替 Rust 验证 |

未运行 frontend 全量、单文件或 native：本切片仅修改私有 unit 路径，尚未增加跨 crate API，
因此未重复 workspace check。all-targets clippy 的既有测试 lint 未修改；本切片使用 lib clippy。

### 第三切片：私有运行时物化记录

主 traversal 在访问 Phase 2 constant use 时登记精确 descriptor 与 inline/String temporary
类别，按 source-qualified expression 去重；每 source 分析后稳定合并。只有 typed 常量事实
完整、ownership 无诊断且无 deferred 时，recovery 内部才保留完整计划。未增加公开 getter、
专用 owned capability 或 native 入口，不能将私有记录等同于本 Spec 完成交付。

直接失败证据是合法动态分支场景没有 runtime plans；接入后通过。unsigned literal 后缀与
deferred 测试夹具曾修正，夹具失败不算生产缺陷。原先双 source 局部读取矩阵又加强为互相
跨文件读取，明确断言 expression 与 target 来自不同 source，保留相同局部 AST ID 对照。

| 验收项 / 命令 | 结果 | 证据边界 |
|---|---|---|
| `cargo test -p lang-frontend --lib compilation_unit::` | 15 passed，48 filtered，0 ignored | 本次 3 项、此前 4 项常量测试与命中的 8 项 unit 共享契约；非 frontend 全量 |
| `cargo test -p lang-frontend --lib ownership_checking::compilation_unit::materialization_tests` | 3 passed，60 filtered，0 ignored | 最终 11 类型跨 source 精确 descriptor/类别、同局部 ID 与正反 inputs；初始化器/return/Abort 排除、动态分支保留、错误/deferred 原子性 |
| `cargo test -p lang-frontend --test multifile_ownership_checking` | 63 passed，0 failed/ignored | 基础 unit 共享回归 |
| `cargo clippy -p lang-frontend --lib -- -D warnings`、fmt check | 通过 | 未运行带既有测试 lint 的 all-targets clippy |
| docs check、diff check | 通过，353 Markdown | inventory 未变更 |
| 独立只读复审 | 无新增阻断发现 | 检查登记入口、source identity、稳定顺序、分支与失败门禁；不代替执行验证 |

公开能力与完整控制流仍未完成：静态检查发现后续实参提前 return 时，前序 pending String
temporary 的清理义务需要补齐；将在开放专用 owned 验证出口前复现并处理。未运行 frontend
全量、单文件、workspace/native：本切片未增加跨 crate API。
