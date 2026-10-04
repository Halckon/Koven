# 本机治理续行与剩余退出条件

> **性质**：本轮有界验收记录 · **状态**：current · **读取时机**：继续 P0–P5 或核验本轮成本与教程时 · **唯一真源**：本页与原始证据；整体状态见执行账本

2026-10-04 从本地 main `7c7cc254a4e8e36ebfa2d858a334d98806c2091d` 继续
[已批准计划](engineering-governance-plan.md)。保留两处网站修改；只本地提交，不推送/PR。
恢复功能已经有[本机验收](recovery-local-delivery.md)，本批不重复未变编译器的完整回归。
旧恢复结果与本批结果分别记账；不重写原计划、历史日志或历史接受决定。

## P0–P5 逐项退出核对

| 阶段 | 已有证据 | 本轮剩余条件与处理 |
|---|---|---|
| P0 | 固定 PR36/main、工具链、能力矩阵、恢复清单及已知 filter | 原后继 SHA/raw 缺失已明示；本轮成本另固定两源码 SHA、参数与原值，不伪造历史 |
| P1 | 原归档首片/0236；恢复0182六项精确矩阵、0255–0261与文档/inventory验收 | 本地生命周期闭合；本批0262追加同轮验收，远端发布不在授权范围 |
| P2 | 十组历史结构配对可用；尺寸/growth护栏与身份/语义保全；45项旧欠账 | 本轮仅 LSP 成本试点；其余九组成本未重测，噪声/预算接受未闭合，非默认平台差额未补 |
| P3a | 普通/const sealed交接、混轮/clone/能力拒绝、原子失败、本机外部合同 | 动态index次数与性能对照有历史固定证据；旧profile/binary不全，Mac不能继承Linux性能结论；不因静态次数宣称提速 |
| P3b | CLI project/bootstrap、LSP unit/legacy共享门面；原host oracle与本机CLI/LSP回归 | 四宿主能力/恢复差异有有界parity；本轮Linux及新exact-head远端验证未运行 |
| P4 | 0255中立helper/配对/词法依赖guard；0260借用query与真实消费者；0261只读事实 | 仅按已验证垂直切片收敛，不全量single→unit；receiver/const/recovery差异保留；平台与成本差额单列 |
| P5 | 原七正例/两完整JSON负例、planned分离、编辑器/std/required接线 | 本批补自动借用、root replace/swap、concrete deinit、跨文件四正例；Mac实跑，Linux教程仍未运行 |

执行顺序：本轮LSP成本方法试点 → P5当前已证明能力的教程补齐 → 逐组复核其余P2成本预算
→ 在已有受支持环境补平台差额 → 汇总P3/P4配对与性能限制 → 原计划全部退出之后外部审计。
其余九组高成本实验没有本批授权；不能把一次试点扩大成十组实验。
45项历史超千文件不是45个必须拆分的任务，不以机械切片作为退出条件。

## LSP 原始成本证据

before `34189046319a8b727285d471596647d5de56996e`；after
`1f3991b6bbaeaf0a5f485bdb139b5b7efa07d3da`。两侧只差LSP测试结构，manifest/lock/toolchain
不变。macOS26.6.2/arm64、Rust/Cargo1.96.0；Rust内置LLVM22.1.2，不是native LLVM21.1.8。
两个隔离源码目录，空target冷样本；不clean、不复制target、不改当前工作树编译器。
`--locked --offline -j 2`、incremental0与linker计时wrapper两侧一致，Cargo串行。

[原始样本](evidence/p2-lsp-mac-20261004/samples.json)、
[方法与固定输入](evidence/p2-lsp-mac-20261004/protocol.json)、
[全部原始日志](evidence/p2-lsp-mac-20261004/raw.tar.gz)、
[逐文件SHA-256](evidence/p2-lsp-mac-20261004/raw-sha256.json)、
[实际采样脚本](evidence/p2-lsp-mac-20261004/probe.py)保留；没有加入二进制或缓存。
复现脚本需复制到独立临时目录执行；它是本次Darwin探针，不是通用benchmark框架。
首轮沙箱拒绝`time`的`kern.clockrate`只读探测，随后通过正常审批路径执行同一操作成功；
失败原值也保留，未绕过被禁止的系统读取。

| 指标 | 样本/侧 | before中位 | after中位 | 配对差值中位（after−before） |
|---|---:|---:|---:|---:|
| 构建产物缓存冷的总墙时 | 3 | 21.256s | 19.430s | −0.388s |
| 依赖缓存热、仅LSP target重编的总墙时 | 5 | 1.646s | 1.704s | +0.055s |
| 11项固定server样本直接执行 | 5 | 1.153s | 1.155s | +0.006s |
| 全fresh/no-op总墙时 | 3 | 0.078s | 0.077s | −0.000s |
| 冷构建最大报告进程RSS | 3 | 1964.625MiB | 1954.031MiB | 不作为整个进程树峰值 |
| 热构建最大报告进程RSS | 5 | 398.688MiB | 396.453MiB | 不作为整个进程树峰值 |

