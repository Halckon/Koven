# SPEC-0271: 当前 tour 的有界组合覆盖

> **性质**：变更合同 · **状态**：in-progress · **读取时机**：补强或验收当前教程时 · **唯一真源**：本 Spec

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P6-0271` |
| 所属 Phase | Phase 6 教程与工程验收 |
| 语言规范 | 已启用 [Guide v0.40](../../guide/README.md) |
| 批准依据 | 用户授权独立分支补强最新 tour、真实 examples 及诊断边界 |
| 前置 Spec | SPEC-0262、SPEC-0268（均 done） |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无语义前置；已获主干同步 PR 授权，双宿主 CI 待执行 |
| 影响范围 | `docs/tutorials`、教程 Python 合同、当前导航与事实摘要 |
| 语言语义变更 | 否 |

## 1. Goal 与基线

给读者增加有限的可执行组合及独立负例，并以真实 CLI 完整输出合同保护它们。
基线为创建时 main `4ce0eb80f4ce91a6a6a7c73426d64872293a18e5`；
worktree `/tmp/koven-tour-spec0271`，分支 `feature/spec-0271`。
基线有12个 executable、2个 diagnostic、1个 planned 定义，17项执行合同。

## 2. 覆盖差距与本批范围

| 维度 | 已有覆盖 | 本批新增的有限代表 | 边界 |
|---|---|---|---|
| 数字与控制流 | 十进制常量、算术、if | radix/分隔符、位操作与移位屏蔽 | 固定 Int；不覆盖全部宽度和溢出 |
| 资源生命周期 | 单个 resource 的作用域析构 | callee资源逆序及caller继续执行 | 分支内多资源的native失败保留；不推定纯内存ASAP |
| unit/native 组合 | 跨文件函数、argv、字段 replace | 临时容器的 Borrow 迭代、局部 resource、continue/break/return 和 caller | 只选择具体 MutableList provider；不推定任意容器 |
| mutable place 诊断 | root replace/swap 正例 | 对不可变 root 的 replace 精确诊断 | 单文件负例；不推定所有 place |
| 迭代借用诊断 | 重复消费的 L0131 | provider 活跃借用时移动 owner 的精确诊断 | 区别于普通 use-after-move |

新增正例 `numbers-bitwise`、`scope-cleanup`、`unit-loop-cleanup`；
负例 `reject-immutable-place`、`reject-iteration-move`。实际发现的 `gap-scope-branch`
单独列为 planned，保留失败源码，不能计作成功。当前21个定义、25个源码fence，
15 executable / 4 diagnostic / 2 planned，共22项执行合同。
每项新增源码只写入 tour Markdown；JSON 只保存输出合同及 fence/path 映射。
现有四组 parameter-report argv 保留。不机械复制它们，也不把本批覆盖称为穷尽。

## 3. 非目标

不修改编译器、Guide 语义或 planned-thread；不增加未来能力、依赖和配置；
不做 P2 性能测量，不重复未影响的 Rust 全套测试，不自动合 main、推送或创建 PR。
发现规范与实现不一致时记录真实失败，不能改规范或将失败当通过。

## 4. 验收标准

- [x] 新正例逐个实际 build、执行 artifact、CLI run；完整 exit/stdout/stderr 相同。
- [x] 新负例实际 JSON build，核对完整诊断码、Span、顺序和无产物。
- [x] 提取器数量合同调整有定向 Python 红绿证据，既有编排合同仍通过。
- [x] 当前入口、覆盖矩阵、Architecture 事实与验收记录一致；docs/diff 门禁通过。
- [x] 明确本机 Mac 验收、未执行 Linux/远端 CI 和仍未覆盖范围。

## 5. 实施与提交计划

1. 建立独立范围及 Spec inventory，执行文档门禁，提交计划。
2. 新增 Markdown 源码和 JSON 合同；先红后绿调整提取器合同；构建基线 CLI，定向实跑新增例。
3. 同步事实与验收证据，执行受影响 Python/docs/diff 检查，独立本地提交。

| 顺序 | 提交边界 | 提交信息 |
|---|---|---|
| 1 | 范围、差距、active inventory 与依赖图 | `docs: define bounded tour coverage (SPEC-0271)` |
| 2 | 新示例、提取合同、真实验收与当前事实 | `docs: verify tour combinations and diagnostics (SPEC-0271)` |

本机实施提交后保持 in-progress；双宿主 CI 及用户决定交付前不宣称归档条件满足。

## 6. 验证记录

| 验收项 | 实际结果 | 未运行项或说明 |
|---|---|---|
| `python3 scripts/gen_spec_dag.py`、`python3 scripts/check_docs.py`、`git diff --check` | passed；513 Markdown，依赖图1 live/255 archive | 初始范围提交的结构验收；不证明语义或 native 行为 |
| `LLVM_SYS_211_PREFIX=/opt/homebrew/opt/llvm@21 cargo build --locked --offline -p lang-cli -j 2` | passed，33.37s；Rust/Cargo1.96.0、LLVM21.1.8、Mac AArch64 | 独立target；Rust生产源码与基线相同；CLI SHA256 `131f089b91f7f791c3cce311c1ea3685d5d4642d687993ef3603a6df5b138e79` |
| `check_tutorial.check(cli, ['numbers-bitwise', 'scope-cleanup', 'unit-loop-cleanup', 'reject-immutable-place', 'reject-iteration-move'])` | passed：3正例build/artifact/run及2负例JSON build，共5项/11次真实子进程 | 临时capture函数实际执行subprocess并保存完整exit/stdout/stderr；[原始输出及源码hash](../../development/evidence/spec-0271-mac-20261004/cli-contracts.json)，非mock；同一选择可由5个`--example`重现 |
| `python3 -m unittest scripts.tests.test_tutorial_contracts.TutorialContracts.test_new_combinations_extract_canonical_sources_and_execute_each_contract` | red：1项实际失败，`ValueError: expected twelve executable contracts` | 新源码/JSON/测试已写，旧helper数量合同尚未改；不是CLI运行结果 |
| `python3 -m unittest scripts.tests.test_tutorial_contracts`（数量合同改后） | green：10项passed | 随后新增planned边界测试，最终结果见下；mock打印的passed不计真实验收 |
| `gap-scope-branch` 单独build探测 | 实际失败：exit2、stdout空、`InvalidModel: frontend lowering failed with InvalidSsa`；仅source.ko留下 | 已在最终CLI证据中再现；列planned，保留缺口，不改编译器/Guide |
| `python3 -m unittest scripts.tests.test_tutorial_contracts scripts.tests.test_check_docs` | passed：48项，11教程合同+37文档检查器合同 | 不将mock输出当实际CLI结果 |
| 最终 `python3 scripts/check_docs.py`、`git diff --check` | passed：513 Markdown；无空白错误 | 仅结构/链接/依赖图，不证明语言或native全部能力 |
| Linux、远端 CI、P2、Rust 全量 | 未运行 | 独立本机教程范围；未授权远端动作 |

## 7. 未决问题

分支内多个resource的native lowering为已知实现缺口；函数作用域正例不能作为该组合的通过证据。
本批不修复此缺口。源码保留在tour的 `gap-scope-branch`，输出保留在上述证据文件。
原17项本批未重复执行；只引用此前0262/0268的双宿主交付历史，不能当作本轮验证。
Linux及远端CI尚未运行；Spec保持active，待用户决定后续交付。

## 8. 用户授权本地合入

2026-10-04 用户授权将两个实施提交普通合入本地main，不推送或创建PR。
合入前main为 `ed61d7d59f75b5da8c054dfd6966ec309ad0f200`，已接收0270/PR47；
归档inventory保留0270，合入后1 active/256 archive。Spec保持active，不把本地合并当远端CI验收。
Spec索引冲突取两批状态的并集，依赖图从合并后的inventory重新生成；
未修改tour源码、JSON输出合同或编译器，原 `4ce0eb8` 基线的Mac输出和 `InvalidSsa` 缺口证据保留。
本次合并的验证记录独立于第6节原基线运行结果，不声称新main已重跑native。

| 合入检查 | 结果 |
|---|---|
| `python3 scripts/check_docs.py`、`git diff --check` | passed：515 Markdown、无空白错误；依赖图1 live/256 archive |
| `python3 -m unittest scripts.tests.test_tutorial_contracts scripts.tests.test_check_docs` | passed：48项；mock不计native验收 |
| 提取与来源hash、与原分支字节比较 | passed：21定义/25 fence、15 executable/4 diagnostic/2 planned、22合同；tour源码、JSON、runner/tests与实际native证据均与原分支相同，已捕获源码hash匹配 |
| 新main native、Rust全回归、Linux、远端CI | 未运行；无代码/源码/输出合同冲突，不重复未受影响的回归；没有远端授权 |
| 原工作区及0272并行worktree的未提交文件 | passed：分别6/6及10/10项SHA256相同；未stash/reset/清理或stage用户文件 |

## 9. 主干同步 PR 授权

2026-10-04 用户要求将本地 main 领先远端的提交通过 PR 推送、合并，然后在最新 main 建立
后续 worktree。该要求更新此前仅本地交付限制；历史验收及当时授权范围保持原文。
同步分支 `codex/main-sync-20261004` 从本地 main `815b143` 的已提交快照建立，
其相对远端 `299469b` 为23个独有提交、远端0个独有提交。原工作区未提交文件不纳入。
本批先验收准确 head 的双宿主 CI，再补齐本 Spec 归档及最终 head CI，不用主干本地合入代替。
嵌套 if 已知实现缺口由独立 SPEC-0273 修复分支承接；本同步 PR 不修改编译器或将 planned
案例改成通过。正常教程例与诊断合同仍须实际验证，历史失败证据原样保留。

同步快照本机复核：`python3 scripts/check_docs.py` 514页通过；Python scripts/tests 全部134项
通过、无跳过（日志 `/private/tmp/main-sync-python-tests.log`）；真实 release CLI 执行当前全部
教程22项合同通过，2 planned明确未执行（日志 `/private/tmp/main-sync-tutorial.log`）。
使用先前从main299469b构建的release CLI；同步差异不包含Rust生产或Cargo manifest/lock，
故生产源码相同，不把该复用描述为在新同步分支重新构建。仍待双宿主远端CI。
