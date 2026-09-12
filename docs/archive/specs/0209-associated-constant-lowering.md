# SPEC-0209：关联常量 SSA/LLVM 重新物化

> **性质**：实施 Spec · **状态**：done · **读取时机**：实施或评审 v0.36 对应 Goal 时 · **唯一真源**：本 Spec

## 1. 元数据

| 字段 | 值 |
|---|---|
| 状态 | `done` |
| Goal ID | `KOV-P4-209` |
| 所属 Phase | Phase 4 |
| 语言规范 | 现行 [v0.36 §36](../../guide/05-declarations-callables.md#36-无运行时存储的关联常量与封闭求值) |
| 批准依据 | 2026-09-12 用户明确启用 v0.36 并要求分阶段实施；依赖未完成者保持 draft |
| 前置 Spec | SPEC-0026、0034、0039、0185、0189、0192、0208 `done` |
| 前置 ADR | ADR-0008、0010、0018 `accepted` |
| 阻塞项 | 无 |
| 影响范围 | `lang-codegen` frontend lowering/SSA/LLVM/native tests；Architecture |
| 语言语义变更 | 否；实施启用后的 v0.36 single-file native lowering |

## 2. Goal

完成后，单文件顶层/object/companion const use 只消费 validated const/materialization facts，
lower 为 scalar/Char constant 或普通 String literal owner，并与声明型 roots、entry、object/link/run
共存而不生成 runtime global、singleton 或初始化代码。

## 3. 范围与需求

- Boolean/整数复用 verified `Operation::Constant`；Char 新增独立的 IR-local `Char` type 与
  constant contract（如 `SsaTypeKind::Char` / `ScalarConstant::Char(u32)` 的等价表示），verifier
  拒绝 surrogate 与大于 `0x10FFFF` 的值，LLVM 映射为 `i32`，不得擦除成 `UInt32`；String
  复用 SPEC-0192 `StringLiteral` owner，不重新编码 UTF-8 或建立 global owner。
- 每个 use 按 0208 产生独立 value/owner；lowerer 不读取 initializer AST、重新执行 evaluator或
  根据 symbol 名称猜测值。
- 未使用的 top-level/object/companion const 只是 declaration root；不进入 callable reachability，
  不产生 LLVM global、init guard、singleton address 或退出 drop。
- 缺失/不一致 facts 在 object 写盘前 fail loud 为内部 lowering boundary，不分配新的源码 L-code。

## 4. 非目标

- 不实现跨文件 const、Float/Double、associated function、object instance method、一般 CTFE、
  runtime global/init 或稳定跨 object constant ABI。新增 Char contract 只存在于编译器 IR/LLVM
  lowering 边界，不定义新的 runtime/global ABI。

## 5. 验收标准

- [x] top-level/object/class/value/enum/interface companion 的 Boolean/整数/Char const native 正例通过。
- [x] Char verifier 接受 U+0000、U+D7FF、U+E000、U+10FFFF，拒绝 surrogate 与超出 Unicode
  scalar range 的内部 constant；LLVM 使用 `i32` 且 SSA 类型不与 `UInt32` 混同。
- [x] String const 多次用于 concat/equality/println/return，stdout 与 drop/owner 计数正确。
- [x] const/object declarations 与 scalar/parameterized entry 共存；LLVM 无 singleton/global-init 符号。
- [x] SSA/verifier 拒绝错误 constant type/value，lowerer 对缺 validated facts 在落盘前失败。
- [x] 两次 SSA/LLVM/object 行为确定，既有 literal/String/declarative-root/native suite 回归。
- [x] Architecture 同步。

## 6. 技术方案与边界

为 constant-use descriptor 增加专用 lowering dispatch；Boolean/整数进入既有 constant operation，
Char 增加独立的 SSA type/constant/verifier 分支并在 LLVM 边界映射为 `i32`，String 调用既有
literal adapter。声明本身仍由 declarative-root 筛选器忽略，不新增 SSA global identity 或 runtime API。

## 7. 实施计划

1. [x] 接 const facts到 scalar/Char/String SSA → 验证：lowering/verifier 窄测试。
2. [x] 接 LLVM/native 与 declarative roots → 验证：object/link/run、stdout、IR 断言。
3. [x] 同步验收与 Architecture → 验证：按[分层验收](../../development/testing.md)选择目标测试与必要下游检查，并记录命中数。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | Char IR-local type/constant/verifier/LLVM 与定向测试 | `feat(codegen): add distinct Char SSA constants (SPEC-0209)` |
| 2 | validated const/materialization 接线、SSA/LLVM 与 native 冒烟 | `feat(codegen): lower validated constant uses (SPEC-0209)` |
| 3 | 完整 native 矩阵、owner/entry/拒绝边界与完成文档 | `test(codegen): complete constant native acceptance (SPEC-0209)` |

## 9. 未决问题

- 无；跨文件集成由 SPEC-0210 承接。

## 10. 验证记录

实施前按[分层验收](../../development/testing.md)将第 5 节各项映射到实际测试目标/过滤器；
记录命中数、结果与未运行原因。同一状态下的有效证据只运行一次，不默认运行 frontend 全量。

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-27 roadmap 审计 | 通过 | Boolean/整数/String literal operations 已存在；Char 尚缺独立 SSA/verifier/lowering，constant name use 与 object/const roots 仍确定性 unsupported |


### 首切片：Char 的 IR-local 契约

新增 `SsaTypeKind::Char`、`ScalarConstant::Char(u32)`；verifier 用 Unicode scalar 校验而非整数范围
代替，Char/UInt32 双向错配均拒绝。Char 为 Copyable/first-class，可作相等/不等比较，数值算术和
排序仍只接受 Integer。SSA renderer 显式显示 char/U+codepoint；LLVM type/layout/constant 使用
独立 SSA Char 所映射的 i32，copyable drop 路径不引入 runtime owner。

本切片未接 frontend Char 或 constant-use descriptor，不证明 const native、declarative roots、
String 物化或 Char 调用实参支持；没有新增公开 crate API。第 5 节只勾选独立 Char 契约。

| 检查 | 结果 | 证据边界 |
|---|---|---|
| `cargo test -p lang-codegen --lib char_constant_tests` | 4 passed，389 filtered | Unicode 四边界、surrogate/越界/类型错配拒绝、确定 LLVM i32、严格 renderer；非恒定参数 Copy、Char call 结果类型与 EQ/NE，拒绝加减乘及四种排序 |
| `cargo test -p lang-codegen --lib -- verify_scalar_tests llvm::layout_tests llvm::tests ssa::type_tests` | 26 passed，367 filtered | 最近 scalar verifier、LLVM adapter、layout 与类型契约回归；0 failed/ignored |
| 独立代码与测试复审 | 无具体实现缺陷；首轮测试缺口已补齐 | 枚举消费者、UInt32 隔离、非法码点、Integer-only 边界、LLVM layout/drop；未额外运行 Cargo |

失败证据：实现前定向测试因缺少 Char type/constant variant 编译失败；实现后四项通过。
未运行 frontend 全量、workspace check 或 object/link/run：内部 SSA 契约未新增公开 API，也尚未
接入源码/native 入口。后续切片必须补 const/native 行为，不能以本表代替。

附加门禁：`cargo clippy -p lang-codegen --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`python3 scripts/check_docs.py`（351 Markdown）及 `git diff --check` 均通过。


### 第二切片：单文件 constant-use 接线

`lower_frontend/constant.rs` 只消费 validated descriptor/materialization；入口验证能力与分析身份，
每次 use 核对计划的 expression/target/type/value。String 走普通 literal owner 与既有临时值清理，
String binary view 优先识别 const，避免把常量名当成本地 binding。顶层 const 和 object 声明不进入
运行时函数图；Char 加入单文件 builtin 映射。没有新增公开 crate API。

| 验收项 / 命令 | 结果 | 证据边界 |
|---|---|---|
| 失败复现：`cargo test -p lang-codegen --lib constant_lowering_tests` | 实施前 3 failed，393 filtered | 三组有效源码均被声明 root 拒绝；接线后暴露 String binary view 的 Name 旁路，已修复 |
| `cargo test -p lang-codegen --lib lower_frontend_tests` | 37 passed，360 filtered | 常量四项、既有 scalar/String/declarative roots、mixed-analysis 与控制流/借用回归 |
| `cargo test -p lang-codegen --lib -- constant_lowering_tests associated_constants_reach_native_scalar_char_and_string_operations dynamic_strings_cross_borrow_value_and_return_boundaries_with_exact_bytes` | 6 passed，392 filtered | 加强精确 LLVM 返回值断言；五种关联 namespace、八种整数、Boolean/Char、4 次 String literal owner、重复 SSA/LLVM；新增 const native stdout 与既有 dynamic String native 回归 |
| 独立审查 | 未发现阻断实现缺陷 | facts 来源、initializer 隔离、String temporary/group/drop 路径、Char 类型、声明 roots |

本切片不宣称第 5 节整体完成：完整 namespace/type native 矩阵、parameterized entry、专门的
owner/drop 计数、const 错配在 object 落盘前拒绝及重复 object 行为仍待后续切片。未运行 frontend
全量和 workspace check；变更仅涉及 codegen 内部接线，未新增公开 API。

附加门禁：`cargo clippy -p lang-codegen --all-targets -- -D warnings`、`cargo fmt --all -- --check`、
`python3 scripts/check_docs.py`（351 Markdown）及 `git diff --check` 均通过。补充独立复审确认 native
测试确实 object/link/run 且核对精确 stdout，标量测试明确断言 LLVM 位宽/值；未扩大计数证据。


### 第三切片：native 验收闭合

`native_constant_tests` 将第 5 节验收映射到四个真实 native 测试。源码经既有 facade 生成 object，
由 clang 链接并执行；String 计数测试对 verified LLVM 的 allocator/free 与 String drop 调用插桩，
再编译执行。未新增生产逻辑、runtime API 或公开 crate API。

| 第 5 节验收 | 测试目标 / 过滤器 | 实际结果与边界 |
|---|---|---|
| namespace/type native 正例 | `constant_types_run_in_every_supported_namespace` | 6 namespace × Boolean/8 integer/Char = 60 组合通过；整数与显式类型 literal oracle 比较，Char 覆盖返回/EQ/NE，精确码点由前述 SSA/LLVM 断言共同证明 |
| String concat/equality/println/return 和 owner | `string_constant_owners_drop_once_without_allocating_literal_buffers` | 精确 UTF-8 stdout；9 literal + 2 concat owner，共 11 次动态 drop；2 malloc/2 free 且逐指针核对 live allocation，无 literal heap allocation |
| scalar/argv entry、重复 object | `constants_coexist_with_argument_entry_and_repeatable_objects` 及 declarative roots 回归 | 借用 argv 与 const/object 共存，两次 object 字节相同、两次运行 stdout 相同；此前 scalar SSA 测试证明无常量 global/init，String 只复用 ADR-0018 允许的 private constant bytes |
| 非法值 / facts 隔离 | `constant_analysis_mismatch_and_invalid_values_never_write_objects` | 同 SourceId/SymbolId 的两轮分析不可混用；Byte overflow 抑制两份常量 capability，均在 object 落盘前拒绝；内部 type/value 非法组合由首切片 verifier 测试覆盖 |
| 确定 SSA/LLVM、最近回归 | 前述 `constant_lowering_tests` 与本切片定向回归 | 复用第二切片重复 SSA/LLVM 证据；旧 object-root 拒绝断言先复现失败，再按 v0.36 改为 const/object 正例，普通运行时全局变量继续拒绝 |

`cargo test -p lang-codegen --lib native_tests::constant_tests`：4 passed，398 filtered，0 failed/ignored。
初次运行中的整数比较缺少显式类型、非 Unit entry、保留字 `borrow` 与裸表达式序列均为测试夹具
错误，按既有语法修正；未为通过测试修改生产语义。

独立复审确认了四项测试的执行路径及 9+2 owner 计数推导。动态 drop 是聚合计数，逐 owner
身份/唯一消费还由 verified SSA 证明；不把聚合计数单独宣称为 literal identity 跟踪。Char 的
native 比较与精确 LLVM 码点断言构成组合证据，不宣称 raw Char literal lowering 已实现。


最近回归：`cargo test -p lang-codegen --lib -- declarative_roots_emit_with_scalar_entry_while_runtime_globals_stay_unsupported char_constant_tests llvm::string_tests dynamic_strings_cross_borrow_value_and_return_boundaries_with_exact_bytes`：8 passed，394 filtered，0 failed/ignored。

第 5 节均已有对应证据；SPEC-0209 完成。没有运行 frontend 全量或 workspace check：本切片仅改
codegen 测试，不改变公开 API。跨文件 typed 集成继续由 SPEC-0210 承接，整体 v0.36 目标尚未完成。

附加门禁：`cargo clippy -p lang-codegen --all-targets -- -D warnings` 与 `cargo fmt --all -- --check` 通过。

文档门禁：`python3 scripts/check_docs.py`（351 Markdown）、`python3 -m unittest discover -s scripts/tests -p 'test_check_docs.py'`（21 passed）及 `git diff --check` 通过。最终独立逐项审计未发现验收阻断；指出的索引状态/计数已按实际文件更新。
