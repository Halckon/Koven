# SPEC-0171: 建立 Parser 超长文件头限定路径矩阵

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-171` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guides/v0.34-pre-restructure/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强 Lexer / 语法 / Parser 测试且简化验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0002、SPEC-0006、SPEC-0014–0015、SPEC-0093、SPEC-0103、SPEC-0128–0129、SPEC-0150–0151、SPEC-0170 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` 4,096-segment package/import Lexer / Parser 压力与末尾恢复测试、Architecture |
| 语言语义变更 | 否；只锁定单个超长 qualified header path 的既有平坦解析、Span 与恢复行为 |

## 1. Goal

完成后，完整文件入口必须稳定保留 4,096-segment package、exact alias import 与 wildcard import；
对应恢复源中的末尾 `.` 和缺 alias name 必须产生精确 L0048 / L0049 / L0050，同时保留全部真实
segment、directive 与最终 root，不得因单 directive 宽度发生递归、游标漂移或级联。

## 2. 范围与需求

- 合法源包含 4,096 个 `pN` segment 的 package、4,096 个 `qN` segment 且别名为 `Alias` 的 exact
  import、4,096 个 `wN` segment 的 wildcard import，以及最终 `val after = 1` root。
- 合法源必须零诊断；三个 header path 的 segment 数、每个 segment 源码切片、完整 directive Span、
  alias / wildcard Span 与最终 Variable root 均精确。
- 恢复源包含：4,096-segment package 后的末尾 `.`、4,096-segment import 后的末尾 `.`，以及
  4,096-segment exact import 的 `as` 后直接换行到 root。
- 恢复源必须按源码顺序仅产生 L0048、L0049、L0050；三条 primary 均为空 Span，分别锚定下一
  import / root starter。package 与前两个 import Span 保留末尾点；最后 import 保留真实 `as` 和
  空 alias name Span，最终 Variable root 不丢失。
- 两个源码均先执行双独立生产 Lexer；合法源精确包含 12,290 个 Identifier 与 12,286 个 Dot，
  恢复源包含 12,289 个 Identifier 与 12,287 个 Dot token，且零 Lexer 诊断；随后由 file helper
  双 Lexer / 双 Parser。
- 合计验证 8 个 Lexer 与 4 个 Parser 产物；全部 header / diagnostic / root Span 必须 source-local、
  确定且可安全切片。
- 不改变 Lexer / Parser 语义、诊断目录、公开 API 或依赖；发现长路径缺陷时只修复直接根因。

## 3. 非目标

- 不改变 qualified-name grammar、package/import 绑定、文件系统映射或跨文件名称解析。
- 不重复 SPEC-0170 的 4,096 directive 数量压力；本 Spec 只扩大单 directive 的 segment 数。
- 不引入递归路径结构、增量解析、wall-clock / 内存阈值或新的固定语言上限。
- 不检查类型、所有权或运行时求值。
- 不新增共享生产状态、测试钩子或第三方依赖。

## 4. 验收标准

- [x] 合法源保留三个 4,096-segment header path、alias / wildcard 与最终 root，且零诊断。
- [x] 恢复源仅产生有序 L0048 / L0049 / L0050，保留全部 segment、末尾 marker 与最终 root。
- [x] 两源的 Identifier / Dot token 数量、directive / diagnostic / root Span 精确且 source-local。
- [x] 两个源码共验证 8 个 Lexer 与 4 个 Parser 公开产物及确定性不变量。
- [x] 未发现生产缺陷，或缺陷有最小修复与定向回归证据。
- [x] 新 qualified-header path target 及窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

新增独立 integration target，以 tests 私有 path / source builder 生成 4,096 个 segment 和精确期望
Span。测试先复用 `lex_parser_source_twice` 检查公开 LexedFile，再复用 `parse_file_twice` 检查
package/import typed header、diagnostics 和 root；不读取 Parser 私有 cursor 或恢复状态。

## 6. 实施计划

1. [x] 审计 file suite 与大 header 压力 → 验证：已有 4,096 directive 数量证据，但单 path 仅覆盖 2–5 个 segment。
2. [x] 建立合法 / 恢复长路径源 → 验证：24,576 个 segment、typed marker、三条诊断与最终 root。
3. [x] 运行新 target 与窄 Clippy → 验证：全部通过，0 warnings。
4. [x] 同步文档并运行一次 workspace 标准基线 → 验证：全部标准命令成功。
5. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0171`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | Parser 长文件头限定路径矩阵、必要修复、Architecture 与完成记录 | `test(frontend): stress qualified header paths (SPEC-0171)` |

## 8. 未决问题

- 无。

## 9. 验证记录

- 新增 `parser_large_qualified_header_paths` integration target；两源合计 24,576 个 segment，验证
  8 个 Lexer 与 4 个 Parser 公开产物，未发现生产缺陷，未修改生产代码。
- `cargo test -p lang-frontend --test parser_large_qualified_header_paths --locked --offline`：通过，
  1 passed，0 failed / ignored。
- `cargo clippy -p lang-frontend --test parser_large_qualified_header_paths --locked --offline -- -D warnings`：
  通过，0 warnings。
- `cargo fmt --all -- --check`：通过。
- `cargo check --workspace --all-targets --locked --offline`：通过。
- `cargo clippy --workspace --all-targets --locked --offline -- -D warnings`：通过，0 warnings。
- `cargo test --workspace --all-targets --locked --offline`：通过；81 个测试产物合计 476 passed，
  0 failed / ignored。
- `cargo build -p lang-cli --locked --offline`：通过。
