# Koven 语言设计规范 · 总纲与索引

Koven 是一门编译型语言：语法尽量贴近 Kotlin 命名与语法习惯，内存模型是 Rust 式简化
所有权/借用，编译器用 Rust 实现，LLVM 后端。本文档集是面向负责实现编译器的 AI agent
的执行契约（agent-facing language design guide），不是面向语言使用者的教程——教程见
仓库中的 [`koven-language-tour.md`](../koven-language-tour.md)。

**这是整个文档集的入口。** 除非你已经知道自己要找哪一份文件，否则从这里开始：下面先
说版本治理规则，再给文档地图、精简版本历史和多张检索表。

---

## 1. 版本与状态

- **当前唯一权威版本是本文档集的 v0.17**，已于 2026-08-20 由用户明确启用并取代 v0.16；
  v0.14 此前已取代 [`agent-language-design-guide-v0.9.md`](../agent-language-design-guide-v0.9.md)。v0.12 及更早
  单文件 guide 只作为历史材料，不参与现行语义优先级。
- **当前文档集版本是 v0.17**：v0.10 引入统一的 callable 参数契约，v0.11 补齐
  整数溢出/`Transferable`/Map 候选设计/`?` 候选设计，v0.12 取消了独立的 `Own` 契约、
  把 `Borrow` 的调用点标注改为可选，v0.13 是纯结构拆分（不涉及语义），v0.14 把
  `Inout` 的调用点标注从关键字 `inout` 改写为符号 `&`；v0.15 封闭完整文件与跨声明恢复
  契约；v0.16 把顶层声明分隔修正为换行或分号；v0.17 以 `package` 取代 `module`，并
  封闭 Kotlin 风格的绝对 `import` 文件头语法。完整逐版本
  记录见下文
  “精简版本历史”与 [`07-changelog-archive.md`](./07-changelog-archive.md) 的完整表格。
- **v0.12、v0.13 已合入 v0.14**。v0.11、v0.12 的单文件候选快照已在 v0.14 启用后
  补回，仅用于核对合入过程；v0.13 没有独立快照。v0.13 是本文档集唯一一次不携带
  任何语义内容的版本号，只把 v0.12 单文件工作稿按内容边界拆成本索引 + 6 份正文 + 1 份
  变更归档，因此没有进入语义变更记录表格，单独在下方“结构调整说明”里交代。除这一版
  外，版本号是单一递增序列，不再区分“语义
  版本”和“结构版本”两条轴——每份正文文档顶部标注的是它自己内容最近一次改动所在的
  版本；本索引聚合记录整个文档集当前启用的 v0.17 状态。
- [`01-design-decisions.md`](./01-design-decisions.md) 第 16、17 节是 v0.14 启用后生效的
  规范规则；第 18–20 节明确标注为候选设计或候选方向，在独立完成设计评审、补充到对应
  实施 Spec 之前，不得被 Phase 2/3/5 实现直接引用为已批准契约。
- **文档治理规则（原第六部分，现收纳于此统一声明）**：`docs/guide/` 正文原地演进，
  [`07-changelog-archive.md`](./07-changelog-archive.md) 与 Git 历史共同保存版本追溯。每次
  文档集版本变更都必须在变更记录里补一条，保持可追溯；后续
  版本继续遵循。跨文件的设计
  决策变更（例如所有权标注简化、`Inout` 调用点符号化）需要在改动波及的**每一份**文档
  里同步，不能只改一处就当作完成；若改动触及已实现的词法/语法产生式（例如 v0.14 新增
  固定符号 `&`），必须在变更记录里明确标注，不能归为低风险改动。

## 2. 结构调整说明（v0.13，本次拆分）

原 v0.12 单文件工作稿（2700 行，约 205KB，内容现已合入 v0.14）按内容性质与预期
变化频率拆分为 6 份正文文档 + 1 份变更归档，并增加本索引。拆分只做了两件事：搬运内容、
把“第 X 部分第 Y 节”式的引用改写成跨文件引用（同文件内的引用简化为裸节号）；**没有增删
或改写任何一条
已有规则**。语法规范文档（03/04/05）内部保留了原第四部分的节号（§1–9），没有从 1 重新
编号，为的是不打乱 `SPEC-000N` 与既有 Span/错误恢复表述里对具体节号的引用。

