# P2 multifile ownership integration分组验收

> **性质**：有界迁移验收 · **状态**：本地验收与独立最终review通过，合并批次PR CI待后继 · **读取时机**：复核本片测试身份、源限定事实、CI与成本时 · **唯一真源**：本页与逐项证据；范围从属[整体计划](engineering-governance-plan.md)

## 范围、基线与非目标

固定审阅基底 `481fb844d62ea38851dc6ccdfb1531e3815b3445`，tree
`c284fa5061cbd9fae9fac90a4333dd16a86be37e`，已含0255本地验收与0254归档。
纯移动commit `db1bb34acb12049b5991e6dd12c8a150515d1e01`；文档/policy/evidence另独立提交。
沿批准P2计划完成原有integration职责整理，不新增Spec、语义、生产API、依赖或通用框架。
两提交随0255同一计划批次交付；本片未push、未创建PR，不把本地结果冒充远端验收。

- `tests/multifile_ownership_checking.rs`4692→141 PLOC，仍唯一同名Cargo integration target
- 72测试与8 helpers全保留；7个跨域helper留根私有，唯一消费者的跨线程helper随capture域
- 12个私有path子模块均低于1000行，没有新binary/pub、include拼接、cfg/ignore或fixture改写
- 本片只移动Phase 3合同测试，不改变Guide、能力边界、诊断、算法或生产调用关系

## 领域与helper归属

| multifile_ownership_checking/模块 | PLOC | tests | 合同 |
|---|---:|---:|---|
| `provenance_contracts.rs` | 355 | 3 | 参数/analysis来源、重复input拒绝、外部/函数值调用无伪造源参数 |
| `call_deliveries.rs` | 345 | 8 | Copy/Move/Temporary、source-qualified field、loan冲突/优先级 |
| `receivers.rs` | 647 | 13 | zeroth operand、this能力、conditional delivery/drop、delegation |
| `assignment_rollback.rs` | 100 | 3 | receiver/字段可写性、RHS回滚、发散前静态检查 |
| `capture_escape.rs` | 753 | 14 | this/closure capture、escape、transferability、失败回滚 |
| `container_places.rs` | 277 | 6 | element/Rc owner place、loan与非法move |
| `constructions.rs` | 577 | 8 | source/container构造交付、root、Nothing前缀、字段label |
| `drop_plans.rs` | 346 | 2 | ASAP/drop顺序、deferred gate |
| `lambda_drop.rs` | 299 | 2 | lambda entry/参数/尾值transfer与drop |
| `control_flow.rs` | 333 | 3 | when可达性、MoveOnly分支结果/主状态 |
| `non_null.rs` | 299 | 5 | assertion来源、Abort、transfer、错误清空与回边 |
| `pending_lifetime.rs` | 257 | 5 | pending owner/receiver/callee、退出与嵌套loan end |

`cross_thread_environments`完整迁入capture_escape；`parsed`、`validated_names`、
`validated_types`、`source_unit`、`expression_with_text`、`symbol_named`、`diagnostic_codes`
保持入口私有。子模块`use super::*`保留两处`self::parsed`解析，不改原函数路径token。
最大完整case280行，未拆case、缩断言或新增尺寸例外；原4692行baseline仅退休这一项。

## 逐项保真与身份

[机器账本](evidence/multifile-ownership-test-migration.json)列72个package/target/full_name
新旧映射、全部80函数起止行与hash、全部文件尺寸、fixture literal/断言、工具与样本原值。
80个函数块与固定Git原文逐字相等，也与同版Rust1.96.0独立rustfmt参考相等。
963个字符串/字符literal、412处assert及72个test属性全保留；无platform cfg或ignore。
literal按块hash再按名称排序的SHA-256 aggregate：
`fe53f6335838bc12d2bbb465449b658bccbc824530cc70e63357fe8f34f4d398`。

所有fixture均内联；root、相对逻辑源码路径、source插入/inputs置换、qualified IDs、诊断码/
Span和原子失败断言不变。capture失败保留closures记录但清空captures的既有区别、deferred
无diagnostics仍拒绝validate、nested loan各自call identity/end_span均原样保全。
无文件I/O、include、file!/line!/module_path!等物理位置依赖；22个既有support文件逐字不变。

前后完整Cargo metadata字节相等，142 targets、frontend132（lib1＋integration131），
新模块没有成为独立target。production/Cargo/CI输入与所有其他Rust文件未变；integration子模块
仍只从原test target私有可达。frontend library187个完整身份前后相等，未声称执行了这187项。

