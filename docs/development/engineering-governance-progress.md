# 整体架构与工程治理执行账本

> **性质**：分阶段执行账本 · **状态**：current · **读取时机**：继续已批准计划、选择下一批次或核验实际交付时 · **唯一真源**：本页维护治理批次状态；语言演进见[演进实施账本](../specs/evolution-status.md)

2026-10-02 用户批准[整体计划](engineering-governance-plan.md)。目标设计不代表已实现；
新行为仍按 Spec、长期架构变化按 ADR、语言变化按 Guide 的既有门禁执行。

## 批次状态

| 阶段 | 当前状态 | 本批交付 / 下一门禁 |
|---|---|---|
| P0 范围与基线 | 文档基线与 LSP 试点身份/窄测已复核；其余 Rust 迁移基线待后继 | 下表锁定 main、CI、工具和 129 targets；P2/P3 前补受影响断言/能力/性能样本 |
| P1a 文档生命周期 | PR15/17已合并；独立0248亦由PR24完成归档合并 | 当前2 active / 234 archive（新增0249）；原P1批次历史1/233保留，0182继续独立补强 |
| P1b 0182 证据 | 独立确定性与conditional-break片已由PR22/23合并；0182仍active | 0248补Copyable Unit temporary-source子集；其余owned/Borrow、projection/cleanup和MoveOnly ZST未闭合 |
| P2 测试结构与软上限 | LSP PR16、尺寸护栏PR18、receiver PR19、plan PR25、iteration PR26、ownership integration PR27与multifile type PR28已合并 | 107项与13 helpers逐字保留，无新例外；原stage双平台选集已覆盖，同target不改CI；其余大integration与生产职责仍待后继 |
| P3a/P3b 交接与编排 | PR29合同片已合并；[0249](../specs/active/0249-owned-unit-borrowed-handoff.md)普通view实施中，编排尚未开始 | 封闭普通 unit 能力与 provenance，后迁 const 和共享分析门面；不得合并能力边界 |
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


## PR19/24 合并与 plan 七域首片（2026-10-02）

