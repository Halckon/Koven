# SPEC-0066: 保留数值字面量后缀身份

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-066` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.22 数值字面量契约](../guide/02-lexical-spec.md#整数与浮点) |
| 批准依据 | 用户于 2026-08-21 明确启用 v0.22 并要求实施 `L` / `u` / `f` 后缀 |
| 前置 Spec | SPEC-0006、SPEC-0007 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` Lexer token、Parser literal AST、Lexer / Parser 测试与 fixture |
| 语言语义变更 | 否；实现已启用的 v0.22 契约 |

## 1. Goal

完成后，Lexer 能确定地区分无后缀、`L`、`u` / `U`、`uL` / `UL` 与 `f` / `F` 数值
字面量，Parser AST 保留其规范化身份，供 SPEC-0019 在不回读源码字符串的情况下定型。

## 2. 背景

SPEC-0006 的历史词法基线把所有字母数字后缀合并为 L0008；Parser 又只保留笼统的
`Integer` / `Float` 类别。v0.22 已允许 Kotlin 风格最小后缀集，因此必须先封闭词法与 AST
交接，不能让类型检查器按源码拼写重新实现第二套 scanner。

## 3. 范围与需求

- Lexer 接受 `L`、`u` / `U`、`uL` / `UL` 与 `f` / `F` 的精确、无空白后缀组合；
  `[0-9]+[fF]` 直接形成实数字面量。
- token 与 Parser literal AST 规范化保存后缀语义；大小写变体不产生不同类型身份。
- 保持 range `1..2` / `1..<2` 的最长边界；未知、错序或追加 identifier continuation 的
  后缀仍合并为一个 L0008 非法数字区域，并保留后续独立 token。
- `-` 继续是独立 token；本 Spec 不解析数值、检查范围或决定无后缀默认类型。
- 迁移受影响的 Parser 测试与真实 Lexer pass / fail fixture，不削弱 SPEC-0006 的覆盖。

## 4. 非目标

- 不增加十六进制、二进制、八进制、指数或数字分隔下划线。
- 不增加 `l`、`D` / `d`、`I` / `i`、Byte / Short 或 Rust 风格完整类型名后缀。
- 不做 contextual typing、默认 `Int` / `Double`、数值范围或 operator 类型检查；这些属于
  SPEC-0019。

## 5. 验收标准

- [x] Lexer 正例覆盖每种规范身份与大小写等价：`1`、`1L`、`1u`、`1U`、`1uL`、`1UL`、
      `1.0`、`1.0f`、`1.0F`、`1f`、`1F`。
- [x] Lexer 反例覆盖 `1l`、`1LU`、`1Ul`、`1ul`、`1.0L`、`1.0u` 和合法前缀后追加名称，
      均为一个精确 L0008 区域且恢复后续 token。
- [x] `1..2` / `1..<2`、`.5` / `1.`、负号与字符串插值边界不回归；lexeme 仍完整覆盖源码。
- [x] Parser AST 对六种规范化 literal 身份可观察，不回读源码文本且 source identity / Span
      保持不变。
- [x] Lexer / Parser 窄测试与 workspace fmt/check/Clippy/test、CLI build 全部通过。
- [x] Architecture、guide 路线图与本 Spec 验证记录同步为最终事实。

## 6. 技术方案与边界

- Lexer `TokenKind` 以小型 suffix enum 携带规范化身份；scanner 是唯一识别字符组合的位置。
- Parser syntax 使用自己的稳定 literal-kind enum，engine 只做一次显式映射，避免 public AST
  泄漏 scanner 内部状态或让类型检查器读取 Span 文本。
- 不新增依赖；未知后缀继续复用 L0008，不改变已发布诊断含义。

## 7. 实施计划

1. [x] 更新 token/scanner 与 Lexer 测试、fixture → 验证：`cargo test -p lang-frontend --test lexer`。
2. [x] 更新 Parser literal AST、映射与表达式回归 → 验证：Parser 窄测试。
3. [x] 同步 Architecture、路线图和验证记录 → 验证：文档链接与 diff 检查。
4. [x] 执行 workspace 基线并创建独立提交 → 验证：staged diff 只属于 SPEC-0066。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | guide 激活、数值 token/AST、测试、Architecture 与完成状态 | `feat(frontend): preserve numeric literal suffixes (SPEC-0066)` |

## 9. 未决问题

- 无。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo test -p lang-frontend --all-targets --locked --offline` | 通过 | 实施前基线 320 tests |
| `cargo test -p lang-frontend --test lexer --test parser_expression --test fixtures --locked --offline` | 通过 | 19 + 53 + 23 tests |
| `cargo fmt --all -- --check` | 通过 | 无格式差异 |
| `cargo check --workspace --all-targets --locked --offline` | 通过 | 全 workspace targets |
| `cargo clippy --workspace --all-targets --locked --offline -- -D warnings` | 通过 | 零 warning |
| `cargo test --workspace --all-targets --locked --offline` | 通过 | 328 tests，0 ignored / filtered |
| `cargo build -p lang-cli --locked --offline` | 通过 | `kovenc` build 成功 |
| Markdown 相对链接、`git diff --check` | 通过 | 本地目标存在；无空白错误 |
