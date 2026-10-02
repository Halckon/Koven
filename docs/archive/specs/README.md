# 完成 Specs

> **性质**：历史验收索引 · **状态**：frozen · **读取时机**：按 SPEC ID 追溯已完成 Goal 时 · **唯一真源**：各 Spec 正文

本目录共 214 份 `done`/`superseded` Spec。它们不是当前工作队列。

全量依赖拓扑（含 active/draft 与本目录全部 Spec）见生成物
[dependency-graph-full.md](dependency-graph-full.md) / [dependency-graph-full.svg](dependency-graph-full.svg)，
由 `scripts/gen_spec_dag.py` 重建；现行拓扑见 [Specs 索引](../../specs/README.md)。

## Phase 0

- [SPEC-0001](./0001-bootstrap-cargo-workspace.md)：建立可检查的 Cargo workspace
- [SPEC-0002](./0002-source-span-foundation.md)：建立统一 source 与 Span 基础设施
- [SPEC-0003](./0003-structured-diagnostics.md)：建立结构化诊断核心
- [SPEC-0004](./0004-indexed-ast-foundation.md)：建立索引式 AST 基础
- [SPEC-0005](./0005-language-fixture-harness.md)：建立语言 fixture harness

## Phase 1

- [SPEC-0006](./0006-deterministic-lexer.md)：建立确定性 Lexer
- [SPEC-0007](./0007-pratt-expression-parser.md)：建立 Pratt 表达式 Parser
- [SPEC-0008](./0008-declaration-parser.md)：建立独立声明 Parser
- [SPEC-0009](./0009-block-statement-parser.md)：建立 block 与 statement 序列 Parser
- [SPEC-0010](./0010-lambda-literal-parser.md)：解析 lambda literal
- [SPEC-0011](./0011-implicit-unit-return.md)：支持具名函数隐式 `Unit` 返回
- [SPEC-0012](./0012-callable-parameter-and-call-argument-parser.md)：解析 callable 参数契约与 typed call argument
- [SPEC-0013](./0013-local-val-destructuring-parser.md)：解析局部 `val` 解构
- [SPEC-0014](./0014-complete-file-parser.md)：组合完整文件并跨声明恢复
- [SPEC-0015](./0015-package-import-parser.md)：解析 package 与 Kotlin 风格 import 文件头
- [SPEC-0016](./0016-control-flow-parser.md)：解析 control-flow、jump 与 super
- [SPEC-0017](./0017-class-family-parser.md)：解析 class-family
- [SPEC-0062](./0062-top-level-declaration-separators.md)：修正顶层声明换行与分号分隔
- [SPEC-0063](./0063-postfix-error-propagation-parser.md)：解析 postfix 错误传播运算符 `?`
- [SPEC-0064](./0064-interface-delegation-parser.md)：解析窄化接口委托
- [SPEC-0065](./0065-parser-module-decomposition.md)：按职责拆分 Parser 模块
- [SPEC-0066](./0066-numeric-literal-suffixes.md)：保留数值字面量后缀身份
- [SPEC-0068](./0068-frontend-adversarial-matrix.md)：增加 Lexer / Parser 对抗组合矩阵
- [SPEC-0069](./0069-parser-entry-adversarial-matrix.md)：覆盖独立 Parser 入口对抗矩阵
- [SPEC-0072](./0072-pratt-operator-matrix.md)：锁定 Pratt 运算符矩阵契约
- [SPEC-0073](./0073-lexer-boundary-matrix.md)：锁定 Lexer 固定词与符号边界矩阵
- [SPEC-0074](./0074-parser-token-inventory-matrix.md)：覆盖 Parser 完整词法片段库存
- [SPEC-0075](./0075-parser-lexical-owner-placement-matrix.md)：覆盖 Parser lexical owner 语法位置矩阵
- [SPEC-0076](./0076-parser-diagnostic-witness-matrix.md)：建立 Parser 已发布诊断 witness 矩阵
- [SPEC-0077](./0077-parser-trivia-invariance-matrix.md)：建立 Parser 非换行 trivia 等价矩阵
- [SPEC-0078](./0078-parser-line-break-boundary-matrix.md)：建立 Parser 结构性换行边界矩阵
- [SPEC-0079](./0079-parser-prefix-truncation-matrix.md)：建立 Parser 前缀截断恢复矩阵
- [SPEC-0080](./0080-parser-token-omission-matrix.md)：建立 Parser 单 token 缺失恢复矩阵
- [SPEC-0081](./0081-parser-lexical-poison-replacement-matrix.md)：建立 Parser 词法 poison 替换矩阵
- [SPEC-0082](./0082-parser-token-duplication-matrix.md)：建立 Parser 单 token 重复恢复矩阵
- [SPEC-0083](./0083-parser-lexical-poison-insertion-matrix.md)：建立 Parser 词法 poison 插入矩阵
- [SPEC-0084](./0084-parser-adjacent-token-transposition-matrix.md)：建立 Parser 相邻 token 交换矩阵
- [SPEC-0085](./0085-parser-entry-prefix-truncation-matrix.md)：建立独立 Parser 入口前缀截断矩阵
- [SPEC-0086](./0086-parser-entry-token-omission-matrix.md)：建立独立 Parser 入口单 token 缺失矩阵
- [SPEC-0087](./0087-parser-entry-token-duplication-matrix.md)：建立独立 Parser 入口单 token 重复矩阵
- [SPEC-0088](./0088-parser-entry-lexical-poison-replacement-matrix.md)：建立独立 Parser 入口词法 poison 替换矩阵
- [SPEC-0089](./0089-parser-entry-lexical-poison-insertion-matrix.md)：建立独立 Parser 入口词法 poison 插入矩阵
- [SPEC-0090](./0090-parser-entry-adjacent-token-transposition-matrix.md)：建立独立 Parser 入口相邻 token 交换矩阵
- [SPEC-0091](./0091-parser-entry-trivia-invariance-matrix.md)：建立独立 Parser 入口 trivia 等价矩阵
- [SPEC-0092](./0092-parser-entry-line-break-boundary-matrix.md)：建立独立 Parser 入口换行边界矩阵
- [SPEC-0093](./0093-parser-entry-adversarial-output-invariants.md)：强化独立 Parser 入口对抗产物不变量
- [SPEC-0094](./0094-parser-token-inventory-output-invariants.md)：强化 Parser token inventory 产物不变量
- [SPEC-0095](./0095-parser-lexical-owner-output-invariants.md)：强化 Parser lexical-owner 矩阵产物不变量
- [SPEC-0096](./0096-parser-diagnostic-witness-output-invariants.md)：强化 Parser diagnostic witness 产物不变量
- [SPEC-0097](./0097-parser-trivia-output-invariants.md)：强化 Parser trivia 等价矩阵产物不变量
- [SPEC-0098](./0098-parser-line-break-output-invariants.md)：强化 Parser line-break 边界产物不变量
- [SPEC-0099](./0099-parser-file-mutation-output-invariants.md)：强化完整文件恢复矩阵共享产物不变量
- [SPEC-0100](./0100-parser-entry-line-break-output-invariants.md)：强化独立 Parser 入口 line-break 产物不变量
- [SPEC-0101](./0101-parser-entry-trivia-output-invariants.md)：强化独立 Parser 入口 trivia 产物不变量
- [SPEC-0102](./0102-frontend-adversarial-output-invariants.md)：强化完整文件对抗矩阵 Lexer / Parser 产物不变量
- [SPEC-0103](./0103-lexer-boundary-output-invariants.md)：强化 Lexer 固定词与符号边界矩阵产物不变量
- [SPEC-0104](./0104-pratt-operator-output-invariants.md)：强化 Pratt 运算符矩阵前端产物不变量
- [SPEC-0105](./0105-parser-entry-adversarial-lexer-invariants.md)：强化独立 Parser 入口对抗矩阵 Lexer 确定性
- [SPEC-0106](./0106-parser-token-inventory-lexer-invariants.md)：强化 token inventory Lexer 确定性
- [SPEC-0107](./0107-parser-lexical-owner-lexer-invariants.md)：强化 Parser lexical-owner Lexer 确定性
- [SPEC-0108](./0108-parser-diagnostic-witness-lexer-invariants.md)：强化 Parser diagnostic-witness Lexer 确定性
- [SPEC-0109](./0109-parser-trivia-lexer-invariants.md)：强化 Parser trivia 等价矩阵 Lexer 确定性
- [SPEC-0110](./0110-parser-line-break-lexer-invariants.md)：强化完整文件 line-break 边界 Lexer 确定性
- [SPEC-0111](./0111-parser-file-mutation-lexer-invariants.md)：强化完整文件 mutation 矩阵 Lexer 确定性
- [SPEC-0112](./0112-parser-entry-line-break-lexer-invariants.md)：强化独立 Parser 入口 line-break Lexer 确定性
- [SPEC-0113](./0113-parser-entry-trivia-lexer-invariants.md)：强化独立 Parser 入口 trivia Lexer 确定性
- [SPEC-0114](./0114-parser-entry-mutation-lexer-invariants.md)：强化独立 Parser 入口 mutation Lexer 确定性
- [SPEC-0115](./0115-fixture-frontend-output-invariants.md)：强化 fixture frontend 重复产物不变量
- [SPEC-0117](./0117-parser-expression-suite-output-invariants.md)：强化表达式 Parser 核心 suite 重复产物不变量
- [SPEC-0118](./0118-parser-declaration-suite-output-invariants.md)：强化声明 Parser 核心 suite 重复产物不变量
- [SPEC-0119](./0119-parser-block-suite-output-invariants.md)：强化 Block Parser 核心 suite 重复产物不变量
- [SPEC-0120](./0120-parser-lambda-suite-output-invariants.md)：强化 Lambda Parser 核心 suite 重复产物不变量
- [SPEC-0121](./0121-parser-call-argument-suite-output-invariants.md)：强化 Call Argument Parser 核心 suite 重复产物不变量
- [SPEC-0122](./0122-parser-local-destructuring-suite-output-invariants.md)：强化局部解构 Parser 核心 suite 重复产物不变量
- [SPEC-0123](./0123-parser-control-flow-suite-output-invariants.md)：强化 Control-flow Parser 核心 suite 重复产物不变量
- [SPEC-0124](./0124-parser-error-propagation-suite-output-invariants.md)：强化错误传播 Parser 核心 suite 重复产物不变量
- [SPEC-0125](./0125-parser-class-family-suite-output-invariants.md)：强化 Class-family Parser 核心 suite 重复产物不变量
- [SPEC-0126](./0126-parser-interface-delegation-suite-output-invariants.md)：强化接口委托 Parser 核心 suite 重复产物不变量
- [SPEC-0127](./0127-parser-implicit-unit-suite-output-invariants.md)：强化隐式 Unit Parser 核心 suite 重复产物不变量
- [SPEC-0128](./0128-parser-file-suite-output-invariants.md)：强化完整文件 Parser 核心 suite 重复产物不变量
- [SPEC-0129](./0129-lexer-core-suite-output-invariants.md)：强化 Lexer 核心 suite 重复产物不变量
- [SPEC-0135](./0135-parser-internal-lexer-input-invariants.md)：强化 Parser 私有算法测试的 Lexer 输入不变量
- [SPEC-0136](./0136-frontend-internal-error-determinism.md)：强化 Lexer / Parser 内部边界错误确定性
- [SPEC-0137](./0137-parser-invalid-lexeme-stream-matrix.md)：建立 Parser 非法 Lexeme 流拒绝矩阵
- [SPEC-0138](./0138-parser-invalid-lexical-owner-matrix.md)：建立 Parser 非法 lexical-owner 流拒绝矩阵
- [SPEC-0139](./0139-parser-invalid-recovery-diagnostic-matrix.md)：建立 Parser 非法 recovery diagnostic 关联矩阵
- [SPEC-0140](./0140-parser-lexer-diagnostic-anchor-contract.md)：锁定 Parser 的 Lexer diagnostic anchor 契约
- [SPEC-0141](./0141-parser-lexer-diagnostic-stream-identity.md)：锁定 Parser 的 Lexer diagnostic 流身份
- [SPEC-0142](./0142-parser-lexer-poison-diagnostic-coverage.md)：锁定 Parser 的 lexical poison 诊断覆盖
- [SPEC-0143](./0143-parser-lexer-diagnostic-anchor-uniqueness.md)：锁定 Parser 的 Lexer diagnostic anchor 唯一性
- [SPEC-0144](./0144-parser-suffix-truncation-matrices.md)：建立 Parser UTF-8 后缀截断矩阵
- [SPEC-0145](./0145-parser-interior-deletion-matrices.md)：建立 Parser UTF-8 内部区间删除矩阵
- [SPEC-0146](./0146-parser-scalar-duplication-matrices.md)：建立 Parser UTF-8 scalar 重复矩阵
- [SPEC-0147](./0147-parser-scalar-transposition-matrices.md)：建立 Parser UTF-8 scalar 相邻交换矩阵
- [SPEC-0148](./0148-parser-scalar-replacement-matrices.md)：建立 Parser UTF-8 scalar 替换矩阵
- [SPEC-0149](./0149-parser-scalar-insertion-matrices.md)：建立 Parser UTF-8 scalar 插入矩阵
- [SPEC-0150](./0150-lexer-large-input-mode-depth-stress.md)：建立 Lexer 大输入与深模式压力矩阵
- [SPEC-0151](./0151-parser-large-flat-recovery-stress.md)：建立 Parser 大平坦列表与恢复压力矩阵
- [SPEC-0152](./0152-parser-recursion-budget-boundaries.md)：锁定 Parser 递归预算的精确公开边界
- [SPEC-0153](./0153-parser-caller-stack-isolation.md)：锁定 Parser 四入口的调用者栈隔离
- [SPEC-0154](./0154-lexer-small-stack-stress.md)：锁定 Lexer 深模式的小调用栈行为
- [SPEC-0155](./0155-parser-owner-rich-stress.md)：建立 Parser 大规模 lexical-owner 压力矩阵
- [SPEC-0156](./0156-parser-string-poison-stress.md)：压力验证 Lexer 字符串错误向 Parser 的唯一传播
- [SPEC-0157](./0157-parser-standalone-poison-matrices.md)：扩展 Parser 独立 lexical poison 变换矩阵
- [SPEC-0158](./0158-parser-standalone-poison-stress.md)：建立 Parser standalone lexical poison 压力矩阵
- [SPEC-0159](./0159-parser-lexical-owner-recursion-boundaries.md)：锁定 Parser lexical-owner 递归预算边界
- [SPEC-0160](./0160-lexer-long-invalid-lexeme-stress.md)：建立 Lexer 超长非法 lexeme 压力矩阵
- [SPEC-0161](./0161-parser-long-lexical-error-bridge.md)：建立 Parser 超长词法错误桥接矩阵
- [SPEC-0162](./0162-parser-mixed-long-lexical-error-stream.md)：建立 Parser 混合超长词法错误流矩阵
- [SPEC-0163](./0163-parser-mixed-long-recoverable-error-stream.md)：建立 Parser 混合超长可恢复词法错误流矩阵
- [SPEC-0164](./0164-parser-long-utf8-line-recovery.md)：建立 Parser 超长 UTF-8 换行恢复矩阵
- [SPEC-0165](./0165-parser-long-utf8-nested-line-recovery.md)：建立 Parser 超长 UTF-8 嵌套换行恢复矩阵
- [SPEC-0166](./0166-parser-long-utf8-char-line-recovery.md)：建立 Parser 超长 UTF-8 Char 换行恢复矩阵
- [SPEC-0167](./0167-parser-long-invalid-number-boundaries.md)：建立 Parser 超长非法数字边界矩阵
- [SPEC-0168](./0168-parser-long-block-comment-line-breaks.md)：建立 Parser 超长块注释换行矩阵
- [SPEC-0169](./0169-parser-long-line-comment-boundaries.md)：建立 Parser 超长行注释边界矩阵
- [SPEC-0170](./0170-parser-large-file-header-stress.md)：建立 Parser 大文件头压力矩阵
- [SPEC-0171](./0171-parser-large-qualified-header-paths.md)：建立 Parser 超长文件头限定路径矩阵
- [SPEC-0172](./0172-parser-large-file-header-separators.md)：建立 Parser 大文件头分隔与恢复矩阵
- [SPEC-0175](./0175-call-argument-lambda-boundary.md)：修复调用实参 lambda 的 block 边界误判
- [SPEC-0201](./0201-instance-receiver-mode-parser.md)：instance receiver mode Parser
- [SPEC-0213](./0213-trailing-lambda-call-parser.md)：尾 lambda 调用 Parser

