# SPEC-0235: 保留三项批准规则并延后字符串分层

> **性质**：Guide-first 变更合同 · **状态**：draft · **读取时机**：审查本地候选、字符串纠正及重基门禁时 · **唯一真源**：本 Spec 的范围与验收；语言语义仍由 Guide 定义

| 字段 | 值 |
|---|---|
| 状态 | draft |
| Goal ID | `KOV-DOC-0235` |
| 所属 Phase | 文档规范启用；先于相关 Phase 1–5 实现 |
| 语言规范 | 本分支候选 [Guide](../../../guide/README.md)；集成时需重基并升为 v0.40 |
| 批准依据 | 2026-10-01 用户对取消调用处 borrow、移位屏蔽、deinit 顺序/只读 this、Str/String 显式转换与混合操作四项建议明确回复“同意” |
| 前置 Spec | 独立 String.clone 切片须先集成，形成本次重基所需的真实 v0.39 |
| 前置 ADR | 无；本次不改变实现 ABI |
| 关联 ADR | [ADR-0018](../../../adr/accepted/0018-string-owner-runtime-abi.md)，既有 String ABI 决定保持原文 |
| 阻塞项 | 尚未发布；等待 clone-first 分支先集成，再重基、升版、归档真实 v0.39 并运行 CI |
| 影响范围 | 根文档、`docs/`、文档检查器及其测试；无 Rust 或 Koven 源码修改 |
| 语言语义变更 | 三项规则已获批准，但本分支未整合；Str/toString 切换按最新决定延后 |

## 1. Goal

让后续实现只有一个可追溯的现行规范入口，三项保留规则与最新 clone-first 方向相互一致，旧规范完整可查，迁移
代价与尚未定义的最小边界明确；不把文档启用写成编译器功能完成。

## 2. 背景与依据

v0.38 同时保留旧调用 Borrow marker、String literal owner 等正文与后来追加的软关键字、
Str/String 分层。首个本地提交曾按四项选择形成候选；用户随后批准 clone-first，Str/toString 切换延后。
保留其余三项批准，不以修改旧快照或旧迁移记录的方式掩盖先后决定。
来源为 main `d3e64a4` 的当前 16 页 Guide；其他独立分支不在快照内。

## 3. 范围与需求

- 三项保留规则的唯一权威分别落在[调用实参](../../../guide/07-calls-lambdas-closures.md#typed-call-argument)、
  [整数移位](../../../guide/04-expressions-operators.md#整数具名位运算与移位)、
  [deinit](../../../guide/08-class-family-members.md#deinit-成员语法与资源析构契约)。
- 字符串字面量、const、所有权与 ADR-0018 恢复既有 String 合同；Str/toString 延后，
  最新纠正证据见[字符串延后记录](../../../archive/migrations/v0.39-string-deferral.md)。
- 关联能力表、类型相容、语法索引、常量物化用词和示例保持一致；不复制完整语义到 Spec。
- 完整保存 v0.38 的索引及 15 个领域页，只机械重算相对链接，逐页摘要与标题映射记录在
  [启用迁移账本](../../../archive/migrations/v0.39-enablement.md)。
- 更新唯一 current 入口、Spec inventory/索引/DAG；强化 current marker 的版本、位置和全局唯一性，
  所有文档行数及默认读取预算保持不变。
- 两处旧 block 恢复/尾 lambda 文案按 v0.38 已批准的 block 分隔正文作同义一致性修正；
  不在此切片修改任何 parser 行为或额外 lambda 规则。

## 4. 非目标与缺口

- 不修改 Rust、标准库、fixture，不运行 Cargo，不合并其他 SPEC-0229–0234 分支。
- 不启用 Str builtin、Str const、混合文本操作或 toString；不扩张普通调用 CTFE。
- 不在本工作区实现或宣称 String.clone 已可运行；完整 API/ABI 与端到端验收归独立 clone 切片。
- 保留两 operand 同整数类型、双轨 drop、资源分类、abort，不开放 borrow-return，
  不改写 accepted ADR。现在不伪造尚未形成的 v0.39 归档。

## 5. 验收账本

- [x] 本地候选的 v0.39 标号及待重基门禁已明确；v0.38 完整快照保留
- [x] 字符串恢复既有 String 合同，其余三项批准规则未改变
- [x] current marker 检查器先有失败回归，再收紧版本/位置/唯一入口检查；预算未放宽
- [x] Architecture 只描述本 worktree 实际实现状态，没有宣称新规则已支持
- [x] 文档检查、检查器测试、DAG freshness 与快照/标题等价检查完成并记录结果
- [x] 字符串纠正完成最终 diff 审查与独立本地提交；状态/路径迁移保持一致
- [ ] 后续重基、v0.40 启用与分支 CI

compile-pass/fail、AST/IR/native 输出：本次 N/A，仅启用文档规范，不作为实现验收。

### 最新纠正的验证边界

本次只纠正规范：普通无插值字面量仍是 String，`const val TEXT: String = "text"`
保持合法，String/Copyable/Transferable、println/error 和封闭文本操作回到既有合同。
没有运行编译器测试，也不将旧 native 验收当成新 String.clone 支持证据。

## 6. 实施与交付边界

1. [x] 保存旧版本与确定性来源证据，升版并对齐已批准规则
2. [x] 补充迁移与最小未决 API/ABI 边界，校准实现事实说明
3. [x] 更新索引、冻结 inventory 和 current marker 回归测试
4. [x] 重建 DAG，完成文档门禁与语义复核
5. [x] 完成纠正 diff 审查与独立本地提交
6. [ ] clone-first 整合后重基、升版与分支交付

建议提交：`docs: enable approved language rules (SPEC-0235)`。
本分支仅针对这一 Goal；clone 切片先形成 v0.39 后，本 Spec 才能重基、升为 v0.40 并重开 active。
在此之前保持 draft，不与 clone 分支争用同一当前版本，也不宣称 main 已更新。

## 7. 未决问题

本次修正不新增语言选择。Str/toString 已延后，不再把它们的 API/ABI 当本轮必须关闭的门禁。
待 clone-first 分支整合后，必须重新核对整套规范、当前版本号、冻结 inventory、归档来源及 CI。

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
