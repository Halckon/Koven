# v0.40 演进计划实施账本

> **性质**：变更进度索引 · **状态**：current · **读取时机**：继续演进计划或核对交付范围时 · **唯一真源**：对应 Guide、Spec 与代码测试

本页对照 2026-10-01 的 13 项计划记录截至 main `34189046319a8b727285d471596647d5de56996e` 的实现与交付事实；语言规则以唯一 current
[Guide v0.40](../guide/README.md) 为准，不以 AST、类型名或文档启用代替端到端证据。
起点为 main `d3e64a4`；七阶段合并提交 `ed0727f` 形成真实 v0.39，随后整合 SPEC-0235。
八个切片及整合合同保留各自验收账本。PR #7 已于 2026-10-01 合并，后继切片基于真实 main `e22e11b`；此前文档闭环逐项补最终证据后归档 0228–0235、0237–0242 与 0247；本批0236诊断Span新增断言双宿主通过后归档，0182当时保留active；本机恢复的有界归档与未覆盖项见[恢复验收](../development/recovery-local-delivery.md)。
后到的 main `3be83b5`（已合并 PR #6）另作[最小协调](../archive/migrations/v0.40-upstream-pr6-reconciliation.md)，
整合 PR CI 后的独立核查与更正见 [SPEC-0238](../archive/specs/0238-guide-litmus-gate.md)及
[当前更正账本](../architecture/guide-conformance.md)；不重写已冻结的真实 v0.39。

## 八阶段已整合范围

| 切片 | 已有成果 | 明确保留边界 |
|---|---|---|
| [SPEC-0229](../archive/specs/0229-extended-numeric-literal-values.md) | radix/underscore 统一解码、单/unit 定型与常量、索引 identity、整数 SSA/native | 浮点 native、具名位运算与 inv 不在此切片 |
| [SPEC-0230](../archive/specs/0230-recursive-boxed-enum-native.md) | 非泛型递归 Box enum 单/unit 构造、运输、真实分配/递归释放计数 | generic enum、拆箱/解引用、nullable/Rc 递归包装与前向 case 查找保持边界 |
| [SPEC-0231](../archive/specs/0231-contextual-type-ref-trials.md) | 上下文 TypeRef、nested 函数模式、strict typed-call trial 与回滚一致 | 调用处取消 Borrow marker 与三个旧 call-argument 失败由 SPEC-0242 后续切片验证，不改写本切片历史验收 |
| [SPEC-0232](../archive/specs/0232-ownership-primitive-type-facts.md) | replace/swap 稳定 intrinsic、交换类型、源码顺序 operand identity、事务与结构验证 | 后继 SPEC-0244 已闭合 owned mutable whole-root 的 ownership/SSA/native；一级字段 SPEC-0246 已独立完成有界本地与首轮双平台 CI 验收并归档；其他投影/Inout ABI 不据此扩大 |
| [SPEC-0233](../archive/specs/0233-parser-compiler-contracts.md) | 九段原文迁入 Compiler Contracts，两页唯一索引、递归门禁与预算 | 渐进拆分首片；混合语义/诊断/Span 段保留 Guide，不声称全部分离 |
| [SPEC-0234](../archive/specs/0234-block-newline-continuation.md) | 普通/control/nested block 的 Pratt/postfix 换行边界、for-header delimiter 修复及矩阵 | 同行缺分隔符、前导/重复分号、lambda 顶层尾表达式范围另列 |
| [SPEC-0236](../archive/specs/0236-explicit-string-clone.md) | String.clone 单/unit typed→loan/drop→StringClone SSA→Linux native；heap/static/empty 与真实分配释放计数 | 原合同诊断 Span 精确 oracle 已经PR17首轮双宿主各14项实际通过并归档；归档提交最终CI见PR；Borrow Rc<String>.value.clone、inline-nullable String 和通用 clone 未扩张 |
| [SPEC-0235](../archive/specs/0235-approved-language-rules.md) | 真实 v0.39 完整归档；v0.40 唯一入口启用三项批准规则，保留 clone-first | 启用合同已随 PR #7 合并且最终双宿主 CI 通过；三项规则的具体实现与非目标仍分别验收 |

合并后交叉用例、统一 target 的本地门禁、实际命中数及剩余失败统一记录于
[SPEC-0237](../archive/specs/0237-local-integration.md)，不把上表各分支旧结果冒充最终整合结果。

## 十三项实况

