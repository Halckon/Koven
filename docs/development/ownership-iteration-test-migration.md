# P2 ownership iteration integration分组验收

> **性质**：有界迁移验收 · **状态**：本地验收通过，独立review / Draft PR CI待后继 · **读取时机**：复核本片身份、场景边界、CI与成本时 · **唯一真源**：本页与逐项证据；范围从属[整体计划](engineering-governance-plan.md)

## 范围、基线与非目标

固定main `e30f9af5200b52c2c9fee7b831f5e63209608cb2`（PR26合并）。
纯移动commit `5b53bf60497136472fda7778842d385b4c1dcaee`，必要CI接线另commit `aaf2473ce72a259da444cc4c2dcfc0766557abcd`，文档/policy/evidence再独立提交。
这是批准计划P2的大integration首片，不新增Spec、语言语义、依赖、生产API或通用测试框架。

- `crates/lang-frontend/tests/ownership_iteration.rs` 12063→77 PLOC；保留原Cargo target名称
  `--test ownership_iteration`，以显式path加载同名目录下20个私有领域模块
- 184 tests及7 helpers完整保留；共享`checked`与原imports/support声明留入口，六个场景helper
  跟随唯一调用领域。不去重、不拆case、不弱化断言、不压行、不使用include拼接
- 没有生产Rust、Cargo manifests/lockfile、workspace/target集合或fixture改动；没有新增pub、trait、
  platform cfg、ignore或should_panic。已有22个support文件逐字不动
- 184个libtest叶名保留，完整身份由根级叶名变为`领域模块::叶名`，全部映射见机器账本。
  不把旧根级`--exact`零命中称为通过；同一target无名字过滤器前后均实际184/184

## 最小领域分组与helper归属

每个模块围绕同一ownership事实或完整replay场景；同一共享状态轨迹不截断。
`prior_owned_instances`保留两轮旧环境实例，`formed_capture_instances`保留形成时的多实例，
`nested_instance_replay`保留内层zero/two rounds与三种transfer，避免合并成新的千行热点。

| ownership_iteration/模块 | PLOC | tests | 域内helper |
|---|---:|---:|---|
| `elvis_continuations.rs` | 249 | 8 | 无 |
| `capture_owner_identity.rs` | 765 | 11 | 无 |
| `origin_fixed_point.rs` | 297 | 7 | 无 |
| `phi_layout.rs` | 795 | 13 | 无 |
| `phi_edges.rs` | 714 | 9 | 无 |
| `phi_source_replay.rs` | 665 | 2 | `assert_loop_phi_source_replay` |
| `sibling_source_loans.rs` | 737 | 5 | `assert_loop_carried_sibling_release` |
| `source_loans.rs` | 660 | 19 | 无 |
| `control_exits.rs` | 622 | 16 | 无 |
| `capture_escape.rs` | 385 | 22 | 无 |
| `enclosing_leaf_snapshots.rs` | 750 | 7 | 无 |
| `enclosing_owned_captures.rs` | 582 | 6 | 无 |
| `call_entry_and_pending.rs` | 506 | 13 | 无 |
| `recursive_capture_graph.rs` | 459 | 10 | 无 |
| `prior_owned_instances.rs` | 657 | 2 | 无 |
| `formed_capture_instances.rs` | 833 | 3 | 无 |
| `nested_instance_replay.rs` | 577 | 7 | `assert_nested_conditional_capture_defers`, `assert_nested_loop_phi_replays_distinct_source_instances` |
| `conditional_cleanup.rs` | 401 | 5 | `assert_selected_closure_source_survives` |
| `replacement_snapshots.rs` | 525 | 14 | 无 |
| `loop_carried_choices.rs` | 868 | 5 | `assert_loop_carried_sources_survive_exit` |

最大文件868行，无新增尺寸例外；最大原case736行、原helper653行，各自完整保留。
尺寸policy只退休原12063行baseline，其他baseline逐值、三个既有例外和generated均不动。
实际638→658个手写Rust文件、50→49个超千行；仍有46项历史超限与3个既有完整场景例外。
生产iteration 1871行职责欠账不由这次integration搬迁消除；P2整体仍未完成。

## 逐项保真、身份和扫描器

package=`lang-frontend`、target=`ownership_iteration`、kind=`test`。191个完整函数块
（184 tests＋7 helpers）逐字相等；另从固定Git原文每块独立经Rust1.96.0的rustfmt生成参考，
新块与该参考逐字相等。没有相对路径token迁移或格式token编辑，不靠忽略路径/标点作归一化。
184个test属性、1174处assert宏与1248个完整字符串/字符literal token均保留，
包括嵌入源码、精确Span定位字符串、diagnostic断言与replay oracle。
literal按块hash再按名称排序的SHA-256 aggregate：
`4303197b8e0f79f4388e144d377a00678ca413b1aea32cc6adad2777e4783030`。

[机器账本](evidence/ownership-iteration-test-migration.json)保存全部184项旧/新完整身份、
191块起止行与原新hash、literal hashes、属性/ignore、全部文件尺寸、support/Cargo hashes、
实测命令原值与test result；同一worktree的前后Cargo metadata完整JSON相等，共129 targets。
不把保持数量当作保真的充分条件。完整原/新libtest list逐项对应，20个新文件不会生成test binaries。

[只读复核器](evidence/verify_ownership_iteration_migration.py)从固定Git原文重建独立参考，
验证全部块、映射、literal/断言、support和Cargo输入、入口声明及唯一Rust变更范围：

```sh
python3 docs/development/evidence/verify_ownership_iteration_migration.py
git diff --color-moved=zebra e30f9af 5b53bf6
```

