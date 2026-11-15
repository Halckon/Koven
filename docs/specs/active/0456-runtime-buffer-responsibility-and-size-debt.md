# SPEC-0456：Runtime buffer 职责分离与历史尺寸额度退休

> **性质**：实施 Spec · **状态**：in-progress · **读取时机**：处理 runtime buffer 与历史尺寸欠账时 · **唯一真源**：本次有界切片；语言语义从属现行 Guide

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
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

- [ ] R1：方法正文/调用点机械保全；私有字段访问成立，可见性等价，无 wrapper。
- [ ] R2：runtime 根和新 buffer 文件均在1000行内；对应历史 baseline/exception 删除，尺寸门禁通过。
- [ ] R3：默认/feature 的原窄位宽、runtime、buffer/生成/argv/Map消费者定向验证；测试名称、source与断言不变。
- [ ] R4：fmt、适用 workspace check、双配置 all-target Clippy、docs、policy tests与空白通过；逻辑独立只读审查。
- [ ] R5：事实快照、真实验收与独立 PR/CI 来源同步；未运行、ignored及观察期限单独记录。

## 6. 实现约束

私有 `runtime/buffer.rs` 为同一 RuntimeAbi 增加原 inherent methods；不是新的服务对象或
类型×控制流 lowering。窄位宽测试继续通过根引入 RuntimeAbi，LLVM sibling 调用仍为原方法。
保留 signed Int 负数检查、逻辑长度桥、unsigned product overflow、signed pointer-index
物理上限、ZST/空 buffer sentinel、malloc OOM、成功块与 phi incoming 的既有顺序。

本次是无行为变化的职责重构：迁移前后运行既有测试，不伪造产品 RED；1011行尺寸为
明确的旧欠账基线。若发现实际行为缺陷，另留失败见证后再修，不顺手扩大范围。

## 7. 顺序实施计划

1. [ ] 冻结基底、方法、调用者、测试库存与定向基线。
2. [ ] 最小职责移动并删除本文件旧尺寸额度 → 机械审计与尺寸检查。
3. [ ] 同一直接消费者双配置验证、门禁、独立复核；有具体发现修正后复核。
4. [ ] 更新 Architecture/账本并提交独立 PR；官方 CI 与后续合入分别核验。

## 8. 提交计划

先提交合同与路由登记；实现、必要 policy 与真实结果独立提交。未取得必需结果时
不标 done、不归档、不把主干旧绿灯用作本片验收。原始日志置仓库外，不复制入库。

## 9. 未决问题

无；若必须改变布局、语义或调用者权限才能拆分则停止，不自行扩权。

## 10. 验证记录

初始输入 main `d39d0d54`，runtime 根1011行、narrow测试141行。
原尺寸 policy 为 runtime 登记 baseline1187及 exception1261；本片只退休该文件额度。
完整命令、来源、结果与未运行项在实际执行后追加，不预填通过。
