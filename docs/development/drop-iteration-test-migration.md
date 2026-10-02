# P2 iteration 私有测试领域拆分验收

> **性质**：有界迁移验收 · **状态**：本地验收通过，独立 review / PR CI 待后继 · **读取时机**：复核本片身份、格式化差分、例外与成本时 · **唯一真源**：本页与逐项证据；范围从属[整体计划](engineering-governance-plan.md)

## 范围、基线与非目标

固定 main `9f9ee5230b6bc0affd2b7af727294c7f1bbc328b`（已合并 PR25）。
纯搬迁 commit `1df4df198c069b196f152e17a0df65e2d54aaeb2`；文档、policy 和证据另 commit。
本片不新增 Spec 或语言语义；按已批准 P2 顺序整理私有单元测试，不把例外当生产达标。

- `iteration.rs` 14688→1876 PLOC；生产前缀1871行逐字不变，保留两个 cfg(test) 声明，
  仅将 inline `mod tests { ... }` 外置为 `mod tests;`。图算法、phi seeding/incoming/forwarding不变
- `iteration/instance_replay.rs` 及其完整子树逐字不变，仍是与 tests 平级的独立 cfg(test) 模块；
  两套回放 helper 不合并，不把局部静态候选回放宣称为已完成动态实例执行
- `tests/ownership_iteration.rs` 12063行及现有184项不变；没有改Cargo manifests、lockfile、
  workspace、target数量、workflow、依赖、production可见性或平台/ignore边界
- 40个测试、11个helper完整搬迁；同版rustfmt外置缩进和169处相对路径是全部代码形态变化。
  没有拆case、删断言、压行、include拼接或新增pub；生产1871行职责拆分仍待后继独立片

## 领域与 helper 归属

`iteration/tests.rs` 为371行私有入口，保留原imports与8项共用replay定义：
`replay_owned_closure_release`、`selected`、`replay_edge_presence`、`replay_edge_candidates`、
`ReplayInstances`、`replay_captured_edge_presence`、`replay_captured_edge`、`replay_snapshot_choices`。
子模块仅 `use super::*` 访问祖先私有定义。三个场景helper各留在唯一调用领域；不增加共享抽象。

| tests/子模块 | PLOC | tests | 场景helper |
|---|---:|---:|---|
| `capture_graph.rs` | 521 | 8 |  |
| `coexisting_paths.rs` | 984 | 1 |  |
| `conditional_diamond.rs` | 882 | 1 |  |
| `conditional_leaf.rs` | 1155 | 1 | `assert_conditional_leaf_replay` |
| `conditional_rebinding.rs` | 609 | 1 | `assert_conditional_seed_rebinding` |
| `environment_drop.rs` | 139 | 2 |  |
| `file_parent.rs` | 1143 | 6 | `assert_cross_loop_parent_release` |
| `independent_roots.rs` | 452 | 3 |  |
| `loop_handoff.rs` | 191 | 2 |  |
| `recursive_body.rs` | 536 | 1 |  |
| `recursive_chain_jumps.rs` | 565 | 1 |  |
| `recursive_chains.rs` | 796 | 2 |  |
| `recursive_jump_roots.rs` | 532 | 1 |  |
| `recursive_shared_layout.rs` | 582 | 2 |  |
| `sibling_shared_loans.rs` | 1145 | 1 |  |
| `snapshot_cleanup.rs` | 384 | 5 |  |
| `snapshot_mixed_roots.rs` | 830 | 1 |  |
| `snapshot_recursive_roots.rs` | 906 | 1 |  |

同一职责按场景完整性分界，例如两个递归chain跳转场景各自需要长连续实例轨迹，
不能为了更少文件把不同oracle拼成超过千行的新热点。图结构/有界遍历与phi局部路径集中在
capture_graph；跨循环父环境的六个参数化case和其唯一helper集中在file_parent。

## 三个新有界例外与剩余欠账

三个新文件是手写完整fixture，不是生成物，也不借原iteration的baseline继承新路径额度。
[scripts/rust-size-policy.json](../../scripts/rust-size-policy.json) 为每项登记负责人、理由、
实际上限、后续拆分方向和复查条件；owner为lang-frontend ownership/drop tests（Halckon）。

