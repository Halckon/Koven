# SPEC-0182：顺序容器 `for` frontend→SSA→native 集成

> **性质**：实施 Spec · **状态**：done · **读取时机**：实施或评审对应阶段 Goal 时 · **唯一真源**：本 Spec

## 1. 元数据

| 字段 | 值 |
|---|---|
| 状态 | `done` |
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

### 2026-10-02 独立分析链确定性的部分验收补测

本片基于 PR21 合并后的 `main 0b1939b471099e8759eb729ea548d7155129403d`，本地测试提交
`2a318630eb7773b6689fd21d3a4dd5adbccb9d1d`，review 后补全 origin 检查的本地测试提交为
`27c0e825a484f35824e69bdbcae966035a6b2828`。仅新增私有测试模块及一行入口，不改变生产
语义、frontend 身份校验或 renderer；本 Spec 保持 active，原 §5 合同与历史记录不改写。

新增三项均为 `lang-codegen --lib`，完整前缀为
`ssa::sequential_for_lowering_tests::determinism_tests::`，无平台过滤、`ignore` 或预期失败：

| 测试（接上述前缀） | 输入与实际覆盖 | 固定 oracle |
|---|---|---|
| `temporary_list_continue_is_deterministic_across_fresh_analyses` | `source(): List<Int>` 返回 `listOf(7, 2, 9)`；temporary source，条件 continue、累加和 exhaustion | 每轮完整 SSA / LLVM 相等；恰 1 个 ContainerLength |
| `mixed_array_destructuring_is_deterministic_across_fresh_analyses` | Borrow `Array<Parts>`，`Parts(Int, String)` mixed 解构、String Borrow println、条件 early return | 每轮完整 SSA / LLVM 相等；恰 1 个 ContainerLength |
| `nested_array_return_is_deterministic_across_fresh_analyses` | Borrow `Array<Array<Int>>`，nested for、内层 continue / return、外层 exhaustion | 每轮完整 SSA / LLVM 相等；恰 2 个 ContainerLength |

每项**两次独立调用** helper；每次新建 SourceMap 和 standard environments，重新 lex、parse、
resolve_names、check_types、check_ownership、lower_scalar_file、verify_program，再调用现有
`render_program` 与 `render_verified_program`。两轮不复用 AST、names/types/ownership、SSA
Program 或 LLVM context。Lexer/Parser/names/types/ownership diagnostics 均要求为空，全部
frontend 产品的 SourceId 必须等于本轮 source；每个函数、block、instruction、terminator，
以及 values / places / loans 的 EntityData.origin、已有 TypeOrigin 的 primary / declaration
均须仍属本轮 source，且 name / byte bounds 能由本轮 SourceMap 验证。最终还断言两轮
SourceId **不相等**，因此不是重印同一 Program 或同一分析。

稳定比较 key 是同一 case 的逻辑文件名、精确 UTF-8 字节和单源注册顺序。同一输入的 source
index 因而都是 0；现有 SourceId Debug 合同仅隐藏 map owner token。测试不删除、替换或
排序任何 renderer 文本：SSA 保留 source index / byte spans / synthetic reasons，以及 type、
function、block、entity IDs、operation、operand 和顺序。LLVM 在同宿主、target、默认 options、
固定 `main` module 下逐字比较全部输出，包括 ModuleID / source_filename、target triple /
layout、symbols 和指令；没有把环境名称差异误归为随机性再随意正规化。该证据仅限**单文件
lower_scalar_file 入口、无 debug 的 verified LLVM render**，不证明跨宿主文本、object/DWARF
字节或 compilation-unit 入口等价；后者当前仍拒绝 Statement::For。

精确输入保存在[新测试模块](../../../crates/lang-codegen/src/ssa/sequential_for_lowering_tests/determinism_tests.rs)。
以下 SHA-256 对应原始 Rust raw string 的 UTF-8 bytes（包含首尾换行和缩进）：

| case filename | 输入 SHA-256 |
|---|---|
| `determinism-temporary-list-continue.ko` | `b0981d6d808ab6e4e88d240d1a394c3e3e59682690f3b2a2f14dd084cd4c312e` |
| `determinism-mixed-array.ko` | `1bc82ad503e5250ccfe167981f006613c8d66c4074fd768aa3af9a982593be03` |
| `determinism-nested-array.ko` | `f85fe21489c5919d71c88a4a6de09b0701c874708f9734bda3ac4deb9c249c31` |

