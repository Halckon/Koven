# Koven 语言设计规范 · 变更历史归档

> 本文档是 Koven 语言设计规范多文档结构的一部分，完整文档地图、版本治理规则与跨文件
> 索引见 [`00-index.md`](./00-index.md)。

本文档保存**完整的**逐版本变更记录表格（v0.3 起持续累积，当前含至 v0.23），供需要
追溯“某条规则从哪个版本、因为什么原因引入”的场景查阅。日常阅读不需要打开这份文档——
`00-index.md` 已经提供了一份一版本一行的精简摘要；只有当摘要不够、需要看到当版逐条
编号的完整表格与 🔴/🟡/🟢 严重度标注时，才需要来这里。

本文档只追加、不修改：每个版本一旦在其它文档里定稿，其完整变更记录表格就归档到本
文档。正文原地反映当前状态，不复制版本目录；历史状态通过这里的表格与 Git 提交共同
还原。v0.11、v0.12 的单文件候选快照已在 v0.14 启用后补回，仅用于合入验证；
v0.13 的纯结构拆分内容已合入 v0.14，没有独立快照。

## v0.12 变更记录

| # | 变更 | 类型 |
|---|---|---|
| 1 | 取消 `Own` 参数契约，并入 `Value`：具名函数参数、函数类型参数声明侧不再有独立的 `own` 标记，按值参数统一为“无标记”一种写法；`own` 仍是硬关键字（词法层不变），但 v0.12 起没有任何产生式接受它，状态与目前同样“无产生式使用”的 `unsafe` 一致 | 🔴 语义变更 |
| 2 | 调用点 `borrow` 标注从“已有 place 必须显式写”改为始终可选：编译器按 callee 已声明的参数契约自动判定是否借用；`borrow` 关键字与其声明侧语义不变，调用点仍可选择显式写出以加强可读性 | 🔴 语义变更 |
| 3 | 调用点 `inout` 标注保持不变，对可变 place 仍是必需项；第四部分第 9 节新增设计说明解释为何单独保留这一项、不随本次简化而放宽 | 🟢 明确不变 |
| 4 | 函数类型参数模式字母表从 `own`/`borrow`/`inout` 三项收窄为 `borrow`/`inout` 两项；函数类型身份对应从四种（无标记/own/borrow/inout）收窄为三种（无标记/borrow/inout） | 🔴 类型语法变更 |
| 5 | 重写第四部分第 9 节 SPEC-0012 的调用点兼容矩阵、`ParameterModeMarker` 封闭枚举（移除 `Own` 变体）、预声明 API 参数契约表（`Box`/`arrayOf`/`listOf`/`mutableListOf`/`MutableList.add`/`thread`/`Sender.send` 从 `Own` 改为无标记 `Value`；`Array`/`List` 运行时构造器的 `size` 同步改为无标记，`initializer` 仍是 `Borrow` 但调用点标注可选） | 🔴 语义与语法补全 |
| 6 | 第一部分第 18 节 Map/MutableMap 候选设计的调用示例同步更新（`map[borrow key]`、`put(own key, own value)`、`remove(borrow key)` 等改为标注可选或按新契约表达），候选未批准状态不变 | 🟡 候选设计同步 |
| 7 | 第三部分关键字表数量、分类均不变（仍是 42 个硬关键字，`own` 仍在其中）；新增说明其目前无产生式可用，避免被误读为遗漏 | 🟢 词法表说明补充 |
| 8 | 第五部分路线图：Phase 1 SPEC-0012 描述、Phase 2/3 验收标准中所有涉及 `own`/四契约的表述同步为新的三契约体系；SPEC-0012 编号、前置依赖与“未实现”状态不变，前置启用版本条件新增 v0.12 选项 | 🟡 路线图同步 |
| 9 | 第一部分第 4、5、8、9、17 节及第四部分相关示例代码中的 `own`/`borrow` 调用点标注同步更新为新语法（多为删除不再必需的标注） | 🟢 示例同步 |

## v0.11 历史变更记录

