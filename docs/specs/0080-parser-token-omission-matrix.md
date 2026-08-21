# SPEC-0080: 建立 Parser 单 token 缺失恢复矩阵

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-080` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guide/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强词法、语法与 Parser 测试验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0006、SPEC-0014、SPEC-0069、SPEC-0075、SPEC-0079 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` 完整文件 Parser 中间缺失恢复测试、必要边界修复、Architecture |
| 语言语义变更 | 否；只锁定单个既有显著 token 缺失时的总性、确定性与后续边界恢复 |

## 1. Goal

完成后，覆盖现行主要语法 family 的合法完整文件在任一显著 token 被完整删除时，生产
Lexer / 完整文件 Parser 仍保持总性和确定性；删除未破坏 string/interpolation 或成对
delimiter owner 时，Parser 还能恢复到后置合法哨兵声明，而不会吞掉全部后续顶层源码。

## 2. 范围与需求

- 复用 SPEC-0079 的 22 个 Lexer / Parser-clean 完整文件 corpus，避免两份代表性语法表漂移；
  corpus 提取只改变测试组织，不改变既有矩阵行为与计数。
- 在每个样本后追加真实换行与 `val sentinel = 0`，用生产 Lexer 枚举原样本范围内全部非 trivia、
  非 EOF lexeme；每个变体只删除一个完整 lexeme Span，不拼接或伪造 token。
- 每个变体重新使用生产 Lexer，验证 lexeme 连续覆盖、同 source、非 EOF lexeme 非空、唯一末尾
  EOF，以及 Lexer / Parser 诊断和四张 AST table 的 Span source-local 且有界。
- 同一 `LexedFile` 重复执行两次完整文件 Parser，要求无 `ParserInternalError` 且公开 `Debug`
  产物一致；矩阵必须固定样本数、删除变体总数和 owner / 非 owner 两类数量。
- 删除 string start/end、interpolation start/end、`()` / `[]` / `{}` 或泛型角括号时，只锁定
  总性与确定性；删除其他 token 时，最后一个顶层 Item 的源码切片必须精确等于哨兵声明。
- 不增加依赖、公开 API、新诊断或合法语法；矩阵发现缺陷时只修复直接根因并添加定向断言。

## 3. 非目标

- 不断言每个缺失位置必须产生某个固定诊断 code 或恢复 AST；精确语义仍由领域测试负责。
- 不要求在 owner closer 被删除后把 owner 内的哨兵错误提升为顶层声明。
- 不删除 trivia、invalid 或 EOF，也不组合两个以上 mutation；这些不属于单 token 缺失 Goal。
- 不替代随机 fuzzing、前缀截断、token inventory、diagnostic witness 或 pass / fail fixture。

## 4. 验收标准

- [x] 22 个共享合法样本保持既有 SPEC-0079 前缀矩阵行为与固定计数。
- [x] 每个原始显著 token 恰好形成一个删除变体，owner / 非 owner 两类均非空且数量固定。
- [x] 全部变体保持 Lexer 覆盖、有界 Span、Parser 无内部错误和重复产物一致。
- [x] 全部非 owner 删除变体精确保留后置哨兵顶层 Item。
- [x] 矩阵发现的生产缺陷有最小修复和定向断言，或明确记录未发现生产缺陷。
- [x] 新矩阵及直接相关 Lexer / Parser 窄测试与窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

把 SPEC-0079 的静态 `GrammarCase` 表移入 tests 下的共享私有模块，由两个 integration test
直接包含；不创建生产 API 或通用框架。mutation 依据原始 lexeme 的字节 Span 执行
`prefix + suffix`，然后重新词法分析，不能复用已不对应源码的 `LexedFile`。哨兵是否存活通过
最后一个 root Item 的 node Span 回切当前 source 验证，不依赖声明 payload 的私有字段。

owner-affecting 集合只表达不能普遍承诺顶层同步的已知语法边界，不改变生产 Parser 规则。
若非 owner 删除丢失哨兵，先缩小到 token kind / source offset，再判断是否遗漏了真实 owner，
或生产恢复跨越了已经重新平衡的顶层边界。

## 6. 实施计划

1. [x] 审计 AST / lexeme API 与现有中间缺失恢复测试 → 验证：现有覆盖为定向 case，缺少逐 token 矩阵。
2. [x] 提取共享合法 corpus 并保持 SPEC-0079 行为 → 验证：22 个样本、1,373 个前缀保持通过。
3. [x] 建立逐 token 删除、总性与哨兵恢复断言 → 验证：396 个 mutation，96 owner / 300 非 owner。
4. [x] 运行直接相关窄测试与窄 Clippy，最小修复实际缺陷 → 验证：216/216，0 warnings；修复两个恢复根因。
5. [x] 运行一次 workspace 标准基线并同步完成记录 → 验证：标准命令全部成功，413 tests。
6. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0080`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | 共享 corpus、单 token 缺失矩阵、必要修复、Architecture 与完成记录 | `test(frontend): verify parser token omission recovery (SPEC-0080)` |

## 8. 未决问题

- 无。

## 9. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo test -p lang-frontend --lib --test parser_token_omission_matrix --test parser_prefix_truncation_matrix --test parser_expression --test parser_declaration --test parser_file --test parser_class_family --test parser_call_argument --test parser_block --test parser_lambda --test parser_control_flow --test parser_lexical_owner_matrix --locked --offline` | 通过 | 216/216；396 个 deletion case 与两个定向生产回归 |
| `cargo clippy -p lang-frontend --all-targets --locked --offline -- -D warnings` | 通过 | 0 warnings |
| workspace Cargo 基线 | 通过 | 每条标准命令执行一次；`fmt`、`check`、Clippy、413 tests、CLI build；0 failed / ignored / measured / filtered |
