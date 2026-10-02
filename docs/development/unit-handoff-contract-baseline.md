# 普通 unit 交接合同测试基线

> **性质**：有界验收账本 · **状态**：current · **读取时机**：实施 P3a 普通 owned-unit 交接或复核其回归时 · **唯一真源**：本页记录测试范围与实际验证；目标设计见[整体计划 §7](engineering-governance-plan.md#p3-封闭交接与共享编排-l-分两片)

## 范围与固定基线

2026-10-02 从 `main b36d5040ef7cafaa86e2b0c6a42cc1d2ab49691d` 独立建立合同测试片。
这里只补已有普通 native 入口的身份/输出合同证据，没有实现封闭 view，没有改变 checker、
公开 native 签名、能力边界、index 次数或 production visibility；不据本片宣称 P3a/P2 完成。

实现位于 [unit_handoff_contract_tests.rs](../../crates/lang-codegen/src/native/unit_handoff_contract_tests.rs)，
通过既有 `cfg(test)` 的 `native::unit_tests::handoff_contracts` 加载。普通 `SourceMap` 与
`ParsedFile` 不实现 Clone；成功对照沿用相同借用，不把同文本的新来源当合法 clone。

为不向超限入口堆正文，将原 `TestDirectory` 与其目录序列 counter 单独迁到
[unit_test_directory.rs](../../crates/lang-codegen/src/native/unit_test_directory.rs)。函数体与初始化
原样保留，仅添加 test-module 内 `pub(super)` 可见性；原有测试、fixture 和断言没有搬迁。
入口 2074→2050 行，新 domain 525 行，目录 helper 31 行；policy 仅收紧原入口额度，无新例外。
此私有 helper 的全部 `native::unit_tests` 消费者另行执行，不仅以新测试代替旧覆盖。

## 八项新测试与精确 oracle

下列测试名前缀均为 `native::unit_tests::handoff_contracts::`：

| 测试末段 | 唯一替换/成功对照 |
|---|---|
| `rejects_foreign_sources_before_output` | 仅替换 SourceMap，另外五项保持原分析链 |
| `rejects_changed_inputs_before_output` | 同 source/parsed，仅改 root identity；另有 duplicate、missing inputs |
| `rejects_fresh_names_before_output` | 同 inputs 的 fresh resolve，index 相等但 provenance 不同 |
| `rejects_foreign_environment_before_output` | 独立 standard environment；合法 environment clone 不作负例 |
| `rejects_fresh_typed_before_output` | 同 N0/E0 得 T1，先证明 T1 仍兼容原前置输入；保留 O0 |
| `rejects_foreign_owned_before_output` | fresh T1 派生 O1，仅换 O0；先证明 O1 兼容 T1 且不兼容 T0 |
| `accepts_cloned_chain_permuted_inputs_and_rechecked_ownership` | 四项分别 clone、全 clone＋重建等价 inputs、重排，以及同 T0 重新 ownership |
| `identity_rejection_never_reserves_sibling_output` | current_exe exact 子进程隔离已有 private counter，不引入生产 hook/API |

六维共八个必要负例，每例覆盖合法/非法 entry × 不存在/已有输出四格，再补不存在 parent，
合计40次真实 native 拒绝。每次精确断言 `MismatchedAnalysis`、`span == None`，已有 bytes
保全、不创建新目标、目录 entry 集合不变、无关文件 bytes 保全。匹配链先证明正常 entry
可生成真实 object，非法 shape 则为 `InvalidEntry` 且有 span，确认身份优先级。

reserve 证明不能仅靠“没有残留临时文件”。独立子进程只执行一个 exact 测试，分别读取现有
`NEXT_UNIT_OBJECT_TEMPORARY` 前后值：40次失配 delta 均0，六次合法成功各1，匹配链非法
entry 也为0；初值0、终值6。父进程普通并行 suite 不读取这个共享 counter，避免竞态假证。

八个成功组合都重新 lower/verify SSA 并精确比较 rendered SSA 和 entry identity，再真实 emit、
核 object machine/格式和 bytes 一致、link/run：stdout 为 `handoff-界\n`、stderr 空、exit code 0。
clone facts 相等且保持 analysis identity；同 T0 重做 ownership 的 facts 相等、owner identity
不同但仍可交接。后者不能因“并非同次 ownership pass”而收紧为拒绝合同。

## 本地验证

宿主为 x86_64 Linux + glibc，Rust/Cargo 1.96.0，native LLVM/Clang 21.1.8。
所有 Cargo 命令串行、同一 shared target、`--locked --offline`；默认 libtest 并行不变。

| 验证 | 实际结果 |
|---|---|
| helper 搬迁后既有 N2 exact | 1 passed / 0 ignored |
| 新 handoff domain | 8 passed / 0 ignored |
| 全部 `native::unit_tests`（含新八项） | 92 passed / 0 ignored |
| 下表既有基线 | 24 条 exact，各1 passed / 0 ignored |
| basic/const compile-fail | 7 passed / 0 ignored，五条过滤器分别2/2/1/1/1 |
| fmt / workspace all-targets check / codegen all-targets strict Clippy | 全部通过（Clippy `-D warnings`） |
| docs / 全部 Python policy / Rust size / diff | 468 Markdown、96 policy；660手写/49超限/0生成物，均通过 |

完整 codegen libtest 清单为738项（原730＋新8），Cargo metadata仍为129 targets；
Cargo配置、workspace依赖、workflow与平台ignore均未改变。

首轮新测试编译曾引用 private `ssa::render` 路径而失败；改用已有 crate-private
`ssa::render_program` 门面后通过。没有扩大可见性，也没有借此改变生产实现。
本片是已存在合同的补测，没有宣称发现/修复生产 bug，未伪造生产行为红→绿。

### 既有24条身份基线

运行形态：`cargo test --locked --offline -p PACKAGE TARGET FULL_NAME -- --exact`。
以下 `F`、`C1–C6` 为 `lang-frontend`，`S`、`N`、`C7–C8` 为 `lang-codegen`，`H` 为 `lang-cli`。
源码归属以对应 target 或完整模块路径为准；这些名字没有在本片更改。

| ID | TARGET | FULL_NAME |
|---|---|---|
| F1 | `--test compilation_unit_index` | `invalid_unit_inputs_fail_before_indexing` |
| F2 | `--test compilation_unit_index` | `canonical_identity_is_input_order_independent_and_merges_roots_by_package` |
| F3 | `--test multifile_type_signature_provenance` | `signature_provenance_rejects_structurally_equal_foreign_analyses` |
| F4 | `--test multifile_type_signature_provenance` | `signature_provenance_rejects_structurally_equal_foreign_inputs` |
| F5 | `--lib` | `type_checking::compilation_unit::bodies::tests::product_queries_use_the_unit_type_space_and_preserve_analysis_identity` |
| F6 | `--lib` | `type_checking::compilation_unit::bodies::tests::body_input_gate_rejects_foreign_names_environment_inputs_and_signatures` |
| F7 | `--test multifile_ownership_checking` | `unit_parameter_bindings_cover_cross_file_member_and_lambda_modes` |
| F8 | `--test multifile_ownership_checking` | `unit_ownership_rejects_mixed_analysis_and_duplicate_inputs` |
| S1 | `--lib` | `ssa::unit_plan_tests::entry_identity::rejects_non_callable_entries_and_foreign_ownership_products` |
| S2 | `--lib` | `ssa::unit_lower_tests::lowers_cross_package_generic_alias_call_to_deterministic_verified_ssa` |
| S3 | `--lib` | `ssa::unit_lower_tests::moves_a_string_across_files_and_drops_the_callee_owner_once` |
| N1 | `--lib` | `native::unit_tests::unit_object_failures_preserve_targets_and_cleanup_sibling_temporary` |
| N2 | `--lib` | `native::unit_tests::unit_object_atomically_replaces_links_and_runs_across_packages` |
| C1 | `--test multifile_constant_facts` | `constant_facts_are_exact_and_separate_from_base_validation` |
| C2 | `--test multifile_constant_facts` | `unused_constants_also_require_the_separate_capability` |
| C3 | `--test multifile_constant_ownership` | `constant_owned_capability_preserves_cross_file_plans_and_cannot_reopen_base_validation` |
| C4 | `--test multifile_constant_ownership` | `constant_entry_retains_its_boundary_even_without_constant_declarations` |
| C5 | `--test multifile_constant_ownership` | `constant_entry_rejects_mixed_inputs_names_environment_and_typed_owner` |
| C6 | `--test multifile_constant_ownership` | `errors_and_deferred_ownership_never_publish_constant_owned_capability` |
| C7 | `--lib` | `native::unit_tests::constants::constant_object_runs_deterministically_and_preserves_output_on_failure` |
| C8 | `--lib` | `native::unit_tests::constants::owners::constant_and_literal_string_owners_have_matching_runtime_cleanup` |
| N3 | `--lib` | `native::unit_tests::non_null_assertion_tests::unit_non_null_assertion_pending_temporary_is_freed_after_control_flow_call` |
| H1 | `--bin kovenc` | `project_command::tests::linker_failure_does_not_publish_and_artifact_drop_cleans_siblings` |
| H2 | `--bin kovenc` | `project_command::tests::atomic_commit_never_replaces_a_racing_final_output` |

compile-fail 命令依次为：

```sh
cargo test --locked --offline -p lang-codegen --doc native::emit_native_unit_object
cargo test --locked --offline -p lang-codegen --doc native::emit_native_constant_unit_object
cargo test --locked --offline -p lang-frontend --doc ConstEnabledTypedUnit
cargo test --locked --offline -p lang-frontend --doc ConstEnabledOwnedUnit
cargo test --locked --offline -p lang-frontend --doc check_compilation_unit_constant_ownership
```

## 明确保留的后继门禁

- 未实现 view/factory，也未减少 index。普通成功路径四次是源码静态推导，没有冒充动态计数
- Rust 1.96.0 自带 LLVM22.1.2；现有 llvm-cov/profdata 为21.1.8，toolchain 未安装
  llvm-tools。未下载工具、未插桩、未运行 index 动态计数或受控 A/B 耗时/RSS；后续需官方
  匹配 LLVM22 工具与固定 fixture，分别扣除 setup，不能用 LLVM21 处理该 Rust profile
- 新 view 的不可伪造、borrow lifetime、recovery/basic/const 工厂 compile-fail 留到生产片；
  当前七项证明既有 basic/const 分离，不预证尚不存在的 API
- 原 N1 保留 commit 失败清理；没有新增真实 LLVM emission-failure 注入，也不把 commit
  失败当作全部 emission 失败证据。H1/H2 仍是宿主邻层保护，不扩大原有场景含义
- 本地未运行 macOS、frontend 全量或 workspace 全量 tests。新 head 的独立 review、Draft PR
  与 exact-head 双宿主 CI 另行核验，终态留 PR；不能复用基底 CI 冒充本片
- 整体计划完成后的外部审计继续排队，本片不提前启动


## 发布前同步 PR28 主干

独立review在原head `31115b00115d98c411f33fc3cdf129fb022beb08` 完成；随后普通merge
`main 34861321d15830dc639e7641edd3a41524d57fc2`（PR28），保留双方父链。冲突仅在开发
索引和治理账本，保全双方导航与完整历史段落；policy保留PR28退休原multifile 7202行项，
同时保持本片native入口2050额度，不重生baseline。三个codegen测试文件逐字未变。

同步后重跑fmt、workspace all-targets check、codegen严格Clippy、新8项、全部native unit92项
和导入main的multifile type完整107项，均通过且0 ignored；docs469、96 policy、尺寸护栏
（677手写/48超限/0生成物）与diff通过。旧24条exact和7项compile-fail的源码、依赖、feature
与工具链输入均未改变，沿用上节实跑证据，不声称本次再次执行。同步差异另作窄review，
首次Draft直接使用同步后的head；远端exact-head CI终态留PR，不以同步前结果替代。
