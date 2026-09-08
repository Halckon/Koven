# SPEC-0027: 建立变量所有权状态并检测 use-after-move

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P3-027` |
| 所属 Phase | Phase 3 |
| 语言规范 | 现行 [v0.25](../guides/v0.34-pre-restructure/00-index.md)：[所有权模型](../guides/v0.34-pre-restructure/01-design-decisions.md)、[Phase 3](../guides/v0.34-pre-restructure/06-roadmap.md#phase-3所有权--借用检查) |
| 批准依据 | 用户要求继续推进 guide 主线并分阶段实施 Specs；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0019、SPEC-0020、SPEC-0022、SPEC-0067 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` 独立所有权检查阶段、L0131、compile-pass / compile-fail 测试、Architecture |
| 语言语义变更 | 否；实施 v0.25 已批准的 MoveOnly 整变量移动与 use-after-move 规则 |

## 1. Goal

新增消费 Phase 2 typed facts 的独立所有权检查阶段，以稳定 `SymbolId` 跟踪局部变量与参数的
`Available` / `Moved` 状态；MoveOnly 整变量在已明确的按值交付点移动，后续读取产生 L0131，
Copyable 变量保持可用。阶段不得实现借用、部分移动或析构插入。

## 2. 范围与需求

- 新增公开 `ownership_checking` 模块及入口；校验 ParsedFile、NameResolution、TypedFile 的 source
  identity，内部错误与用户诊断分离，结果持有稳定排序的所有权诊断。
- 以名称解析的 `SymbolId` 区分同名遮蔽；函数参数和初始化完成的局部绑定进入 `Available`。
- 对 Phase 2 已判定为 `Copyability::MoveOnly` 的整变量，在以下已明确按值交付点转为 `Moved`：
  局部/顶层变量 initializer、具名 callable 的 `Value` 实参、显式 `return` value。
- Copyable 变量在相同位置交付 owned copy，不改变状态；temporary 不建立变量状态。
- 读取 `Moved` 变量产生唯一 L0131 `use of moved value`，primary 指向本次名称，关联 label 指向
  首次使当前值变为 Moved 的名称 Span；重复后续读取各自形成源码有序诊断。
- `var` 经合法普通赋值获得新值后恢复 `Available`；`val` 的赋值合法性继续由 Phase 2 负责。
- 顺序 block 按源码更新状态；`if` / `when` 分支在所有可继续路径合并，任一路径已移动即保守为
  Moved；loop body 可能移动的变量在 loop 后视为 Moved。return/break/continue 的不可达路径不得
  污染继续路径。
- 类型或名称事实缺失的错误节点不追加所有权级联；已有 Phase 2 诊断保持原顺序与身份。

## 3. 非目标

- 不实现 SPEC-0028 的消费式解构、字段投影或完整条件复制矩阵。
- 不实现 SPEC-0029 的 Borrow / Inout、借用活跃区、冲突检查或 ASAP 析构点。
- 不移动成员、索引 place 或不可复制字段，不建立部分移动状态。
- 不实现 closure capture、顺序容器 owner、`Transferable`、Map 或 codegen。
- 不新增依赖、日志、全局可变状态或 LLVM 表示。

## 4. 验收标准

- [x] MoveOnly 参数/局部在 initializer、Value 实参、return 后再次使用产生精确 L0131。
- [x] Copyable 同类路径可重复使用且零所有权诊断。
- [x] SymbolId 遮蔽、`var` 重新赋值、多个后续 use 与首次 move label 精确。
- [x] `if` / `when` / loop 的继续路径合并拒绝可能已移动值，不受终止路径污染。
- [x] Borrow / Inout、temporary、错误 AST 与缺失 typed facts 不被误判为整变量移动。
- [x] 所有权入口的 source identity 错误确定且不形成用户诊断。
- [x] compile-pass / compile-fail 与窄 Clippy 通过。
- [x] workspace 标准基线通过，无 ignored / skipped。
- [x] Architecture、roadmap、Spec 索引与独立提交同步。

## 5. 技术方案与边界

新增独立 checker，读取 AST、名称引用、`TypedFile::copyability` 与 `CallDescriptor` 参数映射；
状态表使用 `BTreeMap<SymbolId, OwnershipState>` 保证确定性。表达式 walker 显式接收
`ExpressionUse::{Read, Consume, Assign}`，控制流复制小型状态映射并做保守 join；不修改 typed AST，
不把所有权状态塞回 Phase 2 checker。

## 6. 实施计划

1. [x] 审计 guide、Phase 2 typed facts 与路线图 → 验证：0027 是最早无门禁主线 Goal。
2. [x] 建立所有权产物、错误边界、L0131 与整变量状态机 → 验证：integration test 覆盖状态转换。
3. [x] 遍历 callable / block / expression 与控制流 join → 验证：正反 integration matrix。
4. [x] 运行窄测试 / Clippy并同步 Architecture / roadmap → 验证：全部通过。
5. [x] 运行 workspace 基线、审阅 staged diff并独立提交 → 验证：提交信息包含 `SPEC-0027`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | 变量所有权状态、L0131、测试、Architecture 与完成记录 | `feat(frontend): detect use after move (SPEC-0027)` |

## 8. 未决问题

- 无；借用与析构规则明确留给 SPEC-0029，不阻塞本 Spec 的整变量状态。

## 9. 验证记录

- `cargo test -p lang-frontend --test ownership_checking --locked --offline`：6 passed，0 failed，
  0 ignored，0 filtered。
- `cargo clippy -p lang-frontend --all-targets --locked --offline -- -D warnings`：通过，0 warnings。
- `git diff --check` 与 `cargo fmt --all -- --check`：通过。
- `cargo check --workspace --all-targets --locked --offline`：通过。
- `cargo clippy --workspace --all-targets --locked --offline -- -D warnings`：通过，0 warnings。
- `cargo test --workspace --all-targets --locked --offline`：全部 test target 通过，0 failed、
  0 ignored、0 filtered。
- `cargo build -p lang-cli --locked --offline`：通过。
