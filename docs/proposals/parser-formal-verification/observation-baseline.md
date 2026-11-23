# F0b：Parser 结果分类与恢复验收基线

> **性质**：非规范验证账本 · **状态**：设计与现有见证映射已记录，四入口统一观察适配器与 `parse_file` 缺口补齐由 [SPEC-0486](../../specs/active/0486-parser-observation-adapter.md) 承接 · **读取时机**：实现观察适配器、归因恢复差异或评估 F2a 前 · **唯一真源**：本页维护当前基点的入口见证、R1–R5 测试选择及覆盖缺口

范围：Phase 1；`main@6cc03907a1bf37d3cdc9ce6e6ccba1a15326ed6b`、Guide v0.47。
本轮未实现统一观察适配器，不新增公开结果或游标。结果和实体定义见[数据模型](data-model.md)
与[验证合同](contracts/validation.md)；本页将它们映射到真实代码、现有预期和实际执行。

## 1. 四个公开入口的观察边界

四入口接收同一 `SourceMap + LexedFile`，均由 `parser/mod.rs` 分配独立 worker 栈。
`parser/engine.rs` 先校验词法产物，再运行入口及上下文验证，将词法与句法诊断合并排序。
`Ok` 内有恢复 AST 并不表示接受。公开 `Parsed*` 不暴露 end cursor：观察层用入口消费合同及
明确的尾部正反例作证，不从根节点 Span 补出一个 end。以后验证内部子入口须用测试内观测。

表中目标均在 `crates/lang-frontend/tests/`。预期来自既有断言或规范，不从当前 Parser 输出回填。

| 入口 | 成功边界（均须无失败诊断和可达恢复节点） | 正例见证 | 拒绝／恢复见证 |
|---|---|---|---|
| `parse_file` | 完整文件头与到 EOF 的有序根；空文件可无根；尾随合法文件分号与独立声明规则不同 | `parser_file::empty_and_trivia_only_files_have_no_roots_or_diagnostics`；`package_and_kotlin_style_imports_preserve_source_order_and_spans` | 同目标 `same_line_declarations_without_semicolon_report_the_next_starter_and_recover`：缺分隔有诊断、两根仍保留 |
| `parse_expression` | 单一 expression root；`consume_expression_tail(Stops::ROOT)` 处理多余输入，不能接受前缀后忽略尾部 | `parser_expression::shapes::associativity_is_encoded_by_child_direction` | 同目标 `recovery::parser_diagnostic_codes_messages_and_primary_spans_are_stable` 的 `a b`，尾 token 锚点 L0013 |
| `parse_declaration` | 单一 item root；`parse_declaration_root` 用 `consume_declaration_tail` 验证剩余输入 | `parser_declaration::variable_and_constant_payloads_preserve_typed_children_and_spans` | `parser_file::standalone_declaration_does_not_accept_a_trailing_semicolon`：不继承 file 分号许可；`standalone_declaration_keeps_its_trailing_token_contract` |
| `parse_block` | 单一 block statement root；缺 `{` 进入错误路径，已闭块后仍须核对尾部 | `parser_block::empty_and_nested_blocks_preserve_typed_roots_and_exact_spans` | 同目标 `completed_block_reuses_the_existing_trailing_token_diagnostic`：闭块后的尾 token 仍报 L0013 |

### 分类顺序与证据缺口

