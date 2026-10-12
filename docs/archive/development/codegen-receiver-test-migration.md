# P2 codegen receiver 私有测试搬迁验收

> **性质**：有界测试搬迁验收记录 · **状态**：本地结构与正确性已验；待独立 review 与 Draft PR exact-head CI · **读取时机**：评审、复现或回退 receiver 测试拆分时 · **唯一真源**：本页记录本片身份映射与实测，整体进度见[执行账本](engineering-governance-progress.md)

## 范围与固定源码

2026-10-02 从 PR18 合并后的 `main 7318d53e4c5677684630e2b0b9148e9c6ceb1c35`
建立独立分支 `feature/spec-p2-codegen-receiver-tests`。该 main 的
[CI 36999639756](https://github.com/Halckon/Koven/actions/runs/36999639756) 为9/9 jobs success，
两宿主 core、stage、Guide 步骤实际成功。该结果只证明基底，不代替本分支 CI。
本地纯搬迁 commit 为 `643f2c4ece1e49de005a1053cf4f594d737207a0`，tree 为
`981f9888f94dfc9027b0c57a7e3f1d0b2a788e8b`；只修改八个私有测试 Rust 文件。尺寸政策收紧与本文另作提交。
本地提交使用仓库既有身份；GitHub connector 发布后核对 blob、tree、父链与 fetch 完整 diff，
远端提交元数据及 SHA 可能不同，实际配对和最终 CI 留在 PR。

本片按[已批准计划](engineering-governance-plan.md)继续 LSP 之后的 codegen 测试整理。
不改 `unit_plan` 或其他生产算法，不新增 Cargo target、依赖、公开 API、语言语义、测试或 ignore。
这只是一个有界结构切片，不代表 P2 整体完成；未获得完整性能证据，不宣称提速或无退化。

## 私有模块与尺寸

路径相对 `crates/lang-codegen/src/ssa/`；PLOC 采用尺寸护栏的 LF 物理行口径。

| 文件 | PLOC | 职责 / 测试数 |
|---|---:|---|
| `unit_lower_receiver_tests.rs` | 34（原3383） | 原 imports、一个私有 `function` helper 与七个私有 module 入口 |
| `unit_lower_receiver_tests/interface.rs` | 605 | concrete StaticSelf、默认实现/继承与泛型布局 demand；10项 |
| `unit_lower_receiver_tests/delegation.rs` | 256 | 单级/链式 heap-field loan 委托与顺序；2项 |
| `unit_lower_receiver_tests/representation.rs` | 150 | stateless object ZST 与 enum receiver 表示；2项 |
| `unit_lower_receiver_tests/inout_inline.rs` | 829 | inline exclusive storage、字段修改与正常调用后写回；6项 |
| `unit_lower_receiver_tests/inout_fields.rs` | 480 | class/generic/nullable payload replacement 与 divergent RHS；7项 |
| `unit_lower_receiver_tests/borrowing.rs` | 429 | receiver-first、隐式/显式 loan forwarding、reborrow 与调用期 owner；7项 |
| `unit_lower_receiver_tests/value_delivery.rs` | 616 | concrete/StaticSelf value delivery、CFG 与 drop identity；12项 |

原 `ssa/mod.rs` 的 `#[cfg(test)] mod unit_lower_receiver_tests;` 原字节不变。
七个子模块继承这条唯一入口；只用 `mod` 与 `use super::*`，helper 继续私有，不使用 `include!`。
领域粒度按职责划分；interface Value delivery 统一进入 value_delivery，避免按原文件区间机械切片。

`rust-size-policy.json` 仅删除已经降到34行的 receiver 历史 baseline 条目（3383）。其余48项
逐值不变，未重新生成/提高 baseline，无新增 exception 或 generated。真实扫描由596→603个
手写 Rust 文件，超1000行由49→48；不是消除了其余历史欠账。

## 完整身份的一对一映射

所有条目 package=`lang-codegen`，target=`lang-codegen`，kind=`lib`；继承 `cfg(test)`，
没有平台 cfg、ignore、should_panic、外部 fixture 或相对 include 路径。
下面每行精确定义：旧名=`ssa::unit_lower_receiver_tests::叶名称`；
新名=`ssa::unit_lower_receiver_tests::新模块::叶名称`。46个叶名称全部保留。

| 叶名称 | 新模块 |
|---|---|
| `interface_default_receiver_is_specialized_to_the_concrete_owner` | `interface` |
| `interface_inout_default_preserves_exclusive_receiver_abi` | `interface` |
| `interface_default_instances_have_distinct_concrete_symbols` | `interface` |
| `super_interface_call_keeps_the_selected_default_and_concrete_receiver` | `interface` |
| `interface_default_propagates_concrete_self_to_super_default` | `interface` |
| `interface_default_propagates_concrete_self_through_explicit_this_call` | `interface` |
| `interface_default_dispatches_abstract_requirement_to_concrete_override` | `interface` |
| `interface_default_dispatches_ancestor_requirement_to_inherited_default` | `interface` |
| `dependent_inherited_owner_key_does_not_materialize_wrapper_layout` | `interface` |
| `dependent_inherited_runtime_demand_materializes_exact_wrapper_layout` | `interface` |
| `borrow_delegation_projects_one_heap_field_loan_and_forwards_it_directly` | `delegation` |
| `delegation_chain_projects_each_heap_field_loan_in_source_order` | `delegation` |
| `stateless_object_receiver_uses_zst_addressization_without_runtime_storage` | `representation` |
| `enum_receivers_preserve_tagged_identity_for_borrow_and_value_modes` | `representation` |
| `inline_inout_read_only_receivers_use_exclusive_call_storage` | `inout_inline` |
| `move_only_inline_inout_replaces_a_move_only_field` | `inout_inline` |
| `copyable_inline_inout_rebinds_the_mutated_value_after_the_call` | `inout_inline` |
| `move_only_inline_inout_rebinds_after_copyable_field_mutation` | `inout_inline` |
| `move_only_inline_inout_read_takes_the_same_root_back_after_the_call` | `inout_inline` |
| `inline_inout_this_forwards_the_existing_exclusive_receiver` | `inout_inline` |
| `inout_class_receiver_replaces_and_reads_the_same_payload_field` | `inout_fields` |
| `inout_class_receiver_replaces_a_move_only_payload_field` | `inout_fields` |
| `direct_slot_generic_receiver_replaces_a_concrete_string_field` | `inout_fields` |
| `direct_slot_generic_receiver_keeps_concrete_int_replacement_trivial` | `inout_fields` |
| `nested_generic_receiver_replaces_a_concrete_wrapper_owner` | `inout_fields` |
| `generic_nullable_receiver_replaces_pointer_like_values_with_conditional_drop` | `inout_fields` |
| `divergent_rhs_does_not_emit_an_inout_class_payload_replace` | `inout_fields` |
| `lowers_borrow_member_receiver_before_explicit_arguments` | `borrowing` |
| `forwards_implicit_this_loan_without_readdressing_receiver` | `borrowing` |
| `forwards_explicit_borrow_binding_as_member_receiver` | `borrowing` |
| `value_this_can_borrow_for_an_implicit_member_call` | `borrowing` |
| `inout_this_reborrows_shared_for_an_implicit_member_call` | `borrowing` |
| `borrow_class_receiver_preserves_owner_until_post_call_drop` | `borrowing` |
| `inout_class_receiver_uses_exclusive_call_scoped_loan` | `borrowing` |
| `value_receiver_reuses_copyable_inline_value` | `value_delivery` |
| `value_receiver_moves_class_owner_to_callee_drop` | `value_delivery` |
| `value_receiver_can_return_this_without_callee_drop` | `value_delivery` |
| `value_receiver_is_carried_and_dropped_on_each_conditional_exit` | `value_delivery` |
| `value_receiver_is_rebound_across_while_edges` | `value_delivery` |
| `interface_value_default_drops_move_only_concrete_receiver_once` | `value_delivery` |
| `interface_value_default_skips_drop_for_copyable_concrete_receiver` | `value_delivery` |
| `interface_value_defaults_deliver_static_self_through_explicit_and_implicit_calls` | `value_delivery` |
| `interface_value_default_delivers_receiver_on_each_early_return_edge` | `value_delivery` |
| `interface_value_default_merges_delivered_and_retained_receiver_paths` | `value_delivery` |
| `consumed_receiver_identity_survives_divergent_sibling_lowering` | `value_delivery` |
| `consumed_receiver_identity_survives_divergent_while_body_lowering` | `value_delivery` |

其余633项完整 libtest 身份不变；实测新旧 `-- --list` 都为679 tests / 0 benchmarks。
集合恰好等于这46项映射加633项不变身份，无丢失/新增/重复。
旧模块 filter `unit_lower_receiver_tests` 搬迁前后都实际命中46项，所有原叶 filter仍可用。
旧 full name不再是新 exact identity；需要 `--exact` 的调用应按上表增加模块段，零命中不算通过。

## Move-aware 保真证据

从固定 main Git blob `855f6336c50555ec2cc9f5894f226a1aabf6c27b` 提取46个完整测试块
（attributes、签名、函数体；仅忽略块间空白分隔），按原叶名匹配新模块：46/46 UTF-8
SHA-256 逐字相等，未经过 token 化或语义近似。一个共享 helper 和原 imports 同样逐字相等。
162处 assert 宏及内嵌源码、诊断/SSA/LLVM oracle、fixture和字符串全由完整块字节相等保全。
按旧完整名排序后，以 LF 连接46个块 hash 的 SHA-256 为
`8deda3a40d78dedeb8c2dfcb11a19aebc7b785c56faf3a7a3a8c1447d83c23ea`。

复核时可从上述 base 和纯搬迁 commit 对每个 `#[test]` 起始块逐字比较，并使用
`git diff --color-moved=zebra <base> <move-commit> -- crates/lang-codegen/src/ssa` 辅助检查。
全部其他 Rust、Cargo manifests、lockfile、toolchain、workflow、现有 test-support 和生产
module入口均无 diff。新旧 Cargo metadata 完整 JSON 相等，共129 targets，无需路径规范化。
数量与 hash 是辅助证据；实际编译和执行证据如下，不凭数量替代测试。

## 本地实际验收

宿主 Linux x86_64；Rust/Cargo1.96.0、LLVM/Clang21.1.8，既有共享 target，
`CARGO_INCREMENTAL=0`。所有 Cargo 命令串行，无 cargo clean、target复制或并发抢锁。
命令均使用现有依赖的 offline/locked 模式；以下 Cargo 检查退出码均0。

| 命令 | 实际结果 |
|---|---|
| `cargo metadata --locked --offline --no-deps --format-version 1` | 新旧完整 JSON 相等；129 targets |
| `cargo test --locked --offline -p lang-codegen --lib -- --list` | 两次都679项；46项映射与633项不变集合完全对应 |
| `cargo test --locked --offline -p lang-codegen --lib unit_lower_receiver_tests` | 搬迁前后三轮各46 passed / 0 failed / 0 ignored / 633 filtered |
| `cargo fmt --all -- --check` | 通过，无需修改搬迁后的 Rust formatting |
| `cargo clippy --locked --offline -p lang-codegen --all-targets -- -D warnings` | 通过；14.980s |
| `cargo check --locked --offline --workspace --all-targets` | 通过；12.581s，只是编译/静态检查 |
| `cargo check --locked --offline -p lang-codegen --release` | 普通 release check通过；7.749s，不是release tests |
| `python3 scripts/check_rust_sizes.py --base origin/main` | merge-base为7318d53；603手写/48超限/0生成物，通过 |
| `python3 -m unittest discover -s scripts/tests -v` | 94/94通过，包含47项尺寸policy测试 |
| `python3 scripts/check_docs.py`、`git diff --check` | 462 Markdown与whitespace检查通过 |

## 有限 warm 样本与不能作出的结论

为后续设计受控测量保留一个最小可复现样本：同一 worktree、依赖、工具链、target 和输入，
分别在旧/新结构完成一次 `--list` 编译后，串行交替执行三轮已构建的 `--no-run` 与46项 filter。
每条命令由独立 Python3 进程以 `subprocess.run(command)` 执行；墙钟为
`time.perf_counter()` 差值，命令及等待过的子进程峰值 RSS 为
`resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss`（本 Linux 单位KiB）。
复现须在命令退出后读取 rusage，独立进程避免跨命令累积峰值；所有样本退出码0。

| 状态 / 命令 | 三次墙钟秒 | 三次峰值RSS KiB |
|---|---|---|
| 原结构 `--no-run` | 0.074905 / 0.059006 / 0.067619 | 30076 / 30272 / 30088 |
| 新结构 `--no-run` | 0.061496 / 0.058056 / 0.055311 | 29824 / 30264 / 30080 |
| 原结构46项 filter | 0.134680 / 0.137050 / 0.127134 | 80436 / 80408 / 79632 |
| 新结构46项 filter | 0.124781 / 0.116152 / 0.114684 | 80884 / 79992 / 80784 |

这里 `--no-run` 是 Cargo freshness/no-op，不是热编译或 link 时间；filter含 Cargo启动，
不是独立测试进程的纯执行耗时。三轮filter的libtest时间旧0.05/0.06/0.05s、新0.05/0.04/0.05s。
首次旧 `--list` 含新worktree引起的frontend+codegen编译（29.386s/2122496KiB），新 `--list`
仅重编译codegen（13.673s/1548152KiB），缓存/编译范围不同，明确不可作迁移前后性能比较。
没有控制宿主负载、清除OS缓存、分离compile/link或确定噪声区间，不能推导提速百分比、无退化
或预算。冷/热compile/link受控重复样本与退化预算仍未完成；不据此整合Cargo targets。

## 未运行项、远端门禁与回退

- 本地未运行完整codegen/native、CLI、frontend全量、macOS、release tests或体积测量。
  workspace check不替代执行；纯搬迁保真和46项只证明本片。完整已配置双宿主门禁交给PR CI
- 独立review、Draft PR与exact-head CI尚待完成；其终态记录到PR，不为追加外部CI结果改动head
- 不自动转Ready、merge或开启auto-merge。完整性能证据、其余大文件和P2整体仍是后继工作
- 八个Rust文件可以按纯搬迁commit独立回退；尺寸政策按当时base另行审阅并重跑护栏。若本片
  已合并，重新引入3383行旧文件必须登记有界例外，不能复活已删除baseline或重生成policy。
  不降低断言/新增ignore来通过；验收和状态文件与代码分开提交
