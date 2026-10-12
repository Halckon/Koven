# P2 codegen plan 私有测试搬迁验收

> **性质**：有界测试搬迁验收记录 · **状态**：本地结构与正确性已验；待独立review与Draft PR exact-head CI · **读取时机**：评审、复现或回退plan测试拆分时 · **唯一真源**：本页记录本片身份映射与实测，整体状态见[执行账本](engineering-governance-progress.md)

## 范围与固定源码

2026-10-02 从 PR24 合并后的 `main 5e9a2954536055e674eed515709d34a65b354e21`
建立 `feature/spec-p2-codegen-plan-tests`。本地纯搬迁commit为
`e8c9b622912b8cf48c23203ea03354db74f7edcb`，tree为
`daab6d1ffb7058476622596b8fa15ce1a7ba5a0d`；只修改八个私有测试文件。
文档与单项尺寸额度退休独立提交。GitHub连接发布后另核blob/tree/parent/fetch完整diff；
提交元数据可能改变SHA，远端配对及最终CI留在PR，不倒填本地SHA。

本片遵循[已批准计划](engineering-governance-plan.md)的P2顺序。
不改3061行 `unit_plan.rs` 生产实现、公开API、算法、语言语义、Cargo targets、依赖、workflow、
平台cfg或ignore；没有新增、去重或降低测试，不使用include拼接。不是P2整体完成。

## 私有领域与尺寸

路径相对 `crates/lang-codegen/src/ssa/`，按尺寸护栏的LF物理行口径。

| 文件 | PLOC | 领域 / 项数 |
|---|---:|---|
| `unit_plan_tests.rs` | 140（原3091） | 原imports和5个私有helper、一个私有alias、七个mod入口 |
| `unit_plan_tests/entry_identity.rs` | 133 | exact descriptor、entry/analysis身份与recovery禁止执行；3项 |
| `unit_plan_tests/instances.rs` | 240 | reachability、canonical实例、owner/callable实参与StaticSelf去重；4项 |
| `unit_plan_tests/delegation_routes.rs` | 742 | override、委托链端点、identity/参数重映射与route布局；13项 |
| `unit_plan_tests/owner_recipes.rs` | 535 | dispatch owner recipe、关闭不支持种类与继承参数重映射；5项 |
| `unit_plan_tests/recipe_cycles.rs` | 454 | 参数增长、SCC、helper间recipe与非cycle边界；8项 |
| `unit_plan_tests/error_order.rs` | 516 | owner字段、call、limit、specialization与稳定witness错误次序；10项 |
| `unit_plan_tests/layout_demand.rs` | 348 | instance-key-only/runtime demand、input-order与lambda/enum ABI升级；5项 |

原 `ssa/mod.rs` 的 `#[cfg(test)] mod unit_plan_tests;` 字节不变，七个子模块只经它可达。
子模块用 `use super::*` 访问原私有helper，无可见性提升。
parent仅追加私有 `use super::unit_lower;`，使recovery测试原有的
`super::unit_lower::lower_scalar_unit_with_entry` 在深入一层后仍指向原生产模块；测试体未改。
分组按语义跨原文件区间：例如input-order的runtime demand进layout，closed dispatch recipe
进owner recipes，具体StaticSelf实例进instances，不以连续区间机械切片。

policy仅删除 `unit_plan_tests.rs:3091` 这一退休baseline；其余47项、exceptions/generated逐值不变。
真实扫描612→619个手写Rust文件，超千行48→47；未重生成额度，也没有消除其余历史欠账。

## 测试身份的一对一映射

全部package=`lang-codegen`，target=`lang_codegen`，kind=`lib`；继承原cfg(test)。
没有额外平台cfg、ignore、should_panic、外部fixture或include路径。
每行定义旧名 `ssa::unit_plan_tests::叶名` → 新名 `ssa::unit_plan_tests::模块::叶名`。
旧模块filter仍命中48；叶名filter保留。旧完整名称加 `--exact` 不再匹配，需按本表增加模块段。

