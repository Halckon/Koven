# Koven v0.39 语言规范

> **性质**：规范性语言规范入口 · **状态**：current（v0.39） · **读取时机**：判断语言语义或实现授权时 · **唯一真源**：本索引导航的 15 个领域页面

<!-- current-guide: v0.39 -->

本目录保留 SPEC-0235 本地候选的历史版本号 v0.39，尚未发布或整合至 main。它不是教程；当前实现事实见
[Architecture](../architecture/README.md)，未来设计见 [Proposals](../proposals/README.md)。

规则正文优先于示例。章节可以独立读取；遇到相邻概念时沿链接进入对应领域，不要顺序加载整套规范。

## 按任务读取

| 领域 | 页面 |
|---|---|
| Token、字面量、注释、词法恢复 | [词法](01-lexical.md) |
| 文件、作用域、package/import、跨文件名称 | [名称、文件与 Package](02-names-files-packages.md) |
| TypeRef、基础/名义类型、泛型、类型检查 | [类型与泛型](03-types-generics.md) |
| primary/postfix、优先级、运算符 | [表达式与运算符](04-expressions-operators.md) |
| 声明、函数签名、参数、返回类型 | [声明与 Callable](05-declarations-callables.md) |
| block、if、when、loop、jump | [Block 与控制流](06-blocks-control-flow.md) |
| 调用匹配、lambda、capture、overload trial | [调用、Lambda 与 Closure](07-calls-lambdas-closures.md) |
| class/value/interface/enum/object、成员与 receiver | [Class Family 与成员](08-class-family-members.md) |
| nullable、Nothing、Result、error、postfix `?` | [空安全与错误值](09-nullability-errors.md) |
| place、loan、move、drop、Transferable | [所有权、借用与析构](10-ownership-borrowing-drop.md) |
| Copyable、Box、布局、构造、结构移动 | [Copyable、布局与构造](11-copyability-layout-construction.md) |
| Array/List/MutableList、索引和解构 | [集合、索引与解构](12-collections-destructuring.md) |
| main/project、String、Rc、IO、并发和标准库 | [程序、Runtime 与标准库](13-program-runtime-standard-library.md) |
| 从语法形式定位唯一规则 | [语法索引](14-syntax-index.md) |
| 权威边界、Phase、门禁和明确非目标 | [一致性与实施边界](15-conformance-and-staging.md) |

## 版本与边界

- 最新决定优先实施 `String.clone()`，延后 Str/toString 规范切换；普通字面量仍是 MoveOnly
  String。本候选保留另外三项批准：调用处取消 `borrow`、移位按位宽屏蔽、`deinit` body
  先于字段逆序析构且 `this` 只读不可消费。
- 集成顺序为独立 clone 切片先形成 v0.39，再将本切片重基、升为 v0.40，并保存当时真实
  v0.39 快照；当前 v0.38 归档只证明原始 main 来源，不能冒充尚未形成的 v0.39。
  [一致性与实施边界](15-conformance-and-staging.md#v039-迁移与未完成边界)记录本地候选与实现范围。

- 2026-10-01 用户明确启用 v0.38：继承并取代 v0.37，实施批次 1 语法止血（块内换行敏感与
  分号语句分隔、修饰符上下文软关键字体系、具名中缀位运算与十六进制/二进制/下划线字面量、
  开放 `Box<enum>` 递归结构），并启动语义内核演进。
- 2026-09-19 用户明确启用 v0.37：完整继承并取代 v0.36，保留既有 grammar、nullable
  flow/extraction、所有权、Abort 与常量契约；新增借用式顺序迭代 provider。
- 首轮 native 明确包含 temporary source，及 owned named source、Borrow 参数；Inout/field
  source 的前端契约不代表首轮 native 已支持。
- Map 所有权、v2 interface value 及其他未启用候选仍不得作为实现依据。
- 旧版规范和候选重基记录只在 [Archive](../archive/README.md) 中用于追溯。