| # | 变更 | 类型 |
|---|---|---|
| 1 | 新增第 16 节：整数运算的溢出与除零语义（v1 采用 checked 语义，溢出与整数除零触发 `error()`；浮点除法遵循 IEEE 754），此前 guide 未定义此契约 | 🔴 语义补全 |
| 2 | 新增第 17 节：将 `Shareable` 简化 / 推迟到 v2，明确定义并结构化推导 v1 唯一需要的 `Transferable` 能力（规则对齐 `Copyable`；`Rc<T>` 恒不满足） | 🔴 语义补全 |
| 3 | 新增第 18 节：`Map` / `MutableMap` 所有权契约候选设计（`Hashable` key 等价关系、借用查询、`put`/`remove` 参数模式）——候选方向，需独立评审后才能进入实施 Spec | 🟡 候选设计 |
| 4 | 新增第 19 节：错误传播运算符 `?` 候选设计（`Result<T, E>` 的 postfix 语法糖，依赖 SPEC-0016 的 `return` 语义）——候选方向 | 🟡 候选设计 |
| 5 | 新增第 20 节：`Copyable` 显式 opt-out 的候选方向，留给 SPEC-0017 评估，不改变 v1 现行规则 | 🟢 候选方向 |
| 6 | 第 8 节顺序容器：自定义索引能力的延后说明由无版本承诺的“后续 guide”改为明确标注 v2 目标 | 🟡 版本澄清 |
| 7 | 第 9 节并发段落：同步第 17 节的 `Transferable` 简化，移除对未定义 `Shareable` 的悬空引用 | 🟡 一致性修复 |
| 8 | 第 10 节决策表新增“位运算符”行，明确标注延后到 v2（此前完全未标注版本去向，且未像其他 v1 裁剪特性一样给出去向） | 🟡 版本澄清 |
| 9 | 第一部分开篇新增“阅读说明”：明确 `if`/`when`/`super`/class-family 在示例代码中反映的是已确定的目标设计意图，不代表对应产生式已经过 SPEC-0016/0017 批准，避免设计意图与已锁定文法之间的时间差造成误解 | 🟡 一致性澄清 |
| 10 | 第五部分路线图：Phase 2/3/5 中“Map 不是本版实施项”的段落补充指向第 18 节候选设计的引用；Phase 1 补充 `?` 运算符对 SPEC-0016 的依赖说明 | 🟢 路线图补全 |

## v0.10 历史变更记录

| # | 变更 | 类型 |
|---|---|---|
| 1 | 为具名函数值参数定义唯一的 `Value` / `Own` / `Borrow` / `Inout` 契约语法，并保持默认参数值与 `vararg` 禁止 | 🔴 语义与语法补全 |
| 2 | 让函数类型的每个参数编码同一参数模式，并明确参数模式属于函数类型身份 | 🔴 类型语法补全 |
| 3 | 固定调用点模式的显式匹配矩阵、临时值省略规则和 `Copyable` 边界 | 🔴 所有权契约补全 |
| 4 | 固定命名实参匹配、位置 / 命名混排、重复名称与源码求值顺序 | 🔴 调用匹配补全 |
| 5 | 要求预声明 API 使用同一 callable contract，不允许按名称绕过匹配规则，并解除 SPEC-0012 的设计门禁 | 🟡 分阶段与治理边界补全 |

## v0.9 历史变更记录

| # | 变更 | 类型 |
|---|---|---|
| 1 | 封闭 lambda literal 的上下文判定、参数、body 值、AST、Span 与 owner 恢复 | 🔴 语义与语法补全 |
| 2 | 允许具名函数在无体或 block body 形态省略返回标注，并把省略语义固定为 `Unit`；表达式体仍必须显式标注 | 🔴 语义与语法变更 |
| 3 | 把调用实参改为 typed argument，定义命名与 `own` / `inout` / `borrow` 的唯一组合顺序 | 🟡 AST 契约补全 |
| 4 | 只新增 block / lambda body 内局部 `val` 解构，排除 `var`、`const`、占位、嵌套 pattern 与类型标注 | 🟡 分阶段边界补全 |
| 5 | 将四类能力拆为 SPEC-0010 至 SPEC-0013，并整体顺延所有尚未物化的后续候选编号 | 🟡 路线图治理 |

## v0.8 历史变更记录

| # | 变更 | 类型 |
|---|---|---|
| 1 | 定义独立 block 入口、block element 序列及不依赖分号或换行的结构边界 | 🔴 语义补全 |
| 2 | 封闭 SPEC-0009 的 block element 为局部 `val` / `var`、表达式与 nested block，不提前接受局部常量、局部函数、控制流或 class-family | 🔴 分阶段边界补全 |
| 3 | 明确 block 本身不产生值、没有尾表达式特例，表达式 element 结果按 `Unit` 语境丢弃 | 🔴 语义补全 |
| 4 | 固定具名函数无体、表达式体和 block body 三种互斥形态，并为 block body 定义 AST 引用与合成 `Span` | 🔴 语义补全 |
| 5 | 定义 block element 的局部恢复、owner delimiter、稳定诊断类别及单调前进约束 | 🟡 诊断边界补全 |
| 6 | 将 SPEC-0009 收窄为单一 block Goal；控制流和 class-family 延后至新 guide 与独立 Spec，SPEC-0011 只组合当时已实现节点 | 🟡 路线图重排 |