- conditional_leaf 上限1155：原6行case＋1159行helper完整保留条件形成、presence和三种tail矩阵
- file_parent 上限1143：原1072行helper＋六个12行case保持同一文件级layout与连续实例状态
- sibling_shared_loans 上限1145：原1156行单case保持兄弟实例运输、loan-end与release整个oracle

上限精确锁定rustfmt后实际PLOC，不预留任意增长。下次修改场景、共享回放helper或
instance_replay合同前复查；未来若按形成/逐边回放/清理oracle提取helper，须独立语义保真评审，
不是本片拆断断言。其余15个领域文件均≤1000，最大coexisting_paths984。
原iteration baseline仅14688→1876；其余46个baseline逐值不变，generated仍为空。
实际扫描619→638个手写Rust文件，超千行47→50（47项旧欠账＋3项新例外）。
文件数和超限数增加是透明记录的迁移成本，不声称消除生产欠账或P2已完成。

## 身份、属性与逐项保真

全部package=`lang-frontend`，target=`lang_frontend`，kind=`lib`，由原cfg(test)门控制。
40个test属性、四个derive属性与内部fixture保持；没有平台cfg、ignore或should_panic新增/移除。
旧前缀为 `ownership_checking::checker::drop_planner::iteration::tests::`；
下表每行旧名=旧前缀＋叶名，新名=旧前缀＋新模块＋`::`＋同一叶名。
旧模块filter前后实际均40；旧完整名加`--exact`需按表增加模块段。

| 原叶名（保留） | 新模块 |
|---|---|
| `coexisting_capture_paths_include_the_root_instance` | `capture_graph` |
| `owned_cycle_release_does_not_claim_independent_leaf_roots` | `capture_graph` |
| `nested_phi_selector_writes_keep_local_instance_paths` | `capture_graph` |
| `optional_nested_capture_requires_instance_presence` | `capture_graph` |
| `finite_capture_graph_checks_long_chains_without_recursive_traversal` | `capture_graph` |
| `finite_layout_order_is_bounded_on_diamond_ladders_and_deep_chains` | `capture_graph` |
| `recursive_capture_layout_visits_each_static_lambda_once` | `capture_graph` |
| `self_recursive_capture_layout_registers_finite_static_slots` | `capture_graph` |
| `conditional_leaf_phi_replays_formed_instance_and_presence` | `conditional_leaf` |
| `planner_enclosing_environment_drop_reaches_owned_descendant` | `environment_drop` |
| `enclosing_environment_keeps_opaque_parent_drop` | `environment_drop` |
| `recursive_body_formation_reads_the_current_header_instance` | `recursive_body` |
| `conditional_seed_recursive_rebinding_jump_matrix` | `conditional_rebinding` |
| `alternating_recursive_capture_releases_the_formed_instance_chain` | `recursive_chains` |
| `recursive_return_releases_the_formed_chain_once` | `recursive_chains` |
| `alternating_recursive_capture_jump_edges_keep_the_formed_chain` | `recursive_chain_jumps` |
| `recursive_capture_jump_edges_carry_the_new_root_instance` | `recursive_jump_roots` |
| `recursive_graph_keeps_independent_unexpanded_closure_roots_owned` | `independent_roots` |
| `recursive_loop_keeps_independent_shared_capture_loan_end` | `independent_roots` |
| `independent_closure_survives_a_recursive_loop_into_the_next_loop` | `independent_roots` |
| `recursive_release_layout_keeps_untracked_shared_capture` | `recursive_shared_layout` |
| `untracked_copyable_shared_capture_keeps_loan_ends` | `recursive_shared_layout` |
| `snapshot_keeps_distinct_same_lambda_phi_and_ordinary_roots` | `snapshot_mixed_roots` |
| `parent_of_two_recursive_loops_needs_file_wide_release_layout` | `file_parent` |
| `file_parent_with_recursive_chains_and_shared_child_keeps_one_release_root` | `file_parent` |
| `file_parent_shared_source_crosses_two_recursive_loops` | `file_parent` |
| `parent_of_recursive_snapshot_needs_file_wide_release_layout` | `file_parent` |
| `nested_environment_capture_needs_file_wide_release_layout` | `file_parent` |
| `nested_environment_capture_of_ordinary_sibling_keeps_ordinary_drop` | `file_parent` |
| `snapshot_keeps_two_recursive_phi_roots_without_tree_origins` | `snapshot_recursive_roots` |
| `next_loop_entry_keeps_recursive_root_after_conditional_snapshot` | `loop_handoff` |
| `next_recursive_loop_entry_keeps_previous_phi_root_sources` | `loop_handoff` |
| `moved_recursive_phi_releases_instances_through_new_binding` | `snapshot_cleanup` |
| `conditional_snapshot_of_recursive_phi_keeps_instance_release` | `snapshot_cleanup` |
| `conditional_snapshot_of_independent_phi_does_not_duplicate_child_drop` | `snapshot_cleanup` |
| `captured_phi_release_does_not_duplicate_child_drop` | `snapshot_cleanup` |
| `mixed_phi_and_new_closure_versions_do_not_duplicate_instance_release` | `snapshot_cleanup` |
| `coexisting_same_lambda_paths_replay_two_rounds_and_release` | `coexisting_paths` |
| `sibling_shared_capture_loans_keep_distinct_instance_paths` | `sibling_shared_loans` |
| `conditional_diamond_keeps_distinct_saved_paths_at_exhaustion` | `conditional_diamond` |

