# SPEC-0318: Map 条件 return operand 的最后使用清理

> **性质**：变更合同 · **状态**：in-progress · **读取时机**：实现或验收 Map return operand 内的最后使用清理时 · **唯一真源**：本 Spec

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P4-MAP-RETURN-LAST-USE` |
| 所属 Phase | Phase 4，保留 Phase 2/3 事实并以 Phase 5/6 验证 |
| 语言规范 | [v0.43](../../guide/README.md)、[ASAP](../../guide/10-ownership-borrowing-drop.md#asap-析构)、[顺序 provider](../../guide/12-collections-destructuring.md#373-source-loan退出清理与冲突) |
| 批准依据 | 用户已批准本轮 M2B 有界收尾顺序；本片由实际基线与独立审阅冻结，不扩大语言语义 |
| 前置 Spec | SPEC-0313、SPEC-0314（均 done） |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 先取得正常源码真实 facts 与红测；若不能唯一绑定提前清理与 Return，登记延期而不推测 |
| 影响范围 | `lang-codegen` 的 single Map carrier、普通源码测试及实现事实文档 |
| 语言语义变更 | 否 |

## 1. Goal

有界 temporary `Array<Int>` 的单 Map、单条件 Return 与互补 Fallthrough 中，返回 owned
`Int` 的 operand 最后一次 remove/contains 所发布的 ASAP Map cleanup 能保持原点消费
最新 owner，并与真实 Return 终结唯一关联；正常回边与 exhaustion 不受影响。

## 2. 冻结输入与对照

从官方 fetch 的 actual-main `b1165b0f` 独立建立 `fix/spec-0318`，不复制未合并0316/0317。
原输入为 `crates/lang-codegen/tests/fixtures/map-conditional-return-0313/updating-operand-boundary.ko`，
保留原字节。它在每轮 put 当前 item 后 remove 同 key，只能代表 present，不能代表 missing。

| 轴 | 本片合同 |
|---|---|
| 入口 | single、unit basic、unit constant 分别跑真实结果；不从 single 推定 unit |
| 普通 Map 出口 | live / dead |
| operand | 原 remove 条件；contains 最后只读；普通 return item |
| 隔离 | 无 Map nullable temporary、Map 与独立 nullable temporary |
| native | 0/1/多轮；first/last/never；remove present/missing 独立源码实例 |
| 边界 | 外围 Resource、直接 owned nullable-return、多/混合退出保留；旧 Map return 正例不回退 |

## 3. 必须证明的合同

1. 使用真实 Map SymbolId、Header owner、availability guard、DropFact 的精确点和真实
   Return expression。不能靠同类型、宽泛 lineage、源码文字或范围包含猜测对应关系。
2. Return 无 phi incoming；Entry/Fallthrough/Exhaustion 的既有互补路径和 presence 证明不变。
3. remove 先更新 Map owner，result 0 是该点最新 owner，只有一次正确消费；result 1 是
   独立 nullable owner，先读 presence 后 drop，不能混用两个 result 的身份。
4. contains 只读借用结束后在既有事实点清理 Map。返回值、Map、provider hidden owner 和
   element/source loan 分开核验；不提前结束 provider 或延迟 Map ASAP 至 return。
5. 正常路径仍将 tail 最新 owner送回 Header，live exhaustion 保留至正常 drop，dead
   exhaustion 按原点清理；Return 不进入任何回边或 exit incoming。
6. 如果现有公开事实无法唯一关联清理与终止路径，保持结构化拒绝，记录所缺合同及延期理由。

## 4. 非目标

不扩展 provider、多个 Map、Resource owner、直接返回 nullable remove 结果、多/混合或
无条件退出、owned source、通用 CFG/NLL。既有已支持形态不得因本片回退。
不修改 frontend ASAP/liveness 来延迟 drop，不放宽全局 presence，不只删除 gate。
不运行故障注入、calibration、sanitizer、aggregate、CompositionContracts 或全量 discover；
云端只运行静审后的正常源码到原版 LLVM/native 和合法 diagnostic，对应远端 CI 独立留证。

## 5. 验收标准

- [x] 精确源码基线、三入口实际结果与 Map/nullable/drop/return facts 已冻结并独立审阅。
- [x] 同一正常源码红测先失败，最小实现后转绿；原 fixture 原字节不改。
- [x] SSA oracle 验证第3节的真实身份、最新 owner、nullable consumption 与完整 CFG。
- [x] 原版 object/link/run 覆盖第2节矩阵，stdout 不冒充动态释放计数。
- [x] 原 Map return、正常 Map loop、独立 nullable 及边界诊断回归通过；命中数明确。
- [x] Architecture 更新、docs/diff/fmt/尺寸及选定 check/clippy 通过。
- [ ] 精确 head 双宿主 PR CI 与独立审阅完成后再归档；未运行不能标通过。

## 6. 实施与提交

先提交 Spec/真实 baseline，再实施 bounded carrier 证明，最后补足结构/native回归及事实文档。
每个可恢复阶段提交并生成固定 bundle；发布与合并由独立发布方执行。
工具链 Rust 1.96.0、LLVM 21.1.8；Cargo 使用独立绝对 target，串行批次前后比较源码 hash。
原始日志/JSON只保留仓外；仓内保留本合同、真实 fixtures 与测试，不复制完整证据。

## 7. 验证记录

### 真实基线与最小修复

原fixture SHA-256 `a03f52e03177ed9ebdb9267683bad14bfd611216e1ad98174c3355ca9d7ba344`
保持不变。真实facts：cache Symbol2/Header owner2，remove CallReturn20 的condition5为
Header selector0 presence与Return29 exit.condition4的合取；operand28求值后原carrier已有
snapshot。Return actions只有provider/source清理，没有Map drop。remove nullable Temporary20
另在AfterBinaryOperands22消费。contains CallReturn20/Return27同形，问题是last-use而非仅变异。

| 固定输入 / 批次 | 实际结果 |
|---|---|
| `dac3a240` 首检查点 / baseline2 | 三测试因独立nullable factory预期错误失败；原始源码manifest可重建，失败保留，不算行为红测 |
| `220fb047` / baseline3 | SSA 3 passed / 5 failed；remove/contains×live/dead四正式goal均UnsupportedNode(cache46..51)；第五是no-map nullable factory旧拒绝，后锁精确声明Span |
| baseline3 独立绿对照 | unit basic/constant×remove/contains/item×live/dead十二格verified LLVM；single item两格成功，不称新增能力 |
| `dc1dccd3` / implementation1 | 同一SSA模块8/8通过；四goal转绿；原unit/item绿及独立nullable边界保持 |
| `ae113bc0` / oracle1 | conditional-return SSA 30/31；唯一失败是native scaffold的named initializer被既有callback gate拒绝；未进入native |
| `5ad58365` / oracle3、native1 | 改既有inline lambda，独立seed oracle只允许透明block参数；SSA31/31、三入口原版native3/3通过 |
| `5ad58365` / shared1 | Map loop-owner SSA68/68、nullable cleanup3/3、旧正常Map loop-owner native52/52；全目标check与严格clippy通过 |
| oracle4 | SSA32/32；新增P3同guard但operand外反例、selected→Map drop完整CFG断言及两Map/Borrow Array拒例通过 |
| final1 | 声明Span断言补强后SSA32/32、三入口native3/3、严格clippy通过；docs/fmt/diff/尺寸通过，未提高cap |

首轮harness曾因Program缺Debug、native helper导入路径、fmt注册排序失败；均单独修复，
不冒称目标行为红测。inline initializer首轮还暴露测试oracle错误要求construct与preheader同block，
已改为独立逐edge透明转发校验，生产不变。上述失败输入、原日志和逐文件manifest均仓外留存，
恢复点保留在各固定提交；Spec只保留结论，不复制日志。

生产只增加精确活动Return身份：lower_return在operand前登记实际Return ID；原CallReturn
清理当场核MapRemove result0或MapContains owner与当前binding，原点消费。operand完成时
须已消费，最终核开始点→drop→对应真实Return全后继无环汇合。旧ControlTransfer路径优先，
正常owner/phi检查保留；frontend ASAP/liveness、global presence、SSA verifier、LLVM/runtime不改。

### 结构、执行与边界

独立SSA oracle按真实call/Return origin核最新Map唯一drop、remove独立result1 presence读取后
唯一drop、与hidden source owner互异、element/source顺序、正常tail最新回边及Return无incoming。
同guard拒例另从真实P3独立锁唯一Map Symbol/Header、CallReturn contains receiver loan、
精确Header presence∧Return path、互补Fallthrough与operand外位置，不调用生产owner_plan自证。

native fixture有17个调用（remove12、contains5）：0/1/3长度、first/last/never、live/dead代表格，
不是笛卡尔全集。实际发生return的remove present3次、missing3次；never不计missing。
每入口stdout逐字匹配105行，source initializer共32次；这不是动态释放计数。
lookup直接传Int，原operand内item+offset的checked Abort形态单列MissingFact(cache46..51)，
不放宽全部后继必须达到cleanup/Return的gate。

保留并验证的边界：独立nullable factory single无Map声明0..97、有Map cache144..149，
unit有无Map均factory声明10..107；外围Resource、直接owned nullable返回、多/混退出；
新提前清理分支的List、owned/Borrow Array、两Map、Boolean结果。普通旧Map return成功不回退。
同guard operand外和两Map均UnsupportedNode(cache46..51)，Borrow Array为cache64..69，
Boolean结果为cache50..55；均锁真实声明Span，不把拒绝说成语言禁止。

### 输入身份与剩余验收

源码manifest范围为全部crates的`.rs`/`.ko`，路径按字节排序逐文件SHA-256再取汇总；每批前后相同。
关键汇总：baseline2 `e3f90ced…3136`、baseline3 `3bdbe361…316`、implementation1
`ac643c41…d487`、oracle1 `1ba0c222…78c3`、oracle3/native1/shared1 `8c2fbe7b…a22c7`、
oracle4 `895407b2…2351b`、final1 `3c6aeb8a…2561`。完整值与命令在仓外固定manifest，不把后续修改归属到旧运行。

主干正常整合后的联合回归、独立最终审阅与双宿主PR CI待完成；
在这些完成前保持in-progress，不提前归档或宣称可合并。
