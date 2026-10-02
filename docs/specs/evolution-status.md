# v0.40 演进计划实施账本

> **性质**：变更进度索引 · **状态**：current · **读取时机**：继续演进计划或核对交付范围时 · **唯一真源**：对应 Guide、Spec 与代码测试

本页对照 2026-10-01 的 13 项计划记录当前本地整合事实；语言规则以唯一 current
[Guide v0.40](../guide/README.md) 为准，不以 AST、类型名或文档启用代替端到端证据。
起点为 main `d3e64a4`；七阶段合并提交 `ed0727f` 形成真实 v0.39，随后整合 SPEC-0235。
八个切片及整合合同保留各自验收账本。PR #7 已于 2026-10-01 合并，后继切片基于真实 main `e22e11b`；既有 Spec 生命周期不在本切片中批量改写。
后到的 main `3be83b5`（已合并 PR #6）另作[最小协调](../archive/migrations/v0.40-upstream-pr6-reconciliation.md)，
整合 PR CI 后的独立核查与更正见 [SPEC-0238](active/0238-guide-litmus-gate.md)及
[当前更正账本](../architecture/guide-conformance.md)；不重写已冻结的真实 v0.39。

## 八阶段已整合范围

| 切片 | 已有本地成果 | 尚未闭环 |
|---|---|---|
| [SPEC-0229](active/0229-extended-numeric-literal-values.md) | radix/underscore 统一解码、单/unit 定型与常量、索引 identity、整数 SSA/native | 浮点 native、具名位运算与 inv 不在此切片 |
| [SPEC-0230](active/0230-recursive-boxed-enum-native.md) | 非泛型递归 Box enum 单/unit 构造、运输、真实分配/递归释放计数 | generic enum、拆箱/解引用、nullable/Rc 递归包装与前向 case 查找保持边界 |
| [SPEC-0231](active/0231-contextual-type-ref-trials.md) | 上下文 TypeRef、nested 函数模式、strict typed-call trial 与回滚一致 | 调用处取消 Borrow marker 与三个旧 call-argument 失败由 SPEC-0242 后续切片验证，不改写本切片历史验收 |
| [SPEC-0232](active/0232-ownership-primitive-type-facts.md) | replace/swap 稳定 intrinsic、交换类型、源码顺序 operand identity、事务与结构验证 | ownership/SSA/native 原子操作尚未接线；无可信 continuation 公开 API |
| [SPEC-0233](active/0233-parser-compiler-contracts.md) | 九段原文迁入 Compiler Contracts，两页唯一索引、递归门禁与预算 | 渐进拆分首片；混合语义/诊断/Span 段保留 Guide，不声称全部分离 |
| [SPEC-0234](active/0234-block-newline-continuation.md) | 普通/control/nested block 的 Pratt/postfix 换行边界、for-header delimiter 修复及矩阵 | 同行缺分隔符、前导/重复分号、lambda 顶层尾表达式范围另列 |
| [SPEC-0236](active/0236-explicit-string-clone.md) | String.clone 单/unit typed→loan/drop→StringClone SSA→Linux native；heap/static/empty 与真实分配释放计数 | Borrow Rc<String>.value.clone、inline-nullable String 和通用 clone 未扩张 |
| [SPEC-0235](active/0235-approved-language-rules.md) | 真实 v0.39 完整归档；v0.40 唯一入口启用三项批准规则，保留 clone-first | 三项新规则的实现不随文档完成；最终 PR CI 未完成 |

合并后交叉用例、统一 target 的本地门禁、实际命中数及剩余失败统一记录于
[SPEC-0237](active/0237-local-integration.md)，不把上表各分支旧结果冒充最终整合结果。

## 十三项实况

| 项 | 状态 | 已有证据与未闭合边界 |
|---|---|---|
| 1. 块换行与分号 | 主要续行切片已落地 | SPEC-0234 覆盖完整左式后 call/prefix 分隔、未完成操作数/delimiter 续行；同行缺分隔诊断与独立 lambda 实施仍未闭环；上游 PR #6 已将 lambda 换行/分号尾表达式写入规范 |
| 2. 上下文关键字 | TypeRef/trial 切片已落地 | SPEC-0231 覆盖 move 普通类型名和 nested 模式；SPEC-0242 已移除调用 Borrow marker，直接 parser 31 项通过；共享路径与PR #9初始双平台CI已通过，与0240的最新main组合本地门禁已通过，新head双平台CI待验证 |
| 3. 数值与具名位运算 | 数值与位运算本地已验收 | SPEC-0229 闭合字面量；[SPEC-0240](active/0240-integer-bitwise-execution.md) 接入六操作 const/SSA/native 与 inv 稳定身份。移位按自身位宽屏蔽且保持两 operand 同型；inv 仍不在 const call 白名单，既有投影边界不扩大 |
| 4. Box enum | 受限 native 已落地 | SPEC-0230 覆盖具体非泛型递归构造/运输/析构计数；Box.value/unbox 已由上游 PR #6 写成后继 staged 合同，尚无对应实现证据；generic、nullable/Rc 递归包装及前向 case 查找不在已支持范围 |
| 5. replace/swap | 可信 typed facts 已落地 | SPEC-0232 发布身份/类型/顺序并验证事务；专用 ownership/SSA/native、返回旧 owner 与原子保持仍缺 |
| 6. 两阶段 receiver 借用 | 未实现 | receiver 仍直接建立 active exclusive loan；Reserved/Activate 和 callee 存活 Borrow 冲突尚需事实及验证 |
| 7. deinit 双轨析构 | 语法/类型层已有 | v0.40 已明确 readonly this、body 先于逆序字段清理；drop planner/codegen 仍无资源 lexical lifetime 与执行 deinit 的 native 证据 |
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
- 原子置换：frontend `type_ownership_primitives` 与 `ownership_primitive::tests`；typed descriptor
  不代表后续阶段已有原子执行事实
- 文档：Compiler Contracts、`scripts/check_docs.py`、`scripts/tests/test_check_docs.py`

上述 suite 名按各 Spec 的完整命令定位；详细实现边界由 [Architecture](../architecture/README.md)维护。

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
参数模式语法与精确 Span 断言迁移修复；`multifile_type_checking` 的五个历史失败已在本次
联合回归重现（99 passed / 5 failed，失败名称相同）。上述旧结果保留为历史因果，不视为当前测试执行。

不得通过降低断言、放宽门禁或把定向通过称作 frontend 全量通过来隐藏上述差异。

## 已批准规则与下一步门禁

调用 Borrow marker、移位 count、deinit body 权限与字段析构顺序的语义决定已进入 v0.40；
后继实施应按 Guide 验证，不再把它们列为等待用户选择。Str/toString 已明确延后；
unsafe 与其他未闭合 API/ABI 不能从已有代码或计划措辞推导新规则。

PR #7 合并后的首个执行切片为 SPEC-0240；此前审查勘误与 Litmus 门禁由 SPEC-0238
记录，SPEC-0239 的 Linux/macOS CI 复用同一脚本。该范围不包含自动合并、
改写历史审计原文或把诊断门禁通过说成全部实现完成。
旧失败须保留精确结果，修复时有独立因果与回归证据；不得降低断言、放宽门禁或称 frontend 全量通过。