| 原叶名（不变） | 新模块 |
|---|---|
| `nested_runtime_layout_requires_the_exact_frontend_owner_descriptor` | `entry_identity` |
| `plans_only_cross_file_reachable_functions_and_deduplicates_recursion` | `instances` |
| `canonicalizes_generic_instances_and_is_input_order_independent` | `instances` |
| `merges_dependent_inherited_runtime_demand_independent_of_input_order` | `layout_demand` |
| `plans_reachable_member_instances_with_owner_and_callable_arguments` | `instances` |
| `plans_one_interface_default_instance_per_concrete_static_self` | `instances` |
| `plans_concrete_override_instead_of_abstract_requirement` | `delegation_routes` |
| `plans_delegate_implementation_from_validated_forwarder_route` | `delegation_routes` |
| `plans_same_requirement_delegation_chain_to_the_direct_implementation` | `delegation_routes` |
| `bodyful_requirement_chain_prefers_the_nested_route_over_an_inherited_default` | `delegation_routes` |
| `plans_identity_changing_delegation_chain_to_the_direct_endpoint` | `delegation_routes` |
| `remaps_identity_changing_next_hop_owner_prefix_and_callable_suffix` | `delegation_routes` |
| `nested_delegate_local_override_terminates_the_outer_route` | `delegation_routes` |
| `rejects_delegation_cycle_before_planning_a_partial_instance` | `delegation_routes` |
| `plans_parameter_independent_generic_delegate_field` | `delegation_routes` |
| `plans_parameter_independent_generic_outer_receiver` | `delegation_routes` |
| `plans_frontend_authorized_nested_generic_delegate_layout` | `delegation_routes` |
| `delegated_dispatch_owner_recipe_keeps_unsupported_nested_kinds_closed` | `owner_recipes` |
| `inherited_dispatch_owner_recipe_keeps_dependent_kinds_and_missing_canonical_closed` | `owner_recipes` |
| `remaps_generic_delegation_owner_prefix_and_callable_suffix` | `delegation_routes` |
| `plans_bodyful_delegation_from_frontend_effective_targets` | `delegation_routes` |
| `remaps_requirement_arguments_to_concrete_owner_and_callable_slots` | `owner_recipes` |
| `remaps_inherited_default_owner_recipe_and_callable_slots` | `owner_recipes` |
| `rejects_parameter_growing_recipe_at_stable_back_edge_across_input_order` | `recipe_cycles` |
| `rejects_parameter_growing_recipe_at_delegation_endpoint` | `recipe_cycles` |
| `rejects_generic_delegation_recipe_before_instance_limit` | `recipe_cycles` |
| `keeps_generic_delegation_local_override_out_of_recipe_failures` | `recipe_cycles` |
| `keeps_delegation_owner_field_error_before_endpoint_recipe_failure` | `error_order` |
| `rejects_generic_delegation_recipe_across_helper_before_instance_limit` | `recipe_cycles` |
| `rejects_generic_helper_recipe_before_instance_limit` | `recipe_cycles` |
| `rejects_fixed_argument_recipe_scc_before_instance_limit` | `recipe_cycles` |
| `keeps_earlier_unsupported_constructor_before_later_recipe_cycle` | `error_order` |
| `keeps_earlier_call_error_before_later_recipe_failure` | `error_order` |
| `keeps_instance_limit_before_concrete_error_in_generic_body` | `error_order` |
| `keeps_earlier_sibling_instance_limit_before_helper_error` | `error_order` |
| `keeps_earlier_helper_error_before_later_recipe_failure` | `error_order` |
| `keeps_first_specialization_error_before_later_specialization_recipe` | `error_order` |
| `keeps_declaration_frontier_before_earlier_source_symbol_recipe` | `error_order` |
| `rejects_later_sibling_recipe_before_current_instance_limit` | `error_order` |
| `selects_stable_recipe_root_across_limit_hit_siblings` | `error_order` |
| `keeps_closed_descriptor_unsupported_out_of_cycle_failures` | `recipe_cycles` |
| `remaps_list_inherited_owner_recipe_to_the_effective_default` | `owner_recipes` |
| `plans_dependent_inherited_owner_recipe_as_instance_key_only` | `layout_demand` |
| `upgrades_dependent_inherited_owner_recipe_to_runtime_layout_required` | `layout_demand` |
| `upgrades_dependent_inherited_owner_recipe_used_only_by_lambda_abi` | `layout_demand` |
| `upgrades_dependent_inherited_owner_recipe_used_only_by_enum_payload` | `layout_demand` |
| `rejects_non_callable_entries_and_foreign_ownership_products` | `entry_identity` |
| `recovery_owner_without_a_layout_cannot_become_executable_ssa` | `entry_identity` |

