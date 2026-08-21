# SPEC-0077: 建立 Parser 非换行 trivia 等价矩阵

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-077` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guide/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强词法、语法与 Parser 测试验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0006、SPEC-0014、SPEC-0073、SPEC-0076 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` Lexer / 完整文件 Parser 集成测试、必要 trivia 处理修复、Architecture |
| 语言语义变更 | 否；只锁定既有非换行 trivia 不改变 token 序列或语法结构的不变量 |

## 1. Goal

完成后，代表完整文件头、声明、类型、表达式、call/lambda、control-flow 与 class-family 的有效
token 序列，在 token 间、文件首尾插入 space、tab 或不含换行的 terminated block comment
时，仍由生产 Lexer 生成相同 significant token 序列，并由完整文件 Parser 生成相同 AST
结构且无诊断。

## 2. 范围与需求

- 建立不少于 20 个 Lexer-clean、Parser-clean 的 grammar case，覆盖文件头、变量 / 常量 / 函数、
  函数类型、typed / named / mode call、lambda、局部解构、if / when / loop-family、postfix、
  class / interface / enum / object / companion、委托、泛型、cast / type test 与主要运算符组合。
- 每例以普通 space 为 baseline，并把 `tab`、无换行 block comment、混合 space/tab/comment 分别
  投放到每个单独 token gap、全部 gap 以及文件首尾。
- 每个变体必须保持与 baseline 完全相同的非 trivia `LexemeKind` 序列；Lexer 与 Parser 均零诊断。
- 以 roots、四张 AST table 的节点类别和源码顺序形成不含 Span 的结构指纹；每个变体与 baseline
  相等，同一源码重复解析的公开 `Debug` 产物也必须确定一致。
- 明确排除 LF / CRLF、line comment 和含换行 block comment；它们在文件声明分隔、裸 `return`
  等位置具有既定结构语义，不属于本等价关系。
- 不增加依赖、公开 API、新诊断或合法语法。

## 3. 非目标

- 不宣称 Span 在插入 trivia 后保持相等；Span 必须继续指向各自真实源码位置。
- 不替代 Lexer trivia 分段测试、文件换行 / 分号分隔测试或各语法领域的精确 AST 测试。
- 不测试字符串 / 字符 / comment 内部插入 trivia；矩阵只在完整 lexical token 之间变换。
- 不实现 formatter、CST、trivia attachment 或 guide 变更。

## 4. 验收标准

- [x] 至少 20 个 grammar case 覆盖目标语法领域，baseline 全部 Lexer / Parser clean。
- [x] 所有单 gap、全 gap、首尾 trivia 变体保持 significant token 序列与 AST 结构指纹不变。
- [x] 每个变体重复解析无内部错误且公开产物确定一致。
- [x] 矩阵发现的生产缺陷有最小修复和定向断言；本矩阵未发现生产缺陷，未改生产代码。
- [x] 新矩阵及直接相关 Lexer / Parser 窄测试与窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

新增独立 integration test，以 token spelling 切片构造 grammar case。renderer 只在切片边界插入
trivia，避免修改复合 symbol、literal 或 identifier 内部。Lexer 指纹过滤 `Trivia` 但保留
Token / Invalid / EOF；AST 指纹使用公开 typed table 的 payload discriminant、root ID、文件头
数量和节点顺序，不比较因真实字节偏移变化而必然不同的 Span。

若某变体产生差异，先检查该 gap 是否确为两个完整 token 的边界；只有确认输入满足矩阵前提后
才修复生产 Lexer / Parser，不得把结构性换行错误归类为 trivia 不变量。

## 6. 实施计划

1. [x] 审计现有 trivia 验收 → 验证：确认只有少量领域用例，缺少跨 grammar 的系统等价矩阵。
2. [x] 建立 grammar case 与 trivia 变体生成器 → 验证：20 个 grammar case、1,175 个源码变体、2,350 次解析。
3. [x] 运行窄测试并最小修复实际缺陷 → 验证：206/206，窄 Clippy 0 warnings；未发现生产缺陷。
4. [x] 运行一次 workspace 标准基线并同步完成记录 → 验证：标准命令全部成功。
5. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0077`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | trivia 等价矩阵、必要修复、Architecture 与完成记录 | `test(frontend): verify parser trivia invariance (SPEC-0077)` |

## 8. 未决问题

- 无。

## 9. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo test -p lang-frontend --test parser_trivia_invariance_matrix --test lexer --test parser_expression --test parser_declaration --test parser_block --test parser_file --test parser_call_argument --test parser_lambda --test parser_control_flow --test parser_class_family --locked --offline` | 通过 | 206/206；20 个 grammar case、1,175 个变体、2,350 次完整文件解析 |
| `cargo clippy -p lang-frontend --test parser_trivia_invariance_matrix --locked --offline -- -D warnings` | 通过 | 0 warnings |
| workspace Cargo 基线 | 通过 | 每条标准命令执行一次；`fmt`、`check`、Clippy、407 tests、CLI build；0 failed / ignored / measured / filtered |
