# SPEC-0207：pointer-like 非空断言 lowering

> **性质**：实施 Spec · **状态**：in-progress · **读取时机**：实施或评审 v0.35 对应 Goal 时 · **唯一真源**：本 Spec

## 1. 元数据

| 字段 | 值 |
|---|---|
| 状态 | `in-progress` |
| Goal ID | `KOV-P4-207` |
| 所属 Phase | Phase 4 |
| 语言规范 | 现行 [v0.35 空安全](../../guide/09-nullability-errors.md)与[阶段边界](../../guide/15-conformance-and-staging.md) |
| 批准依据 | 2026-09-08 用户明确启用 v0.35，按持续推进 Goal 分阶段实施；依赖未完成者保持 draft |
| 前置 Spec | SPEC-0034、0039、0184、0196、0205、0206 `done` |
| 前置 ADR | ADR-0017 `accepted` |
| 阻塞项 | 无 |
| 影响范围 | `lang-codegen` frontend lowering/SSA/LLVM/native tests；必要的 frontend pending-call drop facts；Architecture |
| 语言语义变更 | 否；实施启用后的 v0.35 pointer-like lowering |

## 2. Goal

完成后，class/Box/Rc nullable `!!` 消费 frontend extraction/ownership facts，经
`NullableBranch/Take`、verified LLVM 和 compiler-bound Abort 运行，且 move/drop 与源码契约一致。

## 3. 范围与需求

- 只消费 0205/0206 facts；首轮 pointer-like class/Box/Rc 均为 MoveOnly，只接受 owned
  whole-root/temporary operand并消费 owner，不虚构 Copyable pointer path。
- 先以 ADR-0017 `NullableBranch` 建 proof；non-null edge执行 `NullableTake(owner, proof)`，
  null edge直接 lower 0205 compiler-bound effect 到既有 SSA Abort primitive，不查询 `error`
  名称、不生成普通 call或 unwind cleanup。
- class/Box/Rc 的 pointer null niche保持无 tag/allocation/隐式 retain；verifier先于 LLVM 拒绝
  proof/owner/drop 不匹配。

## 4. 非目标

- 不实现 inline/tagged nullable、borrow unwrap、Elvis、safe call、`as?` 或新的 Abort ABI。

## 5. 验收标准

- [ ] native 覆盖同名 error 遮蔽时 !! 仍 abort，显式 error 调用仍选择源码声明。

- [ ] SSA/verifier 覆盖合法 take、伪/跨 owner proof、重复 take/drop 与 null-edge direct Abort。
- [ ] class/Box/Rc 非空结果与 null 进程终止 native 测试通过，operand副作用只发生一次。
- [ ] Rc retain/release、Box/class free 与 moved binding 后续行为正确，无额外 tag/allocation。
- [ ] Borrow/Inout/field/element 与 inline nullable 继续确定性 unsupported；既有 nullable if/when
  的受影响契约回归。
- [ ] Architecture 同步。

## 6. 技术方案与边界

在 frontend lowerer增加 descriptor-driven `NonNullAssert` 分派，复用既有 nullable SSA/LLVM 与
Abort primitive，不新增 parallel unwrap operation 或后端 AST 模式匹配。

## 7. 实施计划

1. [ ] 接 extraction facts 到 NullableTake/CFG → 验证：lowering/verifier 窄测试。
2. [ ] 接 LLVM/native class/Box/Rc → 验证：IR 与真实进程正反测试。
3. [ ] 同步验收与 Architecture → 验证：按[分层验收](../../development/testing.md)选择目标测试与必要下游检查，并记录命中数。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | 单文件 SSA/LLVM/native 与实际边界 | `feat(codegen): lower single-file non-null assertions (SPEC-0207)` |
| 2 | compilation-unit assertion 接线与待交付实体传递 | `feat(codegen): lower unit non-null assertions (SPEC-0207)` |
| 3 | compilation-unit 控制流组合、native 与最终拒绝用例验收 | `test(codegen): complete non-null assertion acceptance (SPEC-0207)` |

## 9. 未决问题

- 无语义未决项；R1–R3 已随 v0.35 启用，前置依赖见元数据。

## 10. 验证记录

实施前按[分层验收](../../development/testing.md)将第 5 节各项映射到实际测试目标/过滤器；
记录命中数、结果与未运行原因。同一状态下的有效证据只运行一次，不默认运行 frontend 全量。

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-27 roadmap 审计 | 通过 | ADR-0017 已定义 NullableTake，frontend lowerer当前仍确定性拒绝 `NonNullAssert` |

### 实施验收映射

