# SPEC-0028: 检查条件复制、消费式解构与结构分量移动

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P3-028` |
| 所属 Phase | Phase 3 |
| 语言规范 | 现行 [v0.25](../guides/v0.34-pre-restructure/00-index.md)：[条件复制与字段 place](../guides/v0.34-pre-restructure/01-design-decisions.md)、[结构化解构](../guides/v0.34-pre-restructure/01-design-decisions.md#11-解构声明与-componentn-约定)、[Phase 3](../guides/v0.34-pre-restructure/06-roadmap.md#phase-3所有权--借用检查) |
| 批准依据 | 用户要求继续推进 guide 主线并分阶段实施 Specs；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0022、SPEC-0027 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` 字段/结构分量 typed facts、所有权检查、L0132、Phase 3 fixture、Architecture |
| 语言语义变更 | 否；实施 v0.25 已批准的条件复制、原子消费式解构和禁止部分移动规则 |

## 1. Goal

让 Phase 3 消费现有 `Copyability` 与 `DestructuringDescriptor`：完整 value-class 解构按
`Copy` / `Consume` 原子执行；字段投影与自动 `componentN()` 只允许复制可复制分量，拒绝从
聚合中移出不可复制分量，且不建立部分移动状态。

## 2. 范围与需求

- 将有效的 `DestructuringMode::Consume` initializer 作为一次整值按值交付；若 initializer
  是 MoveOnly 变量则整体进入 Moved，全部 binding 同时进入 Available。`Copy` 模式只读取源值。
- 复用 Phase 2 的实际类型实参替换和 `Copyability`，覆盖 value class、enum、nullable、
  intrinsic `Box`、无/有 `Copyable` 上界的类型参数，不建立第二套按名称判断。
- Phase 2 为普通名义类型的主构造器字段访问记录稳定 field projection，保留 expression、
  receiver、field symbol 和替换后的字段类型；安全访问等尚未封闭选择继续 deferred。
- 对无显式同名 callable 的 value class，识别零参数自动 `componentN()`，按字段声明顺序返回
  替换后的分量类型，并以稳定 callable target 记录对应 field symbol；显式成员仍优先。
- 字段在 `Value` 实参、initializer 或 return 等 owned value 位置被交付时，若字段类型为
  MoveOnly，产生 L0132；Borrow / Inout 投影与 Copyable 字段读取合法。自动 `componentN()`
  返回 MoveOnly 分量时同样产生 L0132。
- L0132 primary 指向字段名或 `componentN` 名称，label 指向字段声明；失败不移动 receiver，
  不创建“已移走字段”的部分状态，也不追加 L0131 级联。
- 新增 Rust integration tests 与精确一个 pass / fail fixture，锁定诊断码、Span、typed facts、
  source identity、遮蔽/泛型能力和 initializer 单次遍历。

## 3. 非目标

- 不实现 SPEC-0029 的借用活跃区、Borrow / Inout 冲突、ASAP 析构点或委托调用所有权。
- 不实现顺序容器 element place、Map、closure capture、`Transferable` 或 codegen。
- 不允许部分结构解构、`_` 占位或顶层解构；既有 Parser / L0118 边界保持不变。
- 不实现用户自定义 `componentN()` 的特殊所有权；显式方法只遵守其普通 callable contract。
- 不实现 safe-call lifting、一般属性协议、getter / setter 或字段赋值可变性扩展。
- 不新增依赖、日志、全局可变状态、LLVM 表示或用户可见新语法。

## 4. 验收标准

- [x] Copyable 完整解构后源值仍可用；MoveOnly 完整解构后再次使用源值产生 L0131。
- [x] 消费式解构 temporary 不伪造变量状态，全部 binding 可按各自类型继续交付。
- [x] 条件 value class、nullable、enum、Box、`T` 与 `T : Copyable` 使用同一能力事实。
- [x] Copyable 字段和自动 `componentN()` 可产生 owned copy；MoveOnly 分量产生精确 L0132。
- [x] Borrow / Inout 字段投影合法，失败的部分移动不移动 receiver 或建立部分状态。
- [x] field / component typed facts 使用稳定 expression、receiver、field symbol 与替换后类型。
- [x] pass / fail fixture、窄测试与窄 Clippy 通过。
- [x] workspace 标准基线通过，无 ignored / skipped。
- [x] Architecture、roadmap、Spec 索引与独立提交同步。

## 5. 技术方案与边界

在 `type_checking` 增加单一职责的 aggregate projection 描述符；字段选择仍由类型阶段完成，
所有权阶段只消费 identity 与 Copyability。自动 `componentN()` 在普通候选为空后识别，因此
不会覆盖用户显式方法。所有权 checker 对解构 descriptor 选择 Read / Consume，对投影只在
owned value 边界检查，不把 field 加入变量状态表。

## 6. 实施计划

1. [x] 审计 v0.25、SPEC-0022/0027 与 typed facts → 验证：0028 无 guide / ADR 门禁。
2. [x] 建立 field / automatic component typed projection → 验证：泛型替换和显式成员优先窄测。
3. [x] 消费 Copy/Consume 解构并拒绝 MoveOnly 分量移出 → 验证：L0131/L0132 ownership matrix。
4. [x] 补 fixture、同步 Architecture / roadmap → 验证：窄测试与 Clippy。
5. [x] 运行 workspace 基线、审阅 staged diff并独立提交 → 验证：提交信息包含 `SPEC-0028`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | typed projection、结构所有权检查、测试、Architecture 与完成记录 | `feat(frontend): check structural moves (SPEC-0028)` |

## 8. 未决问题

- 无；借用存续与析构时机明确留给 SPEC-0029，不阻塞本 Spec。

## 9. 验证记录

- `cargo test -p lang-frontend --test ownership_structural --test ownership_checking --test type_copyability --test type_callable --test diagnostic_model --locked --offline`：33 passed，
  0 failed，0 ignored，0 filtered。
- `cargo test -p lang-frontend --test ownership_structural --locked --offline`：4 passed，
  0 failed，0 ignored，0 filtered；加入 ordinary class 与 Inout 回归后再次通过。
- `cargo clippy -p lang-frontend --all-targets --locked --offline -- -D warnings`：通过，0 warnings。
- `git diff --check` 与 `cargo fmt --all -- --check`：通过。
- `cargo check --workspace --all-targets --locked --offline`：通过。
- `cargo clippy --workspace --all-targets --locked --offline -- -D warnings`：通过，0 warnings。
- `cargo test --workspace --all-targets --locked --offline`：全部 test target 通过，0 failed、
  0 ignored、0 filtered。
- `cargo build -p lang-cli --locked --offline`：通过。
- Phase 2 roadmap 审计：SPEC-0022 对应三个误留未勾项已按实现与测试证据修正；真实剩余项及
  SPEC-0024–0026 门禁已在 Spec 索引中明确，不构成 SPEC-0027–0029 的依赖。
