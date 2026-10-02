# v0.39 启用与文档迁移账本

> **性质**：版本迁移记录 · **状态**：frozen · **读取时机**：追溯四项已批准语言规则的启用时 · **唯一真源**：本记录的来源与迁移证据

2026-10-01 用户明确同意四项规则。v0.39 完整继承并取代 v0.38，只修改已批准的调用处
Borrow marker、移位、deinit 和 Str/String 规则及其直接一致性文本，不把规范启用当作实现完成。

## 快照来源与保存方法

- 源版本：main `d3e64a4` 的 `docs/guide/`，共 16 页（README + 15 个领域）。
- 快照目录：[v0.38](../guides/v0.38/README.md)。全部正文与当时的 current 状态保留，
  不把冻结页的版本号、验收结论或冲突改写为现行结论。
- 唯一机械变换：Markdown 链接前缀 `](../` 改为 `](../../../`，使指向 Guide 之外页面的
  相对链接在新目录中仍落到原目标；域内相对文件链接及锚点保持原样。
- 下表 SHA-256 对源 UTF-8 字节计算；快照逆向上述链接变换后须逐页匹配。旧二/三级标题
  在同名现行领域页继续有归属，本次无正文搬迁或标题删除。
- 本快照不含尚未整合的 SPEC-0229–0234 独立分支修改；后续集成必须重新核对版本归属与
  快照/链接证据，不能把此 main 来源宣称为其他分支成果的归档。

| 源文件（`docs/guide/`） | v0.38 源 SHA-256 | 现行归属 |
|---|---|---|
| `01-lexical.md` | `016872b02125422dc9bf3af9d866925f903e5d7f44eec42a329629d7badbe8c2` | [同名 v0.39 页面](../../guide/01-lexical.md) |
| `02-names-files-packages.md` | `edef3d0c1f6e8e6c34f7c65ae37751662d551a0e8963f26c96bb98881690f9a3` | [同名 v0.39 页面](../../guide/02-names-files-packages.md) |
| `03-types-generics.md` | `2c60b139dad5850feaa94a934854ccf3825849211d1f2034585d53537d879363` | [同名 v0.39 页面](../../guide/03-types-generics.md) |
| `04-expressions-operators.md` | `37fe4b29fdf14bd5255a047ac24ffbd75a9dda443821db621e4eef10bd3db855` | [同名 v0.39 页面](../../guide/04-expressions-operators.md) |
| `05-declarations-callables.md` | `e51a056df024ddce5f6dc470dcf70e037ba7cb926a4e26b3c032d64f951c0211` | [同名 v0.39 页面](../../guide/05-declarations-callables.md) |
| `06-blocks-control-flow.md` | `f9a8e5ea696f230de30bc93f473fee23100923963f1eba7ccea0e07f9738cb36` | [同名 v0.39 页面](../../guide/06-blocks-control-flow.md) |
| `07-calls-lambdas-closures.md` | `a88854d27a2ef558f53cdf8577ffb0dd941d5cf9ae18af89a022806a6bdbbe05` | [同名 v0.39 页面](../../guide/07-calls-lambdas-closures.md) |
| `08-class-family-members.md` | `b247405430315d7c0579064022af72a9b6b5ca618847e48bcd6089d5a42792fa` | [同名 v0.39 页面](../../guide/08-class-family-members.md) |
| `09-nullability-errors.md` | `4742e4b8d43df409591b4388b85c5e26212879880a99726414ba9efd08984a95` | [同名 v0.39 页面](../../guide/09-nullability-errors.md) |
| `10-ownership-borrowing-drop.md` | `d169cc9d2cc1c6b45bde4720786b98eabd8634b4b299f3b0fb4a87cd80580648` | [同名 v0.39 页面](../../guide/10-ownership-borrowing-drop.md) |
| `11-copyability-layout-construction.md` | `693579db8c9812db9450976661197a2bde3c58ee7b2e28c73d50f9a0ab073282` | [同名 v0.39 页面](../../guide/11-copyability-layout-construction.md) |
| `12-collections-destructuring.md` | `25eb2369238ddb157c5cf1ccc5e1d6d0aa409271c34c1f391e3bb779b9c2c1c8` | [同名 v0.39 页面](../../guide/12-collections-destructuring.md) |
| `13-program-runtime-standard-library.md` | `b81ff2efef51e57d2e8f6c9fddc4dafc0be6adf08b4c7d2305fd222f5cbcb878` | [同名 v0.39 页面](../../guide/13-program-runtime-standard-library.md) |
| `14-syntax-index.md` | `1ed5fa5c5c8bc613bc7440c97a844f5b5b5361d0a87f88b68a563e75ae122fdb` | [同名 v0.39 页面](../../guide/14-syntax-index.md) |
| `15-conformance-and-staging.md` | `a83a22b8d0fb42dfc598d669dd5b80a62ab2168b6aef5caaf01c0b1720f5fc31` | [同名 v0.39 页面](../../guide/15-conformance-and-staging.md) |
| `README.md` | `d51eb5efd35532da0873e91fdb31430be9e025b545a42801a114514bf2f4c0d0` | [同名 v0.39 页面](../../guide/README.md) |

## 规则与一致性迁移

| 旧位置/冲突 | v0.39 归属与处理 |
|---|---|
| 01/03/04/05/07/08/11/12/14 中调用 Borrow marker 与软词规则冲突 | [07 调用实参](../../guide/07-calls-lambdas-closures.md#typed-call-argument)；声明 marker 保持，调用 marker 仅剩 `&`，普通 borrow 名称不被占用 |
| 04 位移语法缺少超界执行规则 | [04 整数移位](../../guide/04-expressions-operators.md#整数具名位运算与移位)；同型整数、width mask、位级左右移结果 |
| 08 deinit 缺 body / 字段顺序和 this 能力 | [08 deinit](../../guide/08-class-family-members.md#deinit-成员语法与资源析构契约)；10 仅链接该规则，不改双轨析构时机 |
| 03/13 的 literal→String 与静态 Str 分层相冲突 | [13 文本分层](../guides/v0.38/13-program-runtime-standard-library.md#静态-str-字面量与动态-string)；同步字面量、能力表、混合操作、println/error 契约 |
| 05/15 将已定型 String const 物化称为 literal owner | Str literal/const 与封闭文本运算作最小一致性更新；Str use 静态复制，String use 仍独立物化，不开放普通调用 CTFE |
| 03/05 的旧 String literal 示例；15 在 const 中使用移位 | 使用符合分层规则的 Str const / Str 返回示例；移位示例放回普通运行时表达式 |
| 06 恢复段、07 尾 lambda 段否认 block 分隔 | 按 v0.38 已有 block 分隔/续行正文作同义修正；不扩大 lambda tail 或错误恢复的接受范围 |
| current 版本与入口检查 | 16 页与根路由更新；检查器同时校验所有 live 版本 marker、唯一入口位置及 v0.39 身份；所有预算不变 |
| 既有 String ABI / 实现事实 | ADR-0018 保持原文；Architecture 保留真实代码状态，Guide15 标出转换 API、Str ABI 与实现未完成边界 |

## 验证归属

实际命令、计数与未运行项记录于 [SPEC-0235](../specs/0235-approved-language-rules.md)。
快照摘要只证明完整来源保存；文档结构检查不能代替四项规则的一致性审查，也不能证明编译器支持。
