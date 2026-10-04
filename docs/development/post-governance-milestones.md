# 治理完成后的开发里程碑草案

> **性质**：后继开发计划草案 · **状态**：draft / M1A 首片已启动 · **读取时机**：评审后继开发顺序与实施边界时 · **唯一真源**：本页维护候选里程碑；现行语义与实际进度分别见 Guide 和各正式 Spec

## 1. 起草依据与授权边界

2026-10-04，用户要求综合《仓库审计.md》《ML总览.md》与代码事实起草文档，
实际代码开发等待另一位 agent 完成[整体架构与工程治理实施计划](engineering-governance-plan.md)。
两份附件作为评审输入，其中的任务、规模和实施建议不自动成为授权或语言规范。

起草基线为 PR #37 合并提交 `385bb23e1123c3a4ba00ec9fe5d964ebc95f1493`。
[该主干 CI](https://github.com/Halckon/Koven/actions/runs/37185473350)已成功；它只证明实际选择的
双宿主检查，不代表本计划、后续本地提交或全部语言能力已经验收。

本轮交付为本页与 §4 链接的八份待编号 Spec 起草材料，覆盖 M1A–M6。
用户允许后续实际实施时根据代码与需求调整，调整规则见 §10；当前草稿不是已冻结合同。
正式 Spec 编号在治理交接后重新核对全仓及并行分支再分配。
续写时已观察到 SPEC-0262 的 PR #38 合并进入 origin/main；这不自动解除 G0，
本轮也未将该提交吸收到起草分支，代码事实仍以以上起草基线为准。
起草不创建 active Spec，不调整原治理退出条件，不启动编译器、标准库、CI 或 benchmark 实施。

随后用户要求先提交并合并草稿，再开始里程碑。草稿已通过 PR39 合入 `b92593d`；
本次授权更新与基线接收见 §12，取代起草时的代码等待状态，不启用后续候选语言语义。

## 2. 代码开发启动门槛

| 门槛 | 需要的证据 | 当前处理 |
|---|---|---|
| G0 治理交接/推进决定 | 责任 agent 的记录可对照原退出条件；剩余事项有原计划允许的处理或用户明确决定 | 用户此次明确进入里程碑；P2 成本等仍开放，不能称治理全部完成 |
| G1 接收确定基线 | 记录交付 SHA、分支/PR、文档状态、实际验证与保留项，确认已进入 main | 已接收 PR38 + PR39 的 `b92593d`；未接收其他工作区未提交改动 |
| G2 重新核对缺口 | 在实施基线上重查所选里程碑的入口、事实、测试与最小复现 | A2 已完成，A3 拒绝点与 unit for 事实缺口已核对；原完整程序仍待通过 |
| G3 形成可实施合同 | 分配未占用 Spec ID，确认批准依据；语义/长期架构变化完成 Guide/ADR 前置 | 0263/0264已交付 A2/A3；其它草稿仍待交付 |

G0 的判断依据为[治理执行账本](engineering-governance-progress.md)及责任 agent 的交付记录。
本页不另建第二张 P0–P5 完成表，不以 M0 定义替代原计划收尾。
未接受的成本预算保持开放；本次按用户推进决定进入功能工作，不自动扩展成本实验。
当前没有可读取该 agent chat 的工具，交接依据仓库记录和 PR，不声明已自动监控。

## 3. 起草基线的事实修正

| 附件中的前提 | 核验结论及对计划的影响 |
|---|---|
| 五项 multifile 类型失败仍未关闭 | 已由既有修复闭合；PR37 双平台 `multifile_type_checking` 各 107 passed / 0 failed / 0 ignored，交接时复核是否有新回归 |
| P3b 必须统一成一个 facade 才能结束 | 既有有界宿主编排合同已有退出证据；完整 single/unit 合并不属于该合同的隐含要求 |
| 模块、泛型、闭包、Rc 和容器需要从头实现 | 已有有界实现；按具体实例核对 native 支持，复用既有能力 |
| 关闭 SPEC-0182 才能支持 for | 0182 已按原有界范围归档；unit for 使用后继合同，不能用旧归档推定其已支持 |
| unit for 只缺 lowering | 类型 binding 仍为 `Deferred(LoopSource)`，ownership 未提供等价迭代计划，lowering 明确拒绝；需贯通 Phase 2–4 |
| 所有 Borrow 交付都需要重做 | root/temporary Borrow、forwarding 与部分字段 clone 已有能力；M1A 聚焦普通 class 字段直接 Borrow |
| 全部旧测试失败均已清零 | Rust editor fixture 检查不等于 Tree-sitter 完整 corpus；历史 corpus 失败在本次起草中未重跑 |
| 没有安全测试 | 已有资源计数、拒绝、变异和压力测试；尚需验证 sanitizer 与随机执行的覆盖，不据此宣称完整安全证明 |

直接入口：[跨文件字段可变性查询](../../crates/lang-frontend/src/ownership_checking/compilation_unit/dataflow/places.rs)、
[unit for 类型检查](../../crates/lang-frontend/src/type_checking/compilation_unit/bodies/checker/control.rs)、
[unit lowering](../../crates/lang-codegen/src/ssa/unit_lower.rs)。
阶段边界见[流水线](../architecture/pipeline-and-workspace.md)、[字段交付](../architecture/direct-field-replace.md)
与[顺序迭代](../architecture/finite-sequential-iteration.md)。

## 4. 里程碑与依赖

规模 S/M/L 仅表示当前预估的相对复杂度；没有工期承诺，设计选定后再估实施成本。

| 里程碑 | 目标 | 前置 | 规模 | 退出条件 |
|---|---|---|---|---|
| M0 交接与基线收口 | 接收治理结果，固定缺口、入口和成本证据边界 | G0、G1 | S–M | 交接可追溯，必要阻塞项处理完成，保留项明确；不要求删除全部兼容入口 |
| [M1A 多文件程序贯通](multifile-program-spec-draft.md) | 三文件参数报告程序，贯通跨文件字段、直接 Borrow、unit for | M0、G2、G3 | L | 两宿主 project build/run、输出、清理、拒绝与原子性通过 |
| [M1B 文本处理程序](text-processing-spec-draft.md) | argv 词频版，再到选定输入方式的文本版 | M1A、所需 M3A API、必要语义决策 | L | 固定输入与错误集真实执行，文本/错误/API 边界明确 |
| [M2 借用访问方案决策](borrow-access-spec-draft.md) | 根据实际需求选择现行索引/Borrow、受限返回或投影方案 | M1A 暴露的需求 | 设计 M；实现另估 | 接受/拒绝 litmus、方案及非目标明确；新增语义获启用后才实施 |
| [M3A 顺序集合实用化](sequential-collections-spec-draft.md) | 补选定 Array/List/MutableList 操作 | M0、具体程序需求 | M–L | 所选访问/增长/搬迁/删除/清理合同完整，新增 API 语义前置满足 |
| [M3B Map 合同与实现](map-collections-spec-draft.md) | 键、查询、更新、扩容与迭代的完整有界合同 | M1B 需求、语义批准；按查询方案依赖 M2 | L | 正反例与 native 通过，词频程序可用 Map 改写 |
| [M4 安全验证](memory-safety-validation-spec-draft.md) | 检测接线、可复现随机执行、定向外审 | M1A 起逐步接入；外审另有范围与授权 | M–L | 检测覆盖实际生效，失败可定位、回归，审计意见有处置 |
| [M5 度量与分发](measurement-distribution-spec-draft.md) | 新成本基线、双宿主 preview 与使用材料 | preview 依赖 M1A 和选定 M4 门槛；实用版本依赖 M1B | M | 新环境安装运行通过，原始测量与限制公开 |
| [M6 线程与共享演进](thread-transfer-spec-draft.md) | 先 v1 所有权转移线程，另评估跨线程共享 | 稳定 runtime、M4 证据与真实需求 | L | 转移/join/错误/清理合同闭合；共享与新目标分别决策 |

推进主线为 `治理交接 → M0 → M1A → M1B → 实用版本`。
M3A 所需 API 可以先于 M1B 交付，避免“M3 必须等整个 M1”的循环依赖。
M2 不阻塞 M1A，也不当然阻塞所有集合操作；M4 随功能推进，M5 preview 可在 M1A 后准备。
并行描述的是工作依赖，不授权争用同一个 Cargo target，也不要求并行启动 agent。

## 5. M0：接收治理结果

- 核对 P3b 等既有有界合同的交付状态，不以一个 facade 或一个 resolver 作为新的完成定义。
- 对旧 native/lower/planner 入口记录调用方、能力和迁移条件，分别保留必要转接或删除无用入口。
- 两个 concrete-type resolver 先列差异，随功能共享已证明等价的规则；长期方案变更另走 ADR。
- 核对 README 示例与现行术语；可执行内容从单一来源验收，关键词扫描不能替代语义测试。
- 复核 editor corpus 与当前 CI 选择的盲区；不能把历史失败次数直接当成当前结果。
- 成本原值缺失、部分新试点、噪声与接受预算分别接收；不把调查阈值或文件缩小当作提速证据。

起草基线脚本静态选择 131 个 frontend integration 源目标中的 77 个；这不是运行结果，
交接时重新计算。按变更补直接消费者；frontend 全量的预算和触发方式另作明确决策。
M0 完成不意味着剩余历史尺寸欠账、所有语言能力或 P2 原始证据自动闭合。

## 6. M1A 与 M1B：从固定程序到实用程序

M1A 的源码、输出、拒绝路径、阶段边界和验收映射只维护在
[Spec 起草材料](multifile-program-spec-draft.md)，本页不复制另一份夹具或执行账本。
实现顺序为跨文件可变性、直接字段 Borrow、unit 迭代事实/清理、SSA/native、CLI project。
程序通过现行字段 replace 更新值，普通字段赋值 lowering 留作独立后继；迭代 Inout/字段
source 的前端语义覆盖与首轮 native 拒绝分别验收，遵守 Guide §37.4。
入口共享围绕这些功能的实际重复规则推进；不能以全面合并 single/unit 为无限前置任务。

M1B 分两步：先将 argv 的每个参数作为一个词，再增加一种明确的文本输入方式。
选 stdin 或文件输入前，封闭错误模型、UTF-8/分词、数字文本输出与内存上限；
明确公共 Koven 源码与受控 runtime 桥接的职责。现有底层能力不全时，不能默认 `.ko` 已能包装 libc。

现行 String 的操作集合不能自动外推到 length、索引、slice、分词或格式化。
需要的新表面先形成有界 proposal，启用必要 Guide 增量后再制定实现合同。
可选择具体整数转换函数，不必同时引入 Str 或通用 toString 协议。
Result 构造/匹配与后缀 `?` 分别核验，重点验证 Err 提前返回时的 owner/loan/drop；
已有泛型、闭包只补程序实际触发的缺口。borrowed closure 与 move closure 保持现行不同能力，
不把所有闭包缩成只能活过单次调用的模型。

## 7. M2 与 M3：语义决策和集合合同

M2 先说明现行元素 place 直接 Borrow 能解决哪些需求，再评估新增返回能力。
litmus 至少覆盖来源唯一性、临时接收者、链式调用、容器搬迁失效、分支、capture、
返回和字段存储；数量由风险覆盖决定，不为凑数量增加等价样例。
选择新增语义时，proposal 与 Guide 明确启用先于相应 ADR/Spec 实施；ADR 不替代语言规范。

M3A 使用现行 Array/List/MutableList 名称，不默认增加 Vec、Option 或条件成员。
新增操作逐项定义 move/borrow、长度/大小溢出、relocation、别名和资源清理。
M3B 复核[Map 候选](../proposals/map-ownership.md)后形成新决定，至少封闭 Hash/Equal 一致性、
MoveOnly key 查询、覆盖旧值归属、删除、扩容失效与迭代/输出顺序；候选正文仍为非规范。
Set、开放 Iterator/Iterable 与借用视图按各自需求评审，不作为 Map 的隐含交付包。

## 8. M4：分层安全证据

1. M4a 先选 Linux 小范围验证检测接线，用故意错误探针证明 Koven 生成的用户函数与
   runtime/drop glue 都被检测；仅宿主测试包装器触发失败不能算覆盖生成代码。
   ASan、UBSan、泄漏检测各自记录适用操作、工具前提与实际覆盖，不能互相代替。
2. M4b 再对支持的语义片生成程序，保留 seed、源码、编译器 SHA、执行环境和最小化失败。
   差分只用于语义未变且两版共同支持的范围；旧二进制不作为全部语义的真源。
3. 内存错误按 frontend、lowering、runtime、检测接线或 oracle 归因。正常无环路径核精确清理；
   现行 Rc 强环和 Abort 不展开独立分类，不能把允许行为当成清理回归，也不能借分类隐藏 UAF。
4. M4c 定向外审在原治理计划前置满足、材料和范围明确后开展；向外部发送材料需独立授权。
   随机样本数量和外审结论都不构成完整内存安全的证明。

发布前选定的 M4a 用例必须实际通过；大规模随机预算和外审不是首次 preview 的默认无限前置。

## 9. M5 与 M6：可试用交付和后续演进

M5 先提供受支持 macOS arm64 / Linux x86_64 glibc 的 preview，验证 LLVM 依赖和新环境安装。
仓库描述、README、限制页与示例按实际能力交付；发布动作另按当次授权执行。
成本测量固定源码/工具链、冷暖缓存定义和原始日志，分别报告时间、RSS、产物及运行开销；
Rust/C 对照必须固定等价行为、编译选项和工作量，不用单个数字宣传全面优势。

M6 分开对待已实现的非原子 Rc、v1 Transferable 所有权转移线程 API、未来 Arc/共享状态。
线程的 spawn/join、失败、清理和捕获仍需实现合同；跨线程共享需独立语义批准。
交叉编译、Windows 和 musl 是目标平台工作，不与共享所有权捆绑，也不据保留字提前实现协程。

## 10. 执行与复盘

这些文档是需求、待决问题和验收的起草材料，不是注册在 `docs/specs/drafts/` 的正式 Spec，
也不是新语言规则的 proposal。具体语义设计须另写入 proposals；本页不提前分配编号或批准 API。
实施时按以下流程调整，不要求逐项机械照搬草稿：

1. 记录当次 main SHA，阅读相关公开接口、直接调用方、共享实现与现有测试，复核真实缺口。
2. 删除已经解决的任务；按用户能力和风险拆分、合并、重排候选切片，重新估算成本。
3. 在正式 Spec 记录“原假设 → 新证据 → 调整及理由 → 下游依赖/验收影响”，保留必要负向
   与资源验证；不能靠删除失败用例降低验收标准。尚无实施时不虚构调整历史。
4. 常规实现路径和范围内拆分按已有授权推进；新增语言语义、长期架构或超出授权的交付
   仍履行对应 Guide/ADR/授权前置。本轮允许调整草稿，不等于解除治理等待或启用新语义。
5. 将选定内容迁入正式 Spec 后，这里只保留链接与尚未承接的候选范围；避免维护两套合同。

- 一个正式 Spec 对应一个用户能力、正确性问题或可独立验证合同；本页不替代各 Spec 验收账本。
- 连续三个 PR 若没有增加用户可见能力或验收证据，暂停该拆分方式并复盘范围和收益。
- 每项先复用代码、已有依赖和标准库；没有合适实现时说明缺口再增加代码或依赖。
- 层次遵循[测试与分层验收](testing.md)；passed、filtered、ignored、未运行和失败分别记录。
- 当前按用户新授权推进 M1A 的有界切片；后续新语义、外部发布和实验预算仍按各自前置执行。

本页状态保持 draft。G0–G3、M0–M6 均不因文档写入或结构检查通过而被勾选完成。

## 11. 文档起草记录

| 批次 | 范围 | 实际验证与评审 |
|---|---|---|
| 首批 | 本计划、M1A 与开发入口 | 当时 `check_docs.py` 检查 495 份 Markdown 通过；独立评审发现的普通字段赋值和 Inout/字段迭代边界已修正并复核 |
| 后续里程碑续写 | 新增 M1B、M2、M3A、M3B、M4、M5、M6，补充共同调整规则与入口 | `check_docs.py` 检查 502 份 Markdown 通过；`git diff --check` 及 9 份新文件空白/末尾换行检查通过；独立评审发现并修正 Map 对 Abort 清理的过宽要求，复核通过 |

续写评审覆盖七份材料与计划，定向核对 Guide 的借用、Transferable、资源与迭代边界，
同时检查依赖循环、检测 oracle 和 preview 前置。实现缺口、工具兼容性及远端 CI 未重新验收。

本批未修改生产代码、测试脚本、Guide、正式 Spec inventory 或治理执行账本。
各草稿验收矩阵均未执行；没有运行 Cargo、sanitizer、性能测量或安装/线程测试。

## 12. 首片启动与保留项（2026-10-04）

文档 PR39 的 head `213b624` 经 CI37191674421 文档、尺寸、依赖及汇总门禁通过，
Rust job 按纯文档规则跳过；merge 为 `b92593d6f5c8d11c64bac6d0a84811b6b36a7fa1`。
该基线含治理 PR38 `b32602e`，其 CI37188477831 双宿主成功。
[治理续行记录](governance-local-continuation.md)中的旧“待发布/未跑 Linux”是当时快照；
新 CI 补充对应 head 的验证，不自动接受 P2 噪声/退化预算或完成其余九组成本实验。

M0 本次完成基线、Spec 编号及首片入口接收；历史尺寸欠账、成本接受和外审仍按原计划保留。
完整三文件程序的真实 CLI 红测返回 native `UnsupportedNode`，不能据此推定其它缺口通过。
独立字段样例确认跨文件 var 误报 L0134，故拆出
[SPEC-0263](../archive/specs/0263-unit-field-mutability.md)先承接 A2。
选择、结果和未覆盖范围只记该 Spec；M1A 总验收、字段直接 Borrow 和 unit for 继续开放。

后续从 PR40 merge `11acf62` 启动
[SPEC-0264](../archive/specs/0264-unit-direct-field-borrow.md)实施 A3。该 merge 的主干
[CI37193746866](https://github.com/Halckon/Koven/actions/runs/37193746866)10个任务全部成功。
unit for 调查确认 typed descriptor、provider/source loan 与有序清理事实尚缺；先闭合前端合同，
再由后端消费，保持 Guide §37.4 的前端与 native 范围区分。并行实施使用独立分支/worktree，
本地 Cargo 仍串行；M1A 总退出条件保持。

## 13. M0 编辑器覆盖复核（2026-10-04）

在 `6cdaa2c` 对当前 editor 输入进行了独立复核；本记录从主干 `f0effcd` 发布。
逐文件 SHA256 核对两提交的 editor 输入相同，证据见
[审计清单](evidence/m0-editor-20261004/audit.json)和
[重新生成后的原始失败日志](evidence/m0-editor-20261004/tree-sitter-regenerated.log.gz)。
本次没有修改 grammar、scanner、parser、corpus 或任何预期。

| 检查 | 结果 | 证据边界 |
|---|---|---|
| Tree-sitter 0.26.12，editor 目录执行 `tree-sitter test` | exit1；10项中5通过、5失败 | 完整 corpus，不用 Rust fixture 的成功替代 |
| 隔离副本执行 `tree-sitter generate` 后再 `tree-sitter test` | generate exit0，test exit1，仍5通过/5失败 | 新生成的 parser.c、grammar.json、node-types.json 与仓库逐字节相同；重新生成不能修复这批失败 |
| Node v26.8.2 执行 `node editors/textmate/tests/verify-lexical-contract.mjs` | exit0；80条通过 | 词法正则合同，不代表真实编辑器完整着色或 Tree-sitter 语法通过 |

五项失败分别为 File header and declarations、Calls and lambda、Own remains declaration-only、
Control flow、Reserved words are not identifiers。日志中的树差异仍需按 Guide 与实际 parser
区分 grammar/scanner 缺陷和过时预期；未确认根因，不能批量更新 golden 取得绿灯。
后继应独立修复并将完整 corpus 接入必需门禁。当前 CI 的 Rust editor fixture 检查继续有用，
但不执行此 Tree-sitter CLI corpus；本次复核不关闭编辑器交付，也不声明 M0 所有保留项完成。

后继 [SPEC-0267](../specs/active/0267-editor-corpus-gate.md) 已在隔离分支修复参数模式、
上下文词与词运算符边界，明确命名参数优先级，逐项审阅两份恢复 golden。
本地真实 corpus 为 10/10，新增 CLI 树合同为 9/9，frontend editor 交叉合同为 9/9；
独立 editor CI job 已接线，远端精确 head 验收仍待 PR；历史 M0 红测证据保持不变。