拆分动机：第四部分单独占了原文档 43%（1165 行），而 SPEC-0014 至 0017（完整文件组合、
`package`/`import`、control-flow、class-family）在拆分当时都还没写；按已完成 Spec 的密度外推，
Phase 1 全部写完后原文档大概率会超过 4000 行，Phase 2 及以后的详细规则还没算进去。现在
按已有的 Spec 边界拆分，比规模更大之后再拆成本更低。

## 3. 文档地图

| 文件 | 内容 | 约行数 | 预期变化频率 |
|---|---|---|---|
| `00-index.md`（本文档） | 版本治理、文档地图、精简历史、SPEC/错误码索引 | ~160 | 每次任何文档变化都要碰一下 |
| [`01-design-decisions.md`](./01-design-decisions.md) | 原第一部分全部 20 节 + 原第二部分（现为附录） | ~840 | 中——设计级变更会碰它，如本次所有权标注简化 |
| [`02-lexical-spec.md`](./02-lexical-spec.md) | 原第三部分，完整词法规范 | ~240 | 低——v0.17 以 `package` 替换 `module` 硬关键字 |
| [`03-grammar-core.md`](./03-grammar-core.md) | 原第四部分 §1–6：primary/postfix/`type_ref`/运算符优先级/Lexer 交接/AST `Span` 规则 | ~320 | 低到中——是 04、05 的共享基础 |
| [`04-grammar-declarations-blocks.md`](./04-grammar-declarations-blocks.md) | 原第四部分 §7–8 + §10–11：声明、block、完整文件恢复与文件头 | ~530 | 中——v0.17 新增 `package` / `import` 文件头契约 |
| [`05-grammar-calls-lambda.md`](./05-grammar-calls-lambda.md) | 原第四部分 §9：SPEC-0010–0013（lambda、隐式 `Unit`、typed call argument、局部解构） | ~480 | 低——SPEC-0010–0013 均已验收；后续只在勘误或新版语义变更时修改 |
| [`06-roadmap.md`](./06-roadmap.md) | 原第二、五、六部分：结构总览附录见 01；Phase 0–6 路线图 + 工程规范 | ~290 | 高——每验收一个 Spec 就要碰一下 checkbox |
| [`07-changelog-archive.md`](./07-changelog-archive.md) | v0.3–v0.17 完整逐版本变更记录表格（含 v0.13 结构调整说明） | ~225 | 只追加，不修改 |

**不知道该看哪份文档时的经验法则**：要写 parser/lexer 代码 → 02/03/04/05；要理解某条
规则“为什么这么设计” → 01；要知道“现在该做哪个 Spec” → 06；要查“这个错误码/这个 SPEC
编号在哪” → 用下面第 5、6 节的索引表；要查“某条规则是哪个版本引入的” → 07。

## 4. 精简版本历史

完整逐条记录见 [`07-changelog-archive.md`](./07-changelog-archive.md)；这里只给一版本
一行的摘要。