## Phase 1/2/3 纵向切片

- [SPEC-0214](./0214-implicit-it-lambda-parameter.md)：隐式 `it` lambda 参数

## Phase 2

- [SPEC-0179](0179-sequential-iteration-typed-plan.md)：顺序容器借用迭代 typed plan

- [SPEC-0205](./0205-non-null-assertion-facts.md)：非空断言 extraction typed facts
- [SPEC-0202](0202-nullable-when-flow-facts.md)：nullable when 剩余域 typed facts

- [SPEC-0018](./0018-single-file-name-resolution.md)：建立单文件作用域与名称解析
- [SPEC-0019](./0019-basic-type-checking.md)：建立基础类型检查与局部推导
- [SPEC-0020](./0020-nominal-generic-interface-types.md)：建立名义/泛型类型与静态 interface 实现
- [SPEC-0021](./0021-when-exhaustiveness-smart-cast.md)：检查 `when` 穷尽性与 smart cast
- [SPEC-0022](./0022-copyable-structural-destructuring.md)：推导条件 `Copyable` 并检查结构化解构类型
- [SPEC-0023](./0023-sequential-container-types.md)：检查顺序容器类型、构造与 element place
- [SPEC-0067](./0067-callable-type-checking.md)：检查 callable 调用与实参契约
- [SPEC-0130](./0130-name-resolution-frontend-input-invariants.md)：强化名称解析 suite 的前端输入不变量
- [SPEC-0131](./0131-type-checking-frontend-input-invariants.md)：强化类型检查核心 suite 的前端输入不变量
- [SPEC-0132](./0132-callable-type-frontend-input-invariants.md)：强化 callable 类型 suite 的前端输入不变量
- [SPEC-0133](./0133-container-type-frontend-input-invariants.md)：强化顺序容器类型 suite 的前端输入不变量
- [SPEC-0134](./0134-copyability-type-frontend-input-invariants.md)：强化 copyability 类型 suite 的前端输入不变量
- [SPEC-0173](./0173-lambda-parameter-contract-facts.md)：Lambda 参数契约 typed facts
- [SPEC-0174](./0174-overload-lambda-candidate-isolation.md)：overload lambda 候选隔离检查
- [SPEC-0177](./0177-generic-callable-instantiation.md)：泛型 callable 实例化与实例 identity
- [SPEC-0178](./0178-jump-target-checking.md)：检查 break / continue 词法目标
- [SPEC-0180](./0180-instance-receiver-typed-facts.md)：instance receiver typed facts
- [SPEC-0183](./0183-constructor-typed-facts.md)：构造目标、实例化与 typed facts
- [SPEC-0197](./0197-multifile-type-checking.md)：跨文件类型检查
- [SPEC-0218](./0218-compilation-unit-assignment-facts.md)：compilation-unit 普通替换赋值类型事实
- [SPEC-0219](./0219-compilation-unit-runtime-field-layout-facts.md)：compilation-unit 实例限定 runtime 字段布局事实

