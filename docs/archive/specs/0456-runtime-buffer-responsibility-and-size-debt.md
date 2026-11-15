# SPEC-0456：Runtime buffer 职责分离与历史尺寸额度退休

> **性质**：实施 Spec · **状态**：done · **读取时机**：处理 runtime buffer 与历史尺寸欠账时 · **唯一真源**：本次有界切片；语言语义从属现行 Guide

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-0456` |
| 所属 Phase | Phase 4，已验证 SSA 到 LLVM/runtime 边界 |
| 语言规范 | 现行 [Guide v0.47](../../guide/README.md) |
| 批准依据 | 用户要求完善历史欠账，并从远端最新 main 新建分支 |
| 前置 Spec | 已合入的 SPEC-0451–0453 修订交付 |
| 前置 ADR | [ADR-0008](../../adr/accepted/0008-internal-value-and-allocation-abi.md) |
| 阻塞项 | 无；仅职责整理，不新增语言能力 |
| 影响范围 | `llvm/runtime.rs`、直接 buffer 子模块、尺寸 policy、最近测试与事实快照 |
| 语言语义变更 | 否 |

## 1. Goal

保持 RuntimeAbi 的原调用、LLVM IR/ABI、检查顺序与失败文案，按 buffer 分配职责拆出完整方法，
将 runtime 根从1011行降至1000以内，并删除该文件的历史 baseline 与 exception 额度。

## 2. 输入与边界

从重新 fetch 的远端 main `d39d0d54531537d48d1f5948adeaf2885ee7de74` 新建
`fix/spec-0456`，独立工作树 `/tmp/koven-0456`，不从 #423 分支派生、不导入未合 #427。
0451–0453 已批准的34项保留是分类基线，不是必须全部删除的欠账。

本次选择与该清单 `llvm/runtime/narrow_tests.rs` 关联的真实父模块尺寸问题。
原测试既验证窄位宽 buffer/length 桥，也验证 RuntimeAbi 初始化的 pointer index field；
保留原文件和 `llvm::runtime::narrow_tests::*` 名称，不能靠搬测试使统计好看。

## 3. 范围

- `container_int_to_size` 与 `allocate_buffer` 按单一 buffer 职责移入私有直接子模块。
- 保持整个方法正文；只调整模块接线、必要 import 与等价可见性拼写。
- 旧根的 `pub(super)` 实际允许 LLVM 域调用；移入子模块后用 `pub(in crate::llvm)`
  保持同一作用域，不扩大 API，不加 wrapper 或 re-export。
- 根 RuntimeAbi 数据、初始化、sentinel、abort 与 drop 分派不迁移、不新增状态。
- runtime 根低于尺寸软上限后，同批删除对应 baseline 与 exception；其他额度不动。

## 4. 非目标

不改 LLVM operation、目标布局、语言准入、owner/loan facts、allocator ABI、错误分类、
cleanup 或调用顺序。不迁移其余33个保留模块，不把所有历史跨域打包纳入本片，
不删除 narrow 测试，不放宽1000行规则，不增加依赖或 workspace member。

## 5. 验收标准

- [x] R1：方法正文/调用点机械保全；私有字段访问成立，可见性等价，无 wrapper。
- [x] R2：runtime 根和新 buffer 文件均在1000行内；对应历史 baseline/exception 删除，尺寸门禁通过。
- [x] R3：默认/feature 的原窄位宽、runtime、buffer/生成/argv/Map消费者定向验证；测试名称、source与断言不变。
- [x] R4：fmt、适用 workspace check、双配置 all-target Clippy、docs、policy tests与空白通过；逻辑独立只读审查。
- [x] R5：事实快照、真实验收与独立 PR/CI 来源同步；未运行、ignored及观察期限单独记录。

## 6. 实现约束

私有 `runtime/buffer.rs` 为同一 RuntimeAbi 增加原 inherent methods；不是新的服务对象或
类型×控制流 lowering。窄位宽测试继续通过根引入 RuntimeAbi，LLVM sibling 调用仍为原方法。
保留 signed Int 负数检查、逻辑长度桥、unsigned product overflow、signed pointer-index
物理上限、ZST/空 buffer sentinel、malloc OOM、成功块与 phi incoming 的既有顺序。

本次是无行为变化的职责重构：迁移前后运行既有测试，不伪造产品 RED；1011行尺寸为
明确的旧欠账基线。若发现实际行为缺陷，另留失败见证后再修，不顺手扩大范围。

## 7. 顺序实施计划

1. [x] 冻结基底、方法、调用者、测试库存与定向基线。
2. [x] 最小职责移动并删除本文件旧尺寸额度 → 机械审计与尺寸检查。
3. [x] 同一直接消费者双配置验证、门禁、独立复核；有具体发现修正后复核。
4. [x] 更新 Architecture/账本并提交独立 PR；官方 CI 与后续合入分别核验。

## 8. 提交计划

先提交合同与路由登记；实现、必要 policy 与真实结果独立提交。未取得必需结果时
不标 done、不归档、不把主干旧绿灯用作本片验收。原始日志置仓库外，不复制入库。

## 9. 未决问题

无；若必须改变布局、语义或调用者权限才能拆分则停止，不自行扩权。

## 10. 验证记录

初始输入 main `d39d0d54`，runtime 根1011行、narrow测试141行。
原尺寸 policy 为 runtime 登记 baseline1187及 exception1261；本片只退休该文件额度。
完整命令、来源、结果与未运行项按以下实际执行记录，不预填官方 CI。

| 验证 | 实际结果与范围 |
|---|---|
| 迁移前 | 默认/feature 的 `llvm::runtime::narrow_tests::`、`llvm::runtime_tests::`、`llvm::container_tests::`、`llvm::object_tests::borrowed_generate_tests::` 各 **24/0/0**，退出0；均为实际既有用例，不改源码或断言 |
| 机械保全 | 两个完整方法除等价可见性拼写外字节相同；原 narrow 文件141行逐字节不变，两个 libtest 名称/挂载不变；未改其他调用点或新增wrapper |
| 迁移后最终 | 同一24项，加 `borrowed_container_length_builds_links_and_runs_with_int_result` 与 `native_map_owned_remove_strings_and_inline_values_have_conditional_drops` 两项实际 native，两配置各 **26/0/0**，退出0，test-threads=1；不与基线重复累计 |
| 首轮 Clippy | 默认 exit101：移走方法后遗漏清理根的 IntValue import；独立审查发现同一具体问题。删除本次产生的 unused import，不添加 allow |
| 最终 Rust 门禁 | 两配置 `cargo clippy --locked -p lang-codegen --all-targets [--features loan-observation-test-support] -- -D warnings`、`cargo check --locked --workspace --all-targets`、fmt 均退出0 |
| policy | 47项 policy tests通过；JSON机械核对仅删除runtime的baseline/exception，其他值完全一致，无阈值提高 |
| 实际尺寸 | 首次移动根813行；清理unused import并rustfmt后根 **811**、buffer **211**、narrow **141**。base tracked树37个超千行，最终尺寸门禁36个；只清除本文件欠账，不宣称清除全部历史债 |

独立首轮复核确认调用域、私有字段、LLVM检查/abort/ZST/phi顺序及policy退休合法；
发现的 IntValue 导入问题已修复，并重新运行全部最终双配置选择及 Clippy。
不运行完整 LLVM（含此前慢 synthetic-ZST 用例）、frontend 全量或本地 CI-only fault；
compile/list不当执行，官方完整候选 CI 与本地结果另行归因。34项保留清单仍为历史冻结基线，
本次未删除其中任何文件；增加单职责buffer文件不是数量优化或新的例外额度。
docs（953 Markdown）、尺寸、空白门禁通过；审查P1修复后已独立复核闭合。官方PR/CI仍待发布核验。

## 最终交付验收（PR #433）

用户授权先合入再从main开展后续。最终代码头 `914797c2c02dd3787ded8ff1255b53d4d3bc9e09`
普通整合main `fdf5766b2b2707fe29c7ab7f7b8445b98ef329ad` 后，双配置组合各 **663/0/0**；
workspace all-target check/双Clippy、fmt、policy47、docs962、尺寸/空白通过，历史超限
**37→21**（本PR各批次累计，不将此总数归因单一Spec）。

[精确代码PR CI 43699903034](https://github.com/Halckon/Koven/actions/runs/43699903034)
completed/success：双宿主Check/Clippy/Tests、Preview producer与独立consumer、最终汇总
实际成功；Editor skipped不计执行。此前c2328e14的PR为43692407513，43692403094是push，
两者不能替代本集成头。独立交付复核完整实际两个diff（分发2861行、主干增量555行），
无具体问题；历史0456–0464独立审查保持各自范围，不补成原188路径全量正文审计。

本页旧pending/未完成文字是当时检查点，以上最终记录闭合本次合同勾选；原失败、观察期限
及本地未跑项保留，不把CI或归档倒写成所有本地检查已跑。归档候选CI和实际合并另行核验，
不代表E3终态、清空34合理保留或扩大语言准入。
