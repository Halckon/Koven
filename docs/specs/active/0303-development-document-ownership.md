# SPEC-0303: development 文档职责收敛与候选保全

> **性质**：有界文档治理合同 · **状态**：in-progress · **读取时机**：迁移开发资料、核对开放候选与历史保全时 · **唯一真源**：本页的归整范围和验收

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-ENG-303` |
| 所属 Phase | 工程文档治理；不改变语言 Phase |
| 语言规范 | [Guide v0.43](../../guide/README.md)；不变更语义 |
| 批准依据 | 2026-10-11 用户同意将 development 不必要文档收敛、迁移或删除；仅本地归整，扩大后的远端发布另行确认 |
| 后续批准 | 2026-10-11用户确认“做完后文档整理合并后，继续按这个推进”（Sentinel_4cfee78805ac8191954d9a209e29e7c3）；此前仅本地授权为原检查点，恢复阶段暂不执行GitHub写入 |
| 本次交付批准 | 2026-10-11 15:49 用户确认整理完成后合并并继续既定功能顺序（Sentinel_4cfee78805ac8191954d9a209e29e7c3）；2026-10-12接入actual main，授权状态追加于此，旧本地检查点保留；远端操作由交付分工执行 |
| 前置 Spec | 无；集成基线含0302本地证据保留变更，其交付独立维护 |
| 前置 ADR | 无 |
| 影响范围 | development、proposal、archive、Spec 导航与冻结 inventory/DAG |
| 语言语义变更 | 否 |

## 1. Goal 与边界

`development/` 只回答如何开发、验证和交付；候选设计在 `proposals/`，一次实施在正式
Spec，已发生的验收和固定日期计划在 archive。移走辅助账本不代表功能、预算或安全验证完成。
本次依据是 [文档职责与归档规则](../../AGENTS.md)，不是认定既有功能违反 Spec-first。

集成基线为0302本地冻结 `0534b9c4d1aec5f89a6bd6f8bb430b96df0211d6`，其父主干为
`8c6907c238cc0e040bed4541203e450a5f53552b`。0297–0300及审计摘要取自未发布精简链
`d9e2d8094e1429392af3f181cc1d38ddf443b045`；原本地链保留，不引入其完整日志或 JSON。
0302实际主干确定后只同步一次，并重验引用/inventory/DAG；不代0302归档或更改其验收。

## 2. 页级迁移映射

下表文件名的旧目录均为 `docs/development/`；“历史同名”指
`docs/archive/development/<旧文件名>`，正文、标题、数字、未运行和失败记录原样保留，
仅机械重定向链接。线程候选的类别标签及三处启动/验证导航另纠正为live规则，
H1–H6/H-A–H-H正文不变。保留工程页与新候选不维护另一张实施状态表。

| 旧页 | 唯一新归属 / 活需求承接 |
|---|---|
| `README.md` | 保留；简短工程导航，不列逐批进度 |
| `workflow-and-phases.md` | 保留；Guide→Spec→红测→实现→验收/交付方法 |
| `rust-and-modules.md` | 保留；Rust 模块规则 |
| `diagnostics.md` | 保留；诊断规则 |
| `dependencies.md` | 保留；依赖治理 |
| `testing.md` | 保留；测试方法与0302证据保留规则 |
| `rust-size-policy.md` | 保留；尺寸方法及现存政策入口 |
| `preview-candidate.md` | 保留；已支持 preview 工具使用方法 |
| `roadmap.md` | 历史同名；当前导航合并 README 与 Spec 索引 |
| `multifile-program-spec-draft.md` | 历史同名；0263–0265/0268与教程承接，普通字段赋值事实见 `architecture/lowering-support.md#现存入口差异`，未编号候选见 `specs/drafts/v0.43/README.md` |
| `text-processing-spec-draft.md` | 历史同名；0274/0295承接 argv，文本 IO 剩余候选归 `proposals/text-input-and-io.md` |
| `borrow-access-spec-draft.md` | 历史同名；0287/0288为已选合同，剩余来源/投影/逃逸候选复用 `general-borrow-access-results.md` 与 `borrow-access-contract-comparison.md` |
| `sequential-collections-spec-draft.md` | 历史同名；0275/0276/0279–0285为已选合同，容量/API候选归 `proposals/collection-api-followups.md`，算法复用 collection r3 |
| `map-collections-spec-draft.md` | 历史同名；0287/0288/0295为已选合同，Map迭代/API候选归 `proposals/collection-api-followups.md`，实际return/borrow/loop缺口由0297/0299/0300承接 |
| `memory-safety-validation-spec-draft.md` | 历史同名；0266/0269仅关闭所选范围，检测扩展/生成/外审归 `proposals/native-safety-validation.md` |
| `measurement-distribution-spec-draft.md` | 历史同名；0272仅关闭所选preview范围，测量/发布候选归 `proposals/measurement-distribution.md` |
| `thread-transfer-spec-draft.md` | 正文迁 `proposals/thread-transfer.md`；H1–H6及H-A–H-H保持未启用候选，不新占Spec编号 |
| `post-governance-milestones.md` | 历史同名；M0开放成本/整合决策归测量候选，M1–M6分别由本表候选/正式Spec承接，不保留第二份进度表 |
| `engineering-governance-plan.md` | 历史同名；批准范围与原决定保全，未接受P2预算唯一活决定归测量候选 |
| `engineering-governance-progress.md` | 历史同名；固定批次交付保全，未接受P2预算不因归档关闭 |
| `codegen-plan-test-migration.md` | 历史同名 |
| `codegen-receiver-test-migration.md` | 历史同名 |
| `const-owned-unit-handoff-measurement.md` | 历史同名 |
| `documentation-status-sync.md` | 历史同名 |
| `drop-iteration-test-migration.md` | 历史同名 |
| `governance-local-continuation.md` | 历史同名；开放成本决定归测量候选，不以迁移结项 |
| `lsp-test-migration.md` | 历史同名 |
| `multifile-ownership-test-migration.md` | 历史同名 |
| `multifile-type-test-migration.md` | 历史同名 |
| `n1a-contract-integration.md` | 历史同名；正式状态只由Spec索引维护 |
| `owned-unit-handoff-measurement.md` | 历史同名 |
| `ownership-iteration-test-migration.md` | 历史同名 |
| `ownership-planning-milestone.md` | 历史同名；P2限制及入口差异不默认为解决 |
| `p2-frontend-cost-followup.md` | 历史同名；Mac噪声/停止边界转为测量候选决策输入 |
| `p2-linux-cost-transfer.md` | 历史同名；十组/12噪声/首启单对调查保全，预算继续未接受 |
| `recovery-local-delivery.md` | 历史同名 |
| `unit-handoff-contract-baseline.md` | 历史同名 |
| `unit-runtime-layout-migration.md` | 历史同名 |
| `pr72-74-followup-audit.md`（未发布精简链） | `archive/migrations/pr72-74-followup-audit.md`；日期/矩阵为历史，四draft保持Spec归属，nullable factory候选归 `specs/drafts/v0.43/README.md#未编号的现行支持候选` |

