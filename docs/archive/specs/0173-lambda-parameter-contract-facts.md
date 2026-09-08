# SPEC-0173：Lambda 参数契约 typed facts

## 1. 元数据

| 字段 | 值 |
|---|---|
| 状态 | `done` |
| 所属 Phase | Phase 2 |
| 语言规范 | 现行 [v0.25 callable/lambda 契约](../guides/v0.34-pre-restructure/05-grammar-calls-lambda.md#lambda-literalspec-0010) |
| 前置 Spec | SPEC-0019、SPEC-0067 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` parameter typed fact、lambda expected-type checking、callable 测试、Architecture |
| 语言语义变更 | 否；修复现行 guide 已明确的实现漂移 |
| 批准依据 | 当前持续 Goal“继续推进 guide 主线，分阶段实施 specs”的站立授权 |

## 2. Goal

让具有唯一期望函数类型的 lambda 逐项采用其 `Value` / `Borrow` / `Inout` 参数契约，而不是
错误要求所有参数均为 `Value`；Phase 2 产物按稳定参数 `SymbolId` 保存最终模式，供后续
所有权与 capture 检查消费。

## 3. 范围与需求

- lambda 源码参数仍不书写 marker；参数数量和 `move` 前缀必须与唯一期望函数类型一致。
- 参数数量/`move` 匹配后，无论期望参数是 `Value`、`Borrow` 还是 `Inout`，都设置对应参数
  symbol type 和 mode，并用该期望检查 body。
- 具名函数参数同步登记同一 typed parameter-mode fact，避免 Phase 3 回读 Parser marker；
  enum payload 等非 callable `ValueParameter` 不伪造模式。
- `TypedFile::parameter_mode(SymbolId)`（或等价最小 API）只对成功采用契约的具名/lambda 参数
  返回模式；错误或无期望的带参 lambda 保持 `None`。
- 将 parameter-mode 模型提取到职责明确的小模块，避免继续扩大已达 999 行的 `model.rs`。

## 4. 非目标

- 不实现多 overload 候选的 candidate-isolated lambda body checking；该现行 guide 漂移另建
  后续 Goal，不在本次引入 checker snapshot/rollback。
- 不实现 lambda capture、move closure、`Transferable`、借用冲突、mutability、drop point 或
  v0.26 候选语义。
- 不实现无期望带参 lambda 的反向推导、generic callable 实例化、callable reference 或新诊断。
- 不改变 Parser AST、参数 marker 语法、函数类型身份、L0083/L0084 含义或现有 overload 规则。

## 5. 验收标准

- [x] 显式 local annotation 和唯一 callable argument expected type 下，Borrow/Inout lambda
      compile-pass，表达式类型精确保留期望函数类型。
- [x] named 与 lambda 参数按 `SymbolId` 查询得到 Value/Borrow/Inout；非参数及未采用契约的
      错误 lambda 返回 `None`，重复运行顺序一致。
- [x] `move` 前缀与参数数量仍独立检查；结构不匹配继续产生既有 L0084 且不保存错误模式。
- [x] lambda body 中的调用按最终参数类型/模式继续通过现有 callable mapping；不产生所有权
      借用效果或新增诊断。
- [x] Phase 2 callable pass fixture 覆盖 Borrow/Inout lambda；既有 callable/type/ownership
      回归不变。
- [x] 一次受影响 frontend 窄测和一次 workspace 标准基线通过；Architecture、Spec 路线图、
      验收记录和独立提交同步为实际事实。

## 6. 技术方案与边界

- 新建 `type_checking::parameter`，承载 `ParameterMode`、`FunctionParameterType`；`model.rs`
  继续作为 TypedFile 聚合门面但不新增独立变化原因。
- Checker 维护与名称 symbol table 等长的 `Option<ParameterMode>`；predeclare named signature
  和成功采用 expected function 的 lambda 是唯一写入点，最终原样进入 TypedFile。
- 移除 `check_lambda` 对 expected parameter 全为 Value 的错误结构条件；不改变 body traversal、
  type interning、diagnostic aggregation 或 callable candidate algorithm。

## 7. 实施计划

1. [x] 提取 parameter model 并接入 typed fact → 验证：model 编译与查询单测。
2. [x] 修正唯一 expected lambda 契约采用 → 验证：Borrow/Inout/Value 矩阵。
3. [x] 补 callable fixture 与结构错误回归 → 验证：一次 frontend 受影响测试批次。
4. [x] 同步 Architecture/roadmap/验收并运行一次 workspace 基线 → 验证：实际退出状态。
5. [x] 暂存本 Spec 独立范围并审查 staged diff → 验证：13 个文件均属于本 Spec，未包含
       v0.26 实现或无关改动。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | parameter typed fact、lambda 修复、测试、Architecture 与完成记录 | `fix(frontend): preserve lambda parameter contracts (SPEC-0173)` |

## 9. 未决问题

- candidate-isolated overload lambda checking 已登记为 SPEC-0174 候选。
- ungrouped call-argument lambda header 被外层 `)` 提前截断的既有 Parser 缺陷已登记为
  SPEC-0175 候选；本 Spec 用合法 grouped argument 锁定 expected-type 传播，不掩盖该缺陷。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo test -p lang-frontend --test type_callable --test type_checking --test ownership_checking --test ownership_structural --locked --offline` | 通过 | 47 passed；0 failed / ignored / filtered out |
| workspace Cargo 基线 | 通过 | `git diff --check`、fmt check、workspace check、Clippy `-D warnings`、workspace all-target tests、`cargo build -p lang-cli` 均退出 0 |
| 模块规模 | 通过 | `model.rs` 998 行；新增 `parameter.rs` 48 行，没有扩大超限文件或引入新依赖 |
| 开发中回归审计 | 已登记 | 测试源码误用硬关键字 `value` 后由 Parser 正确拒绝；ungrouped argument lambda owner 缺陷登记为 SPEC-0175 |
