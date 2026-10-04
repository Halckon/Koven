# 当前文档与旧 Spec 状态同步

> **性质**：固定基线文档核对记录 · **状态**：current · **读取时机**：核验本轮文档同步或续接并行开发时 · **唯一真源**：本页固定核对输入；规范与各交付分别以Guide和Spec为准

## 基线与授权

2026-10-04，用户要求在独立worktree同步roadmap、Guide与旧Spec状态。
基线为创建时已提交main `adb51d67fd989bc4116ef4b17704f85c275dd3f4`，
本地文档分支`docs/status-sync-20261004`。没有接收主工作区未提交修改，
也不接收另一会话的0268开发分支；本页的支持范围只反映上述固定main。
以上及下文原核对/验证表是创建分支时快照；随后用户授权合入main，接收已合并0268的
最新状态见[合并补充](#合入main时接收后继0268)，不改写旧验证输入或数值。
此次属于Phase 6工程文档维护，无语言/编译器行为变化，无新Spec或生命周期迁移。
只本地提交，不推送、创建PR、合并主干或争用Cargo target。

权威路径为`docs/guide/README.md`，唯一current版本仍是v0.40。
Guide的语言承诺、强制Phase与示例不随实现状态降低；本批只增加实现状态导航，
并澄清“规范启用不构成实现验收证明”的原意。Guide7/11的lambda/Box合同不变，
Str/toString仍延期，clone-first不重开；没有启用新版本或候选语义。

## 已提交事实与精确CI

本机索引按四位编号的Spec正文计数：0 active、254 archive。先前只统计所有Markdown
得到255，是误把非Spec生成文档算入；不据此修改冻结inventory或制造归档迁移。
只读GitHub核对的[元数据](evidence/documentation-status-sync-20261004/ci.json)
保留head、jobs、关键步骤及skip，下面均是历史对应head，不是本次文档分支CI。

| 范围 | 固定交付/验收依据 | 当前边界 |
|---|---|---|
| 0182、0255–0261恢复 | PR37 main `385bb23e`与恢复编译器同tree；已有双宿主CI37185473350及本机恢复账本 | 归档原有界合同，不推导所有语言变体完成 |
| 0262教程 | PR38 head `095cf637`，CI37188477831双宿主成功，merge `b32602ec` | 11正例、2完整JSON负例实际执行；1 planned不执行 |
| 0263/0264 | 已合并PR40/41，各Spec与当前Architecture记录字段可变性/直接字段Borrow | M1A A2/A3完成；不扩大投影alias、字段swap或general Inout ABI |
| 0265 | PR43 head `ab1fff85`，merge `6377f2b4`；[CI37200366612](https://github.com/Halckon/Koven/actions/runs/37200366612)10 success/1 editor合法skip，双宿主组合步骤success | A4前端typed/ownership/清理事实完成；A5 unit SSA/native和M1A总验收仍开放 |
| 0266 | PR44 head `d376c642`，merge `44cb312f`；[CI37203501976](https://github.com/Halckon/Koven/actions/runs/37203501976)11 success，Ubuntu动态sanitizer步骤success | Linux ASan/LSan有界交付；macOS对应动态步骤skipped，不宣称UBSan或完整安全证明 |
| 0267 | PR45 head `304fe257`，merge `5fbc6641`；[CI37199186829](https://github.com/Halckon/Koven/actions/runs/37199186829)11 success，实际CLI corpus步骤success | 旧五失败已修复、corpus10/10及树合同9/9；不代表全部语言/真实编辑器增量解析 |

0265/0266归档正文最后“归档提交仍须最终PR CI”保留写入时事实；本表补对应最终head
及已合并状态，不回写或抹除历史失败。PR push的合法Rust skip不当作PR双宿主执行，
同tree不等于提交SHA相同。本轮没有再次触发或重跑这些CI。

## 旧Spec核对与处理

| 旧记录 | 需要同步之处 | 本轮处理 |
|---|---|---|
| 0182及v0.37阶段索引 | live索引仍写active/approved、验收补强中 | 改为原有界合同done/归档；unit native后继单列 |
| 0254/0255及恢复Spec0256–0261 | archive内待发布/未跑Linux是当时快照 | 状态已done，正文/证据完整保留；当前入口指向后继PR37交付依据 |
| 0262 | archive中Mac完成/Linux未跑的批次快照 | 不改历史；live索引同步PR38精确head双宿主结果 |
| 0242编辑器历史及0238 Guide缺口 | editor五失败、multifile五失败不能继续列当前待修 | 演进/Guide覆盖live摘要分别引用0267与0247后继事实；旧红测原文保留 |
| 0263–0267 | 当前main均已归档；部分末段仍有归档提交待CI快照 | 不重开、不迁移、不修改inventory；补本页最终PR证据与能力边界 |

已经完成的Spec不因为尚有其他语言变体而重开；候选里程碑也不自动成为active合同。
Archive、原批准治理计划、ADR和历史日志不改写。当前roadmap/演进/Spec索引与治理摘要
分别负责导航、能力、生命周期和交付状态，不将后继草案当作规范或完成事实。

## 延期、分歧与验证范围

用户决定暂缓P2噪声/成本检查；原数据和未接受预算保留，不能写作性能通过。
用户说明先前仓库审计由其他工作完成；本次未获得完整审计交付正文，不能独立认证全部
整改结论，也不重启审计。后继M1A由另一会话推进，未合入本基线的工作不倒填。

字段swap/projection alias、nested/index/general Inout ABI、条件资源运输、泛型资源native
仍按Architecture保留边界。Box投影/unbox、lambda separator的规范承诺不变，尚缺对应
最新端到端交付证明；这些是实现验收缺口，不通过改Guide消除。
Escapable、mutable/once closure、controlled unsafe以及M2/M3新增API如需新规则，
仍需独立合同和必要Guide启用；本轮没有发现需要立即裁决并修改规范的冲突。

本批只验证文档结构/链接、教程源码与清单身份一致性、Guide示例与规范领域页保全、
Spec inventory及引用CI的固定head关系；不运行教程CLI、Rust、性能或sanitizer。
旧成功证据只归属其原head，验证结果另记本批交付。

## 本批本地验证与交付

状态同步先提交为`1c30f0e`；随后独立核验并保留
[一致性记录](evidence/documentation-status-sync-20261004/verification.json)。
`python3 scripts/check_docs.py`通过511份Markdown结构/链接/inventory检查；
`git diff --check`与staged whitespace检查通过。
调用现有教程`load_examples`静态核验14份合同/15个唯一源码fence：11 executable、
2 diagnostic、1 planned；没有运行CLI或把静态提取称作link/run。

16份Guide页面的全部fenced示例与基线相同；除入口及Guide15导航澄清外，14份领域页
逐字节相同并保留SHA-256。Archive全路径diff为空，Rust/测试脚本/CI/Cargo/Spec
inventory未修改；3个引用的最终PR head均是固定main祖先，成功/skip口径单列。
没有重跑文档检查器单测（检查器未改）、完整编译器回归、教程native、sanitizer或成本实验，
也没有新远端CI。本地分支待用户后续整合，不自动更新main、推送或发PR。

## 根目录README追加同步

用户随后要求更新根README；本分支同时同步英文`README.md`与中文`README_CN.md`。
保留项目介绍、架构、CLI、构建和许可证定位；把绝对安全/零成本措辞改为有界设计与实现说明，
纠正纯内存ASAP和资源词法清理的区别，明确Transferable能力不等于线程API已交付。
原未受教程合同保护的String.length/整数拼接、general Inout赋值等源码不再复制到README，
入门示例改为链接当前Markdown单一真源；规范语义与native支持继续分别引用Guide/Architecture。
新增教程/roadmap/Spec导航，支持范围仍固定于本批main，不接收并行0268。

构建命令补`--locked`，Rust说明对齐固定1.96.0与manifest MSRV；补本地构建产物PATH，
并区分匹配Clang21的IR测试与系统链接driver。没有执行安装、构建或用户shell命令。
静态检查78个本地链接/锚点、教程ID、中英文bash块相等、Cargo/bin/工具链配置、
CLI的source/project/entry/format/global options及prelude hello入口/输出全部通过。
`python3 scripts/check_docs.py`仍通过511页，`git diff --check`通过；不重跑编译器回归。

## 合入main时接收后继0268

用户随后授权仅合入本地main，不推送。合入前main为
`a83749f8bd1567cdd40a2d92e2b7ee22947773f1`，文档head为
`291f24b87c60786a19758971ee32207a207dfb26`，共同基线仍为`adb51d6`。
所有worktree均未发现Git merge/rebase/cherry-pick/index-lock操作；未提交文件与本轮
文档路径不重叠。采用普通双父merge，保留三次文档提交及新main历史，不stash/reset。
没有文本冲突；自动合并后的旧状态按最新main作语义整合，避免回退里程碑交付。

PR46归档head `fb4b849529ebdcec4e7063723a3a85a5c81e7fbb`已合并为`7c093bb6`；
[最终PR CI37206347562](https://github.com/Halckon/Koven/actions/runs/37206347562)
10项success、1 editor按路径合法skip，双宿主组合测试和Ubuntu动态sanitizer步骤实际success，
macOS的Linux专属步骤skipped。只读[CI元数据](evidence/documentation-status-sync-20261004/merge-ci.json)
保留精确head和步骤，不将本轮文档合并称为新CI通过。

当前0 active/255 archive；0268补齐M1A A5–A10、unit迭代native及完整参数报告程序的
有界双宿主验收。Inout/field/captured Borrow source native仍拒绝，后续0269等未合入
工作不计完成。roadmap、根README、Spec索引、v0.37及演进/治理摘要同步这些状态；
Archive、另一会话的实现/脚本/教程合同和已有验证JSON保留原值。

合并后教程为12正例、2诊断负例、1 planned；parameter-report的4组argv使成功执行合同
为15组，加2诊断共17组。单一源码与清单静态核验，不重新执行CLI或编译器。
P2噪声/成本验收继续延期，原数据保留，不扩展性能或安全检测范围。

本次合并验证：`python3 scripts/check_docs.py`通过513页结构/链接/inventory，
whitespace检查通过；教程15份定义/18个源码fence/17组执行合同静态一致，README
80个本地链接/锚点及中英文bash块一致。Guide全部示例相对合入前main不变，
源码、测试脚本、Cargo/CI与Archive全路径diff为空。合入前记录的6个未提交文件逐一
SHA-256及集合相等，全部保全。未运行Rust回归、CLI、性能或新远端CI。