| 版本 | 一句话摘要 |
|---|---|
| v0.3 | 对照审计报告逐条修复（跨线程 API、`Map`/`Indexable` 拆分、运算符优先级重写等 22 项） |
| v0.4 | `value class` 内联语义与 `Copyable` 能力分离；解构、字段投影借用规则定型 |
| v0.5 | 补齐词法基础：UTF-8/标识符/关键字边界/字面量/trivia/`Span`/Lexer 验收 |
| v0.6 | 表达式与类型引用可执行语法定型；顺序容器（`Array`/`List`/`MutableList`）布局与所有权语义封闭 |
| v0.7 | 独立声明入口（`val`/`var`/`const val`/`fun`）语法与恢复规则定型（SPEC-0008 基础） |
| v0.8 | block、statement 序列、函数 block body 语法定型（SPEC-0009 基础） |
| v0.9 | lambda literal、隐式 `Unit` 返回、typed call argument AST 契约、局部 `val` 解构分阶段边界定型 |
| v0.10 | 引入统一 `Value`/`Own`/`Borrow`/`Inout` 四契约调用点标注体系，解除 SPEC-0012 设计门禁 |
| v0.11 | 补齐整数溢出/除零、`Transferable`；新增 Map/MutableMap 与错误传播 `?` 候选设计；`Copyable` opt-out 候选方向 |
| v0.12 | 取消独立的 `Own` 契约（并入 `Value`），`Borrow` 调用点标注改为可选，`Inout` 保持强制；`own` 的语法用途退役，但仍保留为硬关键字 |
| v0.13 | **结构调整，非语义变更**：v0.12 单文件拆分为本文档集 |
| v0.14 | `Inout` 调用点标注拼写从关键字 `inout` 改为符号 `&`；声明侧关键字 `inout` 不变；词法层新增固定符号 `&`（唯一触及已实现 SPEC-0006 词法基线的改动） |
| v0.15 | 封闭完整文件产生式、声明 soft boundary、文件级错误 Item、跨声明恢复与线性复杂度契约 |
| v0.16 | 顶层声明改由换行或 `;` 分隔；同一行多个声明必须写 `;`，并保留 owner-aware 恢复边界 |
| v0.17 | 以 `package` 取代 `module` 硬关键字；定义 Kotlin 风格 exact / wildcard / alias `import` 文件头及 Phase 1 AST / 恢复边界 |

## 5. SPEC 编号索引

“定义于”指该 SPEC 的产生式、AST 契约与验收标准主要写在哪份文档；多数 SPEC 在
[`06-roadmap.md`](./06-roadmap.md) 的 Phase 1 checklist 里也有一条对应记录，此处不重复
列出。

| SPEC | 内容 | 定义于 | 状态 |
|---|---|---|---|
| SPEC-0007 | 独立表达式入口、运算符层级、`type_ref`、仅位置实参 basic call | `03-grammar-core.md` | ✅ 已实现 |
| SPEC-0008 | 独立 `val`/`var`/`const val`/`fun` 声明入口 | `04-grammar-declarations-blocks.md` §7 | ✅ 已实现 |
| SPEC-0009 | block、statement 序列、函数 block body | `04-grammar-declarations-blocks.md` §8 | ✅ 已实现 |
| SPEC-0010 | lambda literal | `05-grammar-calls-lambda.md` §9 | ✅ 已实现 |
| SPEC-0011 | 具名函数省略返回标注固定为 `Unit` | `05-grammar-calls-lambda.md` §9 | ✅ 已实现 |
| SPEC-0012 | 统一 callable 参数 marker、typed call argument（v0.12 起为三契约版本；v0.14 起调用点 `Inout` 标注改用符号 `&`） | `05-grammar-calls-lambda.md` §9（声明侧类型语法见 `03-grammar-core.md` §3，`&` 词法定义见 `02-lexical-spec.md` §7） | ✅ 已实现 |
| SPEC-0013 | 局部 `val` 解构 | `05-grammar-calls-lambda.md` §9 | ✅ 已实现 |
| SPEC-0014 | 完整文件、声明分隔与跨声明恢复 | `04-grammar-declarations-blocks.md` §10 | ✅ 已实现 |
| SPEC-0015 | `package` / Kotlin 风格 `import` 文件头 | `04-grammar-declarations-blocks.md` §11 | ⬜ 未实现 |
| SPEC-0016 | control-flow（`if`/`when`/循环） | 尚未撰写 | ⬜ 未实现，`01-design-decisions.md` 多处示例依赖其排期 |
| SPEC-0017 | class-family（`class`/`interface`/`enum class`/`object`） | 尚未撰写 | ⬜ 未实现 |
| SPEC-0062 | v0.16 顶层声明换行 / 分号分隔增量 | `04-grammar-declarations-blocks.md` §10 | ✅ 已实现 |

## 6. 错误码索引（近似区间，精确定义以对应文档正文为准）

