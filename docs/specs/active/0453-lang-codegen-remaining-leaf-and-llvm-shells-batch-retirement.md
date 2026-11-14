# SPEC-0453: `lang-codegen` 剩余同目录 `include_str!` 薄壳、`llvm` 子树同级模块与跨父测试小文件批量收敛（37 个）

> **性质**：正式开发规范（Spec） · **状态**：in-progress · **读取时机**：收敛 `crates/lang-codegen/src` 遗留壳子模块时 · **唯一真源**：`crates/lang-codegen/src/`

| 字段 | 值 |
|---|---|
| 编号 | 0453 |
| 标题 | `lang-codegen` 剩余同目录 `include_str!` 薄壳、`llvm` 子树同级模块与跨父测试小文件批量收敛（37 个） |
| 版本目标 | v0.46 |
| 状态 | in-progress |
| 创建日期 | 2026-11-12 |
| 完成日期 | - |
| 关联里程碑 | M2（借用系统）、M3（单线程完备） |
| 关联迭代 | E3（单下降管道收敛） |
| 关联 ADR | ADR-0006, ADR-0008, ADR-0032, ADR-0033 |
| 前置依赖 | SPEC-0381, SPEC-0383, SPEC-0451, SPEC-0452 |

---

## 1. 背景与动机

在 [SPEC-0452](0452-lang-codegen-non-ssa-leaf-and-sibling-shells-batch-retirement.md) 完成 `lang-codegen` 非 `ssa` 子树首轮批量收敛后，仍残留三类可安全收敛的薄壳文件：
1. 同目录下含相对路径 `include_str!` 的直接父子薄壳（3 个）；
2. `llvm.rs`、`llvm/adapter.rs`、`llvm/map.rs`、`llvm/runtime.rs` 及顶层 `*_cases.rs` 下的同级叶子模块（26 个）；
3. 最佳适配（Best-Fit）跨父测试宿主可容纳的 `ssa` / `llvm` / `native` 纯叶子测试模块（8 个）。

本 Spec 一次性批量收敛上述 37 个独立文件，使 `crates/lang-codegen/src` 下所有 `< 500` 行的可退场测试薄壳基本清零，手写 Rust 文件总数从 `1682` 进一步降至 `1645`。

---

## 2. 设计与收敛方案

### 2.1 收敛后父模块与宿主行高

| 模块路径 | 收敛后行高 |
|---|---:|
| `lib.rs` | 928 |
| `llvm.rs` | 939 |
| `llvm/adapter.rs` | 1764 |
| `llvm/adapter/call.rs` | 685 |
| `llvm/adapter/module_lowering.rs` | 900 |
| `llvm/adapter/range.rs` | 681 |
| `llvm/closure.rs` | 680 |
| `llvm/container_tests.rs` | 937 |
| `llvm/layout.rs` | 918 |
| `llvm/map.rs` | 851 |
| `llvm/map/mutation.rs` | 852 |
| `llvm/runtime.rs` | 1011 |
| `llvm/runtime/abort.rs` | 673 |
| `llvm/string.rs` | 719 |
| `native.rs` | 505 |
| `native/unit_const_handoff_contract_tests.rs` | 852 |
| `native/unit_constant_tests.rs` | 862 |
| `native/unit_container_size_tests.rs` | 567 |
| `native/unit_map_loop_owner_tests.rs` | 920 |
| `native/unit_map_return_tests.rs` | 637 |
| `native/unit_numeric_literal_tests.rs` | 640 |
| `native/unit_tests.rs` | 1924 |
| `native_borrow_value_field_cases.rs` | 712 |
| `native_map_provider_tests.rs` | 961 |
| `native_ordered_map_tests.rs` | 765 |
| `native_ordered_map_tests/fixtures.rs` | 954 |
| `native_sequential_for_tests.rs` | 920 |
| `native_sequential_for_tests/cleanup_tests.rs` | 842 |
| `native_tests.rs` | 1123 |
| `native_unit_source_test_support.rs` | 936 |
| `ssa/closure_operation_tests/escape_tests.rs` | 780 |
| `ssa/lower_frontend_owned_source_return_native_tests.rs` | 978 |
| `ssa/lower_frontend_owned_source_return_tests.rs` | 972 |
| `ssa/lower_frontend_tests.rs` | 979 |
| `ssa/map_provider_exits_pipeline_tests.rs` | 950 |
| `ssa/map_provider_exits_pipeline_tests/scope_future_observer.rs` | 972 |
| `ssa/map_provider_exits_pipeline_tests/scope_future_observer/facts.rs` | 978 |
| `ssa/map_provider_exits_pipeline_tests/temporary_delivery.rs` | 845 |
| `ssa/map_provider_exits_pipeline_tests/temporary_delivery/graph.rs` | 978 |
| `ssa/mod.rs` | 969 |
| `ssa/nullable_map_consume_lowering_tests/resource.rs` | 784 |
| `ssa/sequential_for_lowering_tests.rs` | 799 |
| `ssa/sequential_for_lowering_tests/cleanup_tests.rs` | 977 |
| `test_support.rs` | 746 |

