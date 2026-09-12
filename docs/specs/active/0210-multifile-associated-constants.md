# SPEC-0210：跨文件关联常量集成

> **性质**：实施 Spec · **状态**：in-progress · **读取时机**：实施或评审 v0.36 对应 Goal 时 · **唯一真源**：本 Spec

## 1. 元数据

| 字段 | 值 |
|---|---|
| 状态 | `in-progress` |
| Goal ID | `KOV-P2-210` |
| 所属 Phase | Phase 2 |
| 语言规范 | 现行 [v0.36 §36](../../guide/05-declarations-callables.md#36-无运行时存储的关联常量与封闭求值) |
| 批准依据 | 2026-09-12 用户明确启用 v0.36 并要求分阶段实施；依赖未完成者保持 draft |
| 前置 Spec | SPEC-0025/0026/0197 `done` |
| 前置 ADR | ADR-0020 `accepted` |
| 阻塞项 | 无；SPEC-0208/0209 已完成，按既定顺序进入跨文件 typed 集成 |
| 影响范围 | `lang-frontend` compilation-unit const selection/evaluation/tests；Architecture |
| 语言语义变更 | 否；实施启用后的 v0.36 unit integration |

## 2. Goal

完成后，compilation-unit type checker 在统一 DeclarationId/visibility/package facts上复用
SPEC-0026 evaluator，支持跨文件 `import p.Type` 后 `Type.CONST`、绝对 `p.Type.CONST` 与
跨文件 const dependency/cycle，不建立第二套常量语义。

## 3. 范围与需求

- exact import 终端按[现行名称规则](../../guide/02-names-files-packages.md)只接受顶层声明/函数组；`import p.Type.CONST` 使用 L0148，
  `import p.Type` 后的 `Type.CONST` 才由 associated selector 处理。
- 可见性和 package-qualified target 只消费 SPEC-0025/0197 validated facts；import target 不可见
  继续使用 L0149，成功选择 Type 后 associated const 越界使用 L0154，不按逻辑路径或源码
  文本猜测。
- 跨 source unit 的 dependency graph 使用稳定 DeclarationId；前向 chain 与 cycle 不依赖输入顺序，
  每个 SCC 仍只产生一个 L0157。
- 输出与 0026 使用相同 ConstValue/use descriptor shape，并在 SPEC-0197 的基础 validated unit
  上发布 `ConstEnabledTypedUnit` capability marker（或等价类型状态）。它不回写或重新定义
  0197，也不被既有 0198/0199 自动接受。

## 4. 非目标

- 不实现依赖 package、跨 compilation-unit ABI、associated function、runtime global、unit const
  ownership 或 native multi-file lowering。后两者必须由新后继 Spec 显式消费 0210、0208/0209
  与 0198/0199，不能修改已完成节点的验收含义。

## 5. 验收标准

- [ ] `import p.Type`; `Type.CONST`、`p.Type.CONST` 与跨文件 acyclic chain 正确。
- [ ] `import p.Type.CONST` 精确产生 L0148；invisible imported Type 使用 L0149，private associated
  const 越界使用 L0154。
- [ ] 跨文件 self/two-node/multi-node SCC 每个 cycle 一个 L0157，labels 使用稳定 unit/declaration key。
- [ ] 正逆 source-unit 输入顺序产生相同 facts/diagnostics，单文件 wrapper 行为不变。
- [ ] invalid unit/import/type facts 不追加 const 级联或半成品 descriptor。
- [ ] 基础 validated unit 与 const-enabled capability 不可混用；0198/0199 对后者保持确定性拒绝，
  直到独立 unit const ownership/native 后继完成。
- [ ] Architecture 与实现事实同步。

## 6. 技术方案与边界

为 0026 evaluator 提供 compilation-unit symbol adapter；adapter只映射 DeclarationId、visibility
与 qualified target，不复制 evaluator、类型系统或 import resolver。

## 7. 实施计划

1. [ ] 接 unit declaration/associated target → 验证：visibility/import/qualified 矩阵。
2. [ ] 接跨文件 dependency/evaluation → 验证：chain/cycle/order矩阵。
3. [ ] 同步验收与 Architecture → 验证：按[分层验收](../../development/testing.md)选择目标测试与必要下游检查，并记录命中数。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | 抽取可携带 source identity 的共享 evaluator，保持单文件行为 | `refactor(frontend): share constant expression evaluator (SPEC-0210)` |
| 2 | unit const selection、dependency graph 与 capability | `feat(frontend): integrate multifile constants (SPEC-0210)` |
| 3 | 跨文件完整矩阵、下游拒绝与完成文档 | `test(frontend): complete unit constant acceptance (SPEC-0210)` |

## 9. 未决问题

- unit const ownership/native 应另立最小后继 Spec，显式消费 `0210 + 0208 + 0209 + 0198 + 0199`；
  本 Phase 2 Goal 不提前生成 owner facts 或 object，也不重开 0198/0199。

## 10. 验证记录

实施前按[分层验收](../../development/testing.md)将第 5 节各项映射到实际测试目标/过滤器；
记录命中数、结果与未运行原因。同一状态下的有效证据只运行一次，不默认运行 frontend 全量。

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-27 roadmap 审计 | 通过 | exact-import 冲突不阻塞单文件 0026；跨文件 const 必须等待 0025/0197 |


### 首切片：共享 evaluator 内核

`constant_evaluation.rs` 提取既有显式栈遍历、短路、Char/String 解码与纯值运算调用；适配器提供
source-qualified expression identity、AST child 映射、已选择 reference value、typed integer
literal 和诊断出口。单文件 Checker 改为该私有契约的适配器，依赖排序与 SCC 仍在原阶段。
负 literal 继续使用 prefix 表达式的类型和 operand 的 magnitude/span，保留最小有符号整数。

该切片是第 6 节“不复制 evaluator”的实施前置；没有公开 crate API 变化，也尚未接 unit
selector/dependency/capability。第 5 节不据此勾选跨文件完成。

| 检查 | 结果 | 证据边界 |
|---|---|---|
| 重构前 `cargo test -p lang-frontend --test type_constants --test ownership_constants` | 24 + 16 passed | 单文件语义和物化契约基线 |
| `cargo test -p lang-frontend --lib constant_evaluation_tests` | 2 passed，54 filtered | 两个 AST 复用局部 ExpressionId，正逆读取保持 source/selected value，短路阻止除零，执行失败 Span 仍归属正确 source |

新测试使用受控 source-qualified adapter 验证共享遍历，不代表真实跨文件名称解析或 unit typed
集成通过。后续必须消费已有 DeclarationId/visibility/package facts，并发布独立 capability。


重构后再次运行同一 `type_constants` / `ownership_constants` 选择：24 + 16 passed，0 failed/ignored。
独立审查逐分支比对旧 evaluator，未发现负 literal、短路、缺值或错误传播的语义变化；所有
Group/Prefix/Binary child 都通过 context 映射，没有把局部 ExpressionId 当全 unit 身份。


直接消费者：`cargo test -p lang-codegen --lib -- constant_lowering_tests native_tests::constant_tests`
通过 8 项，394 filtered，0 failed/ignored；覆盖既有精确值、String 物化、native matrix、owner 计数、
argv entry 与 object 重复性。未运行 frontend 全量、workspace check；本切片不新增公开阶段 API。

门禁：fmt、docs check（351 Markdown）与 diff check 通过。
`cargo clippy -p lang-frontend --all-targets -- -D warnings` **未通过**：未改动的
`tests/multifile_type_checking.rs:260` 触发 `obfuscated_if_else`，
`tests/multifile_ownership_checking.rs:1006` 触发 `filter_map_bool_then`；已核对两文件与 HEAD 无差异，
本切片未顺手修改。补跑 `cargo clippy -p lang-frontend --lib --test type_constants --test ownership_constants -- -D warnings`
通过；不将该定向结果表述为 all-targets 通过。SPEC-0210 仍在进行中。