1. `Err(NestingLimitExceeded { limit })` → **ResourceLimit**。目前预算上限 1024 是实现保护，不是
   AST 高度或语言拒绝边界。四入口正反深度见 `parser_recursion_boundary_matrix`，worker 隔离见
   `parser_stack_isolation_matrix`（[F0a 基点实跑](recovery-baseline.md#当前基点实跑)）。
2. 其余 `ParserInternalError` → **InternalFailure**。保留具体变体；`ParserThread(kind)` 同时记录环境原因，
   `ParserThreadPanicked` 不归为资源超限。定向见证已有 expression、declaration、block 的 foreign SourceMap；
   file 建立于同一 `source_text` 检验，但 `parser_file::parsed_file_preserves_map_local_source_identity`
   不是 foreign-map 拒绝见证。统一适配器首片可补这一格；不要伪造线程失败实跑结果。
3. `Ok` 且含词法／Parser 错误诊断 → **SyntaxError**，保留诊断与恢复结构。无诊断却有可达 Error 或缺失
   边界属于产物合同失配，适配器应失败，而不是把无解释的产物记为普通语法拒绝或接受。
4. `Ok`、无失败诊断、根结构与位置合同满足、且明确有对应入口消费证据 → **Accept**。观察不足时该验收项
   保持 pending，不增加第五种生产结果。超时属于运行失败；不据此声称 `ResourceLimit`。Phase 2/3 错误不参与分类。

`tests/support/parser_test_assertions.rs` 可复用重复调用、根身份、AST 和诊断的现有检查；
`parser_entry_adversarial` 对表达式、声明、block 重复运行 768 个入口×输入组合，不含 file。
它验证产物不变量与确定性，不提供 768 个规范独立接受标签。Debug 对等只用于固定输入重复运行，
不作为跨文本 NormalizedSyntax 或完整的可达 Error 检查。

## 2. 恢复合同→见证矩阵

| 合同 | 验收观察与现有直接见证 | 本轮证据 | 不能外推 |
|---|---|---|---|
| R1 边界归属 | `parser_block_line_continuation` 的 paired malformed group 保留下一实参、ambiguous closer 保留 caller 逗号、不得越过文件声明或 `else`；`parser_block::nested_closer_belongs_to_the_innermost_owner` | 本轮集成实跑 | 只证明这些 owner 和入口，不能把普通括号结果外推到角括号／插值或所有 stop 组合 |
| R2 恢复推进 | file loop 对未推进显式失败；`parser_entry_adversarial`、`parser_lexical_owner_matrix` 观察所选输入能返回；内部 `declaration_recovery_visits_raw_lexemes_and_terminal_events_linearly` 计原始 lexeme 和 terminal event | 本轮集成与内部实跑 | 返回与计数是有限见证；没有覆盖每个循环的良基度量。后续命题要写明退栈、事件推进及退出条件，不能用超时宿主代替证明 |
| R3 资源 | 四入口嵌套边界／小 caller 栈；内部 `separator_tests` 测 block dispatch、配对括号/下标、poison 尾及文件角括号的访问数 | F0a 两套栈与边界；本轮 11 个内部测试 | 操作计数是所选形状上的增幅证据，不是全 Parser 复杂度上界或所有宿主内存承诺 |
| R4 词法错误交接 | `parser_file::lexer_poison_builds_error_item_without_duplicate_parser_classification`；`parser_expression::recovery::lexer_poison_is_not_duplicated_and_independent_parser_errors_survive`；`parser_lexical_owner_matrix` 跨词法 owner；`parser_diagnostic_witness_matrix` 为 Parser 码提供 lexer-clean 见证；F0a 回归验证委托连续 poison marker | 本轮集成及 F0a 实跑 | 诊断码有见证不等于每条恢复 AST 路径完备；原始组合缺失仍依 F0a 账本标记待核 |
| R5 后继保留 | `parser_file::missing_initializer_preserves_following_function_at_an_empty_boundary`；已配对 lexical owner 后仍保留下一顶层声明；F0a 的未闭合类构造器和接口参数相关见证 | 本轮集成及 F0a 实跑 | 单例保留不能给所有 soft declaration prefixes 授权；必须比较根和 Span，而不是只看诊断数量 |

## 3. F2a 普通括号候选：现有机制已挡住哪些反例

`parser/engine/recovery/ordinary_groups.rs` 的 `GroupClosers` 已按 opener 保存配对及边界归属；
`LexicalRecoveryIndex::ordinary_groups` 惰性建立，`confirmed_closer` 查相邻外层配对与游标之后的 barrier，
`skip_before_closer` 只在目标位于游标后时跳转，并把 closer 留给相应 group 消费。
因此，直接把已有 closer 下标包装成区间不能作为结构收益；新增缓存或预扫描还可能使资源合同退化。

| 原型必须区分的判例 | 当前可复核行为 |
|---|---|
| 明确配对局部 `(value, other)` 放进 `f(…, next)` | 组内逗号留在恢复区；`next` 仍为真实下一实参 |
| `f((value, next)` 只有一个 `)` | 不把唯一 `)` 强塞给 inner group；caller 仍有两个实参，错误范围前不消费它的逗号 |
| 能物理配对，但游标后有可能属于 caller 的文件起点／`else` | 不把整个物理组强行跳过；保留 `fun saved` 或相应 `else` 分支 |
| 多组退栈到同一拒绝位置、长词法 poison | 访问次数测量不能只数 cursor 前进，须覆盖查询及事件处理；已有 `separator_tests` 作基线 |

**F2a 评估已完成并正式暂缓（见 [SPEC-0489](../../specs/active/0489-f2a-paired-paren-recovery-prototype.md)）。**
对 `parse_group` 与 `GroupClosers` 的逐点调用边审计确认：即使对物理配对的普通括号，嵌套 `[ else next ]`、文件级声明与块语句分隔仍依赖动态 `outer_stops`，私有区间视图可缩减的调用方边界传递点为 0；因此保持生产 Parser 与 `LexedFile` 零改动，不启动 F2b–f 多家族迁移，并以 `separator_tests` 与 `parser_f2a_paired_paren_witnesses` 锁定上述全部四类反例与线性工作量/事件计数。

## 4. 本轮验证账本

规划工作树 `HEAD=6cc03907`，只有文档差异；macOS AArch64，固定 Rust 1.96.0。

```bash
cargo test --locked -p lang-frontend --no-fail-fast \
  --test parser_file --test parser_expression --test parser_declaration --test parser_block \
  --test parser_entry_adversarial --test parser_lexical_owner_matrix \
  --test parser_diagnostic_witness_matrix --test parser_block_line_continuation -- --quiet
cargo test --locked -p lang-frontend --lib parser::engine::separator_tests:: -- --quiet
cargo test --locked -p lang-frontend --lib \
  parser::engine::tests::declaration_recovery_visits_raw_lexemes_and_terminal_events_linearly -- --exact --quiet
```

- 集成：8 个目标、168 passed、0 failed、0 ignored、0 filtered。
- 内部：第一次冷编译完成后外层命令超时，未取得测试结果；复用编译产物明确重试，11 + 1 passed、
  0 failed、0 ignored，分别 898 / 908 filtered。不能说整个 `--lib` 已通过。
- 未运行全量 workspace、其他宿主或远端 CI。未实现统一适配器、尚无 F2a 原型。本轮基线执行时 D1–D3 尚未裁决。

**后继**：[F1 裁决卡](grammar-decisions.md)已整理每项入口、候选行为、正反例和版本影响；
D1–D3 已裁决；[Guide 修订草案](guide-amendment-draft.md)未启用。若优先扩展 DiffTest，则在独立 Spec 内选择不含 D1–D3 的构造。
