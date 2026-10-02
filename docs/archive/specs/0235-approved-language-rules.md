# SPEC-0235: 三项批准规则在真实 clone-first 基线启用

> **性质**：Guide-first 变更合同 · **状态**：done · **读取时机**：实施或验收 v0.40 规范整合时 · **唯一真源**：本 Spec 的范围与验收；语言语义仍由 Guide 定义

| 字段 | 值 |
|---|---|
| 状态 | `done` |
| Goal ID | `KOV-DOC-0235` |
| 所属 Phase | 文档规范启用；先于相关 Phase 1–5 实现 |
| 语言规范 | 已启用 [Guide v0.40](../../guide/README.md) |
| 批准依据 | 2026-10-01 用户批准调用处取消 borrow、移位屏蔽、deinit 顺序/只读 this；后续明确 clone-first、延后 Str/toString |
| 前置 Spec | 无新增交付依赖；本地来源是已纳入 clone-first 实现的真实 v0.39，不把其 CI 未完成状态改写为 done |
| 关联 Spec | [SPEC-0236](../../specs/active/0236-explicit-string-clone.md)、[SPEC-0233](0233-parser-compiler-contracts.md)、[SPEC-0234](0234-block-newline-continuation.md) |
| 前置 ADR | 无；本次不改变实现 ABI |
| 关联 ADR | [ADR-0018](../../adr/accepted/0018-string-owner-runtime-abi.md)、[ADR-0027](../../adr/accepted/0027-explicit-string-clone-abi.md) |
| 阻塞项 | 本地文档验证进行中；已获整合 PR 发布授权，最终 PR CI 未完成 |
| 影响范围 | 根文档、docs、文档检查器及其测试；无 Rust 或 Koven 源码修改 |
| 语言语义变更 | 是；仅启用三项已批准规则，完整保留 String.clone，Str/toString 继续延后 |

## 1. Goal

在真实 clone-first v0.39 上启用唯一 current v0.40，保留完整历史和前序 Parser / Compiler
Contracts 成果；规范启用、实现证据与 PR CI 三者分别记录，不把任一项代替其余验收。

## 2. 背景与顺序

首次本地候选 `8181a97` 从 main `d3e64a4` 的 v0.38 出发；`929746d` 按用户最新决定撤回
Str/toString 切换，并暂退 draft 等待 clone-first。本次来源改为七阶段合并提交
`ed0727f93e72c2a795093faaa57b9a3f7e0531cf` 的真实 v0.39，重开 active/in-progress。
原 [clone 启用记录](../migrations/v0.39-enablement.md)保留原路径；
[0235 旧候选记录](../migrations/spec-0235-initial-candidate-v0.39.md)与
[字符串纠正记录](../migrations/v0.39-string-deferral.md)不改写旧结论。

## 3. 范围与需求