新旧完整 `-- --list` 均为730 tests / 0 benchmarks；恰好48项映射＋682项完整身份不变，
没有新增、丢失、重复。Cargo metadata完整JSON相等（同一worktree，无路径规范化），129 targets。

## 逐字保真与复现

原文件blob为 `3050baf36fec9d3b9369dfbb5aa272c1bce448a5`。
按叶名逐一比较包含 `#[test]`、签名和完整函数体的48块UTF-8字节，仅忽略块间空白：
48/48 SHA-256相等，136处assert宏、内嵌源码和错误/Span/实例身份字符串全部保全。
原imports和5个helper整体字节相等，header SHA-256为
`0f4a8139e9acc8e67e2fd75bc4221c781384cf2be4733b992879aaf8512515d3`。
按叶名排序，以LF连接48个hex块hash后再取SHA-256为
`ee1a4bb68b5aea633dfad0d47234d2ce2a586b6f10332185bed5073d14f7e631`。

在本片checkout根目录可复现完整块比较（该文件所有测试属性均为单独的test）：

```python
import hashlib, pathlib, re, subprocess
base = "5e9a2954536055e674eed515709d34a65b354e21"
p = pathlib.Path("crates/lang-codegen/src/ssa/unit_plan_tests.rs")
old = subprocess.check_output(["git", "show", base + ":" + str(p)]).decode()
def blocks(source):
    chunks = re.split(r"(?m)(?=^#\[test\]\n)", source)[1:]
    return {re.search(r"^fn (\w+)", b, re.M).group(1): b.rstrip().encode()
            for b in chunks}
a, b = blocks(old), {}
for f in sorted(p.with_suffix("").glob("*.rs")):
    for name, body in blocks(f.read_text()).items():
        assert name not in b
        b[name] = body
assert len(a) == 48 and a == b
header = old.split("#[test]\n", 1)[0].rstrip()
new_header = p.read_text().split("\nmod ", 1)[0]
assert header == new_header.removeprefix("use super::unit_lower;\n\n").rstrip()
print(hashlib.sha256("\n".join(hashlib.sha256(a[n]).hexdigest()
      for n in sorted(a)).encode()).hexdigest())
```

`git diff --color-moved=zebra <base> <move-commit>` 另用于人工move-aware审阅。
其他Rust/生产入口/Cargo manifests/lockfile/toolchain/workflow完整diff为空；hash与数量不替代实际执行。

## 本地门禁

Linux x86_64；Rust/Cargo1.96.0，LLVM/Clang21.1.8；LLVM_SYS指向LLVM21，
不将rustc自带LLVM22.1.2视为codegen后端。既有共享target、`CARGO_INCREMENTAL=0`，
Cargo全部串行；无clean、复制target或并发争锁。下表退出码均0。

| 命令 | 实际结果 |
|---|---|
| `cargo metadata --locked --offline --no-deps --format-version 1` | 前后完整JSON相同，共129 targets |
| `cargo test --locked --offline -p lang-codegen --lib -- --list` | 前后各730项，48项映射+682项不变 |
| `cargo test --locked --offline -p lang-codegen --lib unit_plan_tests` | 前后各48 passed / 0 failed / 0 ignored / 682 filtered |
| 直接libtest executable加 `unit_plan_tests` | 前后三轮各48 passed / 0 failed / 0 ignored / 682 filtered |
| `cargo fmt --all -- --check` | 通过，无需改变搬迁代码格式 |
| `cargo clippy --locked --offline -p lang-codegen --all-targets -- -D warnings` | 通过，17.012s |
| `cargo check --locked --offline -p lang-codegen --release` | 普通release静态检查通过，9.008s；不是release tests |
| `python3 scripts/check_rust_sizes.py --base origin/main` | merge-base明确5e9a295；619手写/47历史超限/0生成物，通过 |
| `python3 -m unittest discover -s scripts/tests -v` | 94/94通过，含47项尺寸policy黑盒测试 |
| `python3 scripts/check_docs.py`、`git diff --check` | 465 Markdown结构与whitespace通过 |

## 受控compile+link、执行与RSS样本

测量前约定异常复查阈值，以每组中位数比较after相对before：build墙钟增加超过
max(20%,1s)，峰值RSS增加超过max(10%,65536KiB)；短执行墙钟增加超过max(30%,0.020s)。
这只是本切片调查触发预算，不是统计置信区间、全项目性能SLO或已批准提速承诺。
若触发，先查依赖freshness/负载/命令范围，必要时补一对同范围样本，不删异常值。