- [SPEC-0210](./0210-multifile-associated-constants.md)：跨文件关联常量 typed facts 与独立 capability

## Phase 2/3/4 纵向切片

- [SPEC-0196](./0196-nullable-handle-lowering.md)：pointer-like nullable handle lowering

## Phase 2/3/4/5 纵向切片

- [SPEC-0045](./0045-shared-rc-owner.md)：单线程共享 `Rc<T>` owner
- [SPEC-0192](./0192-general-string-runtime.md)：一般 UTF-8 String runtime

## Phase 2（名称解析）

- [SPEC-0025](./0025-multifile-package-import-name-resolution.md)：多文件 package/import 名称解析
- [SPEC-0026](./0026-associated-constant-evaluation.md)：单文件关联常量选择、封闭求值与 typed facts

## Phase 3

- [SPEC-0206](0206-non-null-assertion-ownership.md)：非空断言 Copy/Consume 所有权

- [SPEC-0027](./0027-variable-ownership-use-after-move.md)：建立变量所有权状态并检测 use-after-move
- [SPEC-0028](./0028-conditional-copy-structural-move.md)：检查条件复制、消费式解构与结构分量移动
- [SPEC-0029](./0029-call-loans-drop-points.md)：调用期借用与 ASAP 析构点
- [SPEC-0030](./0030-sequential-container-element-ownership.md)：顺序容器 element place 所有权
- [SPEC-0032](./0032-move-closure-transferable.md)：move closure 与 Transferable 检查
- [SPEC-0176](./0176-borrow-default-parameter-contracts.md)：迁移 borrow-default 参数契约
- [SPEC-0181](./0181-instance-receiver-ownership.md)：instance receiver ownership
- [SPEC-0188](./0188-constructor-ownership-effects.md)：构造 Value delivery 与所有权效果
- [SPEC-0198](./0198-multifile-ownership-checking.md)：跨文件所有权检查
- [SPEC-0211](./0211-sequential-iteration-ownership.md)：顺序迭代 source/element loan 与退出清理
- [SPEC-0215](./0215-lambda-body-result-drop-facts.md)：lambda body 隐式结果析构事实
- [SPEC-0216](./0216-control-result-drop-facts.md)：MoveOnly control result 析构事实
- [SPEC-0217](./0217-lambda-value-parameter-drop-facts.md)：lambda Value 参数入口析构事实
- [SPEC-0222](./0222-static-self-value-delivery-facts.md)：StaticSelf Value receiver 条件交付事实