## v0.7 历史变更记录

| # | 变更 | 类型 |
|---|---|---|
| 1 | 定义仅消费一个 `val` / `var` / `const val` / `fun` 的独立声明入口，不把换行或 trivia 当作声明终止符 | 🔴 语义补全 |
| 2 | 固定简单变量声明的名称、可选类型标注和必需初始化式；常量声明使用固定的 `const val` 前缀 | 🔴 语义补全 |
| 3 | 固定具名函数的泛型参数、普通参数、显式返回类型及可选表达式体，并把 block body 明确延后至 SPEC-0009 | 🔴 语义补全 |
| 4 | 泛型参数仅支持可选的单一内联上界；不接受 trailing comma、型变、默认类型实参、多上界或 `where` | 🔴 语义补全 |
| 5 | 定义 trivia 不敏感、先无副作用试探再提交的调用点类型实参判定及 postfix 归属 | 🔴 歧义消除 |
| 6 | 定义声明、参数、泛型参数、调用点类型实参和 typed call 的合成 `Span` | 🟡 AST 契约补全 |
| 7 | 补齐独立声明的最小诊断与局部恢复，继续把完整文件和跨声明同步留给 SPEC-0011 | 🟡 诊断边界补全 |
| 8 | 明确 SPEC-0008 不接受可见性及其他声明修饰符、类成员上下文、解构、模式实参或 block body | 🟡 分阶段边界补全 |
| 9 | 将依赖 block / statement 载体的 SPEC-0010 明确置于 SPEC-0009 之后，SPEC-0011 只负责最终组合与跨声明恢复 | 🟡 路线图门禁补全 |
| 10 | 统一所有声明级 consume-to-current-level 恢复的 delimiter、字符串、插值与 Lexer terminal-owner 规则，并固定单调单遍复杂度 | 🟡 恢复边界补全 |
| 11 | 为继续构造的声明名与参数名定义 present / missing / error 三态 marker，禁止用虚构名称 token 或范围表达恢复 | 🟡 AST 契约补全 |

## v0.6 历史变更记录

| # | 变更 | 类型 |
|---|---|---|
| 1 | 给出可执行的 primary、postfix、prefix、binary 与 assignment 表达式语法，并明确独立表达式入口和插值 stop token | 🔴 语义补全 |
| 2 | 将 v1 中缀调用封闭为 `to`，并分别定义区间、成员关系、比较、相等组的不结合约束 | 🔴 语义补全 |
| 3 | 定义限定路径、递归泛型、单层可空与函数类型组成的无歧义最小 `type_ref` 语法；可空函数类型暂不表达 | 🔴 语义补全 |
| 4 | 明确基本调用仅接受位置实参，索引恰好接受一个表达式，并把命名 / 模式实参与 use-site 类型实参延后 | 🟡 分阶段边界补全 |
| 5 | 定义无 trivia 相邻的不支持运算符组合、Lexer 错误 token 的 parser 消费规则及最小局部恢复类别 | 🟡 诊断与恢复边界补全 |
| 6 | 定义各表达式 AST 节点的合成 `Span` 规则 | 🟡 AST 契约补全 |
| 7 | 将 Phase 1 拆为 SPEC-0007 至 SPEC-0011 的依赖顺序，并同步 Lexer 已完成事实 | 🟡 路线图同步 |
| 8 | 封闭 `Array<T>`、`List<T>`、`MutableList<T>` 的长度可变性、唯一所有权和单一连续缓冲区表示 | 🔴 标准库契约补全 |
| 9 | 明确顺序容器元素按具体类型内联，禁止逐元素自动 `Box`、small-buffer optimization 和运行时双表示 | 🔴 布局契约补全 |
| 10 | 将顺序容器索引定义为内建元素 place，不建立无法表达该语义的普通 `Indexable.get/set`，移除 `getOrNull`，并补齐不可复制元素的读取、借用、替换和析构规则 | 🔴 所有权语义补全 |
| 11 | 区分显式装箱、ABI 间接传递与可选分配消除，且不把优化结果提升为语言保证 | 🟡 实现边界补全 |
| 12 | v1 固定使用单类型实参 `Array<T>`，为未来编译期长度类型 `Array<T, N>` 保留扩展边界 | 🟡 未来兼容边界 |
| 13 | 要求内联值具有目标可表示的有限布局，并用目标相关 warning 管理大栈帧和大型隐式复制 | 🟡 布局与诊断补全 |
| 14 | 不在尚未定义通用 key 等价关系的前提下臆造 `Map` 所有权语义；保留 v0.5 表面契约并将可实施契约延后到新 guide | 🟡 分阶段边界补全 |

