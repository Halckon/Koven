# SPEC-0209：关联常量 SSA/LLVM 重新物化

## 1. 元数据

| 字段 | 值 |
|---|---|
| 状态 | `draft` |
| Goal ID | `KOV-P4-209` |
| 所属 Phase | Phase 4 |
| 语言规范 | 起草基线 v0.32；候选 [v0.36 §36](../guide/01-design-decisions.md#36-无运行时存储的关联常量与封闭求值v036-候选未启用) |
| 批准依据 | 无；v0.36 尚未启用，且尚未显式重基到现行 v0.34 |
| 前置 Spec | SPEC-0034、0039、0185、0189、0192 `done`；SPEC-0026/0208 待完成 |
| 前置 ADR | ADR-0008、0010、0018 `accepted` |
| 阻塞项 | 明确 v0.36 对现行 v0.34 的重基与取代关系；v0.36 启用；SPEC-0026/0208 `done` |
| 影响范围 | `lang-codegen` frontend lowering/SSA/LLVM/native tests；Roadmap/Architecture |
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

- [ ] top-level/object/class/value/enum/interface companion 的 Boolean/整数/Char const native 正例通过。
- [ ] Char verifier 接受 U+0000、U+D7FF、U+E000、U+10FFFF，拒绝 surrogate 与超出 Unicode
  scalar range 的内部 constant；LLVM 使用 `i32` 且 SSA 类型不与 `UInt32` 混同。
- [ ] String const 多次用于 concat/equality/println/return，stdout 与 drop/owner 计数正确。
- [ ] const/object declarations 与 scalar/parameterized entry 共存；LLVM 无 singleton/global-init 符号。
- [ ] SSA/verifier 拒绝错误 constant type/value，lowerer 对缺 validated facts 在落盘前失败。
- [ ] 两次 SSA/LLVM/object 行为确定，既有 literal/String/declarative-root/native suite 回归。
- [ ] Architecture/Roadmap 同步。

## 6. 技术方案与边界

为 constant-use descriptor 增加专用 lowering dispatch；Boolean/整数进入既有 constant operation，
Char 增加独立的 SSA type/constant/verifier 分支并在 LLVM 边界映射为 `i32`，String 调用既有
literal adapter。声明本身仍由 declarative-root 筛选器忽略，不新增 SSA global identity 或 runtime API。

## 7. 实施计划

1. [ ] 接 const facts到 scalar/Char/String SSA → 验证：lowering/verifier 窄测试。
2. [ ] 接 LLVM/native 与 declarative roots → 验证：object/link/run、stdout、IR 断言。
3. [ ] 同步验收与 Architecture → 验证：codegen、workspace、fmt/clippy 基线。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | SSA/LLVM/native 与完成文档 | `feat(codegen): lower associated constants (SPEC-0209)` |

## 9. 未决问题

- 无；跨文件集成由 SPEC-0210 承接。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-27 roadmap 审计 | 通过 | Boolean/整数/String literal operations 已存在；Char 尚缺独立 SSA/verifier/lowering，constant name use 与 object/const roots 仍确定性 unsupported |