控制同一worktree、Cargo.lock、features、Rust/LLVM、profile和共享target；原/新都使用
`cargo test --locked --offline -p lang-codegen --lib --no-run --message-format=json`。
第一次预热32.566s/2006604KiB，因新worktree重编frontend+codegen；依赖下载已缓存，
不是干净机器冷构建，不纳入前后比较。随后每次仅 `touch .../unit_plan_tests.rs` 强制该crate
重新编译并链接现有libtest target，两次/侧；四份Cargo JSON逐项确认只有lang_codegen nonfresh，
依赖全部fresh。touch不改变源码字节；不开增量缓存，不删已有产物或别人的target。

这是“依赖已热、codegen重新compile+link”的合计成本，不是no-op，也未分离rustc/linker子阶段。
每侧再串行交替三轮无touch的no-op与直接测试进程；后者通过JSON的executable字段取路径，
去掉Cargo启动开销，使用原 `unit_plan_tests` filter和libtest默认并行。
可见9个CPU、约9.7GiB内存；采样附近loadavg为0.55/0.26/0.10，无其他Cargo并发。
没有独占主机或限制其他宿主负载，OS page cache无法清冷；先before后after仍有顺序噪声。

| 范围 | before墙钟s | after墙钟s | before峰值RSS KiB | after峰值RSS KiB |
|---|---|---|---|---|
| build | 15.372558 / 14.433647 | 14.971850 / 14.637630 | 1565840 / 1557968 | 1530740 / 1543532 |
| noop | 0.066006 / 0.075153 / 0.074070 | 0.066632 / 0.073440 / 0.065817 | 30268 / 30336 / 30128 | 29952 / 29952 / 29820 |
| execute | 0.061597 / 0.055833 / 0.058643 | 0.062195 / 0.064528 / 0.058750 | 76896 / 76260 / 76724 | 77292 / 76284 / 76508 |

所有样本exit=0。四次build各约14–15s，与约0.07s的freshness/no-op明确分开；
独立48项执行约0.06s。本片预设复查阈值未触发，因此未追加消耗性重复。
样本量小、执行时间短且缓存/主机有噪声，不宣称提速、性能等价或没有任何退化。
真正冷依赖/冷OS cache、compile/link分段、二进制体积未测，也不据此整合Cargo targets。

复现采样时，先在专用worktree顺次checkout固定base与本片代码commit；只使用一个target，
每个状态先预热，然后两次touch+build、三次no-op与直接executable，保存完整JSON确认编译范围。
每条命令用新Python进程包装 `subprocess.run`，墙钟为 `time.perf_counter()` 差，退出后读取
`resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss`（Linux为KiB）；它是最大子进程峰值，
不是并发进程RSS总和。测量runner应由独立subprocess启动，避免父shell末尾exec继承累计rusage。
以下最小runner用新child进程完成该统计，再由外层等待；命令/stdout/stderr应另存以审阅：

```python
# 保存为measure.py：python3 measure.py cargo test ...
import subprocess, sys
child = """
import json, resource, subprocess, sys, time
start = time.perf_counter()
result = subprocess.run(sys.argv[1:])
usage = resource.getrusage(resource.RUSAGE_CHILDREN)
print(json.dumps(dict(seconds=time.perf_counter()-start,
                     rss_kib=usage.ru_maxrss, exit_code=result.returncode)))
sys.exit(result.returncode)
"""
sys.exit(subprocess.run([sys.executable, "-c", child, *sys.argv[1:]]).returncode)
```

## 未运行项与交付回退

- 本地未跑完整codegen/native、CLI、frontend全量、macOS、release tests或workspace check。
  私有结构变化无跨crate生产影响，48项定向与严格all-targets clippy是本地最小充分覆盖；
  完整已配置双宿主check/clippy/core/stage/Guide交PR exact-head CI，不能沿用base CI
- 独立review与Draft PR CI待后继；最终CI写PR正文，不为外部终态反复制造新head。
  保持Draft，不自行Ready、merge或auto-merge。0182仍独立active，外部审计继续按整计划完成后排队
- 纯搬迁八个Rust文件是独立回退单位；文档/policy为另一commit。若已合并，恢复3091行原文件
  须按当时base申请有界例外，不能复活退休baseline；不降低断言或新增ignore换取通过