- [PR19](https://github.com/Halckon/Koven/pull/19)已合并为
  `f395dbccac1da6044476a4c5cb949b31880d0542`；head
  `20d588c73b6bc03cdfbbd81f0fc812018a8e8e6a` 的
  [CI 37001669457](https://github.com/Halckon/Koven/actions/runs/37001669457)为9/9 success，
  两宿主receiver46/46逐名通过。上节“待review/PR”保留当时快照，不再作为当前状态
- [PR24](https://github.com/Halckon/Koven/pull/24)已合并为
  `5e9a2954536055e674eed515709d34a65b354e21`。最终归档head
  `83e4693c88793bccf9b0358973c334837d149681` 的
  [CI 37026403861](https://github.com/Halckon/Koven/actions/runs/37026403861)为9/9 success；
  [0248](../archive/specs/0248-unit-container-storage.md)仅按其有界Unit存储合同完成，当前1 active/234 archive。
  0182的完整合同仍active；不可由Copyable Unit子集推定MoveOnly ZST等未决能力已完成
- 本片固定上述新main，`unit_plan_tests.rs` 3091→140行，48项拆成七私有领域模块，最大742行。
  730项libtest中48项一对一迁移、682项身份不变；原filter前后实际48/48，129 targets完整metadata相等。
  5 helpers、136处assert及48完整测试块逐字保全；只追加测试parent私有alias维持一个相对路径
- `unit_plan.rs`生产3061行、Cargo/依赖/其他测试/平台ignore不变。尺寸policy仅退休原3091行项，
  其余47项逐值不变；实际619手写/47超限。纯搬迁与文档/policy分commit，未重生历史baseline
- 本地fmt、codegen严格clippy、普通release check、48项执行、94 policy、docs和尺寸门禁通过。
  同一worktree的dependency-warm强制codegen compile+link两轮/侧、no-op和独立执行三轮/侧
  已记录时间/RSS，预设复查阈值未触发；不据小样本宣称提速。方法、原值和未测范围见
  [本片验收](codegen-plan-test-migration.md)。独立review、Draft PR及exact-head双宿主CI终态留在PR

P2后继仍按批准顺序推进iteration私有测试、大integration分组及独立生产职责拆分。
P3/P4/P5未因本片自动完成；全计划完成后的外部审计仍排队，未提前开展。


## PR25 合并与 iteration 私有测试首片（2026-10-02）

- [PR25](https://github.com/Halckon/Koven/pull/25)已合并为
  `9f9ee5230b6bc0affd2b7af727294c7f1bbc328b`；本片据此固定基线。
  上节plan“待review/PR”是当时快照，不再表示当前状态；不沿用基底CI证明本片
- iteration.rs14688→1876，生产1871行逐字不变；40项拆18私有领域，8共用helper留371行入口，
  3场景helper留其唯一调用领域。instance_replay独立树、12063行integration及Cargo目标不动
- 187项libtest中40项一对一映射、147项身份不变，旧filter前后40/40；完整metadata仍129 targets。
  51完整块等于从原文独立生成的同版rustfmt参考；448个literal逐字、1488处assert保全。
  169条super路径逐项解析同一目标，13块42处格式token编辑完整列账，不宣称测试块逐字未变
- baseline14688→1876，仅收紧原路径；conditional_leaf1155、file_parent1143、
  sibling_shared_loans1145登记新有限例外，不借旧baseline授权。实际619→638手写文件，
  47→50超千行含3新例外；生产欠账没有消失，没有压行/拆断言/include拼接
- 本地40 targeted、53 instance_replay、184 ownership_iteration、fmt、frontend all-targets
  check/严格clippy、普通release check、94 policy、文档和尺寸护栏通过。
  dependency-warm强制frontend compile+link两轮/侧，no-op与独立执行三轮/侧，
  预设调查阈值未触发；真正冷缓存、分离link及二进制大小未测，不宣称提速或性能等价
- [本片验收与可复现证据](drop-iteration-test-migration.md)记录精确身份、helper、路径、
  格式差分、全部测量值及未运行项。纯搬迁与文档/policy分commit；独立review和Draft PR
  exact-head双宿主CI仍待后继，终态留PR；合并由维护者判断

P2仍待大integration分组与独立生产职责拆分；P3/P4/P5没有自动完成。
用户要求的整体计划完成后外部审计继续排队，本片不提前插队。


## PR26 合并与 ownership iteration integration分组（2026-10-02）

- [PR26](https://github.com/Halckon/Koven/pull/26)已合并为
  `e30f9af5200b52c2c9fee7b831f5e63209608cb2`；本片固定该main。
  上节私有iteration“待review/PR”保留当时快照，不沿用其CI证明本片184执行
- `tests/ownership_iteration.rs`12063→77行，184项按20个真实领域分组，最大868行；
  checked共用helper留入口，六个完整场景helper留唯一调用域。没有拆case、去重或新增框架
- 191完整块逐字且等于原块独立同版rustfmt参考；1248 literal、1174 assert及22 support
  文件逐字保留，184新旧完整名一对一、前后无filter均实跑184；129 targets完整metadata相等
- policy只退休原12063行baseline，其他baseline和三个既有例外逐值不变；
  638→658手写Rust、50→49超限，无新例外，不把生产1871行欠账视为解决
- 该target此前不在stage/Guide执行选集；本片独立CI提交在既有双平台test job加完整target步骤，
  不加窄路径filter，不扩frontend全量，复用现有Rust PR/main/dispatch触发及fail-closed汇总。
  新增两项CI policy先红后绿，必须再核本PR两平台184实跑，不以check/clippy或旧CI替代
- 本地184 target、187 library（含40私有iteration与53 instance_replay）、迁移后exact1、fmt、
  frontend all-targets check/严格clippy、普通release check、96 policy、docs、尺寸护栏通过。
  integration-only dependency-warm compile+link两次/侧、no-op和独立执行三次/侧已采样，
  仅本target nonfresh；预设调查阈值未触发，不据此宣称提速或性能等价
- [本片验收与固定证据](ownership-iteration-test-migration.md)给出领域、完整身份/hashes、
  CI接线、采样原值与未测项；0182历史exact命令旁补当前映射，历史结果与未决合同不变。
  纯移动、CI接线、文档/policy各独立提交，独立review/Draft PR exact-head CI终态留PR

P2仍待其他大integration分组与独立生产职责拆分；P3/P4/P5没有自动完成。
整体完成后外部审计继续排队，不提前开展。


## PR27 合并与 multifile type integration分组（2026-10-02）

- [PR27](https://github.com/Halckon/Koven/pull/27)已合并为
  `b36d5040ef7cafaa86e2b0c6a42cc1d2ab49691d`；本片据此固定新main。
  上节ownership integration“待review/PR”保留历史快照，不沿用其CI证明本片
- `tests/multifile_type_checking.rs`7202→160行，104根测试拆17个类型领域，最大732行；
  原baseline_regressions209行/3项不动，合计107。9共用helpers留入口、4单域helpers随域
- 120完整块逐字且等于独立同版rustfmt参考；1559 literals、781 asserts、22 support文件保留。
  107新旧完整名逐项映射、前后无filter各107通过；129 metadata完整相等、187 lib身份不变并实跑
- policy仅退休原7202行项，其他45 baseline与3既有例外逐值不变；675手写/48超限，无新例外。
  本target已在双平台stage完整选集，不改CI；仍须核本PR exact-head两平台全部107逐名执行
- 本地107 target、187 library、两条新exact各1、fmt、frontend all-targets check/严格clippy、
  普通release check、96 policy、文档和尺寸护栏通过。integration-only compile+link两次/侧、
  no-op与独立执行三次/侧已采样，预设调查阈值未触发，不据小样本宣称提速
- [本片验收](multifile-type-test-migration.md)记录领域、完整身份/hash、受控成本及未测项。
  纯搬迁与文档/policy独立提交；独立review/Draft PR exact-head CI终态留PR，合并由维护者判断

P2仍待其余大integration与独立生产职责；P3/P4/P5未自动完成，0182仍active。
整体计划完成后的外部审计继续排队，不提前开展。


## P3a 普通 unit 前置合同测试（2026-10-02）

- 从 `main b36d5040ef7cafaa86e2b0c6a42cc1d2ab49691d` 独立补普通 native 六输入合同，
  不改生产/checker/native公开签名或index次数；[有界验收](unit-handoff-contract-baseline.md)记录逐项身份与限制
- 新增8项：六维独立替换（inputs含changed-root/duplicate/missing）共40个失败场景，
  精确kind/span、entry优先级、已有bytes保全、无新输出与目录不变；exact subprocess直接
  证明40次拒绝reserve0，六次匹配成功各reserve1，不增加生产hook或公开counter
- names/environment/typed/owned分别及全clone、重建/重排inputs、同T0重做ownership共8个
  成功组合，facts/verified SSA/object bytes一致，真实stdout/stderr/exit精确核验；
  owned-only负例必须是fresh T1派生O1，不将合法same-T0重查误判错链
- TestDirectory及其counter先独立纯搬迁，既有测试体不变；入口2074→2050，新domain525、
  helper31行，policy仅收紧原额度，无新例外。全部native unit消费者92项通过
- 已有24条exact逐条运行、basic/const七条compile-fail保持；本地其余门禁结果见有界验收。
  独立review、Draft PR与exact-head双宿主CI另验，不将基底CI或配置存在当完成证据
- 尚未构建封闭view或性能采样。Rust LLVM22与现有coverage21不匹配，未下载/插桩；
  动态index基线与受控无插桩对照留作生产片前置。P3a/P2整体及延后外部审计均未完成


### P3a合同片发布前同步PR28

[PR28](https://github.com/Halckon/Koven/pull/28)已合并为
`34861321d15830dc639e7641edd3a41524d57fc2`。本合同片独立review通过后普通merge新main，
保留上节PR27/multifile迁移时的历史验收，也保留本片全部合同测试与收紧额度。
三个codegen测试源逐字不变；索引/账本冲突同时保全双方条目，当前P2摘要标明PR28已合并。
同步后8项、native92、导入multifile107及必要工程门禁已重跑通过，确切范围见
[合同基线](unit-handoff-contract-baseline.md#发布前同步-pr28-主干)。发布前窄review与新head双宿主CI另验。


## PR29 合并与 SPEC-0249 开始（2026-10-02）

[PR29](https://github.com/Halckon/Koven/pull/29)已合并为
`7b3ac11fe1770339f2c5170981839477dae8cbf5`；本片从该main建立 `feature/spec-0249`。
[0249](../specs/active/0249-owned-unit-borrowed-handoff.md)限定普通 owned-unit 的封闭六借用工厂、
旧API转接与CLI ordinary消费者；const、ABI、语言语义和长期架构决定不变，不新建ADR。
本片验收只记入该Spec唯一账本；当前为in-progress，最小工厂与普通消费者已实现，
直接工厂1项、compile-contract7项及双路native9项通过；native/factory 和独立 lower 动态 index
及同期性能已测，噪声不能证明耗时改善；仓内摘要/JSON 独立窄核无 finding，Draft/CI 待验。当前2 active / 234 archive。

旧合同片的8项身份、24条exact、7项能力compile-fail及原红/绿历史保留。新增Display/Into、
两路reserve和same-T0已在9项中执行；compile-contract首轮5过2失败为诊断oracle的lifetime
文本不匹配，修正后7项全部通过，保留该失败历史。Architecture随真实实现同步；不提前宣称P3a
完成、性能改善或整体治理完成。

该实现新增两个frontend integration targets并接入现有双宿主stage选集，selection policy先红
后绿；全Python policy现97项通过。fmt、workspace all-targets check及frontend/codegen/CLI
严格Clippy已过；frontend九targets共253、typed bodies5、frontend docs12、完整codegen739＋
docs4、CLI66及build通过。metadata129→131；682手写Rust/48超限/0生成，policy仅收紧
model/lower/plan 三项历史额度。新增 fresh 链正例曾因 fixture 环境配对错误在类型阶段失败，
修正后 native 9 项通过，9 组合×双路共 18 组真实 object/link/run；fmt、workspace check 与
codegen strict Clippy 亦重跑通过。本地未跑整个 stage 或 frontend 全量。

固定 main 与实现 tree 的两 fixture 动态计数已核：旧 native 4→1、旧 lower 独立2→1、
factory 1、预建 view 的 native/lower 各0；入口与构造器双计数一致。14个 native/factory 与
16个 lower fresh exact 进程成功，后者没有执行原738/739项 libtest；不把 probe 当完整回归。
[仓内测量页](owned-unit-handoff-measurement.md)保留完整 counts 和104进程同期性能样本，
配对差额样本范围跨0且 setup 噪声触发，不能宣称提速、回归或等价。四路 object bytes 相同，
前后真实 link/run 输出42/248保持。生产、原始测量及仓内摘要/JSON 独立窄核均无 finding，
Draft PR尚未发布，exact-head双宿主CI未开始，不提前标完成。
