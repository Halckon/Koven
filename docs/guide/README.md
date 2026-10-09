# Koven v0.42 语言规范

> **性质**：规范性语言规范入口 · **状态**：current（v0.42） · **读取时机**：判断语言语义或实现授权时 · **唯一真源**：本索引导航的 15 个领域页面

<!-- current-guide: v0.42 -->

本规范定义 Koven v0.42。它不是教程，也不描述某项功能何时完成；当前实现事实见
[Architecture](../architecture/README.md)，未来设计见 [Proposals](../proposals/README.md)。
已分离的内部表示、算法与资源合同见 [Compiler Contracts](../compiler-specs/README.md)；
本 Guide 继续作为 Language Reference，语言语义、诊断与强制 Phase 权威不变。

实现查询请从[当前教程](../tutorials/README.md)、[Guide覆盖账本](../architecture/guide-conformance.md)
与[实施路线图](../development/roadmap.md)进入；它们不改变本Guide的规范承诺。
Spec归档或后继里程碑推进，只证明各自验收范围，不能视为所有规范功能已实现。

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

- 2026-10-07 12:24 UTC，用户明确批准普通借用返回、显式 `borrow val` 和受检查的不可逃逸作用域访问；
  v0.42 完整继承并取代 v0.41，仅启用[普通借用结果合同](10-ownership-borrowing-drop.md#普通借用结果与显式局部绑定)与
  [Map 确定借用及作用域访问](12-collections-destructuring.md#map--mutablemap-键值容器所有权规范)。
  [启用账本](../archive/migrations/v0.42-enablement.md)保全旧版完整 16 页及用户批准范围。
  规范启用不代表 SPEC-0288 已实现或验收完成。

- 2026-10-05 用户明确启用 v0.41，完整继承并取代 v0.40，仅澄清 ordinary Function expected
  上下文允许 move lambda literal；已定型 named Function 的 move 身份、参数 mode、捕获与
  所有权边界保持。唯一规则位于[类型与泛型](03-types-generics.md#expected-typelocal-与-lambda)，
  [迁移账本](../archive/migrations/v0.41-enablement.md)保全 v0.40 的完整 16 页。
  规范启用不代表 SPEC-0279 或所有运行时长度构造已经实现。

- 2026-10-01，本地整合的 v0.40 继承真实 v0.39，并启用用户批准的三项规则：调用点不写
  `borrow`、整数移位按位宽屏蔽位数、只读 `deinit` body 先于字段逆序析构。
  [一致性与实施边界](15-conformance-and-staging.md#v040-迁移与未完成边界)规定迁移和阶段验收；
  文档启用不表示对应编译器功能、PR CI 或 main 合并已完成。
- v0.39 已按用户批准先引入 `String.clone()`；其完整合同在本版本保留。所有 String literal
  仍是 MoveOnly、Transferable 的普通 String owner，const 资格保持；`Str` 与 `toString()` 继续延后。

- 2026-10-01 用户明确启用 v0.38：继承并取代 v0.37，实施批次 1 语法止血（块内换行敏感与
  分号语句分隔、修饰符上下文软关键字体系、具名中缀位运算与十六进制/二进制/下划线字面量、
  开放 `Box<enum>` 递归结构），并启动语义内核演进。
- 2026-09-19 用户明确启用 v0.37：完整继承并取代 v0.36，保留既有 grammar、nullable
  flow/extraction、所有权、Abort 与常量契约；新增借用式顺序迭代 provider。
- 首轮 native 明确包含 temporary source，及 owned named source、Borrow 参数；Inout/field
  source 的前端契约不代表首轮 native 已支持。
- 未获批准的条件借用结果、v2 interface value 及其他候选仍不得作为实现依据。
- 旧版规范和候选重基记录只在 [Archive](../archive/README.md) 中用于追溯。
