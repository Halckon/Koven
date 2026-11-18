# Parser DiffTest：首片要求与实施计划

> **性质**：非规范实施计划 · **状态**：未启用，F4c 首片准备中 · **读取时机**：拆分或实施 Formatter / Parser 差分测试时 · **唯一真源**：本页维护首片范围、验收与执行顺序

## 1. 范围

检验 Phase 1 的一个关系：对已选完整文件正例 `P`，`parse_file(P)` 与
`parse_file(format_source(P))` 都应接受，且产生相同的可观察语法结构。
这是变形测试：两边共用生产 Parser，不能充当独立 oracle，也不证明文法无歧义或 Parser 正确。
不涉及类型、所有权、Native Codegen 或 CLI。保留现有几类词法、可解析性和幂等断言，不重复搭建 runner。

首片从 `crates/lang-frontend/tests/fixtures/phase1/parser-file-pass/multiple-declarations.ko` 开始：
它覆盖 package、import、有序顶层声明、整数初始化与空函数体。该 fixture 已被格式化测试发现，
不复制源码到第二份 corpus。用第二条 import 或 marker 作为比较器负控时，可以使用短内存源码。
首片不覆盖所有 AST 节点，不接纳依赖 D1–D3 裁决的构造。测试范围扩展须逐项登记，不能静默跳过未支持节点。

## 2. 要求与验收标准

| ID | 要求 | 通过标准 |
|---|---|---|
| DT-01 | 两臂接受同一完整文件关系 | 原文及格式化文本分别 lex 和 parse，均无错误诊断或恢复 Error，并满足 `parse_file` 的完整入口合同。内部错误、资源不足和超时不能计为接受 |
| DT-02 | 观察结构及源码身份 | 保留 package、按序 import、按序根声明及其受支持的子树；保留节点种类、typed edge、标识符与字面量拼写、显式 marker。位置与 arena ID 不参与比较；先从各自 SourceMap 取字面身份 |
| DT-03 | 位置合同另外检查 | 两臂分别核对 SourceId、有效 UTF-8 字节边界及字段对应的源码切片；不能从 AST 等式或 root Span 推断完整消费 |
| DT-04 | 比较器有独立正负控 | 整体位置平移或已知无影响的格式改动相等；只改一个名称、整数拼写或语法 marker 必须不等；文件头与声明顺序变化也必须可见。两个输入先验证属于声明范围，不用生产输出生成预期 |
| DT-05 | 失败可定位、可复现 | 报告 fixture/case ID、对照两臂及首个不同的结构路径，或明确报未支持节点和位置；复跑使用相同输入、main SHA 与工具链。最小回归 fixture 留在 frontend 测试目录，不提交原始 dump |
| DT-06 | 现有契约不退化 | `formatting` 完整测试目标继续通过，原词法、无诊断及幂等断言保持；对照用例实际命中，不以空 corpus、ignored 或过滤代替验收 |

选择 AST 投影时参照 [数据模型](data-model.md) 和 [验证合同](contracts/validation.md)。
禁止以删除所有 Span 后的 Debug 文本或仅比较节点 discriminant 作为合同实现：
现有 `parser_entry_matrix.rs` 的 `EntrySyntaxShape` 是类别清单，不验证拼写、边或文件头，
不应被提升为完整结构比较器。先在 `formatting` 测试目标内实现最小的测试私有投影；
遇未支持节点显式失败，只在出现第二个实际使用者时考虑抽取共享代码。

## 3. 顺序任务与检查点

| 任务 | 前置 | 修改面 | 验收及退出条件 |
|---|---|---|---|
| F4c.0 固定基线 | 无 | 只读测试边界与 fixture；在规划工作树跑已有 `formatting` 目标 | 记精确 SHA、工具链、测试数；验证原测试目标已覆盖选定 fixture。若基线失败，暂停，先区分环境失败与既有回归 |
| F4c.1 建首片 Spec | F4c.0 有记录 | 从执行时 `main` 另开 `feature/spec-<id>`；登记范围、编号、DT 验收账本与实际选择 | 按仓库规则确认 Spec 状态及批准范围；不把本候选计划当作 active Spec。现行语法不变，不等 F3 或 F0 返工归因完成 |
| F4c.2 红测与投影 | F4c.1 | `crates/lang-frontend/tests/formatting.rs` 及必要的测试私有模块 | 先写 DT-04 的正负控和未支持节点断言，记录预期失败；最小实现 DT-02/DT-03 后重跑同一测试，验证比较器不会吞掉不同源码身份 |
| F4c.3 接入真实往返 | F4c.2 | 在同一 test target 增加显式首片 fixture，复用加载、格式化和解析路径 | 通过 DT-01、DT-05、DT-06；对测试侧构造的结构变化确认报告命中首个差异，随后恢复；不通过改生产 Parser 或 Formatter 让对照变绿 |
| F4c.4 交付与扩展判断 | F4c.3 | Spec 验收账本和必要文档；不新增 required CI job | 跑完整目标和必要的 fmt、clippy、docs 检查，独立审阅语法观察范围与失败分流；先核实既有 CI 是否执行该目标，再决定是否需要接线并走 PR/CI/归档 |

运行命令：

```bash
cargo test --locked -p lang-frontend --test formatting
```

**F4c.0 已完成**：`6cc03907a1bf37d3cdc9ce6e6ccba1a15326ed6b`，
Rust `1.96.0-aarch64-apple-darwin`，上述命令实跑 2 passed、0 failed、0 ignored、0 filtered。
`multiple-declarations.ko` 位于现有 `parser-file-pass` 发现范围，完整语料测试通过。
这次只验证既有测试基线，不验证 DT-01 至 DT-06 已实现；下一项是 F4c.1。
执行期间若遇合法输入的往返差异，先保存最小用例并区分 Formatter、Parser、投影或未裁决条文问题。
范围外生产修复另起 Spec，不扩张首片。

## 4. 后续引入顺序

1. 首片交付后，以一类完整语法构造为单位扩展投影。既有正例、换行及 trivia 矩阵保留其原责任。
2. F4a 在 F3 有可追溯文法时增加独立推导与生产对照；F4b 在明确共同子集后增加 Tree-sitter 报告，先 advisory。
3. Base/Head 对照在投影稳定后另行试验。结果区分 `expected`、`unexpected`、`needs-review`，基线不是规范。
4. Native 优化配置对照、所有权事件与 PR 覆盖反馈分属后续 Spec。没有第二条已落地的复用路径前，不新建 `tools/difftest/` 或 workspace member。
