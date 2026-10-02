# 整体架构与工程治理执行账本

> **性质**：分阶段执行账本 · **状态**：current · **读取时机**：继续已批准计划、选择下一批次或核验实际交付时 · **唯一真源**：本页维护治理批次状态；语言演进见[演进实施账本](../specs/evolution-status.md)

2026-10-02 用户批准[整体计划](engineering-governance-plan.md)。目标设计不代表已实现；
新行为仍按 Spec、长期架构变化按 ADR、语言变化按 Guide 的既有门禁执行。

## 批次状态

| 阶段 | 当前状态 | 本批交付 / 下一门禁 |
|---|---|---|
| P0 范围与基线 | 文档基线与 LSP 试点身份/窄测已复核；其余 Rust 迁移基线待后继 | 下表锁定 main、CI、工具和 129 targets；P2/P3 前补受影响断言/能力/性能样本 |
| P1a 文档生命周期 | 本分支实施，待 Draft PR 与 exact-head CI | 逐项复核 0228–0242/0247、最终账本、归档、索引/inventory/DAG 与 current roadmap |
| P1b 0182 证据 | 待独立批次 | 保持 active；补真实 for→SSA/native 的次数、顺序、畸形产物与确定性 oracle |
| P2 测试结构与软上限 | 未开始 | LSP server 私有测试试点，再按批准顺序推进；先冻结身份/断言/target 与时间/RSS 样本 |
| P3a/P3b 交接与编排 | 未开始 | 封闭普通 unit 能力与 provenance，后迁 const 和共享分析门面；不得合并能力边界 |
| P4 共享内核与双轨 | 条件阶段，未开始 | P3 稳定后逐域比较语义与 recovery，证据成立才收敛 |
| P5 current 教程 | 未开始 | 从受测 fixture 建新 tour 与示例门禁；不改冻结教程 |

## P0：文档首片的固定基线

| 项目 | 已核验事实 / 限制 |
|---|---|
| 源码 | `main 34189046319a8b727285d471596647d5de56996e`；2026-10-02 刷新远端，与批准计划一致 |
| 实现交付 | PR5、7、8、9、13、14 均已合并；逐份 Spec 追加最终 head、merge 与 CI 证据 |
| 主干 CI | [36979753900](https://github.com/Halckon/koven/actions/runs/36979753900)：8/8 jobs success；Ubuntu/macOS check、Clippy、core、stage、Guide 步骤实际 success |
| 原生命周期 | 17 active / 217 archive；只在逐项验收成立后迁移 16 项，0182 保留 |
| 工具链声明 | `rust-toolchain.toml` 固定 Rust 1.96.0；同一云工作区启用既有工具链后实测 rustc/cargo 1.96.0、LLVM/Clang 21.1.8，host x86_64-unknown-linux-gnu；默认 PATH 未找到不代表工具未安装 |
| P0 实测 target 清单 | 固定同一 main 执行 `cargo metadata --locked --offline --no-deps --format-version 1`：129 targets；frontend 122（lib1 + integration121）、codegen lib1、CLI bin1 + integration3、LSP bin1、std lib1 |
| P0 LSP 试点基线 | `cargo test --locked --offline -p lang-lsp --bin lang-lsp -- --list` 列出26项；随后 `cargo test --locked --offline -p lang-lsp --bin lang-lsp server::tests` 实跑11 passed/0 failed/0 ignored/15 filtered；未实跑其余15项 |
| 实际影响 | Markdown、Spec 生命周期 inventory 和生成 DAG；无 Rust、Cargo、CI 行为或 frozen Guide/tour 正文修改 |
| 验证策略 | 本地 docs checker、全部 Python policy tests、链接迁移与 whitespace；PR 按现行 docs-only 过滤，Rust matrix 合法 skip 时明确记为未运行 |
| Rust 后继准备 | P2 前必须在可用工具链环境实际验证受影响身份/filter、target、fixture 与性能样本；本片不据静态审计或 LSP 单点结果承诺其他基线已完成；未重跑其余 Cargo/0182/native/benchmark/macOS |

## 0182 的保留边界

[0182](../specs/active/0182-sequential-for-lowering.md) 已有实现与 7 SSA / 8 native 专项执行证据。
其原勾选不能单独证明 empty/single/source-call-once、逐 CFG 清理、ZST/精确资源次数、
malformed/mixed products 与确定性已具备完整集成 oracle。前端已有事实测试可复用，
但不替代真实 for→lowering/native 的证据。独立批次先建立逐项映射，再补缺口；当前未证明生产 bug。

## 持续更新与交付

- 每批写明范围、固定 main、受影响行为、实际命令、未运行项、PR/exact-head CI 与回退单位
- 本地验证后发 Draft PR；不自动合并、不自动转 Ready。CI 只证明实际选择的检查
- PR 最终 CI 结果留在该 PR，避免只追加同一轮外部状态而反复制造待验证 head
- 本分支合并后，后继批次更新此页的 P1a 状态与 PR 链接，再从新 main 建独立分支
- 回退仅撤销对应批次差异；不覆盖用户修改、不重写冻结验收、不通过降低断言修复门禁
