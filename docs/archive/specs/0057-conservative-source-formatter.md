# SPEC-0057：保守、稳定且幂等的源码格式化器

## 1. 元数据

| 字段 | 值 |
|---|---|
| 状态 | `done` |
| Goal ID | `KOV-P6-057` |
| 所属 Phase | Phase 6 |
| 语言规范 | 现行 [v0.28 Lexer trivia 与 Phase 6](../guides/v0.34-pre-restructure/02-lexical-spec.md)、[roadmap](../guides/v0.34-pre-restructure/06-roadmap.md#phase-6工具链完善) |
| 批准依据 | 当前持续 Goal“继续推进 guide 主线，分阶段实施 specs”的站立授权 |
| 前置 Spec | SPEC-0006、SPEC-0014 `done`；完整 Lexer/Parser 公开不变量矩阵已完成 |
| 前置 ADR | [ADR-0003](../../adr/accepted/0003-diagnostic-architecture.md)、[ADR-0004](../../adr/accepted/0004-source-span-position-model.md)、[ADR-0013](../../adr/accepted/0013-conservative-source-formatting.md) `accepted` |
| 阻塞项 | 无；不依赖多文件 package 解析、LSP、标准库、codegen 或 SPEC-0040/0043 |
| 影响范围 | `lang-frontend` formatting API 与 corpus；`lang-cli` format/stdout/check driver；Architecture、roadmap |
| 语言语义变更 | 否；保留全部非 trivia token、comment 与 newline 边界，只规范合法文件的安全空白 |

## 2. Goal

完成后，调用方和 `kovenc format` 能对任意当前合法单文件 Koven source 产生确定、幂等、可重新
解析且不改变 token/comment/newline 序列的格式化文本；非法文件结构化失败，CLI 不原地覆盖源码。

## 3. 范围与需求

### 3.1 frontend formatting API

- 新增职责单一的 `formatting` 模块，公开 `format_source(&SourceMap, SourceId)` 和结构化
  `FormattingError`；source identity/internal Lexer/Parser 错误与用户 diagnostics 分开建模。
- 先运行生产 Lexer 和 `parse_file`；任一诊断都返回原有 `Diagnostic` 序列，不返回部分格式化
  文本。成功路径只从 lexeme `Span` 读取原始字节，不维护第二份关键字/符号 spelling 表。
- formatter 使用显式 delimiter/line state 单趟生成，复杂度相对输入字节和 lexeme 数线性；深层
  delimiter 不递归，不因用户输入 panic。

### 3.2 whitespace/comment/newline contract

- 完整实施 ADR-0013：四空格 block/continuation indentation、同行 gap 的零/一空格规则；不插入、
  删除或改写 LF/CRLF，不改非 trivia token bytes，不改 comment bytes/order。
- string、char、number、identifier、reserved/invalid 区域不进入文本级替换；合法字符串插值内的
  expression token 可格式化，但 StringText 保持逐字节。
- 空文件、只有 trivia、尾部 whitespace、line/block comments、空/嵌套 delimiter、Unicode、CRLF、
  typed call、lambda、class-family、control-flow 与完整文件 header 均有正例。

### 3.3 不变量与 corpus

- 单元测试锁定 spacing/indent/comment/newline 的小矩阵；integration test 枚举全部 parser-pass
  `.ko` fixture 和专用 formatter corpus，零个输入视为配置错误。
- 每个 case 断言：首次格式化成功、再次格式化字节相同；格式化前后 significant lexeme
  `(kind, bytes)` 相同、comment `(kind, bytes)` 相同、newline bytes 相同；结果重新完整解析零诊断。
- 词法/语法错误分别返回原诊断 code/span；foreign SourceId 返回内部 source error且不 panic。

### 3.4 非破坏性 CLI

- `kovenc format <path>` 只把格式化 UTF-8 写 stdout；`kovenc format --check <path>` 不写 source，
  相同退出 0、有差异退出 1。两者都不修改输入文件。
- 无/多余参数、未知 option、读取失败、non-UTF-8、frontend diagnostics 或内部错误退出 2；用户
  diagnostics 复用现有 renderer 写 stderr，参数/IO/internal failure 使用 CLI 自有简短错误。
- stdout/stderr 写入失败返回非零，不 panic；CLI integration test 使用临时文件和真实 binary
  锁定退出码、输出与输入不变。

## 4. 非目标

- 不实现 line wrapping/max width、import/member 排序、空行合并、尾随逗号、引号/数值规范化、
  doc comment reflow、range formatting、增量/CST formatting 或无效源码 best effort。
- 不实现原地 `--write`、目录递归、stdin、配置文件、ignore 文件、颜色或 machine-readable output。
- 不改变 Lexer/Parser AST、诊断码、语法、新行分隔、标准库、LSP protocol 或 codegen。
- 不新增 crate、第三方依赖或 Tree-sitter runtime。

## 5. 验收标准

- [x] frontend API 对完整合法 corpus 确定且幂等，格式化结果重解析零诊断。
- [x] 前后 significant token bytes/kind、comment bytes/kind/order 与每个 LF/CRLF lexeme 完全一致；
      spacing/四空格 delimiter indentation 符合 ADR-0013。
- [x] lexer/parser diagnostics 保留 code/span，foreign source/internal error fail-loud；空 corpus 失败。
- [x] 真实 `kovenc format` stdout 与 `--check` 的 0/1/2 退出矩阵通过，输入文件字节不变。
- [x] `lang-frontend`、`lang-cli` 窄测及 workspace 五项基线通过；production 文件低于 1000 行，
      Architecture/Spec/roadmap/ADR 索引只记录实际事实。

## 6. 技术方案与边界

- formatting engine 维护前一个/当前 significant lexeme、gap trivia、line-start 与 delimiter stack；
  comments/newlines作为必须立即提交的边界，普通 whitespace 延迟到相邻 token 都已知后决定。
- gap 分类只读取 `LexemeKind` 和原始 slice；上下文相关 `<`/`>`、unary token 额外读取原 gap 是否
  存在，不访问类型检查或名称解析。
- CLI 保持薄层：创建单 source map、调用 formatter、复用 renderer；参数解析是固定小状态机，
  不引入 CLI framework。

## 7. 实施计划

1. [x] 实现 frontend formatting engine 与小型行为矩阵 → 验证：spacing/indent/comment/newline、
   diagnostics、foreign source 和线性深度。
2. [x] 建立全 parser-pass corpus 不变量 → 验证：非空枚举、token/comment/newline 保持、幂等和
   重解析。
3. [x] 接入 `kovenc format` stdout/`--check` → 验证：真实 binary 退出码、输出、stderr 与文件不变。
4. [x] 运行 workspace 基线、同步 Architecture/roadmap/Spec 并审查 staged diff → 验证：实际退出
   状态、文件规模、文档与实现一致。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | frontend engine、错误边界与 corpus 不变量 | `feat(frontend): format Koven sources conservatively (SPEC-0057)` |
| 2 | CLI stdout/check、真实进程验收与完成记录 | `feat(cli): expose Koven formatter command (SPEC-0057)` |

## 9. 未决问题

- 无。更强的 pretty-print、原地写入与 formatter 配置是明确非目标；若首版必须依赖其中任一项
  才能保持 token/newline 语义，应停止并修订 ADR，而不是扩大本 Spec。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-25 前置审计 | 通过 | SPEC-0006/0014 `done`；Lexer 公开完整 trivia/span，Parser 公开完整文件 diagnostics；ADR-0013 `accepted`；不依赖当前 Phase 4/5 门禁 |
| `cargo test -p lang-frontend formatting --lib` | 通过 | spacing、缩进、LF/CRLF、注释、字符串、Lexer/Parser 诊断、foreign source 与 128 层 delimiter |
| `cargo test -p lang-frontend --test formatting` | 通过 | 非空完整文件 pass corpus 与专用 fixture 的 token/comment/newline、重解析、幂等不变量 |
| `cargo check -p lang-cli --all-targets`、`cargo clippy -p lang-cli --all-targets -- -D warnings`、`cargo test -p lang-cli --all-targets` | 通过 | 真实 binary stdout、`--check` 0/1、错误 2、UTF-8、诊断、输入不变和 writer failure |
| `cargo fmt --all -- --check` | 通过 | 最终 workspace 基线 |
| `cargo check --workspace --all-targets` | 通过 | 最终 workspace 基线 |
| `cargo clippy --workspace --all-targets -- -D warnings` | 通过 | 最终 workspace 基线 |
| `cargo test --workspace --all-targets` | 通过 | 最终 workspace 全量测试，无跳过 |
| `cargo build -p lang-cli` | 通过 | 最终 `kovenc` 构建 |
| production 文件规模 | 通过 | frontend formatter 608 行，CLI format driver 119 行、main 115 行，均低于 1000 行软上限 |