## Phase 3/4 纵向切片

- [SPEC-0195](./0195-interprocedural-borrow-lowering.md)：跨 callable Borrow 的 SSA/LLVM lowering

- [SPEC-0203](./0203-nullable-when-ownership.md)：nullable when view/extraction 所有权
- [SPEC-0204](./0204-pointer-nullable-when-lowering.md)：pointer-like nullable when SSA/LLVM/native lowering

- [SPEC-0208](./0208-constant-materialization-ownership.md)：常量重新物化与所有权事实

- [SPEC-0226](./0226-unit-constant-materialization-ownership.md)：跨文件常量重新物化与所有权

- [SPEC-0243](./0243-receiver-two-phase-borrows.md)：方法 receiver 两阶段借用与 unit SSA/native 有界验收

## Phase 4

- [SPEC-0227](./0227-unit-constant-native-lowering.md)：跨文件常量 SSA 与 native 交付

- [SPEC-0207](./0207-pointer-non-null-assertion-lowering.md)：pointer-like 非空断言 SSA/LLVM/native lowering

- [SPEC-0033](./0033-typed-ssa-ir-verifier.md)：最小 typed SSA IR 与 verifier
- [SPEC-0034](./0034-scalar-control-flow-llvm-lowering.md)：标量与控制流经 typed SSA lower 到 LLVM IR
- [SPEC-0035](./0035-aggregate-class-allocation-drop.md)：聚合、class 分配与显式 drop/free 后端基元
- [SPEC-0036](./0036-sequential-container-runtime.md)：顺序容器连续缓冲区与运行时基元
- [SPEC-0212](./0212-borrowed-sequential-iteration-ssa.md)：借用式顺序迭代 SSA/LLVM primitives
- [SPEC-0038](./0038-closure-environment-codegen.md)：具体闭包环境与间接调用后端
- [SPEC-0039](./0039-native-object-entry-link.md)：本机目标文件、显式入口与首个链接链路
- [SPEC-0040](./0040-dwarf-line-tables-lldb.md)：DWARF 源码行表与首个 LLDB 验收
- [SPEC-0184](./0184-nominal-construction-lowering.md)：名义构造与所有权 facts 到 SSA/LLVM lowering
- [SPEC-0185](./0185-declarative-type-roots-codegen.md)：声明型 type roots 的源码模块接纳边界
- [SPEC-0186](./0186-target-layout-preflight.md)：目标布局预检
- [SPEC-0191](./0191-instance-receiver-lowering.md)：instance receiver 与静态委托 lowering
- [SPEC-0199](./0199-multifile-native-lowering.md)：多文件 compilation-unit native lowering
- [SPEC-0220](./0220-compilation-unit-pointer-nullable-storage-lowering.md)：compilation-unit pointer-like nullable storage lowering
- [SPEC-0221](./0221-move-only-empty-enum-case-lowering.md)：MoveOnly enum 空 case owner lowering
- [SPEC-0223](./0223-static-self-value-delivery-lowering.md)：StaticSelf Value receiver 条件交付 lowering
- [SPEC-0224](./0224-dependent-inherited-owner-recipes.md)：dependent inherited owner recipe lowering
- [SPEC-0225](./0225-parameter-growing-runtime-type-cycles.md)：参数增长型 runtime recipe 策略与 lowering

