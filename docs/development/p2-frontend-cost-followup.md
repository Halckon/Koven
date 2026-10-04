# P2 multifile ownership 本机成本续验

> **性质**：有界成本验收记录 · **状态**：current · **读取时机**：复核本轮 P2 样本或准备重测时 · **唯一真源**：本页及链接的原始证据；整体状态见执行账本

## 范围与实际停止点

2026-10-04，用户授权 PR38 合并后继续本地 P2 成本验证，不立即推送或创建 PR。
先只读确认 PR38 merge `b32602ec70dd6a88a9945be6f2398fc42fca6d93`，其 tree 与本地
`d454feaa91f8727d5fa7de61e4574b1db3b257a1`相同；正常 merge 到本地 main
`89e7b498ed96dd60253f2ef1d3ee6cc73211e182`，保留双方历史及两处网站改动，未 reset。
PR38 精确 head `095cf6374473bc41abbe1764c544701acad4bd56`的
[CI37188477831](https://github.com/Halckon/Koven/actions/runs/37188477831)实际10/10 jobs成功，
Ubuntu/macOS均执行新增四个教程及13个执行合同、一个planned；不把merge另称新CI。

选择既有纯结构迁移：before `f28359f6600fc663b837d8491988ec07c804059a`，after
`422786e4ee64de13fc49d1fac93907f67fcd98c6`。单一frontend integration target
`multifile_ownership_checking`，72测试、8 helpers、12私有领域；manifest/lock/toolchain
输入不变。先报告预计3–8分钟、2–3GiB，再执行；没有修改生产源码或新增抽象。
身份归一仅去领域前缀，72个叶名完整相等；所有实际执行均72 passed/0 failed/0 ignored。
重复运行仅用于成本采样，没有重复整个编译器功能回归。

正式采样完成，但after冷构建及两侧增量构建超出预登记噪声线，成本接受仍阻塞。
因此没有继续184项ownership iteration、107项multifile type或其他迁移，也未扩展九组实验。
剩余预算不是跳过噪声判定的理由，不能将本轮中位差当作提速或无退化证明。

## 固定条件与方法纠正

macOS26.6.2/arm64，Rust/Cargo1.96.0，Rust内置LLVM22.1.2；本target不依赖native LLVM。
复用已安装工具链和依赖，`--locked --offline -j 2`、debug、incremental0、相同linker
计时wrapper、单测试线程；每次只有一个Cargo命令运行。两个`git archive`源码快照，
六个独立空target冷样本；OS缓存与机器负载未受控，“冷”只指构建产物缓存。
热构建只touch对应test入口，每次Cargo JSON确认唯一该target非fresh；no-op全部fresh。
先基线校准，再采after；冷顺序ABBAAB，增量/运行逐对交替AB/BA，no-op逐对BA。

第一次校准混入首启：五次运行1.7355、0.1294、0.1285、0.1292、0.1320秒；范围超线，
采after前停止。第二次虽增加首启预热，但之后重编覆盖二进制，顺序错误；运行首个
1.1460秒、其余约0.13秒，仍在after前停止。第三次修正为**每次重编后独立记录首启**，
随后才测热运行；冷构建首次执行与每次增量构建后的首次执行也分开统计。
原两次停止样本完整保留，不删除离群值，不把首启墙时差未经证实归因给安全机制。
LSP先前约1.15秒直接执行也包含重编后首启，不能与本轮约0.15秒热运行跨cohort混算。

校准/正式噪声判断：MAD≤`max(20ms,15% median)`且范围≤`max(40ms,30% median)`。
第三次基线热构建median0.6755s、MAD0.0033s、range0.0608s；热运行median0.1406s、
MAD0.0044s、range0.0143s，均通过。调查线另为build `max(20%,1s)`、RSS
`max(10%,64MiB)`、run `max(30%,20ms)`，仅触发调查，既非接受预算，也非豁免。

## 完整结果与限制

第三次59条样本：13条校准；正式46条包括6冷构建、10增量构建、12首启、10热运行、
6 no-op与2身份清单。前两次停止样本不合并进正式比较。

| 墙时指标 | 样本/侧 | before中位(s) | after中位(s) | 配对差中位(s) | 噪声结论 |
|---|---:|---:|---:|---:|---|
| 空target冷构建 | 3 | 10.4769 | 9.8590 | −0.5473 | after范围8.1195s超线 |
| 仅目标重编 | 5 | 0.7759 | 0.8656 | +0.0993 | 两侧范围0.5347/0.4372s超线 |
| 重编后首次执行 | 5 | 1.0260 | 0.9960 | −0.0158 | 本次两侧范围通过 |
| 随后热运行72项 | 5 | 0.1452 | 0.1470 | +0.0036 | 本次两侧范围通过 |
| 全fresh no-op | 3 | 0.0469 | 0.0470 | +0.0001 | 本次两侧范围通过 |
| 冷产物首次执行 | 1 | 1.5711 | 1.7293 | +0.1582 | 单样本不能判断离散性 |

各阶段配对中位均未触发调查线，但第三对冷构建after17.2408s，比before10.4847s高
6.7561s，单对超出调查线；正式构建噪声使整批成本无法接受。保留每个配对而非仅中位。
逐进程raw定位到该慢样本frontend库rustc为15.403s，对应before为9.474s；该库源码
在这两个SHA之间没有变化。test rustc为1.381/0.700s，cc driver为0.455/0.142s。
增量首对两侧test rustc均约1.06s，较后续约0.66–0.76s高，link driver也有差异。
这些是耗时归属线索，不能证明机器负载的具体原因，也不能将全差额归因于测试分组。
冷构建最大报告进程RSS中位2002.453/2003.578MiB；热构建238.406/223.281MiB，
未触发本次RSS调查线。Darwin原始RSS为bytes；这些是各报告进程最大值，不是进程树
同时驻留峰值。rustc非link区间是elapsed减嵌套linker，包含rustc开销且可与其他进程重叠；
linker driver是完整cc调用，不将这些sum冒充墙时或exclusive CPU。

第三次实际114.82秒、新增2.360GiB，累计已知P2缓存6.185GiB（含LSP与两次停止），
结束磁盘剩余约63.36GiB。每次命令/每10秒轮询两小时、累计12GiB及剩余40GiB硬线；
没有触线、清缓存、安装环境或修改安全设置。本轮三个尝试均计入资源，失败没有隐去。

建议先在同一已支持环境、较稳定负载下按相同固定输入/参数重测此对，继续把首启与热运行
分开，保留所有样本；不要通过降低噪声线或选择性删除17.24秒样本获得接受。
如果噪声仍超线，调查构建/链接阶段的原始进程计时后再决定测量安排。当前停止扩展范围，
Linux成本、其余八组成本和用户明确接受预算仍未运行/闭合，整体P2与治理计划未完成。

## 可发布证据与本批验证

[固定条件](evidence/p2-multifile-ownership-mac-20261004/protocol.json)、
[实际执行脚本](evidence/p2-multifile-ownership-mac-20261004/probe.py)、
[校准](evidence/p2-multifile-ownership-mac-20261004/calibration.json)、
[全部第三次样本](evidence/p2-multifile-ownership-mac-20261004/samples.json)、
[身份](evidence/p2-multifile-ownership-mac-20261004/identities.json)、
[逐阶段统计](evidence/p2-multifile-ownership-mac-20261004/summary.json)与
[三次完整text raw](evidence/p2-multifile-ownership-mac-20261004/raw.tar.gz)一并保留。
探针从仓库根调用、复制到新的独立临时目录执行；Darwin专用，不是通用benchmark框架。
原先每次失败脚本只作归档，含路径token的历史配置需要提供实际目录后才能重现。
第三次探针累计检查已知同级临时缓存及本次target，复核时仍需纳入其他已有缓存。

438个归档成员全部UTF-8 text，不含源码快照、二进制或缓存；路径替换为明确token，
[脱敏记录](evidence/p2-multifile-ownership-mac-20261004/redaction.json)保留归档SHA，
[原始hash](evidence/p2-multifile-ownership-mac-20261004/original-sha256.json)与
[发布hash](evidence/p2-multifile-ownership-mac-20261004/raw-sha256.json)逐项对应。
106份JSON数值/布尔字段脱敏前后完整相等，438个发布成员逐项摘要核验通过。
448个发布text payload的私密路径/常见凭据模式扫描通过，该有界扫描不是通用秘密检测。
`python3 scripts/check_docs.py`通过497页结构检查，`git diff --check`通过。
本批只提交证据与文档，不重跑未变Rust门禁；本轮未推送、创建PR或执行新远端CI。

## 用户负载说明后的同条件重测

用户随后说明“现在没有跑其他了，继续”，授权同一配对按冻结方法重测；这是用户对负载
背景的陈述，不代表采样器证明系统完全空闲。原三次尝试与全部59条样本保持不变。
启动时累计6.185GiB、剩余约63.31GiB；不清理旧target，不复用非空target冒充冷构建。
新临时目录仅为同一固定方法所需空缓存，串行执行，采样顺序、首启/热执行划分、
样本数、阈值、编译参数与前次第三轮完全相同；只补负载metadata及累计缓存清单。

实际25.22秒，完成13条**before校准**后按原噪声线主动停止；没有after、正式配对或
身份对照样本，也没有继续下一迁移。本次所有实际执行均72 passed/0 failed/0 ignored，
不将校准通过的功能结果当作成本接受。增量校准median0.6602s、MAD0.0403s、
range0.0850s，均在原线内；热运行median0.1269s、MAD0.0031s，但range0.0536s
超过`max(40ms,30% median)`=0.0400s，因此停止，未放宽阈值或自动再次重试。

| 热运行校准 | 采样器墙时(s) | Darwin time real(s，原始精度) | harness finished(s) |
|---|---:|---:|---:|
| 0 | 0.1238 | 0.09 | 0.10 |
| 1 | 0.1307 | 0.10 | 0.10 |
| 2 | 0.1253 | 0.10 | 0.10 |
| 3 | 0.1269 | 0.11 | 0.11 |
| 4 | 0.1774 | 0.16 | 0.16 |

最后一次额外耗时同时出现在子进程time与harness中，不能仅归因于采样器等待检测。
该次time还记录0.12s user/0.02s sys，前几次约0.08–0.09s user/0.01s sys；
这些只提供下一步诊断线索，不证明负载来源或代码退化，也不证明观察器没有开销。
建议下次实验先单独评估固定before可执行文件的计时稳定性，并比较采样器墙时、
Darwin time及harness三个时钟，记录环境/负载背景；必要时把逐用例耗时定位作为独立
诊断安排。若考虑更长cohort、样本数或计时口径，应先明确方法变更，不能倒改本次线。
在稳定校准或明确新的测量合同前暂停扩展配对，保持成本预算未接受。

本次新增约0.328GiB，结束累计6.513GiB、剩余约62.91GiB，仍未触两小时/累计12GiB/
剩余40GiB硬线。未安装工具、改安全设置、推送、新建PR或重跑整个功能回归。
[本次方法](evidence/p2-multifile-ownership-retest-mac-20261004/protocol.json)、
[采样脚本](evidence/p2-multifile-ownership-retest-mac-20261004/probe.py)、
[校准](evidence/p2-multifile-ownership-retest-mac-20261004/calibration.json)、
[13条样本](evidence/p2-multifile-ownership-retest-mac-20261004/samples.json)、
[停止原因](evidence/p2-multifile-ownership-retest-mac-20261004/outcome.json)、
[三时钟与统计](evidence/p2-multifile-ownership-retest-mac-20261004/summary.json)和
[全部70个raw成员](evidence/p2-multifile-ownership-retest-mac-20261004/raw.tar.gz)独立保存。
17份JSON数值/布尔字段脱敏前后相等，70个归档成员摘要全部通过；
[脱敏与归档摘要](evidence/p2-multifile-ownership-retest-mac-20261004/redaction.json)、
[原始摘要](evidence/p2-multifile-ownership-retest-mac-20261004/original-sha256.json)和
[发布摘要](evidence/p2-multifile-ownership-retest-mac-20261004/raw-sha256.json)保留，不覆盖前批。
79个发布text payload的私密路径/常见凭据模式扫描通过；文档结构497页与whitespace
检查通过。没有新增产品行为、Spec或Rust实现修改。
