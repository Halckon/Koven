# 整体架构与工程治理执行账本

> **性质**：分阶段执行账本 · **状态**：current · **读取时机**：继续已批准计划、选择下一批次或核验实际交付时 · **唯一真源**：本页维护治理批次状态；语言演进见[演进实施账本](../specs/evolution-status.md)

2026-10-02 用户批准[整体计划](engineering-governance-plan.md)。目标设计不代表已实现；
新行为仍按 Spec、长期架构变化按 ADR、语言变化按 Guide 的既有门禁执行。

## 批次状态

| 阶段 | 当前状态 | 本批交付 / 下一门禁 |
|---|---|---|
| P0 范围与基线 | 文档基线与 LSP 试点身份/窄测已复核；其余 Rust 迁移基线待后继 | 下表锁定 main、CI、工具和 129 targets；P2/P3 前补受影响断言/能力/性能样本 |
| P1a 文档生命周期 | PR15与后继PR17均已合并 | PR15归档15项；PR17补齐0236 Span证据后独立归档，当前1 active / 233 archive；0182继续独立补强 |
| P1b 0182 证据 | 待独立批次 | 保持 active；补真实 for→SSA/native 的次数、顺序、畸形产物与确定性 oracle |
| P2 测试结构与软上限 | LSP首片PR16与尺寸护栏PR18已合并；receiver首片本地验收通过，待review/PR CI | 46项拆至七私有领域模块，历史超限49→48；有限warm样本已记录，受控compile/link与预算仍待补 |
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

## 0236 的验收缺口与后继补齐

[0236](../archive/specs/0236-explicit-string-clone.md) 原 §5 第4项要求诊断与 Span 回归。
直接 `string_clone` suite 的双入口 helper 精确比较诊断 code，但没有 span/primary 断言；
生产 checker 使用 `name_span` 只是静态实现证据，不能替代回归 oracle。
因此不按批准计划中的候选数量机械关闭；本批保持 `in-progress`，后继单独补定向测试。
没有据此判定生产 bug，也不重跑无关全量测试或降低原合同。

后继[PR #17](https://github.com/Halckon/Koven/pull/17)为原14个测试补齐双入口精确Span、source identity与UTF-8 byte offset oracle；[首轮精确head CI](https://github.com/Halckon/Koven/actions/runs/36994788404)双宿主各14/14实际通过，8/8 jobs success，随后独立归档0236。上段保留P1首批识别缺口的历史；归档提交最终CI结果见同一PR，不扩大原语言范围。

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
  Rust矩阵合法skipped，不表述为本PR双平台Rust验收。2 active/232 archive保留为该批历史快照
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

## PR17 合并后的 PR18 同步（2026-10-02）

用户已合并[PR17](https://github.com/Halckon/Koven/pull/17)，main更新为
`55695ae8590c5bb5094496502f2e63010f865197`。归档head
`6ac644e38a1f344f7f242e0db1e3e4aadf6ff723`的
[CI 36996248457](https://github.com/Halckon/Koven/actions/runs/36996248457)双宿主8/8成功，
string_clone在两宿主各14/14实际通过；0236已归档，当前为1 active / 233 archive。
以上P0、PR15/16和PR18首轮验收保留各自固定基底的历史，不倒填为当时已完成0236。

PR18首轮head `d2380c6ecc4b885830f7798d0e82b4aca5c2dd96`的
[CI 36995565580](https://github.com/Halckon/Koven/actions/runs/36995565580)已经9/9成功。
本次从该远端head普通merge新main，保留双方父链，不force改写；唯一文本冲突是roadmap的
当前进度段落，解法同时保留PR15/16已合并、0236由PR17补齐归档和P2后继职责。
当前治理摘要同步1/233，原始Spec验收、P0历史及0236缺口发现记录均保全。

尺寸baseline仍以4383509为历史来源；新main实际逐文件重算为596个手写Rust文件与49项
超限记录，与原baseline完全一致，未重生成或提高额度。本次不新增Rust变化，不把PR17的Span补强归功于护栏。
新head的本地验证与双宿主CI独立核验，不能沿用首轮9/9；冷/热compile/link、RSS与重复
性能样本仍未测，0182仍待独立补强。

本次同步实际重跑：完整Python policy 94/94、docs 461 Markdown、尺寸护栏（base明确为
55695ae；596手写/49历史超限/0生成物）、fmt与diff检查均通过；DAG重生成后无diff，
live/archive为1/233。相对新main的全部Rust/Cargo文件diff为空，护栏实现、policy与测试
相对PR18首轮head亦无diff；没有重跑本地Rust行为或macOS，远端新head门禁结果留在PR18。

## PR18 合并与 codegen receiver 首片（2026-10-02）

[PR18](https://github.com/Halckon/Koven/pull/18)已于11:11:28 UTC合并为
`7318d53e4c5677684630e2b0b9148e9c6ceb1c35`。其最终head
`89a10905c858332a32b9dd2bdc0c171c024f6a8f`的
[CI 36998108953](https://github.com/Halckon/Koven/actions/runs/36998108953)为9/9 success；
新main的[CI 36999639756](https://github.com/Halckon/Koven/actions/runs/36999639756)亦9/9 success，
双宿主core/stage/Guide实际成功。以上PR18待CI文字保留为当时交付快照。

本批从该main独立拆 `unit_lower_receiver_tests.rs`：3383→34行入口，七个领域模块均≤829行，
46个完整测试块、原helper/imports逐字保全。完整libtest679项一对一映射、129 targets完整metadata
不变，旧filter前后各46 passed；生产代码、Cargo、workflow与其他测试不变。
尺寸policy仅删除该已消除的3383行历史条目，其余baseline/例外/生成物不变；真实超限49→48。
本地fmt、codegen严格Clippy、workspace all-targets check、普通release check通过；本地完整native
与macOS未跑。身份映射、实际命令、最小warm三轮时间/RSS及限制见[本片验收](codegen-receiver-test-migration.md)。
三轮no-op/filter样本不证明冷/热compile/link收益或退化预算；P2整体未完成。后续独立review和
Draft PR exact-head双宿主CI仍需核验，终态留在PR；不沿用基底CI冒充新head验收。
