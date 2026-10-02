# SPEC-0232：原子置换原语的可信类型事实

> **性质**：实施 Spec · **状态**：done · **读取时机**：实现或消费 replace/swap 的 typed 身份时 · **唯一真源**：本 Spec

| 字段 | 值 |
|---|---|
| 状态 | `done` |
| Goal ID | `KOV-P2-232` |
| 所属 Phase | Phase 2 |
| 语言规范 | [所有权与原子置换](../../guide/10-ownership-borrowing-drop.md)、[标准库原语](../../guide/13-program-runtime-standard-library.md) |
| 批准依据 | 2026-10-01 用户要求继续演进计划的未完成项，分阶段实施、验证与提交 |
| 前置 Spec / ADR | 既有 callable / intrinsic / typed transaction 合同，无新增语义决定 |
| 阻塞项 | 无 |
| 影响范围 | frontend single/unit typed descriptor、事务快照与验证、定向测试 |
| 语言语义变更 | 否 |

## 1. Goal

后续阶段能够通过经过验证的 typed 产物识别编译器绑定的 replace/swap，并取得交换值类型、
按源码求值顺序排列的两个 operand identity；不得根据源码函数名、普通 External target 或
AST 形状猜测内建身份。

## 2. 背景与阶段边界

当前标准环境和两条类型检查路径已实现参数模式与同型检查，但仅发布普通 external call。
这是 native 实施前的明确事实缺口。按照现有 Rc / container descriptor 模式补齐独立事实，
同一表达式的普通 call 参数契约继续保留，source-qualified unit identity 不退化成单文件 ID。

本阶段不宣称原语已经能够运行。原子置换 ownership、owned var root native、稳定字段、索引、
普通 Inout 参数及 closure/provenance 转移分开验收；现有赋值的隐式 old-value drop 不能代替
replace 返回旧 owner 或 swap 保持两个 owner 的合同。

## 3. 范围与要求

- 只有 TypeEnvironment 显式绑定的 intrinsic 产生此 descriptor；同名源码声明不获得。
- 记录 call expression、Replace/Swap identity、规范化交换类型、两个 operand identity。
- 与现有 call/type/category 产物保持一致；public getter 不开放伪造可执行事实的构造器。
- 单文件与 unit 试探状态完整回滚；失败候选、错误参数或未提交 trial 不留下半份 descriptor。
- unit 验证必须核对 source/type/operand/call 关联，不仅追加一个无校验的列表。
- Nothing operand 仍保留静态调用 descriptor 与真实 operand identity；后继依据自己的控制流
  事实建立 abort/exit prefix，不把静态身份视为 ownership 可执行提交许可。
- 不发布 continuation 布尔值：既有 ExprCheck 对闭包体、短路、when、普通调用等的启发式
  传播不足以支持精确公开合同，不能为方便后端而将其提升为可信执行事实。
- 不改调用位置 borrow 兼容策略、Str、deinit、移位或普通泛型推导规则。

## 4. 实施与验收

1. [x] 单文件/unit 正反例与来源身份、trial rollback 的红测。
2. [x] 显式 typed descriptor、查询、事务与 validation 接线。
3. [x] Nothing operand 保留静态结构，不发布 continuation 或冒充 native 能力。
4. [x] 定向 frontend、下游编译与相关 codegen 回归、严格 lint、文档门禁。
5. [ ] 分支发布后的 CI 和 Spec 归档。

## 5. 验证账本

| 命令 / 验收 | 实际结果 | 限制 |
|---|---|---|
| 新 integration 首轮 | 编译失败：24 处新 API 尚不存在 | 实现前红测 |
| unit 结构验证红测 | 2 失败，覆盖损坏事实未被拒绝 | 之后补足两个验证入口 |
| Deferred operand 红测 | 10 通过、1 失败：错误追加 L0084 | 已改为 deferred recovery，不发布成功原语 |
| `cargo test -p lang-frontend --test type_ownership_primitives --test type_checking --test type_callable --test type_copyability --test type_constants --test ownership_checking --test multifile_ownership_checking --no-fail-fast --locked --offline` | 251 通过 | 其中新 suite 17 通过；非全量 frontend |
| `cargo test -p lang-frontend --lib ownership_primitive::tests --locked --offline` | 2 通过、177 filtered | 14 种结构损坏；基础及 const-enabled 入口 |
| `cargo fmt --all -- --check` | 通过 | 最终源码 |
| `cargo check --workspace --all-targets --locked --offline` | 通过 | 新公开 API 下游编译 |
| `cargo clippy -p lang-frontend --all-targets --locked --offline -- -D warnings` | 通过 | 严格 lint |
| `cargo test -p lang-codegen --lib --locked --offline` | 478 通过 | 含真实 Linux LLVM/native 既有回归；未新增原语 native |
| `cargo test -p lang-frontend --test multifile_type_checking --locked --offline` | 99 通过、5 个已知失败 | 与基线相同，无新增失败 |
| `python3 scripts/check_docs.py` / `python3 -m unittest discover -s scripts/tests -v` | 386 篇通过 / 21 通过 | 结构检查不等于语义证明 |
| 独立只读复核 | 无剩余阻塞 | 已撤回不可信 continuation API，保留静态身份范围 |
| macOS / PR CI | 未运行 | 未发布；Linux 本机不替代 macOS 门禁 |