- 仅新增批准的[调用处自动 Borrow](../../guide/07-calls-lambdas-closures.md#typed-call-argument)、
  [位宽移位屏蔽](../../guide/04-expressions-operators.md#整数具名位运算与移位)、
  [只读 deinit body 与字段逆序清理](../../guide/08-class-family-members.md#deinit-成员语法与资源析构契约)。
- 完整保留 String.clone 的 shared Borrow、独立 owner、typed/ownership/SSA/native 合同；
  String 继续 MoveOnly + Transferable，旧 literal、const、拼接和比较保持，Str/toString 延后。
- 完整归档真实 v0.39 的索引和 15 个领域页，仅机械重算链接；逐页 SHA-256 与 191 个二/三级
  标题归属见 [v0.40 迁移账本](../migrations/v0.40-enablement.md)。
- 保留 SPEC-0233 Compiler Contracts 的九段迁移和全部检查；保留 SPEC-0234 Guide06/07 续行规则。
- 同步唯一 current 标记、页元数据、冻结 inventory、索引及 DAG，不放宽行数/路由预算。

## 4. 非目标与未闭环边界

本 Spec 不修改 Rust、标准库或 fixture，不实现三项新规范，不运行 Cargo。旧调用 Borrow
marker、具名位运算 const/SSA/native、deinit 执行/资源词法生命周期仍需后继实现验收。
不改双轨析构分类、移位 operand 同整数类型约束、abort、borrow-return 或 accepted ADR。
SPEC-0229–0234、0236 的实现与本地集成验收由 [SPEC-0237](0237-local-integration.md)汇总；
所有未获最终 CI 的 Spec 保持原 active 状态，不据此宣称 main 已更新。

## 5. 验收账本

- [x] 真实 clone-first 基线已整合，唯一 current 版本升为 v0.40
- [x] 16 页前版、两条 v0.39 历史线与纠正顺序保全；仅机械调整移动后链接
- [x] 三项批准规则一致，String.clone、MoveOnly/Transferable、旧 String literal/const 全合同保留
- [x] Compiler Contracts 与 Parser 续行澄清保留；Spec 回到 active/in-progress
- [x] 结构检查、完整 Python tests、快照摘要/标题与 diff 检查通过并记入下节
- [ ] 独立交付审查、获准发布、最终 PR CI 与归档

## 6. 本地整合验证

| 验收项 | 2026-10-01 本地 v0.40 检查点结果 | 边界 |
|---|---|---|
| `python3 scripts/check_docs.py` | 通过，438 Markdown | 生命周期、链接、current marker、页元数据、预算与 DAG freshness |
| `python3 -m unittest discover -s scripts/tests -p 'test_check_docs.py' -v` | 37 passed，0 failed/error/skipped | 含完整 Compiler Contracts 与版本唯一性回归，未放宽预算 |
| `python3 scripts/gen_spec_dag.py` | 11 live + 213 archive；full 224 | current/full Markdown 与 SVG 四份同步 |
| 真实 v0.39 快照 / SHA-256 / 标题归属 | 16/16 页逐字相等，191 个二/三级标题保留同页 | 只逆向本账本声明的机械链接前缀变换；来源 ed0727f |
| 既有 v0.38 archive、clone 原账本、Compiler Contracts 及九段迁移账本 | 全部与七阶段基线字节相等 | 两条 v0.39 历史线未混淆 |
| 0235 首次候选与字符串纠正记录 | 完整保留929746d正文，仅机械更新链接 | 字符串旧标题链接指向冻结 v0.38 页 |
| Guide13 clone 全合同 / Guide06 续行规则 | 除版本外与七阶段基线字节相等 | Guide07 保留 Compiler Contracts 与续行指向并应用 Borrow 新规则 |
| 冲突标记扫描 / `git diff --check` | 通过 | 不代替最终提交或合入更新 main 后复验 |
| Rust / native / Cargo | 本 Spec 未运行 | 代码整合门禁由 SPEC-0237 记录；本表不证明新三项规则实现 |

此为合入后续 main 更新前的文档检查点；后续相关输入改变须复验，不复用为最终 CI 证据。

## 7. 交付边界

在 `feature/spec-0237-local-integration` 整合后按用户授权提交整合 PR；不自动合并或更新 main。
最终 PR CI 前不将本 Spec 或其他实现切片归档；CI 通过后另按要求审查 PR #6。

## 8. 首次本地提交的验证记录

以下保留 `8181a97` 首版证据，不代表这次纠正或 clone 实现的验收。

| 验收项 / 命令 | 结果 | 未运行原因 / 边界 |
|---|---|---|
| `python3 -m unittest discover -s scripts/tests -p 'test_check_docs.py'`（红测） | 25 项，5 项失败 | 原检查器仅识别 v0.38，不能约束新版本唯一入口；已保存失败输出 |
| 同上（实现后） | 25 项通过 | 全部文档检查器测试，无 Cargo |
| `python3 scripts/gen_spec_dag.py` | 通过：3 live + 213 archive；full 216 | current/full 四份生成物同步 |
| `python3 scripts/check_docs.py` | 通过，403 Markdown | 链接、生命周期、预算与 DAG freshness 均通过 |
| 快照 SHA-256 / 逐页标题归属核对 | 16/16 摘要匹配，191 个二/三级标题保留同页归属 | 只逆向机械链接前缀变换；16 页 current 版本均为 v0.39 |
| 非 Git 空白/冲突标记扫描 | 通过，28 份核心变更文档/检查器 | 不代替最终 `git diff --check` |
| `git diff --check` / 父任务独立快照复核 | 通过 / 16/16 字节一致（仅声明的链接变换） | 最终分支提交后仍等待发布与 CI |
| Rust / native / Cargo | 未运行 | 无编译器实现变更；不把旧测试结果当新规范支持 |

## 9. 字符串纠正验证

| 验收 | 结果 | 边界 |
|---|---|---|
| `python3 scripts/check_docs.py`、25 项检查器测试、`git diff --check` | 通过，405 Markdown / 25 tests | 2026-10-01；仅文档修正 |
| v0.38 快照与非字符串三项规则 | 16/16 页与 main `d3e64a4` 等价；三项规则未变 | 仅逆向声明的机械链接变换；复核纠正 diff |
| Rust / native / clone | 未运行 | 由独立 clone 切片负责 |

## 10. 最终交付与关闭依据（2026-10-02）

本节承接第5节的最终交付项，保留第6–9节各时点的未运行、暂退 draft、授权与 CI
限制原义。被取代的是早期候选及旧版 Guide；本合同实际履行的是在真实 clone-first
基线上启用 v0.40，最终关闭结论为 `done`，不是把整份 Spec 标成 `superseded`。

- 最终规范整合提交：`f6d38ab3718f8b1c043618514e402fc538d5b5b2`，来源仍为
  `ed0727f93e72c2a795093faaa57b9a3f7e0531cf` 的真实 v0.39。
- 最终交付：[PR #7](https://github.com/Halckon/Koven/pull/7)，head
  `11051e200441a21cdf6dee6a6d153d2e9ffe26c6`，合并节点
  `e22e11b736aab1231209e3403bd0c931b9ddb940`；已包含于复核 main
  `34189046319a8b727285d471596647d5de56996e`。
- 精确 head 的 [CI run 36877486546](https://github.com/Halckon/Koven/actions/runs/36877486546)
  为8/8 jobs success，含 Docs & Spec Structural Gate；双平台 Rust 门禁属于组合交付证据，
  不能反向证明本次 Guide 启用已实现所有新规范。

| 第5节原验收 | 最终证据与关闭范围 |
|---|---|
| 1：真实 clone-first、唯一 current v0.40 | `docs/guide/README.md` 与 `scripts/check_docs.py` 的版本/入口约束；当前 Guide 保留0236合同，三项规则仅按已批准范围启用 |
| 2：16页、191标题与两条历史线 | `docs/archive/migrations/v0.40-enablement.md` 的逐页摘要/标题账本；本轮将16页归档只逆向声明的相对链接前缀变换，再与 `ed0727f` 原页逐字比较：16/16相等、191个二/三级标题同页同序 |
| 2–3：纠正顺序、String 合同与三规则 | `v0.39-enablement.md`、`spec-0235-initial-candidate-v0.39.md`、`v0.39-string-deferral.md` 保留独立来源；Guide07/04/08分别承载自动借用、位宽屏蔽、deinit顺序，Guide13保留clone与MoveOnly/Transferable |
| 4：Compiler Contracts 与续行 | 0233九段迁移账本与 `docs/compiler-specs/` 保留；Guide06/07 的0234规则继续存在，未被版本切换覆盖 |
| 5：结构、Python、快照与差异 | 第6节438 Markdown、37 tests及快照原始结果；最终 PR7 的文档 job 成功，当前生命周期变更另由本批文档门禁验证 |
| 6：独立审查、发布与最终 CI | 最终 PR7 已合并；[SPEC-0238](0238-guide-litmus-gate.md)承接 CI 后 PR6 独立核查及更正，保持历史审计和冻结快照 |

本轮未运行 Cargo；此 Spec 从未以 Cargo 作为文档启用的替代验收。调用迁移、位运算执行、
资源析构分别由 SPEC-0242、0240、0245 的实现合同承担；Str/toString、borrow-return
及其他非目标不因本次关闭变成已实现。旧候选8181a97、撤回929746d、真实v0.39、v0.40
四个阶段继续可追溯，不合并或改写其原结论。
