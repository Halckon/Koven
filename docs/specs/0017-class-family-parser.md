# SPEC-0017: 解析 class-family

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-017` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.20](../guide/00-index.md)：[class-family 与后续接口委托](../guide/04-grammar-declarations-blocks.md#13-spec-0017-class-family-与后续接口委托) |
| 批准依据 | 用户于 2026-08-20 明确要求按建议启用 v0.20；当前持续 Goal 的站立授权 |
| 前置 Spec | SPEC-0014、SPEC-0016、SPEC-0063 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` Parser / AST、诊断、fixture、Architecture |
| 语言语义变更 | 否；实现已经批准并启用的 v0.20 契约 |

## 1. Goal

完整文件和独立声明入口能够确定性解析 v0.20 的 `value class` / `class` / `interface` /
`enum class` / 具名 `object` / `companion object`，保存修饰符、构造器字段、supertype、变体与
成员的源码结构，并在错误后保留同 body 或后续顶层声明。

## 2. 范围与需求

- 扩充索引式 AST：classifier kind、visibility / override、主构造器字段、源码有序 supertype、
  enum variant、companion 和 member `ItemId`；缺失名称继续使用 `NameMarker` 三态。
- 顶层及成员声明支持固定顺序的 visibility，实例成员支持其后的 `override`；不新增 Lexer
  token，也不从 identifier 拼写猜测未授权修饰符。
- `value class` 强制非空字段构造器，普通 `class` 构造器可省略或为空；字段只接受带类型的
  `val` / `var`，不接受 marker、default、trailing comma 或未存储参数。
- supertype 复用 `TypeRef`；本 Spec 遇到 `by` 必须以 L0077 定向拒绝并恢复，委托 AST 留给
  SPEC-0064，不把它误吞成普通 identifier。
- class/value/interface/object body 按换行或 `;` 分隔成员；enum 变体按逗号分隔，有成员时
  必须写 `;`。成员复用既有 function / constant Item，不复制 callable parser。
- 分配 L0066–L0077，保持 owner-aware 单调恢复、确定诊断顺序和每个声明 `O(n)`。

## 3. 稳定诊断分配

| 错误码 | 含义 |
|---|---|
| L0066 | expected `class` keyword |
| L0067 | expected classifier name |
| L0068 | expected constructor field |
| L0069 | expected constructor separator |
| L0070 | expected supertype |
| L0071 | expected member |
| L0072 | expected member separator |
| L0073 | expected enum variant |
| L0074 | expected enum variant separator |
| L0075 | expected enum member delimiter |
| L0076 | invalid declaration modifier |
| L0077 | unsupported class-family form |

既有 `:`、TypeRef、`)`、`}`、函数参数和函数 body 错误继续复用对应已发布诊断，不改变其
含义；Lexer poison 不产生重复 Parser 根因。

## 4. 非目标

- 不实现 `Interface by field`；该增量属于 SPEC-0064。
- 不做名称重复、visibility、接口归属、`override`、函数有体要求、enum 穷尽性或常量求值。
- 不实现构造器语义、二级构造器、`init`、body 存储字段、嵌套/local/匿名 class-family、
  class implementation inheritance、属性委托、`nocopy` 或运行时 `dyn`。
- 不改变 block/lambda element、普通表达式、类型或所有权语义。

## 5. 验收标准

- [x] 正例覆盖五类顶层 classifier、value/普通 class 构造器、泛型、supertype、visibility、
      `override`、companion、object 常量/函数、interface 抽象/默认函数和源码有序 AST。
- [x] enum 正例覆盖无数据/带数据变体、逗号、成员前 `;`、共享函数与 companion。
- [x] AST 精确断言 kind、modifier / delimiter Span、name marker、child ID 顺序、父 Span 与
      source identity；缺失 child 不跨 trivia 扩张。
- [x] L0066–L0077 均有最小反例和精确 primary Span；恢复保留下一字段、supertype、variant、
      member 或顶层声明，不重复 Lexer 诊断。
- [x] `by`、匿名/local/nested object、constructor/init/普通 body field、trailing comma 与
      非法 modifier 被定向拒绝；SPEC-0064 边界保持清楚。
- [x] N 与 2N 长字段/member/variant 序列提供线性增长证据，不回扫完整声明。
- [x] 真实 pass / fail `.ko` fixture 被 harness 枚举执行。
- [x] frontend 窄测、workspace fmt/check/Clippy/test/build 全部通过，无 ignored / skipped。
- [x] Architecture、guide roadmap、Spec 索引与验证记录同步为实现后事实。
- [x] 独立提交成功。

## 6. 技术方案与边界

在现有 file / declaration dispatcher 前置解析 visibility，并以 keyword pair 提交 classifier。
body parser 持有 `{}` owner 和自身 member/variant starter 集；已有 function / constant parser
接收 modifier 与 owner stop，避免建立第二套 callable grammar。classifier 专用内嵌记录保存
field/supertype/variant，成员仍通过 typed `ItemId` 连接。

恢复只扫描当前 owner 的非空错误区：delimiter 或 lexical owner 未回到 baseline 时不识别
逗号、分号、换行或下一 starter。试探 classifier / enum variant / member 前缀时不分配 AST、
不发诊断、不移动正式游标。

## 7. 实施计划

1. [x] 扩充 AST、诊断目录与顶层 dispatch → 验证：diagnostic / AST 窄测
2. [x] 实现 header、构造器、supertype、body / enum parser 与恢复 → 验证：专用正反例矩阵
3. [x] 增加 fixture、复杂度证据并同步 Architecture → 验证：fixture 与 workspace 基线

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | Parser、AST、诊断、测试、fixture、Architecture 与完成状态 | `feat(frontend): parse class family (SPEC-0017)` |

## 9. 未决问题

- 无；接口委托已明确拆到 SPEC-0064，Phase 2/3 语义边界由 v0.20 封闭。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo test -p lang-frontend --test parser_class_family --locked --offline` | 通过 | 11 passed；五类 classifier、恢复、拒绝边界及 v0.20 聚合示例 |
| `cargo test --workspace --all-targets --locked --offline` | 通过 | 310 passed；frontend 303、CLI 6、lang-std 1；0 failed / ignored / measured / filtered out |
| `cargo fmt --all -- --check` | 通过 | 无格式差异 |
| `cargo check --workspace --all-targets --locked --offline` | 通过 | 五个 workspace member 全部成功 |
| `cargo clippy --workspace --all-targets --locked --offline -- -D warnings` | 通过 | 0 warnings |
| `cargo build -p lang-cli --locked --offline` | 通过 | CLI dev target 构建成功 |
| Markdown 相对链接检查；`git diff --check` | 通过 | `AGENTS.md` 与 `docs/**/*.md` 本地目标均存在；无空白错误 |
