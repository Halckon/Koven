# SPEC-0204：pointer-like nullable `when` lowering

> **性质**：实施 Spec · **状态**：done · **读取时机**：实施或评审 v0.35 对应 Goal 时 · **唯一真源**：本 Spec

## 1. 元数据

| 字段 | 值 |
|---|---|
| 状态 | `done` |
| Goal ID | `KOV-P4-204` |
| 所属 Phase | Phase 4 |
| 语言规范 | 现行 [v0.35 空安全](../../guide/09-nullability-errors.md)与[阶段边界](../../guide/15-conformance-and-staging.md) |
| 批准依据 | 2026-09-08 用户明确启用 v0.35，按持续推进 Goal 分阶段实施；依赖未完成者保持 draft |
| 前置 Spec | SPEC-0034、0184、0196 `done`；SPEC-0202/0203 `done` |
| 前置 ADR | ADR-0017 `accepted` |
| 阻塞项 | 无 |
| 影响范围 | `lang-codegen` frontend lowering/SSA/LLVM/native tests；Architecture |
| 语言语义变更 | 否；实施启用后的 v0.35 pointer-like lowering |

## 2. Goal

完成后，owned whole-root/temporary 的 class/Box/Rc nullable `when` 消费 frontend
typed/ownership plans，经 verified SSA、LLVM、object/link/run 执行 null、non-null、else 与
extraction 分支。

## 3. 范围与需求

- 只消费 0202/0203 descriptor，不按 AST spelling 重算 remaining domain、copy/move 或 drop。
- 复用 ADR-0017 `NullableBranch` non-null view及必要的 `NullableTake`，保持 subject 一次求值。
- 所有正常/diverging edge 精确交付 block parameter 与 owner obligation；null drop no-op，non-null
  未提取 owner conditional drop，已提取 inner 只由新 owner负责。
- 首轮只接受 descriptor 标记为 owned whole-root/temporary 的 class、Box、Rc subject；Borrow/
  Inout/field/element subject 保持确定性 unsupported，等待 nullable-place/loan branch ADR。
- native 覆盖 MoveOnly view/consume/drop；IR/LLVM 验证无 wrapper allocation/tag/隐式 retain。

## 4. 非目标

- 不实现 inline/tagged nullable、`!!`、Elvis、safe call 或新的 nullable ABI。

## 5. 验收标准

- [x] native 覆盖只读 when 后复用 named subject、循环复用及提取结果恰好析构一次。
- [x] nullable when 用作后续实参时，消费 `LoanEndFact::point()` 的 ControlTransfer 结束边，
  在 return/break/continue cleanup 前发出 BorrowEnd；较早 Borrow 实参的 root/temporary
  在正常调用前保持存活，abort 不产生 unwind。用 SSA 与 native 反例锁定时序。

- [x] SSA/verifier 正反矩阵覆盖 proof source、edge、take、drop 与重复 owner。
- [x] owned-root/temporary class/Box/Rc 的 `null`+`else`、non-consuming view 与 MoveOnly extraction
  native 运行正确；不虚构 pointer-like Copyable 类型。
- [x] subject 一次求值、Rc retain/release 计数与 conditional drop 被 IR/native 测试锁定。
- [x] Borrow/Inout/field/element 与 inline nullable 继续确定性 unsupported；既有 nullable `if`
  的受影响契约回归。
- [x] Architecture 同步。

## 6. 技术方案与边界

扩展现有 `lower_when` 分派，复用 nullable type/terminator/LLVM adapter；不新增 ABI ADR或并行
nullable IR 类型。

SPEC-0203 的匹配边 drop 与精确 loan-end 事实必须在对应 CFG 边消费；不能只查询正常
`loans_ending_at(call)`，也不能在 active loan 下直接消费 ControlTransfer drop。

## 7. 实施计划

1. [x] 接 typed/ownership plan 到 SSA CFG → 验证：lowering/verifier 窄测试。
2. [x] 接 LLVM/native class/Box/Rc → 验证：object/link/run 与 IR 断言。
3. [x] 同步验收与 Architecture → 验证：按[分层验收](../../development/testing.md)选择目标测试与必要下游检查，并记录命中数。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | SSA/LLVM/native 与完成文档 | `feat(codegen): lower nullable when branches (SPEC-0204)` |