共38份基线顶层页：8保留、29历史归档、1迁proposal；另接入1份未发布审计历史。
原 evidence 短索引沿用0302，既有fixture和消费者不改。

## 3. 规则与开放项保全

- M1B-b保留输入源、UTF-8/分词、限制、IO错误与关闭、Result/`?`分开核验及公共`.ko`边界。
- M2B现行Guide已启用部分与剩余候选分开；已有proposal为唯一设计owner，不复制来源核心。
- 容量、未选Map查询/迭代/顺序及开放协议不因0295应用通过而启用；算法候选也不隐含承接Map迭代。
- M4检测/生成扩展及外审、M5未选发布/兼容性/成本、M6线程合同保留，外发/实验预算另需授权。
- P2预算明确延期/未接受；旧Mac超噪声、旧云raw缺失、Linux噪声与调查项不得改写为通过。
- 通用工作方式复用现存工程规则；旧批次授权及约束只保留历史，不转写为永久新增门禁。
- 0297–0300原有界Goal、非目标及Guide前置保持；本片不替其激活或实施。普通nullable工厂边界仍为未编号候选。

## 4. 非目标与提交边界

不改生产Rust、Guide语义、测试行为、现有历史结论；不运行Cargo、CLI、故障注入、校准或
聚合驱动；不认证旧测量，不自动接受预算，不删除独立信息，不改写本地原链或远端历史。
纯粹重复的导航/状态条目可撤下；历史正文保留。本次无需放宽checker路径规则。

1. 本合同、页级mapping、active索引/inventory/DAG先提交。
2. 机械迁移历史及所有引用，补最小live候选并收敛工程导航。
3. 接入0297–0300及短审计/标签纠正，静态验证、独立审查后记录实际结果。

