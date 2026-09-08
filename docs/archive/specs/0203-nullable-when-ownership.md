# SPEC-0203：nullable `when` view 与 extraction 所有权

> **性质**：实施 Spec · **状态**：done · **读取时机**：实施或评审 v0.35 对应 Goal 时 · **唯一真源**：本 Spec

## 1. 元数据

| 字段 | 值 |
|---|---|
| 状态 | `done` |
| Goal ID | `KOV-P3-203` |
| 所属 Phase | Phase 3 |
| 语言规范 | 现行 [v0.35 空安全](../../guide/09-nullability-errors.md)与[阶段边界](../../guide/15-conformance-and-staging.md) |
| 批准依据 | 2026-09-08 用户明确启用 v0.35，按持续推进 Goal 分阶段实施；依赖未完成者保持 draft |
| 前置 Spec | SPEC-0028、0029 `done`；SPEC-0202 `done` |
| 前置 ADR | 无 |
| 关联 ADR | ADR-0017 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` ownership/drop facts/tests；Architecture |
| 语言语义变更 | 否；实施启用后的 v0.35 ownership 契约 |

## 2. Goal

完成后，ownership checker 消费 SPEC-0202 plan，为 nullable `when` 发布 non-owning proof view、
Copy/Consume extraction 与每分支 wrapper/inner drop facts。

## 3. 范围与需求

- null 判别只读 subject；non-null edge view 绑定同一 root/place/loan identity，不产生
  copy/retain/owner obligation。
- non-consuming use 复用 view；Value use 对 Copyable inner 复制，对 MoveOnly inner 只允许消费
  整个合法 nullable root/temporary，并拒绝 MoveOnly Borrow/Inout、field/element partial extraction。
- 每条 branch 精确跟踪 subject 未提取、已提取、null 与 diverging 状态；正常 join、return、
  break/continue 与 abort 不重复 drop wrapper/inner。
- MoveOnly Borrow/Inout/field/element extraction 分别使用 L0133/L0132/L0136，active loan 使用
  L0135，成功 consume 后再次使用为 L0131；Copyable inner 从这些 source copy 仍合法。facts/
  diagnostics 顺序确定，失败不发布半成品 validated ownership plan。

## 4. 非目标

- 不 lower inline nullable SSA/LLVM ABI；不实现 `!!`、borrow-return 或新的 loan lifetime。

## 5. 验收标准

- [x] field/element view 只关联内部单次求值 subject 的 root/place/loan identity，
  不能关联到分支中重新求值的字段或元素；不得复用该 proof 绕过 move/loan 检查。

- [x] 未消费 named subject 在 when 后或循环后续迭代仍合法可用；已转移的结果 owner 正常析构。

- [x] 一般 frontend nullable 的 read/view、Copyable copy 与 MoveOnly whole-root extraction 通过，
  覆盖 scalar/value/enum/String 及 class/Box/Rc 类型层事实而不依赖 LLVM 表示。
- [x] MoveOnly Borrow/Inout、field、container element、active loan 与重复 move 反例分别产生
  稳定 L0133/L0132/L0136/L0135/L0131。
- [x] null/non-null/else、comma alternatives、branch join 与所有控制转移的 drop facts 精确。
- [x] no retain/no duplicate owner/validated marker 与现有 ownership suite 回归。
- [x] Architecture 与实现事实同步。

## 6. 技术方案与边界

ownership 只消费 0202 typed plan及现有 place/loan/drop machinery；不自行重算 `when` coverage。
proof view 与 ADR-0017 语义对齐，但 frontend facts 保持 LLVM 无关。

## 7. 实施计划

1. [x] 建立 branch proof/extraction ownership facts → 验证：model 正反测试。
2. [x] 接入 checker/drop planner → 验证：move/loan/control-transfer 矩阵。
3. [x] 同步验收与 Architecture → 验证：按[分层验收](../../development/testing.md)选择目标测试与必要下游检查，并记录命中数。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | ownership facts、测试与完成文档 | `feat(frontend): own nullable when branches (SPEC-0203)` |

## 9. 未决问题

- 无语义未决项；R1–R3 已随 v0.35 启用，前置依赖见元数据。

## 10. 验证记录

验收以最后一次相关行为修改后的证据为准。历史 red 已由下列回归锁定，不重复执行同一门禁。

| 验收项 | 目标 / 命令 | 实际结果 |
|---|---|---|
| 单次 subject / place identity、field/element 重读限制、Copy/Consume 与条件 alternative | `ownership_nullable_when` 对应 facts 测试 | 26 项套件通过，含共享 loan 下 Copy、赋值失效、错误清空 |
| named subject 合流后及循环复用、temporary 正常/return/break/continue/abort | 同一 nullable 套件 | 通过；abort 无 unwind，转移先发布 LoanEndFact 再 drop |
| Borrow/Inout、field、element、active loan、重复 move | 同一 nullable 套件 | L0133/L0132/L0136/L0135/L0131 反例通过 |
| scalar/value/enum/String/class/Box/Rc；无额外 retain/share | `cargo test -p lang-frontend --test ownership_nullable_when owned_move_only_inner_can_transfer_the_whole_root -- --exact` | 1 passed / 24 filtered；增补 MoveOnly value/enum 与 Consume/no Rc effects 断言 |
| 提取结果 owner 恰好析构一次 | `cargo test -p lang-frontend --test ownership_nullable_when transferred_result_owner_gets_its_own_normal_drop -- --exact` | 1 passed / 25 filtered |
| 共享所有权、容器和结构契约 | `cargo test -p lang-frontend --test ownership_nullable_when --test ownership_checking --test ownership_containers --test ownership_structural --no-fail-fast` | 58 passed：26 + 16 + 12 + 4；包含最终 Copyable temporary cleanup 修复 |
| 非 nullable comma 匹配边 SSA / native 消费 | `cargo test -p lang-codegen --lib ssa::lower_frontend_tests`；`native_tests::comma_when_match_edge_cleanup_links_and_runs -- --exact` | 24 + 1 passed；native 内部运行 true/false 两进程。消费者回归发现的 Copyable 临时借用误生成 drop 已修复并转绿 |
| 格式与 frontend 静态检查 | `cargo fmt --all -- --check`；`cargo clippy -p lang-frontend --lib --test ownership_nullable_when --test ownership_checking --test ownership_containers --test ownership_structural -- -D warnings` | 通过；Clippy 的三处 collapsible-if 已按建议修正 |
| codegen 静态检查 | `cargo clippy -p lang-codegen --all-targets -- -D warnings` | 通过 |
| 公开 API 下游编译 | `cargo check --workspace --all-targets` | 通过（8m 24s） |
| Architecture / 分阶段交接 | ownership / SSA Architecture；SPEC-0204；`python3 scripts/check_docs.py`；`git diff --check` | 已同步；336 份 Markdown 结构检查通过，diff 检查通过 |

独立复审覆盖 control-result delivery、Copy/Consume proof、匹配边 owner、临时值及 pending call
生命周期。发现的提前条件执行、comma 漏 drop、Nothing 正常 flow、较早实参 owner 提前 drop
和控制转移漏 cleanup 均先取得 red，再由上述测试修复。最终增量复审确认 Copyable 临时借用仅保留 loan-end、不生成 owner drop；MoveOnly temporary 与 Group 的清理保持有效。

### 明确未通过或未运行的项目

- 全目标 `cargo clippy -p lang-frontend -p lang-codegen --all-targets -- -D warnings` 被未改动的
  `multifile_type_checking.rs:260` 的 `obfuscated_if_else` 阻断；相同源码已核对存在于 `ad02ae0`。
  不把受影响目标 Clippy 通过表述为全目标通过，不顺带修改该既有告警。
- 不运行 frontend 全量测试。单文件 facts 不扩大为 compilation-unit 或 nullable native 支持。
- ControlTransfer 的 `LoanEndFact` 尚未被 native lowering 消费；Phase 4 必须在对应 cleanup 前
  发出 BorrowEnd，已加入 SPEC-0204 验收。MoveOnly enum 后续 TypeTest 的主体重绑定亦不在本次
  native 证据内。