### 2.2 退役薄壳清单（37 个）

- `bitwise_test_support.rs`
- `llvm/adapter/abort.rs`
- `llvm/adapter/borrowed_generate.rs`
- `llvm/adapter/callable.rs`
- `llvm/adapter/constant.rs`
- `llvm/adapter/integer.rs`
- `llvm/adapter/integer_conversion.rs`
- `llvm/adapter/integer_range.rs`
- `llvm/adapter/map_iteration.rs`
- `llvm/adapter/nullable.rs`
- `llvm/adapter/root_exchange.rs`
- `llvm/adapter/storage.rs`
- `llvm/adapter/text.rs`
- `llvm/adapter/text_copy.rs`
- `llvm/closure_tests.rs`
- `llvm/container_tests/provider_cfg.rs`
- `llvm/debug.rs`
- `llvm/entry.rs`
- `llvm/map/iteration.rs`
- `llvm/runtime/inline_nullable.rs`
- `llvm/runtime/map_result.rs`
- `llvm/runtime/narrow_tests.rs`
- `llvm/runtime/string.rs`
- `llvm/type_map.rs`
- `llvm/utf8.rs`
- `native/unit_companion_boundary_tests.rs`
- `native/unit_const_handoff_test_support.rs`
- `native/unit_map_break_tests.rs`
- `native/unit_map_continue_tests.rs`
- `native_local_object_constant_qualifier_tests.rs`
- `native_map_provider_scope_value_births_tests.rs`
- `native_multi_borrow_cases.rs`
- `native_nested_borrow_cases.rs`
- `native_sequential_for_tests/conditional_owned_source_tests.rs`
- `ssa/closure_operation_tests/escape_tests/projected_content_tests.rs`
- `ssa/field_replace_lower_tests.rs`
- `ssa/nullable_map_consume_lowering_tests/resource/runtime.rs`

---

## 3. 验收标准

1. 上述 37 个薄壳文件从物理树移除，全部非例外修改后 Rust 文件严格小于 1000 行。
2. `python3 scripts/gen_spec_dag.py`、`python3 scripts/check_docs.py` 与 `python3 scripts/check_rust_sizes.py --base main` 通过，手写 Rust 文件数从 `1682` 降至 `1645`。

## 4. main 集成修复检查点（未完成验收）

本节仅记录冲突修复，不改变原验收标准，不宣称 SPEC-0451–0453 或 E3 完成。
输入为 main `fbfb5a3a`、原始批次 tip `7451e9a3`、修复版前序批次 `1a30b51d`；
集成后的代码检查点为 `5f0e88d2`。旧的全选一侧冲突解决混合了不同模块布局；
本轮恢复有语义归属的 helper/test 子模块、移除跨域 packed 副本，保留原断言与 fixture。
其中 §2.2 的若干文件已恢复，原删除数量和 §2.1 行高不是当前验收结果，仍须重新对账。

| 检查 | 实际结果与边界 |
|---|---|
| 默认及 `loan-observation-test-support` 的 `cargo check -p lang-codegen --all-targets` | 均通过；后续两配置 Clippy 亦通过 |
| `cargo clippy -p lang-codegen --all-targets [--features loan-observation-test-support] -- -D warnings` | 两配置均退出码 0 |
| `cargo fmt --all -- --check`、`git diff --check` | 均退出码 0；格式化仅覆盖本批次已有失败的 codegen 文件 |
| `cargo test -p lang-codegen --lib --features loan-observation-test-support -- --list` | 3119 项；相对前序 3118 项，末尾名称多重集无缺项，仅新增 #407 nullable 分类用例；名称核对不能代替行为证明 |
| 全 feature lib 回归 | 1800 秒超时，部分 438 项通过、1 项失败，无最终退出码，不计通过 |
| feature SSA/LLVM 较广回归（`-- ssa:: llvm::`） | 1200 秒超时，部分 2016 项通过、46 项失败，无最终退出码，不计通过 |
| 上述 46 项失败集合原名重跑 | 修复 BorrowReturn 唯一来源 verifier 与 #407 分类 guard 后，46 passed / 0 failed，退出码 0 |
| 原子性子进程精确回归 | `native::unit_tests::boxed_enum_tests::for_atomic::actual_for_rejections_are_atomic_before_reservation_and_llvm`：1 passed；修正迁移后的 `--exact` 路径，保留确实运行 1 项的断言 |
| 受影响测试域定向回归 | nullable operation、closure operation、lowering entry、multi-borrow range、map stage gate、scalar ABI、BorrowReturn verifier、LLVM narrow/provider CFG、grouped/negated/leaf-not owner：feature 配置 120 passed / 0 failed；默认配置追加 #407 与原子性子进程测试，122 passed / 0 failed；均退出码 0 |
| 独立只读审查 | 核对 17 组 packed 正文恢复、挂载、fixture 与 #407 保留；未发现本轮尚未解决的具体问题，不替代全量与 CI 验收 |