跨链拒绝复用并实际重跑既有完整身份
`ssa::lower_frontend_tests::rejects_mixed_analysis_chains_before_constructing_ssa` 与
`native_tests::entry_shape_and_analysis_identity_fail_before_object_emission`；前者分别锁
MismatchedSource / MismatchedAnalysis，后者锁身份不匹配时 object 不出现。新测试的不同
SourceId / origin 断言不是新的 mixed-products 负例，不能替代这两条门禁。

#### 未修复的 conditional-break 正例红证据

初选的 temporary case 在 `continue` 后增加条件 `break`，frontend 无诊断，但
lower_scalar_file 返回 `LoweringErrorKind::InvalidSsa`（span 为 None），尚未进入 LLVM 或
文本比较。首轮 `cargo test --locked --offline -p lang-codegen --lib determinism_tests` 实际为
2 passed / 1 failed / 0 ignored / 691 filtered，退出 101；另两项即上表 mixed / nested。
原失败输入如下，必须由后继独立修复片闭合，不能把本片正式三项绿灯解释为该红例已修复：

```kotlin
fun source(): List<Int> = listOf(7, 2, 9)
fun scan(): Int {
    var total = 0
    for (value in source()) {
        if (value == 2) { continue }
        if (value == 9) { break }
        total = total + value
    }
    return total
}
```

后续测试侧能力探针将失败缩到
`fun scan(): Unit { for (value in listOf(1)) { if (value == 1) { break } } }`，
仍是 frontend 无诊断 / InvalidSsa；无需 factory、continue、累加变量或复杂 body。
同一 temporary 的无条件 break、normal 累加、条件 continue，以及 Borrow List 的条件
break/continue 累加均通过 SSA + LLVM；将原 temporary provider 换为 Array 仍 InvalidSsa。
另一个具名 owner 对照在 `val xs = listOf(1)` 更早返回 UnsupportedNode，不把它混同为同一
InvalidSsa 根因。临时定位 harness 捕获各 case 的 panic 以继续报告，其整体退出 0 **不表示
所有 case 通过**；未将它加入正式测试，也未用 ignore / should_panic 把正例失败当验收。
本片没有修改生产代码，最终 temporary 测试明确只比较已支持的 conditional continue。

| 实际命令 / 检查（2026-10-02，正式检查退出码均为 0） | 结果 |
|---|---|
| `timeout --signal=TERM --kill-after=10s 300s cargo test --locked --offline -p lang-codegen --lib sequential_for` | 30 passed / 0 failed / 0 ignored / 664 filtered；原 27 + 新 3，新三项各完整分析两次 |
| 同上 Cargo 参数，filter `ssa::lower_frontend_tests::rejects_mixed_analysis_chains_before_constructing_ssa -- --exact` | 1 passed / 693 filtered |
| 同上 Cargo 参数，filter `native_tests::entry_shape_and_analysis_identity_fail_before_object_emission -- --exact` | 1 passed / 693 filtered |
| `timeout --signal=TERM --kill-after=10s 300s cargo test --locked --offline -p lang-frontend --test ownership_iteration iteration_ownership_facts_are_deterministic_across_analyses -- --exact` | 1 passed / 183 filtered；共享上游回归，不冒充 fresh frontend→LLVM 证据 |
| `timeout --signal=TERM --kill-after=10s 120s cargo fmt --all -- --check` | 通过 |
| `timeout --signal=TERM --kill-after=10s 600s cargo clippy --locked --offline -p lang-codegen --all-targets -- -D warnings` | 通过 |
| `python3 scripts/check_rust_sizes.py --base 0b1939b471099e8759eb729ea548d7155129403d` | merge-base 为上述 main；606 手写 / 48 历史超限 / 0 生成物，新模块 196 行；无新增例外或 legacy 超限增长 |
| `python3 -m unittest discover -s scripts/tests -v` | 94 passed |
| `python3 scripts/check_docs.py`、`git diff --check` | 通过 |

2026-10-02 P2 integration模块化后，上表历史根级exact名称映射为
`control_exits::iteration_ownership_facts_are_deterministic_across_analyses`；当前复跑使用同一
`--test ownership_iteration`与新完整名加`-- --exact`，本地实际1 passed / 183 filtered。
旧命令与结果保留为当时执行记录；[迁移验收](../../development/ownership-iteration-test-migration.md)
给出全部184项一对一映射，未改变此项确定性断言或关闭0182未决范围。