Rust尺寸扫描通过git tracked/untracked递归发现新目录；诊断与parser support加载入口未迁移。
现有Guide源码pattern scanner仅扫描guide_litmus.rs；上一片私有iteration复核器仅扫描其自身树，
两者不受影响。未发现工具硬编码读取本target正文；0182历史exact命令旁新增当前`control_exits::`
映射与实际1 passed / 183 filtered，保留历史结果原义，不关闭0182剩余合同。

## 必要双平台CI接线

该target此前不在stage/Guide选集；普通check/clippy编译通过不等于184执行通过。
本片在既有`Targeted Tests`双平台job直接加独立步骤：
`cargo test --locked -p lang-frontend --test ownership_iteration`。
没有细分路径条件、名字filter、ignore、continue-on-error或新增矩阵；复用现有Rust PR/main/
workflow_dispatch触发，后续生产frontend改动也会执行。feature/fix push的既有成本策略不变。

不扩frontend全量、stage/Guide名单或summary结构；失败直接使test job与fail-closed汇总失败。
新增两个CI policy回归先红（缺step/命令）、接线后12/12通过，固定单target无filter与双平台位置，
并禁止借本片加入frontend全integration/workspace测试。最终必须核验本PR exact-head两宿主日志
各自184 passed / 0 failed / 0 ignored / 0 filtered；旧CI绿灯不能替代。

## 实际本地门禁

Linux x86_64，Rust/Cargo1.96.0，LLVM/Clang21.1.8；共享target、CARGO_INCREMENTAL=0，
Cargo串行，无clean、不复制target。以下均exit0；未跑项列在末尾。

| 命令/范围 | 实际结果 |
|---|---|
| `cargo metadata --locked --offline --no-deps --format-version 1` | 前后129 targets完整JSON相等 |
| `cargo test --locked --offline -p lang-frontend --test ownership_iteration -- --list` | 前后各184，逐项名称映射 |
| `cargo test --locked --offline -p lang-frontend --test ownership_iteration` | 前后184 passed / 0 ignored / 0 filtered |
| 同一libtest executable直接运行 | 前后三轮各184 passed，不含Cargo启动开销 |
| 上述target加新`control_exits::iteration_ownership_facts_are_deterministic_across_analyses -- --exact` | 1 passed / 183 filtered |
| `cargo test --locked --offline -p lang-frontend --lib` | 187 passed / 0 ignored / 0 filtered；含40 iteration tests与53 instance_replay，执行10.72s |
| `cargo fmt --all -- --check` | 通过，2.954s |
| `cargo check --locked --offline -p lang-frontend --all-targets` | 通过，8.222s |
| `cargo clippy --locked --offline -p lang-frontend --all-targets -- -D warnings` | 通过，13.362s |
| `cargo check --locked --offline -p lang-frontend --release` | 普通release check通过，5.217s；非release tests |
| `python3 scripts/check_rust_sizes.py --base origin/main` | merge-base e30f9af；658手写/49超限，0新例外，通过 |
| `python3 -m unittest discover -s scripts/tests -v` | 96/96通过 |
| `python3 scripts/check_docs.py`、`git diff --check` | 文档结构与whitespace通过 |

## 受控compile+link、执行与RSS

测量前固定调查阈值：after中位数相对before的build墙钟增长超过max(20%,1s)、
峰值RSS超过max(10%,65536KiB)、短执行墙钟超过max(30%,.020s)时检查负载/范围/freshness，
必要时追加同范围配对样本。不是性能SLO或统计置信，所有原值保留。

同一worktree/lockfile/features/profile/工具链/target。首次基线list预热21.332s/2088516KiB
包含frontend重建，不纳入配对；after list预热4.183s/615940KiB也不纳入配对。
每侧两次只touch integration入口，Cargo JSON确认仅ownership_iteration nonfresh，
frontend lib与依赖均fresh。三次/侧no-op和独立executable交替运行，libtest默认并行。

| 范围 | before墙钟s | after墙钟s | before峰值RSS KiB | after峰值RSS KiB |
|---|---|---|---|---|
| build | 3.930727 / 4.155863 | 4.670783 / 4.478514 | 589548 / 595720 | 600416 / 587756 |
| noop | 0.033895 / 0.034140 / 0.036532 | 0.036007 / 0.034655 / 0.035852 | 29752 / 29496 / 29384 | 29456 / 29328 / 29244 |
| execute | 0.261594 / 0.256077 / 0.259389 | 0.265630 / 0.256802 / 0.262826 | 20496 / 20360 / 20692 | 20308 / 20088 / 20388 |

每命令由新Python进程subprocess.run，墙钟用perf_counter，退出后读取
resource.getrusage(RUSAGE_CHILDREN).ru_maxrss（Linux KiB，最大单子进程峰值，非总RSS）。
依赖warm、OS page cache未清空、非独占主机，顺序小样本保留噪声。build约4–4.7s，
no-op约.03s，184执行约.26s明确分开；调查阈值未触发。不宣称提速、性能等价或无任何退化。
真正冷依赖/冷OS、compile/link分段和二进制体积未测，不据此合并其他target或调整全量预算。

## 未运行、交付和回退

本地未运行frontend其余120个integration、workspace/codegen/native全量、macOS、release tests；
普通release check不冒充release二进制体积证据。双平台全配置及新增184步骤交Draft PR exact-head
CI核验，独立review/CI终态留PR，避免只追加外部状态反复产生新待验证head；合并由维护者决定。

纯移动21个Rust文件、CI两文件、文档/policy分别为独立回退单位；若回退已退休的大文件，
必须按当时base显式审阅尺寸额度，不恢复历史baseline绕过。P2仍待其他大integration及独立生产
职责拆分；P3/P4/P5未自动完成，0182仍active，整体计划完成后的外部审计继续延后。