## v0.5 历史变更记录

| # | 变更 | 类型 |
|---|---|---|
| 1 | 补齐 UTF-8、ASCII 标识符、关键字边界和大小写敏感规则 | 🔴 语义补全 |
| 2 | 将已用于闭包和函数类型的 `move` 明确列为硬关键字 | 🔴 冲突修复 |
| 3 | 定义十进制整数 / 浮点、`Char`、单行 `String` 及 `${...}` 插值的最小 v1 词法 | 🔴 语义补全 |
| 4 | 定义 ASCII 空白、LF / CRLF 换行、裸 CR 诊断以及非嵌套行 / 块注释 | 🔴 语义补全 |
| 5 | 定义固定运算符 / 标点集、Phase 5 `@` 预留 token、最长匹配与相邻组合规则 | 🔴 语义补全 |
| 6 | 定义 token / trivia / EOF 的 `Span` 契约与词法错误恢复边界 | 🟡 实现边界补全 |
| 7 | 把 Phase 1 Lexer 验收收敛为可执行的正反例与稳定诊断 | 🟡 路线图同步 |
| 8 | 同步 Phase 0 已完成事实，不把尚未实现的 Lexer / Parser 写入工程骨架验收 | 🟡 已确认事实同步 |

## v0.4 历史变更记录

| # | 变更 | 类型 |
|---|---|---|
| 1 | 将 `value class` 的内联值语义与 `Copyable` 能力分离，允许值类型包含不可复制字段 | 🔴 语义修正 |
| 2 | `value class` 按字段递归、按实际泛型实参自动获得条件 `Copyable`；该预声明 marker trait 可作泛型上界，但 v1 不允许用户手动实现、覆盖或用同名声明冒充 | 🔴 语义补全 |
| 3 | `Pair<A, B>` 不再要求 `A`、`B` 可复制；含 `Sender` / `Receiver` 的 `Pair` 使用移动与消费式解构 | 🔴 冲突修复 |
| 4 | 解构右值只求值一次；不可复制聚合的解构作为一次原子所有权转移，不再按多次独立 `componentN()` 调用解释 | 🔴 语义修正 |
| 5 | 不可复制字段只可投影借用；禁止普通字段部分移动，消费式解构必须完整覆盖全部分量 | 🔴 语义补全 |
| 6 | 明确 `Copyable` 不允许复制 glue 或唯一析构义务；`Box<T>` 在 v1 只接受 `value class` | 🔴 契约补全 |
| 7 | Phase 2–5 的任务与验收同步覆盖条件复制、移动后使用和资源只析构一次 | 🟡 路线图同步 |
| 8 | 测试扩展名统一为 `.ko`，Phase 0 验收改为不依赖临时 parser 的工程骨架验收 | 🟡 已确认事实同步 |
| 9 | 修正示例中遗漏的显式 `Unit` 返回类型，使其符合既有函数签名规则 | 🟢 示例勘误 |

## v0.3 历史变更记录

对照 `agent-language-design-guide-audit.md` 的审计结果逐条修复，编号与审计报告一致：

