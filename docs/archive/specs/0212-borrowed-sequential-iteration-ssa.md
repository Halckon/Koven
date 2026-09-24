# SPEC-0212：借用式顺序迭代 SSA/LLVM primitives

> **性质**：实施 Spec · **状态**：done · **读取时机**：追溯 SPEC-0212 交付证据时 · **唯一真源**：本 Spec

## 1. 元数据

| 字段 | 值 |
|---|---|
| 状态 | `done` |
| Goal ID | `KOV-P4-212` |
| 所属 Phase | Phase 4 |
| 语言规范 | [现行 v0.37 §37](../../guide/12-collections-destructuring.md#37-借用式顺序容器迭代-provider) |
| 批准依据 | 2026-09-19 用户明确启用 v0.37；按持续 Goal 顺序推进 |
| 前置 Spec | SPEC-0034、0036、0186、0195 `done` |
| 前置 ADR | [ADR-0023](../../adr/accepted/0023-borrowed-sequential-iteration-provider.md) `accepted` |
| 阻塞项 | 无；Phase 4 primitive 已独立于 SPEC-0211 的递归环境运输验收 |
| 影响范围 | `lang-codegen` SSA model/verifier/container LLVM adapter/tests；Architecture |
| 语言语义变更 | 否；实现启用后的 v0.37 provider primitives |

## 2. Goal

完成后，手工 typed SSA 能以 active shared container loan、一次 `Int` length、hidden cursor、
checked element place 和线性 cleanup 表达并验证无分配 sequential provider，LLVM 对 owned/Borrow
container 使用同一 header/runtime ABI；本 Spec 不接真实 `for` AST。

## 3. 范围与需求

- `ContainerLength` operand 扩为 container Value 或 active shared Loan；exclusive/ended/wrong-type
  loan 被 operation/ownership verifier 拒绝，Value 路径保持兼容。
- length result 固定为真正 signed 32-bit Koven `Int`。LLVM header 仍为 target `size_t`，只在
  `logical_length <= 2^31 - 1` invariant 下转换；清除现有 signed i64/`size_t` 冒充 Int 的测试。
- 所有 container 创建边界共同建立 representability invariant：runtime-length construction 先在
  `Int` 域拒绝负数，再无损宽化到 `size_t`；list-form construction 在写 header 前验证元素数
  `<= 2^31 - 1`；未来增长操作也必须在提交新长度前执行同一上限检查。删除现有“语言 length
  与 `size_t` 位宽必须相等”的假设。
- checked index 先在 signed `Int` 域拒绝负数并比较 logical length，再无损宽化为 target index；
  `ContainerLength`/`container.size` 的 `size_t → Int` 只消费上述创建/增长边界已经建立的 invariant，
  不在读取时静默截断或事后修复非法 header。
- provider cursor 为 nonnegative Int；手工 verified CFG 覆盖 acquire once、length snapshot once、
  `cursor < length` 后取 element、increment 不溢出和逐轮 element loan begin/end 的规范形状。
- source Value/shared Loan、cursor、element Loan 可按 ADR-0006/0016 跨合法 edge 传递；verifier 拒绝
  错误 edge entity、inactive/重复 BorrowEnd、active source/element loan 下 replace/drop。
- LLVM loan length 路径先从 pointer load container header；element 继续使用既有 checked-index/
  ZST sentinel，provider 不声明 runtime symbol、不 allocation、不新增 type layout。
- 手工 SSA/LLVM tests 覆盖 empty/non-empty、Copyable/MoveOnly/ZST、normal/continue/break/return shape，
  以及 malformed state/loan/length contracts。

## 4. 非目标

- 不读取 frontend iteration/ownership facts，不 lower `Statement::For`，不生成 executable。
- 不改变 container header/allocator/drop ABI，不实现 Inout source lowering、public iterator object、
  runtime API、自定义或 consuming iteration。
- 不把 Koven `Long`、UInt/ULong 或公开 machine-size type 引入 provider contract。

## 5. 验收标准

- [x] ContainerLength Value/shared Loan 的 operation、ownership、render 与 LLVM 正例通过；exclusive、
  ended、wrong target 和 result 非 Int 的反例被 verifier 拒绝。
- [x] runtime-length/list-form construction 共用 logical length 上限，header size_t→Int bridge
  有边界/不变量测试；既有 container construction/size 测试不再依赖位宽相等或 signed 64-bit
  Koven Int 假设。未来增长操作须复用此上限，本 Spec 不实现尚不存在的增长 API。
- [x] verifier 接受 owner+source loan+cursor 跨 edge 与逐轮 element begin/end；拒绝错误 edge type、
  inactive/重复 BorrowEnd 和 active-loan replace/drop。
- [x] checked element place 固定为 signed `Int` 负数检查 → logical upper-bound → target index 无损宽化，
  ZST 按逻辑 index 迭代且不解引用 sentinel。
- [x] LLVM IR 不含 iterator runtime symbol/allocation；两次 render 确定，SSA/LLVM verifier 与
  受影响的 SSA/verifier/LLVM 契约测试通过。
- [x] Architecture 同步为已实现事实。

## 6. 技术方案与边界

复用现有 integer compare/arithmetic、block parameter、RootPlace/BorrowBegin/End、ContainerLength、
ContainerElementPlace、Read/Drop；只在现有 operation 无法表达 shared length 时扩大 operand identity。
provider 不新增 first-class SSA type 或专用 runtime operation；普通 typed edge、loan active-state 与
owner conflict verifier 共同拒绝 malformed CFG。职责明确的 canonical builder 构造 length-once、
zero-cursor、guarded-place 与 unit-increment 形状，结构测试和后继 lowering 测试锁定该算法；不要求
通用 verifier 从任意整数 CFG 证明遍历算法。

## 7. 实施计划

1. [x] 修正 borrowed ContainerLength 与 Int/size_t bridge → 验证：operation/LLVM/边界窄测。
2. [x] 建 provider CFG/loan 线性 verifier 矩阵 → 验证：positive/negative 手工 SSA。
3. [x] 接 checked place/ZST/determinism 并同步 Architecture → 验证：codegen、workspace、fmt/clippy。
4. [x] 复核创建边界的共同长度 guard → 验证：list-form 与 runtime-length 窄目标边界反例。
5. [x] 建立 canonical provider builder → 验证：length-once、zero cursor、guarded place 与 unit increment 形状复用。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | borrowed length、provider verifier、LLVM 与完成文档 | `feat(codegen): verify sequential iteration provider (SPEC-0212)` |

## 9. 未决问题

- 无；真实 source integration 由 SPEC-0182 承接。

## 10. 验证记录

实施前按[分层验收](../../development/testing.md)将第 5 节各项映射到实际测试目标/过滤器；
记录命中数、结果与未运行原因。同一状态下的有效证据只运行一次，不默认运行 frontend 全量。

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-27 roadmap/实现审计 | 通过 | existing element place/loan CFG/header 足够；ContainerLength loan operand 与 Int/size_t 契约必须先修正 |


2026-09-19 候选重基核对：保留现行 v0.36 的 grammar、nullable/Nothing、所有权与常量契约，
拟议版本取代关系见 proposal。仅更新基线与状态前置，不改变本 Spec 的阶段范围、验收条目或
批准状态；guide 启用与 ADR 接受仍是实施前置。未运行 Rust 测试（本次仅文档）。

2026-09-19 启用记录：v0.37 已启用、ADR-0023 accepted；temporary source 纳入首轮 native。
上方重基时的未启用说明是历史记录，不再是当前阻塞项；实现/验收尚未完成。

2026-09-24 实施记录：前置全部满足，SPEC-0212 移入 active。借用式 `ContainerLength` 的
Value/shared Loan operation、ownership、render、LLVM header load 和反例窄测通过；
`Int`/`size_t` bridge 与 list-form 上限开始实施。provider CFG、逐轮 loan、checked place/ZST
及真实 `for` native 尚未完成；不得据此勾选第 5 节整项验收。

2026-09-24 本轮 Cargo 检查均以 `CARGO_TARGET_DIR=/private/tmp/koven-codex-target-20260924`
和 `CARGO_BUILD_JOBS=2` 运行，避免与现有 target 锁争用。`cargo test -p lang-codegen container_operation_tests --lib --quiet`
7 passed；`cargo test -p lang-codegen container_tests --lib --quiet` 11 passed；
`int_container_construction_links_and_runs_with_size_t_header` 与
`borrowed_container_length_builds_links_and_runs_with_int_result` 各 1 passed，分别完成源码
构造及手工 SSA 借用长度的 build/link/run。`cargo clippy -p lang-codegen --all-targets -- -D warnings`、`cargo fmt --all -- --check`、
`python3 scripts/check_docs.py` 与 `git diff --check` 通过。完整
`cargo test -p lang-codegen --lib --quiet` 在新增 native 测试前运行：445 passed、2 failed、
1 ignored；失败项是 `nested_string_owners_survive_normal_path_and_clean_up_on_early_return`
和 `string_containers_clean_up_on_normal_path_and_early_return`，均在 frontend lowering 阶段
报 `InvalidSsa`；临时诊断显示 `if (early) return` 的 CFG edge 有 `ValueUnavailable`，
后继出口有 `MissingOwnedExit`，原因尚未修复，不能计为通过。第 5 节其余验收未运行完。

2026-09-24 续验：新增手工 provider CFG 正例，验证 source owner/shared loan、length snapshot、
cursor 与逐轮 element loan 跨 edge 的线性关系；`provider_cfg_carries_owner_and_source_loan_and_ends_each_element_loan`
通过。此前两个 native 失败定位为构造和容器的已交付 MoveOnly operand 仍被保留在源 binding；
已按 operand 完成顺序转入跨边 pending slot，并补构造中途控制转移的前端 temporary drop fact。
新增 native 分支、提前 return、Copyable 原绑定回归通过；`cargo test -p lang-codegen --lib --quiet`
结果为 454 passed、0 failed、1 ignored。`cargo clippy -p lang-codegen --lib --tests -- -D warnings`
通过。此处仅确认构造修复和 provider CFG 正例；第 5 节的 provider 反例矩阵、checked place/ZST
及确定性验收仍待推进。

同日补充 `provider_element_loan_can_end_after_a_block_edge` 通过，覆盖 element loan 与 source
loan 同时跨合法 edge。现有 `edge_arity_type_condition_and_return_contracts_are_independent`、
`move_only_place_reads_and_inactive_loan_end_are_rejected`、
`move_after_drop_and_replacement_during_element_loan_are_rejected` 提供错误 edge type、
重复 BorrowEnd 和借用中 replace 反例；借用 length 反例
另验证 ended/incompatible loan。由此完成第 2 步 verifier 矩阵；LLVM/provider 的其他形状仍按
第 5 节未勾选项继续。

同日 checked place 切片：新增 i64 index 被 operation verifier 拒绝的失败回归；随后把
element place/replace 的索引合同收紧为 signed i32 `Int`。LLVM 先比较负数和逻辑长度，
成功边才转换到 target `size_t`；ZST 也执行逻辑检查而不形成 GEP。
`container_operation_tests` 10 项、`container_tests` 11 项与
`borrowed_container_lowering_tests` 2 项定向通过；这三组中既有 i64 fixture 已按现行
Int 合同修正。完整 codegen 复验仍在运行，第 5 节的 LLVM/provider 多形状与总体确定性验收
未勾选。

同日复验结果：`cargo test -p lang-codegen --lib --quiet` 456 passed、0 failed、1 ignored；
上述两项 borrowed-index 旧夹具更新后全套通过。provider CFG 的两次 SSA render 一致性
断言另行定向通过。`cargo clippy -p lang-codegen --lib --tests -- -D warnings`、
`cargo fmt --all -- --check`、`python3 scripts/check_docs.py`（371 份 Markdown）及
`git diff --check` 通过。尚未运行 workspace 全量或 LLVM provider 多出口形状的完整验收。

同日独立复核发现窄目标修正：原 `allocate_buffer` 把已经转换为 unsigned `size_t` 的高位
当成负数；容器构造、生成、element place 和 drop 的物理 GEP 改为非 inbounds。
初次修正曾误判 i16 目标的 32768 字节物理分配合法，后续复核按 LLVM 指针索引上限更正。
此前用 i16 size_t 的 32768 边界验证不再产生 signed negative check，定向
`container_tests` 11、`borrowed_container_lowering_tests` 2、`runtime_tests` 5 项通过；
这项修正后的 codegen 全套和 clippy 仍待复验。

窄目标修正续验：`cargo test -p lang-codegen --lib --quiet` 457 passed、0 failed、
1 ignored；`cargo clippy -p lang-codegen --lib --tests -- -D warnings` 通过。
`cargo fmt --all -- --check`、`python3 scripts/check_docs.py`（371 份 Markdown）及
`git diff --check` 通过。i16 测试只验证生成的 LLVM，不等同于在 16-bit 目标运行 native；
真实 `for` lowering 仍属后续 Spec。

后续复核确认：LLVM 分配对象的物理字节数不能超过指针索引位宽的最大有符号值。
`allocate_buffer` 现从目标 DataLayout 读取默认地址空间的索引位宽，在无符号乘法溢出后
拒绝超界字节数；静态 heap/shared allocation 和元素 stride 同样受此上限约束。
零大小元素不分配缓冲区，其逻辑长度仍可占用窄 `size_t` 的高半区。
将 i16 回归改为验证 32768 字节命中 32767 上限检查，并补充 pointer size 64 / index size 32
的 DataLayout/`RuntimeAbi::lower` 上限检查；这些检查不等于窄目标 native 执行。
更正后 `cargo test -p lang-codegen --lib --quiet`：458 passed、0 failed、1 ignored；
`cargo clippy -p lang-codegen --lib --tests -- -D warnings`、`cargo fmt --all -- --check`、
`python3 scripts/check_docs.py`（371 份 Markdown）及 `git diff --check` 通过。
frontend 全量、workspace 全量和窄目标 native 均未运行。
接线补测后 narrow runtime 定向 2 项通过，strict Clippy、格式、文档结构与 diff 检查复验通过。

后续 provider LLVM 验收：手工 verified CFG 让 owner/source loan、length 与 cursor 跨 header/body，
逐轮建立并结束 element loan；耗尽和 body break 汇入同一 exit，结束 source loan 后返回 owner。
MoveOnly ZST 变体另从 body 提前 return，独立结束 source loan 并交付 owner。
同一测试分别使用 `Int`、Copyable ZST 与 MoveOnly ZST 元素，核对入口 length 一次、checked
index、ZST 不形成 GEP、无 iterator runtime allocation，以及两次 LLVM render 一致。
首次定向命令误带 `--exact` 导致 0 命中，未计作通过；修正过滤器后 1 项通过，
`llvm::container_tests` 10 项通过。新增测试的初版将整个 IR 的 header 提取数误判为 1；
元素 place 每轮另有合法的 checked-index header 读取，断言现只约束入口快照。
`cargo test -p lang-codegen --lib --quiet` 在三元素参数化前为 459 passed、0 failed、1 ignored；
参数化后定向测试通过。`cargo check --workspace --all-targets`、
`cargo clippy -p lang-codegen --lib --tests -- -D warnings`、`cargo fmt --all -- --check`、
`python3 scripts/check_docs.py`（371 份 Markdown）与 `git diff --check` 通过。
共同 guard/canonical builder 和真实 `for` 集成仍未验收，Spec 保持 in-progress。
独立复核指出初版缺少循环体内提前 return；补充后 provider 定向 1 项通过，
该变体的 LLVM 有两条实际 return 边，其他两种元素变体保持耗尽/break 共用出口。
同轮补窄目标长度桥回归：i16 `size_t` 下，runtime-length 在 signed i32 域拒绝负数、
在转换前拒绝超过 65535 的值，成功路径截为 i16；header 读取再零扩展回 i32。
`llvm::runtime::narrow_tests` 2 项通过。尚无增长操作，本项共同 guard/增长边界验收未勾选。
随后将静态 list-form 和动态 runtime-length 的共同 `max_logical_length(size_bits)` 收敛为一处，
两条路径分别由静态边界测试与 i16 IR guard 测试覆盖。第 4 节已排除 runtime API，而增长操作
尚不存在，因此将第 5 节验收明确限定为本 Spec 的两个创建入口；未来增长仍须遵守第 3 节
同一上限，不能借本次勾选宣称增长已实现。canonical builder 继续未完成。
随后将 provider builder 纳入正式 codegen 构建：`enter_header` 负责一次 length/零 cursor 的
入口运输，`guard_and_begin` 将取元素绑定到 guard 真边，`backedge` 负责固定 length/下一 cursor
的回边运输，`finish_element_and_advance` 固定逐轮 loan end 和 `+1`。结构测试故意传入交换
的入口/回边占位值，并断言最终 edge identity、header compare、body place 和 unit increment。
定向 provider 测试 1 项通过；完整 `cargo test -p lang-codegen --lib --quiet` 在最后一次
入口/回边构造器调整前为 459 passed、0 failed、1 ignored，须在最终代码状态复验。
独立复核发现并推动关闭了入口参数交换、任意回边 cursor、跨容器 source 及 builder 句柄
可改写四类绕过。另将手工 SSA provider CFG 的 element loan end 后路径改为独立 continue block
再回 header；对应定向测试 1 项通过，普通回边/break/return 则由 LLVM provider 测试覆盖。
含同期未提交 Phase 3 测试的工作树中，`cargo test -p lang-codegen --lib --quiet` 为
459 passed、0 failed、1 ignored。独立暂存快照（含先行修复 `c6c7f36` 与合法 `for` 夹具）
复验为 452 passed、0 failed、1 ignored；`cargo check --workspace --all-targets`、
`cargo clippy -p lang-codegen --lib --tests -- -D warnings`、`cargo fmt --all -- --check`、
`python3 scripts/check_docs.py`（371 份 Markdown）及 `git diff --cached --check` 通过。
收紧 builder 句柄字段可见性后，provider 与 continue 定向测试各 1 项通过。
本 Spec 不含真实 `for` source/native 集成，后者由 SPEC-0182 承接。