## Phase 4/6 纵向切片

- [SPEC-0194](./0194-parameterized-main-argv.md)：参数化 main 与 argv owner

## Phase 5

- [SPEC-0042](./0042-standard-library-bootstrap.md)：标准库目标语言 bootstrap 闭环
- [SPEC-0043](./0043-standard-error-abort.md)：标准 `error()` identity 与 abort 接线
- [SPEC-0044](./0044-standard-pair-result.md)：标准 `Pair` 与 `Result`
- [SPEC-0189](./0189-standard-println-output.md)：标准 `println(String)` 与最小 stdout 输出

## Phase 6

- [SPEC-0052](./0052-minimal-project-manifest-source-set.md)：最小 project manifest 与本地 source-set provider
- [SPEC-0054](./0054-local-project-build-run.md)：无依赖本地 project build/run
- [SPEC-0055](./0055-single-document-lsp-diagnostics.md)：发布单文档 LSP 诊断
- [SPEC-0056](./0056-single-document-definition.md)：单文档语义跳转定义
- [SPEC-0057](./0057-conservative-source-formatter.md)：保守、稳定且幂等的源码格式化器
- [SPEC-0058](./0058-textmate-grammar.md)：提供 TextMate grammar 与回归 fixture
- [SPEC-0059](./0059-tree-sitter-grammar.md)：提供 Tree-sitter grammar 与 corpus
- [SPEC-0060](./0060-machine-readable-diagnostics.md)：发布版本化机器可读诊断
- [SPEC-0070](./0070-tree-sitter-word-contract.md)：锁定 Tree-sitter 与 Lexer 词表契约
- [SPEC-0071](./0071-textmate-lexical-contract.md)：执行 TextMate symbol 与 literal 契约
- [SPEC-0116](./0116-grammar-bridge-frontend-invariants.md)：强化语法工具链 frontend 重复产物不变量
- [SPEC-0187](./0187-multifile-lsp-diagnostics-definition.md)：跨文件 LSP 诊断与跳转定义
- [SPEC-0190](./0190-public-single-file-build-run.md)：公开单文件 `kovenc build/run`
- [SPEC-0193](./0193-conventional-zero-argument-main.md)：单文件零参数 conventional `main`

- [SPEC-0209](./0209-associated-constant-lowering.md)：单文件关联常量 SSA/LLVM/native 重新物化
