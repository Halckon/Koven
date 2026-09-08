# SPEC-0201：instance receiver mode Parser

| 字段 | 值 |
|---|---|
| 状态 | `done` |
| Goal ID | `KOV-P1-201` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.34 §34.1](../guides/v0.34-pre-restructure/01-design-decisions.md#341-声明语法与规范化-receiver) |
| 批准依据 | 2026-08-31 当前持续 Goal 明确要求在 v0.33 完成后显式启用 v0.34 并继续分阶段实施 |
| 前置 Spec | SPEC-0017、0064、0176 `done` |
| 前置 ADR | 无 |
| 阻塞项 | 无；v0.34 已启用，前置 Spec 均为 `done` |
| 影响范围 | `lang-frontend` Parser/AST、class-family grammar/fixtures、formatter/grammar bridges；Architecture/Roadmap |
| 语言语义变更 | 否；实施现行 guide 已生效的声明语法 |

## 1. Goal

完成后，instance member 可按固定顺序保存缺省/显式 Borrow、Inout 或 Value receiver marker，
非法位置、重复和乱序能稳定恢复，后续类型阶段无需从方法体猜测 receiver contract。

## 2. 范围与需求

- 把 `method_modifiers` 扩展为 `[visibility] [override] [borrow|inout|own] fun`；interface member
  使用 `[public] [receiver-mode] fun`，缺 marker 的语法事实与显式 Borrow 保持可区分。
- `DeclarationModifiers` 保存 receiver marker kind 与精确 Span；不得把 marker 塞入普通参数、
  函数名或通用 visibility 字段。
- receiver marker 只在 class/value/interface/enum/object 的 instance-function slot 解析；顶层、
  companion、constant、field、classifier 或其他声明位置继续确定性拒绝。
- duplicate、逆序、marker 后缺 `fun` 与 owner-aware recovery 复用 L0076/L0077 边界；错误 member
  不吞掉下一 member、enum delimiter、owner `}` 或下一顶层声明。
- 具名 object 的显式 Inout/Value 由 Phase 2 拒绝；Parser 只保存合法语法形态，不承担 owner
  runtime-state 判定。

## 3. 非目标

- 不规范化 receiver mode，不检查 override/interface/delegation contract。
- 不建立 `this` 类型、member call、loan/move/drop 或 SSA/LLVM。
- 不增加调用点 receiver marker、extension receiver、callable reference 或 safe-call 语法。

## 4. 验收标准

- [x] class/value/interface/enum 的缺省、Borrow、Inout、Value，以及 object 的三种显式 marker
  均能形成保留真实语法的 AST/Span；object Inout/Value 的语言拒绝留给 SPEC-0180。
- [x] visibility/override/receiver 固定顺序、重复/逆序、顶层/companion/非函数位置正反矩阵通过。
- [x] 缺 `fun`、缺名称/body 与相邻 member 恢复保持确定，L0076/L0077 primary 精确。
- [x] formatter、TextMate/Tree-sitter bridge 与双 Lexer/Parser 不变量不回归。
- [x] 受影响 `lang-frontend` 窄测试及 workspace Layer 2 静态门禁通过，Architecture/Roadmap 同步；
  除非风险升级，不机械运行完整 frontend 测试集。

## 5. 技术方案与边界

扩展既有 class-family modifier scanner 和 `DeclarationModifiers`，复用 `ParameterModeMarker` 的
三种 token identity 或增加职责等价的 receiver marker enum；不得把 member parser 复制成第二套
function parser。formatter 仍按源码 token 保守输出，不因规范化改写缺省/显式 Borrow。

## 6. 实施计划

1. [x] 扩展 grammar/AST 与 modifier scanner → 验证：Parser AST/Span 窄测。
2. [x] 完成非法位置、顺序和恢复矩阵 → 验证：class-family compile-fail fixtures。
3. [x] 同步 grammar bridges、Architecture/Spec 并运行 workspace 基线。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | receiver marker AST/Parser/恢复与文档 | `feat(frontend): parse receiver modes (SPEC-0201)` |

## 8. 未决问题

- 无；语义门禁由 v0.34 与 SPEC-0180 表达。

## 9. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-26 receiver 审计 | 通过 | Lexer 已有三个 marker；现行 member modifier 明确拒绝它们，需独立 Parser Goal |
| 2026-08-26 候选闭合审计 | 通过 | grammar §13.5 已同步产生式、合法 owner slot、固定顺序与恢复边界；v0.34 未启用，仍不授权实现 |
| 2026-08-31 重基与启用审计 | 通过 | v0.34 已重基到完整 v0.33 并显式启用；SPEC-0201 前置均完成，批准进入实施 |
| `cargo test -p lang-frontend --test parser_class_family --no-fail-fast` | 通过 | 19/19；五类 owner、Span、顺序/重复/逆序、非法位置、缺 `fun` 与后继恢复 |
| `cargo test -p lang-frontend --test parser_file --no-fail-fast` | 通过 | 27/27；file boundary 与后继 root 回归 |
| `cargo test -p lang-frontend --test parser_interface_delegation --no-fail-fast` | 通过 | 5/5；既有 interface/member 语法回归 |
| `cargo test -p lang-frontend --test tree_sitter_grammar --no-fail-fast` | 通过 | 4/4；生产 Lexer/Parser 与 grammar fixture 一致 |
| `XDG_CACHE_HOME=/tmp/koven-tree-sitter-cache npm test` | 通过 | Tree-sitter 8/8；缓存隔离在 `/tmp` |
| `npm test`（`editors/textmate`） | 通过 | 80/80 lexical contract |
| formatter receiver exact test | 通过 | marker token 保真且幂等 |
| `cargo test -p lang-frontend --lib --no-fail-fast` | 基线漂移 | 49/51；两项既有 block/lambda 线性访问阈值失败，receiver boundary 收窄后失败计数不变，不冒充全量通过 |
| `cargo test -p lang-frontend --test parser_declaration --no-fail-fast` | 基线漂移 | 21/22；旧断言仍把 v0.33 已合法的同行 `x {}` 当 trailing block，不属于本 Spec |
| `cargo fmt --all -- --check` | 通过 | final tree 无格式差异 |
| `cargo clippy -p lang-frontend --all-targets -- -D warnings` | 通过 | final tree，零 warning |
| `cargo check --workspace --all-targets` | 通过 | final tree；workspace 下游编译兼容 |
| `cargo build -p lang-cli` | 通过 | CLI build 门禁 |
| 独立复审 | 通过 | 首轮发现 `fun borrow` 逆序 P2；修复并补三模式矩阵后复审确认关闭，无剩余 P1/P2 |
