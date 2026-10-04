# 当前文档与旧 Spec 状态同步

> **性质**：固定基线文档核对记录 · **状态**：current · **读取时机**：核验本轮文档同步或续接并行开发时 · **唯一真源**：本页固定核对输入；规范与各交付分别以Guide和Spec为准

## 基线与授权

2026-10-04，用户要求在独立worktree同步roadmap、Guide与旧Spec状态。
基线为创建时已提交main `adb51d67fd989bc4116ef4b17704f85c275dd3f4`，
本地文档分支`docs/status-sync-20261004`。没有接收主工作区未提交修改，
也不接收另一会话的0268开发分支；本页的支持范围只反映上述固定main。
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
