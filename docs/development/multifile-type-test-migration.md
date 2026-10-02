# P2 multifile type integration分组验收

> **性质**：有界迁移验收 · **状态**：本地验收通过，独立review / Draft PR CI待后继 · **读取时机**：复核本片测试身份、源限定事实、CI与成本时 · **唯一真源**：本页与逐项证据；范围从属[整体计划](engineering-governance-plan.md)

## 范围、基线与非目标

固定main `b36d5040ef7cafaa86e2b0c6a42cc1d2ab49691d`（PR27合并）。纯移动commit
`1ebb3ac9124419def3cb1745ca562be58475eb03`；文档/policy/evidence另独立提交。
这是批准计划P2的大integration后继片，不新增Spec、语言语义、依赖、生产API或通用测试框架。

- `tests/multifile_type_checking.rs`7202→160 PLOC，保留原Cargo target `--test multifile_type_checking`
- 104个根级测试按17个类型领域分组；原`baseline_regressions.rs`209行/3测试逐字不动，共107项
- 13 helpers完整保留：9个跨域helper留入口，4个只在单域使用的helper随域；不去重、拆case或压行
- 新文件只由显式path私有模块加载，没有新binary/pub、include拼接、生产Rust、Cargo、CI、platform cfg或ignore变化
- source-qualified IDs、输入置换、诊断code/Span、负例、overload trial rollback、deferred recovery和runtime能力边界均保留

## 领域与helper归属

| multifile_type_checking/模块 | PLOC | tests | 域内helper |
|---|---:|---:|---|
| `assignments.rs` | 463 | 6 | `assignment_expressions` |
| `body_recovery.rs` | 503 | 10 | `symbol_for_expression` |
| `control_flow.rs` | 732 | 9 | `if_expressions` |
| `delegation.rs` | 363 | 5 | 无 |
| `destructuring.rs` | 241 | 2 | `destructuring_statements` |
| `dispatch_receivers.rs` | 500 | 8 | 无 |
| `expression_tails.rs` | 315 | 5 | 无 |
| `external_calls.rs` | 504 | 4 | 无 |
| `generics.rs` | 509 | 5 | 无 |
| `initializers.rs` | 419 | 6 | 无 |
| `intrinsic_box_rc.rs` | 500 | 10 | 无 |
| `intrinsic_containers.rs` | 444 | 8 | 无 |
| `lambdas.rs` | 178 | 2 | 无 |
| `members.rs` | 317 | 3 | 无 |
| `nullable_flow.rs` | 509 | 11 | 无 |
| `runtime_layout.rs` | 266 | 3 | 无 |
| `source_construction.rs` | 330 | 7 | 无 |
| `baseline_regressions.rs`（原有） | 209 | 3 | 无 |

每个完整case和helper按原文保全；最大case231行，没有需要新增例外的完整场景。
初轮编译指出原baseline回归也使用`when_expressions`；该helper因此继续留共享入口，
没有扩大可见性或改动回归文件。最终全target及严格门禁均通过，早期编译失败不计作验收通过。
尺寸policy仅退休原7202行baseline，其余45项baseline、三个既有例外和generated逐值不变。
实际658→675个手写Rust文件、49→48个超千行；生产职责欠账与P2整体仍待后继。

## 逐项保真与身份

package=`lang-frontend`、target=`multifile_type_checking`、kind=`test`。120个完整函数块
（107 tests＋13 helpers）逐字相等，也等于从固定Git原文分别生成的Rust1.96.0 rustfmt参考。
没有相对路径token或格式token编辑；1559个字符串/字符literal、781处assert与全部107 test属性保留。
literal按块hash再按名称排序的SHA-256 aggregate：
`255eb559f89401b55a7480e6f0261ca5e40412cca2a17e15afd41904a4c06f5b`。

[机器账本](evidence/multifile-type-test-migration.json)记录107个新旧完整身份、120块起止行/hash、
literal/断言数量、全部尺寸、22个support与Cargo/CI输入hash、命令与测量原值。
104个根级叶名改为`领域::叶名`，原3个baseline完整名不变；前后list逐项映射，无filter各107/107。
完整Cargo metadata JSON前后相等，仍129 targets；当前frontend library187个完整身份逐项不变，
并实际执行187 passed / 0 ignored / 0 filtered。数量相等不是保真的充分条件。

[只读复核器](evidence/verify_multifile_type_migration.py)重建独立同版格式化参考并检查所有块、
入口声明、路径映射、literal/断言、support/Cargo/CI与唯一Rust变更范围，以及唯一policy退休：

```sh
python3 docs/development/evidence/verify_multifile_type_migration.py
git diff --color-moved=zebra b36d504 1ebb3ac
```