| # | 变更 | 类型 |
|---|---|---|
| 1 | `thread()` 等跨线程 API 的函数类型参数改为 `move (...) -> T`，强制要求闭包不含借用捕获 | 🔴 修复 |
| 2 | `Indexable<K, V>` 与 `Map` 拆开，`Map`/`MutableMap` 改为独立接口，不复用非空 `get` 签名 | 🔴 修复 |
| 3 | `class Node` 示例去掉多余的 `Box<Node>` 包装，`Box<T>` 重新定位为“把 value class 显式装箱到堆上” | 🔴 修复 |
| 4 | 明确 `dyn`（trait object 动态分发）降级为 v2 特性 | 🔴 修复 |
| 5 | 运算符优先级表按 Kotlin 官方语法核实后重写，Elvis(`?:`) 优先级大幅上调 | 🔴 修复 |
| 6 | `value class` 字段可拷贝规则改为“所有字段类型需满足 `Copyable`”，不再区分 val/var | 🟡 修复 |
| 7 | 不再用“栈分配”描述 `value class`，改用“值语义/内联布局” | 🟡 修复 |
| 8 | `error` 从硬关键字表移除，改为标准库顶层函数 | 🟡 修复 |
| 9 | 补充 `super<Interface>.method()` 的语义（接口默认方法冲突消歧义） | 🟡 修复 |
| 10 | 补充解构声明的 `componentN()` 约定机制 | 🟡 修复 |
| 11 | 补充 `enum class` 变体内部共享方法的语法（`when (this)` 分派） | 🟡 修复 |
| 12 | 智能类型转换（smart cast）列为 Phase 2 显式交付项 | 🟡 修复 |
| 13 | 关键字表补回 `vararg` | 🟢 修复 |
| 14 | 明确 `!!` 保留，定义为 `?: error(...)` 的语法糖 | 🟢 修复 |
| 15 | `own`/`inout`/`borrow` 从通用运算符优先级表移除，改为调用实参位置的专属语法 | 🟢 修复 |
| 16 | Phase 1 验收范例扩充，覆盖 lambda、索引、Elvis 等 | 🟢 修复 |
| 17 | Phase 4 补充闭包环境捕获的 codegen 任务 | 🟢 修复 |
| 18 | Phase 3 补充 `Shareable`/`Transferable` 标记 trait 检查任务 | 🟢 修复 |
| 19 | 补充 `companion object` 作为类型级静态成员机制 | 🟢 修复 |
| 20 | 明确砍掉自定义属性 getter/setter 语法 | 🟢 修复 |
| 21 | Phase 4 调试信息验收标准补充 `lldb` | 🟢 修复 |
| 22 | 全文中英文标点混用问题统一修正 | 🟢 修复 |

---


## v0.13 结构调整说明

v0.13 没有独立的语义变更表格——它是纯粹的组织形式调整（单文件拆分为多文档结构），
不改变任何已定义语义。完整说明见 `00-index.md` 第 2 节“结构调整说明”，此处不重复。

## v0.14 变更记录

> v0.14 于 2026-08-20 由用户明确启用并取代 v0.9。v0.12、v0.13 内容合入 v0.14，不保留
> 独立文件快照；自本版起采用 `docs/guide/` 正文原地演进、本文档与 Git 历史共同追溯的
> 治理模型。

| # | 变更 | 类型 |
|---|---|---|
| 1 | 调用点 `Inout` 标注的拼写从关键字 `inout` 改为符号 `&`（如 `mutate(&x)`）；声明侧标注一个参数为 `Inout` 契约仍使用关键字 `inout`（`fun mutate(inout x: Point)`、`(inout T) -> R`），未受影响 | 🔴 语法变更 |
| 2 | 词法层新增固定符号 `&`：单独 `&` 现在合法，与既有 `&&` 通过最长匹配规则区分；此前裸 `&` 产生非法字符诊断。这是本次改动中唯一触及已实现 SPEC-0006 词法基线的部分，`own` 退役时特意回避了这一类改动，这次因为要新增 token 无法回避 | 🔴 词法变更（触及已实现内容） |
| 3 | Call argument 的 `argument_mode` 产生式不再复用声明侧 `explicit_parameter_mode`，改为独立产生式 `"borrow" \| "&"`；三处共享的是语义枚举 `ParameterModeMarker`，不再是完全相同的表面拼写 | 🔴 语法补全 |
| 4 | `ParameterModeMarker::Inout(Span)` 枚举变体不变，但用于 `CallArgument` 时 `Span` 覆盖符号 `&` token；用于 `ValueParameter` / `FunctionTypeParameter` 时仍覆盖关键字 `inout` token，两者不可混淆 | 🟡 AST 契约澄清 |
| 5 | 重写调用点兼容矩阵行标签（`inout operand` → `&operand`，“缺 `inout`” → “缺 `&`”）、Callable 参数契约段落、`05-grammar-calls-lambda.md` 设计说明 callout（新增 v0.14 小节，说明符号化理由与 Swift 先例） | 🔴 语义与语法补全 |
| 6 | 更新 `01-design-decisions.md`（容器索引 `mutate` 示例、`size` 属性限制、`getOrNull` 安全访问段落）、`03-grammar-core.md`（prefix 层级说明）中所有调用点 `inout` 示例为 `&` 语法 | 🟢 示例同步 |
| 7 | `06-roadmap.md` Lexer 验收标准补充 `&`/`&&` 最长匹配边界测试要求（`&x`/`&&x`/`& &x` 三态断言） | 🟡 验收标准补全 |
| 8 | `duplicate argument mode`（L0038）/ `invalid argument mode ordering`（L0037）诊断类别定义与恢复语义不变，仅更新示例拼写为新字母表（如 `f(borrow &x)`），并补充 `&&` 与两个独立 `&` token 的边界说明 | 🟢 示例同步 |