剩余：完整回归及双宿主 CI、原退役数量与宿主职责重新对账。不得仅凭编译通过归档。

### 4.1 续接 #421 后的 37 项物理清单核对

输入检查点 `0cf1d025` 已包含 #421 冲突修复 `4e201f4b` 及 SPEC-0452 三个 owner
模块的同父内联。§2.2 的 37 个路径中 30 个不存在、7 个仍保留；不存在只表示路径删除，
不证明正文、fixture 与测试全部迁移。以下尺寸是该输入的实际物理行数；两者之和仅用于
排除直接内联，不作为某种压行或跨域打包方案的许可。

| 保留文件（相对 `crates/lang-codegen/src/`） | 子文件行数 | 原父模块行数 | 当前决定 |
|---|---:|---:|---|
| `llvm/container_tests/provider_cfg.rs` | 369 | 940 | 保留；直接内联超限 |
| `llvm/runtime/narrow_tests.rs` | 141 | 1014 | 保留；不扩大原父历史尺寸欠账 |
| `native/unit_const_handoff_test_support.rs` | 275 | 853 | 保留；继续服务原 const handoff 测试域 |
| `native_local_object_constant_qualifier_tests.rs` | 333 | 955 | 保留；不迁入无关 SSA 宿主 |
| `ssa/closure_operation_tests/escape_tests/projected_content_tests.rs` | 489 | 787 | 保留；继续使用原 closure 私有 helper |
| `ssa/field_replace_lower_tests.rs` | 494 | 983 | 保留；继续使用原 lowering 测试域 helper |
| `ssa/nullable_map_consume_lowering_tests/resource/runtime.rs` | 495 | 784 | 保留；Resource fixture 与 native oracle 不拆散 |

物理进度为 **30/37**，这 7 项不能简单并回原父模块，也不应为满足文件数量再次放入无关宿主。
本轮仅形成清单与阻塞记录，没有追加退役、改变 §3 验收标准或将本 Spec 标记完成。
后续先明确职责边界与尺寸目标的取舍，再实施新的布局；整批语义完整性、全量与 CI 验收继续待办。

### 4.2 三组 include_str 迁移审计与 companion 职责归位

本轮输入为集成检查点 `555694f8`，原始正文基线为 `2b3d624b`（原 0453 提交
`7451e9a3` 的直接父提交）。核对 `native/unit_companion_boundary_tests.rs`、
`native/unit_map_break_tests.rs`、`native/unit_map_continue_tests.rs` 三组迁移正文，
不是再次按路径数量推断内容保全。

审计排除 Rust 代码空白及注释，保留 raw-string 原字节，并按 Rust 规则折叠普通字符串
backslash-newline 续行；除 companion 迁移后的 `super` 层级适配外，三组 token 正文
均与原基线一致。七处 `include_str!` 分别仍解析到原 frontend final-admission 两个
companion fixture、codegen map-conditional-break 两个 fixture，以及
map-conditional-continue 三个 fixture；解析后路径和文件字节均与基线相同。
此核对保留实际源码、预期输出和断言，不能替代运行证明。

发现 companion capability/原子输出边界测试被挂在不相关的 container-size 测试模块。
恢复 246 行 `native/unit_companion_boundary_tests.rs`，与原基线文件逐字节一致；从
`native/unit_container_size_tests.rs` 删除旧内联副本，在 `native/unit_tests.rs`
将根级 re-export 改回直接模块声明。根接口仍为
`native::unit_tests::companion_boundary`，仅实际 libtest 路径去掉
`container_size_tests` 中间层。Fixture、两项测试、Span/拒绝种类、完整输出目录与旧目标
字节保持断言未改动；没有压行或移入另一无关宿主。break/continue 仍留在原 map-loop
owner 职责域，同名模块层级及正文均未调整。

