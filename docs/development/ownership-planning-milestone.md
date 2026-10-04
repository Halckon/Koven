# 所有权到 SSA 规划边界治理里程碑

> **性质**：有界组合交付与验收 · **状态**：本地实现、验收与独立最终 review 通过，精确 head 双宿主 CI 待后继 · **读取时机**：复核本批责任划分、原文保全与统一交付边界时 · **唯一真源**：本页及逐项证据；范围从属[已批准计划](engineering-governance-plan.md)

## 范围与交付单位

本页下述分支、提交、active及待PR/CI状态是旧云端批次的历史记录，不作为本次验证。
其未发布后继对象已不可用；当前本机重建与未覆盖项见[恢复验收](recovery-local-delivery.md)。
原P2成本证据/预算接受缺口仍未闭合，不从本轮文档归档推定原治理全计划完成。

2026-10-03 用户要求在当前集成分支持续提交后续工作，减少按小片发 PR 的操作。
本批继续 `refactor/p2-multifile-ownership-contracts`，从已验收组合 head
`6cc87fefe7d6486912d91966f751256ca256c0a0`、tree
`cac928487edf32f59cc868fa69b997f254a364bc` 增量实施；不为每个文件另发 PR。
分支承接从真实 main `9ac49f3` 建立的 SPEC-0255，并保留原完整提交历史。

一个最终 PR 包含已完成本地验收的 0254 归档、[0255 中立支撑边界](../archive/specs/0255-neutral-lowering-support.md)、
[multifile ownership 十二测试域](multifile-ownership-test-migration.md)，以及本页两项生产责任迁移。
生产迁移分别提交，可独立回退；统一最终验收、独立 review 与 exact-head 双宿主 CI 后才交付。
SPEC-0255 继续 active；本地通过不替代远端 CI，也不把整个 P2/P4 或治理计划标为完成。

本增量仅为 Phase 3 所有权规划及 Phase 4 实例规划的私有代码组织，沿用已批准 P2；
不新增语言规则、功能、ABI、crate、依赖或长期架构决定，因此不另建 Spec/ADR。
不重写算法，不混合两入口 concrete-type 解析，不新增尺寸例外，不修改测试正文。

## 设计审阅与职责图

两项设计均先由实施者提交函数分组、依赖与可见性说明，再经独立审阅通过，之后才移动源码。

### Frontend iteration

- `iteration.rs` 保留 provider/element 生命周期、主循环顺序、退出计划和外部使用的 root 条件查询
- `capture_graph` 负责静态 lambda 有限捕获图、可达性、环与布局顺序；只读已发布 capture facts
- `phi_state` 负责 header/exit 身份预分配、有限布局与 body/后继 seed；调用捕获图，不记录入边
- `phi_incoming` 负责 Entry/回边/耗尽实际来源、存在位、并存判断与转发；只向 state 查询出口存活

所有模块私有，主生命周期依赖 state/incoming，state 依赖 graph，incoming 可读 state 的
`exit_binding_required`。跨子模块方法至多开放到 iteration；原 drop_planner 消费方法留根。
测试专用私有导入保留旧测试路径，完整 `tests` 和 `instance_replay` 原文不动。

图算法、phi 身份与输入状态是三种不同变化原因。拆分使有限图压力预算、seed 的身份建立顺序、
实际入边的路径条件能分别审阅；不会把静态候选回放冒充已完成动态实例执行。
递归/并存场景的 deferred 原子拒绝与 Phase 4 能力限制继续保留。

### Codegen unit planner

- `unit_plan.rs` 保留 capability/身份入口、共享模型、source locator、确定性实例队列及收尾
- `recipe_preflight` 沿有限 callable/fact frontier 收集失败并保持稳定优先级
- `call_routes` 消费 frontend 已选择的 route，实例化 owner 参数并形成精确 instance key
- `recipe_validation` 传播 recipe root facts，检查 declaration/closed field graph 许可、回边与 witness
- `concrete_types` 只查询已有 canonical type；保留 preflight 和正式解析的不同拒绝边界
- 既有 `runtime_layout`、`deinit` 原文保持，分别消费 exact 布局与规划隐藏 deinit