执行宿主 x86_64 Linux，Rust/Cargo 1.96.0、LLVM/Clang 21.1.8；既有共享 Cargo target，
`CARGO_INCREMENTAL=0`，Cargo 串行、libtest 默认并行，未 clean。本片 PR 双宿主 CI 尚未运行；
codegen / frontend / workspace 全量、性能、cold/warm 编译与 RSS 未运行。没有生产或跨 crate API 变动。
其余 owned named/Borrow 完整边界及再读、nominal element native、完整 projection/cleanup
矩阵、ZST 和真实 for 负例落盘桥接仍未闭合；本片支持子集的确定性不关闭这些条款。

独立 review 后补足不参与 renderer 文本的 EntityData / TypeOrigin 来源检查；按上述
`27c0e82` 最终测试代码重新执行表中的 fmt、codegen all-targets clippy、sequential_for
（仍 30 passed / 664 filtered），以及 docs、尺寸和 diff 检查，均通过。其余表中共享门禁的
已执行证据仍有效；没有把补充来源断言解释为新生产修复或 mixed-products 负例。

2026-10-02 发布前执行环境重置后，从变更记录重建上述三个文件；在增加本段恢复说明前，
完整 Git tree 为 `2047417a68367a3c7612d971a7cb95ede493fba7`，与重置前最终 tree 完全相同。
恢复后的本地测试提交为 `18699a1849e9e93a457117160c2cf7d45c29b145`，新模块仍为 196 行，
三个输入 SHA 不变。上述重置前提交和执行结果保留为历史，未把丢失的原始日志伪造为新证据。
在恢复的同版本 Rust/Cargo 1.96.0、LLVM/Clang 21.1.8 和 x86_64 Linux 环境，重新运行表中
全部正式检查：fmt、clippy、sequential_for 30 / 664 filtered、两条 mixed 门禁各 1 / 693、
frontend exact 1 / 183、94 policy、606 手写 / 48 历史超限的尺寸检查、docs 与 diff，均退出 0。
本次没有重新执行最初的失败探针，也未进行性能、冷/热或 RSS 测量；production 仍零改动。

### 2026-10-02 conditional break 的 source-loan CFG 修复

本片从 `main 0b1939b471099e8759eb729ea548d7155129403d` 独立建分支，修复现行
§3 退出清理合同，不改 frontend facts、verifier、LLVM/runtime ABI 或语言语义。本 Spec
继续保持 active；本节只闭合下述 CFG 身份错误，不重写上方历史验收或宣称全部 §5 完成。

合法最小输入 `fun scan(): Unit { for (value in listOf(1)) { if (value == 1) { break } } }`
在 frontend 无诊断，修复前 `lower_scalar_file` 返回 `InvalidSsa`。本地临时诊断显示：
break block 接收重绑定 source loan `%l6`，却对旧 body loan `%l2` 发出 BorrowEnd，产生
`HiddenLinearLiveIn` / `LoanInactive`，随后 owner drop 与仍有效 `%l6` 冲突。诊断插桩
已移除；正式代码仍独立运行原 verifier，不把失败降级为允许通过。

根因是 `body_source` 与 `active_source_loan` 在 CFG 后失去一致性。
`ForLoopData::rebind_source` 在普通 edge 重绑定、单出口恢复和多出口 merge 三个入口同时更新
二者。active 状态从本层原有 source-loan ownership marker 重建，不能沿用 sibling 的
`.take()` 后状态；复用的外部 Borrow source 仍不由本层结束。原本的 exhaustion cleanup
仍使用 exhaustion block 自己接收的 loan。

新增 [source CFG 回归](../../../crates/lang-codegen/src/ssa/sequential_for_lowering_tests/source_cfg_tests.rs)
包含三项测试：Array/List/MutableList 的最小 conditional break 对实际 edge 参数逐项断言
`element loan → 本块 source loan → temporary owner`；正常 sibling 只结束 element，回边
保留 owner/source；九种 if/else、nested if、continue、单/多出口 merge、conditional return 与两个 temporary provider
组合均通过 SSA 与 verified LLVM；Borrow 参数另明确不生成该 source 类型的 BorrowEnd。
这些期望直接检查模型实体和手写顺序，不读取 frontend cleanup plan 生成 oracle。

