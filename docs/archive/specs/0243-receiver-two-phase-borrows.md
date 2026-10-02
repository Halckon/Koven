# SPEC-0243: Instance receiver 两阶段借用

> **性质**：历史验收 · **状态**：done · **读取时机**：追溯 receiver reservation/activation 验收时 · **唯一真源**：本 Spec 的范围与验收账本

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P3-0243` |
| 所属 Phase | Phase 3，及 Phase 4 直接消费合同 |
| 语言规范 | [Guide10](../../guide/10-ownership-borrowing-drop.md#two-phase-borrows方法接收者两阶段借用) |
| 批准依据 | 用户已批准演进计划及持续分阶段实施；仅真正未定义语义需重新决定 |
| 前置 Spec | 无；基于已合并 PR #9 的 main `2ad6967aebdbf91d18817d23b9cae1a8e1981d56` |
| 前置 ADR | 无 |
| 关联 ADR | [ADR-0016](../../adr/accepted/0016-interprocedural-borrow-abi.md) |
| 阻塞项 | 本 Spec 范围无未决验收；PR #10 保持 Draft，合并由用户决定 |
| 影响范围 | `lang-frontend`、`lang-codegen`、定向门禁与当前事实文档 |
| 语言语义变更 | 否 |

## 1. Goal

让规范允许的 `worker.update(worker.read())` 通过两条所有权入口；方法 receiver 的自动 Inout
借用在参数求值期间预留、在实际 call entry 激活。使用现有 unit backend
执行相同语义，同时继续拒绝存活 Borrow 参数与 receiver 的重叠。

## 2. 范围与非目标

- 保留 receiver 与参数的源码顺序、单次求值、稳定 place 和来源 Span。
- Reserved 可读及 shared reborrow；mutation、新 exclusive、move 仍为 L0135。
- callee 的 Borrow 实参保持到 callee 返回，不能通过统一提前终止参数 loan 放行激活。
- 普通 `&` 实参立即独占；具名方法的显式 receiver、隐式/显式 this 使用相同两阶段规则。
- 正常分支发布 CallEntry；return/break/continue 放弃本次调用，Nothing operand 不激活，
  Nothing callee 仍先激活再进入。错误不发布可执行 loan/drop/receiver facts。
- unit SSA 保存已求值 receiver place，参数完成后建立 exclusive loan；既有 loan end、
  temporary owner、writeback 和 CFG 运输继续由事实驱动。
- 不扩展单文件 instance receiver native、借用返回、NLL、Escapable、closure callable mode、
  deinit/资源词法析构、原子 replace/swap 或既有非本切片失败。

## 3. 实施与事实合同

1. 先记录 readonly 正例失败和冲突负例基线，再实现两入口状态机。
2. single `LoanFact` 与 unit `UnitReceiverOwnershipFact` 标明 receiver reservation，且仅在
   正常 call entry 发布可查询 activation point；未到达入口的预留仍可保留用于控制边清理。
3. backend 不重新推导借用语义；核对 reservation/activation identity 后，将 Exclusive
   `BorrowBegin` 放在所有 operand 完成之后，不重求值 receiver 或结束 callee 参数 loan。
4. 单入口 this 使用 nominal-qualified target。receiver 的保留能力不等于可复制的 borrow 值。
5. 同步 Architecture、Guide 派生测试和定向门禁；实现与最终验收分两次可复核提交。本地提交作者统一为
   `halckon <halckon0@hotmail.com>`。本地阶段未发布；随后用户批准持续按本地验证、草稿 PR、双平台 CI 发布，合并仍由用户决定。

## 4. 验收账本

| 验收项 / 命令 | 当前实际结果 | 后续 / 未运行 |
|---|---|---|
| `cargo test -p lang-frontend --test multifile_two_phase_borrows`（红测） | 最初 5 项：2 readonly 正例 L0135 失败、3 negative 通过 | 原始日志保留；后续扩展覆盖 |
| `cargo test -p lang-frontend --test ownership_two_phase_borrows`（红测） | 最初 5 项：1 readonly 正例 L0135 失败、4 通过 | this nested mutation 追加红测复现漏检 |
| `cargo test -p lang-codegen receiver_two_phase`（红测） | 最初 SSA/native 2 项均在 frontend L0135 失败 | 后续 CFG/native 检查 |
| 两入口直接 suite、精确 Span、facts、输入顺序、控制边与 rollback | single 18 + unit 27 全通过 | 覆盖 conditional capture 合流与 nested-call 释放的红转绿 |
| 13 个 ownership suites（命令见下） | 最终 437 passed / 0 failed / 0 ignored | 不运行 frontend 全量 |
| `cargo test -p lang-frontend --lib` | 180 passed / 0 failed / 0 ignored | 最终 Rust 实现快照 |
| `cargo test -p lang-codegen` | 544 passed，另 4 doctests passed；0 failed / ignored | 含 6 项两阶段 SSA/native；实际执行 class/value class 正常及提前退出 |
| `cargo test -p lang-cli -p lang-lsp` | CLI 66、LSP 26 全通过 | 公共产物的直接工具消费者 |
| fmt、严格 workspace clippy、workspace check、文档结构及 checker tests | 已通过；文档 449 篇，checker tests 45 项 | 最终提交前复核 |
| `bash scripts/check_stage_integration.sh` | 742 passed / 0 failed / 0 ignored | 59 个定向 targets |
| `bash scripts/check_guide_litmus.sh` | 201 passed / 0 failed / 0 ignored | 187 frontend + 14 codegen；不是全部 native / frontend 验收 |
| 新切片双平台 PR CI | PR #10 head `d82fae3` 的 run 36957502026 completed / success，8/8 jobs 无 skip | macOS/Ubuntu 实际计数见下；不是 main/PR #9 或 feature push 的结果 |

## 5. 交付条件

- [x] 正反例、phase facts、early exits 与清理全部通过定向回归。
- [x] backend 实际 native 正常/控制退出、源码顺序通过；borrow temporary lifetime 的 loan/drop 事实及 SSA 回归通过。
- [x] Architecture 与门禁明确已完成范围和 single native 既有边界。
- [x] 本地实现提交 `35cfc50` 及可验证 bundle/patch 备份已完成；验收账本以独立提交收尾。
- [x] 获准发布后已核对本切片 head 双平台实际 CI；本提交归档 Spec、同步 inventory 与索引，最终 head 验证记录在 PR #10。

当前没有需要改写 Guide 的未决语义。既有 caller shared loan、index identity 与能力边界
保持当前合同；不借两阶段接收者扩大其他语言功能。

### 最终 ownership 选择

```bash
cargo test -p lang-frontend --no-fail-fast \
  --test ownership_two_phase_borrows --test multifile_two_phase_borrows \
  --test ownership_checking --test multifile_ownership_checking \
  --test ownership_closures --test ownership_containers --test ownership_construction \
  --test ownership_iteration --test ownership_constants --test multifile_constant_ownership \
  --test ownership_rc --test ownership_structural --test ownership_nullable_when
