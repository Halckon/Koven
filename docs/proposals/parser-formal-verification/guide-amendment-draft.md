# Guide 修订草案：调用尾 lambda、lambda return 与 move

> **性质**：待合入 Guide 的非规范修订草案 · **状态**：未启用 · **读取时机**：同步 F1 裁决或为后续实现拟定 Spec 时 · **唯一真源**：本页维护拟议 Guide 增量；现行 v0.47 正文仍是规范

维护者将 D1 视为旧条文未明确的边界，指定按 Kotlin 对齐；D2、D3 已确认。
不分配新 Guide 版本，不修改现行正文，不把下列预期作为已启用规则或 Parser golden。

## D1：尾 lambda 的附着点

**拟替换 Guide 07「尾 lambda 调用糖」的换行条款**：以值实参列表 `(...)` 或已严格识别的
类型实参列表 `<...>` 开始一次调用后，该后缀可接一个尾 lambda。尾 lambda 的 `{` 可以与该列表的
闭合符分处两行。`(...)` 已存在时，把 lambda 追加到同一次 Call 的实参末尾；只有 `<...>` 时，
将 lambda 作为该次 Call 的唯一值实参。两种情况下都不建立第二次 Call。

若没有进入上述调用后缀，只有直接同一行的 `{` 才开始普通无括号尾 lambda。因此在普通块元素中，
`f\n{}` 仍是名称表达式后接嵌套块，不从后续块倒推一个调用。不要越过分号、调用方 hard stop、
已结束的外层语法或某个其他 owner 的边界寻找 `{`。

**同步点**：Guide 03 的提交条件、Guide 04 的 `typed_call_suffix`、Guide 06 的 block element 分隔、
Guide 07 的 lambda literal 概述都应指向同一规则。03/04 应允许严格 `<...>` 后直接接这个尾 lambda，
不再对 `>` 到 `{` 添加同行门控。失败仍从 `<` 零状态回退。不顺带移植 Kotlin 的标签、注解、
多尾 lambda、类型投影或独立类型实参形式。本次对齐只涉及尾 lambda；`>` 到圆括号的既有 Koven
提交规则不是这次修订对象。换行示例中的 `\n` 表示实际换行，不是源码中的转义字符。

### Kotlin 对照实跑门槛

固定 [Kotlin/JVM 2.2.21](https://github.com/JetBrains/kotlin/releases/tag/v2.2.21)，
同一 `main` block 内用 `plain` 和 `typed<Int>` 调用函数字面量。被调用函数先输出 `call:`，
实际尾 lambda 输出 `body`，默认实参输出 `default`；只有 `call:body` 才算附着，而非仅编译成功。
编译器和临时源码均在仓库外，未增加项目依赖。

| 调用形状 | Kotlin 编译／运行结果 | Koven 草案验收 |
|---|---|---|
| `plain { … }`、`typed<Int> { … }` | 均为 `call:body` | 同行保持一个 Call |
| `plain\n{ … }` | 编译拒绝：名称缺调用 | block 产物是名称表达式及 nested block；Phase 2 名称是否合法不由本例判定 |
| `typed<Int>\n{ … }` | LF、CRLF 均为 `call:body` | 一个 typed Call，保留类型实参与 lambda Span |
| `typed<Int> /*\n*/ { … }` | `call:body` | 含换行注释不阻断已开始的后缀 |
| `plain()\n{ … }`、`typed<Int>()\n{ … }` | 均为 `call:body` | 已有 `(...)` 时继续附着，不拆成两个元素 |
| `typed<Int>`、`typed<Int>\n({ … })` | 均编译拒绝 | 对照项，不用 Kotlin 的拒绝反推 Koven 的比较回退或跨行圆括号规则 |

补 Koven 见证时还要覆盖 `f();\n{}`（不附着）、非法/未闭合 `<...>`（不提交试探产物）和调用方
边界（不吞下后继根或实参）。Kotlin 实跑只核实选定用例的附着，不能证明两套文法一致。

## D2：`return` 的局部目标

**拟替换 Guide 07「Lambda literal」的 L0032 清单**：从 unsupported introducer 中删除 `return`。
合法 body element 可包含无标签 `return`。当最近 callable 是 lambda 时，退出该 lambda；穿过普通
嵌套 block 不改变目标，不得越过 lambda 返回外层函数。若内层另有具名 callable，则仍依 Guide 06
选择最近 callable。合法性、返回类型与目标判定留给 Phase 2；Phase 1 只保留 jump 与可选操作数。
不放宽 `return@label`，不借此变更其他 L0032 条目。验收覆盖裸/有值、嵌套 lambda、普通 block、
换行后的独立 element 和非法标签；Phase 2 单独核对目标。

## D3：表达式位置的 `move`

**拟替换 Guide 07「Lambda literal」的 expected-expression 条款**：在等待 primary 时，
`move` 后的下一个非 trivia token 为 `{` 才提交 move lambda。前缀与 `{` 之间允许 trivia，
包括换行；这不是调用尾 lambda 的连接规则。否则 `move` 继续是普通 Identifier，后续不合法的
token 由所在入口诊断，不专门为缺 `{` 发 expected-expression。普通名称、调用和成员规则
回链 Guide 01；函数类型中的 `move` 由 Guide 03 独立规定，不变。

## 后续实施边界

D1 的维护者裁决解释了旧条文的意图，但草案与当前 Koven 的可观测换行行为不同。后续要修改
现行规则和生产 Parser 时，必须显式启用规范再开实现切片。现在可继续 F3a 的工具试验及不依赖
D1–D3 的 grammar 切片；不写受上述草案约束的 CI 门禁、Reader 规则或 golden。