[native cleanup 回归](../../../crates/lang-codegen/src/native_sequential_for_tests/cleanup_tests.rs)
新增两项，复用已有双 concat String 元素、两个逆序 deinit Guard 和逐指针分配计数 helper。
第一项 conditional break 首轮退出，固定 stdout 为
`first\nlater\nearlier\nafter\ndone\n`，5 次分配/释放，含未访问元素的 buffer；第二项
nested conditional break 在第一轮正常 sibling 后于第二轮退出，固定 stdout 为
`first\nlater\nearlier\nsecond\nlater\nearlier\nafter\ndone\n`，7 次分配/释放。
每项均执行 public object/link/run 和 verified LLVM 计数 link/run，核对成功退出、空 stderr、
精确 stdout 和每个 live pointer 恰好释放一次。不能把这些 buffer/payload 计数解释为全部
String logical drop、nominal element deinit 或 ZST 验收。

正式修复前，source CFG 选择为 1 passed / 2 failed（Borrow 对照通过，两个 owned/temporary
测试 InvalidSsa）；native conditional-break 为 1 failed，在 object 发射前被同一 InvalidSsa
拒绝。修复后原测试不改 expected 转绿。另一个 `own xs: List<Int>` 参数探针在参数建立处
返回既存 `UnsupportedNode`，尚未进入本次 CFG；未加入拒绝即成功的回归，不扩展实现范围。

以下验收在工作环境重建后重新执行，不以重建前日志抵扣；生产修改和全部五个新增测试
均使用最终源码，两条 nested temporary provider 用例也在此轮真实执行。Cargo 串行，
libtest 默认并行，没有 clean、ignore 或平台过滤。

| 恢复后实际命令 / 检查（2026-10-02，正式门禁退出码均为 0） | 结果 |
|---|---|
| `cargo test --locked --offline -p lang-codegen --lib sequential_for` | 32 passed / 0 failed / 0 ignored / 664 filtered；原 27 + 新 5，新增两项 native 各实际运行两条路径 |
| 同上 Cargo 目标，libtest 多过滤器 `ssa::lower_frontend_tests ssa::container_operation_tests ssa::verify_tests ssa::verify_ownership_tests native_tests::entry_shape_and_analysis_identity_fail_before_object_emission` | 99 passed / 0 failed / 0 ignored / 597 filtered；64 CFG/lowering、10 provider/container、8结构 verifier、16 ownership verifier、1 native mixed identity |
| `cargo fmt --all -- --check` | 通过 |
| `cargo check --locked --offline -p lang-codegen --all-targets` | 通过 |
| `cargo clippy --locked --offline -p lang-codegen --all-targets -- -D warnings` | 通过 |
| `python3 scripts/check_rust_sizes.py --base 0b1939b471099e8759eb729ea548d7155129403d` | 606 手写 / 48 历史超限 / 0 生成物；control.rs 1443 行不增长，loop_control.rs 973→983，新增 source_cfg_tests.rs 157 行；无 baseline/exception 修改 |
| `python3 -m unittest discover -s scripts/tests -v` | 94 passed |
| `python3 scripts/check_docs.py`、`git diff --check` | 通过；Architecture 原有 200 行入口上限保持 |

所有 Cargo test 均外包 `timeout --signal=TERM --kill-after=10s 300s`，check/clippy 为 600s，
fmt 为 120s。宿主 x86_64 Linux，Rust 1.96.0、LLVM/Clang 21.1.8，共享 Cargo target、
`CARGO_INCREMENTAL=0`。本片 PR 双宿主 CI 尚未运行；macOS 本地、frontend 全量、
workspace 全量、性能/RSS 未运行。无跨 crate API 变化；现有 nominal element 构造边界、
完整 owned named/Borrow 矩阵、ZST、derived/component cleanup、真实 for 负例落盘与
独立分析链确定性不由本修复的结果证明，保持原合同和各自后续验收范围。

### 2026-10-02 PR22 合并后的 conditional-break 整合复验