前后完整libtest `-- --list` 均187项：40项一对一映射＋147项完整身份不变。
同一worktree的Cargo metadata完整JSON相等，无路径正规化，共129 targets。
1488处assert宏保留，448个完整字符串/字符literal token逐字相等，包含全部嵌入源码、
错误信息和断言字符串；literal按块再按名称排序的SHA-256 aggregate为
`35767dc72f64f3858fdee840cbeab0d10263f3f6a48da75fb2ab4b348d9e3631`。

不宣称搬迁前后完整块逐字相同：默认rustfmt因退出inline缩进而在13块中自动产生42处
纯表达式closure/match arm的花括号与尾逗号编辑。每个原完整块先仅加入必要super前缀，
独立经同版rustfmt生成参考；实际新块与参考逐字相等。另比较原/参考token，
只允许完整记录的 `{`、`}`、`,` 差分，标识符、运算、literal、comment均不变。
169条super路径逐项从旧tests作用域和新领域作用域解析，完整目标路径必须相同，
并验证新文本恰多一个super；不是仅比较数量或忽略所有路径token。

[机器账本](evidence/drop-iteration-test-migration.json)逐项保存51块原/新hash、字面量hash、
40项映射、169条原新路径及解析目标、13块42处格式token编辑与全部测量原值。
其中token index是独立块的零基索引，format diff在路径调整之后比较，便于人工定位。
[只读复核器](evidence/verify_drop_iteration_migration.py)从固定Git原文重建独立参考，
不依赖工作目录外日志，不写Rust文件，不把语义token变化模糊归一化：

```sh
# 使用项目固定Rust 1.96.0的rustfmt，在仓库根运行
python3 docs/development/evidence/verify_drop_iteration_migration.py
git diff --color-moved=zebra 9f9ee5230b6bc0affd2b7af727294c7f1bbc328b 1df4df198c069b196f152e17a0df65e2d54aaeb2
```

## 实际本地门禁

Linux x86_64；Rust/Cargo1.96.0，LLVM/Clang21.1.8，codegen后端与rustc自带LLVM22.1.2区分。
既有共享target，`CARGO_INCREMENTAL=0`，Cargo严格串行，无clean或复制target。下列均exit0。

