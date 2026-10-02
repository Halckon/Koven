# SPEC-0244：owned mutable root 原子 replace / swap

> **性质**：实施 Spec · **状态**：done · **读取时机**：实现或验收 owned root 原子置换时 · **唯一真源**：本 Spec

| 字段 | 值 |
|---|---|
| 状态 | `done` |
| Goal ID | `KOV-P4-244` |
| 所属 Phase | Phase 3 ownership；Phase 4 SSA / LLVM / native |
| 语言规范 | [所有权与原子置换](../../guide/10-ownership-borrowing-drop.md) |
| 批准依据 | 用户已授权原演进计划持续按“本地验证 → 草稿 PR → 双平台 CI”发布，合并由用户决定 |
| 前置 Spec / ADR | SPEC-0232 的两入口 typed intrinsic 身份；无新增语言语义或 ABI |
| 阻塞项 | 无；SPEC-0243 的方法 receiver 两阶段借用不属于本切片依赖 |
| 影响范围 | owned mutable whole-root 的 ownership、两入口 lowering、SSA verifier、LLVM/native |
| 语言语义变更 | 否 |

## 1. Goal 与封闭边界

从 main `2ad6967` 新建独立分支，随后合入用户已合并 PR #10 的 main `17a7a0e`，消费 SPEC-0232 的稳定 intrinsic 身份及源码 operand 顺序，
为已支持存储表示的 owned mutable whole-root 实现 replace 返回旧 owner、swap 保留两个 owner。
不得按源码名称或普通 ExternalCall 猜测原语，也不得用普通 assignment 的 old-value drop 代替交换。

“原子”指语言所有权转换，不承诺 CPU 跨线程原子性。operand 各求值一次；第一个 Inout 立即
独占，并在后续求值期间保护旧值。所有 operand 正常继续后才提交；return / break / continue
清理未调用前缀，Nothing abort 不展开，目标不得提前移空。Copyable 保留独立快照，不派发重载。

首片不扩展字段、index、普通 Inout 参数 ABI、closure/provenance 交换、deinit、原始指针或线程
原语。前端继续使用现有 mutable-place 和 overlap 检查，未覆盖 native 位置明确拒绝；不得把
既有 Guide known-gap 当成完整支持。具体 storage family 与控制流验收由以下实际测试记录决定。

## 2. 实施

1. [x] 单文件 SSA 红测确认原基线 replace / swap 均 UnsupportedNode。
2. [x] ownership 正常 commit 计划、source-qualified 身份与错误清空。
3. [x] 显式 ROOT SSA 交换操作、严格 provenance / loan / consumption verifier。
4. [x] 两入口按顺序求值与 CFG/提前退出处理，LLVM 交换不析构旧 owner。
5. [x] 原语 native 输出、源顺序与逐指针分配/释放计数。
6. [x] 定向回归、共享门禁、严格 lint、文档和双平台 PR CI。

## 3. 验证账本

| 验收项 / 命令 | 实际结果 | 限制 |
|---|---|---|
| `cargo test -p lang-codegen --lib root_primitive_single --locked --offline` 实施前 | 0 passed / 3 failed / 538 filtered | 均为预期 UnsupportedNode；原语尚无 backend |
| 工具与空间 | Linux x86_64；Rust 1.96 / LLVM 21.1.8；19GB 可用 | 统一 target、CARGO_INCREMENTAL=0，Cargo 串行 |
| ownership / SSA API 红测 | 19 处缺失 ownership API；4 处缺失 SSA op；均已修复 | compile-fail 先于实现 |
| 多分支 root 析构红测 | 原 CallReturn 生成两份条件 drop，1 failed；归一完整值后通过 | 不放宽普通条件 closure drop |
| 内层 loop / Unit 合流红测 | unit 2 failed；single Unit 1 failed，修复后直接选择 33 passed | 保留 ordinary Value snapshot 隔离 |
| `ownership_primitives` / frontend `root_primitive_` | 14 integration / 3 lib 通过 | 9 类损坏事实拒绝，基础与 constant 验证 |
| frontend lib / stage gate | 183 passed；60 targets / 756 passed | 非 frontend 全量，无新增 ignored |
| 首轮 codegen / doctests、CLI / LSP | 590 + 4 / 66 / 26 passed | Linux 真实 object/link/run；末次 loop 修复后的重跑另记 |
| 原语 native 精确回收 | 六类 owner 共 30 allocations / 30 frees；conditional return 3 / 3；unit 3 / 3 | 逐 pointer 核对；Abort 保留旧值且不 unwind |
| Guide gate | 187 frontend + 14 codegen = 201 passed | 含新 main 的 two-phase；known gaps 不改写 |
| workspace check、严格 clippy、fmt | 首轮均通过 | `--all-targets --locked`、`-D warnings`；无新增 lint 豁免 |
| 文档 / Python policy / diff | 451 篇 / 45 tests / whitespace check 通过 | 结构不替代语义证明 |
| 最终 shared-CFG 回归 | codegen 595 + 4 doctests、CLI 66、Guide 201；check / clippy / fmt 全通过 | 内层loop与Unit修复后的最终源码；Linux无ignore |
| 首轮本 PR 双平台 CI | run 36962330050 completed / success，8/8 jobs success、无job skip | head bedc21ad；不以旧PR结果替代 |