| 项 | 状态 | 已有证据与未闭合边界 |
|---|---|---|
| 1. 块换行与分号 | 主要续行切片已落地 | SPEC-0234 覆盖完整左式后 call/prefix 分隔、未完成操作数/delimiter 续行；同行缺分隔诊断与独立 lambda 实施仍未闭环；上游 PR #6 已将 lambda 换行/分号尾表达式写入规范 |
| 2. 上下文关键字 | TypeRef/trial 切片已落地 | SPEC-0231 覆盖 move 普通类型名和 nested 模式；SPEC-0242 已移除调用 Borrow marker，直接 parser 31 项通过；与 0240 的最终整合 head `99e63d5` 双宿主 CI 已通过，PR #9 已合并；编辑器完整 corpus 的历史缺口仍保留 |
| 3. 数值与具名位运算 | 数值与位运算有界验收完成并进入 main | SPEC-0229 闭合字面量；[SPEC-0240](../archive/specs/0240-integer-bitwise-execution.md) 接入六操作 const/SSA/native 与 inv 稳定身份。移位按自身位宽屏蔽且保持两 operand 同型；inv 仍不在 const call 白名单，既有投影边界不扩大 |
| 4. Box enum | 受限 native 已落地 | SPEC-0230 覆盖具体非泛型递归构造/运输/析构计数；Box.value/unbox 已由上游 PR #6 写成后继 staged 合同，尚无对应实现证据；generic、nullable/Rc 递归包装及前向 case 查找不在已支持范围 |
| 5. replace/swap | owned root 与有界一级字段完成最终整合验收并进入 main | SPEC-0232 提供 typed 身份；SPEC-0244 已归档进入 main。SPEC-0246 在原 main `8eb2cd3` 上完成直接字段验收与归档，global/captured 与 paired target 身份已闭合；[PR #13](https://github.com/Halckon/Koven/pull/13) 最终 head `c5507a5` 已与 `e6e1100` 资源能力整合，8 项资源字段 native 交叉用例及双宿主 CI 通过，并合入 `efc52b6`。先前归档 head 因冲突未触发 PR CI 是历史事件，不作为最终验收；nested/index/field swap/Inout ABI 不扩大 |
| 6. 两阶段 receiver 借用 | SPEC-0243 有界验收完成并归档 | 两入口发布 reservation/CallEntry activation，保持 callee Borrow 冲突；45 项直接前端与 6 项 SSA/native 正反例通过。PR #10 初始 head 双平台 CI 全绿，各宿主 stage 742 / Guide 201 通过，随后已合并进入 main。single instance receiver、unit field/index native 既有边界不扩大 |
| 7. deinit 双轨析构 | SPEC-0245 有界完成并归档，已进入 main | 两入口 descriptor/递归分类与资源词法清理已接，single/unit native 验证 body→字段逆序、控制退出与唯一回收；该切片合并 PR #11 后本地 634 codegen、62 targets/793 stage 通过，PR #12 初始 head 双平台 CI 通过，现已被用户合并。与 SPEC-0246 新组合的验收另记，不复用独立通过；资源 wrapper、条件 outer-owner 运输边界见[实现事实](../architecture/resource-deinit.md) |
| 8. 静态 Str | 明确延后 | 字面量/const 仍为 MoveOnly + Transferable String；Str/toString/混合文本操作未启用。先行 String.clone 已有 SPEC-0236 全链路定向证据 |
| 9. 二等借用与 Escapable | 未实现 | 仍为 owned 值与调用期 loan；Ref/InoutRef/Span/StringView、来源和逃逸需新规范 |
| 10. inout/once closure | 新增部分未实现 | 现有函数指针+具体 inline 环境未新增 mutable/once callable；栈借用/堆逃逸分层待 Guide 与取代 ADR-0009 的决定 |
| 11. 受控 unsafe 与 RawPtr | 未实现 | Parser 无 unsafe/extern 产生式，类型环境无 RawPtr；权限来源与 C ABI 类型矩阵尚未封闭，不从方向性计划补规则 |
| 12. 双层文档解耦 | 首片已落地 | SPEC-0233 九段工程合同独立真源与检查完成；grammar/诊断/Span/Phase 等保留 Guide，后续按清晰边界渐进迁移 |
| 13. 十二 Litmus | 12 例前端正向，Litmus4单文件与Litmus12双入口native | SPEC-0238/0240/0241直接提取Guide；部分typed仍有精确快照缺口，4的unit enum condition/direct-case argument仍未支持；不宣称全套native验收 |

### 主要代码与测试入口

- Parser：frontend `src/parser/engine/`；`parser_contextual_type_ref`、`parser_block_line_continuation`、
  `parser_entry_line_break_boundary_matrix`、既有 Parser matrix/资源测试
- 数值/Box/clone：frontend `numeric_literals`、`string_clone`；codegen 的 numeric literal、boxed enum、
  string clone SSA/verifier/native 测试；两种入口与分配/释放计数分别留证
- 原子置换：frontend `type_ownership_primitives`、`ownership_primitives` 与 root validation，
  codegen root lowering/verifier/native 已有独立执行证据，见[root 专页](../architecture/root-ownership-primitives.md)；
  `ownership_field_replace` 与 field exchange 直接测试已有 SPEC-0246 有界验收，typed descriptor
  仍不能单独代替 root/field ownership commit 能力
- 文档：Compiler Contracts、`scripts/check_docs.py`、`scripts/tests/test_check_docs.py`

上述 suite 名按各 Spec 的完整命令定位；详细实现边界由 [Architecture](../architecture/README.md)维护。

## 最终交付核验（2026-10-02）

本表是本页所述实现的交付依据；每个 Spec 的最终节保留 Goal 映射、命令与原始失败历史。
CI 只证明实际选择范围，不是 frontend 全量、全部语言功能或 macOS LLDB 通过。

| 范围 | 最终 PR head / 合并节点 | 已核验 CI |
|---|---|---|
| 0228 | PR #5 `53bc552173db3ba2883e6493f11cd94a17a8d91b` → `d3e64a4` | [36846162707](https://github.com/Halckon/koven/actions/runs/36846162707)，6/6 success；当时 macOS，Linux 原本地证据另记 |
| 0229–0239 | PR #7 `11051e200441a21cdf6dee6a6d153d2e9ffe26c6` → `e22e11b` | [36877486546](https://github.com/Halckon/koven/actions/runs/36877486546)，8/8 success，双宿主实际执行 |
| 0240 | PR #8 `57a181695f4408378140fae1cea40caaa396bfae` → `8f3e460` | [36884583273](https://github.com/Halckon/koven/actions/runs/36884583273)，8/8 success |
| 0240/0241/0242 最终组合 | PR #9 `99e63d51854dbade0304dd8c86339183031dae06` → `2ad6967` | [36952398650](https://github.com/Halckon/koven/actions/runs/36952398650)，8/8 success |
| 0246 与资源整合 | PR #13 `c5507a51e0a630bb888a95b2f30247e09e1f60c5` → `efc52b6` | [36975900096](https://github.com/Halckon/koven/actions/runs/36975900096)，8/8 success |
| 0247 | PR #14 `d333e41441dcb2b9b34f58b3ae5251fe75d046a0` → `3418904` | [36978695764](https://github.com/Halckon/koven/actions/runs/36978695764) 与 [main 36979753900](https://github.com/Halckon/koven/actions/runs/36979753900)，各 8/8 success |

0243–0246 已有归档正文不重新打开或重写。本次归档文档自身的 PR 按现行 docs-only
过滤验收；Rust matrix 合法跳过不等于重新执行上述双平台历史检查。

## 已知独立基线失败

2026-10-01 在任何 SPEC-0229 生产修改前实际运行两个套件：

- `parser_call_argument`：25 passed / 3 failed。失败为
  `call_only_ampersand_is_not_a_prefix_operator_or_lexer_error`、
  `l0033_expected_value_preserves_committed_prefix_and_empty_error_value`、
  `empty_recovery_children_do_not_extend_parent_spans_across_trivia`；均关联软词转换后的旧断言。
- `multifile_type_checking`：99 passed / 5 failed。失败为
  `deferred_explicit_constructor_type_arguments_publish_no_construction_fact`、
  `cross_file_when_diagnostics_cover_shape_order_coverage_and_branch_join`、
  `unit_lambda_diagnostics_stop_jumps_and_returns_at_callable_boundary`、
  `top_level_initializers_publish_stable_cross_file_symbol_and_expression_types`、
  `companion_constant_initializers_publish_stable_ordinary_typed_facts`。

SPEC-0242 重建后的 `parser_call_argument` 已实际 31 passed / 0 failed，旧三个失败通过现行
参数模式语法与精确 Span 断言迁移修复。上述旧结果保留为历史因果，不视为当前测试执行。

2026-10-02 [SPEC-0247](../archive/specs/0247-multifile-baseline.md) 在 PR #13 合并后 main `efc52b6`
再次重现 `multifile_type_checking` 的 99 passed / 5 failed；现已修复 layout recovery 内部错误，
另四项按 Guide 的 Any join、隐式 it、独立 const capability 迁移并加强 facts/负向断言。
同一完整 suite 当前实际 107 passed / 0 failed，无 ignored，已纳入 stage/双平台 CI 脚本。
PR #14 的实现 head `d333e414` 与 main `3418904` 的双宿主 CI 均通过；归档分支未赶上合并的文档尾项已在本分支补齐。精确验收见该 Spec，不代表 frontend 全量通过。

完整编辑器 corpus 的五项历史失败仍为 `File header and declarations`、`Calls and lambda`、
`Own remains declaration-only`、`Control flow`、`Reserved words are not identifiers`；
具体证据与两个新增 corpus 的边界见
[SPEC-0242](../archive/specs/0242-automatic-borrow-call-migration.md#编辑器精确边界)。
SPEC-0246 不修复这些独立失败，也不把旧结果算作本次重跑。

不得通过降低断言、放宽门禁或把定向通过称作 frontend 全量通过来隐藏上述差异。

## 已批准规则与下一步门禁

调用 Borrow marker、移位 count、deinit body 权限与字段析构顺序的语义决定已进入 v0.40；
后继实施应按 Guide 验证，不再把它们列为等待用户选择。Str/toString 已明确延后；
unsafe 与其他未闭合 API/ABI 不能从已有代码或计划措辞推导新规则。

PR #7 合并后的首个执行切片为 SPEC-0240；此前审查勘误与 Litmus 门禁由 SPEC-0238
记录，SPEC-0239 的 Linux/macOS CI 复用同一脚本。该范围不包含自动合并、
改写历史审计原文或把诊断门禁通过说成全部实现完成。
旧失败须保留精确结果，修复时有独立因果与回归证据；不得降低断言、放宽门禁或称 frontend 全量通过。
