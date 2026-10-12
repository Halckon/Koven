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

## 5. 验收账本

以下为2026-10-11原本地e21231198的历史验收；环境重置后不作为恢复树的新验证。

| 验收项 | 结果 |
|---|---|
| 编号唯一、mapping覆盖38份基线页与额外审计 | 编号未复用；38份基线页及额外审计逐项对应，无遗漏/重复 |
| 历史正文和审计保全 | 29份正文除机械链接/裸路径外相等；87行审计相对原lean仅链接和frozen元标签变化，数字/失败/未测项不变 |
| 线程候选原合同保全、每项开放需求有live owner | H1–H6/H-A–H-H正文不变，仅2类别标签与3处live导航纠正；5个候选页和既有owner覆盖全部开放范围，P2预算未接受 |
| development 8页、全部内链/索引/可达性 | 顶层38页/5201行→8页/443行；29历史迁archive、1线程迁proposal；全部本地链接/索引/可达性通过 |
| 四draft/inventory/DAG、0302/0296既有状态保全 | 四draft相对d9e2d809仅链接变化，均未激活；DAG重新生成并核新鲜度；0302正文/规则/fixture/evidence索引与Rust/workflow未变，0296仍archive |
| `python3 scripts/check_docs.py` | 649 Markdown通过；结构检查不证明语义等价 |
| `python3 -m unittest scripts.tests.test_check_docs scripts.tests.test_gen_spec_dag -q` | 42项通过（41 docs + 1 DAG）；未放宽checker路径规则，仅维护inventory |
| `git diff --check`及增量范围/有限凭据模式核对 | 通过；无新增JSON/log/ko或旧evidence回流；有限扫描无命中，不是完整秘密审计 |
| 独立审查 | 非作者按0534b9c4→dd4582f79逐项核对历史、开放候选、四draft及静态门；发现的live历史路由和README标签已纠正，无阻塞 |
| Cargo、动态、注入、聚合与远端发布 | 未运行；不在本片授权范围 |

2026-10-11以上仅为本地静态验收。0302实际main尚待接收，接收后一次同步并重验；
本地归整验收不等于远端交付，在真实PR/CI闭环前保持in-progress。

## 6. 2026-10-12恢复边界

原未发布HEAD `e21231198be7e95d6aee1a0692687502cb1c2a1e` / tree
`7d145901751d7d6d8b375ce1f1f0bfb2d94e9fbe`的Git对象及旧工作树丢失。
本轮基于0302恢复提交`1633cf24f1570c0da3e22fef84ff72d49a3e9060`，其tree为
`51d23ebdd8d618784eaa369625b643886a003ea2`，由0302独立恢复记录确认内容等价；不恢复原提交身份。

保留工具正文支持归整合同、五个候选、历史审计及机械链接重放；0297草案SHA256精确命中
`980c5e8b3eae3593e5af9b92bf74b6cc612576a381aa0c00ac48b1fab47a7b0b`。
0298–0300原稿无法完整恢复，按原核实Goal/非目标与现行Guide重新起草，明确不是逐字恢复，
保持draft，未扩实施授权。旧原链/矩阵raw现在不可取，不将已知hash称为文件已恢复。
恢复材料和缺失报告另存仓库外；本树不新增完整日志/JSON或重复测试源码。

| 恢复后检查 | 实际结果 |
|---|---|
| 历史29页与线程合同保全、0297精确hash | 相对1633cf24，29/29页在机械链接/裸路径正规化后正文相等；线程H1–H6/H-A–H-H保全；0297实际SHA256命中上述值 |
| 三份重起草范围及开放候选独立审查 | 非作者核对7b217ad4及后续三页修正，无剩余阻塞；0298清理次序、0299 Copyable桥、0300仅非Resource guarded owner均已明确；5候选/P2开放边界保全 |
| docs、42项静态测试、diff及新tree | 修正后重新运行check_docs：649 Markdown；docs/DAG单测42项通过；diff检查通过，审查者独立重跑同门通过；新tree由本轮Git提交记录，不冒称旧tree恢复 |
| 基线内容及增量边界 | 生产Rust、workflow、0302规则/正文/fixture/evidence索引未改；checker仅维护inventory；无新增完整日志/JSON/重复源码 |
| Cargo/native、注入/聚合、GitHub | 未运行；本恢复片仅文档与静态验证 |

等待0302实际main后一次同步，再记录真实交付。三份重起草及恢复记录使新tree与原tree不同；
原tree缺失，无法逐文件认证全树等价。以上仅报告实际比对的内容，不声称旧e21231198已还原。

## 7. 2026-10-12 actual main 一次集成

已fetch并接入PR76 actual main `8ef058f6f7a41e1b43f4ff74826ac16e47866299`，tree
`57126cd1c1a94a617fd0410c718d85af84869410`；保留0302归档、规则、短索引、fixture及引用格式修正。
接收时[main CI 38191208630 attempt 1](https://github.com/Halckon/Koven/actions/runs/38191208630)
仍运行，按pending记录；其终态由PR交付记录跟踪，不为状态轮询反复修改仓库。
0297–0300保持draft，沿用本轮重起草边界；此集成不实施或代为激活任何功能。

| 集成后检查 | 结果 |
|---|---|
| 0302基线、历史正文及四draft保全 | 代码/workflow/fixture/0302规则与归档/短索引逐字保持actual main；29迁移页及454份既有archive正文按机械路径核对通过；线程14条保全；四draft、候选及审计与d4228f53逐字相同 |
| 文档、静态测试与diff | 新跑649 Markdown、42项docs/DAG静态测试、diff检查通过；有限凭据模式扫描无命中，不等于完整秘密审计 |
| 独立审查 | 非作者核对actual main集成差异、历史保全、四draft与开放候选，独立重跑649/42静态门及diff检查，无剩余阻塞；不代表远端CI或交付完成 |
| Cargo/native、注入/聚合、远端写入 | 未运行 |
