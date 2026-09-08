# Koven v0.34 语言规范

> **性质**：规范性语言规范入口 · **状态**：current（v0.34） · **读取时机**：判断语言语义或实现授权时 · **唯一真源**：本索引导航的 15 个领域页面

<!-- current-guide: v0.34 -->

本规范定义 Koven v0.34。它不是教程，也不描述某项功能何时完成；当前实现事实见
[Architecture](../../../architecture/README.md)，未来设计见 [Proposals](../../../proposals/README.md)。

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

- v0.34 已明确取代 v0.33；本次文档重组不改变任何 v0.34 语义。
- v0.35–v0.37、Map 所有权和 v2 interface value 均未启用，不得作为实现依据。
- 旧版规范和重组前快照只在 [Archive](../../README.md) 中用于追溯。
