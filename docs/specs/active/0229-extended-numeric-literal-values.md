# SPEC-0229：扩展数值字面量值的端到端闭合

> **性质**：实施 Spec · **状态**：in-progress · **读取时机**：实施或评审扩展数值字面量时 · **唯一真源**：本 Spec

| 字段 | 值 |
|---|---|
| 状态 | `in-progress` |
| Goal ID | `KOV-P4-229` |
| 所属 Phase | Phase 2 / Phase 3 / Phase 4 |
| 语言规范 | [v0.38 数值字面量](../../guide/01-lexical.md#整数与浮点)、[类型规则](../../guide/03-types-generics.md) |
| 批准依据 | 2026-10-01 用户要求继续演进计划未完成项，分阶段实施、验证和提交；本项属于已启用批次 1 |
| 前置 Spec | 无新增前置；现有 scalar / constant / native 链可复用 |
| 前置 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | frontend 字面量定型/常量求值/元素索引 identity、codegen 单文件与 unit 整数 lowering、定向测试 |
| 语言语义变更 | 否；闭合已启用的 radix 与数字分隔下划线 |

## 1. Goal

Lexer 已接受的十六进制、二进制和数字内下划线在单文件与 compilation-unit 入口获得一致的
精确值、类型和范围诊断；整数贯通常量求值、SSA、LLVM 与宿主 native 执行。

## 2. 背景与范围

当前 Lexer 已识别 v0.38 拼写，但五处整数消费者仍直接用十进制 Rust parse；两处浮点
消费者也直接解析含下划线源码，导致合法字面量误报 L0090 或 lowering InvalidLiteral。
统一前端字面量数值解码，后端复用该接口，不在 LLVM 层重推语义。保留已有后缀、默认类型、
expected type、有符号负边界与溢出规则。

审查另发现两条 ownership 路径用十进制 parse 识别元素索引，导致新拼写降为 Unknown，
不能证明原本不重叠的两个元素。两处也复用统一解码，不改变既有 place/alias 规则。

## 3. 非目标

不扩展浮点 native 支持、具名位运算、常量表达式集合或语句/软关键字语义；不修复无关的
parser_call_argument 三个与 multifile_type_checking 五个既有失败。

## 4. 验收与实施

1. [x] 新建失败测试，证明合法 radix/underscore 误报 L0090。
2. [x] 统一整数解码及浮点分隔符规范化，单文件/unit 类型与常量结果一致。
3. [x] 原始 Span、expected-type label 与非法/溢出诊断保留；索引 Known identity 正反例通过。
4. [x] 单文件/unit 的 runtime 与 constant native 实际链接运行。
5. [x] fmt、严格 clippy、跨 crate check、受影响 suites 与文档门禁通过；更新事实与账本。
6. [ ] 分支发布后的远端 CI 验证及 Spec 归档；当前 Rust CI jobs 为 macOS，本机证据为 Linux。

## 5. 提交计划

| 顺序 | 提交边界 | 提交信息 |
|---|---|---|
| 1 | 数值解码、两套前端与后端接线、测试和账本 | `fix(literals): lower radix and separator values (SPEC-0229)` |

## 6. 验证记录

| 验收项 / 命令 | 实际结果 | 限制 |
|---|---|---|
| `cargo test -p lang-frontend --test numeric_literals`（实施前） | 1 passed / 2 failed；`0x7F` 常量和 `0x2A` 普通值误报 L0090 | 失败证据已建立 |
| 实施前两个旧套件复核 | parser_call_argument 25 passed / 3 failed；multifile_type_checking 99 passed / 5 failed | 全部原样保留，不属于本 Spec |
| `cargo test -p lang-frontend --test numeric_literals extended_indices`（ownership 接线前） | 0 passed / 1 failed；`list[0x0]` 与 `list[0b1]` 误报 L0135 | 独立审查发现漏接线，补红测后修复 |
| `cargo test -p lang-codegen --lib numeric_literal`（临时恢复原有两个 SSA 十进制解码器） | 2 passed / 2 failed；两条 runtime 路径报 InvalidLiteral | red 验证后恢复修复文件；常量路径已由 frontend 解码修复 |
| 同一 native 命令（最终代码） | 4 passed / 0 failed / 0 ignored；每条 25 组整数对照，实际 object/link/run | Linux x86_64 + glibc |
| `cargo test -p lang-frontend --lib literal_value` | 1 passed / 177 filtered / 0 ignored | 公共解码器后缀、非法拼写及 u128 checked 边界 |
| frontend 定向 integration 命令（下列完整目标集合） | 246 passed / 0 failed / 0 ignored | 不是 frontend 全量 |
| `cargo test -p lang-codegen --lib` | 482 passed / 0 failed / 0 ignored | 公共整数解码影响两套 lowering，扩大至完整 codegen lib |
| `cargo fmt --all -- --check` | 通过 | 最终 Rust 代码 |
| `cargo check --workspace --all-targets` | 通过 | 新跨 crate 解码接口与所有直接消费者 |
| `cargo clippy -p lang-frontend -p lang-codegen --all-targets -- -D warnings` | 通过 | 未放宽 lint |
| `python3 scripts/check_docs.py` | 387 Markdown / 0 errors | 含 DAG inventory；未放宽页面预算 |
| `python3 -m unittest discover -s scripts/tests -v` | 21 passed | inventory 修改对应检查器回归 |
| `git diff --check` | 通过 | 无 whitespace 错误 |
| macOS / PR CI | 未运行 | 当前只有 Linux 宿主；发布后必须单独核验 |

frontend 定向命令：

```bash
cargo test -p lang-frontend --test numeric_literals --test type_checking \
  --test type_constants --test multifile_constant_facts \
  --test multifile_constant_qualification --test multifile_constant_dependencies \
  --test multifile_constant_selection --test lexer --test ownership_containers \
  --test multifile_ownership_checking --no-fail-fast
```

对应测试数依次为 8、82、24、6、14、4、5、20、12、71，总计 246。
工具链为 Rust 1.96.0、LLVM/Clang 21.1.8，系统 C driver 为 Debian cc 14.2.0。

## 7. 未决问题

无。本 Spec 仅实现已确定的字面量数值，不替其他计划项作语义决定。
