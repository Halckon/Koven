# SPEC-0092: 建立独立 Parser 入口换行边界矩阵

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-092` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guide/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强 Lexer / 语法 / Parser 测试且简化验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0006–0009、SPEC-0069、SPEC-0078、SPEC-0085–0091 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` 独立 Parser 入口换行边界测试、必要直接修复、Architecture |
| 语言语义变更 | 否；只锁定现行结构性换行 carrier 在三个独立入口中的既有边界契约 |

## 1. Goal

完成后，LF、CRLF、line comment 终止换行及 terminated block comment 内换行，在 expression
的 `when` entry、declaration 的 class member、block 的裸 `return` 边界产生一致结构；四种
无 LF trivia 不冒充边界，同时换行不替代 enum comma 或截断 expression / block 中缀表达式。

## 2. 范围与需求

- 复用 SPEC-0078 的 6 个结构载体与 4 个非结构载体，并自检前者包含 LF、后者不含 LF。
- 三类正向边界分别走 expression / declaration / block 公开入口：`when` entry 与 class member
  对非结构载体精确产生 L0065 / L0072，裸 `return` 精确区分无值 / 有值结构。
- 三类反向 case 分别证明 expression 中缀连续、declaration enum variant 仍需 comma、block
  中缀连续；全部 10 个载体必须保持同一诊断与无 Span AST 结构。
- 每个源码必须 Lexer-clean；Parser 重复执行并保持公开 `Debug` 确定，所有 Lexer / AST /
  diagnostic Span source-local 且有界，typed root 有效。
- 不增加依赖、公开 API、新诊断或合法语法；发现缺陷时只修复直接根因并添加定向断言。

## 3. 非目标

- 不把换行提升为通用 block statement separator，也不改变现有 newline-sensitive 位置集合。
- 不测试文件头或顶层声明边界；它们已由完整文件矩阵覆盖且不存在于三个独立入口。
- 不把 Unicode line separator、裸 CR 或 comment 结束本身定义为结构换行。
- 不测试 unterminated comment / string，不跨不同长度 carrier 比较绝对 Span。

## 4. 验收标准

- [x] 6 个结构载体和 4 个非结构载体在三个入口边界 case 中形成预期的两组等价结果。
- [x] 三个入口反向 case 在全部 10 个载体下保持既定诊断与 AST 结构。
- [x] 60 个源码 Lexer-clean，重复解析无内部错误、typed root 有效且公开产物确定。
- [x] 全部 Lexer / AST / diagnostic Span source-local 且有界。
- [x] 矩阵未发现生产缺陷，无需生产修复。
- [x] 直接相关窄测试与窄 Clippy 通过。
- [x] 最终一次成功的 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

以 `prefix + carrier + suffix` 构造固定源码；入口指纹包含 significant `LexemeKind`、诊断 code /
message、root index、四张 AST table 的 payload discriminant，以及与本矩阵相关的 classifier member、
`when` entry 和 `return` value 结构。不跨 carrier 比较 Span 数值，只验证 identity 与源码边界。

## 6. 实施计划

1. [x] 审计完整文件换行矩阵、现行 guide 与独立入口测试 → 验证：确定三类边界和三类反向 case。
2. [x] 建立 carrier × entry × boundary 矩阵 → 验证：60 个源码唯一执行、120 次解析。
3. [x] 修复直接缺陷并运行窄测试与窄 Clippy → 验证：45/45 tests、0 warnings，未发现生产缺陷。
4. [x] 同步事实并运行最终一次 workspace 标准基线 → 验证：五条标准命令全部成功，428 tests。
5. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0092`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | 独立入口换行边界矩阵、必要修复、Architecture 与完成记录 | `test(frontend): verify parser entry line boundaries (SPEC-0092)` |

## 8. 未决问题

- 无。

## 9. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo test -p lang-frontend --test parser_entry_line_break_boundary_matrix --test parser_line_break_boundary_matrix --test parser_control_flow --test parser_class_family --test parser_block --locked --offline` | 通过 | 45/45；60 个独立入口 carrier 源码，未发现生产缺陷 |
| `cargo clippy -p lang-frontend --test parser_entry_line_break_boundary_matrix --locked --offline -- -D warnings` | 通过 | 0 warnings |
| workspace Cargo 基线 | 通过 | `fmt`、`check`、Clippy、428 tests、CLI build；0 failed / ignored / measured / filtered |
