# SPEC-0079: 建立 Parser 前缀截断恢复矩阵

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-079` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guide/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强词法、语法与 Parser 测试验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0006、SPEC-0014、SPEC-0068、SPEC-0074、SPEC-0078 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` Lexer / 完整文件 Parser EOF 恢复测试、必要边界修复、Architecture |
| 语言语义变更 | 否；只锁定任意合法 UTF-8 前缀作为不可信用户输入时的阶段总性与确定性 |

## 1. Goal

完成后，一组覆盖文件头、声明、callable、block、lambda、control-flow、postfix、class-family、
接口委托、完整运算符层级及嵌套字符串插值的合法完整文件，在每个 Unicode scalar 边界被 EOF
截断时，生产 Lexer 都保持完整字节覆盖，完整文件 Parser 都返回确定 AST / 诊断或已定义资源
错误，不因普通截断输入 panic 或误报内部 lexeme stream 损坏。

## 2. 范围与需求

- 建立至少 20 个互异且 Lexer / Parser 均无诊断的完整文件样本，覆盖现行主要语法 family；
  额外包含非 ASCII `Char` / `String`、嵌套 interpolation 和 comment 内容。
- 对每个样本枚举空前缀、每个 UTF-8 scalar 结束位置及完整源码；矩阵必须断言样本数、互异性和
  实际前缀总数，不能因空集合或重复样本静默缩减。
- 每个前缀使用生产 Lexer，验证 lexeme 按字节连续覆盖、同 source、非 EOF lexeme 非空且唯一
  EOF 精确位于源码末尾；Lexer 与 Parser 的全部主 / label Span 均 source-local 且有界。
- 同一 `LexedFile` 重复执行两次完整文件 Parser，要求均无 `ParserInternalError`，并比较公开
  `Debug` 产物，锁定诊断顺序、恢复 AST 与 Span 的确定性。
- 不增加依赖、公开 API、新诊断或合法语法；矩阵发现缺陷时只修复直接根因并添加定向断言。

## 3. 非目标

- 不把任意字节前缀强行送入 `SourceMap`；无效 UTF-8 仍由 source 加载边界负责拒绝。
- 不为每个截断位置固定诊断 code 或恢复 AST 形态；精确语义继续由领域测试和 diagnostic witness
  矩阵负责。
- 不替代随机 fuzzing、复杂度 / 递归预算测试、四个公开入口库存矩阵或 pass / fail fixture。
- 不改变 EOF、词法错误抑制、语法恢复或资源错误的现行契约。

## 4. 验收标准

- [x] 至少 20 个合法完整文件覆盖列出的语法与词法 owner，并由测试证明基线零诊断。
- [x] 全部 UTF-8 scalar 前缀实际执行，case 数固定且每例保持完整 lexeme 覆盖和有界 Span。
- [x] 每个前缀重复解析无内部错误，公开 AST / 诊断产物确定一致。
- [x] 矩阵发现的生产缺陷有最小修复和定向断言，或明确记录未发现生产缺陷。
- [x] 新矩阵及直接相关 Lexer / Parser 窄测试与窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

新增独立 integration test，以静态合法源码表作为唯一输入。测试先完整解析每个样本，再通过
`char_indices()` 派生合法前缀结束位置；不复制 Scanner 或 Parser 私有状态机。确定性比较使用
公开产物的 `Debug`，Span 验证遍历 lexeme、诊断及四张 AST table。若某个前缀返回内部错误，
先缩小到所属样本和 offset，再判断是 Lexer owner 终止恢复还是具体 grammar EOF 恢复根因。

## 6. 实施计划

1. [x] 审计既有对抗、库存、owner、trivia 与换行矩阵 → 验证：确认缺少完整语法逐前缀 EOF 总性覆盖。
2. [x] 建立合法文件 corpus 与 UTF-8 前缀枚举 → 验证：22 个完整样本零诊断，1,373 个前缀固定执行。
3. [x] 验证 Lexer 覆盖、Span 与 Parser 重复结果并最小修复缺陷 → 验证：每个前缀两次解析，未发现生产缺陷。
4. [x] 运行直接相关窄测试与窄 Clippy → 验证：71/71，0 warnings。
5. [x] 运行一次 workspace 标准基线并同步完成记录 → 验证：标准命令全部成功，410 tests。
6. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0079`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | 前缀截断矩阵、必要修复、Architecture 与完成记录 | `test(frontend): verify parser prefix recovery (SPEC-0079)` |

## 8. 未决问题

- 无。

## 9. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo test -p lang-frontend --test parser_prefix_truncation_matrix --test frontend_adversarial --test parser_token_inventory --test parser_file --test parser_class_family --test parser_control_flow --test lexer --locked --offline` | 通过 | 71/71；22 个完整文件、1,373 个 UTF-8 前缀、2,790 次总解析（其中前缀 2,746 次） |
| `cargo clippy -p lang-frontend --test parser_prefix_truncation_matrix --locked --offline -- -D warnings` | 通过 | 0 warnings |
| workspace Cargo 基线 | 通过 | 每条标准命令执行一次；`fmt`、`check`、Clippy、410 tests、CLI build；0 failed / ignored / measured / filtered |
