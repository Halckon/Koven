# SPEC-0185：声明型 type roots 的源码模块接纳边界

## 1. 元数据

| 字段 | 值 |
|---|---|
| 状态 | `done` |
| Goal ID | `KOV-P4-185` |
| 所属 Phase | Phase 4 |
| 语言规范 | 现行 [v0.28 class-family 契约](../guide/04-grammar-declarations-blocks.md#13-class-family-声明v020) 与 [Phase 4 roadmap](../guide/06-roadmap.md#phase-4llvm-代码生成) |
| 批准依据 | 当前持续 Goal“继续推进 guide 主线，分阶段实施 specs”的站立授权 |
| 前置 Spec | SPEC-0020、0034 `done`；SPEC-0042 已证明单文件标准源码 bootstrap |
| 前置 ADR | [ADR-0007](../adr/0007-llvm-toolchain-and-first-target.md)、[ADR-0010](../adr/0010-first-native-object-and-linker-contract.md) `accepted` |
| 阻塞项 | 无；本 Spec 不选择 constructor、object storage 或 nominal ABI，只修正无运行时初始化的声明 root 对既有函数实例图的阻断 |
| 影响范围 | `lang-codegen` frontend→SSA module orchestration 与 native object 回归；Architecture、roadmap |
| 语言语义变更 | 否；frontend 已接受并检查这些声明，本 Spec 不新增语法、类型、所有权或运行时行为 |

## 2. Goal

完成后，frontend-clean 的单文件可以在可达标量函数旁包含普通 `class`、`value class`、
`interface` 与 `enum class` 顶层声明；这些无模块初始化动作的 type roots 不进入函数实例图、
不生成伪 SSA/LLVM 实体，也不再仅因存在而阻断既有 object/link 流水线。

## 3. 范围与需求

### 3.1 声明 root 分类

- `collect_functions` 在解开既有 `Modified` wrapper 后，继续收集顶层 `Item::Function`，并跳过
  `ClassifierKind::Class`、`ValueClass`、`Interface` 与 `EnumClass` root。
- 跳过只表示该声明本身没有模块初始化动作；不得为 classifier、constructor、variant、字段、
  member 或 companion 生成 placeholder SSA/LLVM，不得把源码名称映射为已有 IR-local 类型。
- 顶层 `object` 具有唯一值 identity，不属于纯 type root；顶层 variable/constant、standalone
  companion、恢复节点和其他 root 继续以现有 `UnsupportedNode` fail-loud。

### 3.2 可达操作门禁

- 纯 type root 不进入 `FunctionTemplate` / instance planning；既有 scalar function 的实例顺序、
  `FunctionId`、SSA rendering、LLVM symbol 与 entry 选择保持不变。
- 函数签名或 body 一旦实际要求尚未接线的 nominal type、constructor、variant、projection、
  destructuring、member receiver 或 drop，继续由现有 frontend deferred/diagnostic 或
  source→SSA `UnsupportedNode` 拒绝；不得因声明 root 可共存而伪装为支持这些操作。
- frontend diagnostics、analysis identity 与显式 entry 预检仍先于 object 写盘；本 Spec 不改变
  `emit_native_object` 的失败分类或输出原子性。

### 3.3 标准库前置能力

- 回归测试使用代表性的声明型 roots 与显式 `() -> Unit` entry，证明同一 source 可生成 verified
  SSA、LLVM 与 Mach-O object；不在本 Spec 中修改 `prelude.ko` 或提前声明 `Pair`/`Result`。
- 该能力只解除“声明存在即失败”的机械门禁；SPEC-0183/0188/0184 仍负责 nominal/enum/Box
  constructor typed fact 与 aggregate lowering，完成后才可执行依赖构造值的标准库验收。

## 4. 非目标

- 不实现 nominal/enum/Box constructor、字段或 variant projection、解构、member function、
  receiver ownership、drop glue 接线或运行时类型元数据。
- 不接纳具名 `object`、顶层存储初始化、`companion object` 运行时值或编译期常量求值。
- 不修改 `Pair`、`Result`、`Rc`、容器、IO、thread/channel 的标准库 API，也不提前完成
  SPEC-0044/0045。
- 不改变 reachability：既有不可达顶层函数仍按实例规划规则处理，本 Spec 不新增 dead-code
  elimination 或 whole-module validation 策略。

## 5. 验收标准

- [x] 带修饰符或无修饰符的普通 class/value class/interface/enum class roots 可与标量 entry 共存，
      重复 lowering 产生相同 verified SSA/LLVM，且不出现 classifier 名称的伪 function/type。
- [x] native object API 可从同一 source 的显式 Unit entry 生成 Mach-O object；入口形状和输入
      analysis identity 规则保持不变。
- [x] `object`、顶层 variable/constant 仍以 `UnsupportedNode` 拒绝，native 失败不写 object。
- [x] 实际使用尚未接线的 nominal constructor/type 仍被 frontend deferred/diagnostic 或 lowering
      门禁拒绝，不因 type root 跳过而形成错误成功。
- [x] 受影响 codegen 窄测及 workspace 五项基线通过；Architecture、roadmap 与 Spec 只记录实际
      完成事实。

## 6. 技术方案与边界

- 在现有 `collect_functions` 内做一处分类型 match；不新增 pass、IR node、全局 registry 或第三方
  依赖。函数分支沿用现有 symbol/callable identity 查找，声明型分支只 `continue`。
- 分类直接读取 parser 的 `ClassifierKind`，不按名称、字段形状、是否被引用或源码文本猜测。
  `Modified` 继续由同一 helper 解开，保证 visibility 不改变 root 的运行时类别。
- 测试放在现有 frontend→SSA 与 native object suite，分别锁定纯声明共存、保守拒绝和不落盘。

## 7. 实施计划

1. [x] 收敛函数收集器的声明 root 分类 → 验证：四类 classifier pass，object/storage fail。
2. [x] 锁定 SSA/LLVM/object 与可达 nominal operation 门禁 → 验证：确定性文本、真实 object、失败
   不落盘。
3. [x] 同步 Architecture/roadmap/Spec，运行 workspace 基线并审查 staged diff。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | Spec 与实施门禁 | `docs(spec): define declarative type root lowering (SPEC-0185)` |
| 2 | codegen、测试、Architecture 与完成记录 | `feat(codegen): accept declarative type roots (SPEC-0185)` |

## 9. 未决问题

- 无。type root 对应的运行时构造与布局继续由现有 0183/0188/0184 边界决定。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-25 前置审计 | 通过 | v0.28 已定义四类 type declaration；0020/0034 已完成 frontend/type 与标量 lowering；当前阻断来自 `collect_functions` 对所有非函数 root 的机械拒绝 |
| `cargo test -p lang-codegen declarative_type_roots --lib` | 通过 | 两项定向测试覆盖四类 type root、确定性 SSA/LLVM、真实 object、object/storage/constructor 保守拒绝 |
| `cargo clippy -p lang-codegen --all-targets -- -D warnings` | 通过 | codegen 零 warning |
| `cargo test -p lang-codegen --all-targets` | 通过 | 97 项测试通过，无失败或 ignored |
| `cargo fmt --all -- --check` | 通过 | workspace 格式基线 |
| `cargo check --workspace --all-targets` | 通过 | workspace 所有 target 检查通过 |
| `cargo clippy --workspace --all-targets -- -D warnings` | 通过 | workspace 零 warning |
| `cargo test --workspace --all-targets` | 通过 | 全部测试通过，无失败或 ignored |
| `cargo build -p lang-cli` | 通过 | `kovenc` 构建成功 |
