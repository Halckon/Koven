# Parser 验证规划的检查与执行指南

> **性质**：非规范验证指南 · **状态**：未启用 · **读取时机**：检查本次文档或启动首个实施切片时 · **唯一真源**：本页维护验证入口与预期，不保存实现源码

## 当前可运行：仅文档规划验收

前提：Git、Python 3；进入本工作树根目录。
不需要 LLVM，不安装文法或证明工具。

```bash
git branch --show-current
git rev-parse HEAD
python3 scripts/check_docs.py
git diff --check
```

预期：分支为 `feature/spec-parser-formal-plan`；初始 HEAD 为主计划的 main SHA；
docs 检查退出 0，无断链、metadata 或索引遗漏；diff 无空白错误。
尚未提交的新文件还需独立 whitespace 检查，不能只靠 git diff --check。

验证来源保全（迁入原文应无差异）：

```bash
git show e00bc60bf2eec035634e005e93242745f5e333b3:docs/Koven_Parser_Grammar_Implementation_Plan_2026-11-08.md |
  cmp - docs/archive/designs/parser-grammar-implementation-2026-11-08.md
```

预期退出 0。评审页仅改变迁移账本声明的链接，不改历史结论。
独立审阅需对照[后续评审](../parser-grammar-verification-review.md)与
[主计划](../parser-formal-verification-plan.md)，核实阶段不遗漏、门槛可验、未夸称证明。

## 下一步 F0：归因，不实施

```bash
git log --format='%H %s' 6cc03907 -- crates/lang-frontend/src/parser
git diff --stat a775568f..6cc03907 -- crates/lang-frontend/src/parser docs/guide editors/tree-sitter
```

第一条只枚举候选；逐个读取 diff 后才能归因，不能仅用提交标题计数。
记录历史范围的起止与排除理由，不将 main 移动后的新提交混入同一分母。
先映射 `tests/support/parser_test_assertions.rs`、既有 matrix 与 R1–R5，再选择重现项。
未执行任何实现检查时，F0 不得写“当前开放缺陷已确认”。

## 实施后可运行：现有定向回归（本轮未执行）

前提：对应 Spec 获批并完成测试选择；使用仓库固定 Rust 工具链与 locked 依赖。
根据改动契约增减下列选择，这不是所有家族永久白名单：

```bash
cargo test --locked -p lang-frontend --no-fail-fast \
  --test parser_expression --test parser_trailing_lambda --test parser_contextual_type_ref
cargo test --locked -p lang-frontend --no-fail-fast \
  --test parser_class_family --test parser_index_recovery --test parser_statement_separators
cargo test --locked -p lang-frontend --no-fail-fast \
  --test parser_recursion_boundary_matrix --test parser_stack_isolation_matrix
```

预期：各套件实际命中、无失败；记录 passed/filtered/ignored/未运行。
先添加独立预期的失败测试，再实现最小变化，再重跑同一选择。
共享路径变更必须追加受影响的变异矩阵及直接消费者，不以本页短列表替代影响分析；
不默认运行 frontend 全量，也不并发争用同一 Cargo target。
生产修改另需 fmt、相关 clippy，按[测试指南](../../development/testing.md)执行。

## 后续 F3/F4/F6 的端到端验收情景

以下是待实现验收场景，不是已存在命令。本轮不提供虚构可执行文件名。
对应 Spec 在实现前必须将场景绑定到实际工具命令与目标。

1. EBNF 漂移：改动一个映射产生式，检查器必须失败；恢复后通过。
2. 冲突见证：注入已知歧义，报告保留原始冲突；不能靠 precedence 隐藏后称原文法无歧义。
3. 泛型尾 lambda：D1 批准后覆盖同行注释/换行注释/缺失类型实参及失败试探无副作用。
4. AST 身份：更换名称或字面量，normalizer 必须不同；仅平移源码位置则结构相同，Span 检查仍独立通过。
5. 错误边界：破坏内层分隔符后保留 caller closer 与后继 Sentinel，诊断和 Error 归属正确。
6. 资源分类：超预算得到 ResourceLimit 而非 SyntaxError；panic、timeout、零命中必须失败。
7. 证据失效：改变对应源码/规则/normalizer，旧报告不得为新输入背书。
8. 局部证明：若启用 F6，明确模型到生产 Rust 的连接；只有模型定理时不得报告生产 G4。

本次未运行这些场景，不宣称测试、冲突分析或形式化证明已经通过。
