# SPEC-0238：Guide 勘误与可执行 Litmus 前端门禁

> **性质**：实施与验证 Spec · **状态**：in-progress · **读取时机**：修正 PR #6 审查问题或复用 Litmus 门禁时 · **唯一真源**：本 Spec 的范围、验收及剩余交付

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-DOC-0238` |
| 所属 Phase | 文档治理与 Phase 1/2/3 验证 |
| 语言规范 | [Guide v0.40](../../guide/README.md) |
| 批准依据 | 2026-10-01 用户要求继续未完成计划，分阶段实施、验证与提交；整合 PR CI 后先审查最新 main 的 PR #6，再推进下一步 |
| 前置 Spec / ADR | 没有新增语义或架构决定；验证已启用 Guide，不把尚待交付的切片改为 done |
| 关联 Spec | SPEC-0237、SPEC-0232、SPEC-0235 |
| 基线 | 整合 `9b83ab2`；对应最新 main 审查快照 `3be83b5` |
| 分支 | `fix/spec-0238-guide-litmus`，按本阶段明确范围从尚未合并的整合 head 建立隔离 worktree；不写 main |
| 影响范围 | 五个 Guide 页面、frontend 测试、可复用脚本、当前更正账本与文档治理索引 |
| 语言语义变更 | 否；正文既有合同优先，修正示例语法与过宽表述，版本仍为 v0.40 |
| 阻塞项 | 本阶段仅本地提交；发布/CI/合并不在当前执行范围 |

## 1. Goal 与非目标

把真实文档勘误修好，并让规范性 Litmus 自动暴露当前诊断状态；不能通过削弱源码、忽略
失败或把 known-gap 当成功功能，制造“12 例全功能通过”。保留审计原文与所有冻结快照。

不实现 const/backend、return-when、receiver Reserved/Activate、deinit、replace/swap native、
RawPtr 或 Box 投影；不新增语言版本/ADR，不修改 workflow，不推送、新建 PR 或合并 main。

## 2. 实施边界

- `Counter.increment` 使用 Inout receiver；replace/swap 示例只采用现有参数模式 grammar；
  enum payload 去掉非法 `val`；Result 的 `success` 名称与 Guide11/prelude 对齐。
- Two-phase 文案区分嵌套调用已结束的 loan 与交付外层 callee 的 Borrow；后者到外层返回
  才结束，receiver 激活不得与之重叠。不新造 lifetime/NLL 规则。
- 直接提取 Guide15 的 12 段源程序，测试单文件与 unit 入口；缺口锁定阶段、code、精确
  Span 与源码片段，不使用 ignored tests。Litmus12 const 位运算示例保留规范要求。
- 无诊断的 typed Deferred/Error/缺失节点也逐例锁定，构造/type qualifier 非值目标与实际
  assignment/for-body 延后事实区分；确认最终 ownership 常量 capability，不冒称 typed 完整。
- 补独立 runtime/括号变体与负例，并补两入口 nested-loan call/end-span 事实断言。
- 现行更正与三个快照分界见 [更正账本](../../architecture/guide-conformance.md)；历史审计原文不改。

## 3. 验收

- [x] 文档修改前红测：Litmus5 与被改写成 runtime 的 Litmus12 被门禁捕获
- [x] 规范例子直接提取，21 个新测试锁定明确前端状态与缺口
- [x] nested argument loan 的单文件/unit 正例和重叠 Borrow 负例保持
- [x] 最终 fmt、check、严格 clippy、定向 frontend 与文档门禁
- [x] 独立复核与本地阶段交付准备（本提交为本地成果，不代表远端发布）
- [ ] 获准后的发布/远端 CI 与生命周期归档

## 4. 验证账本

2026-10-01，Linux x86_64 + glibc，Rust 1.96.0 / LLVM 21.1.8 / Clang 21.1.8。
Cargo 串行使用统一 target；未执行 frontend 全量或 native/macOS 验收。

| 命令 / 验收 | 实际结果 | 限制 |
|---|---|---|
| 新 `guide_litmus` 对未改 Guide15 的红测 | 14 passed / 2 failed；Litmus5 L0135，Litmus12 缺少规范 const 源码 | 测试框架最初的编译错误已先修复，此行是实际执行红测 |
| 新 `guide_litmus` 最终内容 | 21 passed / 0 failed / 0 ignored / 0 filtered | 10 个 Guide Litmus 诊断/ownership 检查通过（typed 部分仍有精确快照缺口），2 个诊断缺口；测试通过不等于功能全完成 |
| `cargo test --locked --offline -p lang-frontend --no-fail-fast --test guide_litmus --test ownership_checking --test multifile_ownership_checking` | 124 passed（21 + 31 + 72） | 只选三个相关 integration suites；两个新增 nested-loan 事实测试包含在内 |
| `CARGO_NET_OFFLINE=true bash scripts/check_guide_litmus.sh` | 文档442篇通过；124 passed / 0 failed / 0 ignored / 0 filtered | 最终脚本含21+31+72测试；输出明确 typed 未完整、known-gap 不等于功能完成 |
| `cargo fmt --all -- --check` | 通过，退出0 | 最终 Rust 内容 |
| `cargo check --locked --offline --workspace --all-targets` | 通过，退出0 | 包含下游编译；不执行各 target |
| `cargo clippy --locked --offline -p lang-frontend --all-targets -- -D warnings` | 通过，退出0 | 受影响 crate 严格 lint，无豁免 |
| `python3 scripts/check_docs.py` / `python3 -m unittest discover -s scripts/tests -v` | 442 Markdown / 37 tests 通过 | inventory 含0238，DAG按生成器更新 |
| `bash -n scripts/check_guide_litmus.sh` / `git diff --check` | 通过 | 脚本语法与差异空白 |
| 历史保全 | 当前 Guide 以外的历史 Guide、迁移账本与 PR #6 审计原文相对9b83ab2无差异 | archive 下仅自动生成的全量 Spec DAG 随新增编号变化 |
| 独立只读复核 | 无阻塞 | 完整性/一次替换/typed精确快照建议已落实；复核未冒称独立运行 Cargo |
| 新提交的远端 CI / native / macOS / frontend 全量 | 未运行 | 本地阶段，不改 workflow；不由诊断门禁推导后端可执行 |

## 5. 精确遗留与升级条件

- Litmus4：未分组的 `return when` 仍被当前前端诊断为 return 值缺失（L0087）；括号变体
  不是规范源码的替代。后继修复须更新原例为正例，并保留失败诊断边界回归。
- Litmus12：六个具名位运算的 const evaluator 尚未接入（L0156）；后继须验证单/unit 常量
  值、位宽与错误边界，然后另验 SSA/native。不能只把 expect-error 改成 expect-pass。
- receiver two-phase：Reserved/Activate 仍未实现；嵌套只读例仍 L0135，传入 callee 的
  重叠 Borrow 负例必须继续拒绝。现有 nested-loan 正例不证明 receiver reservation 已落地。
- Litmus11 unit for-body 的 LoopSource/ControlJoin/Assignment，以及单文件赋值 typed 延后事实
  仍需后继闭合。构造/type qualifier 不是 runtime value，其占位状态与真实缺口分别维护。
- RawPtr 名称/权限/capability、deinit 资源执行及原子置换后续阶段维持原边界；此处只记录事实。
- Linux CI 接线是后继独立切片，复用 `scripts/check_guide_litmus.sh`；当前 workflow 原样保留。
