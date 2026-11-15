# SPEC-0463：Ownership operation 与 lowering 领域职责归位

> **性质**：实施 Spec · **状态**：in-progress · **读取时机**：本批 ownership operation 与 legacy lowering 职责整理时 · **唯一真源**：本批范围与验收，语言语义从属 Guide

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P1-0463` |
| 所属 Phase | Phase 4，typed SSA verifier与lowering |
| 语言规范 | 现行 [Guide](../../guide/README.md)，无语义变更 |
| 批准依据 | 用户要求继续下一批；沿fix/spec-0456与PR #433，不另开PR |
| 前置 Spec | [0462](0462-refine-call-and-control-merge-responsibility.md)，输入4df265b1（CI 43681113210 success） |
| 前置 ADR | [ADR-0006](../../adr/accepted/0006-typed-ssa-block-parameters.md)、[ADR-0016](../../adr/accepted/0016-interprocedural-borrow-abi.md) |
| 影响范围 | ssa/verify_ownership.rs、ssa/lower_frontend.rs及私有子模块、对应policy |
| 语言语义变更 | 否 |
| 阻塞项 | 无 |

## 1. 三片完整职责

1. `apply_operation` 完整移入私有 `verify_ownership/operation.rs`：一次 instruction 的 ownership
   效果应用整体搬迁，不拆 match、不改 arm 顺序；原 `#[allow(clippy::too_many_arguments)]` 随函数。
   根保留门面、entry/edge/exit、helpers、aliases/closure/range 与测试加载；函数 `pub(super)`
   恢复原私有可达域，仅一处 `super::model::Instruction` 改为 `crate::ssa::model::Instruction`。
2. `lower_construction`、`lower_rc_operation`、`validate_construction_root` 完整移入私有
   `lower_frontend/construction.rs`，原方法名、`pub(in crate::ssa)` 有效域和调用点不变；仅一处
   `super::model::PlaceAccess` 改为 `crate::ssa::model::PlaceAccess` 等价全路径。
3. `lower_string_literal`、`lower_string_binary`、`lower_string_view`、`is_string_expression`
   完整移入私有 `lower_frontend/string.rs`，测试与外部 FQN 不变。

## 2. 非目标

不动 `llvm/adapter.rs`：1777 行中约 999 行是单个 `lower_instruction` 分发函数，整体迁移会让
新文件自身超 1000，必须按 operation 家族重构分发，超出本批机械边界，另行立项。不导入未合
#427，不扩大 borrow/receiver/nullable 或 E2/D10 能力，不为凑数删除合理保留模块。

## 3. 验收

- [x] R1：三片完整函数机械保全（仅等价路径/可见域），原根剩余代码与全部测试不变。
- [x] R2：逐片默认定向GREEN→移动→GREEN，本地提交；未逐片push、未伪造RED。
- [x] R3：与0464成组统一双配置组合、workspace check、双Clippy、fmt/docs/尺寸/policy。
- [x] R4：独立完整精确提交区间审查；Architecture/账本后一次push同PR。
- [ ] R5：精确新头官方CI另验；保持active/Draft，无本轮合并授权不归档或合入main。

## 4. 真实证据

合同 d1133ba1，三片先默认定向、通过后本地提交（未逐片push）：

| 切片 | 真实执行 | 尺寸 |
|---|---|---|
| ownership operation | 81/0/0（verify11、ownership16、borrow_result12、scalar12、nullable12、aggregate8、container10），前后逐名相同 | 1857→905，新 operation 959 |
| construction/Rc | 333/0/0（lower_frontend287、while_flow46），前后逐名相同 | 1321→1012，新 construction 315 |
| String 表达式 | 同 333/0/0 选择，前后逐名相同 | 1012→919，新 string 100 |

机械核对：三个完整块与剩余根除上述两处等价全路径与空白外完全相同；唯一
`too_many_arguments` allow 随原函数移动，不是新增；全部测试/fixture/FQN 未改；policy 只删除
`verify_ownership.rs`、`lower_frontend.rs` 两项 baseline 与两项 exception，其他登记逐 JSON 相同。

真实失败保留：ownership 首次编译报 E0603（新 child 内函数仍私有），补 `pub(super)` 恢复原域后
81 项通过；construction 首次报 E0433（child 中 `super::model` 多了一层），改等价全路径后通过；
另有一次 policy 快照被先前失败脚本覆盖导致审计误报，已从 git HEAD 重建快照复验，未改产品。

组末与0464同树：双配置各 **658/0/0**；workspace all-target check/Clippy、feature codegen
all-target Clippy、fmt、47项policy tests、docs961/尺寸/空白通过。尺寸 **25→22** 历史超限
（本 Spec 两根 25→23），未提高额度。rustfmt 只重整空行与模块声明顺序（`mod string;`），
之后重跑默认组合仍 **658/0/0**。

原始日志/快照/机械脚本与完整区间审查留仓库外。未跑本地完整 frontend/codegen/LLVM 慢组、
真实 generated-owner 校准、CI-only faults 或 sanitizer 全套；不把编译/list 当执行。