| 契约 | 目标 / 过滤器 | 实际状态 |
|---|---|---|
| owned class/Box/Rc 的 proof/take、直接 Abort、无额外分配或 retain | `cargo test -p lang-codegen --lib non_null_assertion -- --nocapture` | 8 项通过；含 6 项 lowering/边界与 2 项 native，真实运行 9 个进程 |
| temporary、group、Borrow call result、待交付 Value 实参、普通及 nullable if、return/break/continue、不可达 inline | `cargo test -p lang-codegen --lib ssa::lower_frontend_tests -- --nocapture` | 33 项通过；nullable if 操作数先复现 MissingFact，修复后通过 |
| proof 合法性、跨 owner proof、borrow 参数伪 proof、active view 阻止 drop、既有 nullable if/when 和 LLVM/native | `cargo test -p lang-codegen --lib nullable -- --nocapture` | 19 项通过；覆盖既有 class/Box/Rc native 与 unit nullable 存储回归 |
| 后续 operand 控制转移清理待交付 Value owner | `cargo test -p lang-frontend --test ownership_checking --test ownership_nullable_when --no-fail-fast`，及新增 `pending_value_argument_cleanup_follows_control_transfer_not_call_return` 定向检查 | 原调用中既有 ownership_checking 28 项、nullable_when 26 项通过；新增用例最初因测试环境缺失 error 声明失败，改为本地 Nothing 函数后单独重跑 1 项通过；新项覆盖 return/break/continue、正常调用和 Nothing 五种场景 |
| native 非空交付、null 终止、error 遮蔽、operand 一次求值 | `non_null_assertion_pointer_results_and_shadowed_error_run_natively`（包含在首行） | class/Box/Rc 各运行正反例，null 均 SIGABRT；Node/Rc 读取 payload，Box 验证 owner 交付，其 payload projection 仍为既有不支持边界 |
| 移交后析构与 Rc 显式别名 | `non_null_assertion_transfers_one_allocation_and_preserves_explicit_rc_alias`（包含在首行） | 每类 1 次分配、1 次释放；Rc 另验证 1 次 retain、2 次 release，原 owner 释放后别名仍读取 payload |
| Rust 静态检查 | `cargo fmt --all -- --check`；`cargo clippy -p lang-codegen --all-targets -- -D warnings` | 通过 |
| frontend 受影响目标 lint | `cargo clippy -p lang-frontend --lib --test ownership_checking --test ownership_nullable_when -- -D warnings` | 通过 |
| Architecture 与文档门禁 | `python3 scripts/check_docs.py`、`git diff --check` | 通过 |
| compilation-unit lowering/LLVM | `cargo test -p lang-codegen --lib unit_non_null_assertion -- --nocapture` | 5 项通过，含 class/Box/Rc × root/group/call/group-call 的 12 个场景、跨文件 Rc、前序 Value/Borrow 实参、接收者/callable 与 class/container 构造 |
| compilation-unit 共享回归与 Diverged 隔离 | `cargo test -p lang-codegen --lib ssa::unit_lower -- --nocapture` | 126 项通过，含新增 assertion 测试与终止分支回归；`take(a, error("stop"))` 的兄弟分支执行 `take(a, b!!)`，修复前复现 InvalidSsa，恢复各层 pending 栈后通过 |
| compilation-unit 控制流结果作为 assertion operand | `cargo test -p lang-codegen --lib unit_non_null_assertion_control_operand -- --nocapture` | 1 项通过，包含普通 if 与 Boolean when；两分支分别返回 owned nullable root 与 call temporary，合流结果通过 SSA/LLVM 验证 |
| 重复消费的精确 verifier 拒绝 | `cargo test -p lang-codegen --lib nullable_operation_tests -- --nocapture` | 6 项通过；新增 1 项含重复 take、take 后 drop wrapper、inner 重复 drop 三个场景，均断言对应 owner 的 ValueUnavailable，并复用可通过验证的正例图 |
| compilation-unit pending 控制流组合 | `cargo test -p lang-codegen --lib ssa::unit_lower -- --nocapture` | 129 项通过；覆盖前序 Value/Loan 与 if、两类 when、短路条件、单边 Diverged 的 15 个组合，以及 checked 算术与部分分支别名 |
| compilation-unit pending borrow drop facts | `cargo test -p lang-frontend --test multifile_ownership_checking -- --nocapture` | 63 项通过；新增分支/嵌套调用保护与插值 Abort 无正常 drop 两项。Abort 夹具消除重名参数后定向重跑 1 项通过 |
| pending 切片静态检查 | `cargo fmt --all -- --check`；`cargo clippy -p lang-codegen --all-targets -- -D warnings`；`cargo clippy -p lang-frontend --lib --test multifile_ownership_checking -- -D warnings` | fmt 与 codegen（含 frontend library）通过；frontend integration lint 被原有第 1006 行 `filter_map_bool_then` 阻塞，该段未修改 |
| compilation-unit temporary 控制流组合/native | 待验收 | 独立 temporary owner 跨普通控制流与 unit native 尚未完成；Spec 保持 in-progress |

单文件入口复用现有 nullable SSA operations，不新增 Abort ABI。独立复核发现并已修复
pending Value 实参跨分支身份、nullable if 结果别名及不可达 inline descriptor 类型登记问题。
控制转移的 owner 义务由 frontend 发布；codegen 在分支和循环出口合流前消费已有 drop facts。

未运行 frontend 全量测试；未重复运行已知含既有 lint 的 frontend 全目标 clippy，仅检查受影响
library 和两个 ownership integration targets。单文件 native 证据不代表 compilation-unit 路径完成，
后者继续由本 Spec 承接。重复 take/drop 的显式拒绝证据见上表。

编译单元独立复核发现前序构造字段跨 assertion 分支遗漏，以及 Diverged 分支遗留 pending
实体污染兄弟分支；均已修复并复核。测试初版自定义 Nothing 函数触及既有 unsupported，
改用标准 error 后取得上述红绿证据。编译单元切片未新增 native 验证，也未重复 frontend 测试。

本次验收测试切片经过独立自检步骤：正例 SSA 图仍通过，三个反例逐个核对消耗对象，
控制流操作数用例验证 root 与 temporary 汇合后只有一个可交付结果；没有修改生产逻辑。

后续 pending 控制流切片修复 Value/Loan 的跨边重绑定，并补齐前端具名借用 owner 的调用前缀
保护。独立复核发现 checked 算术遗漏 pending、合流别名仅在部分入边成立，以及插值无正常
出口未传播导致清理循环无法前进，均已修复并复核。标量身份断言初版误将默认 Borrow 参数与
Value 比较，修正为显式 own 参数后重跑。此切片不代表 temporary 控制流或 unit native 已完成。