## 历史快照补归档说明

2026-08-20 补回
[`agent-language-design-guide-v0.11.md`](../agent-language-design-guide-v0.11.md) 与
[`agent-language-design-guide-v0.12.md`](../agent-language-design-guide-v0.12.md) 两份单文件候选快照，用于验证
v0.11→v0.12→v0.14 的合入过程。它们从未单独启用，不改变 v0.14 的现行真源地位；
v0.13 仍只是纯结构拆分，没有独立快照。

## v0.15 变更记录

> v0.15 于 2026-08-20 由用户明确启用并取代 v0.14，继续采用滚动正文、本文档与 Git
> 共同保留历史的治理模型。

| # | 变更 | 类型 |
|---|---|---|
| 1 | 定义允许空文件、按源码顺序组合既有简单声明的 `source_file`，不提前接纳 module/import、control-flow 或 class-family | 🔴 语法补全 |
| 2 | 声明不依赖换行或分号；最外层 `val`/`var`/`const`/`fun` 在 owner baseline 构成 soft declaration boundary，嵌套 owner 内同形关键字不构成边界 | 🔴 语法与恢复补全 |
| 3 | 文件级未知区域复用 L0017 并构造 `Item::Error`，Lexer poison 不重复分类；独立声明入口继续保留 EOF 与 L0013 契约 | 🟡 诊断边界补全 |
| 4 | 固定 `ParsedFile` 的有序根 Item API、跨声明级联抑制与单调 `O(n)` / `O(d)` 资源边界 | 🟡 AST 与验收契约补全 |

## v0.16 变更记录

> v0.16 于 2026-08-20 由用户明确启用并取代 v0.15，修正 v0.15 的顶层声明自分隔设计。

| # | 变更 | 类型 |
|---|---|---|
| 1 | 顶层声明之间必须出现实际 LF / CRLF 或 `;`；同一行多个声明必须显式写 `;`，最后一个声明后允许一个可选 `;` | 🔴 语法修正 |
| 2 | 把 `;` 加入固定符号表，但只允许用于完整文件的顶层声明分隔；block 与独立声明入口不接受它 | 🔴 词法与语法增量 |
| 3 | 顶层 starter 继续作为 owner-baseline 恢复边界；同行缺 `;` 时以 L0047 报错并保留后一声明，不把恢复成功误当作合法分隔 | 🟡 诊断与恢复补全 |
| 4 | 换行分隔按实际 LF / CRLF 判断，包含已终止 block comment 内的换行；space、tab 或无换行注释不分隔声明 | 🟡 trivia 边界补全 |

## v0.17 变更记录

> v0.17 于 2026-08-20 由用户明确启用并取代 v0.16，采用 Kotlin 风格的 package / import
> 源码组织，明确拒绝 Rust 风格模块导入语法。

| # | 变更 | 类型 |
|---|---|---|
| 1 | 42 个硬关键字中的 `module` 替换为 `package`；`module` 恢复为普通 Identifier，关键字总数不变 | 🔴 词法变更 |
| 2 | 每文件允许一个位于首部的可选 `package`，省略时属于默认 package；名称使用非空点分 Identifier segment | 🔴 语法补全 |
| 3 | 定义绝对 exact import、末尾 `.*` wildcard import 与 exact import 的 `as` alias；拒绝 `mod` / `use` / `::` / 花括号分组等 Rust 风格形式 | 🔴 语法补全 |
| 4 | 文件头之间及文件头到声明之间沿用换行 / `;` 分隔意图；为名称、位置、分隔与 wildcard alias 分配 L0048–L0054 | 🟡 诊断与恢复补全 |
| 5 | `ParsedFile` 内嵌保存 package 与有序 imports，普通 roots 保持只含 ItemId；文件映射与名称绑定延后到 SPEC-0025 + package ADR | 🟡 AST 与 Phase 边界补全 |

## v0.18 变更记录

> v0.18 于 2026-08-20 由用户明确启用并取代 v0.17，封闭 SPEC-0016 的 control-flow 与
> callable-local `return` 契约。

