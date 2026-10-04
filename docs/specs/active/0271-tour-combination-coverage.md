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
| 阻塞项 | 无语义前置；远端交付未获授权 |
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
| 资源生命周期 | 单个 resource 的作用域析构 | 内层资源逆序及外层继续执行 | 不把纯内存 ASAP 等同资源清理 |
| unit/native 组合 | 跨文件函数、argv、字段 replace | 临时容器的 Borrow 迭代、局部 resource、continue/break/return 和 caller | 只选择具体 MutableList provider；不推定任意容器 |
| mutable place 诊断 | root replace/swap 正例 | 对不可变 root 的 replace 精确诊断 | 单文件负例；不推定所有 place |
| 迭代借用诊断 | 重复消费的 L0131 | provider 活跃借用时移动 owner 的精确诊断 | 区别于普通 use-after-move |

每项新增源码只写入 tour Markdown；JSON 只保存输出合同及 fence/path 映射。
现有四组 parameter-report argv 保留。不机械复制它们，也不把本批覆盖称为穷尽。

## 3. 非目标

不修改编译器、Guide 语义或 planned-thread；不增加未来能力、依赖和配置；
不做 P2 性能测量，不重复未影响的 Rust 全套测试，不自动合 main、推送或创建 PR。
发现规范与实现不一致时记录真实失败，不能改规范或将失败当通过。

## 4. 验收标准

- [ ] 新正例逐个实际 build、执行 artifact、CLI run；完整 exit/stdout/stderr 相同。
- [ ] 新负例实际 JSON build，核对完整诊断码、Span、顺序和无产物。
- [ ] 提取器数量合同调整有定向 Python 红绿证据，既有编排合同仍通过。
- [ ] 当前入口、覆盖矩阵、Architecture 事实与验收记录一致；docs/diff 门禁通过。
- [ ] 明确本机 Mac 验收、未执行 Linux/远端 CI 和仍未覆盖范围。

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
| 本机固定基线 CLI build | 运行中 | 离线、LLVM21；不采用来源未确认的其它 worktree binary |
| 新例真实 build/artifact/run、诊断 | 未运行 | 源码与合同落实后执行 |
| Python 提取/编排合同红绿 | 未运行 | 调整 helper 前记录失败 |
| Linux、远端 CI、P2、Rust 全量 | 未运行 | 独立本机教程范围；未授权远端动作 |

## 7. 未决问题

无语义未决；执行结果及平台边界以本 Spec 后续实际记录为准。
