# SPEC-0023: 检查顺序容器类型、构造与 element place

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P2-023` |
| 所属 Phase | Phase 2 |
| 语言规范 | [现行 v0.25 顺序容器契约](../guide/01-design-decisions.md#8-顺序容器内存表示与索引语义array--list--mutablelist) |
| 批准依据 | 用户在当前持续 Goal 中要求继续分阶段实施 Specs，并授权简化重复验收环节 |
| 前置 Spec | SPEC-0020、SPEC-0022、SPEC-0067 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` intrinsic/type checker、顺序容器 typed model、诊断、Phase 2 fixture、Architecture |
| 语言语义变更 | 否；实现现行 guide 已封闭的顺序容器 Phase 2 契约 |

## 1. Goal

完成后，Phase 2 能按预声明身份识别 `Array<T>`、`List<T>`、`MutableList<T>` 及其核心构造，
拒绝不可结构存储的元素类型，并把合法下标表达式记录为带容器可变性的 element place。

## 2. 范围与需求

- 三种容器是单类型实参、名义互异且始终 move-only 的 intrinsic owner；只接受 structurally
  storable 元素类型，不把 `Any`、裸 interface、`Nothing`/`Nothing?` 擦除、装箱或动态化。
- `arrayOf`、`listOf`、`mutableListOf` 按预声明 callable 身份处理；expected container 或显式
  use-site 类型实参确定 `T`，否则由首元素静态类型确定，空调用和 bottom/null 首元素不能推导。
- `Array<T>(size, initializer)`、`List<T>(size, initializer)` 与 `MutableList<T>()` 按预声明 type
  identity 处理，并保存固定 `Value`/`Borrow` 参数契约。
- 顺序容器索引要求 `Int` key，结果类型为 `T`，typed 产物保存 receiver、index、容器种类、
  元素类型与可变性；`List` 元素不能作为 `&` 或赋值目标。
- `size` 类型为只读 `Int`；普通 `.get`/`.set` 不获得内建语义。

## 3. 非目标

- 不检查元素读取的 copy/move、借用活跃区、替换求值顺序或 use-after-move；这些属于 Phase 3。
- 不实现缓冲区、分配、边界检查、析构或 ABI；这些属于 Phase 4。
- 不实现 `MutableList.add` 等 Phase 5 API，不实现 Map、自定义索引、切片或隐式容器转换。
- 不实现一般泛型 callable 推导；本 Spec 只处理封闭的核心构造 identity。

## 4. 验收标准

- [x] 三种容器精确接受一个 storable 类型实参并保持互异、move-only 的 `TypeKind`。
- [x] 列表式构造覆盖 expected、显式类型实参、首元素推断、空调用和异型元素正反例。
- [x] 运行时长度构造与空 `MutableList` 检查 arity、`Int` size 和 `(Int) -> T` initializer。
- [x] 索引 typed descriptor、`Int` key、`List` 不可变性、元素赋值和 `size` 只读规则有正反例。
- [x] `.get`/`.set` 不绕过内建 `[]`，storable 反例具有稳定 code/Span。
- [x] 受影响窄测和一次 workspace 标准基线通过；Architecture、guide 路线图和本 Spec 同步。

## 5. 实施与提交计划

1. [x] 扩展 intrinsic 环境、容器 typed model 与诊断目录。
2. [x] 实现类型实参/storable、构造、索引、只读与赋值检查。
3. [x] 补 Rust 专测与 Phase 2 pass/fail fixture。
4. [x] 同步事实文档并执行简化验收。
5. [x] 独立提交：`feat(frontend): check sequential containers (SPEC-0023)`。

## 6. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo test -p lang-frontend --test type_containers --test type_checking --test type_copyability --test type_callable --locked --offline` | 通过 | 49 passed；容器专测、Phase 2 fixture、Box/Copyable 与 callable 回归 |
| `cargo clippy -p lang-frontend --all-targets --locked --offline -- -D warnings` | 通过 | frontend 全 target 无 warning |
| Phase 2 fixture | 通过 | `type-pass` / `type-fail` 各 6 个；容器 fail 精确核对 L0084、L0126、L0128–L0130 byte Span |
| workspace Cargo 基线 | 通过 | fmt、check、Clippy `-D warnings`、379 tests、`cargo build -p lang-cli` 全部成功；首轮测试发现并修正旧诊断目录上界后，最终基线重跑通过 |
