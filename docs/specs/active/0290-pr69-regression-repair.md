# SPEC-0290: PR69 交付回归修复与验收边界一致

> **性质**：变更合同 · **状态**：in-progress · **读取时机**：修复或验收 PR69 审计发现时 · **唯一真源**：本 Spec

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P6-0290` |
| 所属 Phase | Phase 3 所有权事实；Phase 4 typed SSA；Phase 5 LLVM；Phase 6 native 与交付验收 |
| 语言规范 | 现行 [Guide v0.42](../../guide/README.md)，尤其所有权、集合及实施边界章节 |
| 批准依据 | 2026-10-10 用户授权在云端对 PR69 审计六项发现分阶段计划与修复；未授权 push、创建 PR 或 merge |
| 前置 Spec | SPEC-0288 已归档且 PR69 已合入 main；原证据按需从 archive 追溯 |
| 前置 ADR | [ADR-0029](../../adr/accepted/0029-ordinary-borrow-result-continuation.md) 已 accepted |
| 关联 ADR | 无新增长期架构决定 |
| 阻塞项 | 工具链恢复与回归证据待核验；N1a 语义处置属于 decision gate，不是已批准实现 |
| 影响范围 | `lang-codegen`、相关 frontend/CLI 回归、CI 测试选择与当前事实文档 |
| 语言语义变更 | 否；不通过本 Spec 启用、禁用或改写 N1a 语义 |

## 1. Goal

在现行 Guide 合同内修复 PR69 的编译回归，使普通源码回归、CI 测试选择和交付陈述相互
一致；对未授权的 N1a 语义处置保留明确决策边界，不将未决发现或未运行验收计作闭环。

## 2. 背景

本次从 main `ef60f2f` 新建 `fix/spec-0290`，不占用为 N1a 后续工作预留的 0289 编号。
审计定位四个代码缺陷、一项 CI 选择遗漏，以及 N1a “休眠”陈述与实际入口的不一致。
旧 PR 的归档结论保留为历史，当前修复与新证据单独记录。

现行规范以 [Guide 入口](../../guide/README.md)为准；其他页面的旧版本标记不能覆盖其
v0.42 权威。工具链保持 Rust 1.96.0 与 LLVM 21.1.8，不以降级、替代版本或删除覆盖来
取得绿色。失效的旧工作目录或旧构建结果不能充当本分支验收。

## 3. 范围与需求

| 编号 | 审计发现 | 本次批准的可观察结果 |
|---|---|---|
| R1 | Map receiver owner 的缓存 ValueId 跨后续参数求值的 CFG 变化失效 | 合法参数控制流之后，Map 操作使用当前 block 的有效 owner；单文件/unit 路径按实际影响验证 |
| R2 | MapPut/MapRemove 的 K/V drop glue 依赖偶然出现的 scope Drop | 操作自身登记所需 drop glue；普通源码以 Abort 终止、无作用域 Drop 时仍可生成并验证 LLVM |
| R3 | 透明 Group 包装借用返回 Call 时，source.call 与外层表达式身份不等导致拒绝 | 透明分组不改变来源合同；保留真实 Call 身份，合法源码可 lowering，非法来源仍拒绝 |
| R4 | unit borrow-return wrapper 以 owner.field 传借用参数时缺 pending call frame | 为直接字段来源建立真实调用帧并正确结束 loan；不扩大投影、逃逸或其他 ABI 支持 |
| R5 | 23 个新增 frontend integration targets 未进入 CI 组合执行 | 清点实际遗漏名单、去重纳入现有选择，并以选择合同锁定应执行目标，避免只有源码却不执行 |
| R6 | N1a 声称休眠，但 CLI 标准源与 take 实际无开关 | 核对普通 CLI 输入、标准源装载及 take 的实际路径；准确登记可达性与规范差异，提交用户决定 |

R1–R4 先写普通 Koven 源码最小回归，记录修复前失败及其阶段，再对同一选择记录修复后
结果；不以手造非法内部状态或故障注入代替用户可遇到的输入。R2 的正常源码 Abort 合同
不等同于内存故障生成、注入或校准；其验证不要求执行危险内存路径。

R5 的“23”是审计清单，不是永久白名单：以实际 Cargo integration target 清单、现有脚本
与 CI 调用链对照，记录每个目标是否恰当选择、是否实际运行。配置接线不等于执行成功。

R6 仅批准事实核对和决策请求，不批准改变 N1a 的实现能力或语义。如果需新增、删除或
关闭已有能力，或 Guide 正文与实现发生语义冲突，先停在决策门，取得用户明确决定并满足
Guide 启用规则后另行确定合同。这个边界不阻塞已明确的 R1–R5 现行合同修复。

## 4. 非目标

- 不新增 Map API、任意借用存储、复杂 NLL、未支持的投影或 ABI。
- 不借 N1a 审计自动启用扩展函数/carrier/take，也不擅自删除或禁用已存在的能力。
- 不运行故障生成、内存故障注入或校准；不调用会间接触发这些入口的聚合门禁。
- 不修改 archive 中的旧 Spec 正文、验收结论或证据；生成的依赖拓扑只作机械更新。
- 不默认执行 frontend 全量，不用无关全量测试替代定向红绿证据。
- 本次不 push、不创建远端 PR、不 merge；后续需另获授权，不能据本地结果宣称远端 CI 通过。

## 5. 验收标准

- [ ] A0：记录当前分支、基线、Rust/LLVM/Clang 与 native 链接工具实际版本，确认可执行基线。
- [ ] A1：R1 的普通源码控制流参数回归在修复前失败、修复后通过；有效 SSA 与 native 输出符合预期。
- [ ] A2：R2 的 MapPut/MapRemove 各有普通源码回归，无 scope Drop 也能完成 LLVM 生成与验证；相邻正常清理路径不退化。
- [ ] A3：R3 的分组借用返回正例与不分组对照通过，现有错误来源/逃逸拒绝边界不变。
- [ ] A4：R4 的 unit owner.field wrapper 正例通过，loan/frame 事实与 native 结果吻合，已有字段借用拒绝边界保留。
- [ ] A5：R5 的遗漏清单逐项对齐 CI 调用链，选择 policy 正反例通过，实际执行数与未运行部分分别登记。
- [ ] A6：R6 的当前路径、Guide 对照与需用户决定的选项有源码/测试证据；未决不标为语义修复完成。
- [ ] A7：按影响面完成 fmt、受影响 crate clippy、直接消费者与必要 native 回归，记录实际命中数和未运行项。
- [ ] A8：独立审查确认修复、CI 选择和批准边界一致；当前 Architecture 已同步，或逐项说明无需更新。
- [ ] A9：文档/尺寸/diff 门禁及分批本地提交证据完成；远端交付保持待授权，不据此归档为 done。

## 6. 技术方案与边界

Map lowering 按当前 CFG 消费 owner 事实，不能跨参数求值复用已失效 SSA identity。
LLVM 对 Map 操作的资源依赖应由操作自身声明，不依赖函数其他位置出现 Drop。
借用 wrapper lowering 消费前端发布的真实 Call/来源身份与 loan 计划：透明 Group 只负责
语法包装，unit 字段实参必须使用已有调用帧机制，不另造全局或旁路来源推断。

测试优先落在已有 Map 与 borrow-result lowering/native 套件；必要时补 frontend 类型或
所有权正反例。内部断言可补强源码回归，但不能取代实际编译输入与 native 行为证据。
CI 选择沿既有脚本整合，使用 Python policy 测试验证缺失、重复或过滤器错误会被捕获。
不新增绕过门禁的 skip 开关，不将 N1a 可达性测试当成语义授权。

## 7. 实施计划

阶段按 S0 → S1 → S2 → S3 交付；各阶段独立准备不代替上一阶段验证，Cargo 门禁串行。

| 阶段 | 状态 | 实施与退出条件 |
|---|---|---|
| S0 恢复基线 | in-progress | 恢复固定工具链、确认仓库/依赖与测试入口，运行最小 smoke；具备真实回归失败证据后进入 S1 |
| S1 四项修复 | pending | 按 R1–R4 每项完成普通源码红测 → 最小修复 → 同选择绿测；共享 helper 追加直接调用方回归 |
| S2 选择与边界 | 准备结果已产出，待阶段验收 | R5 补齐 CI 选择及 policy；R6 核验事实并呈交 decision gate。未决语义不实施，不宣称六项全部修好 |
| S3 验证与审查 | pending | 汇总定向与 native 证据、独立审查、同步事实并分批本地提交；保留 macOS/远端未运行及用户决策阻塞 |

## 8. 提交计划

本 Spec 可分批提交，但每个实现提交须可构建、可测试；不把未运行项填成成功。

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | 合同、索引与冻结 inventory | `docs(spec): plan PR69 regression repairs (SPEC-0290)` |
| 2 | R1–R2 Map 修复及各自回归，可按独立可验证边界再分 | `fix(codegen): repair Map owner and drop dependencies (SPEC-0290)` |
| 3 | R3–R4 借用 wrapper 修复及回归，可按独立可验证边界再分 | `fix(codegen): preserve borrow wrapper call frames (SPEC-0290)` |
| 4 | R5 CI 选择、policy 与 R6 事实核验记录 | `test(ci): cover PR69 frontend targets (SPEC-0290)` |
| 5 | 实际验收、独立审查与当前事实同步 | `docs(spec): record bounded PR69 repair evidence (SPEC-0290)` |

分批提交由本次任务统一协调；远端操作待单独授权。未完成所需双宿主/PR 交付或仍有
会影响本合同闭环的阻塞时保持 active，不能为了最终文档提交而提前 done/归档。

## 9. 未决问题

本次 R1–R5 的现行合同修复范围无待启用语义。R6 的 N1a 语义处置尚未决定，明确在本次
已批准实现范围之外：要维持哪项能力、是否需开关、是否启用新 Guide，须提交具体差异与
证据后由用户决定。审计六项不能因此整体宣称已闭环。

若后续把该待定语义实现纳入同一 Goal，须重新确认合同；存在改变范围/语义的未决问题
时按模板保持 draft，不把本 Spec 的 in-progress 当作 N1a 批准依据。

## 10. 验证记录

实际命令、精确目标/过滤器、测试命中数与结果在执行后补入本表；失败与未执行记录保留。

| 验收项 / 命令（目标与过滤器） | 结果（实际测试数） | 未运行原因 / 复用证据 |
|---|---|---|
| A0 固定工具链与 smoke | 未执行 | 基线恢复中，由统一执行入口核验 |
| A1–A4 普通源码红绿、SSA/LLVM/native | 未执行 | 等待工具链与新增回归，按各修复的真实选择填入 |
| A5 新增 selection policy 修复前单测 | failed：1 test，23 个 target 子项失败 | 锁定 PR69 相关 25 个 targets，23 个原先遗漏；保留红测，不计为成功 |
| A5 `python3 -m unittest scripts.tests.test_check_ci_results scripts.tests.test_recovery_gates` | 35 passed | 选择修复后；组合 102 个唯一 integration targets / 10 次 Cargo 调用，未执行真实 Rust targets |
| A5 `bash -n scripts/check_stage_integration.sh` | passed | 仅 shell 语法检查，未运行聚合或故障脚本 |
| A6 N1a 可达性与规范差异 | 未执行 | 仅核对与决策请求；语义处置未获批准 |
| A7 fmt/clippy/消费者与相邻 native | 未执行 | 由最终代码影响面确定最小充分集合 |
| A8 独立审查与 Architecture | 未执行 | 等待代码及事实核验结果 |
| A9 `python3 scripts/check_docs.py` | passed，598 Markdown 文件 | 仅结构检查，不证明语义或 Rust 实现正确 |
| A9 `python3 -m unittest scripts.tests.test_check_docs` | 37 passed | 冻结 inventory 修改后的检查器回归 |
| A9 `git diff --check` | passed | 2026-10-10 文档初版；最终提交前需按最终 diff 复核 |
| macOS / 远端 PR CI / main CI | 未执行 | 本次云端本地执行不替代双宿主 CI；push/PR/merge 未授权 |
