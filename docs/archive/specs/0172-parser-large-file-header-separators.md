# SPEC-0172: 建立 Parser 大文件头分隔与恢复矩阵

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-172` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guides/v0.34-pre-restructure/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0002、SPEC-0006、SPEC-0014–0015、SPEC-0062、SPEC-0078、SPEC-0093、SPEC-0103、SPEC-0128–0129、SPEC-0150–0151、SPEC-0170–0171 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` 4,096-import 文件头分隔与 L0053 恢复测试、Architecture |
| 语言语义变更 | 否；只锁定既有文件头分隔、Span、诊断与平坦恢复行为 |

## 1. Goal

完成后，完整文件入口必须在 4,096 个 import 的大文件头中稳定接受 LF、CRLF、分号和内部含
换行的 block comment 四类合法分隔；对应完全缺失分隔的源码必须对每个相邻 header / root
starter 产生一条精确 L0053，同时保留 package、全部 imports 与最终 root，不得停滞、吞项或级联。

## 2. 范围与需求

- 合法源包含 `package stress.separators`、4,096 个 `import pkg.ItemN` 与最终
  `val after = 1`；import 后依次循环 LF、CRLF、`;`、含单个 LF 的 block comment 分隔。
- 合法源必须零诊断，精确保留 package、4,096 个 imports、各自两个 qualified-name segment、
  directive / keyword Span 与最终 Variable root。
- Lexer 必须观察到 4,096 个 import keyword、4,097 个 Dot、8,195 个 Identifier、1,024 个
  Semicolon、1,024 个 BlockComment 和 2,049 个独立 Newline trivia；block comment 内换行不得额外
  拆成 Newline trivia。
- 恢复源使用相同 package、imports 与 root，但所有 4,097 个相邻边界只含普通空格；必须按源码
  顺序仅产生 4,097 条 L0053，primary 依次精确覆盖 4,096 个 `import` 与最终 `val` starter。
- 恢复源仍须保留全部 header 节点、segment、directive Span 与最终 root；不得生成 Error root 或
  其他诊断，也不得把普通空格误判为结构换行。
- 两个源码均先执行双独立生产 Lexer，再由 file helper 双 Lexer / 双 Parser，合计验证 8 个 Lexer
  与 4 个 Parser 产物；全部 AST / diagnostic Span 必须 source-local、确定且可安全切片。
- 不改变 Lexer / Parser 语义、诊断目录、公开 API 或依赖；发现缺陷时只修复直接根因。

## 3. 非目标

- 不重复 SPEC-0170 的合法 import 形态 / 缺 target 恢复或 SPEC-0171 的单路径 segment 宽度。
- 不改变文件头分隔 grammar、block comment 词法规则或 L0053 含义。
- 不测试重复 / 错位 package/import、package 映射、名称解析、类型或所有权。
- 不引入 wall-clock / 内存阈值、随机语料、新共享生产状态或第三方依赖。

## 4. 验收标准

- [x] 合法源通过四类分隔保留 package、4,096 个 imports 与最终 root，且零诊断。
- [x] 合法源的 keyword / Dot / Identifier / Semicolon / BlockComment / Newline 数量精确。
- [x] 恢复源仅产生有序 4,097 条 L0053，primary 精确覆盖下一 import / root starter。
- [x] 恢复源保留全部 imports、segments 与最终 root，不产生 Error root 或额外诊断。
- [x] 两个源码共验证 8 个 Lexer 与 4 个 Parser 公开产物及确定性不变量。
- [x] 未发现生产缺陷，或缺陷有最小修复与定向回归证据。
- [x] 新 header-separator target 及窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

新增独立 integration target，以 tests 私有 source builder 生成完整源码与精确期望 Span。测试先
复用 `lex_parser_source_twice` 检查公开 LexedFile，再复用 `parse_file_twice` 检查 typed header、
diagnostics 和 root；不读取 Parser 私有 cursor、inspection 计数或恢复状态。

## 6. 实施计划

1. [x] 审计 file header 与大输入 suite → 验证：小型 L0053 和大规模合法 / 缺 target 已覆盖，
   但大规模混合分隔及缺分隔恢复缺失。
2. [x] 建立合法 / 恢复大文件头源 → 验证：8,192 个 imports 与 4,097 条精确 L0053。
3. [x] 运行新 target 与窄 Clippy → 验证：全部通过，0 warnings。
4. [x] 同步文档并运行一次 workspace 标准基线 → 验证：全部标准命令成功。
5. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0172`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | Parser 大文件头分隔矩阵、必要修复、Architecture 与完成记录 | `test(frontend): stress file header separators (SPEC-0172)` |

## 8. 未决问题

- 无。

## 9. 验证记录

- 新增 `parser_large_file_header_separators` integration target；两个源码共验证 8,192 个 imports、
  4,097 个缺分隔边界、8 个 Lexer 与 4 个 Parser 公开产物，未发现生产缺陷，未修改生产代码。
- 首次窄测试在新增测试辅助函数处编译失败：`Lexeme::kind()` 返回按值的 `LexemeKind`，辅助函数
  错误要求引用；修正测试签名后未再出现该错误，生产代码未改动。
- `cargo test -p lang-frontend --test parser_large_file_header_separators --locked --offline`：最终重跑
  通过，1 passed，0 failed / ignored。
- `cargo clippy -p lang-frontend --test parser_large_file_header_separators --locked --offline -- -D warnings`：
  通过，0 warnings。
- `cargo fmt --all -- --check`：通过。
- `cargo check --workspace --all-targets --locked --offline`：通过。
- `cargo clippy --workspace --all-targets --locked --offline -- -D warnings`：通过，0 warnings。
- `cargo test --workspace --all-targets --locked --offline`：通过；82 个测试产物合计 477 passed，
  0 failed / ignored。
- `cargo build -p lang-cli --locked --offline`：通过。
