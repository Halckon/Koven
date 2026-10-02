# SPEC-0237: 八阶段本地整合与交叉契约验证

> **性质**：整合验收合同 · **状态**：done · **读取时机**：验证或交付八个独立阶段的统一集成时 · **唯一真源**：本 Spec 的整合范围及最终验收

| 字段 | 值 |
|---|---|
| 状态 | `done` |
| Goal ID | `KOV-INT-0237` |
| 所属 Phase | Phase 1 / 2 / 3 / 4 与文档治理整合 |
| 语言规范 | [Guide v0.40](../../guide/README.md) |
| 批准依据 | 2026-10-01 用户要求继续下一步，并随后授权先提交整合 PR、CI 通过后拉最新 main 审查 PR #6 |
| 前置 Spec | 无新增功能依赖；验证下列已批准切片的组合，不将未完成 CI 的切片假记 done |
| 关联 Spec | SPEC-0229、0230、0231、0232、0233、0234、0235、0236 |
| 前置 ADR | 无新增决定；保持各切片已接受 ABI/架构合同 |
| 阻塞项 | 最终整合门禁与 PR CI 尚待验收 |
| 影响范围 | 八阶段合并冲突、共享消费者与交叉回归、Guide/Spec/Architecture/inventory/DAG |
| 语言语义变更 | 不新增规则；仅整合 SPEC-0235 已批准的 v0.40，保留真实 v0.39 历史 |

## 1. Goal

在从 main 建立的 `feature/spec-0237-local-integration` 上形成可审计的八阶段整合 PR；
证明所选交叉契约与共享路径的实际结果，不以多个分支单独通过代替最终组合验证。

## 2. 范围与来源

- [0229 数值](0229-extended-numeric-literal-values.md)、[0230 Box enum](0230-recursive-boxed-enum-native.md)、
  [0231 TypeRef](0231-contextual-type-ref-trials.md)、[0232 原子原语 typed facts](0232-ownership-primitive-type-facts.md)、
  [0233 Compiler Contracts](0233-parser-compiler-contracts.md)、[0234 block 续行](0234-block-newline-continuation.md)、
  [0236 String.clone](../../specs/active/0236-explicit-string-clone.md)先形成真实 v0.39（`ed0727f`）。
- [0235 三项批准规则](0235-approved-language-rules.md)随后重基式整合为唯一 v0.40；旧 clone
  迁移记录保留原路径，0235 旧候选历史另存；完整前版来源见[v0.40 账本](../migrations/v0.40-enablement.md)。
- 检查共享 typed/ownership/SSA 与 numeric + clone + Box 组合，不扩大既有功能边界。
- 保持统一 Cargo target、串行 Cargo，按直接行为/共享契约/下游风险选择门禁；禁止默认全量 frontend。
- `scripts/check_stage_integration.sh` 交付49个定向 frontend targets及新增
  `clone_primitive_integration` 的可复现本地门禁。现有 CI 仅运行 frontend lib；integration
  matrix 为独立本地验收，不声称远端 CI 覆盖这些目标。本轮未修改 workflow。

### 后到的 main 来源

保存 v0.40 检查点 `f6d38ab` 后，合入 main `3be83b5`（PR #6 已合并）。保留其 const
位运算白名单、Box.value/unbox staged 合同、lambda/Litmus 修改与审计原文；String.clone
仍采用完整0236合同，空串不分配。冲突与来源见[协调记录](../migrations/v0.40-upstream-pr6-reconciliation.md)。
真实v0.39归档不随较晚上游改写；本轮不代替 CI 通过后要求的完整 PR #6 审计。

## 3. 非目标与遗留

不新造语言规则，不借整合实现 replace/swap native、deinit、Str、两阶段 receiver、unsafe
或其余演进计划。保留 multifile_type_checking 五项与 parser_call_argument 三项旧失败的
原始证据及本次实际复测结果；未修复不能写通过。Linux 本地结果不替代 macOS/远端 CI。