主要依赖为入口 → preflight → routes → recipe validation → runtime layout → concrete types；
各层可直接引用更低层 helper，仅借根共享模型与签名查询。不存在新反向算法依赖。
原 crate 内生产消费者路径由窄重导出保留。仅 sibling tests 使用的两个 owner-argument
resolver 通过根 `cfg(test)` 重导出保留测试路径，不添加 lint allow 或为接线另造模块。

call route 不再混入 field graph 递归许可；有限失败前沿与正式实例上限的错误顺序更容易独立检查。
三个类型解析合同仍不同：preflight 可选查找、正式 canonical-only 解析、closed recipe 的
既有 template fallback。本批不因同名或形似而合并它们。

## 保全与验收边界

- 函数正文、完整类型、注释、属性、literal、调用/错误顺序以固定基底逐块核对
- 必要可见性变动逐项登记，不用忽略全部 token 或仅比较测试数量代替语义证据
- 旧 root imports 与相对路径逐项核解析目标；测试、fixture、cfg/ignore、Cargo/CI 输入保持
- frontend library 全身份、codegen library 全身份、Cargo metadata target 集合前后比较
- iteration 图/seed/incoming 的 93 项私有测试与 184 项 integration、planner 七域 48 项前测已通过
- 最终按新增生产影响面统一跑最近测试、下游 codegen/native、workspace check、严格 clippy、fmt、policy/docs/尺寸
- 不运行无关 parser 矩阵或 frontend 全量；不因文档提交或合并后的同树状态重复 Cargo

### 逐块保真与可回退提交

iteration 生产 commit `c6a480e8eb4d1019494b7b01cac6a569f0c5bb79`；unit planner 生产 commit
`1d21f137d5a61ec8b3ccf53c927e2c2bbf32cf7d`，后者 tree
`2dbddc77862ae4044015cba1b6f380f535552171`。各 commit 只包含本职责源码。

| 职责 | 原入口 → 新入口 | 私有子模块 PLOC | 保真证据 |
|---|---:|---|---|
| iteration | 1876 → 275 | graph374、state527、incoming766 | 38函数正文逐字节相同；45完整item仅28项已枚举可见性及3签名末尾逗号变化；30测试/回放文件不变 |
| unit planner | 2651 → 735 | routes798、concrete160、preflight408、validation649 | 35函数+1枚举逐字迁移；10保留顶层函数逐字相同；实际新根与子模块逆向重建原文件字节；215既有codegen文件不变 |

[iteration 逐项证据](evidence/iteration-production-migration.json)与
[只读复核器](evidence/check_iteration_production_migration.py)固定基底、逐项限制归一，
不忽略路径或算法 token；独立完整函数体比较另锁全部正文。unit planner 的[逐项证据](evidence/unit-planner-responsibility-equivalence.json)
与[只读复核器](evidence/unit-planner-responsibility-equivalence.py)核实际逆向重建及18项可见性变化。
737 个既有 Rust/Cargo/CI 文件 hash 保持，仅两原入口改变；新 Rust 文件恰为七个职责模块。
完整 frontend library187、codegen library770 身份前后相同，metadata 完整 JSON 字节相同、142 targets。
旧过滤器仍分别命中 iteration93 与 planner48，无新增 target、test、ignore 或依赖。

### 实际本地验收

Rust/Cargo1.96.0、LLVM/Clang21.1.8，x86_64 Linux；共享 target、`CARGO_INCREMENTAL=0`，
Cargo 严格串行，无 clean 或另建 target。Cargo 命令均 `--locked --offline`（fmt除外）。

| 命令/范围 | 实际结果 |
|---|---|
| frontend `--lib ownership_checking::checker::drop_planner::iteration` | 前后93 passed、0 failed/ignored、94 filtered；40私有合同+53独立实例回放 |
| frontend `--test ownership_iteration` | 前184/184，后纳入统一完整target；无ignore/filter |
| codegen `--lib unit_plan_tests` | 前后48 passed、0 failed/ignored、722 filtered，七域错误/recipe/layout身份不变 |
| frontend `--lib` + 明确列出的22 integration | library187 + integration641，共828 passed；各0 failed/ignored/filtered |
| frontend `--doc` | 12 passed |
| codegen、CLI、LSP 完整 package 测试 | codegen770+native compile2+doc4、CLI82、LSP45，全部0 failed/ignored/filtered |
| `cargo check --workspace --all-targets` | 通过 |
| `cargo clippy --workspace --all-targets -- -D warnings` | 通过 |
| `cargo fmt --all -- --check` | 通过 |
| Python policy tests | 102 passed |
| 尺寸 `--base 9ac49f3` | 731手写Rust、45超千行，通过；仅退休两项旧baseline，无新增例外 |