错误码按 SPEC 完成顺序连续分配，因此区间边界严格来说属于“分配顺序”而非“文件边界”；
下表给出实际观察到的近似区间，精确的单条定义仍须查对应文档正文，不要仅凭本表下结论。

| 区间 | 主要归属 | 文档 |
|---|---|---|
| L0001–L0008 | 已实现 Lexer 诊断 | `02-lexical-spec.md` |
| L0009–L0032 | 已实现 Parser 诊断 | `03-grammar-core.md`、`04-grammar-declarations-blocks.md`、`05-grammar-calls-lambda.md` |
| L0004–L0006 | Lexer 字符串/插值终止相关诊断类别 | `02-lexical-spec.md`，恢复语义见 `04-grammar-declarations-blocks.md` |
| L0016 | SPEC-0007 历史类别 `unsupported argument form`（SPEC-0012 后生产 Parser 不再发出，历史含义与编号保留不复用） | `05-grammar-calls-lambda.md` |
| L0024–L0026 | SPEC-0008 声明列表诊断 | `04-grammar-declarations-blocks.md` |
| L0031–L0032 | SPEC-0010 lambda body 诊断 | `05-grammar-calls-lambda.md` |
| L0033–L0039 | SPEC-0012 call argument / 参数模式诊断（含 `duplicate argument mode` 等，v0.14 起调用点字母表为 `borrow` 关键字 + `&` 符号，诊断类别与编号不变） | `05-grammar-calls-lambda.md`，声明侧引用见 `03-grammar-core.md`、`04-grammar-declarations-blocks.md` |
| L0040–L0046 | SPEC-0013 局部解构诊断 | `05-grammar-calls-lambda.md` |
| L0047 | SPEC-0062 同行声明缺少 `;` | `04-grammar-declarations-blocks.md` §10 |
| L0048–L0054 | SPEC-0015 package/import 名称、位置、分隔与 wildcard alias 诊断 | `04-grammar-declarations-blocks.md` §11 |

`&` 符号本身没有分配新的错误码——调用点继续使用 L0033–L0038 既有类别，只把其中
“调用模式 token”的字母表从 `borrow`/`inout` 两个关键字改成 `borrow` 关键字 + `&` 符号；
声明侧 L0039 的 `borrow`/`inout` 字母表不变。v0.14 后裸 `&` 不再触发 L0001（`&` 现在是
合法固定符号），但 L0001 类别仍用于其他非法字符；这里只移除了该字符的旧触发情形。

## 7. 核心概念速查（概念 → 主要讨论位置）

| 概念 | 主要位置 |
|---|---|
| `Copyable` / `Transferable` / `Hashable` 标记能力 | `01-design-decisions.md` §5、§17、§18.1 |
| `Value`/`Borrow`/`Inout` 调用点契约（v0.12 起三契约） | `01-design-decisions.md` §4-5、`03-grammar-core.md` §3、`05-grammar-calls-lambda.md` §9 |
| `Own` 契约与 `own` 语法用途退役说明 | `02-lexical-spec.md` §1、`05-grammar-calls-lambda.md` §9 |
| `Inout` 调用点符号 `&`（v0.14，区别于声明侧关键字 `inout`） | `02-lexical-spec.md` §7、`03-grammar-core.md` §3-4、`05-grammar-calls-lambda.md` §9 |
| 顺序容器（`Array`/`List`/`MutableList`）所有权语义 | `01-design-decisions.md` §8 |
| `Map`/`MutableMap` 候选设计（未批准） | `01-design-decisions.md` §18 |
| `enum class` / ADT 能力 | `01-design-decisions.md` §7 |
| 所有权检查 Phase 3 验收标准 | `06-roadmap.md` Phase 3 |

---

*本索引与其余 7 份文档共同构成 Koven 现行语言设计规范 v0.17；v0.13 是唯一的
纯结构调整版本，不携带语义内容。版本、启用状态、候选边界与治理规则以本索引为准；具体
语言语义冲突时以对应正文为准，并请提交修正。*
