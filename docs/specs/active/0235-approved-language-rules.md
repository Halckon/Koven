# SPEC-0235: 启用四项已批准语言规则

> **性质**：Guide-first 变更合同 · **状态**：in-progress · **读取时机**：审查四项规则的规范启用、迁移与文档验收时 · **唯一真源**：本 Spec 的范围与验收；语言语义仍由 Guide 定义

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-DOC-0235` |
| 所属 Phase | 文档规范启用；先于相关 Phase 1–5 实现 |
| 语言规范 | 现行 [v0.39 Guide](../../guide/README.md) |
| 批准依据 | 2026-10-01 用户对取消调用处 borrow、移位屏蔽、deinit 顺序/只读 this、Str/String 显式转换与混合操作四项建议明确回复“同意” |
| 前置 Spec | 无；不依赖尚未整合的独立实现分支 |
| 前置 ADR | 无；本次不改变实现 ABI |
| 关联 ADR | [ADR-0018](../../adr/accepted/0018-string-owner-runtime-abi.md)，既有 String ABI 决定保持原文 |
| 阻塞项 | 最终分支审查、提交与 CI 尚未执行；具体转换 API 和 Str native ABI 不在本次交付范围 |
| 影响范围 | 根文档、`docs/`、文档检查器及其测试；无 Rust 或 Koven 源码修改 |
| 语言语义变更 | 是；已明确启用 v0.39 继承并取代 v0.38 |

## 1. Goal

让后续实现只有一个可追溯的现行规范入口，四项已批准规则相互一致，旧规范完整可查，迁移
代价与尚未定义的最小边界明确；不把文档启用写成编译器功能完成。

## 2. 背景与依据

v0.38 同时保留旧调用 Borrow marker、String literal owner 等正文与后来追加的软关键字、
Str/String 分层。本次按明确批准的四项选择形成新版本，不以修改旧快照的方式掩盖冲突。
来源为 main `d3e64a4` 的当前 16 页 Guide；其他独立分支不在快照内。

## 3. 范围与需求

- 四项规则的唯一权威分别落在[调用实参](../../guide/07-calls-lambdas-closures.md#typed-call-argument)、
  [整数移位](../../guide/04-expressions-operators.md#整数具名位运算与移位)、
  [deinit](../../guide/08-class-family-members.md#deinit-成员语法与资源析构契约)、
  [文本分层](../../guide/13-program-runtime-standard-library.md#静态-str-字面量与动态-string)。
- 关联能力表、类型相容、语法索引、常量物化用词和示例保持一致；不复制完整语义到 Spec。
- 完整保存 v0.38 的索引及 15 个领域页，只机械重算相对链接，逐页摘要与标题映射记录在
  [启用迁移账本](../../archive/migrations/v0.39-enablement.md)。
- 更新唯一 current 入口、Spec inventory/索引/DAG；强化 current marker 的版本、位置和全局唯一性，
  所有文档行数及默认读取预算保持不变。
- 两处旧 block 恢复/尾 lambda 文案按 v0.38 已批准的 block 分隔正文作同义一致性修正；
  不在此切片修改任何 parser 行为或额外 lambda 规则。

## 4. 非目标与缺口

- 不修改 Rust、标准库、fixture，不运行 Cargo，不合并其他 SPEC-0229–0234 分支。
- 不新增 Str→String 转换函数或构造器的拼写；现有 Guide、prelude 与 compiler-bound API
  没有可复用的转换契约，不能把 host 的 `to_string` 或内部 StringLiteral 当作语言 API。
- 不把 Str const/literal 及封闭文本运算的必要一致性更新扩张为普通调用 CTFE，
  不开放用户函数执行或其他常量类型。String 常量保持既有逐次 owner 物化契约。
- 不改变两 operand 同整数类型、双轨 drop、资源分类、异常 abort、不开放 borrow-return，
  不自行改写 accepted ADR 或决定 Str native ABI。

## 5. 验收账本

- [x] 当前 Guide 16 页为 v0.39；v0.38 完整快照保留，只有链接机械迁移
- [x] 四项规则及直接关联的语法、类型、能力、示例与迁移边界一致
- [x] current marker 检查器先有失败回归，再收紧版本/位置/唯一入口检查；预算未放宽
- [x] Architecture 只描述本 worktree 实际实现状态，没有宣称新规则已支持
- [x] 文档检查、检查器测试、DAG freshness 与快照/标题等价检查完成并记录结果
- [ ] 父任务完成最终 diff 审查、独立提交与分支 CI；状态/路径迁移保持一致

compile-pass/fail、AST/IR/native 输出：本次 N/A，仅启用文档规范，不作为实现验收。

### 后继实现必须覆盖的定向用例（本次未执行）

- `const val TEXT: Str = "中\0文"` 保留 UTF-8 bytes 与 Str identity；重复 use/copy 无 owner/drop
- `const val TEXT: String = "a" + "b"` 产生 String const，重复运行时 use 分别建立 owner
- Str/Str、Str/String、String/Str、String/String 的封闭 concat/equality 与普通文本运算同结果
- `const val TEXT: String = "text"` 类型失败；普通函数调用 initializer 仍按既有 const 资格拒绝
- 单文件与 compilation-unit 的 typed、ownership、SSA/native 各自验收；Str ABI 未封闭前不接 native

## 6. 实施与交付边界

1. [x] 保存旧版本与确定性来源证据，升版并对齐已批准规则
2. [x] 补充迁移与最小未决 API/ABI 边界，校准实现事实说明
3. [x] 更新索引、冻结 inventory 和 current marker 回归测试
4. [x] 重建 DAG，完成文档门禁与语义复核
5. [ ] 父任务完成最终 diff 审查、独立提交与分支交付

建议提交：`docs: enable approved language rules (SPEC-0235)`。
本分支仅针对这一 Goal；未完成的编译器实现继续使用独立后继 Spec。

## 7. 未决问题

本次文档交付范围没有未批准的规则选择。具体 Str→String 源码转换 API、Str native ABI 以及
Str 常量的实现验收仍是独立实施缺口，不能因该文档 Spec 获批准而默认实施。

## 8. 验证记录

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
