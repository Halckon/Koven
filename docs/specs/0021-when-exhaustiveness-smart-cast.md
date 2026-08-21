# SPEC-0021: 检查 `when` 穷尽性与 smart cast

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P2-021` |
| 所属 Phase | Phase 2 |
| 语言规范 | [现行 v0.24 §24](../guide/01-design-decisions.md#24-when-穷尽性与-smart-castv024) |
| 前置 Spec | SPEC-0016、SPEC-0020 `done` |
| 阻塞项 | 无 |
| 影响范围 | name-resolution enum case identity、typed flow facts、when/type-test、L0106–L0114、Phase 2 fixture、Architecture |

## 1. Goal

在不引入一般 RTTI、member/call 选择或所有权检查的前提下，为 enum case 建立 type-test 身份，
让 `if`/`when` 的稳定引用获得可验证 smart cast，并确定性检查 Boolean、enum 与 nullable
有限域的 `when` 穷尽性、重复覆盖和分支类型。

## 2. 范围

- enum case 的值/类型双命名空间身份、内外部限定名称，以及 payload 候选引用；不把 case type
  暴露为普通签名类型，非法 TypeRef 位置使用 L0114。
- `is`/`!is` 的合法关系、Boolean 结果与 enum/nullable flow fact；`as`/`as?` 继续 deferred。
- `this`、参数、local val 与受限 local var 的稳定 key；赋值、capture、短路布尔和分支 join。
- subjectful/subjectless when 条件检查、else 顺序、重复覆盖、有限域穷尽性和 value/statement context。
- 显式 value/statement expression use、expected type、Nothing 与无 expected 分支 LUB；
  L0106–L0114 及真实 pass/fail fixture。

## 3. 非目标

- 不实现一般 member/constructor/overload/call 选择、`in` 协议、cast、sealed class、跨文件层级。
- 不建立任意 class/interface RTTI，不允许 interface 或泛型参数 runtime type-test。
- 不做完整 CFG/SSA、循环定点、所有权/借用、NLL 或跨 callable 副作用分析。
- 不新增 crate、依赖或 Parser 语法，不改变 v0.23 已实现诊断含义。

## 4. 验收标准

- [x] 用户明确启用 v0.24，Spec 从 `draft` 推进为 `in-progress`。
- [x] enum case 在值/类型命名空间共享稳定身份；短名/限定名确定解析；case type 非测试位置
      覆盖 L0114；payload 候选只在唯一 case fact 下可访问。
- [x] 合法/非法 `is`/`!is`、nullable test 与无 RTTI 边界覆盖 L0106，结果精确为 Boolean。
- [x] local val/parameter/this、可赋值 var kill、capture kill、`!`/`&&`/`||` 与分支交集有测试。
- [x] Boolean/enum/nullable 覆盖、negative test、逗号 alternative、poison、重复及 else 位置覆盖 L0107–L0111。
- [x] value/statement context 由 owner 显式传递，不用 expected presence 猜测；expected type、
      Nothing、nullable/enum/Any join 覆盖 L0111/L0112。
- [x] L0113 精确覆盖无事实与歧义 payload；已有 L0080 不抢占合法候选。
- [x] source/environment identity、重复运行、深条件/长 case 集预算、确定性顺序有测试。
- [x] frontend 与 workspace 基线、CLI build、Markdown 链接、diff 全通过，文档同步当前事实。

## 5. 模块边界与实施顺序

1. [x] 激活 v0.24，并把本 Spec 置为 `in-progress`。
2. [x] 建立 enum case identity 与 payload candidate name target。
3. [x] 建立 typed flow-key/fact/kill/join 模型和 type-test 检查。
4. [x] 实现 when coverage、context、branch join 与 L0106–L0114。
5. [x] 补窄测、Phase 2 fixture、预算/确定性测试。
6. [x] 同步 Architecture/guide/Spec，运行 workspace 基线。
7. [x] 创建独立提交 `feat(frontend): check when exhaustiveness (SPEC-0021)`。

实现保持 `type_checking/mod.rs` 门面稳定；flow/coverage 应按职责放入 checker 子模块，不把
已有 `checker.rs` 再扩成超限文件。名称阶段只保存候选身份，不进行类型或控制流判断。

## 6. 未决门禁

- 无；v0.24 已获得版本级明确启用，正文已封闭实施所需语义选择。

## 7. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo fmt --all -- --check` | 通过 | workspace 格式基线 |
| `cargo check --workspace --all-targets` | 通过 | 全 workspace / target 检查 |
| `cargo clippy --workspace --all-targets -- -D warnings` | 通过 | 零 warning |
| `cargo test --workspace --all-targets` | 通过 | 359 passed；0 failed / ignored / measured / filtered |
| `cargo build -p lang-cli` | 通过 | `kovenc` dev build |
| Markdown 相对链接、`git diff --check` | 通过 | 全仓 Markdown 本地目标存在；diff 无空白错误 |