```

本地使用 Rust 1.96.0、LLVM/Clang 21.1.8、Linux x86_64 + glibc，统一 Cargo target、
`CARGO_INCREMENTAL=0` 串行执行命令。最终 fmt、workspace all-target check 与
`cargo clippy --workspace --all-targets -- -D warnings` 通过；文档结构 449 篇、检查器
测试 45 项通过，`git diff --check` 通过。以上为原本地验收快照；随后新增的
macOS/Ubuntu 实际 CI 证据见下，不用已合并的 PR #9 或 main CI 替代。

复核还发现跨文件普通字段 assignment 的既有 mutability map 缺口可先报告 L0134；本切片
不把它改写成 two-phase 通过或顺带扩大字段 native。五项 multifile type 与五项编辑器
完整 corpus 历史失败未在本切片重跑/改写，其记录继续由 SPEC-0242 保留。单文件
instance receiver native、unit 非根字段/index receiver native、MoveOnly inline receiver
跨块写回仍是既有边界；本切片不声称普遍解除。

## 6. 草稿 PR 与双平台验收

用户批准发布后创建 [PR #10](https://github.com/Halckon/Koven/pull/10)，保持 Draft，
未自动合并。远端初始 head `d82fae3d6ee76c9b3902f101a679c33085566b64` 的 tree
`b03117907fb292a6dab80cc7ab57edd40531e651` 与本地最终 `c58e47b` 一致；47 个
blob SHA 与最终 tree 已核对。

该 head 的 [pull_request CI run 36957502026](https://github.com/Halckon/Koven/actions/runs/36957502026)
于 2026-10-02 验证为 completed / success，8 个 jobs 全部 success、无 job skip。
两个宿主的 workspace check、严格 clippy、core、stage、Guide 和汇总均实际执行。

| 宿主 | frontend lib | codegen / doctests | CLI / LSP | stage | Guide |
|---|---|---|---|---|---|
| Ubuntu 24.04 x86_64 | 180 passed | 544 / 4 passed | 66 / 26 passed | 59 targets / 742 passed | 187 frontend + 14 codegen passed |
| macOS 14 AArch64 | 180 passed | 543 passed、1 既有 ignored / 4 passed | 65 / 26 passed | 59 targets / 742 passed | 187 frontend + 14 codegen passed |

macOS 唯一 ignored 是既有 `lldb_hits_a_koven_source_breakpoint_and_reports_the_frame`，
CI 缺少 debugserver task-port 权限；未新增 ignore，也不将它计为调试器验收通过。
Guide 三个 codegen 选择另有 538 / 542 / 538 filtered，不计作通过。两平台的 6 项
`receiver_two_phase` SSA/native 均真实通过，包含 nested readonly 源码顺序与提前退出。

本次归档只表示上述有界 Goal 的验收完成，不扩大 §2 非目标和 §4 既有 unsupported
边界，也不表示 PR 已合并 main。归档提交不改 Rust 实现或测试门禁；其最终 head 的 CI
结果记录在 PR #10，避免为每次 CI 结果再生成一轮自指文档提交。
