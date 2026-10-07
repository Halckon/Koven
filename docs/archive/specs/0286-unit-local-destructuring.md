# SPEC-0286: 编译单元局部解构 SSA Lowering 与原生执行 (`val (a, b) = expr`)

> **性质**：变更合同 · **状态**：done · **读取时机**：追溯编译单元局部解构 lowering 与原生执行设计与实现时 · **唯一真源**：本 Spec

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P4-0286` |
| 所属 Phase | Phase 4 SSA Lowering 与 native 原生运行验证 |
| 语言规范 | 现行 [Guide v0.41](../../guide/README.md)；[集合与解构](../../guide/12-collections-destructuring.md) |
| 批准依据 | 用户持续推进里程碑授权；Guide v0.41 §12 局部 `val` 解构语义 |
| 前置 Spec | SPEC-0280 ~ SPEC-0285 动态容器操作已全部合并入 main |
| 前置 ADR | [ADR-0008](../../adr/accepted/0008-internal-value-and-allocation-abi.md)、[ADR-0016](../../adr/accepted/0016-interprocedural-borrow-abi.md) |
| 关联 ADR | 无新增 ABI |
| 阻塞项 | 无 |
| 影响范围 | `lang-codegen` compilation unit SSA lowering (`unit_lower.rs`, `unit_lower/aggregate.rs`) 与 native 测试 |
| 语言语义变更 | 否（打通 Guide v0.41 §12 局部解构在 compilation-unit 的未连接后端） |

## 1. Goal

在编译单元（compilation unit）多文件入口交付 `val (a, b) = initializer` 局部解构语句的完整 SSA lowering 与原生执行能力：
- 消除 `unit_lower.rs` 中对 `Statement::LocalDestructuring` 的 `UnsupportedNode` 限制；
- 消费前端 compilation unit 类型检查与所有权检查已发布的 `UnitDestructuringDescriptor`；
- 根据解构模式发射对应的 SSA 解构原语：
  - `DestructuringMode::Copy`：发射 `Operation::AggregateCopyExplode { aggregate }`，解构各分量并保留源值；
  - `DestructuringMode::Consume`：发射 `Operation::AggregateExplode { aggregate }`，原子消费源聚合并转移各分量所有权；
- 将各分量的 SSA 实体正确绑定至对应局部符号；
- 编写端到端 SSA lowering 与 native 运行测试，覆盖 Copyable 与 MoveOnly 结构解构的正确执行与零泄漏生命周期。

## 2. 背景与需求

Koven v0.41 Guide §12 明确规范：
> “对 `value class`，编译器使用内建结构解构，不展开为普通方法调用：
> 1. 编译器把所有分量绑定作为一次结构化操作检查。若源类型满足 `Copyable`，绑定获得分量的复制，源值仍可使用。
> 2. 若源类型不满足 `Copyable`，解构消费整个源值并一次性转移各分量的所有权；操作完成后源值不可使用，所有拥有资源的字段最终只能析构一次。
> 3. v1 的消费式结构解构必须覆盖主构造器的全部分量，并且每个分量恰好绑定一次；部分结构解构不支持。”

目前现状：
- 前端词法与语法已具备 `Statement::LocalDestructuring`；
- 前端单文件与 compilation unit 类型检查（`check_local_destructuring`）已完整发布 `DestructuringDescriptor` 与 `UnitDestructuringDescriptor`；
- 前端所有权检查（liveness 与 drop planning）已完整为解构分量规划生命周期；
- 单文件 SSA lowering（`lower_frontend/aggregate.rs`）已实现 `lower_destructuring` 并正常运行；
- 然而，多文件编译单元后端（`crates/lang-codegen/src/ssa/unit_lower.rs:976`）仍将 `Statement::LocalDestructuring` 视为 `UnsupportedNode`，导致跨模块使用 `val (a, b) = pair` 时报错崩溃，形成单文件与多文件编译单元的关键能力断层。

本 Spec 补齐 compilation unit 的 `lower_destructuring`，消除这一断层。

## 3. 范围与技术方案

1. **Compilation Unit SSA Lowering**：
   - 在 `crates/lang-codegen/src/ssa/unit_lower/aggregate.rs` 中实现 `lower_destructuring(&mut self, statement: StatementId, initializer: ExpressionId, span: Span)`；
   - 从 `self.typed.destructuring(id)` 获取 `UnitDestructuringDescriptor`；
   - 递归降低右值 `initializer`，取得聚合的 SSA `ValueId`；
   - 解析各分量的 SSA 类型；
   - 根据 descriptor 的 `mode` 发射 `Operation::AggregateCopyExplode` 或 `Operation::AggregateExplode`；
   - 若为 `DestructuringMode::Consume`，调用 `transfer_owned_expression` 转移源聚合所有权；
   - 将结果实体逐一存入 `self.bindings` 供后续局部变量引用；
   - 在 `crates/lang-codegen/src/ssa/unit_lower.rs` 中，将 `Statement::LocalDestructuring { initializer, .. }` 路由至 `self.lower_destructuring(statement, *initializer, span)`。
2. **测试验证**：
   - SSA 单元测试：验证 multi-file 场景下生成合法的 `aggregate.copy_explode` 与 `aggregate.explode` 指令；
   - Native 运行测试：
     - 多文件 `IntPair(10, 32)` Copyable 解构原生运行验证，正确计算并输出 `42`；
     - 多文件包含 MoveOnly 资源的 `ResourcePair(Leaf("left"), Leaf("right"))` 解构原生运行验证，确认资源按需转移且在退出时正确逆序析构（先 `right` 后 `left`），经 `run_counted_allocations` 确认精确两次分配，零内存泄漏与双重释放。

## 4. 非目标

- 不支持非 `value class` 的普通类解构（Guide 明确要求等待 `componentN` 方法开放）；
- 不支持部分解构或下划线 `_` 忽略分量（语法规范明确拒绝）；
- 不支持顶层解构声明（仅限 block / 函数局部）。

## 5. 验收标准

- [x] G1: 编译单元支持 Copyable `value class` 的局部解构，分量绑定正确，源值可继续使用。
- [x] G2: 编译单元支持 MoveOnly `value class` 的局部解构，源值原子消费，分量所有权移出并由新局部变量持有。
- [x] G3: MoveOnly 分量在作用域结束时恰好析构一次，验证端到端零内存泄漏与双重释放。
- [x] G4: 生成并通过类型完备的 SSA 指令（`aggregate.copy_explode` 与 `aggregate.explode`）。
- [x] G5: 双宿主（macOS arm64 / Linux x86_64）native 测试全绿，通过架构及尺寸门禁。

## 6. 验证记录

- **Phase 1 (规范与拓扑)**:
  - 产物：`docs/specs/active/0286-unit-local-destructuring.md`，更新拓扑并运行 `check_docs.py` 通过。
- **Phase 2 (SSA Lowering 实现)**:
  - 修改 `crates/lang-codegen/src/ssa/unit_lower/aggregate.rs`，实现 `lower_destructuring`；
  - 修改 `crates/lang-codegen/src/ssa/unit_lower.rs`，分派 `Statement::LocalDestructuring`。
  - `cargo check -p lang-codegen` 0 警告通过。
- **Phase 3 (测试与门禁)**:
  - SSA 测试：`cargo test -p lang-codegen --lib -- lowers_compilation_unit_copyable_and_move_only_destructuring` 通过；
  - Native 测试：`cargo test -p lang-codegen --lib -- unit_destructuring_native_move_only_and_copyable_execution` 在 `constants = false` 与 `constants = true` 下均执行通过且内存统计精确为 2 次无泄漏；
  - 套件测试：`cargo test -p lang-codegen --lib -- resource_deinit_tests` 33 项全绿；
  - 门禁：`check_rust_sizes.py` 100% 绿灯；`cargo clippy --all-targets` 零告警通过；`cargo fmt --check` 通过。
- **Phase 4 (归档与 PR 闭环)**:
  - 归档至 `docs/archive/specs/0286-unit-local-destructuring.md`。
