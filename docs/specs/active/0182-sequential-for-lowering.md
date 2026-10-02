# SPEC-0182：顺序容器 `for` frontend→SSA→native 集成

> **性质**：实施 Spec · **状态**：approved · **读取时机**：实施或评审对应阶段 Goal 时 · **唯一真源**：本 Spec

## 1. 元数据

| 字段 | 值 |
|---|---|
| 状态 | `approved` |
| Goal ID | `KOV-P4-182` |
| 所属 Phase | Phase 4 |
| 语言规范 | [现行 v0.37 §37](../../guide/12-collections-destructuring.md#37-借用式顺序容器迭代-provider) |
| 批准依据 | 2026-09-19 用户明确启用 v0.37；按持续 Goal 顺序推进 |
| 前置 Spec | SPEC-0034、0036、0179、0184、0192、0195、0211、0212 `done` |
| 前置 ADR | [ADR-0023](../../adr/accepted/0023-borrowed-sequential-iteration-provider.md) `accepted` |
| 阻塞项 | 无；前置均已满足，当前顺序切片 |
| 影响范围 | `lang-codegen` frontend lowering/SSA/LLVM/native tests；Architecture |
| 语言语义变更 | 否；实现启用后的 v0.37 executable `for` |

## 2. Goal

完成后，真实 Koven `for` AST 只消费 validated typed/ownership/provider facts，lower 为无分配的
preheader/header/body/continue/exit SSA CFG，并对三种顺序容器、名称/discard/解构及全部退出路径
完成 LLVM object/link/run 闭环。

## 3. 范围与需求

- source 在 preheader 精确 lower 一次；按 ownership plan 建立/复用 source loan 与 temporary owner，
  length snapshot 一次，cursor 从零开始。
- header 先比较 cursor/length；body edge 建立 current element place/loan 并安装名称或 value-class
  component Borrow bindings。`_` 不生成无用 owner/binding。
- Copyable binding read 生成 copy，MoveOnly 只作为 loan 使用；lowerer 不读取 copyability 来重推所有权，
  不从 container 移出 element。
- normal/continue edge 按 facts 逆序 drop body-local owner/结束其派生 loan，再结束 element/component
  loans、递增 cursor 并回 header；break/exhaustion 随后 finish provider、结束 source loan 并在
  要求处 drop temporary source。return 先交付 operand，再按 body-local → element → provider →
  source → temporary → outer-scope 顺序清理。
- 扩展 loop control，使 break/continue 消费 `DropPoint::ControlTransfer` 与 iteration cleanup facts；
  最近 loop/nested provider 顺序精确，return 沿用先求值返回 operand 再 cleanup。
- lowerer 对缺失/mismatched typed、ownership 或 provider facts 在 LLVM/object 写盘前 fail loud；不按
  source 类型名、`iterator` 方法或 AST 形状重推 provider。

## 4. 非目标

- 不实现 Inout 或 field source native lowering（等待一般 source place lowering 后继 Spec）、receiver/
  公开容器 API、Map/range/String/IO/user-defined/consuming iteration、iterator object 或新 runtime ABI。
- 不改变 SPEC-0179/0211 诊断语义、SPEC-0212 provider verifier 或现有 container layout/drop glue。
- 不实现循环优化、bounds-check elimination、vectorization 或 allocation elimination。

## 5. 验收标准

- [x] owned named、Borrow 参数与 temporary source 的 Array/List/MutableList，empty/single/multi 及 source-call-once 正例通过。
- [x] Int Copyable binding、String/nominal MoveOnly Borrow binding、`_` 与 mixed value-class 解构产生
  预期 SSA loan/read/projection 且无 element consume。
- [x] normal/continue/break/exhaustion/return/nested loop CFG 的 body-local/derived/element/source loan
  结束与 owner drop 顺序由 SSA/render 断言锁定；temporary source 每条可达退出精确 drop 一次。
- [x] named source 循环后可读；compile-fail 的 move/mutate source、MoveOnly binding consume/return、
  escaping capture 由 frontend 既有 L0133/L0135/L0137/L0138 拒绝且不进入 lowering。
- [x] native stdout 覆盖递增顺序、Unicode String Borrow、continue/break/early return；ZST 逻辑次数和
  container/element drop 次数准确，无 iterator runtime symbol/allocation。
- [x] malformed/mixed analysis product 在落盘前失败；SSA/LLVM 文本确定，受影响契约回归通过，
  Architecture 与实现事实同步。

## 6. 技术方案与边界

在 `lower_frontend::loop_control` 增加只接受前置阶段 validated facts 的 sequential `for` path。
source owner/loan、length 与 cursor 显式作为 CFG entities 传递；borrow bindings 使用现有 loan
binding map，value-class projection 复用 `SharedFieldLoan`，drop emission 只查 SPEC-0211 facts。
LLVM adapter 不认识 AST。

## 7. 实施计划

1. [x] 接 preheader/header/body 与名称/discard binding → 验证：SSA CFG/loan 窄测。
2. [x] 接 borrowed destructuring 与全部 jump cleanup → 验证：projection/drop/nested 矩阵。
3. [x] 完成 LLVM/native/ZST/determinism 与 Architecture → 验证：object/link/run 及受影响的下游契约检查。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | frontend→verified SSA integration | `feat(codegen): lower sequential for loops (SPEC-0182)` |
| 2 | LLVM/native矩阵与完成文档 | `feat(codegen): run sequential for loops (SPEC-0182)` |

## 9. 未决问题

- 2026-09-19 用户已决定 temporary source 纳入首轮 native；§3/§5 延寿、source-call-once 与逐退出 drop 验收完整保留。

- Inout/field source 的 native lowering 等待一般 source place lowering 后继 Spec；不阻塞本 Spec
  对 owned named source、Borrow 参数与 temporary source 的首轮 executable 闭环。其他 provider 和优化由后续
  guide/Spec 独立推进。

## 10. 验证记录

实施前按[分层验收](../../development/testing.md)将第 5 节各项映射到实际测试目标/过滤器；
记录命中数、结果与未运行原因。同一状态下的有效证据只运行一次，不默认运行 frontend 全量。

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-27 roadmap/实现审计 | 通过 | loop CFG/checked element/borrow ABI 可复用；当前 lowering 明确拒绝 Statement::For |
| 2026-10-01 SSA lowering 矩阵集成 | 通过 | 7 个 SSA lowering 测试全绿，覆盖单/多/空、discard/destructuring、break/continue/return 与嵌套循环 |
| 2026-10-01 native执行闭环与全量回归 | 通过 | 8 个端到端 native 用例全绿，全量 475 unit + 4 doc-tests 通过 |


2026-09-19 候选重基核对：保留现行 v0.36 的 grammar、nullable/Nothing、所有权与常量契约，
拟议版本取代关系见 proposal。仅更新基线与状态前置，不改变本 Spec 的阶段范围、验收条目或
批准状态；guide 启用与 ADR 接受仍是实施前置。未运行 Rust 测试（本次仅文档）。

2026-09-19 启用记录：v0.37 已启用、ADR-0023 accepted；temporary source 纳入首轮 native。
上方重基时的未启用说明是历史记录，不再是当前阻塞项；实现/验收尚未完成。

2026-10-01 前置满足与激活记录：SPEC-0211 已完成全部 Phase 3 验收并归档为 done，阻塞项消除；
本 Spec 正式激活为 approved 并进入 Phase 4 实施阶段。

2026-10-01 Phase 4 实施与验证完成：完成 sequential for lowering 到 typed SSA，修复循环体内借用槽复用与嵌套循环借用传递；通过 8 个端到端 native 编译执行用例（覆盖 Array/List/MutableList、借用解构、临时容器、嵌套循环、break/continue、提前 return 与 Unicode String 借用遍历），并完成 lang-codegen 全量 475 个单元测试无回归验证。


### 2026-10-02 temporary source 边界的部分验收补测

本片基于 `main 7318d53e4c5677684630e2b0b9148e9c6ceb1c35`，测试提交为
`537586d0cb5529b5558f9fe7b4172ee9ec0f892c`。仅新增测试，不改变生产 lowering、语言语义或
上方历史记录。本 Spec 继续留在 active；以下证据只补强 §3 / §5 第一项的 temporary source
子矩阵及其基本 CFG 合同，不将原有勾选或历史全量记录当作其余验收缺口已关闭的证据。

新增测试都属于 `lang-codegen --lib`，无 `ignore` 或平台过滤：

- `native_tests::sequential_for_tests::temporary_array_source_boundaries_run_natively`
- `native_tests::sequential_for_tests::temporary_list_source_boundaries_run_natively`
- `native_tests::sequential_for_tests::temporary_mutable_list_source_boundaries_run_natively`
- `ssa::sequential_for_lowering_tests::temporary_source_boundaries_keep_call_and_length_in_preheader`

前三个测试各执行三格 object/link/run，最后一个表驱动测试独立分析同样九格。每格 ID 为
`temporary_<provider>_<cardinality>.ko`；provider 为 `Array` / `List` / `MutableList`，
分别使用 `arrayOf<Int>` / `listOf<Int>` / `mutableListOf<Int>`。精确输入模板及固定表保存在
[native 测试](../../../crates/lang-codegen/src/native_sequential_for_tests.rs)和
[SSA 测试](../../../crates/lang-codegen/src/ssa/sequential_for_lowering_tests.rs)，失败输出包含
case ID 和完整源码。三个 cardinality 的实参与独立、手写 stdout bytes 如下：

| cardinality | 构造实参 | 固定 stdout（`\n` 表示一个 LF） |
|---|---|---|
| `empty` | 空实参，显式 `<Int>` | `source\ndone\n` |
| `single` | `7` | `source\nbody\nseven\ndone\n` |
| `multi` | `7, 2, 9` | `source\nbody\nseven\nbody\ntwo\nbody\nnine\ndone\n` |

native 九格还逐一要求成功退出、stderr 为空。`source` 锁零次迭代也调用一次工厂，`body`
锁实际迭代次数，元素 marker 锁非单调顺序 `7,2,9`，`done` 锁正常耗尽后的后继执行；期望值
不从源码元素表、SSA 或生产 builder 计算。复用既有 `emit_link_and_run`，原八个 native
和七个 SSA 测试体保持原样。

SSA 九格复用 `analyze` 完成 frontend diagnostics 为空、lowering 与 verifier；按函数名和
FunctionId 找 `source` / `run`，按 entity、block parameters 与 edge arguments 验证：
source DirectCall / ContainerLength 各唯一且位于无入边的 entry/preheader，按
call → RootPlace → shared BorrowBegin → length 顺序使用同一 owner/loan；零 cursor 与唯一
length 结果运输到 header，`cursor < length` 真边是 body 的唯一入口；唯一 element place
使用真边运输的 source loan/cursor；正常回边保留 length snapshot/source loan，并将 cursor
加一。不固定 block 序号，不用生产 provider builder 生成测试期望。

| 实际命令 / 检查（2026-10-02，退出码均为 0） | 结果 |
|---|---|
| `timeout --signal=TERM --kill-after=10s 300s cargo test --locked --offline -p lang-codegen --lib sequential_for` | 19 passed / 0 failed / 0 ignored / 664 filtered；原 15 + 新 4，新增 native 9 格及 SSA 9 格均执行 |
| `timeout --signal=TERM --kill-after=10s 120s cargo fmt --all -- --check` | 通过 |
| `timeout --signal=TERM --kill-after=10s 600s cargo clippy --locked --offline -p lang-codegen --all-targets -- -D warnings` | 通过 |
| `python3 scripts/check_rust_sizes.py --base origin/main` | 实际 merge-base 为上述 main；596 手写文件 / 49 历史超限 / 0 生成物，两个修改文件为 290 / 396 行，无新增例外 |
| `python3 -m unittest discover -s scripts/tests -v` | 94 passed，包含尺寸 policy 47 项 |
| `python3 scripts/check_docs.py`、`git diff --check` | 通过 |

执行宿主为 x86_64 Linux，Rust 1.96.0、LLVM/Clang 21.1.8；使用既有共享 Cargo target、
`CARGO_INCREMENTAL=0`，Cargo 命令串行，libtest 默认并行，未 clean。首轮定向即通过，
没有生产红测或修改 expected 迁就实现。macOS 与本片 PR 双宿主 CI 尚未运行；frontend 全量、
workspace 全量、性能/RSS 未运行，本片无生产或跨 crate API 修改。

尚未关闭：owned named/Borrow 的完整边界矩阵及循环后再读、MoveOnly/mixed projection、
逐退出 loan/drop 精确顺序与计数、ZST、真实 for 负例落盘桥接、独立分析链 SSA/LLVM 确定性。
这些条款保持原合同，不能由本片九格或共享 provider 模型的历史测试代替。

### 2026-10-02 temporary source 四类退出的部分验收补测

本片基于合并 PR19/20 后的 `main 295b9ef32bbb6d3147d370f35e6e83908564ad2a`；
本地测试提交 `0d7f389641a365454f50ee06ded79d3afa24982a`。仅增加两个领域测试子模块与小入口，
原 19 项 `sequential_for` 测试保持不变，不修改 production lowering、语言语义或 §5 合同。
本 Spec 保持 active；这里只补强一个 temporary provider 的 normal/continue/break/return，
不将八个新增测试解释为完整三容器、nested 或 derived/component loan 矩阵已经完成。

新增测试均属于 `lang-codegen --lib`，无平台过滤或 `ignore`。两个前缀分别是
`ssa::sequential_for_lowering_tests::cleanup_tests::` 与
`native_tests::sequential_for_tests::cleanup_tests::`：

| 退出 case | SSA 测试（前缀见上） | native 测试（前缀见上） |
|---|---|---|
| normal → exhaustion | `normal_exit_keeps_source_until_exhaustion` | `normal_exit_drops_body_guards_and_temporary_buffers_once` |
| continue → exhaustion | `continue_exit_keeps_source_until_exhaustion` | `continue_exit_preserves_temporary_buffers_until_exhaustion` |
| break | `break_exit_cleans_source_before_join` | `break_exit_drops_unvisited_element_buffer_too` |
| return | `return_exit_copies_operand_before_cleanup` | `return_operand_runs_before_guards_and_source_cleanup` |

[SSA 精确 oracle](../../../crates/lang-codegen/src/ssa/sequential_for_lowering_tests/cleanup_tests.rs)
独立分析四份以 `arrayOf(7, 2)` 构造 `Array<Int>` 的 source 工厂输入，
每个 body 依次建立两个带 `deinit` 的 scope-bound Guard。按 named factory 的 FunctionId、
结果 entity 和 edge arguments 跟踪 owner/source loan 到 header/body/exhaustion，不固定 block
编号或读取 frontend cleanup plan 生成期望。逐块比较完整 Drop/BorrowEnd 子序列：
`later Guard → earlier Guard → element loan`；break/return 随后为 `source loan → temporary owner`；
exhaustion 只有本层 `source loan → temporary owner`。其他 block 不得有这些 cleanup，也没有
Consume。normal/continue 回边保留同一 owner/loan/length，element loan 结束后 cursor 加一；
break 与 exhaustion 汇合到无 provider 参数、无重复 cleanup 的返回块；return 的终结值必须
来自当前 element loan 的 Copyable Read，且 Read 先于所有 body-local cleanup。SSA verifier
仍独立运行。这是各路径的结构/身份/顺序证明，不以渲染 contains 或静态 drop 总数替代。

[native 精确 oracle](../../../crates/lang-codegen/src/native_sequential_for_tests/cleanup_tests.rs)
改用当前可执行的 `Array<String>` 两个非空动态 concat 元素 `"fi" + "rst"`、`"sec" + "ond"`，
每轮依次创建 `Guard("earlier")`、`Guard("later")`。固定手写 stdout bytes 验证访问顺序、
两 Guard 逆序 deinit、break 不进入第二轮，以及 return operand 的输出先于 Guard deinit。
normal/continue 固定输出为 `first\nlater\nearlier\nsecond\nlater\nearlier\nafter\ndone\n`；
break 为 `first\nlater\nearlier\nafter\ndone\n`；return 为
`first\nlater\nearlier\nreturned\ndone\n`。每格分别运行公开 object/link/run 与 verified LLVM
计数 link/run，均要求成功退出和 stderr 为空。

计数复用 `boxed_enum_tests::run_counted_allocations`，没有新 runtime/hook 框架：normal/continue
各 7 次，break/return 各 5 次 allocation/release，预算独立来自两个 concat buffer、一个容器
buffer、每个实际 body 的两个非零 Guard payload。既有 helper 核验每个 free 对应 live pointer，
拒绝未知/重复 free，并在程序结束要求无 live pointer；libc stdout 内部分配不计入。该证据
证明这些实际 buffer/payload 唯一释放，包括 break/return 未访问元素的 buffer；不把分配计数
等同于全部 String logical drop、nominal element deinit 或 ZST logical drop 的验收。

能力探针还保留一个未关闭的正例红证据：
`class Leaf(val name: String) { deinit() { println(this.name) } }` 配合
`fun source(): Array<Leaf> = arrayOf(Leaf("first"), Leaf("second"))`，在真实 `for` 中使用时，
frontend 无诊断，但单文件 `lower_scalar_file_with_entry` 在该 arrayOf 调用返回
`LoweringErrorKind::UnsupportedNode`，尚未到 LLVM/native。本片不修改生产实现，不把拒绝写成
预期成功测试，也不删除原 §5 的 nominal MoveOnly / element drop 要求；这是后续需闭合的
具体能力缺口。原探针运行退出码 101（1 SSA 探针通过、1 native 探针失败）；其后支持子集
的探针和下列正式测试通过，不把两者报告为同一红绿修复。

| 实际命令 / 检查（2026-10-02，正式检查退出码均为 0） | 结果 |
|---|---|
| `timeout --signal=TERM --kill-after=10s 300s cargo test --locked --offline -p lang-codegen --lib sequential_for` | 27 passed / 0 failed / 0 ignored / 664 filtered；原 19 + 新 8，新增 native 4 格各运行两条 native 路径 |
| 同上 Cargo 参数，filter `ssa::container_operation_tests` | 10 passed / 681 filtered |
| 同上 Cargo 参数，filter `resource_deinit` | 26 passed / 665 filtered；包含单文件、unit 与 native 资源回归 |
| `timeout --signal=TERM --kill-after=10s 120s cargo fmt --all -- --check` | 通过 |
| `timeout --signal=TERM --kill-after=10s 600s cargo clippy --locked --offline -p lang-codegen --all-targets -- -D warnings` | 通过 |
| `python3 scripts/check_rust_sizes.py --base origin/main` | merge-base 为上述 main；605 手写 / 48 历史超限 / 0 生成物，新模块 364 / 92 行；无新增例外、无 legacy 超限增长 |
| `python3 -m unittest discover -s scripts/tests -v` | 94 passed |
| `python3 scripts/check_docs.py`、`git diff --check` | 通过 |

宿主 x86_64 Linux，Rust 1.96.0、LLVM/Clang 21.1.8；既有共享 Cargo target，
`CARGO_INCREMENTAL=0`，Cargo 串行、libtest 默认并行，未 clean。代码定向正式首轮通过。
本片 PR 双宿主 CI 尚未运行；frontend 全量、workspace 全量与性能/RSS 未运行，无生产或
跨 crate API 变动。其余完整 owned named/Borrow 边界与再读、nominal/mixed projection、
nested/derived/component cleanup、ZST、真实 for 负例落盘桥接、独立分析链确定性仍未闭合。
