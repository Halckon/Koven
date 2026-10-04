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