| # | 变更 | 类型 |
|---|---|---|
| 1 | 定义 `if` 的 statement/value context：完整 statement position 可省略 `else`，所有需要值的位置必须有 `else` | 🔴 语法与上下文补全 |
| 2 | 定义 subjectful / subjectless `when`、条件族、逗号分组、换行 / 分号 entry 分隔和 Phase 2 穷尽性边界 | 🔴 语法补全 |
| 3 | 定义 `while` / `for` / `loop`、单次求值迭代 source、名称 / 解构 binding 与 `_`，迭代协议绑定延后到 Phase 2/5 | 🔴 控制流补全 |
| 4 | lambda 成为独立 callable boundary；裸 `return` 退出最近 lambda 或具名函数，明确拒绝标签与非局部 lambda return | 🔴 jump 语义补全 |
| 5 | 定义 `super<Interface>.member`、control-flow AST / owner-aware 恢复边界及 L0055–L0065 | 🟡 AST、诊断与恢复补全 |

## v0.19 变更记录

> v0.19 于 2026-08-20 由用户同意前述完整错误值方案并要求继续实施，取代 v0.18，封闭
> `Result<T, E>` 与 postfix `?` 契约。

| # | 变更 | 类型 |
|---|---|---|
| 1 | 可恢复失败固定为显式 `Result<T, E>` 返回值；函数返回类型即失败契约，不增加 `throws` 等重复声明 | 🔴 错误模型定案 |
| 2 | 明确 v1 不提供 `throw` / `try` / `catch` / `finally`、可捕获异常层级或异常栈展开；`error()` 继续为不可捕获 abort | 🔴 能力边界定案 |
| 3 | postfix `?` 只传播 `Result` 的 `Err` 到最近 callable；lambda 内只退出该 lambda，不允许非局部返回 | 🔴 控制流与类型契约补全 |
| 4 | v1 要求错误类型 `E` 精确一致，不提供用户传播协议或隐式错误转换；Phase 1 只建 AST，Phase 2/3/4 分别负责类型、所有权与正常 return cleanup | 🟡 Phase 边界补全 |
| 5 | `?` 与 `?.` / `?:` 依赖 Lexer 最长匹配消歧，与其余 postfix 同为最高优先级、左结合、可连续；由 SPEC-0063 实施 | 🟡 语法与实施边界补全 |

## v0.20 变更记录

> v0.20 于 2026-08-20 由用户要求按已确认建议调整并启用，取代 v0.19，封闭
> SPEC-0017 的 class-family 契约，并为窄化接口委托保留独立增量边界。

| # | 变更 | 类型 |
|---|---|---|
| 1 | 正式定义 `value class` / `class` / `interface` / `enum class` / 具名 `object` 与 `companion object` 的 Kotlin 表面风格产生式、visibility/override、成员分隔、AST、Span、恢复及 L0066–L0077 | 🔴 语法与诊断补全 |
| 2 | 主构造器字段必须写 `val` / `var`；拒绝普通临时参数、默认值、二级构造器、`init`、body 存储字段、trailing comma 与未列修饰符；v1 明确不增加 `nocopy` | 🔴 class-family 边界定案 |
| 3 | enum 变体改用 Kotlin 风格逗号分隔；存在共享成员时以必需 `;` 分开变体区和成员区，关联数据继续使用 Koven ADT 语义 | 🔴 enum 表面语法定案 |
| 4 | companion 被定义为可选、无对象身份的关联命名空间，只容纳 `const val` 与无 receiver 关联函数；函数体可执行普通运行时代码，运行时静态状态、初始化 guard 与析构不进入 v1 | 🔴 companion 语义定案 |
| 5 | interface 可以通过 companion 暴露固定 `const val`，常量不继承、不 override；每个实现类型各自提供 associated constant 的契约继续延后 | 🟡 接口常量边界补全 |
| 6 | 具名 `object` 保留无运行时字段的 singleton 类型/值和实例函数；排除匿名内部类、object expression、SAM 自动转换、nested/local class-family，单回调继续使用函数类型/lambda | 🔴 object 与 lambda 边界定案 |
| 7 | 保留 ordinary class 的 `Interface by valField` 窄化接口实现委托：`by` 为上下文软词、具体类型静态分发、原样转发 callable 契约；排除任意表达式、`var` delegate、运行时代理和属性委托 | 🔴 组合能力定案 |
| 8 | 接口委托 Parser 拆为 SPEC-0064，SPEC-0017 先交付无 delegation clause 的 class-family；在前者完成前 `by` 必须定向拒绝，不得误判成普通 supertype 名称 | 🟡 分阶段边界补全 |

