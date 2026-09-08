# SPEC-0170: 建立 Parser 大文件头压力矩阵

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-170` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guides/v0.34-pre-restructure/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强 Lexer / 语法 / Parser 测试且简化验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0002、SPEC-0006、SPEC-0014–0015、SPEC-0093、SPEC-0103、SPEC-0128–0129、SPEC-0150–0151、SPEC-0168–0169 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` 大规模 package/import 文件头 Lexer / Parser 压力测试、Architecture |
| 语言语义变更 | 否；只锁定 4,096 项合法与恢复 import header 的既有线性、顺序和 typed 产物行为 |

## 1. Goal

完成后，完整文件入口必须稳定解析 4,096 个混合合法 import，并线性恢复 4,096 个缺 target 的
import；两种大文件都必须保留 package、源码顺序的 header 产物、LF / CRLF 分隔、最终普通 root
及精确诊断，不得游标漂移、吞 directive、把 header 降级为 root 或产生级联。

## 2. 范围与需求

- 合法源以 `package stress.headers` 开头，随后按四项循环生成 4,096 个 exact multi-segment、alias、
  wildcard 与更长 qualified import；每项使用交替 LF / CRLF 分隔，最后保留 `val after = 1` root。
- 合法源必须零诊断；package 两个 segment、全部 import Span / keyword / segment、wildcard / alias
  形态和源码切片逐项准确，imports 顺序与生成顺序一致，且恰有一个 Variable root。
- 恢复源同样先放 package，随后生成 4,096 个仅含 `import` keyword、缺 target 的 directive，并交替
  LF / CRLF；最后保留同一 root。每项必须产生一条 L0049，primary 是下一个 header / root starter
  处的空 Span；4,096 个恢复 ImportDirective 均只有真实 keyword，无 segment / wildcard / alias。
- 两个源码均先执行双独立生产 Lexer，精确检查 4,096 个 import keyword、2,049 个 LF Newline 与
  2,048 个 CRLF Newline trivia、连续覆盖和零 Lexer 诊断；随后由 file helper 双 Lexer / 双 Parser。
- 合计验证 8 个 Lexer 与 4 个 Parser 产物；合法与恢复源的 package / imports / roots / diagnostics
  均必须 source-local、确定且可按 UTF-8 byte Span 安全切片。
- 不改变 Lexer / Parser 语义、诊断目录、公开 API 或依赖；发现大 header 缺陷时只修复直接根因。

## 3. 非目标

- 不改变 package/import 语法、名称可见性或跨文件名称解析；本 Spec 仍只验证 Phase 1 AST。
- 不重复 512 roots、4,096 普通声明/error region 或 256 简单 import 的既有小型断言。
- 不引入并行解析、增量编译、wall-clock / 内存阈值或新的固定语言上限。
- 不检查类型、所有权或运行时求值。
- 不新增共享生产状态、测试钩子或第三方依赖。

## 4. 验收标准

- [x] 合法源保留 package、4,096 个混合 import 与最终 Variable root，全部精确且零诊断。
- [x] 恢复源保留 4,096 个空 target import、4,096 条有序 L0049 与最终 Variable root。
- [x] 两源均精确保留 4,096 个 import keyword、2,049 个 LF 与 2,048 个 CRLF Newline trivia。
- [x] 所有 header / diagnostic / root Span source-local、可切片且顺序确定。
- [x] 两个源码共验证 8 个 Lexer 与 4 个 Parser 公开产物及确定性不变量。
- [x] 未发现生产缺陷，或缺陷有最小修复与定向回归证据。
- [x] 新 file-header stress target 及窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

新增独立 integration target，以 tests 私有 source builder 生成合法 / 恢复 header 与逐项期望 Span。
测试先复用 `lex_parser_source_twice` 检查公开 LexedFile，再复用 `parse_file_twice` 检查完整 typed
header、diagnostics 和 root；不读取 Parser 私有 cursor、恢复状态或计数器。

## 6. 实施计划

1. [x] 审计 file suite、平坦压力与 header 覆盖 → 验证：既有上限为 256 个简单 import，缺少 4,096 项混合正例和同规模 target 恢复。
2. [x] 建立合法 / 恢复大 header → 验证：8,192 个 directive、typed 形态、诊断与最终 root。
3. [x] 运行新 target 与窄 Clippy → 验证：全部通过，0 warnings。
4. [x] 同步文档并运行一次 workspace 标准基线 → 验证：全部标准命令成功。
5. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0170`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | Parser 大文件头压力矩阵、必要修复、Architecture 与完成记录 | `test(frontend): stress large file headers (SPEC-0170)` |

## 8. 未决问题

- 无。

## 9. 验证记录

- 新增 `parser_large_file_header_stress` integration target：合法 / 恢复两个源码分别保留 4,096 个
  import、package 与最终 Variable root；独立 Lexer 加 file helper 双运行共覆盖 8 个 Lexer 和 4 个
  Parser 产物。
- 合法源四类 import 的完整 Span、segment、wildcard、alias 与顺序逐项准确且零诊断；恢复源保留
  4,096 个空 target directive 与 4,096 条有序空 Span L0049。两源均精确包含 4,096 个 import
  keyword、2,049 个 LF 和 2,048 个 CRLF Newline trivia。
- 未发现生产 Lexer / Parser 缺陷；未修改生产代码、公开 API、诊断目录或依赖。
- `cargo test -p lang-frontend --test parser_large_file_header_stress --locked --offline`：最终重跑
  1 passed，0 failed，0 ignored，0 filtered out。
- 窄 Clippy 首轮仅报告测试 helper 的 `manual_is_multiple_of`；改用 `usize::is_multiple_of` 后重跑
  窄测试，并执行
  `cargo clippy -p lang-frontend --test parser_large_file_header_stress --locked --offline -- -D warnings`：
  通过，0 warnings。
- workspace 标准基线在实现与 Architecture 同步后仅执行一次：
  - `cargo fmt --all -- --check`：通过。
  - `cargo check --workspace --all-targets --locked --offline`：通过。
  - `cargo clippy --workspace --all-targets --locked --offline -- -D warnings`：通过，0 warnings。
  - `cargo test --workspace --all-targets --locked --offline`：475 passed，0 failed，0 ignored。
  - `cargo build -p lang-cli --locked --offline`：通过。
