# Parser 验证接口候选合同

> **性质**：非规范测试接口设计 · **状态**：未启用 · **读取时机**：实现适配层、oracle 与报告时 · **唯一真源**：本页维护测试边界，不改变生产接口

## 1. 被测接口

现有 `parser::parse_file/parse_expression/parse_declaration/parse_block` 接收
`&SourceMap + &LexedFile`，返回 `Result<Parsed*, ParserInternalError>`。
本计划不新增 CLI/网络协议，不改变 Rust 公共 API；接口是验证 harness 与现有库的适配边界。

概念适配为 `observe(baseline, case, budget) -> observation`。
实体字段见 [data-model](../data-model.md)。生产公开入口预算是固定值；报告记录实际值，
不能假装已经支持外部预算注入。

## 2. 接受与拒绝

1. 先验证输入身份与词法接口合同。
2. 再运行所选入口并分类内部错误。
3. 对 Ok 检查合并诊断、可达 Error、入口完整消费与根结构。
4. 单独执行 Span/owner 断言。
5. 最后比较独立预期和规范化 AST。

资源耗尽、超时、环境故障、零命中不计语法拒绝或成功。
file 入口不要求唯一声明根；独立表达式等入口不允许把 trailing token 当作成功。
四分类属于观察层，不声称生产已有同名枚举。

## 3. Oracle 与可信边界

| Oracle | 必查性质 | 不得推出 |
|---|---|---|
| 规范审阅的正反金标准 | 已裁规则的接受/拒绝、结构、边界 | 所有输入 soundness |
| 文法推导生成 | 指定范围内的正例接受与 AST 对应 | 仅正例证明没有误接受 |
| tree-sitter 支持交集 | 合法输入接受/结构差分，显式列差异 | tree-sitter 输出就是规范 |
| 打印/格式化往返 | 已选范围内 AST 保持与 Span 有效 | 原始 parser 与 printer 不可能同错 |
| 恢复矩阵/资源测试 | R1–R5 的见证、预算与复杂度观测 | 所有非法输入恢复已被证明 |

共用 lexer 只能验证相对该词法产物的 Parser；共用 normalizer 须另有独立金标准。
变异输入不自动等于非法输入；需规范反例证明拒绝预期，其余只检查恢复安全/分类一致性。
pending、failed、unsupported、未解释差异必须单列，不可混进“通过”。

## 4. 文法证据与版本合同

每份报告绑定 source SHA、Guide 版本/hash、grammar/encoding/normalizer 版本、
toolchain/flags、scope、budget。分别保存原始冲突和消解后结果。
G1 表示明确范围的机器规范；G2 表示该范围的机械解析性质；
G3 表示实现一致性的工程证据；G4 必须有精确生产实现精化关系的机器检查证明。
G3 不是数学正确性证明，有限展开不得标为无限输入 G4。

新报告先 advisory。转 required 前须获批覆盖范围、无未解释差异、工具和 normalizer 有独立校验、
可复现且成本可控，既有门禁保持。
本合同批准后，稳定工程段落才迁入 Compiler Contracts；验证方法迁入 development。

## 5. 调用方不回归

生产变更先检查格式化器、unit 名称分析、CLI bootstrap 的直接消费路径。
保持格式化器拒绝错误输入、名称分析可保留 recovery snapshot、CLI 不重复解析用户文件等既有合同。
如果 F2 改变 LexedFile 对外形态或跨 crate API，先独立 ADR/API 设计，再按测试指南追加下游验证。
