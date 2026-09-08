# SPEC-0160: 建立 Lexer 超长非法 lexeme 压力矩阵

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-160` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guides/v0.34-pre-restructure/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强 Lexer / 语法 / Parser 测试且简化验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0006、SPEC-0073、SPEC-0103、SPEC-0129、SPEC-0150、SPEC-0154、SPEC-0157 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` Lexer 大输入压力测试、Architecture |
| 语言语义变更 | 否；只锁定 L0003–L0008 在超长单 lexeme / owner 中的既有扫描行为 |

## 1. Goal

完成后，生产 Lexer 面对约 65,536-byte 的单个非法 comment、string/interpolation owner、escape、
char 或 number 时，必须保持精确 lexeme 分段、byte Span、唯一根因、EOF 与双运行确定性，不得
把输入长度转化为额外诊断、错误切片或内部错误。

## 2. 范围与需求

- 复用 `lexer_stress_matrix` 的 `LONG_RUN = 65_536`，新增七个确定性源码：unterminated block
  comment、unterminated string、unterminated interpolation、terminal string escape、长前后缀
  中的可恢复 invalid escape、closed invalid char、invalid numeric suffix。
- 七个源码分别精确产生一个 L0003、L0004、L0005、L0006、L0006、L0007、L0008；诊断 primary
  Span 必须等于 owner 全范围或对应 escape 精确范围，不得随长前后缀漂移。
- L0003 / L0007 / L0008 必须形成单个覆盖全部源码的 `Invalid` lexeme；L0004 / L0005 必须保留
  opener 与长 Text / Identifier 分段；terminal L0006 保留长 Text 与末尾一字节 Invalid。
- interior L0006 必须保留 StringStart / 长 Text / 两字节 Invalid / 长 Text / StringEnd 的完整
  恢复序列，证明错误后仍能扫描剩余 owner。
- 每个源码运行两次生产 Lexer，并验证 source identity、连续字节覆盖、唯一 EOF、source-local
  诊断 Span 与完整公开产物确定性。
- 更新后 `lexer_stress_matrix` 共处理 21 个大输入 / 小栈源码和 42 个 Lexer 产物。
- 不修改 Lexer 语义、诊断目录、公开 API 或依赖；发现复杂度或分段缺陷时只修复直接根因。

## 3. 非目标

- 不声明 65,536 为语言 token 长度上限，不设置 wall-clock 或内存阈值。
- 不重复大量短 L0001 流、reserved word、深 mode / brace 或 Parser 传播测试。
- 不覆盖所有 escape spelling、numeric suffix 组合或换行恢复位置。
- 不新增随机 fuzzing、property-testing 依赖或生产测试钩子。

## 4. 验收标准

- [x] 七个长错误源码分别产生唯一预期 L0003–L0008 与精确 primary Span。
- [x] 七种 lexeme 分段、长 payload Span、错误 Span 与唯一 EOF 全部精确锁定。
- [x] interior L0006 在错误后保留长后缀 Text 与 StringEnd，terminal L0006 精确停在 EOF。
- [x] 更新 target 共验证 21 个源码、42 个双运行 Lexer 产物及公开不变量。
- [x] 未发现生产缺陷，或缺陷有最小修复与定向回归证据。
- [x] 更新 target 及窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

扩展现有 `lexer_stress_matrix`，新增 tests 私有 lexeme-kind / diagnostic helper，仍通过
`lex_source_twice` 调用生产 Lexer。期望只描述公开 `LexemeKind`、Span 与诊断，不复制 scanner
循环或引入时间测量；七个 case 保持显式构造，避免通用生成器掩盖不同 mode 路径。

## 6. 实施计划

1. [x] 审计长合法 token 与短非法 token 压力证据 → 验证：单个超长非法 lexeme/owner 缺失。
2. [x] 增加 L0003–L0008 七种长错误源码 → 验证：分段、Span、唯一诊断与 EOF。
3. [x] 运行更新 target 与窄 Clippy → 验证：全部通过，0 warnings。
4. [x] 同步文档并运行一次 workspace 标准基线 → 验证：全部标准命令成功。
5. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0160`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | Lexer 超长非法 lexeme 压力矩阵、必要修复、Architecture 与完成记录 | `test(frontend): stress long invalid lexemes (SPEC-0160)` |

## 8. 未决问题

- 无。

## 9. 验证记录

- `cargo test -p lang-frontend --test lexer_stress_matrix --locked --offline` 通过：7 passed，
  0 failed / ignored / measured / filtered；target 共验证 21 个大源码和 42 个 Lexer 产物。
- `cargo clippy -p lang-frontend --test lexer_stress_matrix --locked --offline -- -D warnings`
  通过：0 warnings。
- 实现完成后执行 `cargo fmt --all`，随后标准基线一次通过。
- `cargo fmt --all -- --check` 通过。
- `cargo check --workspace --all-targets --locked --offline` 通过。
- `cargo clippy --workspace --all-targets --locked --offline -- -D warnings` 通过：0 warnings。
- `cargo test --workspace --all-targets --locked --offline` 通过：465 passed，0 failed / ignored /
  measured / filtered。
- `cargo build -p lang-cli --locked --offline` 通过。
