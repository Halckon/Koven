# SPEC-0083: 建立 Parser 词法 poison 插入矩阵

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-083` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guides/v0.34-pre-restructure/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强词法、语法与 Parser 测试验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0006、SPEC-0014、SPEC-0077、SPEC-0081、SPEC-0082 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` Lexer mode / 完整文件 Parser 额外 poison 恢复测试、必要边界修复、Architecture |
| 语言语义变更 | 否；只锁定不删除原语法 token 的额外 L0001 / L0002 输入及 string text 插入行为 |

## 1. Goal

完成后，覆盖现行主要语法 family 的合法完整文件在每个显著 token gap 插入非法字符或未来
保留字时，生产 Lexer 按当前 lexical mode 给出精确结果，完整文件 Parser 返回确定 AST / 用户
诊断而不产生内部错误，并在原语法 token 与所有 owner 均保留时恢复到后置合法哨兵声明。

## 2. 范围与需求

- 复用 SPEC-0079–0082 的 22 个 Lexer / Parser-clean 文件和 396 个原始显著 token；对每个文件
  枚举源码起点与每个原 token 末尾，共 418 个唯一 gap。
- 对每个 gap 分别插入带分隔空格的 `#` 与 `async`，共生成 836 个重新词法分析的源码；不删除、
  替换原 token，也不手工构造 Invalid / Reserved lexeme。
- 从 baseline token owner 序列确定插入点处于 code 还是 string mode：409 个 code-mode gap 的
  每种变体必须在插入 Span 精确产生一个 L0001 / L0002；9 个 string-mode gap 的每种变体必须
  作为普通 string text 保持 Lexer / Parser 零诊断。
- 每例验证 lexeme 连续覆盖、source identity、非 EOF lexeme 非空、唯一末尾 EOF，以及 Lexer /
  Parser 诊断与四张 AST table Span source-local 且有界。
- 同一 `LexedFile` 重复执行两次完整文件 Parser，要求无 `ParserInternalError` 且公开 `Debug`
  产物一致；code / string 两类 gap 数及两个 poison 的 mutation 总数必须固定且非零。
- 全部 836 个变体最后一个顶层 Item 的源码切片必须精确等于 `val sentinel = 0`；因原始 owner
  delimiter 全部保留，本 Spec 不放宽 owner 位置的顶层同步要求。
- 不增加依赖、公开 API、新诊断或合法语法；矩阵发现缺陷时只修复直接根因并添加定向断言。

## 3. 非目标

- 不把 `async` 或 `#` 变成合法 code token，也不改变 L0001 / L0002 的 code、message 或 Span。
- 不要求 string mode 内的 `#` / `async` 形成独立 `StringText` lexeme，只要求实际源码内容被现有
  Scanner 无损覆盖且整个文件保持合法。
- 不在 trivia 内部、char literal 内部或 UTF-8 scalar 内部插入，也不组合多个 gap mutation。
- 不替代 poison replacement、token duplication、随机 fuzzing 或领域精确 AST / 诊断测试。

## 4. 验收标准

- [x] 22 个共享样本、396 个 token 和 418 个 gap 保持固定计数与基线合法性。
- [x] 两种 poison 分别覆盖全部 gap，共执行 836 个重新词法分析的 mutation。
- [x] 409 个 code-mode gap 的每种 mutation 精确包含一次目标词法根因及插入 Span。
- [x] 9 个 string-mode gap 的每种 mutation 保持 Lexer / Parser 零诊断。
- [x] 全部 mutation 保持 Lexer 覆盖、有界 Span、Parser 无内部错误和重复产物一致。
- [x] 全部 mutation 精确保留后置哨兵顶层 Item。
- [x] 矩阵发现的生产缺陷有最小修复和定向断言，或明确记录未发现生产缺陷。
- [x] 新矩阵及直接相关 Lexer / Parser 窄测试与窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

把 SPEC-0081 的两类 poison 提取为 tests 私有共享模块。gap 枚举只遍历 baseline `LexedFile`
的生产 `TokenKind`，以 `Code` / `String` 栈在 StringStart、
InterpolationStart/End 与 StringEnd 后更新下一 gap 的真实 mode；well-formed baseline 结束时
必须回到唯一 `Code` 根。

mutation 依据 gap byte offset 执行 `prefix + space + poison + space + suffix`，再建立全新
`SourceMap` / `LexedFile`。code-mode 目标诊断直接核对插入源码的精确 Span；string-mode 不固定
lexeme 分段，只验证零诊断、连续覆盖和公开解析产物。

## 6. 实施计划

1. [x] 审计 replacement / duplication / trivia gap 矩阵 → 验证：确认缺少保留原语法的 poison gap 覆盖。
2. [x] 提取共享 poison 定义并建立 lexical-mode gap 枚举 → 验证：418 个 gap，409 / 9 分层。
3. [x] 建立 418 × 2 insertion 矩阵并验证 sentinel，最小修复实际缺陷 → 验证：836 个 mutation，无生产缺陷。
4. [x] 运行直接相关窄测试与窄 Clippy → 验证：136/136，0 warnings。
5. [x] 运行一次 workspace 标准基线并同步完成记录 → 验证：标准命令全部成功，417 tests。
6. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0083`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | poison gap 矩阵、必要修复、Architecture 与完成记录 | `test(frontend): verify parser lexical poison insertion (SPEC-0083)` |

## 8. 未决问题

- 无。

## 9. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo test -p lang-frontend --lib --test lexer --test lexer_boundary_matrix --test parser_expression --test parser_file --test parser_lexical_owner_matrix --test parser_token_omission_matrix --test parser_lexical_poison_replacement_matrix --test parser_token_duplication_matrix --test parser_lexical_poison_insertion_matrix --locked --offline` | 通过 | 136/136；836 个 insertion mutation，未发现生产缺陷 |
| `cargo clippy -p lang-frontend --all-targets --locked --offline -- -D warnings` | 通过 | 0 warnings |
| workspace Cargo 基线 | 通过 | 每条标准命令执行一次；`fmt`、`check`、Clippy、417 tests、CLI build；0 failed / ignored / measured / filtered |
