# SPEC-0085: 建立独立 Parser 入口前缀截断矩阵

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-085` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guide/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强词法、语法与 Parser 测试验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0006、SPEC-0007、SPEC-0008、SPEC-0009、SPEC-0069、SPEC-0079 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` expression / declaration / block 独立 Parser EOF 恢复测试、必要直接修复、Architecture |
| 语言语义变更 | 否；只锁定合法独立语法在 UTF-8 scalar 边界截断后的入口总性与公开产物不变量 |

## 1. Goal

完成后，三个独立 Parser 入口不再只依赖固定 malformed 拼接样本：代表性合法 expression、
declaration 与 block 源码在空前缀、每个 UTF-8 scalar 结束位置及完整源码处，均由生产 Lexer
完整覆盖，并由对应入口返回确定、有界且根 ID 可解析的产物，不因普通 EOF 截断产生内部错误。

## 2. 范围与需求

- 建立 12 个互异且 Lexer / 对应 Parser 均零诊断的完整源码：expression、declaration、block
  各 4 个，覆盖 callable/postfix、control-flow、运算符、lambda/string interpolation、泛型函数、
  class-family、局部解构、loop-family 与 Unicode nested lexical owner。
- 对每个样本枚举空前缀、每个 UTF-8 scalar 结束位置及完整源码；矩阵必须固定样本数、各入口
  样本数和实际前缀总数，不能因空集合、重复样本或非法完整 baseline 静默缩减。
- 每个前缀重新调用生产 Lexer，验证 lexeme 连续覆盖、source identity、非 EOF lexeme 非空、
  唯一末尾 EOF，以及 Lexer / Parser 诊断与四张 AST table Span source-local 且有界。
- 对应入口的 root 必须能在 expression / item / statement table 中解析；同一 `LexedFile` 重复
  执行两次入口，公开 `Debug` 产物必须一致且均无 `ParserInternalError`。
- 完整源码必须保持 Lexer / Parser 零诊断；截断前缀不固定诊断 code 或 AST 形态，只锁定总性、
  source-local 范围、根有效和确定性。
- 不增加依赖、公开 API、新诊断或合法语法；矩阵发现缺陷时只修复直接根因并添加定向断言。

## 3. 非目标

- 不重复 SPEC-0079 的 22-file `parse_file` corpus，也不改变完整文件 Parser 的既有 1,373 前缀计数。
- 不把资源预算耗尽视为语法诊断；本 corpus 规模必须处于既有固定递归预算以内。
- 不枚举 UTF-8 code unit 内部非法切点；这属于 `SourceMap` / Lexer 非字符边界输入契约。
- 不固定每个截断位置的精确恢复 AST 或诊断集合；领域测试和 diagnostic witness 继续负责语义。

## 4. 验收标准

- [x] 12 个完整样本按 4 / 4 / 4 覆盖三个独立入口，互异且 baseline 零诊断。
- [x] 每个样本的空、逐 scalar 与完整前缀被精确执行，共 747 个前缀。
- [x] 全部前缀保持 Lexer 覆盖、唯一 EOF、诊断 / AST Span source-local 且有界。
- [x] 全部入口 root 可解析，重复解析公开产物一致且无内部错误。
- [x] 矩阵未发现生产缺陷，无需生产修复。
- [x] 既有完整文件前缀矩阵、入口对抗矩阵和三个领域入口测试保持通过。
- [x] 新矩阵及直接相关窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

把 SPEC-0079 的 UTF-8 scalar 前缀结束位置算法提取为 tests 私有共享模块，避免两个矩阵漂移。
把现有 frontend 矩阵中的 lexeme、diagnostic 与 AST Span 检查拆到职责单一的共享 assertion 模块，
由完整文件与独立入口矩阵复用；不创建生产 API。

新矩阵按静态 `EntryKind` 分派到三个公开入口。每次解析后先验证 `source_id`、全部 table Span、
诊断与对应 root table，再比较两次完整 `Debug` 指纹；内部错误直接带 entry / case / byte offset
上下文失败，便于定位最早不满足总性的前缀。

## 6. 实施计划

1. [x] 审计 SPEC-0069 / 0079 与三个领域测试 → 验证：确认独立入口缺少逐 scalar EOF 矩阵。
2. [x] 提取共享前缀 / output assertion 并建立 4 / 4 / 4 合法 corpus → 验证：baseline 全部零诊断。
3. [x] 逐前缀验证三个入口总性、root、Span 和确定性，最小修复实际缺陷 → 验证：195 / 350 / 202，共 747 个前缀，未发现生产缺陷。
4. [x] 运行直接相关窄测试与窄 Clippy → 验证：143/143，0 warnings。
5. [x] 运行一次 workspace 标准基线并同步完成记录 → 验证：标准命令全部成功，419 tests。
6. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0085`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | 独立入口前缀矩阵、必要修复、Architecture 与完成记录 | `test(frontend): verify parser entry prefix recovery (SPEC-0085)` |

## 8. 未决问题

- 无。

## 9. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo test -p lang-frontend --lib --test lexer --test parser_entry_adversarial --test parser_prefix_truncation_matrix --test parser_entry_prefix_truncation_matrix --test parser_expression --test parser_declaration --test parser_block --locked --offline` | 通过 | 143/143；747 个独立入口前缀，未发现生产缺陷 |
| `cargo clippy -p lang-frontend --all-targets --locked --offline -- -D warnings` | 通过 | 0 warnings |
| workspace Cargo 基线 | 通过 | 每条标准命令执行一次；`fmt`、`check`、Clippy、419 tests、CLI build；0 failed / ignored / measured / filtered |