## 4. 验收标准

- [x] 八阶段冲突解决和独立交叉审查无遗漏，提交范围清楚
- [x] 所选 frontend、共享 Parser 资源、typed 事务与 drop planner 门禁逐项记录命中数/退出码
- [x] codegen/CLI/LSP 下游与新增交叉行为、真实 native 分配析构验证完成
- [x] 已知失败精确对照，不降低断言、不扩大忽略集合
- [x] fmt、严格 clippy、workspace all-targets check 与文档/历史保全门禁完成
- [ ] 提交整合 PR，最终提交的必需 CI 全绿且无未决状态
- [ ] CI 通过后获取最新 main，独立审查 PR #6 的 v0.38 审计与文档编辑，再决定下一步

## 5. 最终整合验证账本

2026-10-01，Linux x86_64 + glibc；Rust 1.96.0 / LLVM 21.1.8 / Clang 21.1.8。
Cargo 串行、统一 target；以下结果覆盖本次共享 trial 修复后的最终 Rust 状态。

| 验收项 / 命令 | 实际结果 | 边界 |
|---|---|---|
| 交叉红测 `cargo test -p lang-frontend --test clone_primitive_integration` | 修正 fixture 身份比较后 2 passed / 2 failed | unit 中错误候选 recovery Deferred 否决成功候选，single 同源码通过；未以删断言避开 |
| 新增交叉 target（最终5项） | 5 passed / 0 failed | 成功/失败/歧义 trial、候选顺序、single/unit/source identity；无错误 Deferred 候选仍不得过早提交 |
| `bash scripts/check_stage_integration.sh` | 50 targets，627 passed / 0 failed / 0 ignored / 0 filtered | 366 numeric/typed/ownership/String + 237 Parser普通 + 24资源matrix/stress；不是 frontend 全量 |
| `cargo test --locked --offline -p lang-frontend --lib` | 180 passed / 0 failed / 0 ignored / 0 filtered | 对齐现有 CI lib；包含 parser28、numeric decoder1、primitive validator2、drop planner96 等 |
| `cargo test --locked --offline -p lang-codegen -p lang-cli -p lang-lsp --no-fail-fast` | 606 passed / 0 failed / 0 ignored / 0 filtered | codegen510、CLI66、LSP26、codegen compile-fail doc-tests4 |
| 两项新增 numeric index + clone + Box enum native | 2 passed（包含上行） | 单/跨文件真实执行，UTF-8输出，输入顺序不变，分别4/5次 malloc 与 free 精确匹配 |
| `cargo test --locked --offline -p lang-frontend --no-fail-fast --test multifile_type_checking --test parser_call_argument` | 124 passed / 8 failed，退出101 | 恰好原99/5与25/3；名称、诊断及Span对照原干净基线，无新增失败、skip或弱化断言 |
| `cargo check --locked --offline --workspace --all-targets` | 通过，退出0 | 最终所有target编译 |
| `cargo clippy --locked --offline --workspace --all-targets -- -D warnings` | 通过，退出0 | 无新增lint豁免 |
| `cargo fmt --all -- --check` | 通过，退出0 | 包含最终新测试与trial修复 |
| `cargo build --locked --offline -p lang-cli` | 通过，退出0 | 最终真实CLI构建 |
| 文档检查 / Python检查器测试 / `git diff --check` | 440 Markdown / 37 tests / 通过 | 上游PR6协调完成后复验；结构门禁不代替语义审计 |
| v0.39历史归档 | 16/16 字节等价（仅声明的链接变换），191标题保留 | 来源 `ed0727f`；稍后进入的PR6不回写已冻结快照 |
| 远端最终提交CI / macOS | 待PR运行 | 不以Linux本地结果冒充，最终SHA与checks以PR为准 |

本次唯一新增生产修复位于 compilation-unit overload trial：已产生新错误诊断的候选中，
recovery `Deferred` 不再否决另一完整候选；无错误的真正未定候选仍保留原保守门禁。
单文件与跨文件 snapshot/restore 同时保留 String 与 ownership primitive facts。

