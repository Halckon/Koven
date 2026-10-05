# SPEC-0277: Linux P2 证据接收与词法测试预期对齐

> **性质**：变更合同 · **状态**：in-progress · **读取时机**：接收、验证本批成本证据及测试修复时 · **唯一真源**：本 Spec

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P1-0277` |
| 所属 Phase | Phase 1 测试合同；工程治理 P2 证据交付 |
| 语言规范 | 已启用 [Guide v0.40 词法](../../guide/01-lexical.md) |
| 批准依据 | 用户授权云端验证后在最新Mac main独立Spec/分支接收证据并修复测试 |
| 前置 Spec | 无；接续已获批P2治理与既有迁移 |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无实施前置；成本接受与新分支CI仍开放 |
| 影响范围 | 两个frontend测试及其CI有界组合选择/失败传播合同、压缩证据、current开发入口与事实 |
| 语言语义变更 | 否 |

## 1. Goal 与固定身份

使用户能在本机完整复核Linux成本、失败/噪声及修复证据，并在最新main得到符合现行Guide的两个测试套件。
云成本/失败基线为 `c5c4a8df271858b23898e0bcf4b55d823c760bb9`，云修复提交为
`2aa75b0c86e9fe5cf4e2fc7600c657f7cc8c54c6`。Mac基线为创建时最新main
`2be64066a2011bb07a31bd68f9ac7441ab5a4baf`；worktree `/tmp/koven-spec0277`，分支 `feature/spec-0277`。
已核后继0275改变codegen、交付元数据及文档，frontend目录无变化，两测试自云基线未变；补丁适用，不回退0275。
另一会话0276及主工作区未提交文件不纳入本批。各SHA是不同提交，不宣称保留云提交身份或整树相同。

## 2. 有界范围与非目标

- 修复长hex被误认为非法radix的测试预期：保留不支持octal的负例，增加hex/bin非法尾部；旧exponent/suffix负例及四入口、诊断Span、Error/真实operator/rhs/sentinel不删。
- 重分类六个上下文词并补软词库存：保留旧120片段，增加到127；仍保留L0001–L0008及四入口产物合同。新增长hex/bin表达式正例不证明后续类型/数值溢出接受。
- 接收完整脱敏压缩包，保留旧raw、旧失败、冻结probe/summary、v2脚本与20个回归；不将5269小文件直接铺入Git。
- 只在Mac跑两目标红绿、格式、对应严格Clippy、文档/尺寸及v2 Python回归；不重跑全编译器或十组P2采样，不新增依赖、不改生产/Guide/配置。原接收阶段未授权push/PR/merge；本次后续交付授权见§9。

## 3. Linux口径与接受边界

Debian13.6 x86_64/glibc2.41、Rust/Cargo1.96.0、LLVM21.1.8；不是Ubuntu24.04 exact CI镜像。
十组历史配对以固定公开SHA、git archive、debug、incremental0、locked/offline、j2及单测试线程测量。
冷构建3对、目标重编5对、热执行10对、no-op3对；每次重编后首启独立，C child RSS预热后5对；不混阶段/cohort。
计时为创建进程至blocking wait4的墙时；compile归属为rustc减嵌套cc driver区间，sum可重叠，不当exclusive CPU/总墙时。
Linux RSS为KiB，保留Python launcher raw；测试child接受只取C observer内部wait4，不用launcher高水位代替。

v2修复冻结汇总漏报两类首启，仅重算14组/attempt、917条记录，不新增样本或改旧raw。
独立审计交叉比较462指标序列/118调查/12噪声；plan-tests冷产物首启第2对
0.161750947→0.218992066s，增长0.057241119s/35.3884%，触发单对调查，配对中位未触。
没有构建/C-child-RSS增量调查触发，不代表无噪声/等价/用户预算接受。
旧LSP exit101、runtime中断及旧Mac停止/噪声证据不覆盖。单次校准与补测不当额外正式执行样本。
调查线与诊断噪声线、用户接受是三件事；本批只交付证据，不自行豁免或关闭整体P2。

## 4. 验收标准

- [x] Library官方物化到Mac，核大小/SHA、安全路径和5266发布文件；完整压缩包可独立解包复核。
- [x] 核最新main差异、补丁适用性，并在最新Mac基线上实际复现两失败。
- [x] 两测试修复有Mac定向green，测试源字节等于云修复manifest；旧负例覆盖保留。
- [x] 汇总v2及20回归已接收，在Mac实际20 passed；不重采样。
- [x] fmt、受影响严格Clippy、docs/尺寸门禁、diff实际通过，Architecture与验收同步。
- [x] 本地分阶段提交及云/Mac SHA映射明确；新分支CI未运行、成本接受仍待用户决定。

## 5. 提交与材料

1. Spec范围、完整压缩证据与入口，文档门禁后本地提交。
2. 应用两测试修复，实际green与定向门禁、事实/账本同步后提交。
3. 记录第2提交精确SHA及最终验收，独立提交；保持active，未跑新分支CI不归档。

云包原报告/脚本/发布hash的唯一保存处是 [完整证据归档](../../development/evidence/p2-linux-20261005/koven-p2-transfer.tar.gz)。
接收、解包、复核入口见 [本机接收说明](../../development/p2-linux-cost-transfer.md)。
云base精确CI37248672159及云全量1704 passed仅是原SHA/宿主证据，不冒充Mac全量或新分支CI。

## 6. 验证记录

| 检查 | 结果及范围 |
|---|---|
| Library接收 | 首次下载失败后唯一一次官方helper重试成功；5,648,921 bytes、SHA256 `bf712bb7c7f47176e5422dabbebe556f31c04af39d39a12c5f244309811f3eb6`；未换渠道 |
| `verify_transfer.py <接收解包根>` | passed：5266发布payload文件、路径安全/原始与发布key相同，无样本执行；另3个hash/脱敏manifest完整保留 |
| `git apply --check <云format-patch>` | passed；两文件在Mac/cloud基线间逐字节无变化 |
| `cargo test --locked --offline -j 2 -p lang-frontend --test parser_long_invalid_number_boundaries --test parser_token_inventory --no-fail-fast`（修改前） | red：2 passed/2 failed，exit101，0 ignored/measured/filtered；默认unoptimized+debuginfo；编译13.03s |
| `PYTHONDONTWRITEBYTECODE=1 python3 <归档解包根>/test_summarize_v2.py` | Mac passed：20项，0 failed/skip；0.009s；不写pycache、不重采样 |
| 同两目标选择green（默认profile，进程内清除`CARGO_PROFILE_*`） | Mac passed：5 passed/0 failed/ignored/measured/filtered，exit0，编译0.83s；[实际日志](../../development/evidence/p2-linux-20261005/mac-green.log)、[命令与口径](../../development/evidence/p2-linux-20261005/mac-green.json) |
| `cargo fmt --all -- --check` | passed，exit0；未运行格式化写入 |
| `cargo clippy --locked --offline -j 2 -p lang-frontend --test parser_long_invalid_number_boundaries --test parser_token_inventory -- -D warnings` | passed，exit0，4.37s；只选择受影响两个测试目标 |
| `python3 scripts/check_rust_sizes.py --base 2be64066a2011bb07a31bd68f9ac7441ab5a4baf` | passed：781手写Rust/45历史超限/0生成登记；两文件504→579、380→396，未新增/修改例外或baseline |
| `python3 -m unittest scripts.tests.test_check_docs` | passed：37项；仅inventory变动，不扩展checker算法 |
| 文件、delta与覆盖独立比对 | 两文件SHA256与云manifest相同；测试diff SHA256 `595c64a2b22df45fcdce7799139bed02d85f644c2091cd21af0cc070f1514d07`与云相同；旧120库存片段全保留，新127；非法数字矩阵16→24，新增4种合法radix表达式 |
| 初始 `python3 scripts/gen_spec_dag.py`、`python3 scripts/check_docs.py`、`git diff --check` | passed：521 Markdown、1 live/261 archive、无空白错误；初始材料提交前 |
| 修复后 `python3 scripts/check_docs.py`、`git diff --check` | passed：521 Markdown、无空白错误；新分支文档/结构验收，不是远端CI |
| 新分支CI、Mac全量、P2重采样 | 未运行；仅影响两测试，用户授权证据接收，避免重复未变范围 |

没有改变公共接口、生产源码、Guide、依赖或Rust尺寸policy；因此没有追加workspace/native或未变的全部frontend回归。
云全量1704覆盖131 integration+lib+doc是云修复SHA的结果；Mac本次5项是最新基线的独立红绿证据。

## 7. 剩余事项

用户尚未决定噪声/首启触发及退化预算接受。该决定不阻止完整证据与修复交付，但整体P2仍开放。
新分支远端CI未运行；成本材料不证明最新0275或后继能力的性能等价。

## 8. 本地提交与云/Mac映射

| 身份 | 精确SHA及对应关系 |
|---|---|
| 云成本/失败基线 | `c5c4a8df271858b23898e0bcf4b55d823c760bb9` |
| 云两测试修复 | `2aa75b0c86e9fe5cf4e2fc7600c657f7cc8c54c6` |
| Mac新分支基线 | `2be64066a2011bb07a31bd68f9ac7441ab5a4baf`，包含后继0275 |
| Mac材料/范围提交 | `9c08a666c97336b260a6382d0f6e3594e69e54e8` |
| Mac修复/验收提交 | `550d540e27e1a174c7a5ba6917b383a186b136fa`；两测试文件及测试delta与云修复相同，不宣称整树或commit SHA相同 |

[机器接收/映射记录](../../development/evidence/p2-linux-20261005/receipt.json)与本表一致。
最终账本提交独立保存本表及实际验证。所有本地提交作者halckon `<halckon0@hotmail.com>`。
本机可复核材料与有界修复已交付；未合main、未push/PR；成本接受及新分支CI未执行，状态保持in-progress。

## 9. 后续交付授权与原始门禁补证（2026-10-05）

用户要求在合适时合并本地`feature/spec-0277`，授权接入最新main、推送、PR验收和合并，
取代§2原接收阶段不推送/不合并的工作边界；原三个提交及§6–8历史记录保留。
先完成PR54的最终head CI、merge与actual main闭环，再接入0277，保留0276新事实与inventory。
本片验收不依赖用户接受整体P2预算，成本接受继续开放，不将证据交付当作成本豁免。

独立完整审阅确认两测试符合Guide01，旧120片段与四入口负例/恢复合同保留，云/Mac测试bytes、
压缩包路径/5266payload摘要/917索引raw及云全量计数一致。审阅同时指出：初次Mac的v2、fmt、
Clippy、尺寸和docs仅有receipt声明，分支内未保存其原始门禁输出；云日志不补作Mac原始记录。

本次在精确`f18e0391a85ed60e75d48da5f56dc4103d96884c`重新验证，并保存
[交付前Mac复核账本](../../development/evidence/p2-linux-20261005/integration-preflight.json)：
两测试5passed/0failed/ignored/filtered、v2回归20passed/0failed/skip、transfer verifier5266payload、
fmt及受影响严格Clippy、docs521页/37checker tests、尺寸护栏均实际通过。原始输出无损gzip，
各项保存argv/cwd/exit/工具身份、压缩及原始SHA；Rust测试使用独立target、默认profile。
这批是新复核，不重建首次Mac的0.009s运行，未重采样或执行M4b实际故障校准。

最新main接入后的直接复验、精确PR head CI、归档、最终head CI及merge/main仍待后续实际执行；
不得将本机5项或旧云1704项作为这些新提交的验收。新的原始证据补齐后须独立窄复审。

接入准备另确认两个修复目标未在原77项CI组合中选择。按当前PR交付授权，本片最小追加
这两个完整target，使双宿主实际执行两矩阵；组合为79个唯一integration，Cargo调用仍10次。
选择一次、无filter及两目标失败传播由既有Python接线harness验证，不新增全量前端门禁。
首次接线合同红测3项、5个failure（缺两个target/缺失败传播/77≠79），生产选择尚未修改时
完成。接线后的完整Python政策验收、双宿主实际五项命中仍须真实记录，不提前记通过。

实际接线绿测：`python3 -m unittest scripts.tests.test_check_ci_results scripts.tests.test_recovery_gates`
初次33passed/0failed/skip；`bash -n scripts/check_stage_integration.sh`通过。红绿原始输出、exit、
选择数和被测working diff指纹见[CI选择验收](../../development/evidence/p2-linux-20261005/ci-selection.json)。
Python接线harness不冒充双宿主Rust运行，实际五项命中仍待PR CI。

独立接线审阅发现仅排除` -- `不能阻止Cargo的裸`TESTNAME`过滤。新增反例先得到
1项/1failure；随后以现有有界argv合同拒绝位置filter及libtest分隔符，完整政策复验
34passed/0failed/skip。初次33项、反例红测和最终34项绿测分别保留，不改写历史计数。