## 6. 提交计划

`feat(frontend): publish ownership primitive type facts (SPEC-0232)`。

## 7. 未决问题

无新增语言选择。当前 unit typed product 不持有 AST，旧 call argument fact 也不含 operand ID；
validator 交叉核对现有 source/type/category/call 事实，精确 AST operand 由封闭 producer 绑定，
后继读取 AST 时必须比对顺序，不能把结构验证说成独立重建了语法映射。

普通 unit call 的 flow 失效、已收窄 nullable place 的 storage-T 推导和既有通用 continuation
分析不足继续单列，不在这个静态类型身份切片偷偷改变普通调用规则。稳定 place、原子提交、
CFG/中断前缀和 native 仍须独立实施；本分支的 CI、发布与 Spec 归档也尚未完成。

### 既有失败边界

unit suite 的五项既有失败为 `companion_constant_initializers_publish_stable_ordinary_typed_facts`、
`cross_file_when_diagnostics_cover_shape_order_coverage_and_branch_join`、
`deferred_explicit_constructor_type_arguments_publish_no_construction_fact`、
`top_level_initializers_publish_stable_cross_file_symbol_and_expression_types`、
`unit_lambda_diagnostics_stop_jumps_and_returns_at_callable_boundary`。
不删除或弱化这些测试，本阶段不声称 unit suite 全绿。

## 8. 最终交付与关闭验收（2026-10-02）

实现提交 `668473e41ff6c9123058e4d4f4c0cff993603354` 最终经
[PR #7](https://github.com/Halckon/Koven/pull/7) head
`11051e200441a21cdf6dee6a6d153d2e9ffe26c6` 合并为
`e22e11b736aab1231209e3403bd0c931b9ddb940`，包含于复核基线
`34189046319a8b727285d471596647d5de56996e`。
该 head 的 [CI 36877486546](https://github.com/Halckon/Koven/actions/runs/36877486546)
8/8 jobs success，Ubuntu/macOS 的 check、strict clippy、core、stage 与 Guide 步骤均实际成功。
该版本 stage 明确选择 `type_ownership_primitives` 与相关受影响 suites，core 执行 frontend
lib 的结构 validation 测试及完整 codegen；不宣称原语 native 或 frontend 全量已因此通过。

| 原验收项 | 直接证据与关闭判断 |
|---|---|
| §4.1 red、来源与 rollback | §5 保留 24 处 API 缺失编译 red、两项结构损坏 red 和 deferred operand 的 L0084 red；[17 项 integration](../../../crates/lang-frontend/tests/type_ownership_primitives.rs)的 source-function/imported-alias 负例、explicit-environment binding、invalid/failed-overload 与成功 trial-once 明确核对绑定来源及事务提交 |
| §4.2 descriptor 与查询 | [single descriptor](../../../crates/lang-frontend/src/type_checking/ownership_primitive.rs)只有 crate 内构造器；integration 的 single/unit 精确核对 Replace/Swap、交换类型、call contract、两个源码顺序 operand，unit 保留 source-qualified identity 及输入反序稳定性 |
| §4.2 validation | [unit validation](../../../crates/lang-frontend/src/type_checking/compilation_unit/bodies/ownership_primitive.rs)中 `primitive_validation_rejects_inconsistent_source_type_operand_and_call_facts` 与 `constant_enabled_validation_also_checks_primitive_structure` 覆盖原账本的 14 种损坏及 basic/const 两入口；仍不把它称为从 AST 独立重建 operand 映射 |
| §4.3 Nothing 与静态边界 | integration 的 divergent replace/place-prefix、closure-body、short-circuit、if-condition 测试保留真实静态 identity，deferred/error cases 不发布成功表；descriptor 未发布 continuation 布尔值，不把静态 call 当作 ownership 原子 commit 许可 |
| §4.4 定向与下游门禁 | §5 251 项定向、2 项 validation、478 项 codegen 与 fmt/strict clippy/workspace check 的本地结果保留；最终双宿主 CI 实际覆盖对应 suite/core，未新增本 Spec 的 native 能力声明 |
| §4.5 交付 | 最终 PR7、精确 CI 与 main merge 相互对应，已满足可信 typed 身份的原 Goal；文档状态/路径与 inventory 随生命周期收尾同步 |

§5、§7 的待发布与未运行是历史记录；本轮 docs-only 没有新运行 Cargo。原五项 multifile
失败由后继 [SPEC-0247](0247-multifile-baseline.md) 单独修复，本合同不以 CI 绿灯追溯改写。
后继 [SPEC-0244](0244-root-ownership-primitives.md) 交付 owned root，
[SPEC-0246](0246-direct-field-replace.md) 交付有界 direct field replace。
这些下游成果不扩大本 Spec 的 Phase 2 交付；nested/index place、字段 swap、普通 Inout ABI、
通用 continuation 与 closure/provenance 转移仍须按各自合同验收，不能用 typed descriptor 代替。