## 9. 未决问题

- 无语义未决项；R1–R3 已随 v0.35 启用，前置依赖见元数据。

## 10. 验证记录

实施前按[分层验收](../../development/testing.md)将第 5 节各项映射到实际测试目标/过滤器；
记录命中数、结果与未运行原因。同一状态下的有效证据只运行一次，不默认运行 frontend 全量。

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-27 roadmap 审计 | 通过 | ADR-0017/SPEC-0196 基元已存在，当前 `when` lowering 仍拒绝 pointer-like nullable subject |

### 当前实施验收映射

| 契约 | 定向验证入口 |
|---|---|
| class/Box/Rc 只读 proof 与 named subject 再用 | `ssa::lower_frontend_tests::nullable_when_read_views_preserve_owned_subject_for_reuse` |
| Consume/take、temporary 与控制转移 loan-end | `ssa::lower_frontend_tests::nullable_when_consumes_inner_without_duplicate_owner`、`ssa::lower_frontend_tests::call_lifetimes_tests` |
| null/non-null、Rc 计数与一次求值 | native 定向用例；Phase 3 facts 不替代运行结果 |
| unsupported source / inline、既有 nullable if | 保留相应 lowering 反例与 nullable if 定向回归 |

第一项 red：`cargo test -p lang-codegen --lib ssa::lower_frontend_tests::nullable_when_read_views_preserve_owned_subject_for_reuse -- --exact` 命中 1 项，在首个 class subject 上返回 `UnsupportedNode`，0 passed / 1 failed。

当前增量证据：`cargo test -p lang-codegen --lib ssa::lower_frontend_tests -- --nocapture`
28 passed / 0 failed，包含 class/Box/Rc view/reuse、take、temporary、较早 Borrow 实参与后续
return/break/continue、unsupported source 和既有 nullable `if` 回归。只读路径另断言 SSA 无
`shared.retain`/`heap.allocate`，LLVM 无 wrapper malloc/tag。CFG loan lifetime 与 checked
arithmetic owner 传递已分别独立复审；修复后重跑上述 28 项通过。

`cargo test -p lang-codegen --lib nullable_when_class_box_and_rc_sources_link_run -- --nocapture`
在包含控制转移、分配释放计数与 Rc 别名存活用例的版本上 1 passed / 0 failed；三类 owner
各 8 次分配、8 次释放。最终版本另验证 Rc 动态 retain 1 次、release 9 次，同一 native 用例重跑 1 passed / 0 failed。
BorrowEnd 的先后由 SSA verifier/定向用例证明，不由 native malloc 计数推断。

`cargo test -p lang-codegen --lib nullable_operation_tests -- --nocapture`：5 passed / 0 failed，
覆盖 proof owner/source、active view 下 drop、合法 edge/take 与表示边界。
`cargo test -p lang-codegen --lib verify_ownership_tests -- --nocapture`：16 passed / 0 failed，
覆盖互斥边 owner 传递、隐藏 live-in、缺失 transfer、重复 owner/drop 与 loan 冲突。
Clippy 首轮报告新增 lowering 两处 `collapsible_if`；等价合并后
`cargo clippy -p lang-codegen --all-targets -- -D warnings` 通过；最终 lowering 28 项重跑通过。
`cargo fmt --all -- --check` 通过。最终 native 计数与条件合并已独立复审。
按分层验收未运行 frontend 全量与 CLI build/run：本切片未改 frontend 或 CLI 编排。

文档归档后 `python3 scripts/check_docs.py`（336 Markdown）、
`python3 -m unittest discover -s scripts/tests -p 'test_check_docs.py'`（21 项）与
`git diff --check` 均通过。

独立复审另识别既有边界：较早的 own 实参遇到后续 checked arithmetic 时，临时 owner
尚未统一纳入参数间 CFG carry；该路径与本 Spec 的较早 Borrow 实参不同，未在本次扩展或验收。
