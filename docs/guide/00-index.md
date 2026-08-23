# Koven 语言设计规范 · 总纲与索引

Koven 是一门编译型语言：语法尽量贴近 Kotlin 命名与语法习惯，内存模型是 Rust 式简化
所有权/借用，编译器用 Rust 实现，LLVM 后端。本文档集是面向负责实现编译器的 AI agent
的执行契约（agent-facing language design guide），不是面向语言使用者的教程——教程见
仓库中的 [`koven-language-tour.md`](../koven-language-tour.md)。

**这是整个文档集的入口。** 除非你已经知道自己要找哪一份文件，否则从这里开始：下面先
说版本治理规则，再给文档地图、精简版本历史和多张检索表。

---

## 1. 版本与状态

- **当前唯一权威版本是本文档集的 v0.26**，已于 2026-08-23 由用户明确启用，取代 v0.25；
  v0.14 此前已取代 [`agent-language-design-guide-v0.9.md`](../agent-language-design-guide-v0.9.md)。v0.12 及更早
  单文件 guide 只作为历史材料，不参与现行语义优先级。
- **当前文档集版本是 v0.26**：v0.10 引入统一的 callable 参数契约，v0.11 补齐
  整数溢出/`Transferable`/Map 候选设计/`?` 候选设计，v0.12 取消了独立的 `Own` 契约、
  把 `Borrow` 的调用点标注改为可选，v0.13 是纯结构拆分（不涉及语义），v0.14 把
  `Inout` 的调用点标注从关键字 `inout` 改写为符号 `&`；v0.15 封闭完整文件与跨声明恢复
  契约；v0.16 把顶层声明分隔修正为换行或分号；v0.17 以 `package` 取代 `module`，并
  封闭 Kotlin 风格的绝对 `import` 文件头语法；v0.18 封闭 control-flow、statement/value
  context `if` 与最近 callable `return` 语义；v0.19 正式采用 `Result<T, E>` 错误值、postfix
  `?` 与无异常展开的可恢复失败模型；v0.20 封闭 class-family、轻量 companion、匿名对象
  边界、窄化接口委托与 interface companion 常量；v0.21 封闭单文件双命名空间、作用域、
  预声明环境和首批名称诊断；v0.22 封闭最小数值后缀、默认数值类型与基础类型检查契约；
  v0.23 封闭名义/泛型身份、interface 静态实现、override/default 冲突与窄化接口委托；
  v0.24 封闭 enum case type、`when` 穷尽性与 smart cast；v0.25 封闭条件 `Copyable`、有限
  内联布局、intrinsic `Box` 与结构化解构类型契约；v0.26 把 callable 的无标记参数改为
  `Borrow`、恢复声明端 `own` 作为既有 `Value` 契约的显式拼写，并启用调用期 loan 与 ASAP
  drop-point 契约。
  完整逐版本
  记录见下文
  “精简版本历史”与 [`07-changelog-archive.md`](./07-changelog-archive.md) 的完整表格。
- **v0.12、v0.13 已合入 v0.14**。v0.11、v0.12 的单文件候选快照已在 v0.14 启用后
  补回，仅用于核对合入过程；v0.13 没有独立快照。v0.13 是本文档集唯一一次不携带
  任何语义内容的版本号，只把 v0.12 单文件工作稿按内容边界拆成本索引 + 6 份正文 + 1 份
  变更归档，因此没有进入语义变更记录表格，单独在下方“结构调整说明”里交代。除这一版
  外，版本号是单一递增序列，不再区分“语义
  版本”和“结构版本”两条轴——每份正文文档顶部标注的是它自己内容最近一次改动所在的
  版本；本索引聚合记录整个文档集当前启用的 v0.26 状态。
- [`01-design-decisions.md`](./01-design-decisions.md) 第 16、17、19–26 节是现行规范规则；
  第 18 节仍明确标注为 Map 候选设计，在完成设计门禁并补充到对应实施 Spec 之前，
  不得被 Phase 2/3/5 实现直接引用为已批准契约。v0.22 的数值后缀由 SPEC-0066 实施，
  L0082–L0090 与基础类型检查由 SPEC-0019 实施。
- **v0.25 已明确启用**：[`01-design-decisions.md`](./01-design-decisions.md) §25 的条件
  `Copyable`、有限内联布局、intrinsic `Box` 与结构化解构契约，以及 L0115–L0118 已成为
  现行语义；SPEC-0022 已完成实施。
