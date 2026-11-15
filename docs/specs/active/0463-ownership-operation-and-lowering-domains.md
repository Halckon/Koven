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

1. `apply_operation` 完整移入私有 `verify_ownership/operation.rs`：一次 instruction 的
   ownership 效果应用整体搬迁，不拆分 match、不改 arm 顺序。根保留 verify_ownership
   门面、entry/edge/exit、helpers、aliases/closure/range 子模块与测试加载。函数
   `pub(super)` 恢复原私有可达域，仅一处 `super::model::Instruction` 改为
   `crate::ssa::model::Instruction` 等价全路径。
2. `lower_construction` / `lower_rc_operation` / `validate_construction_root` 完整移入私有
   `lower_frontend/construction.rs`：构造与 Rc 操作 lowering 及其根校验整体搬迁，
   原方法签名与 `pub(in crate::ssa)` 可见域不变。
3. `lower_string_literal` / `lower_string_binary` / `lower_string_view` / `is_string_expression`
   完整移入私有 `lower_frontend/string.rs`：String 字面量、视图、二元与类型判定整体
   搬迁，仍由原方法名调用。

三片均保留原表达式求值顺序、fact 缺失错误、span/origin、所有权效果、cleanup 和
所有测试/fixture/FQN；无 wrapper、不扩可见域、不增 allow 或 clone。根与新文件均不超
1000，只删除对应 policy 登记，其他登记逐 JSON 相同。

## 2. 非目标

不动 `llvm/adapter.rs`：其 1777 行中约 999 行是单个 `lower_instruction` 分发函数，整体迁移
会让新文件自身超 1000，必须按 operation 家族重构分发，超出本批职责归位的机械边界，
另行立项。不导入未合 #427，不扩大 borrow/receiver/nullable 或 E2/D10 能力，不为凑数
删除 34 项合理保留，不把 baseline/exception 上限提高。

## 3. 节奏与验收

- [ ] R1：完整函数机械保全（仅等价路径/可见域），原根剩余代码与全部测试不变。
- [ ] R2：逐片默认定向GREEN→移动→GREEN，本地提交；不逐片push、不伪造RED。
- [ ] R3：与0464成组统一双配置组合、workspace check、双Clippy、fmt/docs/尺寸/policy。
- [ ] R4：独立完整精确提交区间审查，有finding修后复核；Architecture/账本同步后一次push同PR。
- [ ] R5：精确新头官方CI另验；保持active/Draft，无本轮合并授权不归档或合入main。

原始快照/日志/完整区间diff留仓库外。只报告实际执行的检查，不把编译、列举或mock单测
当执行成功，不借上一批CI替代当前候选。

## 4. 真实证据

待逐片记录。
