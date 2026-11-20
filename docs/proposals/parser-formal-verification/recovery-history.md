# F0a：旧样本逐提交归因

> **性质**：非规范研究账本 · **状态**：未启用 · **读取时机**：复核 F0a 的旧样本或选择 F2a 候选时 · **唯一真源**：本页维护旧评审样本的逐提交归因

样本、R1–R5 记号及增量 6 个提交见[归因检查点](recovery-baseline.md)。下表恰好覆盖 `a775568f`
路径限定的 33 个 `fix(parser)` 提交，按原 Git 输出倒序；不是按标题判因。每行保留一条代表机制及
同提交可复核的直接见证。测试名若随后迁移或改名，按对应 SHA 读取。不把当前矩阵当作修复前一定能失败的证据：右列只判断物理区间约束能覆盖哪部分，不证明新设计可行。
主因互斥；斜线后的次因可重叠。「部分」表示可减轻硬边界的传递负担，但仍需额外条件。

| 提交 | 主因／次因 | 调用边与修复机制 | 直接见证或证据缺口 | 仅有 owner 区间 |
|---|---|---|---|---|
| `d031ea14` | R1／R5 | Error receiver → paired postfix skip，带上 block 分号保护而不一律禁止局部配对后缀 | `parser_super_recovery::paired_error_postfix_leaves_a_proven_block_declaration` | 部分；要区分同组局部分号与外层语句起点 |
| `a8471402` | R1／R4、R5、R3 | 调用恢复 → 普通组跳转，只看游标之后的 caller barrier；相邻 poison 缓存保留自己的 owner | `parser_block_line_continuation::group_boundary::a_malformed_call_group_leaves_a_declaration_after_a_paired_nested_index` | 部分；同组分号和坏 token 归属仍需证据 |
| `a10a561e` | R1／R5 | raw call scan → 配对子块／下标，子块才遮蔽已确认的外分号，索引配对不授予该权限 | `parser_index_recovery::raw_call_recovery_leaves_a_block_declaration_after_a_damaged_nested_group` | 部分；需从 owner 类型与语境导出屏蔽规则 |
| `897b2f4b` | R1／R5 | block dispatch → call argument → TypeRef/index，传播已确认语句分号；后继并非语句时仍保留内部错误分号 | `parser_index_recovery::an_index_in_a_call_argument_leaves_the_enclosing_block_declaration`、同文件 `an_index_in_a_call_argument_keeps_an_internal_invalid_semicolon` | 不充分；同一对括号能包含两种分号情况 |
| `09f66d06` | R1／R5 | block → paired index/tail，分号候选用真正语句起点过滤，已配对局部花括号仍拥有内分号 | `parser_index_recovery::a_paired_index_comma_tail_leaves_recognizable_block_statements` | 部分；只知道 `]` 不知道语句从哪里恢复 |
| `1b6d63a5` | R1／R3、R5 | grouped Pratt → closer 跳转；仅同深度、当前游标后的 barrier 参与；唯一 `)` 可能是 caller 的 | `parser_block_line_continuation::group_boundary::ambiguous_group_closer_leaves_the_caller_separator`；内部线性计数 | 部分；需先为坏输入解决配对歧义 |
| `d7cc41c8` | R4／R1、R3 | unpaired index → 相邻 poison 尾扫描；只在确认 caller boundary 后接管错误尾，拒绝的长串可复用 | `parser_index_recovery::adjacent_poison_stays_with_the_unpaired_key_before_a_caller_comma`；内部 `unpaired_indexes_do_not_rescan_a_rejected_poison_run` | 不充分；缺 closer 时，错误尾归属还取决于后继证据 |
| `0031db7a` | R1／R5 | error postfix → declaration recovery，只有本地连续 typed-call 或配对且有显式 header 的 lambda 才能吞进恢复区 | `parser_super_recovery::only_a_local_contiguous_typed_call_can_hide_a_recovery_comma` | 部分；角括号及 lambda header 不能只靠词法配对 |
| `8bfc8d8d` | R1／R4、R5、R3 | index key → caller argument/interpolation，失败后不能把当前位置的任意 `]` 当作本层 closer；未配对保留外逗号 | `parser_index_recovery::comma_recovery_skips_the_bracket_in_a_nested_index`、同文件 `unpaired_index_returns_the_first_caller_comma` | 部分；真实 closer 的配对有收益，无 closer 仍需 fallback |
| `2f49f39d` | R1／R4、R3 | parenthesized expression → ordinary group index；以同词法区域配对保留内部错误，未配对交回下一文件声明 | `parser_block_line_continuation::paired_poisoned_group_is_local_to_its_interpolation` | 部分；这是适合尝试有界视图的局部边界 |
| `cc814f32` | R1／R3 | delegation lambda tail → 成对后继调用组；仍须证明后面存在 header 分隔才消耗可能属于类体的 brace | `parser_interface_delegation::unproved_delegation_lambda_continuations_keep_the_class_body_owner` | 不充分；类体与非法尾部 lambda 均可配对 |
| `0b3adb92` | R1 | named delegation suffix → lambda header/body 与 classifier body 分流；显式 header + 配对 + 后继证据 | `parser_interface_delegation::delegation_lambda_suffix_keeps_the_real_classifier_body` | 不充分；配对不证明 brace 的句法角色 |
| `dc735e3a` | R5／R1、R3 | file class type params → TypeRef unwind；恢复识别换行后的完整软声明前缀，共享 trivia 后首 token | `parser_class_family::file_boundary::unfinished_file_type_parameters_preserve_the_next_root_after_newline` | 不充分；缺 `>` 且软前缀需句法识别 |
| `844414c1` | R1／R5、R3 | accepted delegate name → unsupported tail；完整 member 前缀能回交，裸软词及伪 `deinit()` 留本层 | `parser_interface_delegation::unsupported_delegation_suffix_does_not_promote_a_destructor_shaped_call` | 不充分；同 owner 内的软词和成员起点需上下文 |
| `4fabef5e` | R1／R5 | invalid delegate target → shared suffix scan，保留已证明内组的逗号，未证明尖括号不能吞外逗号或文件根 | `parser_interface_delegation::invalid_delegation_target_keeps_proven_inner_commas_out_of_the_parent_list` | 部分；普通分隔符可约束，尖括号和 member 前缀另证 |
| `9b5c33ea` | R4／R1、R3 | delegation/父类型尾部 → 词法 poison 与组配对，独立延长错误区，复用惰性坏字符串标记 | `parser_interface_delegation::lexical_suffix_extends_an_existing_delegation_error_target`；内部字符串线性计数 | 不充分；区间不决定 Error 节点和诊断归属 |
| `781b1ab3` | R1／R4、R3 | poisoned type list → recovery-only type trial，损坏原子结果另存，不能授予严格 typed-call 准入 | `recovered_type_closers_never_authorize_a_strict_call_or_a_nested_owner`（trial 单测） | 否；角列表闭合与调用准入要分轨 |
| `225e8a94` | R1／R4、R5、R3 | supertype → TypeRef nested-element recovery，实参/函数参数拥有真实 delimiters，已确认文件分号交回 caller | `parser_class_family::header_recovery::function_parent_parameters_keep_their_comma_after_lexical_errors` | 部分；可隔离真实闭合，缺 `>`、分号和独立 Error 仍需契约 |
| `16bba7fe` | R1／R4、R5 | TypeRef completion → supertype suffix，完整/未完成的类型采用不同恢复；尾随词法错误独立保存并保住后继根 | `parser_class_family::header_recovery::a_proven_nested_suffix_group_cannot_claim_the_outer_supertype_boundary` | 部分；调用方边界和 TypeRef 完成态都必需 |
| `97cc36f0` | R4／R1 | valid delegation → trailing poison，保存目标名称与独立错误区域，外逗号不被未闭合角列表吞掉 | `parser_interface_delegation::closed_type_arguments_after_delegation_poison_keep_the_outer_comma` | 不充分；还需把正确名称和错误标记分开 |
| `e6e633d2` | R1／R4 | poisoned supertype head → strict type-list closed query，读取真实 `>` 但不授权后续调用 | `parser_class_family::header_recovery::an_unclosed_poisoned_type_suffix_leaves_the_next_supertype_available` | 否；尖括号不是普通词法 delimiter |
| `3443b3a2` | R1／R3、R5 | class file angles → per-opener ordinary/lexical pairing；只以当前组基线考察 `>`，前一文件根 opener 不参与 | `parser_class_family::file_boundary::comparisons_in_closed_groups_do_not_end_the_type_parameter_list` | 部分；局部配对有收益，仍须区分普通组内比较与类型 closer |
| `3150b46c` | R1／R5、R3 | class type list/TypeRef → file dispatch；同 `<` 基线确认分号与后继完整前缀，嵌套退栈共享判定 | `parser_class_family::file_boundary::file_type_parameters_preserve_an_explicit_separator_before_the_next_declaration` | 不充分；缺 `>` 时需要文件分隔策略 |
| `bf8e6750` | R4／R1 | invalid delegation marker → adjacent lexical recovery，将连续错误纳入既有 marker/子句范围 | `parser_interface_delegation::consecutive_lexical_errors_remain_in_the_invalid_delegation_target` | 否；问题是错误节点范围及归属 |
| `d64fbc4e` | R4／R5 | supertype slot → declaration recovery，将 invalid/reserved 创建为可达 `TypeRef::Error` 而不是仅丢弃输入 | `parser_class_family::header_recovery::lexer_poison_at_a_supertype_slot_is_reachable_without_losing_the_next_entry` | 否；区间不会自动分配错误节点 |
| `62c39d29` | R1／R5 | file dispatch → classifier constructor/TypeRef 与非法委托，缺真实 closer 才提升文件起点，成对坏组保留局部内容 | `parser_class_family::file_boundary::unfinished_file_class_header_preserves_the_next_root`；`parser_interface_delegation::grouped_invalid_delegation_does_not_promote_inner_tokens_to_supertypes` | 部分；这是区间候选，但未闭合路径需要软起点策略 |
| `d15fdc69` | R1／R5 | classifier members → declaration recovery，仅 owner/delimiter 回到基线才认 receiver 软起点；缺嵌套名不取父 `}` | `parser_class_family::header_recovery::member_recovery_preserves_a_contextual_receiver_only_at_owner_baseline`、同文件 `missing_nested_classifier_name_preserves_the_enclosing_closer` | 部分；可硬保留父 closer，仍需识别 `own borrow` 等前缀 |
| `91e6e413` | R1／R4、R5 | class header → constructor field/type params/supertypes/TypeRef，透传真实类体 opener 及外层 closer；保留非法默认 lambda 的局部 owner | `parser_class_family::header_recovery::unfinished_classifier_headers_keep_the_body_and_next_file_owner` | 部分；开 brace 的角色及诊断恢复仍依赖上下文 |
| `37ec9d43` | R1／R3、R4 | enum/companion → TypeRef 保留显式外分号；classifier/member 恢复纳入递归预算；委托字符串只消费词法 owner | `parser_class_family::enum_payload_keeps_its_outer_delimiter_during_nested_type_recovery`；`parser_recursion_boundary_matrix::classifier_and_companion_recovery_share_the_recursion_budget` | 部分；资源上限和词法错误交接不能由硬区间推出 |
| `934704b9` | 维护 | `class.rs` 只删除 companion 旁一行注释以满足尺寸基线；无 Parser 调用变化 | diff 无行为变化，无新增行为测试 | 不适用 |
| `bf51d586` | 句法选择／R1 | block element → Pratt/postfix 基于换行断开已完成表达式与下一个元素；group/call/index 清除相应软停止 | `parser_block_line_continuation::structural_breaks_separate_group_and_unary_starts_with_exact_spans` | 否；合法换行角色不由词法 group 决定 |
| `772b1f7f` | 句法选择／R4 | formal TypeRef 与 strict call trial 对齐 move/参数 mode 的上下文准入；失败试探不提交节点 | `parser_contextual_type_ref::nested_function_parameter_modes_agree_in_casts_and_typed_calls` | 否；相同 delimiter 可有不同合法上下文 |
| `d8ce3d78` | R3／R2 | block/lambda dispatch 重用当前 lexeme；分号直接推进，保留 caller closer 并避免重复探查 | `parser_block::semicolon_runs_preserve_empty_block_closer_and_eof_spans`、`parser_lambda::semicolon_runs_leave_inherited_hard_closers_for_the_caller` | 否；有界视图不保证访问次数或 dispatch 推进 |

后续做恢复矩阵映射时，结构侧先检查 `parser_token_omission_matrix`、
`parser_line_break_boundary_matrix`、`parser_lexical_owner_matrix` 与 `parser_lexical_poison_insertion_matrix`；
资源侧先检查 `parser_recursion_boundary_matrix`、`parser_stack_isolation_matrix` 和内部操作计数。
这些是候选验证入口，不是对每条历史修复的已证覆盖。任何新原型必须带能在旧机制下失败、
且能由区间约束本身阻止的最小见证，而不是只复跑已有全部绿测。
