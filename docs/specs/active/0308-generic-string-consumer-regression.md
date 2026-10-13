# SPEC-0308: 普通 own 泛型直接 String 消费者持久回归

> **性质**：变更合同 · **状态**：in-progress · **读取时机**：验收普通 own 泛型 String 消费者回归时 · **唯一真源**：本 Spec

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P4-0308` |
| 所属 Phase | Phase 4 测试覆盖 |
| 语言规范 | [Guide10](../../guide/10-ownership-borrowing-drop.md)、[Guide13](../../guide/13-program-runtime-standard-library.md) |
| 批准依据 | 2026-10-13 持续现行里程碑；独立补齐直接 String 的已有普通泛型消费者回归 |
| 前置 Spec | [SPEC-0306](../../archive/specs/0306-mixed-value-borrow-result.md) |
| 前置 ADR | [ADR-0016](../../adr/accepted/0016-interprocedural-borrow-abi.md)、[ADR-0018](../../adr/accepted/0018-string-owner-runtime-abi.md) |
| 阻塞项 | 本片独立审查、远端双宿主 CI 与发布待验收 |
| 影响范围 | 既有普通 source fixture、SSA 断言与 native stdout oracle；生产代码零改 |
| 语言语义变更 | 否；无新表示、ABI 或 verifier 行为 |

## 1. 唯一 Goal 与真实输入

保留普通泛型 `unused`、`forward`、`readThenReturn`、`readTwice` 的 Int/Resource 实例，
增加直接 String 实例，固定既有 own Value delivery、非消费辅助 Borrow、唯一最终 owner
清理和返回 payload。fixture 不包含 borrow-return；这不是新的语言能力或生产修复。

基于 actual main `046ee221c29f8a84d2d9fb56c945751f07e7d6d8`，tree
`2147d7562edde1c2abb889d6f1c003db8c256b8f`，有序 parents 为 `557e3684`、`86da3104`，
建立 `feature/spec-0308`。真实 commit 字节从已保存 API JSON 恢复并核 SHA，未伪造 merge。
[base main CI](https://github.com/Halckon/Koven/actions/runs/38387165217)冻结时仍运行中，
不称完整闭环，也不代替本片未来 CI。

准备阶段普通源码 probe 已绿；原草稿仅 apply-check，没有运行私有 SSA 断言。
本片记录“覆盖缺失 → 新增绿测”，不伪造语言失败或红到绿生产修复。

## 2. 冻结验收与非目标

- 每个函数存在 Int、Resource、直接 String 三个实例；String 对应真实 `StringOwner`。
- own T 保持 Value entry；MoveOnly 最终 owner 恰一次 Drop；forward 恰一次转交。
- readThenReturn/readTwice 的 observe 借用原 owner，结束调用 loan 后仍保留原 binding。
- single 与真正跨文件 unit 均经实际 frontend、原版 SSA verifier、LLVM/object/native；
  stdout 精确包含 `returned-string`，stderr 为空。普通 stdout 不证明精确 allocation/free 次数。
- 不修改 Guide、frontend、生产 lowering、model/verifier、LLVM、allocator；不引入依赖。
- unit Borrow value-class direct fields 与 single 普通 method 是独立后继，不属于本 Goal，
  不以通用 Borrow ABI 的间接关系宣称覆盖它们。

## 3. 唯一验收账本

| 验收项 | 精确选择 | 当前结果 |
|---|---|---|
| single/unit SSA | `cargo test --locked --offline -p lang-codegen --lib mixed_value_borrow_generic_ordinary_consumers -- --nocapture` | 草稿 2/2 通过；最终提交身份与复核待登记 |
| single native | `native_tests::borrow_result_tests::mixed_value_borrow_single_native_matrix` exact | 待运行 |
| unit native | `native::unit_tests::borrow_result_tests::mixed_value_borrow_unit_native_matrix` exact | 待运行 |
| 静态与文档 | fmt、codegen 严格 clippy、docs、diff、尺寸 | 待运行 |
| 独立审查/双宿主 CI | 精确最终 head 与 PR 输入 | 待运行 |

本地仅执行已审普通 source 路径；不运行构造坏 IR、故障注入、校准、sanitizer、aggregate
或全部 scripts/tests discover。未运行项保持可见，macOS 留 CI。

## 4. CI 消费者与提交顺序

既有两项 SSA 测试及两个 native 矩阵已引用同一 fixture；不增加 integration target。
`.github/workflows/ci.yml` 的 Ubuntu/macOS Targeted Tests 均经 `check_integration.sh`
→ `check_core.sh` → `cargo test --locked -p lang-codegen` 完整选择这些 lib 测试。
frontend 目标计数继续是 103，本片无需调整脚本或固定 count。

先单独提交本合同和库存/导航，再提交三文件测试变更；最后按实际证据更新账本。
原始日志/恢复材料留仓库外，远端 CI、归档和合并另行验收。
