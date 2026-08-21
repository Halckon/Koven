# SPEC-0068: 增加 Lexer / Parser 对抗组合矩阵

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-068` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guide/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强词法、语法与 Parser 测试验收 |
| 前置 Spec | SPEC-0006、SPEC-0014 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` Parser lexical-recovery、Lexer / 完整文件 Parser 集成测试、Architecture |
| 语言语义变更 | 否；修复阶段内部恢复契约并增加确定性测试证据 |

## 1. Goal

完成后，一组确定性生成的短源码组合会共同验证生产 Lexer 的完整字节覆盖，以及完整文件
Parser 面对跨词法模式和语法 owner 的合法或非法输入时不崩溃、重复运行结果稳定。

## 2. 范围与需求

- 使用固定前缀与后缀做笛卡尔积，覆盖声明、参数、block、call、索引、lambda、control-flow、
  字符串插值、字符、注释、Unicode、NUL 与不匹配 closer。
- 每个 case 验证 lexeme 从字节 0 连续覆盖至唯一 EOF，所有 span 属于当前 source 且不越界。
- 对同一词法产物重复调用 `parse_file`，精确比较诊断和四张 AST table 的 span 指纹、顶层根、
  package / import 结构，证明确定性且无用户输入触发的内部错误。
- 若矩阵发现生产 Lexer 的终止性根因抑制了外层 lexical-owner 诊断，Parser 必须在 EOF 恢复
  剩余 owner，保留原 Lexer 诊断且不得返回 `InvalidLexemeStream`。
- 只使用标准库和公开 frontend API，不增加依赖。

## 3. 非目标

- 不以组合矩阵替代已有精确语义测试、fixture、复杂度测试或 Tree-sitter corpus。
- 不引入随机种子、fuzzer、snapshot、第三方 property-testing crate 或新诊断。
- 不改变 Lexer、AST、guide、公开 API 或既有用户诊断。

## 4. 验收标准

- [x] 组合矩阵 case 数由测试精确断言且非零，覆盖列出的词法与 owner 边界。
- [x] 每个 case 的 lexeme 完整覆盖、source identity 与 EOF 不变量通过。
- [x] 每个 case 重复解析的诊断、AST span、文件头与根指纹完全一致。
- [x] 终止性字符根因位于 interpolation 时只保留 `L0007`，Parser 不产生内部错误或级联诊断。
- [x] frontend 窄测试与一次 workspace 标准基线通过。
- [x] Architecture 与 Spec 状态同步并创建独立提交。

## 5. 技术方案与边界

新增独立 integration test；指纹只包含公开且应确定的分类、code/message/span、节点 table span
及文件结构，不依赖 `Debug` 地址或私有 Parser 状态。组合数据写在测试内，保持无 IO、无环境和
随机性。Parser lexical-recovery 只在 EOF 已有终止性 Lexer 根因时按 inner-to-outer 顺序补齐
剩余 string/interpolation owner；无终止根因的不平衡仍作为内部不变量失败。

## 6. 实施计划

1. [x] 添加组合矩阵和不变量 helper，修复发现的 lexical-owner EOF 恢复 → 验证：新增 integration test。
2. [x] 同步 Architecture、Spec 与索引 → 验证：文档事实一致。
3. [x] 运行一次 workspace 基线并提交 → 验证：staged diff 单一且提交成功。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | 对抗矩阵、Architecture 与完成记录 | `test(frontend): add adversarial parse matrix (SPEC-0068)` |

## 8. 未决问题

- 无。

## 9. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo test -p lang-frontend --test frontend_adversarial --locked --offline` | 通过 | 2/2；324-case 矩阵与定向 `L0007` 回归 |
| `cargo test -p lang-frontend --lib --test lexer --test parser_expression --test parser_file --locked --offline` | 通过 | 123/123 lexical-recovery 相关测试 |
| workspace Cargo 基线 | 通过 | `fmt`、`check`、Clippy、388 tests、CLI build；0 failed / ignored / measured / filtered |
