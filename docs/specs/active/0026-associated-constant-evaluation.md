# SPEC-0026：单文件关联常量选择与编译期求值

> **性质**：实施 Spec · **状态**：in-progress · **读取时机**：实施或评审 v0.36 对应 Goal 时 · **唯一真源**：本 Spec

## 1. 元数据

| 字段 | 值 |
|---|---|
| 状态 | `in-progress` |
| Goal ID | `KOV-P2-026` |
| 所属 Phase | Phase 2 |
| 语言规范 | 现行 [v0.36 §36](../../guide/05-declarations-callables.md#36-无运行时存储的关联常量与封闭求值) |
| 批准依据 | 2026-09-12 用户明确启用 v0.36 并要求分阶段实施；依赖未完成者保持 draft |
| 前置 Spec | SPEC-0017、0018、0019、0020 `done` |
| 前置 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` 单文件名称/类型/常量求值 facts 与测试；Architecture |
| 语言语义变更 | 否；实施启用后的 v0.36 单文件常量契约 |

## 2. Goal

完成后，单文件 frontend 能选择顶层、具名 object 与 companion 常量，验证封闭 const 类型/
表达式，发布确定的 typed `ConstValue`、依赖和 use descriptor；不执行用户函数或创建运行时
singleton/global storage。

## 3. 范围与需求

- 支持同文件 bare const reference、`Object.CONST` 与 `Type.CONST`；object 的 type/value 双身份
  在该形态下精确选择关联常量，普通 object instance method 仍不因此获得调用能力。
- 允许 `Boolean`、八种定宽整数、`Char`、`String`；值规范化为 compiler-owned bool、精确
  width/signedness integer、Unicode scalar 或 UTF-8 bytes。其他类型使用 L0155。
- initializer 只接受 literal/group、const reference、`+`/`-`/`!`、整数算术/比较/相等、Boolean
  short-circuit、String concat/equality；首个不允许子表达式使用 L0156。
- 所有常量先收集再建立稳定依赖图；前向引用合法，每个强连通 cycle 产生一个 L0157，invalid
  dependency 不追加 cycle/evaluation 级联。
- dependency graph 按语法引用建立，`&&`/`||` 的 RHS 即使会被短路也形成 edge 并参与 SCC；
  两侧都做类型与 const-expression 资格检查。值求值保留 short-circuit，未求值 RHS 不产生
  L0158；因此 `false && (1 / 0 == 0)` 不报求值失败，但 RHS 非常量表达式仍报 L0156，RHS
  引用形成的 cycle 仍报 L0157。
- checked overflow、除零、remainder zero 与 signed MIN/-1 在编译期使用 L0158，不生成运行时
  Abort；普通类型/operand/literal 错误继续复用 L0084/L0085/L0090。
- companion 中 `this`、实例 field/member 或 enclosing type parameter 使用 L0153；private
  associated const 越界使用 L0154；interface const 不被实现类型继承或 override。
- facts 包含 declaration symbol/type/value/dependency、每个 constant use 的 target/value/category
  与 validated marker；失败 trial/recovery 不发布半成品。

## 4. 非目标

- 不实现跨文件 const graph、exact import、associated function 调用、object instance method、
  Float/Double、一般 CTFE、用户函数执行、SSA/LLVM 或运行时 global/init。

## 5. 验收标准

- [ ] 顶层/object/class/value/enum/interface companion 常量的 bare/qualified 正例通过。
- [ ] 显式/推导 Boolean、八整数、Char、plain String 与前向 const chain 产生精确稳定值。
- [ ] invalid companion context、visibility、type、expression、cycle、overflow/div-zero 分别产生
  L0153–L0158，主 Span/labels 精确且不级联。
- [ ] short-circuit RHS 的资格/edge/SCC 始终保留，未求值的 overflow/div-zero 不产生 L0158。
- [ ] object type/value 双身份、companion/instance scope 隔离和 interface 不继承规则稳定。
- [ ] descriptor/diagnostic 不依赖声明表或 HashMap 遍历顺序，trial rollback 与既有 suite 回归。
- [ ] Architecture 与实现事实同步。

## 6. 技术方案与边界

在现有单文件 symbol/type 环境之上增加 associated-const selector 与纯 typed evaluator；使用
显式 DFS/SCC 状态和稳定 declaration key，不把求值结果写回 AST，也不复制 runtime operator
lowering。跨文件后继必须复用同一 evaluator。

## 7. 实施计划

1. [ ] 建立常量 identity/type/use descriptor → 验证：顶层与关联选择矩阵。
2. [ ] 建立封闭 evaluator、依赖图和 L0153–L0158 → 验证：值/错误/确定性矩阵。
3. [ ] 同步验收与 Architecture → 验证：按[分层验收](../../development/testing.md)选择目标测试与必要下游检查，并记录命中数。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | typed facts、evaluator、测试与完成文档 | `feat(frontend): evaluate associated constants (SPEC-0026)` |

## 9. 未决问题

- 无语义未决项；跨文件 typed facts 由 SPEC-0210 承接。

## 10. 验证记录

实施前按[分层验收](../../development/testing.md)将第 5 节各项映射到实际测试目标/过滤器；
记录命中数、结果与未运行原因。同一状态下的有效证据只运行一次，不默认运行 frontend 全量。

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-27 roadmap 审计 | 通过 | AST/scope/type 壳已存在；当前 Constant 与 Variable 共用普通 initializer 检查且没有 evaluator/associated selector |

### 实施切片与验收映射

| 契约 | 目标 / 过滤器 | 状态 |
|---|---|---|
| const 封闭类型集合、L0155 Span 与普通变量隔离 | `cargo test -p lang-frontend --test type_constants -- --nocapture` | 4 项通过；覆盖显式/推导、22 个允许 literal 场景、10 种非法类型与普通变量对照、Any/Any?、object/companion、既有 L0084 抑制级联 |
| 诊断注册表与现有 renderer/model | `cargo test -p lang-frontend --test type_constants --test diagnostic_model --no-fail-fast -- --nocapture` | diagnostic_model 9 项通过；同次旧 type_constants 2 项通过、1 项因误写 L0085 失败，修正为既有 L0084 后定向重跑见上一行 |

先落实 const 类型资格；关联选择、L0153/L0154/L0156–L0158、依赖图、值、use descriptor 与
validated capability 尚待后续切片，不能据此将本 Spec 标记完成。

类型资格首轮先复现显式/推导 Double 未产生 L0155，再补入门禁。独立只读复核发现
Any/Any? 经 Deferred(AnyValueRepresentation) 绕过检查，修复并补测试后复核通过。
仅针对已知 const 类型建立资格诊断；Error 和其他 Deferred 不追加诊断，后续求值与 validated
capability 切片仍须处理前向依赖，不能把此处的 recovery 当作 const 验证成功。
诊断目录测试同步既有 L0152 与新增 L0155；未运行 frontend 全量。

静态与文档门禁：`cargo clippy -p lang-frontend --lib --test type_constants --test diagnostic_model -- -D warnings`、
`cargo fmt --all -- --check`、`python3 scripts/check_docs.py`（351 份 Markdown）与 `git diff --check` 均通过。
未修改公共阶段产物；未重复无关的全量类型/所有权/下游测试。
