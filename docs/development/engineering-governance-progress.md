# 整体架构与工程治理执行账本

> **性质**：分阶段执行账本 · **状态**：current · **读取时机**：继续已批准计划、选择下一批次或核验实际交付时 · **唯一真源**：本页维护治理批次状态；语言演进见[演进实施账本](../specs/evolution-status.md)

2026-10-02 用户批准[整体计划](engineering-governance-plan.md)。目标设计不代表已实现；
新行为仍按 Spec、长期架构变化按 ADR、语言变化按 Guide 的既有门禁执行。

## 批次状态

| 阶段 | 当前状态 | 本批交付 / 下一门禁 |
|---|---|---|
| P0 范围与基线 | 文档基线与 LSP 试点身份/窄测已复核；其余 Rust 迁移基线待后继 | 下表锁定 main、CI、工具和 129 targets；P2/P3 前补受影响断言/能力/性能样本 |
| P1a 文档生命周期 | PR15 已合并；文档范围 exact-head CI 通过 | 逐项复核 16 候选；15 项归档，0236 因 Span 证据不足保留 active；同步最终账本/索引/inventory/DAG/roadmap |
| P1b 0182 证据 | 待独立批次 | 保持 active；补真实 for→SSA/native 的次数、顺序、畸形产物与确定性 oracle |
| P2 测试结构与软上限 | LSP首片 PR16 已合并；尺寸护栏本批实施，待发布/CI | 49项新main历史baseline、明确base增长检查与例外登记；后续搬迁前补受控时间/RSS样本 |
| P3a/P3b 交接与编排 | 未开始 | 封闭普通 unit 能力与 provenance，后迁 const 和共享分析门面；不得合并能力边界 |
| P4 共享内核与双轨 | 条件阶段，未开始 | P3 稳定后逐域比较语义与 recovery，证据成立才收敛 |
| P5 current 教程 | 未开始 | 从受测 fixture 建新 tour 与示例门禁；不改冻结教程 |

## P0：文档首片的固定基线

| 项目 | 已核验事实 / 限制 |
|---|---|
| 源码 | `main 34189046319a8b727285d471596647d5de56996e`；2026-10-02 刷新远端，与批准计划一致 |
| 实现交付 | PR5、7、8、9、13、14 均已合并；逐份 Spec 追加最终 head、merge 与 CI 证据 |
| 主干 CI | [36979753900](https://github.com/Halckon/koven/actions/runs/36979753900)：8/8 jobs success；Ubuntu/macOS check、Clippy、core、stage、Guide 步骤实际 success |
| 原生命周期 | 17 active / 217 archive；复核后迁移 15 项，0182 与 0236 保留，结果 2 active / 232 archive |
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

## 0236 的新增验收缺口

[0236](../specs/active/0236-explicit-string-clone.md) 原 §5 第4项要求诊断与 Span 回归。
直接 `string_clone` suite 的双入口 helper 精确比较诊断 code，但没有 span/primary 断言；
生产 checker 使用 `name_span` 只是静态实现证据，不能替代回归 oracle。
因此不按批准计划中的候选数量机械关闭；本批保持 `in-progress`，后继单独补定向测试。
没有据此判定生产 bug，也不重跑无关全量测试或降低原合同。

## 持续更新与交付

- 每批写明范围、固定 main、受影响行为、实际命令、未运行项、PR/exact-head CI 与回退单位
- 本地验证后发 Draft PR；不自动合并、不自动转 Ready。CI 只证明实际选择的检查
- PR 最终 CI 结果留在该 PR，避免只追加同一轮外部状态而反复制造待验证 head
- P1a与LSP首片已在本轮按合并状态更新；后继批次仍从新main建独立分支，不倒填旧验收快照
- 回退仅撤销对应批次差异；不覆盖用户修改、不重写冻结验收、不通过降低断言修复门禁

## PR15/16 合并与 P2 尺寸护栏（2026-10-02）

- [PR15](https://github.com/Halckon/Koven/pull/15) 于本日合并，merge
  `353667587b46db9bc214730c51f9df8cca7ca748`。head `9b137faae184b14ecfda4ed6fb32b61323df6591`
  的[CI 36989856621](https://github.com/Halckon/Koven/actions/runs/36989856621)为文档范围通过；
  Rust矩阵合法skipped，不表述为本PR双平台Rust验收。2 active/232 archive事实保持
- [PR16](https://github.com/Halckon/Koven/pull/16) 随后合并为
  `4383509dbfb805f774581a29136fc46dd62504a4`。head `3d1c2a24ef92afdf35d1aec3982b58aa0b02fc06`
  的[CI 36991173937](https://github.com/Halckon/Koven/actions/runs/36991173937)双宿主8/8成功；
  [该main CI 36993317772](https://github.com/Halckon/Koven/actions/runs/36993317772)亦8/8成功
- LSP `server.rs` 1747→499行；11项迁移身份与其余15项、129 targets及生产字节保全证据见
  [首片验收](lsp-test-migration.md)。首片结构/正确性完成，不代表P2整体或性能采样完成
- 本尺寸护栏独立分支基于上述最新main，实际扫描596个手写Rust文件、49个超千行，
  `exceptions` 与 `generated` 均空；不机械继承旧50项，不拆任何Rust源码。
  [政策](rust-size-policy.md)涵盖生产/tests/helpers、baseline防增额、base真实尺寸收紧、
  rename/copy、明确例外与生成物登记，并将护栏接入每次CI与最终汇总
- 冷/热compile/link、RSS、重复性能样本、退化预算仍未测；没有以脚本耗时或旧CI执行时间替代。
  0182、0236的独立补强也不因本护栏自动视为完成

### 本尺寸护栏的本地验收与未运行项

- `python3 -m unittest discover -s scripts/tests -v`：94项通过，其中新护栏47项黑盒测试、
  原45项与新增2项CI接线/汇总policy测试；测试作者独立编写尺寸测试
- 新黑盒测试实际发现越过例外额度时诊断未显示已登记额度，修正后同一测试通过。
  独立review另发现多空字段错误受hash seed影响，补5个seed的红→绿回归并固定字段顺序。
  删除保留历史baseline是明确允许的政策，已测删除后重建超限不能复用额度
- `python3 scripts/check_rust_sizes.py --base origin/main`：明确merge-base为4383509，
  596手写/49历史超限/0生成物通过；基线与该main全部Rust逐文件重算一致
- `python3 scripts/check_docs.py`：461 Markdown通过；`cargo fmt --all -- --check`、
  `git diff --check`均通过。既有Rust/Cargo1.96.0工具链已启用，未新增依赖
- 本批没有Rust/Cargo源码变化；本地没有重跑Rust行为、native、frontend全量或macOS。
  workflow改动触发PR现有双宿主门禁，Draft PR与精确head结果待发布后记录在PR中
- 代码/测试/CI与政策/状态文档分成两个提交，可按本批diff回退，不降低既有测试或改写旧验收
