# SPEC-0325: 简单变量普通赋值类型事实

> **性质**：有界类型修复 · **状态**：draft · **读取时机**：实现 p42 类型前置与所有权诊断时 · **唯一真源**：本 Spec；语言合同见 Guide

| 字段 | 值 |
|---|---|
| 状态 | draft |
| Goal ID | `KOV-P2-0325` |
| 所属 Phase | Phase 2；final admission 仍先于 Phase 3 ownership |
| 语言规范 | [Guide v0.44 简单赋值](../../../guide/03-types-generics.md#简单变量赋值与整数-range-身份) |
| 批准依据 | 2026-10-15 用户批准 B 最小合同，随后明确启用 v0.44 及四组细则 |
| 基线 | Guide 分支 `be3264bc7143fdddb160ce77039fb902694d0d77`；其 actual main 祖先 `fcc5cc02c5f8b78b71dd703f864e04ad98c2a678` |
| 分支依赖 | `fix/spec-0325-v044` stacked 在 Guide 启用提交上；Guide PR 尚未合并，发布须先完成其主干交付 |
| 前置 Spec | [0319](../../../archive/specs/0319-final-type-admission-diagnostics.md)、[0323](../../../archive/specs/0323-final-admission-static-recovery.md) |
| 关联实现 | [0218 unit 赋值事实](../../../archive/specs/0218-compilation-unit-assignment-facts.md) |
| 语言语义变更 | 无新增合同；实现当前 Guide B，其他 A/C/D/E/F 不在本片 |

## 1. 范围与顺序

已解析简单变量普通 `=` 按声明 storage 类型检查 RHS，成功结果 Unit；RHS 后 kill 目标流事实。
分组目标透明。捕获、可变性、loan、move 继续按既有 ownership 阶段检查；不绕 final admission。
不扩大 field/index/compound，不实现 while 新收窄，不混入其他 v0.44 功能。
Unit 允许兼容值上下文；expected Int 的 lambda 尾赋值须拒绝，不能全局改成 statement。

原 p42 保存原字节，类型前置完成后仍须 L0135 拒绝捕获变量赋值，不是开放 mutable capture。
旧分支 `f5fc15759b723a6f86f90098752c81af29fd06df` 保留当时未启用/未执行历史；
本分支复用其普通源码测试并按新规范更新 Spec，未重写旧分支。

## 2. 定向验收

测试 target `final_type_admission`，模块 `simple_assignment`；每项均真实 single/unit-basic 成对：

1. `original_p42_capture_reaches_ownership`：原 p42，Unit 事实后准确 L0135。
2. `captured_and_local_tail_assignments_keep_ownership_boundary`：capture 拒绝/local var 接受。
3. `expected_non_unit_lambda_rejects_assignment_result`：Int expected 拒 Unit，保留 L0084。
4. `invalid_rhs_preserves_storage_type_diagnostic`：storage 不兼容 RHS，L0084/Span/完整诊断保留。
5. `grouped_variable_matches_plain_assignment`：分组透明。
6. `compound_assignment_keeps_deferred_boundary`：保留既有 Deferred/L0165 边界。
7. `nullable_storage_rhs_clears_target_refinement`：窄化后按 nullable storage 接受 null，随后读取 nullable；
   另以 `val after: Int = current` 精确拒绝，证明旧非空事实未残留。
8. `field_assignment_retains_existing_entry_difference`：仅保留已有 single/unit 差异，不扩 field。

helper 只普通源码 lex/parse/names/raw types/complete；complete 成功才 ownership，unit 用 basic validate。
不修改 facts，不构造 IR，不运行 native/附件批次。p42 SHA256：
`6d99f070206795274701749b4a6cd475c0709882bb8dfb83a567e33fadf58137`。

固定 head/tree/input SHA、完整 exact argv 与 runner 经独审后串行红测；失败保留真实 raw。
每条命令 `cargo test --locked --offline -p lang-frontend --test final_type_admission simple_assignment::NAME -- --exact --nocapture --test-threads=1`。
编译/依赖/非单命中停报；行为红收齐后才最小实现，最终共享普通回归与工程门禁另冻结。

## 3. 当前结果

红测输入 `620782607dd01ca04827e222e2cda2d8f1ea65a4`，2026-10-15 串行8 exact各实际1命中，
16输入前后SHA一致，6行为红/2边界绿，无编译或API基础失败。p42 single仍L0165，
unit Unit后L0135；expected Int及非法RHS unit已有L0084，single未完成；nullable single
缺storage expected报L0083，unit按storage成功且后续Nullable。compound和field raw对照通过。
field真实single complete成功→L0135 receiver，unit complete成功→L0134 immutable，随后补锁此边界。
多case首断言红后未到达的local/group/nullable负例不称已执行。仓外原始收据保留在
`spec0325-evidence/red-receipt.json`；固定源码与选择可由该提交及上列exact重现。
实现准入后再登记 active；native/资源与完整工程门禁尚未运行。