完整统计及全部配对差额见[summary.json](evidence/p2-lsp-mac-20261004/summary.json)。
各rustc/linker进程单独保留计时；rustc非link区间以该进程耗时减去嵌套linker，包含rustc前后
开销，不是exclusive CPU。多个rustc可能并行，进程时间之和不能冒充构建墙时。
Darwin RSS以bytes归一MiB；保留原始time输出，不套用Linux的KiB口径。

基线先校准：三次target重编、五次固定运行；之后登记调查阈值，再采after。
基线warm MAD约0.027s；执行校准出现0.18s与1.36s两类样本，正式执行也有1.37s样本，
全部保留，不能跨cohort混算。调查线为build `max(20%,1s)`、RSS `max(10%,64MiB)`、
run `max(30%,20ms)`；配对中位未触发，仅证明本次调查线结果，不是用户接受预算或无退化证明。
OS缓存、机器负载未受控；冷样本只表示空构建产物缓存。三/五个样本不足以宣布性能等价。

成功采样189.73s、target合计3.194GiB；包含首轮失败重试也远低于两小时。
剩余空间未低于40GiB、新增缓存未达12GiB；没有扩展九组实验或执行完整功能回归。
编译器未修改；执行身份严格对应迁移的11项，其他15项未执行。

## P5 本轮验收与停止条件

[SPEC0262](../archive/specs/0262-current-tutorial-plan-coverage.md)补齐四个明确教程空缺。
首次deinit夹具使用已有UnsupportedNode边界，改为既有concrete scope/deinit能力；
跨文件首次manifest名称不符合CLI协议，改为`project.toml`；未为教程扩展编译器能力。
最终四个新增例的build/artifact/run完整输出通过；未重跑原七正例与两负例。
六项提取/重复/遗漏/路径/selection合同通过；原始红、绿与CLI日志见
[本批证据目录](evidence/tutorial-0262/input-sha256.json)。

本轮实际检查：`python3 -m unittest discover -s scripts/tests -p 'test_tutorial_contracts.py' -v`
六项通过；`python3 -m unittest discover -s scripts/tests -p 'test_check_docs.py' -v` 37项通过；
`python3 scripts/check_docs.py` 496页结构通过；`git diff --check`通过。
原始压缩包与804个文件的SHA-256逐项核验通过。没有Rust实现变化，未重跑Cargo检查、
Clippy、原功能全量或旧教程；四新增例分段合计通过，不宣称一次全套教程脚本退出0。

已有OrbStack为Linux/aarch64，活动容器为其他应用；本地镜像中未找到现成Rust/LLVM验证环境。
只做只读查询，不启动/改造容器、不安装VM/工具链、不拉镜像或改变网络；Linux验证未运行。
禁止远端写入，因此本轮PR/CI闭环未执行；历史CI只能证明对应历史SHA。

完整计划满足完成条件时，必须逐项关闭原P0–P5验收：实际身份/语义/资源证据齐全，
结构成本的可接受噪声与退化预算有明确接受记录，受影响双宿主与required门禁实际运行，
生命周期/架构快照同步且无未解释缺项。当前不能给出完整完成日期，平台和预算尚未具备。
不得靠豁免、历史Linux数字、补写raw或机械拆完45文件宣布完成；外部审计继续后置。

## PR37 关系与后续只读补验

以上平台判断是`edac6c9`交付时的快照。随后只读核验发现现成CI结果，不改写先前未运行事实。
`origin/main`的PR37 merge `385bb23e1123c3a4ba00ec9fe5d964ebc95f1493`与恢复
`7c7cc254a4e8e36ebfa2d858a334d98806c2091d`完整tree都是
`eeee35fd9e9ec0b75245902a0b40ce24afe693b7`；全路径diff为空。
PR37第二父提交就是恢复head；本地`7ad9ed6`、`edac6c9`从同一恢复head衍生。
在该固定审阅head，图上是远端独有一个merge、本地独有两个后继；内容没有远端新增修改，
因此没有对应的内容合并冲突。没有执行merge/rebase/reset/fetch，后续提交仍只在本地main。
不把同tree等价说成SHA相同，也不据此推断未来推送可以fast-forward。

[可复现静态脚本](evidence/pr37-and-p2-static-review.py)与
[逐项结果](evidence/pr37-and-p2-static-review.json)冻结十组完整before/after SHA与tree；
每对Cargo/lock/toolchain输入diff为空。multifile type的120块、ownership的80块，
用实际保留的合并历史提交逐块核对旧fidelity记录，前后共400个SHA-256匹配；
另有iteration/planner九个完整生产文件hash匹配原记录。
这是历史迁移的静态差分证据，不是新行为测试或九组成本结果；缺失本地SHA没有被重新生成。