原八项失败的完整名称与最早干净基线入口见[演进账本](../../specs/evolution-status.md#已知独立基线失败)。
本地合计1413个选定测试通过，另有基线套件124通过/8失败；不能把该范围称作frontend全量或全绿。

## 6. 交付边界

已获发布整合 PR 授权；不得自动合并、改写 main 或提前归档任何待 CI 的 Spec。
CI 通过后审查 PR #6 的后续工作仍以用户要求为界，不从审计结果擅自启用新语言规则。

## 7. 最终交付与关闭依据（2026-10-02）

本节补齐第4节末两项。前文“尚待CI”“不代表main”及原8项基线失败均为当时快照，
保留不改；最终范围按真实合并链与精确提交CI核对，不能使用PR正文的早期head说明。

- 交叉契约实现提交：`9b83ab28544ebc673494f57945d8988faeb020d3`；后继整合
  `aafccaa5a4679bfa33fe7c5132d67707d044b3e0`（0238）及
  `11051e200441a21cdf6dee6a6d153d2e9ffe26c6`（0239）。
- 最终交付：[PR #7](https://github.com/Halckon/Koven/pull/7)，最终head为上述
  `11051e200441a21cdf6dee6a6d153d2e9ffe26c6`；合入
  `e22e11b736aab1231209e3403bd0c931b9ddb940`，包含于复核 main
  `34189046319a8b727285d471596647d5de56996e`。
- 精确最终head的 [CI run 36877486546](https://github.com/Halckon/Koven/actions/runs/36877486546)：
  8/8 jobs success；两平台 check/Clippy/core/stage/Guide 步骤实际成功。
  PR正文旧 `9b83ab2` / run `36867583061` 及“远端无stage matrix”不代表最终交付。

| 第4节原验收 | 最终映射 |
|---|---|
| 1：八阶段与冲突范围 | 本Spec第2节来源链、`v0.40-enablement.md` 与 `v0.40-upstream-pr6-reconciliation.md` 保留；所有来源已位于最终PR7 ancestry |
| 2：typed事务、共享parser/drop路径 | `crates/lang-frontend/tests/clone_primitive_integration.rs` 五项：直接调用、候选顺序、失败/歧义不泄漏、跨source身份及真正Deferred仍保守；第5节50targets/627、lib180及后继0238增量分别记录 |
| 3：真实下游与交叉native | `native_string_clone_tests.rs::integrated_numeric_element_clone_moves_into_boxed_enum_and_drops_once` 和unit对应 `integrated_numeric_clone_and_boxed_enum_cross_file_facts_reach_native`；真实UTF-8输出与4/5次分配释放，最终双宿主core运行 |
| 4：基线失败与不弱化断言 | 第5节124通过/8失败保留；后继0242修复3项call-argument，0247修复5项multifile，分别独立验收，不倒填成整合时全绿 |
| 5：工具、文档与历史 | 第5节fmt/check/Clippy、440 Markdown/37 tests；真实v0.39的16页/191标题保全及PR7 Docs job，后到PR6不回写冻结快照 |
| 6：最终PR CI | 上述最终head的双宿主完整配置成功并已合并；stage始终是有界选择，未运行或未选择的frontend不据此算通过 |
| 7：CI后审查PR6 | [SPEC-0238](0238-guide-litmus-gate.md)以PR6 main `3be83b5` 和整合 `9b83ab2` 为独立快照完成核查；`docs/architecture/guide-conformance.md`逐项区分历史意见、规范事实和真实阶段缺口，后继提交aafccaa纳入最终PR7 |

原整合 Goal 与要求的后续审查已闭合，可按 `done` 关闭。本轮只复核代码、历史与
远端证据，没有重跑Cargo；不以本Spec承诺实现replace/swap、deinit、Str、unsafe或
全部Litmus。当前阶段支持范围继续由各实现Spec与Architecture维护。