- **v0.26 已明确启用**：无标记 callable / function-type 参数是 `Borrow`；显式 `borrow`
  是同一契约的可选强调，不形成不同函数类型或 overload；声明端 `own` 映射既有
  `ParameterMode::Value`，调用点仍不接受 `own`，向该参数传入 `MoveOnly` place 时以无标记
  调用隐式移动；`inout` / 调用点 `&` 保持不变。语法与 typed-contract 已由 SPEC-0176
  实现；调用期 loan 与 ASAP drop-point 等待 SPEC-0029，不能把参数迁移误写成借用检查完成。
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
| [`01-design-decisions.md`](./01-design-decisions.md) | 26 节现行设计 + Map 候选 §18 + 原第二部分（现为附录） | ~1550 | 中——设计级变更会碰它，如名称、作用域与类型契约 |
| [`02-lexical-spec.md`](./02-lexical-spec.md) | 原第三部分，完整词法规范 | ~240 | 低——v0.22 新增最小数值后缀集合 |
| [`03-grammar-core.md`](./03-grammar-core.md) | 原第四部分 §1–6：primary/postfix/`type_ref`/运算符优先级/Lexer 交接/AST `Span` 规则 | ~330 | 低到中——v0.19 新增 postfix `?` |
| [`04-grammar-declarations-blocks.md`](./04-grammar-declarations-blocks.md) | 原第四部分 §7–8 + §10–13：声明、block、完整文件恢复、文件头、控制流与 class-family | ~850 | 中——v0.20 新增 class-family 契约 |
| [`05-grammar-calls-lambda.md`](./05-grammar-calls-lambda.md) | 原第四部分 §9：SPEC-0010–0013（lambda、隐式 `Unit`、typed call argument、局部解构） | ~480 | 低——SPEC-0010–0013 均已验收；后续只在勘误或新版语义变更时修改 |
| [`06-roadmap.md`](./06-roadmap.md) | 原第二、五、六部分：结构总览附录见 01；Phase 0–6 路线图 + 工程规范 | ~290 | 高——每验收一个 Spec 就要碰一下 checkbox |
| [`07-changelog-archive.md`](./07-changelog-archive.md) | v0.3–v0.26 完整记录（含 v0.13 结构调整与 v0.26 启用审计） | ~390 | 只追加，不修改 |

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
| v0.18 | 定义 `if` / `when` / loop-family / jump / `super`；缺 `else` 的 `if` 仅限 statement context，lambda 成为独立 `return` 边界 |
| v0.19 | 可恢复失败固定为显式 `Result<T, E>` 错误值；定义最近 callable postfix `?`，明确不提供异常语法或栈展开 |
| v0.20 | 封闭 Kotlin 表面风格 class-family；companion 为无状态关联命名空间；排除匿名对象和属性委托，保留窄化接口实现委托与 interface companion 常量 |
| v0.21 | 封闭单文件类型/值双命名空间、稳定 scope/symbol 身份、预声明与顺序 local 可见性，以及 L0079–L0081 名称诊断 |
| v0.22 | 新增 `L` / `u` / `f` 最小数值后缀；封闭默认数值类型、单向 expected type、基础 local/lambda/函数返回检查与 L0082–L0090 |
| v0.23 | 封闭名义/泛型身份、interface 静态实现、override/default 冲突、窄化接口委托与 L0091–L0105 |
| v0.24 | enum case type、流敏感 smart cast、有限域 `when` 穷尽性与 L0106–L0114 |
| v0.25 | 封闭条件 `Copyable`、有限内联布局、intrinsic `Box` 与结构化解构；已明确启用 |
| v0.26 | 无标记参数改为 Borrow，声明端 `own` 映射既有 Value 契约；启用调用期 loan、参数绑定能力与 ASAP drop-point；SPEC-0176 已实现参数迁移，loan/drop 待 SPEC-0029 |

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
| SPEC-0015 | `package` / Kotlin 风格 `import` 文件头 | `04-grammar-declarations-blocks.md` §11 | ✅ 已实现 |
| SPEC-0016 | control-flow（`if`/`when`/循环/jump/`super`） | `04-grammar-declarations-blocks.md` §12 | ✅ 已实现 |
| SPEC-0017 | class-family（`class`/`interface`/`enum class`/`object`） | `04-grammar-declarations-blocks.md` §13 | ✅ 已实现 |
| SPEC-0018 | 单文件声明收集、作用域与名称诊断 | `01-design-decisions.md` §21 | ✅ 已实现 |
| SPEC-0019 | 基础类型、局部推导与函数返回检查 | `01-design-decisions.md` §22 | ✅ 已完成 |
| SPEC-0020 | 名义类型、泛型、interface 实现与窄化委托 | `01-design-decisions.md` §23 | ✅ 已完成 |
| SPEC-0021 | `when` 穷尽性与 smart cast | `01-design-decisions.md` §24 | ✅ 已完成 |
| SPEC-0022 | 条件 `Copyable`、有限内联布局与结构化解构类型检查 | `01-design-decisions.md` §25 | ✅ 已实现 |
| SPEC-0023 | 顺序容器名义类型、核心构造与索引 place 类型检查 | `01-design-decisions.md` §8 | ✅ 已实现 |
| SPEC-0027 | 变量 ownership state 与 use-after-move | `../specs/0027-variable-ownership-use-after-move.md` | ✅ 已实现 |
| SPEC-0028 | 条件复制、结构化移动与禁止部分移动 | `../specs/0028-conditional-copy-structural-move.md` | ✅ 已实现 |
| SPEC-0029 | 调用期 loan 与 ASAP drop-point | `01-design-decisions.md` §26、`../specs/0029-call-loans-drop-points.md` | ⏳ v0.26 已启用，未实现 |
| SPEC-0062 | v0.16 顶层声明换行 / 分号分隔增量 | `04-grammar-declarations-blocks.md` §10 | ✅ 已实现 |
| SPEC-0063 | v0.19 postfix `?` 错误传播增量 | `01-design-decisions.md` §19、`03-grammar-core.md` §2/§4/§6 | ✅ 已实现 |
| SPEC-0064 | v0.20 `Interface by valField` 接口实现委托 Parser 增量 | `04-grammar-declarations-blocks.md` §13.3 | ✅ 已实现 |
| SPEC-0066 | v0.22 `L` / `u` / `f` 数值后缀 Lexer / AST 增量 | `02-lexical-spec.md` §6 | ✅ 已实现 |
| SPEC-0067 | 单态 callable/member 选择、实参映射与 place/temporary 分类 | `05-grammar-calls-lambda.md` §9 | ✅ 已实现 |
| SPEC-0173 | 唯一期望函数类型的 lambda 参数契约 typed facts | `05-grammar-calls-lambda.md` §9、`../specs/0173-lambda-parameter-contract-facts.md` | ✅ 已实现 |
| SPEC-0175 | block 内调用实参 lambda 边界修复 | `05-grammar-calls-lambda.md`、`../specs/0175-call-argument-lambda-boundary.md` | ✅ 已实现 |
| SPEC-0176 | v0.26 无标记 Borrow、声明端 `own` 与 callable typed-contract 迁移 | `03-grammar-core.md` §3、`04-grammar-declarations-blocks.md` §7、`05-grammar-calls-lambda.md` §9、`../specs/0176-borrow-default-parameter-contracts.md` | ✅ 已实现 |

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
| L0033–L0039 | SPEC-0012 call argument / 参数模式诊断（含 `duplicate argument mode` 等；调用点字母表仍为 `borrow` 关键字 + `&` 符号，v0.26 声明侧字母表扩为 `own` / `borrow` / `inout`，诊断类别与编号不变） | `05-grammar-calls-lambda.md`，声明侧引用见 `03-grammar-core.md`、`04-grammar-declarations-blocks.md` |
| L0040–L0046 | SPEC-0013 局部解构诊断 | `05-grammar-calls-lambda.md` |
| L0047 | SPEC-0062 同行声明缺少 `;` | `04-grammar-declarations-blocks.md` §10 |
| L0048–L0054 | SPEC-0015 package/import 名称、位置、分隔与 wildcard alias 诊断 | `04-grammar-declarations-blocks.md` §11 |
| L0055–L0065 | SPEC-0016 条件、分支、when entry、loop、for 与 super 诊断 | `04-grammar-declarations-blocks.md` §12 |
| L0066–L0077 | 已实现的 SPEC-0017 class-family 头、字段、成员、enum 与修饰符诊断 | `04-grammar-declarations-blocks.md` §13 |
| L0078 | 已实现的 SPEC-0064 expected delegation target | `04-grammar-declarations-blocks.md` §13.3–13.4 |
| L0079–L0081 | 已实现的 duplicate / unresolved / use-before-local 名称诊断 | `01-design-decisions.md` §21.3 |
| L0082–L0090 | v0.22 的基础类型、推导、return、control 与数值范围诊断；SPEC-0019 已实现 | `01-design-decisions.md` §22.5 |
| L0091–L0105 | v0.23 的名义/泛型/interface/override/委托诊断；SPEC-0020 已实现 | `01-design-decisions.md` §23.5 |
| L0106–L0114 | v0.24 的 type-test/smart-cast/when 诊断；SPEC-0021 已实现 | `01-design-decisions.md` §24.5 |
| L0115–L0118 | v0.25 的 `Copyable` bound、内联递归、intrinsic `Box` 与结构化解构诊断；SPEC-0022 已实现 | `01-design-decisions.md` §25.5 |
| L0119–L0124 | callable target、命名/数量/模式映射、无匹配与歧义诊断；SPEC-0067 已实现 | `05-grammar-calls-lambda.md` §9 |
| L0125–L0130 | 顺序容器元素、推导、核心构造、索引、只读 place 与禁用 `.get`/`.set` 诊断；SPEC-0023 已实现 | `01-design-decisions.md` §8 |
| L0131–L0132 | use-after-move 与禁止不可复制分量部分移动；SPEC-0027/0028 已实现 | `../specs/0027-variable-ownership-use-after-move.md`、`../specs/0028-conditional-copy-structural-move.md` |
| L0133–L0135 | borrowed value 移出、非法 `Inout` place 与有效 loan 冲突；v0.26 已启用，尚未实现 | `01-design-decisions.md` §26.5 |