PR22 的独立分析链确定性片已合入 `main a7a92790a49053d110e00655f174515a13093449`。
本修复从远端 `3536b25ff1f1d8ed4c1e4088f0eb93beed53361b` 普通 merge 该 main，
保留 `determinism_tests` 与 `source_cfg_tests` 两个入口及以上两份验收原文，不重写历史结果。
确定性片中记录的 temporary conditional-break `InvalidSsa` 红例，已由
[本节 source-loan CFG 修复](#2026-10-02-conditional-break-的-source-loan-cfg-修复)闭合；
其 named owner / `own` 参数的既存 `UnsupportedNode` 边界不包含在这项结论中。

以上一节相同的命令、timeout、宿主与工具链重新运行整合树；只有尺寸比较 base 更新为
上述新 main。旧 head 的 9/9 CI 不作为新整合树的证据，本次发布后需运行新的双宿主 CI。

| 新整合树的本地验收（2026-10-02） | 实际结果 |
|---|---|
| `lang-codegen --lib sequential_for` | 35 passed / 0 failed / 0 ignored / 664 filtered；原 27 + 确定性 3 + 修复 5 |
| 上节相同的五个 libtest CFG/provider/verifier/mixed 过滤器 | 99 passed / 0 failed / 0 ignored / 600 filtered |
| fmt、codegen all-targets check、严格 clippy | 三项均通过 |
| 尺寸检查 `--base a7a92790a49053d110e00655f174515a13093449` | 607 手写 / 48 历史超限 / 0 生成物；production 两文件与已审修复相同，无 baseline/exception 变化 |
| policy、docs、diff | 94 passed；462 Markdown 结构通过；无空白错误 |

确定性测试源和前述两段历史正文均逐字保留。本轮不扩大已声明的能力范围，也不宣称
全部 SPEC-0182 已验收；尚未运行的项与上节范围保持。

整合复核另将确定性片最初的完整红输入（`source(): List<Int> = listOf(7, 2, 9)`、
conditional continue + break、`total` 累加）永久加入现有 CFG 测试，明确通过 fresh frontend、
SSA verifier 与 verified LLVM，支持上述精确红例闭合引用。该测试函数另以 `--exact --nocapture`
单独重跑 1 passed / 698 filtered，再运行同一套整合门禁；函数数不变，sequential_for 仍为 35。


### 2026-10-02 Unit 容器存储独立修复的验收衔接

[SPEC-0248](../../archive/specs/0248-unit-container-storage.md) 在独立 `fix/spec-0248` 中实施单文件列表式
Unit 容器 operand 物化及 container-only storage/layout；它不属于本 Spec 的纯验收补测，
不改变 §4 的原边界。待该片取得实际结果后，仅在此追加三容器 × 0/1/3 temporary source、
Copyable Unit named binding/CFG、逻辑次数和共享 bounds 路径的有界证据。

此段只登记关联与待测范围，尚无新增通过结论；本 Spec 保持 active，原 §5、历史记录与
其余未闭合条款均保留。MoveOnly ZST 的逻辑 drop、nominal element、完整 owned/Borrow
矩阵和一般 Unit ABI 不由这项修复推定完成。


0248 的首轮实施反馈已取得两个独立原始红例不改 expected 转绿；`unit_storage` 过滤器为
14 passed / 0 failed / 0 ignored / 698 filtered，其中包含三容器 × 0/1/3 native、discard
与 if 后 Unit Read。这是实施中工作树的部分证据，完整 SSA 身份、bounds、ABI 及最终门禁
仍待 0248 自己闭合；既有 `MutableList<Unit>()` 空构造另列独立 native 验收，不混入九格。
本 Spec 仍保持 active，不据此勾选其余未闭合合同，也不把 Copyable Unit 当成 MoveOnly ZST。


0248 最终 Linux 本地验收补充：`unit_storage` 32 passed / 0 failed / 0 ignored / 698 filtered
（31 新增 + 1 旧 Unit root），`sequential_for` 57 passed / 673 filtered；完整 codegen
730 unit + 4 doctests 全通过，无 ignored/filtered。新增领域源码与精确门禁见
[0248 账本](../../archive/specs/0248-unit-container-storage.md#7-分层验收账本)，实现边界见
[Unit 容器存储](../../architecture/unit-container-storage.md)。三容器九格独立 native 固定
bytes 和 SSA 身份检查、count/if CFG 后 Unit Read、discard、三 provider Diverged、一格
fresh-chain 文本一致性及独立空 MutableList 构造均已通过；低层 Array bounds 五格保留
逻辑守卫，CFG-carried loan 的合法零字节 load 不被误判为缺陷。

这些结果只补充本节声明的 Unit temporary-source/正常迭代子集，不扩大原 owned/Borrow、
projection/cleanup 或 MoveOnly ZST 验收。0248 仍 in-progress、双宿主 CI 待发布；本 Spec
继续 active，§5 原合同与全部历史不重写。


0248 后继完成证据：PR24 实现 head `c77a9aa0a6fc1b0fcf951afcf4ea9ea945778cfd` 的
[双宿主 CI 37024363674](https://github.com/Halckon/Koven/actions/runs/37024363674)
已 9/9 jobs success；Ubuntu/macOS 各逐名确认相同 32 项 Unit storage 测试全部 `ok`，
其中31新增、1旧Unit root，无新增 ignored。完整 codegen 分别为730 passed和729 passed
加1既有LLDB权限ignored，两宿主各4 doctests通过。0248已按此有界证据归档为done；
本 Spec 仅获得前述 Copyable Unit temporary-source/正常迭代子集证据，仍保持active，
不关闭一般 owned/Borrow、projection/cleanup 或 MoveOnly ZST 未决验收。归档后文档head
的CI/review另行核验，不从本次首轮结果推定；原历史与合同保持不变。

### 2026-10-04 Mac 本机重建的六项验收映射

基线为PR36 `201d415d126d86183275d86ff2bc45caae6586a4`。丢失的本地后继对象不可用，
按用户清单重新实现，不恢复原patch或SHA；以上历史记录不作为本轮通过结果。
用户授权仅本地main提交，禁止推送、PR和远端更改；最终集中验证，所有Cargo串行。

| §5验收项 | 本机实际证据 |
|---|---|
| source once与owned/Borrow/temporary | 三容器×owned/Borrow×0/1/3×四退出72格native，factory/source-once及last-use；既有temporary路径同时保留 |
| binding、Copy/Borrow与components | Int/String/Cell字段及resource Parts完整、partial、discard组合；Parts72格实际object/link/run与计数 |
| CFG与cleanup | direct resource36格、Parts72格、非ASAP存活出口guard、独立provider实际edge/参数/tombstone与预算拒绝 |
| source保护负例 | frontend完整ownership_iteration184项通过，单文件真实for动态resource和非source owner拒绝保持 |
| native与ZST精确计数 | 内部合成MoveOnly ZST三容器×0/1/3×零/非零stride18格，保留真实drop loop，独立logical计数与storage malloc/free |
| 原子性与确定性 | single真实for整目录bytes与LLVM调用拒绝矩阵；actual unit-for准确拒绝、普通unit正例证明失败后恢复；fresh frontend/SSA/LLVM链确定性 |

以上目标均在本轮修后的完整codegen library中执行：787 passed、0 failed、1既有LLDB权限ignore、
0 filtered，554.15秒。该ignore属于调试器断点测试，不能表述为该项通过。
frontend库190 passed；真实stage75 targets/994 passed、Guide23、教程7+2及最终严格Clippy通过；
Return边界修复后5+3定向通过，
最终结果由[本机恢复账本](../../development/recovery-local-delivery.md)登记。

unit driver真实for仍为UnsupportedSource，未扩大其能力；独立unit正例不冒充unit-for成功。
SyntheticZero仅在内部测试提供，不支持源码MoveOnly ZST，也不声称ZST逆序可观测。
旧SharedFieldLoan配对swap误报保留；此处provider有限状态引擎不重写一般verifier。
本机Mac结果不替代Linux/双宿主CI，旧成本raw丢失且本轮没有成本实验。


## 2026-10-04 本机恢复最终验收

本轮仅按用户授权在本地main重建和交付，禁止推送/PR/远端更改。
实际Mac验证见[本机恢复账本](../../development/recovery-local-delivery.md)：core、
ownership_iteration184、stage75个target/994项、Guide23、教程7正例+2完整JSON负例通过；
Return只读自查修复5+3定向通过，最终workspace all-targets严格Clippy通过。
既有LLDB权限ignore1保留；Linux/远端CI未运行，旧云端证据不替代本轮。
本地验收归档不等于原P0–P5全部退出；P2成本raw丢失、预算接受未授权，缺口不关闭。
unit-for仍准确拒绝，源码MoveOnly ZST、旧SharedFieldLoan swap误报及其他原非目标保持。