只读`gh run list/view`确认[PR37精确CI 37185473350](https://github.com/Halckon/Koven/actions/runs/37185473350)
为completed/success，10个jobs成功，Ubuntu24.04与macOS14的check/Clippy、
bounded composition与required汇总步骤实际success，不是配置存在或合法skip。
[原始job元数据](evidence/pr37-ci/run.json)、
[Ubuntu摘要与raw hash](evidence/pr37-ci/ubuntu-summary.json)、
[macOS摘要与raw hash](evidence/pr37-ci/macos-summary.json)及同目录压缩日志保留。
Ubuntu库frontend192、codegen788零ignore；macOS frontend192、codegen787通过及既有LLDB ignore1。
两侧组合均实际执行旧教程七正例/两负例，planned不执行。

固定本地审阅head的全部`crates/`与PR37无diff，恢复编译器及其相同测试可对应这份双宿主证据；
本地SPEC0262的四例、提取器与当前文档/脚本没有进入PR37，不能宣称新head CI或这些新例Linux通过。
本次只读取已经完成的run，不新触发CI，不推送，不改造Linux环境，不重跑未变功能。

| 下一项 | 可立即做的原范围工作 | 需要决策/条件 |
|---|---|---|
| PR37/本地关系 | 本轮完整tree、父链和影响文件已核对 | 无内容冲突待修；不自动整合提交图 |
| P2历史差分来源 | 本轮十对映射、400块与九文件hash已补验 | 其余九组成本没有授权，不自动测；LSP正式噪声/退化预算仍待接受 |
| P3/P4配对 | 0255八项双入口、source/input置换、receiver差异及八项guard已有真实库覆盖 | 未发现未覆盖的明确同语义切片；不为凑进度统一不同driver或重复旧测试 |
| P5新例与提取器 | Mac四例与六项合同已完成；本批文档补平台范围 | 新例Linux实际运行尚缺；不能用PR37旧九例代替 |
| 原计划收尾 | 保留准确账本与已有双宿主证据 | 当前本地脚本的exact-head required验收与成本接受均未闭合 |

建议下一步优先让当前本地教程/脚本在现有双宿主CI验证，避免为四例新装Linux工具链。
这需要另行允许发布本地提交；当前禁止远端写入，因此不执行。替代方案是提供已有且已授权的
Rust1.96/LLVM21 Linux执行环境。成本建议保持P2开放，先明确一个后继配对的范围、资源上限
及噪声/退化接受规则；不将调查线自动当豁免，不默认启动九组实验。
这些条件之外，目前没有定位到必须补写的原范围功能或轻量配对缺口。

## 本次统一发布授权与证据脱敏

用户随后授权本地实施/提交完成后，统一推送开发分支并创建一次draft PR；
该授权取代本批先前禁止推送的限制，仅用于Halckon/Koven，不包含自动合并或备份。
本地main后来已有外部加入的`e60f498`合并提交，内容相对`e4a8cb7`无diff；保留该历史。
发布分支从PR37 main建立，只承载本批脱敏后的净差分，不重写本地main或用户分支。
这样不会上传本地中间提交中的未脱敏证据版本。用户两处网站修改不纳入发布。

成本与红测日志的本机/CI用户路径替换为明确token，必要的工具链、指标、失败原因和
逐项命中保留；没有上传target、rlib/object/executable或Cargo缓存。
[脱敏审查记录](evidence/publication-redaction.json)记录原始/发布摘要与路径token；
[成本原始摘要索引](evidence/p2-lsp-mac-20261004/publication-original-sha256.json)保留脱敏前hash。
发布版raw与CI日志的摘要重新核验；所有JSON数值字段及原始归档中的指标未改变。
probe只把仓库位置改为调用者cwd，需从仓库根执行；原已执行脚本摘要保留，没有重测成本。

发布前846个文本/压缩归档payload的私密路径及常见凭据模式扫描通过，804份发布raw摘要
逐项通过。该扫描不宣称通用秘密检测；必要上下文、压缩包成员和净diff也人工按范围核对。
新精确head双宿主CI结果留在该draft PR，不沿用PR37通过状态，不以发布自动接受P2预算。
完整P2成本接受和其余九组高成本实验仍开放，不因这次PR收口而宣称整体计划完成。

静态复核脚本改以发布分支首提交`03395c6`核验关系，使完整克隆后的公开历史也可复现；
原`edac6c9`关系JSON保留。新[发布关系](evidence/publication-relationship.json)的400块与
九文件逐项结果与原记录完全相同；只更新发布分支父链，不重复行为测试或成本测量。
