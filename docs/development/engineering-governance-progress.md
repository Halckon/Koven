# 整体架构与工程治理执行账本

> **性质**：分阶段执行账本 · **状态**：current · **读取时机**：继续已批准计划、选择下一批次或核验实际交付时 · **唯一真源**：本页维护治理批次状态；语言演进见[演进实施账本](../specs/evolution-status.md)

2026-10-02 用户批准[整体计划](engineering-governance-plan.md)。目标设计不代表已实现；
新行为仍按 Spec、长期架构变化按 ADR、语言变化按 Guide 的既有门禁执行。

## 批次状态

| 阶段 | 当前恢复状态 | 验收与保留边界 |
|---|---|---|
| P0 范围与基线 | PR36 `201d415`本机只读核验；丢失后继对象不可用 | 按15组清单重建，不恢复原patch/SHA；用户改动保留 |
| P1a 文档生命周期 | 本地0 active / 248 archive | 0182、0255–0261仅按本机有界验收归档；没有推送/PR/远端CI |
| P1b 0182 证据 | 六项映射补齐本机证据 | 三容器owned/Borrow、资源/组件/ZST/原子性/确定性；unit-for仍拒绝，源码ZST不支持 |
| P2 测试结构与成本 | 有界职责拆分与尺寸护栏本机通过 | 751手写Rust、45历史欠账；原成本raw丢失，预算接受未授权，P2不关闭 |
| P3a/P3b 交接与编排 | 已发布普通/const交接在本机回归通过 | codegen外部2、frontend五组外部31项通过；不宣称原P3全部后继完成 |
| P4 共享内核与双轨 | 0255、0259/0260和0261有界重建/补验 | 真实LLVM失败、借用query、只读validator和最近callable修复；不宣称全P4完成 |
| P5 current教程 | 七正例/两JSON负例真实CLI通过 | 一planned不执行；保留旧教程，不改变Guide语义 |

本次代码重建及Mac选定范围验证完成；原P0–P5全部验收尚未完成。
唯一本机证据、15组落点、提交及未覆盖项见[恢复验收账本](recovery-local-delivery.md)。
以下历史批次原文与当时状态保留，不能作为本轮Linux/远端或成本原始证据。

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

[0182](../archive/specs/0182-sequential-for-lowering.md) 已有实现与 7 SSA / 8 native 专项执行证据。
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
[0249](../archive/specs/0249-owned-unit-borrowed-handoff.md)限定普通 owned-unit 的封闭六借用工厂、
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


## SPEC-0249 首轮双宿主验收与归档（2026-10-02）

