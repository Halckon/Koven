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

不扩大 owned inline direct-field、generic projection、多级 loan 投影、capture 或字段来源的结果延续、
Inout、CFG、stdlib 或 method/receiver 支持；非泛型一级参数投影是本片实现边界，
不是新的语言限制。一级 Copyable 字段 Read 后允许沿既有 owned AggregateProject 继续读取，
不按源码点号数拒绝自然组合；这不产生多级 child loan。结果调用只拒绝该字段 child 实际
成为 Borrow/Carrier 唯一来源的情况，非来源参数仍按同步调用结束。真实 source-qualified
call descriptor 的 origin、参数映射与实参身份决定该区别，不按实参位置猜测。
现有表示能表达的字段按能力处理，禁止 String 白名单。
不更改 Guide、frontend、SSA model/verifier 或 LLVM ABI。

## 4. 验收与交付顺序

先提交真实失败的普通源码测试，再做最小生产实现。真实 frontend 必须无诊断，
正向测试断言成功，unit 首轮 UnsupportedNode 为缺口证据，不把期待失败当作红测。
重复 callee 字段访问、caller 重复借用与最终整体移交锁定 child/parent/owner 身份；
owned inline direct-field、generic 和 MoveOnly 多级投影仍拒绝；借用结果来源、借用结果
receiver、普通局部 capture 及捕获外层 Borrow 参数不得被新路径放行。
旧 Copyable capture 读取回退与 Class capture 字段读取由独立源码对照保持。

SSA 检查真实未改写产物与 verifier；native 只经原版 LLVM/object/link/run，检查 stdout、
stderr 与退出状态。stdout 不宣称动态精确 free 次数。普通源码外的故障注入、校准、
sanitizer、bad IR、aggregate（含 mock/stub 驱动）及 full discover 留 CI，不在云上运行。

## 5. 唯一验收账本

合同提交 `89462713`；首轮 tests-only `2bd1cd09`；边界与既有能力对照追加 tests-only
`4ff912c8`。三份提交均先于生产实现提交。首轮真实红为 5/8 通过、3/8 失败；
增强测试在撤去全部生产补丁的真实基线上为 7/11 通过、4/11 失败：unit 两字段、
Copyable 组合及 unit native 的第一子例在 lowering UnsupportedNode / native UnsupportedSource。
frontend 通过，失败不冒称 object/native 执行；该矩阵第一失败之后的子例没有执行。

最终新测试选择：`cargo test --locked --offline -p lang-codegen --lib borrow_value_field -- --nocapture`。
该选择共 11 项，包含两个 native 矩阵；single 为 5 个、unit 为 7 个普通源码子例。
新增 lib 模块由既有 `check_core.sh` 的完整 codegen target 自动覆盖，静审确认
103 个显式 frontend targets / 10 次 Cargo 调用不变；没有运行 aggregate 或其 stub 测试。

相邻定向选择均使用 `cargo test --locked --offline -p lang-codegen --lib <exact> -- --exact --nocapture`：

- `ssa::unit_field_borrow_tests::unit_field_borrow_parameter_projects_child_without_consuming_parent`：Class basic/constant 两路径，原版 LLVM/link/run，无计数修改。
- `ssa::unit_field_borrow_tests::unit_field_borrow_keeps_nested_generic_inline_and_owned_parameter_boundaries`：6 个普通源码边界 × basic/constant。
- `ssa::unit_lower_closure_tests::shared_capture::borrowed_closure_keeps_its_source_owner_until_the_closure_drop`：普通源码 capture 身份与原版 LLVM。

Linux x86_64、Rust 1.96.0、LLVM 21.1.8；macOS 与远端 CI 未运行。native stdout 只证明
真实 payload/控制路径，精确 owner/loan 消费由源码 SSA 与 verifier 断言证明，不称动态内存计数。
最终 11/11 通过（1446 filtered）；上述相邻三个 exact 各 1/1 通过（1456 filtered）。
`cargo fmt --all -- --check`、`cargo clippy --locked --offline -p lang-codegen --all-targets -- -D warnings`
通过；尺寸检查显式 base 为 046ee221，新增及生产修改文件均低于 1000 行，不新增增长豁免。
原有 38 个超限文件作为历史欠账报告。docs 最终检查通过（661 Markdown）且 `git diff --check` 通过；双宿主 CI、归档和发布仍待 root 统一执行。

## 6. 准备中实际发现的既有边界

正常源码 single `show((value.text))`（value 为 Borrow value-class 参数）通过 frontend，
但在 lowering 返回 UnsupportedNode；直接 `show(value.text)` 已支持。本片保留两次直接字段
访问对照，不修 single Group MoveOnly 字段实参；Copyable Group Read 对照保持。
该首轮额外失败与随后收窄输入的日志保留在仓库外交付材料，不将其混入本片已支持范围。
借用返回 callee 的含 println 普通 block body 也在两入口既有 lowering 拒绝；非来源参数正例
使用现有直接表达式返回合同，不扩 callee body 范围。

## 7. 实施中收口与失败保留

共享 helper 同时服务普通结果与 carrier：初稿曾让字段成为 Borrow result 来源，普通源码
边界测试意外成功；后来只拦普通结果的草稿又使短期 receiver carrier 源码走到 InvalidSsa。
两者均在最终来源身份门收口：Borrow/Carrier 的 Parameter 与显式 Receiver 按真实 descriptor
匹配，透明 Group 不改变身份；只结束非来源同步 child。Class 旧路径不受此门影响。
这些均为未提交生产草稿的正常源码检查，不曾手造、改写或执行坏 IR；完整失败日志保留。
另外两次测试端编译错误（Program 的 Debug 要求、测试操作名拼写）及一次控制 fixture 的
缺少 else 解析错误已修正并保留日志；不把它们冒作语义红。