## v0.21 变更记录

> v0.21 于 2026-08-21 由用户明确要求启用并取代 v0.20，封闭 Phase 2 首个单文件名称解析
> 契约；不提前决定 package 到文件系统的映射。

| # | 变更 | 类型 |
|---|---|---|
| 1 | 定义类型 / 值双命名空间、源码有序 `ScopeId` / `SymbolId` 与显式外部 `NameEnvironment`，名称保持 ASCII 大小写敏感，Kotlin 命名风格不提升为语义错误 | 🔴 名称模型定案 |
| 2 | 顶层和 classifier member 在 body 前完整收集并允许前向引用；block local 在 initializer 后才可见，嵌套作用域允许遮蔽，较早引用只在没有外层可见名称时构成 use-before-local | 🔴 作用域语义定案 |
| 3 | 同作用域函数形成 overload set，其他同命名空间冲突使用 L0079；未解析名称与声明前 local 分别使用 L0080、L0081，并固定 primary / label 与确定性顺序 | 🔴 诊断契约补全 |
| 4 | SPEC-0018 只解析单文件 lexical name 与 receiver/type 首段；package/import、member/constructor/overload 选择、跨文件 visibility、类型与捕获所有权继续由后续 Spec 处理 | 🟡 Phase 边界补全 |

## v0.22 变更记录

> v0.22 于 2026-08-21 由用户明确要求启用并取代 v0.21，封闭最小 Kotlin 风格数值后缀和
> Phase 2 基础类型检查契约。

| # | 变更 | 类型 |
|---|---|---|
| 1 | 新增 `L`、`u` / `U`、`uL` / `UL`、`f` / `F` 精确数值后缀；`1f` 是 `Float`，小写 `l`、错序组合及其他后缀仍为 L0008 | 🔴 词法语义变更 |
| 2 | 无约束 signed 整数按范围默认 `Int`→`Long`，unsigned 按 `UInt`→`ULong`；无后缀实数固定 `Double`，`f` 固定 `Float`，不发生已定型变量隐式 widening | 🔴 类型语义定案 |
| 3 | 定义显式 TypeEnvironment、稳定 TypeId、单向 expected type、local/lambda 基础推导、函数返回与 `Nothing` bottom；分配 L0082–L0090 | 🔴 Phase 2 契约定案 |
| 4 | 数值后缀的 Lexer/AST 交接拆为 SPEC-0066，基础类型检查交给 SPEC-0019；nominal、generic、call/member、smart cast 与所有权继续后置 | 🟡 Phase 边界补全 |

## v0.23 候选变更记录（未启用）

> 本候选于 2026-08-21 为下一阶段审计起草。当前权威版本仍是 v0.22；只有用户明确启用
> v0.23 后，本表内容才成为现行语义并解除 SPEC-0020 门禁。

| # | 候选变更 | 类型 |
|---|---|---|
| 1 | 定义声明 symbol 派生的名义身份、invariant 泛型实例、类型参数身份与捕获规避替换；精确禁止 raw/default/型变/星投影 | 🔴 候选类型语义 |
| 2 | 把 v1 显式上界收窄为 `Any`、interface 或预声明 `Copyable`/`Transferable` 能力；interface bound 在 SPEC-0020 检查，能力满足性分别后置 | 🔴 候选泛型边界 |
| 3 | 明确无 `dyn` 时 interface 只允许作 bound/supertype/delegation target，不能直接作为 runtime value TypeRef | 🔴 候选表示边界 |
| 4 | 封闭 interface 继承图；overload shape 明确排除返回/bound/参数模式以避免可选 Borrow marker 歧义，完整 contract 仍精确比较；并定义 abstract/default、显式 override、缺实现与多默认冲突 | 🔴 候选接口语义 |
| 5 | 封闭 `Interface by valField` 的静态满足、转发签名、手写 override 优先与多来源冲突规则，不生成隐藏 AST | 🔴 候选委托语义 |
| 6 | 为 SPEC-0020 预分配 L0091–L0105，并明确 nominal/type-parameter/this deferred 的完成后交接；call/member/smart-cast/Copyable/companion 继续拆分 | 🟡 候选诊断与 Phase 边界 |

## v0.23 启用记录

> v0.23 于 2026-08-21 由用户明确启用并取代 v0.22；上述候选契约自此成为现行语义，
> SPEC-0020 的版本门禁解除并进入实施。候选起草记录按只追加治理保留，不回写历史措辞。
