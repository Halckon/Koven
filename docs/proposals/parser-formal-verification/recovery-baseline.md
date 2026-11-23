# F0a：Parser 返工归因检查点

> **性质**：非规范研究账本 · **状态**：未启用，F0a 样本归因已完成 · **读取时机**：判断是否进入 F2 恢复结构原型前 · **唯一真源**：本页维护本轮候选集合、已归因批次与剩余证据

## 固定样本

目标 `6cc03907a1bf37d3cdc9ce6e6ccba1a15326ed6b`，现行 Guide v0.47。
旧评审基点 `a775568f`（提交日期 2026-11-10），目标基点提交日期 2026-11-18。
标题只用来重建旧样本；新批次按路径枚举，避免漏掉非 `fix(parser)` 标题。

```bash
git log --format='%H %s' --grep='^fix(parser)' a775568f -- crates/lang-frontend/src/parser
git log --format='%H %s' a775568f..6cc03907 -- crates/lang-frontend/src/parser
```

第一条得到旧样本 33 个。第二条得到增量 6 个，其中 4 个为 `fix(parser)`、2 个为 `fix(frontend)`。
合并后固定样本 39 个；只用 `fix(parser)` 会得到 37 个。这里计的是提交，不是缺陷或修复速率；
后续归因继续读取 diff 和用例，旧评审的 31/33 不能自动沿用。

## 已逐项复核：6 个增量提交

R1 边界归属、R2 推进、R3 资源、R4 词法诊断/错误交接、R5 后继保留沿用
[来源评审](../parser-grammar-verification-review.md)的恢复轨道。
合法语法分派另标为「句法选择」；不会因为改了 `Stops` 就自动计为恢复缺陷。
表中回归是提交新增的见证，不是断言已有矩阵原先必然能捕捉同一输入。

| 提交 | 主因／次因 | 调用边与观察 | 直接见证 | 仅靠分隔符 owner 区间能否避免 |
|---|---|---|---|---|
| `69b76b5b` | R4／R1、R5 | 调用实参缺分隔报错路径重复分类 lexer poison；下标将 Parser 诊断计数不变误当作键解析未损坏，从而错误认领嵌套 `]`。修复补查 lexer 诊断锚点和真实配对 | `parser_index_recovery::a_call_poison_does_not_hide_the_index_closer_behind_an_error_postfix`、`parser_standalone_poison_stress_matrix::call_separator_does_not_repeat_the_lexical_poison_diagnostic` | 只能限制 closer 读域；不能替代诊断交接或无新增 Parser 诊断的失败识别 |
| `d3cfeb18` | R1／R3、R4 | `if` 的 then 分支到 `else` 交接要保留 caller 软停止；相邻 poison 尾缓存避免反复扫描，并让错误仍归局部 | `parser_block_line_continuation` 的三个 `else` 专项；内部 `poison_before_else_keeps_linear_significant_visits`、`rejected_else_poison_runs_are_not_rescanned_from_each_statement` | 不充分；`else` 并非词法 closer，同一 owner 内还需要上下文边界及资源策略 |
| `a1dfb01e` | 句法选择 | `when` entry body 的 postfix 遇换行配对组时，需要判定下一选择箭头；仅看 `(` 会把后继 selector 当成调用，不能把所有换行都切断 | `parser_control_flow::grouped_when_entries_own_their_arrows_across_lines`、`nested_when_selectors_keep_their_own_postfix_calls` | 否；合法调用和后继 selector 都可能包含配对括号 |
| `e74fd827` | R3 | 将两个可失败的 postfix 边界检查移出递归解析帧；没有改变停止规则 | 提交自身未新增测试；合适的已有资产为 `parser_recursion_boundary_matrix` 和 `parser_stack_isolation_matrix`，当前基点两套共 5 项通过 | 否；限制 token 区间不能保证每个递归帧的栈成本 |
| `288d185b` | 句法选择／R1 | Pratt/postfix 和 `when` 条件探针共享 body 上下文；跟踪同层 selector 箭头，跳过内层调用、类型试探和其他 owner 的箭头 | `parser_control_flow::composite_grouped_when_selectors_keep_their_arrows`、`when_selector_probe_preserves_other_arrow_owners` | 不充分；配对是必要输入，箭头归属仍由语法上下文决定 |
| `bf3eb7f8` | 句法选择／R1 | `when` 探针按当前 body 区分裸 selector 和续行；`break`/`continue` 零操作数，不把后面内容扫描成其操作数；未配对组保留原恢复边界 | `parser_control_flow::grouped_when_selector_keeps_a_conditional_loop_jump_in_its_own_arm`、`unpaired_when_group_keeps_the_block_recovery_boundary` | 否；同一分隔符区间内仍须判定 jump 与 selector 的语法边界 |

## 合并归因与 F2 决策

[旧样本逐提交账本](recovery-history.md)与本页各覆盖 33 / 6 个唯一 SHA，均与 Git 枚举逐项核对。