`&` 符号本身没有分配新的错误码——调用点继续使用 L0033–L0038 既有类别，只把其中
“调用模式 token”的字母表从 `borrow`/`inout` 两个关键字改成 `borrow` 关键字 + `&` 符号；
声明侧 L0039 的字母表在 v0.26 扩为 `own`/`borrow`/`inout`。v0.14 后裸 `&` 不再触发 L0001（`&` 现在是
合法固定符号），但 L0001 类别仍用于其他非法字符；这里只移除了该字符的旧触发情形。

## 7. 核心概念速查（概念 → 主要讨论位置）

| 概念 | 主要位置 |
|---|---|
| `Copyable` / `Transferable` / `Hashable` 标记能力 | `01-design-decisions.md` §5、§17、§18.1 |
| `Value`/`Borrow`/`Inout` 三契约与 v0.26 默认 Borrow | `01-design-decisions.md` §4-5、`03-grammar-core.md` §3、`05-grammar-calls-lambda.md` §9 |
| 声明端 `own` → `ParameterMode::Value` 与调用点隐式 move | `02-lexical-spec.md` §1、`03-grammar-core.md` §3、`05-grammar-calls-lambda.md` §9 |
| `Inout` 调用点符号 `&`（v0.14，区别于声明侧关键字 `inout`） | `02-lexical-spec.md` §7、`03-grammar-core.md` §3-4、`05-grammar-calls-lambda.md` §9 |
| 顺序容器（`Array`/`List`/`MutableList`）所有权语义 | `01-design-decisions.md` §8 |
| `Map`/`MutableMap` 候选设计（未批准） | `01-design-decisions.md` §18 |
| `Result<T, E>` / postfix `?` / 无异常错误模型 | `01-design-decisions.md` §19、`03-grammar-core.md` §2/§4 |
| `enum class` / ADT 能力 | `01-design-decisions.md` §7 |
| class-family / companion / 接口委托 / 匿名对象边界 | `01-design-decisions.md` §12、§14–15，`04-grammar-declarations-blocks.md` §13 |
| 单文件双命名空间、作用域、预声明与名称诊断 | `01-design-decisions.md` §21 |
| 基础类型、局部推导与返回检查（v0.22） | `01-design-decisions.md` §22 |
| 名义类型、泛型与 interface 实现（v0.23） | `01-design-decisions.md` §23 |
| `when` 穷尽性与 smart cast（v0.24） | `01-design-decisions.md` §24 |
| 条件 `Copyable`、内联布局、intrinsic `Box` 与结构化解构（v0.25） | `01-design-decisions.md` §25 |
| 调用期借用与 ASAP 析构点（v0.26，实施待 SPEC-0029） | `01-design-decisions.md` §26 |
| 所有权检查 Phase 3 验收标准 | `06-roadmap.md` Phase 3 |

---

*除明确排除的候选 §18 外，本索引与其余 7 份文档共同构成 Koven 现行语言设计规范
v0.26；v0.13 是唯一的纯结构调整版本，不携带语义内容。版本、启用状态、候选边界与治理规则以本索引为准；具体
语言语义冲突时以对应正文为准，并请提交修正。*
