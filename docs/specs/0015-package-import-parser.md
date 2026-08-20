# SPEC-0015: 解析 package 与 Kotlin 风格 import 文件头

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-015` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.17](../guide/00-index.md)：[词法关键字](../guide/02-lexical-spec.md#1-硬关键字42-个按用途分类不可作为标识符)、[文件头语法](../guide/04-grammar-declarations-blocks.md#11-spec-0015-package-与-kotlin-风格-import-文件头) |
| 批准依据 | 用户于 2026-08-20 明确启用 v0.17 取代 v0.16；当前持续 Goal 的站立授权 |
| 前置 Spec | SPEC-0014、SPEC-0062 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无；后续 package 映射 ADR 属于 SPEC-0025 门禁 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` Lexer、完整文件 Parser / AST、诊断、fixture、Architecture |
| 语言语义变更 | 否；实现已经批准并启用的 v0.17 契约 |

## 1. Goal

完整文件入口解析可选 `package` 与源码有序的 Kotlin 风格 exact / wildcard / alias imports，
保留精确 Span、顺序和 owner-aware 恢复，同时不提前实施名称解析或文件系统映射。

## 2. 范围与需求

- Lexer 以 `Keyword::Package` 替换 `Keyword::Module`；总数保持 42，`module` 恢复为 Identifier。
- `ParsedFile` 内嵌公开只读 package / imports 文件头节点；普通 roots 仍只含 ItemId。
- 解析可选首部 package、exact import、末尾 wildcard import、exact import alias。
- 文件头构造之间及到首个声明之间沿用实际换行 / `;` 分隔；EOF 不强制尾分隔。
- 实现 L0048–L0054、错位 directive、缺失 segment / alias、wildcard alias 与 owner-aware 恢复。
- 保持单调文件游标和 O(n) 文件 dispatch，不新增依赖或 AST table。

## 3. 非目标

- 不实现 source root / 文件映射、package identity、名称绑定、可见性、重复导入或 wildcard 展开。
- 不接受 `mod` / `use` / `::` / 花括号分组或相对 import。
- 不改 block、expression、独立声明入口或已经发布的 L0001–L0047 含义。
- 不实现 control-flow、class-family、类型或所有权检查。

## 4. 验收标准

- [x] 42 个硬关键字精确包含 `package` 而不含 `module`，前后缀边界与完整覆盖通过。
- [x] 缺省 / 点分 package、exact / wildcard / alias imports 及其与声明组合 compile-pass。
- [x] 文件头节点、segment、wildcard、alias、roots 顺序和精确 Span 可由公共 API 观察。
- [x] 重复 / 错位 package、声明后 import、缺名称 / target / alias、wildcard alias、同行缺分隔 compile-fail，并断言 L0048–L0054 与主 Span。
- [x] nested delimiter / string / interpolation 中同形 token 不提升为文件头；恢复保留下一合法 directive 或声明且不重复 Lexer 根因。
- [x] `mod` / `use` / `module` 仅按普通标识符进入既有非法顶层恢复；`::` / 分组 import 被明确拒绝。
- [x] 长 import 序列保持源码顺序、单调前进和线性实现证据。
- [x] 受影响窄测及 workspace fmt/check/Clippy/test/build 全部通过，无 ignored / skipped / filtered。
- [x] Architecture、guide/roadmap、Spec 索引、fixture 守卫和验证记录同步为实现后事实。
- [x] 独立提交成功。

## 5. 技术方案与边界

文件头使用内嵌 `PackageDirective`、`ImportDirective`、`QualifiedNameSegment` 与 `ImportAlias`
值结构，直接由 `ParsedFile` 持有，避免把非声明结构伪装为 Item 或增加 arena。完整文件循环在
进入普通声明序列前解析合法 header，并在后续文件级恢复中识别错位 directive；分隔检查复用
现有 raw trivia / semicolon 机制。路径 segment 只保存 Span，源码拼写继续由 SourceMap 回查。

## 6. 实施计划

1. [x] 替换 Lexer 关键字并扩充诊断目录 → 验证：Lexer / diagnostic 窄测
2. [x] 增加文件头 AST API 与 Parser / 恢复 → 验证：parser_file 专用正反例
3. [x] 增加 fixtures、同步 Architecture 与验收记录 → 验证：fixture 与 workspace 基线

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | Lexer、Parser、测试、fixture、Architecture 与完成状态 | `feat(frontend): parse package imports (SPEC-0015)` |

## 8. 未决问题

- 无；文件映射和名称绑定已明确延后，不阻塞本 Spec。

## 9. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo test -p lang-frontend --test parser_file --test lexer --test diagnostic_model --locked --offline` | 通过 | parser_file 27、lexer 19、diagnostic 9；0 failed / ignored / measured / filtered |
| `cargo test -p lang-frontend --test fixtures checked_in_file_suites_execute_multiple_roots_and_cross_declaration_recovery --locked --offline` | 通过 | 1 个定向 harness 测试；其余 22 个被命令过滤，随后由全量基线执行 |
| `cargo fmt --all -- --check` | 通过 | 无格式差异 |
| `cargo check --workspace --all-targets --locked --offline` | 通过 | workspace 全 target |
| `cargo clippy --workspace --all-targets --locked --offline -- -D warnings` | 通过 | 0 warnings |
| `cargo test --workspace --all-targets --locked --offline` | 通过 | 合计 281 passed；0 failed / ignored / measured / filtered |
| `cargo build -p lang-cli --locked --offline` | 通过 | dev build 成功 |
| `cargo tree -p lang-frontend --edges all --locked --offline` | 通过 | 仅 `lang-frontend` 自身；未新增依赖 |
| 修改文档 Markdown 相对链接检查 | 通过 | 本 Spec 范围内文档目标均存在 |
| `git diff --check` | 通过 | 无空白错误 |
