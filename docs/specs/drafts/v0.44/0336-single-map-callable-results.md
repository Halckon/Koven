# SPEC-0336: Single 函数值的 owned Map 结果交付

> **性质**：既有调用能力前置草案 · **状态**：draft · **读取时机**：闭合Single具体函数值Map结果ABI时 · **唯一真源**：本片未实施目标；语义以Guide为准

| 字段 | 值 |
|---|---|
| 状态 | draft |
| Goal ID | `KOV-P4-0336` |
| 前置 Spec | [SPEC-0331 provider范围对账](../../active/0331-map-borrowed-iteration.md)；从实际合并main另建分支，当前不实施 |
| 语言规范 | [Guide07函数值与闭包](../../../guide/07-calls-lambdas-closures.md)、[Guide12 Map](../../../guide/12-collections-destructuring.md)；不新增语义 |
| 阻塞项 | 具体callable的owned Map结果、调用ABI及唯一交付设计/验收尚未完成；不借此扩大动态interface或借用结果 |
| 当前状态 | 原无For调用已失败，独立于0331 provider；保留全部原输入与成功目标 |

## 1. 原源码与实际前置

真实非捕获lambda调用nongeneric `produce(argument())` 返回非空Map/MutableMap；entry保存具体
函数值，分别使用noFor size、named For、direct temporary For三形态。完整六源与golden由
[真实fixture目录](../../../../crates/lang-codegen/tests/fixtures/map-provider-births-0331/)保留；
原字节不因后续provider修复而替换，Single六项不是预期拒绝测试。

首轮12条观察实际2条完整SSA/LLVM、10条失败：Unit两noFor成功、四For在provider门Unsupported；
Single两noFor在 `factory()` 普通调用处Unsupported，四For更早在provider准入停Unsupported。
Rust真实0通过/1失败/0忽略、exit101，没有native。Unit四个For出生阻断归0331修复，不能转交本片；
Single的已知普通调用前置不证明未来For一定支持，也不豁免其后组合验收。

| 原源 | source SHA-256 | golden SHA-256 |
|---|---|---|
| `function-value-Map-no-for` | `be7e6a45815ebc61ac82971d3db258729cc3d87ba54a7a6e10537db6163eb26c` | `da95d1b454482b975db858cc6ccddbde531aa368f8d1a6074d89eeb6b1258046` |
| `function-value-Map-named` | `a0d712cf70babc5e128bee9d654eb48a60bc2a89578e08e00f6447c393c592a7` | `85884605638547f61c83ef880ee8018dfe5283e9a88cb91dfee0cbd222e63f65` |
| `function-value-Map-direct` | `0ed7690b79bb45cc709c6b96bfcfb8319f7f6ced65cb4e2d2c3aeea76ac96ce0` | `85884605638547f61c83ef880ee8018dfe5283e9a88cb91dfee0cbd222e63f65` |
| `function-value-MutableMap-no-for` | `d77720737b2d1a7c134f945be29bbd85ea9c5d696c0b2cf24171e13915adb17f` | `da95d1b454482b975db858cc6ccddbde531aa368f8d1a6074d89eeb6b1258046` |
| `function-value-MutableMap-named` | `f8eea8ea5246db99025b3c064b16c087c2a858675daf1c1126f2969fb43ac96a` | `85884605638547f61c83ef880ee8018dfe5283e9a88cb91dfee0cbd222e63f65` |
| `function-value-MutableMap-direct` | `bb44b84cc13d392d174419ba79f72e0ac9951ec5347ff45a339cb5cdb31c09f9` | `85884605638547f61c83ef880ee8018dfe5283e9a88cb91dfee0cbd222e63f65` |

## 2. 有界实施与硬成功目标

- [ ] Single支持该已合法具体callable的零参数owned Map/MutableMap结果，沿原callable实体/真实函数类型及实例锚定；不得按变量名或fixture文本特判。
- [ ] 普通callee保持原参数求值一次、Int/Boolean真实overload选择、Borrow/CallReturn结束及owned结果唯一返回；不扩Single所有旧函数值调用或动态interface。
- [ ] 原六源×双入口12项全部成为完整SSA/LLVM与严格native成功目标；保留Unit六条回归，分别核noFor与两种For出生/current owner/完整incoming及唯一清理。
- [ ] 同分析FE与实际FunctionAddress/CallableInvoke、thunk返回owner身份可追溯；noFor失败不得改成provider资格拒绝通过。
- [ ] 原golden、exit0、空stderr、独立审查、工程门和精确新头CI分别验收；本机不运行故障、校准或sanitizer对照。

本草案不声明上述结果已支持，不改变Guide、Field/Inout provider、nullable ABI或D10。
首红仓外收据 `f-final-births-first-attribution.json` 与 `/tmp/koven-function-value-first-attribution.log`
保留原12条阶段与位置；后续必须记录真实修复，不按Unsupported类统一免除。