22 integration为 ownership_iteration/checking/closures/constants/construction/containers/
field_replace/nullable_when/primitives/rc/resource_deinit/structural/two_phase_borrows，
multifile_type_checking/ownership_checking/constant_facts/constant_ownership/two_phase_borrows，
owned_compilation_unit_view、const_owned_compilation_unit_view、basic_unit_ownership、guide_litmus。
命令、34次完整target执行（1743 passed）及每项身份hash记录于[统一验收证据](evidence/ownership-planning-milestone.json)；该数不包含前后窄测。

codegen 完整测试覆盖真实 native object/link/run、资源 drop/alloc/free、ELF/DWARF、输出原子性，
0255 的实际递归依赖护栏也扫描新 unit_plan 子树。不是只检查编译或 planner 数量。
初次下游窄测发现一个新增无用 import，已删除；最终严格clippy及测试无warning。
未运行macOS/远端CI、frontend全量、完整stage/Guide脚本（无关parser矩阵未纳入），不将
其中已选行为测试称为完整脚本通过。未修改的其他基线复用既有证据，不因文档提交重跑Cargo。

## 有界成本样本与审阅

先于两侧采样登记调查阈值：dependency-warm compile/link 中位耗时增长超过 max(25%,1秒)，
或峰值RSS增长超过 max(15%,128MiB)。同worktree/工具链分别仅touch对应根源码，前后各两次；
八次JSON artifact均证明只有目标crate的library test为nonfresh，其依赖保持fresh。

| 受控样本 | 前两次 | 后两次 |
|---|---|---|
| frontend compile/link 秒 | 24.597、24.748 | 26.029、25.139 |
| frontend 峰值RSS KiB | 2542432、2525464 | 2678312、2602408 |
| codegen compile/link 秒 | 15.285、15.139 | 14.925、14.829 |
| codegen 峰值RSS KiB | 1526772、1554528 | 1593080、1593164 |

耗时/RSS均未触发预登记调查阈值。这些小样本只排查明显退化，不证明性能等价或提速。
iteration 同filter执行单次前10.22/后9.95秒、planner前0.04/后0.05秒，不作统计性能结论。
未测干净冷构建、分离编译/链接成本、release benchmark或macOS成本。

两源码提交经独立review Approve：复核全部函数/类型/属性、字节逆向重建、解析目的地、
可见性和原消费者/测试保全，无阻断项。两个只读核验器实际通过，拒绝token/visibility/路径/
测试或证据篡改的有界反证也已执行；它们不是完整Rust解析器或行为/性能测试的替代。
最终文档与证据窄审已 Approve，实际日志、身份、八个成本artifact与未运行项均独立核对，无阻断项。
精确最终head的远端双宿主CI仍待，没有创建远端分支、PR或执行发布操作。

## 大文件处置原则与下一边界

本批基底的 47 个超千行文件是审阅清单，本批后为 45 个；不是 47 个待发 PR，也不是全部必须拆到 999 行的目标。
本批优先两个同时混合算法、状态推进和错误策略的生产热点，搭配已保全的测试域与中立 helper。
下一批按共同调用链与验证收益选完整责任，不按单文件滚动发 PR。

现有三个有界大场景例外继续保留：条件叶 phi、跨循环 File parent、兄弟 shared loan，
因为它们保护连续形成/逐边运输/清理 oracle；本批未触及其场景或回放 helper。
AST/typed/SSA 模型等较大声明集合不能仅按尺寸判定不合理；先审数据归属和消费者耦合，
没有证明责任错置时保留并报告现有欠账，不扩额度，也不伪造永久例外。

本里程碑验收的是 P2 两条生产边界和已有 P4 中立支撑首片。其余 P2 责任审阅、P4 逐域 parity
与共享内核、P5 current 教程/持续防漂移、0182 原合同仍有各自验收边界。
全计划结束后的外部附件审计继续后置，不能从本批行数变化宣布整体完成。