72个原叶名统一变为`域::叶名`；旧substring仍命中，旧裸根名`--exact`需用映射新名。
[current handoff baseline](unit-handoff-contract-baseline.md#既有24条身份基线)的F7/F8已同步。
archive SPEC-0214历史命令与结果不改写；其现行exact名为
`lambda_drop::lambda_value_parameters_publish_entry_read_and_transfer_drop_facts`，实际1/72通过。
新增identity不表示新增行为覆盖；仅数量相等不足以证明保真。

[只读复核器](evidence/verify_multifile_ownership_migration.py)重建原块及独立格式参考，检查
所有模块/私有入口、完整身份、literal/assert、support/Cargo/CI和唯一policy退休：

```sh
python3 docs/development/evidence/verify_multifile_ownership_migration.py
git diff --color-moved=zebra 481fb844 db1bb34
```

## CI接线与实际本地门禁

stage与Guide脚本各完整选择该target恰一次，均无名字filter；workflow通过既有Rust路径
选择两宿主门禁。只移动测试正文不会改变选择规则，本片不改脚本/workflow或policy测试。
合并批次PR仍须逐phase核查Ubuntu/macOS各72项完整新身份，不能以基底CI或clippy替代。

Linux x86_64、Rust/Cargo1.96.0、LLVM/Clang21.1.8；共享target、CARGO_INCREMENTAL=0、Cargo串行。

| 命令/范围 | 实际结果 |
|---|---|
| `cargo metadata --locked --offline --no-deps --format-version 1` | 前后完整JSON相等，142 targets |
| `cargo test --locked --offline -p lang-frontend --test multifile_ownership_checking -- --list` | 前后72，逐项一一对应 |
| 同target无名字filter执行 | 前后72 passed / 0 failed / 0 ignored / 0 filtered |
| 同一libtest executable直接执行 | 前后三轮各72 passed，默认测试并行 |
| `cargo test --locked --offline -p lang-frontend --lib -- --list` | 前后187完整身份一致，仅list |
| F7/F8新全名、lambda参数drop、capture原子清理，分别`-- --exact` | 各1 passed / 71 filtered |
| 旧`unit_non_null_assertion` substring | 5 passed / 67 filtered |
| `cargo fmt --all -- --check` | 通过 |
| `cargo check --locked --offline -p lang-frontend --all-targets` | 通过 |
| `cargo clippy --locked --offline -p lang-frontend --all-targets -- -D warnings` | 通过 |
| `python3 -m unittest discover -s scripts/tests -v` | 102/102通过 |
| `python3 scripts/check_rust_sizes.py --base 481fb844d62ea38851dc6ccdfb1531e3815b3445` | 724手写/47超千行；仅退休本片4692行项 |
| `python3 scripts/check_docs.py`、复核器、`git diff --check` | 通过；初次docs检查发现遗漏索引，补入口后重跑 |

## 受控compile+link、执行与RSS

测前固定复查阈值：after中位数相对before的build墙钟增长超过max(20%,1s)、峰值RSS超过
max(10%,65536KiB)、短执行墙钟超过max(30%,.020s)时核负载/freshness并追加配对样本。
本次均未触发；这是复查触发条件，不是性能SLO/统计置信界，不宣称提速或性能等价。

同一worktree、lockfile、features、profile、工具链与共享target，初次list预热不纳入样本。
每侧两次只touch integration入口，Cargo JSON确认仅本target nonfresh、library与依赖fresh；
每侧三次no-op与独立执行交替运行，未clean、未复制target、未清OS cache。

| 范围 | before墙钟s | after墙钟s | before峰值RSS KiB | after峰值RSS KiB |
|---|---|---|---|---|
| build | 1.364829 / 1.046826 | 1.137815 / 1.193663 | 500752 / 501152 | 502772 / 503456 |
| noop | 0.033935 / 0.036537 / 0.033035 | 0.035162 / 0.034989 / 0.036117 | 29624 / 29308 / 28176 | 29716 / 28180 / 29700 |
| execute | 0.095692 / 0.087948 / 0.087597 | 0.091117 / 0.085914 / 0.095024 | 13164 / 13588 / 13476 | 13836 / 13888 / 13976 |

每样本用独立Python进程subprocess.run、perf_counter与RUSAGE_CHILDREN.ru_maxrss；Linux KiB
为最大单子进程峰值，不是总RSS。依赖warm、非独占主机、小顺序样本有噪声；真正冷依赖/OS、
compile与link分段、二进制体积未测，不用于推导frontend全量预算。

## 未运行、交付与回退

本片未跑其他130个frontend integration、frontend library执行、workspace/codegen/native全量、
完整stage/Guide脚本、macOS或release tests；生产与门禁未改，0255固定基底的既有完整验收保留
其原范围，不因PR合并或文档变更重复本地执行，也不冒充新head/macOS结果。
独立设计与最终move-aware review均Approve；评审另以原文跨度逐块核对，并重跑复核器、
docs、尺寸与diff检查通过，未重跑Cargo。最终policy门禁已通过，远端终态留PR。
纯移动、文档/policy为两个回退单位；回退大文件须重审当时额度，不能恢复历史baseline绕门禁。
P2其他生产/测试职责、0182、P4/P5和整体计划后的外部审计继续保留。
