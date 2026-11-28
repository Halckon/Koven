# Parser 验证数据模型

> **性质**：非规范观察模型设计 · **状态**：未启用 · **读取时机**：实施验证适配层与证据报告前 · **唯一真源**：本页维护候选实体与约束

所有实体是测试/证据概念，不要求新增生产类型。实际 AST 结构仍由
[AST 合同](../../compiler-specs/parser-ast.md)与源码定义。

## 实体与关系

| 实体 | 最小字段 | 关系与校验 |
|---|---|---|
| Baseline | source_sha、guide_version/hash、工具版本 | 一次运行绑定一个不可变输入身份，不用分支名替代 SHA |
| Rule | rule_id、Guide 路径/锚点、谓词、切片、状态 | 回链规范；pending 规则不能产生已批准 expected |
| ParseCase | case_id、UTF-8 源码、entry、start/context、budget、expected 来源 | entry 为 file/expression/declaration/block；内部子入口单列 caller owner/停止边界 |
| LexicalView | lexeme kind/Span、拼写、trivia/换行、EOF、诊断/terminal events | 源码完整覆盖、SourceId 与 UTF-8 边界有效，不能只保留 TokenKind |
| Observation | outcome、roots/ast、diagnostics、boundary_evidence | 依入口判断成功，file 可有多个根，完整文件结构投影还须纳入 package 与有序 imports，否则标为未覆盖；不伪造公开 API 没有的 end 游标 |
| NormalizedSyntax | 节点类别、typed edges、拼写身份、marker 存在性、子节点顺序 | 先 SourceMap 切片再去 arena/source/绝对位置；引用实际 AST 合同 |
| RecoveryCase | R1–R5、caller→callee、owner/stops、进度/资源见证 | 独立于合法推导；同一失败可有多个违反项 |
| Conflict | rule_ids、原始冲突、工具/模式、处置、批准依据 | precedence/谓词后的确定性与原 CFG 无歧义分开记录 |
| Evidence | scope、命题、方法、版本/hash、budget、命令、结果、稳定来源 | 工具输出为证据而非语义裁决；完整 raw 不入文档 |
| Difference | case_id、差异、归因、最小反例、状态 | 归因到规范/lexer/parser/tree-sitter/normalizer/harness/预算，不自动更新 golden |

## Observation 四分类

- Accept：符合入口消费合同，无表示语法失败的词法/Parser 诊断或恢复 Error；不检查 Phase 2/3。
- SyntaxError：保留错误诊断及可能的恢复 AST，不要求存在合法文法推导。
- ResourceLimit：明确的实现预算耗尽，例如 NestingLimitExceeded；不是语法反例。
- InternalFailure：Source/AST/diagnostic/非法词法流不变式、线程 panic 等内部失败。
  线程创建错误在观察层保留原 error kind，并标记环境来源，不能伪装成语法拒绝。

同一 case、baseline、budget 重跑必须得到相同观察；环境错误保留并使该次检查无成功结论。
公开入口无 end 游标时用完整入口断言与 Span/后继 sentinel 见证；需要内部 trace 时另行 Spec，
不能由 root Span 推断“所有输入已消费”。

## NormalizedSyntax 约束

- `x/y`、`1/2`、不同字符/字符串以及 marker 有无必须可区分。
- 无 header 与显式零参数 header、move 有无、参数模式、显式/隐式函数形式不得合并。
- 仅 arena 分配编号或整体位置平移不同应相等；名称/字面量拼写不做语义值归一。
- Span 独立验证：合法字符边界、真实消费、缺失节点零宽锚点、owner/closer 与父子覆盖例外。
- Normalizer 自身先有上述正反金标准，不由生产 Parser 当次输出生成 expected。

## 状态迁移

规则：pending → 用户批准后纳入指定切片；规范变化后重新审阅。
证据：pending → tested / validated / proved，必须满足对应方法与命题，不是自动晋级；
任何阶段可 failed / unsupported，二者均不计通过。
差异：open → attributed → 修复后复跑 verified；仅解释原因不等于修复。
来源 SHA、规则、投影或关键工具变化后，证据标记失效并重跑受影响切片。

保存真实 fixture 于所属测试目录。文档只留短账本和稳定来源，临时运行产物进仓库外或
`.local-artifacts/`，遵守[证据保留](../../development/testing.md#证据保留)。