旧叶名substring仍非零命中；旧根级`--exact`须使用新完整名，不能把零命中当通过。
SPEC-0214唯一记录的本target历史exact命令旁补当前`lambdas::`映射，原命令及结果不改写；
新exact实际1 passed / 106 filtered。`runtime_layout::runtime_field_layouts_are_atomic_on_typed_recovery`
亦exact1通过。递归Rust尺寸扫描已发现新文件；现有Guide与其他迁移扫描器不读取本target正文。

## CI与实际本地门禁

该target已在`check_stage_integration.sh`的完整target选集中；CI双平台`Targeted Tests` job
运行此脚本，没有名字filter。现有接线足够，本片不改workflow、脚本、policy tests或矩阵。
最终必须检查本PR exact-head的Ubuntu/macOS日志各107项逐名执行，不能用compile/check/clippy或基底CI替代。

Linux x86_64、Rust/Cargo1.96.0、LLVM/Clang21.1.8；共享target，CARGO_INCREMENTAL=0，Cargo串行。

| 命令/范围 | 实际结果 |
|---|---|
| `cargo metadata --locked --offline --no-deps --format-version 1` | 前后129 targets完整JSON相等 |
| `cargo test --locked --offline -p lang-frontend --test multifile_type_checking -- --list` | 前后107，完整身份一一对应 |
| 同target无名字filter执行 | 前后107 passed / 0 failed / 0 ignored / 0 filtered |
| 同一libtest executable直接执行 | 前后三轮各107 passed，默认测试并行 |
| `cargo test --locked --offline -p lang-frontend --lib -- --list` / `--lib` | 前后187身份一致；187/0实际通过 |
| 新lambda与runtime layout完整名加`-- --exact` | 分别1 passed / 106 filtered |
| `cargo fmt --all -- --check` | 通过 |
| `cargo check --locked --offline -p lang-frontend --all-targets` | 通过 |
| `cargo clippy --locked --offline -p lang-frontend --all-targets -- -D warnings` | 通过 |
| `cargo check --locked --offline -p lang-frontend --release` | 普通release check通过，非release tests |
| `python3 -m unittest discover -s scripts/tests -v` | 96/96通过 |
| `python3 scripts/check_rust_sizes.py --base origin/main` | merge-base b36d504；675手写/48超限，无新例外 |
| `python3 scripts/check_docs.py`、复核器、`git diff --check` | 通过 |

## 受控compile+link、执行与RSS

测前固定调查阈值：after中位数相对before的build墙钟增长超过max(20%,1s)、峰值RSS超过
max(10%,65536KiB)、短执行墙钟超过max(30%,.020s)时核负载/freshness并追加配对样本。
这些是复查触发条件，不是性能SLO或统计置信界。此次均未触发，不宣称提速、性能等价或无任何退化。

同一worktree/lockfile/features/profile/工具链/target。基线list预热19.308s（含frontend重建）和
迁移后list1.574s不纳入配对。每侧两次只touch integration入口，Cargo JSON确认仅本target nonfresh，
frontend lib和依赖均fresh；每侧三次no-op与独立执行交替运行。未clean、未复制target或清OS缓存。

| 范围 | before墙钟s | after墙钟s | before峰值RSS KiB | after峰值RSS KiB |
|---|---|---|---|---|
| build | 1.571157 / 1.496477 | 1.523503 / 1.562142 | 511692 / 511388 | 512712 / 511816 |
| noop | 0.039355 / 0.031783 / 0.034642 | 0.033817 / 0.032918 / 0.044412 | 29844 / 28176 / 29680 | 29476 / 29364 / 29404 |
| execute | 0.093542 / 0.095049 / 0.095448 | 0.092261 / 0.099995 / 0.095910 | 14628 / 15056 / 14676 | 15088 / 14772 / 15096 |

每样本由新Python进程subprocess.run；perf_counter墙钟，RUSAGE_CHILDREN.ru_maxrss为Linux KiB，
表示最大单子进程峰值而非总RSS。build约1.5s、no-op约.03–.04s、执行约.09–.10s分开报告。
依赖warm、非独占主机、顺序小样本包含噪声；真正冷依赖/冷OS、compile/link分段和二进制体积未测。
该样本不能用于合并其他Cargo targets或推定frontend全量预算变化。

## 未运行、交付与回退

本地未跑其他120个frontend integration、workspace/codegen/native全量、macOS或release tests。
新head双平台CI交Draft PR逐名核验；独立review与远端终态留PR，避免仅追记外部状态制造新待验head。
纯移动与文档/policy为独立回退单位；回退已退休大文件时必须按当时base审阅额度，不恢复历史baseline绕过。
P2后续大integration与生产职责仍未完成，0182继续active；P3/P4/P5未自动完成，整体计划后外部审计继续排队。
