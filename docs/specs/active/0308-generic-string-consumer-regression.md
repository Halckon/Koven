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
| single/unit SSA | `cargo test --locked --offline -p lang-codegen --lib mixed_value_borrow_generic_ordinary_consumers -- --nocapture` | 2/2 通过，1444 filtered；fixture/测试字节与 a510c5be 完全相同 |
| single native | `native_tests::borrow_result_tests::mixed_value_borrow_single_native_matrix` exact | 1/1 通过，1445 filtered；26 个普通源码子例 |
| unit native | `native::unit_tests::borrow_result_tests::mixed_value_borrow_unit_native_matrix` exact | 1/1 通过，1445 filtered；26 个普通源码子例 |
| 静态与文档 | fmt、codegen 严格 clippy、docs、diff、尺寸 | 全通过；docs 661；1065 个手写 Rust 文件、38 项旧尺寸债务无新增例外 |
| 独立审查/双宿主 CI | 精确最终 head 与 PR 输入 | 待运行 |

本地运行范围本应仅为已审普通 source 路径；构造坏 IR、故障注入、校准、sanitizer、
aggregate 与全部 scripts/tests discover 留 CI。实际发生一次越出该边界的 stub 选择检查，
其 aggregate shell 调用与未执行的真实测试严格区分，见 §6；macOS 留 CI。

## 4. CI 消费者与提交顺序

既有两项 SSA 测试及两个 native 矩阵已引用同一 fixture；不增加 integration target。
`.github/workflows/ci.yml` 的 Ubuntu/macOS Targeted Tests 均经 `check_integration.sh`
→ `check_core.sh` → `cargo test --locked -p lang-codegen` 完整选择这些 lib 测试。
frontend 目标计数继续是 103，本片无需调整脚本或固定 count。

先单独提交本合同和库存/导航，再提交三文件测试变更；最后按实际证据更新账本。
原始日志/恢复材料留仓库外，远端 CI、归档和合并另行验收。

## 5. 本地测试交付检查点

测试提交 `a510c5be9f45bd0a9e706d70859bb010edd1b3f1`，tree
`a1ec81199f532c4298e508a18aaf85d7a8a863da`；前一独立合同提交 `5e958c28`。
仅三处测试输入/断言变化，生产源码、Guide、ABI、verifier 和 CI 脚本字节均未改变。
既有 `native_mixed_borrow_cases` 只经 `#[cfg(test)]` 模块编译，不新增生产数据。

Linux x86_64、Rust 1.96.0 / LLVM 21.1.8、普通 `/usr/bin/cc` 链接；两 native
矩阵各 26 子例，共 52 次真实 object/link/run。日志只逐名显示两个矩阵测试成功，
26 子例来自被执行循环及固定 case 列表，不冒称日志逐子例输出。直接 String 的
readThenReturn 返回 payload 逐字节输出，其余无可观察返回值的函数由 SSA owner/loan
断言覆盖，不把空输出说成释放计数证据。

静态命令：`cargo fmt --all -- --check` 与
`cargo clippy --locked --offline -p lang-codegen --all-targets -- -D warnings`；
尺寸检查显式 base 为 046ee221。已静审的
`python3 -m unittest scripts.tests.test_recovery_gates.CompositionContracts.test_selection_is_unique_and_bounded -v`
1/1 通过，103 frontend targets / 10 次 Cargo 命令选择保持。该检查实际调用 aggregate
shell，而 Cargo/Python 子调用由 stub 替代；这是本应留 CI 的检查误执行，详见 §6，
不能视作完整 CI 验收。没有执行全部 Python discover。

原始日志及真实 commit 恢复材料保存于交付恢复包，不入库。下一步为精确最终快照的
独立审查与 root 统一发布；本地成功不代替双宿主 PR CI、归档、merge 或 actual-main CI。

## 6. 本地执行范围纠正

2026-10-13 11:10:55 UTC 实际执行了一次 §5 所列 exact Python selection 测试，
结果为 1/1、0.030s。该项本应留 CI，却被误判为允许的普通 policy 检查。之前的
“aggregate 未运行”笼统交付表述不准确，本节纠正事实，不删除原始日志或掩盖执行。

实际调用链为 Python exact test → `invoke(fail="")` →
`bash scripts/check_integration.sh` → `check_core.sh` / `check_stage_integration.sh`。
这些 aggregate shell 确实执行；临时 PATH 中的 `cargo`、`python3` 是记录参数的 shell
stub，`FAIL_MATCH` 为空，未触发 stub 的 exit 37 故障分支。没有选择另一个
`test_each_phase_propagates_failure_and_stops_following_phases` 测试。

该调用链没有运行真实 Cargo suite、tutorial Python、故障注入、校准、sanitizer 或
bad IR；其结果仅描述 stub 下的命令选择，不作为合规本地验收依据，也不证明完整 CI 或这些真实
测试通过。真实
source/SSA/native 与静态门禁结果仍分别按 §3–5 的实际执行登记。临时 calls 文件随
TemporaryDirectory 清理，保存的原始 unittest 日志证明 exact 测试执行/通过，调用
边界由该提交未改变的脚本源码核实。本次只修账本并做 docs/hash/Git 恢复核验，
不重跑该检查或其他测试。