| 验证 | 实际结果 |
|---|---|
| 修改前 companion feature 定向回归 | 2 passed / 0 failed / 0 ignored，退出码 0 |
| 修改后 companion 2 项、map break 5 项与 continue 7 项 | 默认及 feature 均 14 passed / 0 failed / 0 ignored，退出码均 0；companion 正控只发射 object，不链接运行；break/continue 保留真实 object/link/run 与 Unit basic/constant 控制 |
| feature 完整测试发现清单 | 前后均 3269 项，只有预期的两个 companion FQN 迁移，其他测试身份无变化；不等同执行全部测试 |
| 默认及 feature all-targets Clippy（`-D warnings`） | 均退出码 0 |
| fmt、文档、Rust 尺寸门禁 | 均退出码 0；container-size 宿主 569→319 行，独立 companion 246 行，原根宿主 1953 行未增长；旧尺寸欠账未清零 |
| 独立只读审查及账本复核 | 未发现 Rust 迁移的具体问题；发现当前物理清单未更新，追加本节后复核确认已解决，且明确 companion 正控仅发射 object。复核时提示文档门禁待执行，父会话随后实际执行成功；审查本身未运行 Cargo 或门禁 |

当前原清单物理删除进度为 **29/37**，保留文件增为八个：§4.1 七项加
`native/unit_companion_boundary_tests.rs`。0451、0452 的 21、2 项保留决定不变，
合计 31 个原目标路径保留。此次为恢复已要求的模块职责，不改 §3 原验收条件，不将
文件数量变化当成语义完成；其余既有迁移审计、完整回归及集成分支双宿主 CI 仍待验收，
本 Spec 保持 `in-progress`。

### 4.3 PR #423 P2：删除未接入的 Map break 校验副本

审阅输入为 `55a3a0fc24a8ac3df312822fb72515545a6ddf39`（base `51a3aad3`）。
`temporary_forms/graph.rs::structure` 无实际调用，且复制原 Map owner/loan/Drop
顺序断言；删除该 421 行模块，保留文件前 512 行的真实 `graph::check`。
原 `lower_frontend_map_break_structure_tests.rs` 及其两个直接调用点不变。
这是测试维护修复，不是已确认生产回归，不改变任何旧源码、golden 或断言。

默认/feature 的实际接线路径
`ssa::lower_frontend_tests::map_result_tests::loop_owner_tests::conditional_break::`
连同 temporary representations 原测试均 **12 passed / 0 failed / 0 ignored**。
初次误用文件名作 libtest 过滤器，只执行了 temporary 一项，不将其冒称 Map break
回归；随后核对清单并按真实路径重验。此记录不替代整个 #423 的候选 CI 验收，
也不改变既有 Spec 未决范围或状态。
本轮 fmt、docs、尺寸、空白及默认/feature 的 lang-codegen all-target Clippy
`-D warnings` 均通过；未运行无关全量，官方新候选 CI 不预填。

### 4.4 runtime inline nullable 析构职责恢复（有界一步）

输入为 `83191797d0c5dc42c632d9d0b9fb4c1ad2d9abb6`。发现 inline nullable 析构被内联在
`llvm/runtime/abort.rs`，再由 runtime 根用带 unused-import 例外的重导出接线。
恢复 `llvm/runtime/inline_nullable.rs` 为 runtime 直接子模块，删除这一重导出和例外；
不新增兼容 wrapper，不改同轮其余 map-result/string 打包。

机械审计确认去掉模块外壳/缩进及父路径导入调整后，函数正文、参数/返回与
`pub(crate)` 可见性不变，abort 其余字节不变。唯一 drop 分派调用与原测试挂载不变；
LLVM 条件分支、payload drop 顺序、符号与 void ABI 均未改变。独立只读复核无具体问题，
审查本身未运行 Cargo。

| 验证 | 实际结果及边界 |
|---|---|
| 迁移前 `inline_nullable` 定向基线 | 默认/feature 各 **12/0/0**，退出 0 |
| 迁移后相同选择 | 默认/feature 各 **12/0/0**，退出 0；包含 String niche 精确一次分配释放、Int/Boolean/Char 双边实际 native、MoveOnly tag 的借用读取与既有拒绝边界 |
| Clippy | 两配置 lang-codegen all-target `-D warnings` 通过 |
| 其他门禁 | workspace check、fmt、docs（951 Markdown）、尺寸、空白通过；尺寸门禁报告 2137 手写 Rust、36 项历史超限，无新增欠账 |
| 尺寸 | runtime 根 **1014→1013**、abort **674→627**、恢复模块 **44** 行；既有根超限欠账未扩大，不提高阈值 |

重新枚举 §2.2 的 37 个冻结路径：**28 不存在、9 保留**，相对 §4.2 增加本次恢复文件；
0451/0452 的既有 21/2 个保留路径不变，三批共 32 个原路径保留。数字仅是物理库存，
不是语义完成比例。本轮修复职责归属，不以任意打包凑删除数，也不改变 §3 验收条件。
全批合同与其余打包审计继续待办，Spec 保持 in-progress；新候选 CI 单独验收。