[PR30](https://github.com/Halckon/Koven/pull/30) 首轮 head `410ed04c94608798d66bdebd2b3e6423cdf2b2cf`
的 [CI 37054717054](https://github.com/Halckon/Koven/actions/runs/37054717054) 9/9 success。
本地 `ffcef525` 与该远端完整 tree 同为 `192363b74005a9d40f660328b98ca4f65a2c61f4`，
不是以不同 SHA 的假定等价替代核验。双宿主17项合同＋7项关键回归逐名各恰1次 ok；
这一24项集合与 PR29 的原24基线不同。macOS codegen738＋既有LLDB1 ignored、CLI65；
Ubuntu codegen739、CLI66；两宿主 frontend lib187、LSP26、ownership iteration184及
stage/Guide实际通过，完整映射见[归档0249后继账本](../archive/specs/0249-owned-unit-borrowed-handoff.md#8-首轮-exact-head-双宿主验收与归档2026-10-02)。

0249 仅完成普通 owned-unit 封闭借用交接，当前1 active/235 archive；上节in-progress、
未发布和CI未开始文字保留为当时历史。0182继续active，P2其余职责、P3b/P4/P5及整体计划
未自动完成。归档文档新 head CI 仍待实际运行，最终结果留 PR，不反复追加 Git 外部状态；
本归档不自动改变 PR 的 Draft 状态或启用 auto-merge；最终合并遵循现有授权与最终门禁，
原外部审计排队次序不变。


## SPEC-0250 共享名称前缀首片（2026-10-02）

本片固定 PR30 merge `64ace382c2a634ebf19bab66da928242120930cd`，从刷新后的 main 建立
`feature/spec-0250`；0250 在 active/drafts/archive 与本地/远端分支均未占用。
[有界合同与唯一验收账本](../archive/specs/0250-unit-name-snapshot.md)限定 frontend 无 IO 的
UnitNameSnapshot 与 CLI project 首迁；原 SourceMap 身份、canonical 语法/metadata 配对、
同次 name/type environment、names首gate及basic/const/ownership/native后段保持。

新增完整名字/索引/诊断 parity 与外部rustc合同，CLI 三个 gate fixture 先在旧前缀基线实跑。
两新 frontend targets 进入原双宿主 stage 选集，policy红→绿，不新增workspace/dependency，
不扩 frontend 全量。全部 Cargo 串行，独立实现review、Draft/exact-head双宿主CI仍待核验。
本地命令、红/绿和未运行项只记录在0250账本，后续状态不能由本段静态实现描述替代。

P3b完整driver/bootstrap/LSP迁移仍未完成；LSP const/recovery和legacy协议本片不改。
0182、其余P2/P4/P5及整体计划未自动完成，外部审计继续排在整体计划之后。


### SPEC-0250 首轮双宿主与归档

[PR31](https://github.com/Halckon/Koven/pull/31)首轮head83a54d6的
[CI37062811973](https://github.com/Halckon/Koven/actions/runs/37062811973)9/9成功。
两宿主新snapshot7/compile4及完整project_cli9逐名各恰一次ok；原core、stage、Guide实际成功。
测试使用合成merge2ae137f，已fetch核实其tree1767f193与head完全相同，未将它描述为真实merge。
详细父链、提交映射、原日志链接、macOS既有LLDB1 ignored和有界结论见
[0250最终首轮账本](../archive/specs/0250-unit-name-snapshot.md#6-首轮精确-head-双宿主验收与有界归档2026-10-02)。

0250归档后当前1 active/236 archive；本节此前“待review/CI”保留为实施时快照。
只完成CLI project名称前缀，完整P3b/bootstrap/LSP与其余治理未完成；最终归档head另行窄review/CI，
终态留PR，由维护者决定合并，不自动转Ready或启用auto-merge，外部审计仍延后。


## SPEC-0251 LSP unit 名称前缀消费（2026-10-02）

从 PR31 merge `08c7b0966115f2f709cbd8a9257a6e8b2d704429` 建立 `feature/spec-0251`。
[有界合同与唯一验收账本](../archive/specs/0251-lsp-unit-name-snapshot.md)限定 unit_session 组合
唯一 UnitNameSnapshot，保持 names validation、typed recovery、const 无 owned、URI/UTF16、
overlay/version/last-good 与全部 send 后 commit；不改变 legacy、CLI 或 frontend API。

旧完整 LSP 26 基线、新宿主 oracle 旧前缀35项先绿；真实 owner 消费与错误映射先编译红，
最小迁移后完整37项绿。两层差分核同源完整名称与AST配对、跨map真实URI/UTF16全部输出；
相邻 snapshot/compile targets及工程门禁已本地通过，细目只记 Spec。独立实现评审、Draft PR
及精确head双宿主CI尚待后继，不由配置存在或基底CI替代。

0251尚未归档；P3b整体、bootstrap/legacy、const完整owner、其余P2/P4/P5仍未完成。
不宣称性能改善，整体计划之后的外部审计次序保持。


### SPEC-0251 首轮双宿主与归档

[PR32](https://github.com/Halckon/Koven/pull/32)首轮head9443e00的
[CI37068719547](https://github.com/Halckon/Koven/actions/runs/37068719547)9/9成功。
两宿主原26＋新增11共37个LSP身份各恰一次ok，0 ignored/filtered；双宿主check/Clippy、
core/stage/Guide实际成功。合成merge c62393e7已fetch核实tree9794ca21与head完全相同。
完整提交映射、父链、日志链接与有界结论见[0251唯一账本](../archive/specs/0251-lsp-unit-name-snapshot.md#6-首轮精确-head-双宿主验收与有界归档2026-10-02)。

当前1 active/237 archive；前节未发布、待评审/CI及尚未归档文字保留实施时快照。
归档新head另行窄review和最终CI，不由首轮替代；终态留PR，合并遵循既有授权与门禁。
P3b整体和bootstrap/legacy/const后继、其余治理、0182及延期外部审计边界不变。


## SPEC-0252 unit 基础所有权共享推进（2026-10-02）

从 PR32 merge `15dfb13a6d771f66fbcfa21ca8db2fed92e77868` 建立 `feature/spec-0252`。
[唯一合同与验收账本](../archive/specs/0252-basic-unit-ownership-driver.md)限定一个有界PR：
CLI project先迁、LSP unit后迁，共享typed basic validation→普通ownership；无IO、const或
宿主诊断策略。Outcome按值，不是完整owner或额外身份证明；LSP消费式恢复typed去除大clone。

旧main完整CLI输出oracle、新LSP差分先冻结；新API编译红后最小实现，保留typed非空gate、
NotBasic/const分流、raw诊断/deferred、entry优先级及last-good。真实运行结果只进0252账本，
配置接线不替代新head实跑；独立review进行中、尚未发布或CI，不预称本片完成。

### P3b 有界剩余与退出条件

- 本片完成后，project/unit_session共用名称前缀与基础ownership推进，const差异完整锁定
- 后续一只有界单文件Spec同时覆盖bootstrap与legacy analysis纯阶段门面，保留各自策略；
  不追加单文件→unit内核合并、basic/const合并或新整体snapshot作为本阶段必做项
- 四宿主parity、公开能力/身份合同与exact-head双宿主证据齐全，才可结项P3b
- const owned交接明确留作P3a剩余，不静默取消、不由P3b结项推定整个P3完成

批准计划正文继续保存原目标与基线；本节是范围收敛和实施状态。0182、P2/P4/P5及全计划后的
外部审计顺序不变，不据本片扩大完成声明或承诺性能改善。


### SPEC-0252 首轮双宿主与归档

[PR33](https://github.com/Halckon/Koven/pull/33)首轮head9b872bc3的
[CI37074981360](https://github.com/Halckon/Koven/actions/runs/37074981360)9/9成功。
两宿主新API6/compile4、新CLI9/原project9与完整LSP40各身份恰一次ok，完整targets
0 failed/ignored/filtered；core、iteration、stage、Guide及check/严格Clippy均实际成功。
合成merge b562cfb8已核父链base15dfb13＋head9b872bc3及完整tree08ac5566相同。
完整映射、日志与有界结论见[0252唯一账本](../archive/specs/0252-basic-unit-ownership-driver.md#7-首轮精确-head-双宿主验收与有界归档2026-10-02)。

当前1 active/238 archive；前节待review/未发布/CI为实施时快照。归档新head另行窄review和
最终CI，终态留PR并由维护者决定merge后核main。P3b单文件bootstrap/legacy、P3a const
owned交接、0182及其余治理仍保留；外部审计继续排在整个计划之后，不扩大完成声明。


## SPEC-0253 单文件阶段门面（2026-10-03）

从PR33 main `82610ab9` 建立 `feature/spec-0253`，先保留独立审阅的0252归档提交，
与[0253有界合同](../archive/specs/0253-single-file-analysis-facade.md)合并为一份后继Draft PR，
本地验收完成后才发布，避免中间push与独立归档PR重复触发CI。

独立设计评审批准固定纯runner加五阶段diagnostic gate和typed observer两个接缝；
封闭只读view绑定原parsed/names/typed，observer高阶借用不能逃逸，结果按值拥有raw产物。
CLI逐阶段早停与LSP完整recovery及definition先于ownership必须保持。
旧生产oracle、新API编译红/正负合同、宿主迁移与实际门禁只记0253唯一验收表。
本地旧host oracle先绿后迁移，最终完整CLI82/LSP45与17个frontend targets284及docs12通过；
设计/实现独立评审均Approve，工程门禁通过，细目与失败历史只保留在0253验收表。
当前尚未发布或运行本片精确head双宿主CI，不以PR33绿灯代替，不提前结项P3b。

P3a const owned交接、0182、其余P2/P4/P5与整体计划后的外部审计继续保留。


### SPEC-0253 最终双宿主与本地归档

[PR34](https://github.com/Halckon/Koven/pull/34)最终head3e3901d4的
[CI37082158020](https://github.com/Halckon/Koven/actions/runs/37082158020)9/9成功，独立raw日志
核验双host109有界身份（含22新增）各恰一次ok；真实merge c6b84ecb的完整tree d5a305f0
与本地95862ed、远端head及合成merge7c92af03均一致。主干
[CI37083070686](https://github.com/Halckon/Koven/actions/runs/37083070686)亦9/9成功，双host
实际checkout该真实merge，109/22身份再次逐项通过。完整映射、保留的macOS LLDB ignore
及平台差额见[0253唯一账本](../archive/specs/0253-single-file-analysis-facade.md#6-精确head双宿主验收与有界归档2026-10-03)。

前段未发布/待CI为本地实施时快照；本次按减少PR/CI频率要求，仅本地准备归档，随下一
相关批次发布，不另发文档PR。P3b的四宿主parity与公开身份/能力退出证据闭合；const
owned交接仍属P3a，0182、其余P2/P4/P5与后续外部审计继续保持。


## SPEC-0254 const owned-unit 封闭交接（2026-10-03）

从PR34真实main `c6b84ecb`建立`feature/spec-0254`，先携带独立审核的0253归档，
与[本片有界合同](../archive/specs/0254-const-owned-unit-borrowed-handoff.md)一起本地完成后发布一份Draft PR。
独立const六借用工厂、旧native/lower转接与CLI project const消费已实现；空常量仍保留专用
短路/ownership能力，LSP const无owned不变。旧生产oracle先锁，再做新API编译红与最小实现。

本地frontend267、codegen748+2+4、CLI82、LSP45、frontenddocs12及工程门禁通过；
精确双counter两fixture实测native4→1、lower2→1、factory1、预建view下游0。
次数仅证明重复index减少，不声称耗时/RSS收益；全部验证与失败历史只记本片唯一账本。
独立实现review已Approve；精确head双宿主CI尚待，不能提前归档或结项全部P3。
0182、其余P2/P4/P5及整体计划后外部审计仍按原范围保留。


## P2 unit runtime layout 生产职责拆分（2026-10-03）

从0254本地已验收head `2ec24abc` 独立建立 `feature/spec-p2-runtime-layout`，遵循已批准P2。
纯移动提交 `d79a692` 将 runtime demand升级与exact owner字段布局消费八函数抽到私有模块；
原resolver门面路径不变，parent3035→2658行、新模块398行，父文件其余字节不变。
八函数归一后逐字相等，48项七域测试及完整748项library身份不变；不进入P4或合并
单文件/unit的不同concrete-type算法。policy只收紧原额度到2658，余下历史欠账继续保留。

本地48项前后窄测、codegen748+2+4、workspace all-targets check、codegen严格clippy、
普通release check、fmt、102 policy与尺寸护栏通过；真实native roundtrip仍执行。
两次/侧dependency-warm compile/link与三次no-op/直接执行样本未触发本片采用的调查阈值，
不声称性能等价或提速。完整命令、边界、逐函数字节/身份hash和成本原值见
[唯一验收账本](unit-runtime-layout-migration.md)。独立源码与最终验收review均Approve。

本片仅本地交付，无远端写入；精确head双宿主CI尚未执行，不能由旧CI或本地结果代替。
0254、0182及其他P2/P4/P5和整体计划后的外部审计均不据此扩大完成声明。


## PR35 合并与0254本地归档（2026-10-03）

[PR35](https://github.com/Halckon/Koven/pull/35)携带0253归档、0254 const六借用交接及P2
runtime layout纯移动，已合并为 `9ac49f3ad3b1c8b0e5c762413f56bde719bf11d0`。
最终head `0abc8691`、合成merge `50f4c967` 与真实merge完整tree均为 `c79e4d87`；
[PR CI37123982795](https://github.com/Halckon/Koven/actions/runs/37123982795)为9/9 jobs、
77/77已记录steps success。每host新增25、const ownership20、planner48及const资源60按
phase/package/target/full_name独立核验，无异常；Ubuntu2446 passed，macOS2444 passed与
既有LLDB ignored1明确分开。完整平台差额与未执行范围只见
[0254最终验收](../archive/specs/0254-const-owned-unit-borrowed-handoff.md#6-pr35-精确head双宿主与归档收尾2026-10-03)。

真实merge的[main CI37124524536](https://github.com/Halckon/Koven/actions/runs/37124524536)
另有9/9 jobs、77/77steps success与双raw逐身份通过证据，平台计数及有界选集与PR一致。
上文0254/P2“仅本地、待双宿主”保留各自
交付时快照，当前状态由本节及唯一验收账本更新。归档后本地仅0182 active / 240 archive；
PR35真实main仍为0182、0254两active / 239 archive，不把本地状态冒充已发布。
P3a const封闭交接有界合同已具备PR验收；P2其余职责、P4/P5、0182与全计划后外部审计不关闭。
本批只有文档、inventory及生成DAG变化，随下一相关实施批次发布，不另开docs PR。


## SPEC-0255 中立 lowering 支撑边界（2026-10-03）

从PR35精确main `9ac49f3` 建立 `feature/spec-0255`，随批携带0254归档与三处已核实事实纠正。
P3主干双宿主9/9及97批raw审计、独立设计Approve均先于生产修改；旧helper6与双入口8项合同
先运行，前后同选择57项通过。新测试比较逻辑来源、StringOwner结构与definition→use关系，
包括source插入/inputs置换、同名旁源、泛型、const、recovery与拒绝优先级；single Borrow String
receiver原UnsupportedNode/unit成功差异保留，不强行合并driver。

既有decoder逐字迁移2089bytes，错误种类/字段及原crate-private re-export路径不变；三个同形
Some(span)构造由neutral私有helper承接，None span构造与原validation顺序保持。
依赖真实反向导入编译后判红、恢复绿均成立；完整codegen770+2+4、workspace check、严格Clippy、
fmt、102项policy、docs与尺寸通过；独立最终review已Approve，远端精确CI仍待。实际结果只记[0255唯一账本](../archive/specs/0255-neutral-lowering-support.md)。
不声称性能改善、全部P4或整体治理完成；0182、其余P2/P5与整体计划后外部审计继续保留。


## P2 multifile ownership 十二域（2026-10-03）

从0255已审阅本地head `481fb844` 独立叠加有界P2片，随0255与0254归档同一计划批次发布，
本片不单独push/开PR。原4692行入口降至141行；72测试及8 helpers完整原文保留，
12私有领域最大753行，单一integration target不变。source-qualified facts、Span、capture失败
保留closure记录的区别、deferred gate与nested loan end全部保留；详见
[唯一验收账本](multifile-ownership-test-migration.md)与逐函数机器证据。

前后各72/72及4条新exact实际通过；142 targets完整metadata、187库测试身份前后相等。
frontend all-targets check/严格Clippy、fmt、102 policy与尺寸门禁通过；policy仅退休原4692行
baseline，无新增例外，真实724手写/47超千行。两侧有限warm compile+link/执行/RSS样本
未触发预设复查阈值，不宣称提速；未重跑不受影响的workspace/native或frontend全量。
独立设计及最终review均Approve，docs482与逐块复核通过；精确head双宿主CI待后继同批PR。
P2其余职责、0182、P4/P5与整体计划后的外部审计均不因此关闭。


## 所有权到 SSA 规划边界组合里程碑（2026-10-03）

用户要求减少PR操作、在当前分支持续后续工作；本批改为一个完整责任里程碑。
保留0254归档、0255中立helper与multifile ownership十二域，在同一分支追加iteration有限图/
phi建立/入边转发，以及unit planner route/recipe/concrete type边界，两项生产迁移各独立commit。
完整函数、拒绝顺序、fixture与身份保全，不重写算法、统一不同type resolver或扩大能力。

新增受影响统一验收34个完整target共1743 passed（frontend187+22integration641+docs12、
codegen770+2+4、CLI82、LSP45）；workspace check/严格Clippy/fmt、102policy和尺寸通过。
两入口1876→275、2651→735；无新增例外，731手写Rust中45超千行继续如实报告。
metadata142与library187/770身份不变；成本两侧有限warm样本未触发预登记复查阈值，
不宣称提速。源码及最终文档/证据独立review均Approve，exact-head双宿主CI仍待；详见
[唯一里程碑验收](ownership-planning-milestone.md)。

本批基底的47个超限文件不是47个PR或必须全部拆分的任务，本批后为45个：原完整大场景例外继续保持，
数据模型等凝聚的大文件先审责任，不为凑行数机械切割。剩余P2/P4/P5、0182与整体计划后
外部附件审计均不自动关闭；0255仍active，未重复无变化主干的本地门禁。