| 样本 | R1 边界 | R2 推进 | R3 资源 | R4 词法交接 | R5 后继 | 句法选择 | 维护 |
|---|---:|---:|---:|---:|---:|---:|---:|
| 旧 33 | 23 | 0 | 1 | 5 | 1 | 2 | 1 |
| 增量 6 | 1 | 0 | 1 | 1 | 0 | 3 | 0 |
| 合计 39 | 24 | 0 | 2 | 6 | 1 | 5 | 1 |

表只计主因。R1 是最多的主因，但 **24 个 R1 不等于 24 个可由 owner 树消除的缺陷**。
39 个案例中没有一条能据现有证据判定「仅给子解析器一个物理分隔符区间即可替代整项修复」。
这个结论不是对任意区间化设计的不可能性证明。部分硬边界（如已证实的局部括号 closer）适合用视图约束，
但软边界、缺失或歧义 closer、角括号、Error 表示、栈成本与合法语法判定都不由物理区间推出。
增量样本说明仍有修复进入主线，不足以衡量修复速率趋势。

**决定：不启动 F2 多家族迁移。** F0b 先把已知反例映射到合同。只有在某个家族能证明「减少调用方
边界传递点、保留同组软停止、现有行为与工作量不回退」时，才建议另开 Spec 做 F2a 原型。
最窄候选（已证实配对的普通括号错误尾定位，`2f49f39d`／`1b6d63a5`）已由
[SPEC-0489](../../specs/active/0489-f2a-paired-paren-recovery-prototype.md) 完成调用边审计并触发停止条件：
保持生产 Parser 与 `LexedFile` 零改动，四类强制反例与线性扫描/事件计数已锁定为可执行见证。

## 旧开放项：先收窄，不关闭整类

[SPEC-0392](../../specs/active/0392-compiler-layered-review.md) §80 的三类旧问题在目标源码已有具体回归：

| 旧项 | 当前见证（只证明该例） |
|---|---|
| 无类体时保留下一个文件声明 | `parser_class_family::file_boundary::unfinished_file_class_header_preserves_the_next_root`；`header_recovery::invalid_supertype_slot_stops_at_the_next_file_declaration_without_a_body` |
| 分组委托内外逗号归属 | `parser_interface_delegation::grouped_invalid_delegation_does_not_promote_inner_tokens_to_supertypes`；`invalid_delegation_target_keeps_proven_inner_commas_out_of_the_parent_list` |
| 连续词法 poison 在原结构内可达 | `parser_interface_delegation::consecutive_lexical_errors_remain_in_the_invalid_delegation_target`；`parser_class_family::header_recovery::lexer_poison_at_a_supertype_slot_is_reachable_without_losing_the_next_entry`（后者覆盖单槽，不等价于所有连续 poison 组合） |

### 旧输入到当前见证的对应程度

- §80 记载的未闭合构造器吞 `public class Sentinel`：§81 明确记录修复；当前
  `unfinished_file_class_header_preserves_the_next_root` 使用 `class C(val x: Int\npublic class Sentinel`，
  验证两个根、消费范围及零宽诊断。关闭这个见证，不关闭所有构造器恢复。
- §80 的未闭合 interface 参数吞 `value class Sentinel`：当前 `file_angle_keeps_split_soft_prefixes_without_a_comparison_group`
  验证 `interface C<T` 后的拆行 `value\nclass Sentinel(val y: Int)`。语义相关但不是旧原始文本；
  §83 的另一个明确字符串 `interface C<T; public val sentinel = 2 > 1` 则与
  `file_type_parameters_preserve_an_explicit_separator_before_the_next_declaration` 的对应测试同文本。
- §80 未给出非法分组委托的原始源码。§81 接入的现行矩阵覆盖普通括号、方括号、
  lambda／插值与后继父类型，支持列出的形状；原始输入无法逐字核对，不作完全关闭。
- §82 的 `class C: I by β async, J {}` 和 `class C: async, J {}` 有现行字面见证，
  断言目标错误 marker 或 `TypeRef::Error` 的可达性及后继结构。
  §80 的连续 TypeParameter、嵌套父类型实参、函数型参数的完整原始组合未恢复；
  当前 `function_parent_parameters_keep_their_comma_after_lexical_errors` 等只是近邻覆盖，不把整个族标为完成。

### 当前基点实跑

规划工作树 `HEAD=6cc03907`，只有文档改动；macOS AArch64 / Rust 1.96.0。

```bash
cargo test --locked -p lang-frontend --no-fail-fast \
  --test parser_class_family --test parser_interface_delegation \
  --test parser_recursion_boundary_matrix --test parser_stack_isolation_matrix \
  --test parser_standalone_poison_stress_matrix -- --quiet
```

5 个目标共 **125 passed，0 failed、0 ignored、0 filtered**（88 / 30 / 4 / 1 / 2）。
未运行全量 frontend、内部操作计数或双宿主 CI；上述结果不证明所有旧失败组合已修复。

## 后继检查点

[F0b 结果分类与恢复验收基线](observation-baseline.md)记录四入口见证、R1–R5 映射、
普通括号候选反例及定向执行。原始输入待恢复的旧组合仍是证据缺口；没有新证据前不写生产改动。
