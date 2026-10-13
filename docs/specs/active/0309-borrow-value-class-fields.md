# SPEC-0309: 普通 Borrow value-class 参数的直接字段读取

> **性质**：变更合同 · **状态**：in-progress · **读取时机**：实施或验收普通 Borrow inline 参数字段时 · **唯一真源**：本 Spec

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P3-0309` |
| 所属 Phase | Phase 3 lowering，Phase 4 native 验收 |
| 语言规范 | [Guide08](../../guide/08-class-family-members.md)、[Guide10](../../guide/10-ownership-borrowing-drop.md) |
| 批准依据 | 2026-10-13 用户“继续实施”；普通 Borrow value-class 字段的有界实现缺口 |
| 前置 Spec | [SPEC-0264](../../archive/specs/0264-unit-direct-field-borrow.md) |
| 前置 ADR | [ADR-0016](../../adr/accepted/0016-interprocedural-borrow-abi.md) |
| 阻塞项 | 本地红绿验收、独立审查、双宿主 PR CI 与发布待验收 |
| 影响范围 | unit lowering 与普通 source/codegen 回归 |
| 语言语义变更 | 否 |

## 1. Goal 与输入

基于 `046ee221c29f8a84d2d9fb56c945751f07e7d6d8` 的干净 `feature/spec-0309`，
补齐 compilation-unit 中普通 Borrow 非泛型 value-class 参数的一级字段访问：
Copyable 字段 Read 与 MoveOnly 字段同步 Borrow，以 String 为真实资源样例。
能力判定消费已验证类型、字段、参数与 Loan 事实，不按类名、字段名或 String 名字特化。
这是现行 Guide 已授权的 lowering 缺口，不启用新语义；SPEC-0308 独立且不修改。

## 2. 冻结合同

- 保留 source-qualified projection、root、field、target 类型与唯一 shared LoanFact 校验。
- 新 ValueClass 路径只接受真实普通参数与当前函数 entry Shared Loan 的对应关系；
  `borrow_bindings` 存在本身不构成证明。既有 Class/capture 支持保持。
- 从 parent 派生 `SharedFieldLoan`，Copyable Read 后结束 child；同步调用 child 持续到
  call 后结束。entry parent 不显式结束，不复制、移动或析构整个 MoveOnly owner。
- 复用既有 SSA/verifier/LLVM 表示与 cleanup，不能引入虚假 storage identity。
- single 既有字段能力、双入口 String clone 为独立对照，不能替代 unit direct-field 验收。

## 3. 非目标与边界

不扩大 owned inline direct-field、generic/nested projection、capture、borrow-result、
Inout、CFG、stdlib 或 method/receiver 支持；非泛型一级参数投影是本片实现边界，
不是新的语言限制。现有表示能表达的字段按能力处理，禁止 String 白名单。
不更改 Guide、frontend、SSA model/verifier 或 LLVM ABI。

## 4. 验收与交付顺序

先提交真实失败的普通源码测试，再做最小生产实现。真实 frontend 必须无诊断，
正向测试断言成功，unit 首轮 UnsupportedNode 为缺口证据，不把期待失败当作红测。
重复 callee 字段访问、caller 重复借用与最终整体移交锁定 child/parent/owner 身份；
owned inline direct-field 及相邻 generic/nested 限制仍拒绝。

SSA 检查真实未改写产物与 verifier；native 只经原版 LLVM/object/link/run，检查 stdout、
stderr 与退出状态。stdout 不宣称动态精确 free 次数。普通源码外的故障注入、校准、
sanitizer、bad IR、aggregate（含 mock/stub 驱动）及 full discover 留 CI，不在云上运行。

## 5. 唯一验收账本

合同冻结时仅完成静审与干净输入核验，尚未运行新测试。红绿输入、精确选择、实际结果、
未运行平台和 CI 来源在后续逐项登记；不以本地成功代替双宿主 CI 或实际 main 验收。