## 4. 交付

先保存本地 commit/bundle，再完成有界验收，发布一个面向 main 的 Draft PR，跟进 macOS/Ubuntu
完整 pull_request CI；不自动合并、不改 ready 状态。验收全部通过后归档本 Spec，最终 head
再次验证，最终远端结果记 PR，避免无限追加账本提交。

## 5. 已知边界与保留项目

- owned mutable root 支持真实既有 scalar/Unit/String、class/value class/enum、Box、Rc、顺序容器与 pointer-like nullable 表示；详见[实现边界](../../architecture/root-ownership-primitives.md)
- field/index/Inout 参数原语 native、closure 及递归含 closure provenance、未实例化泛型模板、deinit/资源词法析构均未交付
- 基础 unit 的既有参数控制退出边界未取消；constant-enabled unit 有 native return/break/continue/内层loop证据，通用基础短路分析不在本片重写
- frontend 全量未运行；SPEC-0242 的五项 multifile type 与五项完整编辑器 corpus 历史失败不修改、不声称通过
- 本地 checkpoint 为 6c8752e、ab9dc33；合入用户 PR #10 后的基线整合提交为 2409eb3。独立 feature 最终对 main 发 Draft PR，不隐藏堆叠依赖

## 6. 首轮远端完成证据

[Draft PR #11](https://github.com/Halckon/Koven/pull/11) 的实现 head
`bedc21ad7e82a6f7c152545654a70e701e63ef71` 已完成
[pull_request run 36962330050](https://github.com/Halckon/Koven/actions/runs/36962330050)，
2026-10-02 读回 completed / success，全部 8 个 job 成功，无跳过的必需 job。
远端 tree `b818004a562f0479a97d2708d7e9b4a9971f554a` 与本地 `bce3336` 完全相同，
59 个 blob SHA 逐一核对，git fetch 再次验证远端树。发布经过同一 GitHub connector，无 force。

| 宿主 | frontend lib | codegen / doctests | CLI / LSP | stage | Guide |
|---|---|---|---|---|---|
| Ubuntu 24.04 | 183 | 595 / 4 | 66 / 26 | 60 targets / 756 | 187 frontend + 14 codegen |
| macOS 14 | 183 | 594 passed、1 既有 ignored / 4 | 65 / 26 | 60 targets / 756 | 187 frontend + 14 codegen |

两平台均实际执行全部新 root 原语 SSA/LLVM/native 测试，包含内层循环与 Unit 提前退出修复。
macOS 唯一 ignored 仍是既有 `lldb_hits_a_koven_source_breakpoint_and_reports_the_frame`，原因是
CI 不具备 debugserver task-port 权限；没有新增 ignore，也不计作调试器通过。Guide 的三个
codegen 过滤分别为 589 / 593 / 589，不能计为测试通过。未运行的 frontend 全量与其他边界不变。

本 Spec 以声明的 owned root 切片验收完成而归档，PR 保持 Draft、由用户决定合并。归档提交
仅维护 Spec 路径、inventory、索引与生成图；最终 head 的双平台运行在 PR 更新，不继续为
最后一轮 CI 结果生成循环账本提交。