| 命令/范围 | 实际结果 |
|---|---|
| `cargo metadata --locked --offline --no-deps --format-version 1` | 前后129 targets完整JSON相等 |
| `cargo test --locked --offline -p lang-frontend --lib -- --list` | 前后187项，40映射＋147不变 |
| `cargo test --locked --offline -p lang-frontend --lib ownership_checking::checker::drop_planner::iteration::tests` | 前后40 passed / 0 failed / 0 ignored / 147 filtered |
| 直接libtest executable加同一旧filter | 前后三轮各40 passed，无空命中 |
| `cargo test --locked --offline -p lang-frontend --lib ownership_checking::checker::drop_planner::iteration::instance_replay` | 53 passed / 0 ignored / 134 filtered，10.660s |
| `cargo test --locked --offline -p lang-frontend --test ownership_iteration` | 184 passed / 0 ignored / 0 filtered，含编译20.307s |
| `cargo fmt --all -- --check` | 通过，2.667s |
| `cargo check --locked --offline -p lang-frontend --all-targets` | 通过，8.103s |
| `cargo clippy --locked --offline -p lang-frontend --all-targets -- -D warnings` | 通过，13.622s |
| `cargo check --locked --offline -p lang-frontend --release` | 普通release静态检查通过，5.300s；非release tests |
| `python3 scripts/check_rust_sizes.py --base origin/main` | merge-base9f9ee52；638手写/50超限，含3新例外，通过 |
| `python3 -m unittest discover -s scripts/tests -v` | 94/94通过 |
| `python3 scripts/check_docs.py`、`git diff --check` | 文档结构与whitespace通过 |

## 受控compile+link、执行与RSS

采样前固定调查触发预算：after相对before中位数的build墙钟增加超过max(20%,1s)，
峰值RSS增加超过max(10%,65536KiB)，短执行墙钟增加超过max(30%,.020s)即查freshness、
命令范围与负载，必要时追加同范围配对样本；保留全部异常，不作统计置信或性能SLO。

同一worktree/lockfile/features/profile/工具链/target；首次基线预热25.230s/2616768KiB，
新状态预热23.902s/2561324KiB，不纳入配对比较。依赖已缓存，不是干净机器冷构建。
两次/侧仅touch iteration.rs强制frontend重新compile+link，每次Cargo JSON均确认只有
lang_frontend nonfresh，依赖全部fresh。三次/侧no-op与直接executable交替运行，
执行使用libtest默认并行，避免Cargo启动开销混入测试耗时。

| 范围 | before墙钟s | after墙钟s | before峰值RSS KiB | after峰值RSS KiB |
|---|---|---|---|---|
| build | 25.263994 / 24.835445 | 25.077207 / 23.236271 | 2592952 / 2581832 | 2532048 / 2551252 |
| noop | 0.033883 / 0.036754 / 0.036634 | 0.037150 / 0.042840 / 0.033989 | 28252 / 29524 / 29816 | 29420 / 29384 / 29744 |
| execute | 0.049127 / 0.040286 / 0.040807 | 0.047865 / 0.049162 / 0.048844 | 23300 / 22712 / 22248 | 22056 / 22356 / 22968 |

每条命令由新的Python进程调用subprocess.run，墙钟用perf_counter，退出后读取
resource.getrusage(RUSAGE_CHILDREN).ru_maxrss（Linux KiB），记录最大单个子进程峰值，
不是进程RSS总和；外层shell未用exec复用测量进程，避免继承累计rusage。
采样前loadavg0.03/0.03/0.03；非独占主机，OS page cache不清空，顺序采样仍有噪声。
四次build约23–25s，与约.03–.04s的no-op明确分开；独立40项执行约.04–.05s。
所有样本exit0，调查阈值未触发；不声称提速、性能等价或无任何退化。
真正冷依赖/冷OS缓存、compile/link分段、二进制体积均未测，不据此调整target集合。

## 未运行、交付与回退

本地未跑frontend全量integration、workspace/codegen/native、macOS或release tests。
与本片相关的非默认ownership_iteration已实际184/184，动态回放边界53/53；普通release无
测试泄漏由保留cfg(test)可达性及release check支持，未做二进制大小证明。
完整配置双宿主check/clippy/core/stage/Guide交Draft PR exact-head CI，不能沿用基底绿灯。
独立review/PR CI终态留PR，不为外部终态反复制造新head；合并由维护者最终判断。

20个Rust文件的纯搬迁commit可独立回退；文档/policy/evidence为另一commit。
若已合并，重新扩大iteration.rs必须按当时base申请有界例外，不能恢复已收紧baseline冒充旧额度。
不删断言或增ignore换绿。0182保持独立active；P2后继integration与生产职责拆分、
P3/P4/P5均未因本片完成；用户要求的整体完成后外部审计仍排队。
