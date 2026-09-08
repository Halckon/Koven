# SPEC-0078: 建立 Parser 结构性换行边界矩阵

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-078` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guides/v0.34-pre-restructure/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强词法、语法与 Parser 测试验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0014、SPEC-0016、SPEC-0017、SPEC-0062、SPEC-0077 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` Lexer / 完整文件 Parser 换行边界测试、必要边界修复、Architecture |
| 语言语义变更 | 否；只锁定现行 LF / CRLF 与 comment 内换行的结构边界契约 |

## 1. Goal

完成后，实际 LF、CRLF、line comment 的终止换行及 terminated block comment 内换行，在文件头、
顶层声明、class member、`when` entry 和裸 `return` 等已定义位置产生一致结构边界；space、tab
与不含 LF 的 block comment 不冒充边界，同时 enum comma 和表达式连续性不被错误放宽。源码
裸 CR 仍按 lexical guide 产生 `L0001`，不属于 Lexer-clean Parser 边界输入。

## 2. 范围与需求

- 固定 6 个含实际 LF 的结构性载体：LF、CRLF、LF / CRLF line comment、含 LF / CRLF 的
  terminated block comment。
- 固定 4 个非结构载体：space、tab、不含换行 block comment、内容只含裸 CR 但不含 LF 的
  block comment；comment 内容不改变其作为单个合法 trivia 的身份。
- 把两组载体投放到 package→import、import→declaration、两个顶层声明、两个 class member、
  两个 `when` entry 与 `return`→表达式六类位置；前五类锁定是否出现既有 separator 诊断，
  `return` 锁定 `Expression::Return.value` 的 `None` / `Some` 与后续 statement 结构。
- 反向证明 enum variant 仍必须用 comma，表达式在中缀运算符前不因换行而截断；两类输入在全部
  10 个载体下保持相同诊断 / AST 结构。
- 每个源码使用生产 Lexer 并要求零词法诊断，完整文件 Parser 重复执行两次并比较公开 `Debug`；
  所有诊断 Span 必须 source-local 且位于源码边界内。
- 不增加依赖、公开 API、新诊断或合法语法。

## 3. 非目标

- 不改变哪些语法位置把换行视为边界，也不把 Koven 改造成普遍 newline-sensitive 语法。
- 不把 Unicode line separator、裸 CR 或 comment 结束本身定义为结构换行。
- 不替代各领域对精确 Span、AST child 与恢复形态的定向测试。
- 不测试 unterminated comment / string；其 lexical-owner 恢复由既有矩阵负责。

## 4. 验收标准

- [x] 6 个结构载体和 4 个非结构载体在六类边界位置形成预期的两组等价结果。
- [x] enum comma 与中缀表达式两个反向 case 在全部 10 个载体下保持既定语义。
- [x] 全部源码 Lexer-clean，每例重复解析无内部错误且公开产物确定一致。
- [x] 矩阵发现的生产缺陷有最小修复和定向断言；本矩阵未发现生产缺陷，未改生产代码。
- [x] 新矩阵及直接相关 Lexer / Parser 窄测试与窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

新增独立 integration test，以 `prefix + carrier + suffix` 构造固定源码。公开产物指纹包含诊断
code / message、文件头与 root 数量、四张 AST table 的 payload discriminant，以及 class member、
`when` entry 和 `return` 是否带值等与本矩阵相关的结构字段；Span 只验证 identity / 范围，
不跨不同长度 carrier 比较绝对偏移。

结构载体必须实际包含 `\n`，非结构载体必须不含 `\n`，测试先自检此前提。若出现差异，先确认
Lexer 把 carrier 完整保留为 trivia，再定位共享 gap 判定或具体 grammar owner，不复制换行扫描。

## 6. 实施计划

1. [x] 审计共享换行判定与现有测试 → 验证：确认实现统一但领域覆盖零散，缺少 carrier × 位置矩阵。
2. [x] 建立结构 / 非结构 carrier 与边界位置矩阵 → 验证：80 个源码、160 次解析，诊断、AST 结构与 Span 均符合预期。
3. [x] 运行窄测试并最小修复实际缺陷 → 验证：120/120，窄 Clippy 0 warnings；仅纠正草案对裸 CR 的错误假设，未发现生产缺陷。
4. [x] 运行一次 workspace 标准基线并同步完成记录 → 验证：标准命令全部成功。
5. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0078`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | 结构性换行边界矩阵、必要修复、Architecture 与完成记录 | `test(frontend): verify parser line boundaries (SPEC-0078)` |

## 8. 未决问题

- 无。

## 9. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo test -p lang-frontend --test parser_line_break_boundary_matrix --test lexer --test parser_file --test parser_class_family --test parser_control_flow --test parser_expression --locked --offline` | 通过 | 120/120；80 个 Lexer-clean 源码、160 次完整文件解析 |
| `cargo clippy -p lang-frontend --test parser_line_break_boundary_matrix --locked --offline -- -D warnings` | 通过 | 0 warnings |
| workspace Cargo 基线 | 通过 | 每条标准命令执行一次；`fmt`、`check`、Clippy、409 tests、CLI build；0 failed / ignored / measured / filtered |
