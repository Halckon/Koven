# SPEC-0247：跨文件类型基线与恢复事实闭合

> **性质**：实施 Spec · **状态**：in-progress · **读取时机**：修复跨文件类型历史失败或消费恢复布局事实时 · **唯一真源**：本 Spec

| 字段 | 值 |
|---|---|
| 状态 | `in-progress` |
| Goal ID | `KOV-P2-247` |
| 所属 Phase | Phase 2，直接 Phase 3/4 消费者回归 |
| 语言规范 | [类型](../../guide/03-types-generics.md)、[控制流](../../guide/06-blocks-control-flow.md)、[lambda](../../guide/07-calls-lambdas-closures.md)、[常量交接](../../guide/05-declarations-callables.md#364-import分阶段交接与非目标) |
| 批准依据 | 用户持续推进演进计划，分阶段实施、验证、提交及“本地验证 → 草稿 PR → 双平台 CI”的站立授权 |
| 基线 | PR #13 已合并的 main `efc52b6993b60c0bdf5072ef1283c145479f7531` |
| 前置 Spec / ADR | 已启用 Guide v0.40；不新增语言或架构决定 |
| 阻塞项 | 无 |
| 影响范围 | unit runtime field layout recovery、multifile 类型断言、stage 门禁及当前事实 |
| 语言语义变更 | 否 |

## 1. Goal 与边界

完整 `multifile_type_checking` 的五项历史失败得到逐项因果闭合：真正的内部错误由最小生产修复消除，
过时断言迁移到已启用的规范，并用更强的正反事实证明迁移而非降级。
不扩展 place/ownership、generic resource、SSA/LLVM 能力，不修复独立的五项 editor corpus 失败。

## 2. 诊断与方案

| 原失败 | 根因与批准依据 | 最小纠正 |
|---|---|---|
| `deferred_explicit_constructor_type_arguments_publish_no_construction_fact` | signature 中未绑定 external 字段是 recovery Error；layout producer 将非 concrete field 错当 `MissingDeclarationSymbol` | 跳过该 owner 的完整 layout；保留无关完整 owner、真实诊断的全表失效与真正缺 identity 的内部错误 |
| `cross_file_when_diagnostics_cover_shape_order_coverage_and_branch_join` | Guide06 无 expected type 的其他已知类型 join 为 Any，Unit/Int 不再应 L0112 | 保留六项 shape/coverage/order 诊断，新增 result 与 when 的 Any 强断言 |
| `unit_lambda_diagnostics_stop_jumps_and_returns_at_callable_boundary` | Guide07 一元 expected 激活隐式 it，即使 body 未读取 | 原 wrongArity 改为显式零参数 `{ -> 1 }`，保留七条负向诊断；另验未使用 it 的成功参数/type/mode |
| `top_level_initializers_publish_stable_cross_file_symbol_and_expression_types` | Guide05 §36.4 含 const 必须独立 const-enabled capability | 普通 validate 明确拒绝，常量入口成功；原 typed facts 保留，补值/use/identity/逆序等价 |
| `companion_constant_initializers_publish_stable_ordinary_typed_facts` | 同一常量阶段能力隔离 | 保留完整普通 typed 断言，补同上两条入口正反验证 |

新增布局红测包含字段前缀不可发布、复合 poison、有效 owner 保留、本地 SymbolId 碰撞与输入置换。
签名中的未绑定类型仍为 Error，body 中仍为 Deferred；不借本次修复重写旧表示边界。
单文件 assignment branch 仍可能为 Deferred(Assignment)，不据 unit 已知 Unit 结果放宽该路径；
单/unit join parity 使用已知返回 Unit 的 callable，原 unit assignment 案例单独保留验证。

## 3. 验收账本

| 验收项 / 命令 | 实际结果 |
|---|---|
| `cargo test --locked -p lang-frontend --test multifile_type_checking`，未改 main | 99 passed / 5 failed；五个名称与上表相同，无 ignored；e6e1100 与 PR13 合并后 efc52b6 均独立复核 |
| 新增布局定向红测 | 2 failed / 1 passed；两个新 layout 用例均为 MissingDeclarationSymbol，语义 parity 正向先行通过 |
| 同一完整 `multifile_type_checking` | 107 passed / 0 failed / 0 ignored；原5项全部闭合 |
| 11 个直接 frontend suites（含完整 multifile） | 317 passed / 0 failed / 0 ignored |
| `cargo test --locked -p lang-frontend --lib` | 187 passed / 0 failed / 0 ignored |
| workspace all-targets `check` / `clippy -- -D warnings` / `cargo fmt --all -- --check` | 全通过，未加豁免 |
| `cargo test --locked -p lang-codegen` / `-p lang-cli` / `-p lang-lsp` | codegen 679 + 4 compile-fail doctests；CLI 66 / LSP 26，全部通过 |
| `recovery_owner_without_a_layout_cannot_become_executable_ssa` | 1 passed；完整 unit lowering 以 UnsupportedNode 拒绝 poison 字段，无生产 codegen 修改 |
| `bash scripts/check_stage_integration.sh` | 64 targets / 917 passed；本次将完整 multifile suite 纳入持续门禁 |
| `bash scripts/check_guide_litmus.sh` | 187 frontend + 14 codegen = 201 passed；另 2023 filtered 不计通过 |
| `python3 scripts/check_docs.py` / `python3 -m unittest discover -s scripts/tests -v` / `git diff --check` | 456 Markdown / 45 tests / whitespace 均通过 |
| Draft PR / 最终 exact-head macOS+Ubuntu CI | 待发布 |

默认不启动 frontend 全量。命令串行复用共享 target，`CARGO_INCREMENTAL=0`。
本地 Linux x86_64/glibc、Rust 1.96.0、LLVM/Clang 21.1.8；macOS 以实际 CI 留证。

## 4. 交付

1. [x] 主干复核与逐项诊断，明确四项迁移、一项真正生产错误。
2. [x] 红测 → 最小实现 → 同一测试和直接消费者闭合。
3. [x] 当前 Architecture/演进账本、stage 门禁与本地验收同步。
4. [ ] 独立草稿 PR、双平台 CI 全绿后归档；不自动合并或转 Ready。

历史 Spec 与归档结果保持不变，只更新当前 inventory。无新语义未决项。

## 5. 本地验证选择与保存

直接 frontend 命令为 `cargo test --locked -p lang-frontend --no-fail-fast`，逐项 `--test`：
`multifile_type_checking`、`multifile_type_signatures`、`multifile_type_signature_determinism`、
`multifile_type_signature_provenance`、`multifile_type_member_graph`、`multifile_type_capability_graph`、
`type_checking`、`type_callable`、`multifile_constant_facts`、`multifile_ownership_checking`、
`ownership_construction`。check/clippy 都使用 `--locked --workspace --all-targets`，不隐式启动测试。

首个实现检查点 `abd3a07` 及完整 Git bundle 已验证；以上门禁均在该源码上执行，后续只更新验收文档。
同一事实链可依次通过 typed recovery、ownership 与 reachability planner；这不等于可执行存储能力。
新增下游测试通过完整 SSA lowering 核验 Error 字段 fail-closed，未为测试修改 planner 的既有职责。
复合布局 poison 的六种 source 形态均通过；单/unit 无诊断恢复对照限定 direct Opaque，
不声称两路径的所有 composite capability recovery 表示已统一。
