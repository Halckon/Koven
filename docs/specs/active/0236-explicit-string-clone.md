# SPEC-0236: String.clone 显式深拷贝端到端

> **性质**：变更合同 · **状态**：in-progress · **读取时机**：实施或验收 String.clone 时 · **唯一真源**：本 Spec

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P234-0236` |
| 所属 Phase | Phase 2 / 3 / 4 |
| 语言规范 | [Koven v0.39](../../guide/README.md)，[String 封闭操作](../../guide/13-program-runtime-standard-library.md#封闭的最小操作) |
| 批准依据 | 2026-10-01，用户对“先落地 String.clone()，暂缓 Str 和 toString() 的规范切换”明确回复“认可，开始实施” |
| 前置 Spec | [SPEC-0192](../../archive/specs/0192-general-string-runtime.md)（done） |
| 前置 ADR | [ADR-0018](../../adr/accepted/0018-string-owner-runtime-abi.md)（accepted）、[ADR-0027](../../adr/accepted/0027-explicit-string-clone-abi.md)（accepted） |
| 关联 ADR | ADR-0027 |
| 阻塞项 | 无语义阻塞；实现验收与 PR CI 尚待完成 |
| 影响范围 | `lang-frontend`、`lang-codegen`、`lang-cli` 回归测试、文档与结构门禁 |
| 语言语义变更 | 是；仅启用 v0.39 的 String.clone 增量及 String literal / Str 冲突消解 |

## 1. Goal

单文件与 compilation-unit 编译链均能将 builtin `String.clone()` 编译为 shared Borrow 源、
返回独立新 owner 的显式深拷贝；源与结果分别遵守普通 move、loan 和 ASAP drop 规则。

## 2. 背景

一般 String runtime 已有唯一 owner、literal、concat、比较与输出，但缺少将借用值物化为
独立 owned String 的封闭操作。本次从实际 main v0.38 基线增加该能力；完整语义以
[v0.39 String](../../guide/13-program-runtime-standard-library.md#string) 为准。
不把任何尚未合并的 SPEC-0235 分支规则带入本次 guide；0236 先集成，0235 之后重基到后续
v0.40，不能与本次 v0.39 并列成为 current。

## 3. 范围与需求

- Phase 2 在单文件与 compilation-unit 路径发布稳定的 String clone intrinsic identity、
  receiver 类型、shared Borrow effect 和 owned String 结果；绑定 builtin identity，不以
  用户声明的 `String` / `clone` 名称冒充 intrinsic。
- Phase 3 消费 typed facts，建立调用期 shared loan，保留源 owner 可用性；结果形成独立
  owner obligation。覆盖局部值、Borrow 参数、临时值、以及已支持的 owned / Borrow
  顺序容器的 String 元素 place，不改变元素的不可移动规则。
- Phase 4 通过专用 `StringClone` SSA operation 运输该契约；verifier 要求有效 shared loan、
  String operand/result 与独立 owner。LLVM 复用目标布局、集中 malloc/free/abort adapter，
  遵循 ADR-0027 的非空精确长度分配与空串 canonical storage。
- 沿用既有诊断类别与 span；错误参数/类型实参、错误 receiver、move 后读取等不得绕过
  frontend 检查，也不得靠 backend 成员名称分支补语义。
- `String?` 沿用一般 nullable 规则，不添加 nullable clone 特例；既有 inline-nullable
  String native ABI 限制保留。

## 4. 非目标

- `Str`、Str→String、`toString()`、interpolation 转换协议；字面量仍是普通 String owner。
- `Copyable` / `Cloneable` 能力扩展、ARC/GC、共享缓冲区、SSO 或容器整体深拷贝。
- 一般 extension/member 机制、通用 clone 协议、其他 builtin clone 或常量求值 clone。
- SPEC-0235 的其他语义内核规则、inline-nullable String ABI 或新增 workspace crate。

## 5. 验收标准

- [x] 单/多文件 typed 合同可见；仅 builtin String 的合法零参零类型实参调用绑定 intrinsic
- [x] source 在 clone 后仍可读，结果可独立 move / 返回 / 捕获；Borrow 参数和临时源合法
- [x] owned 与 Borrow 容器元素 clone 保留元素 owner；直接 owned 读取仍按既有规则拒绝
- [x] moved source、错误参数/类型实参、错误 receiver 的诊断与 span 回归覆盖
- [x] SSA 正向 verifier 与错误类型、缺失/失效/错误模式 loan 等负向用例覆盖
- [x] native 覆盖 heap/static/empty、Unicode、内嵌 NUL、单/多文件及源/结果独立生命周期
- [x] 非空结果 malloc 精确 length 并完整复制；空串不 malloc；drop 不重释放或释放静态存储
- [x] 受影响 Rust 定向门禁与下游检查已执行；clone 相关门禁通过，既有5项失败单列，记录实际命中数
- [x] Architecture 更新为经代码与测试确认的事实；Guide / ADR / proposal / inventory / DAG 一致
- [ ] PR 对应最终提交的必需 CI 全绿且无未决状态后，迁移 Spec 到 archive 并同步索引

## 6. 技术方案与边界

类型检查、ownership 和 SSA 只消费上一阶段发布的事实；单/多文件入口对外语义相同，
可共享窄领域 helper，但不得用单文件循环替代 compilation-unit 分析。
长期布局与分配决定见 [ADR-0027](../../adr/accepted/0027-explicit-string-clone-abi.md)，
原始布局和 drop provenance 继续由 ADR-0018 规定。frontend 不包含 pointer/capacity/LLVM 事实。

## 7. 实施计划

1. [x] 完成 typed intrinsic 和 ownership 事实运输 → 定向 frontend 正反例
2. [x] 完成专用 SSA/verifier 与 native helper → 定向 codegen/SSA/native 检查
3. [x] 同步实现事实和文档治理 → Python 文档门禁及迁移校验
4. [ ] 填写实际验收、创建 PR、等待 CI 后归档 → 最终提交与 CI 证据

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | String.clone 全链路、测试与 v0.39 文档 | `feat(string): add explicit owned clone (SPEC-0236)` |
| 2 | 必需 CI 验收与 Spec 归档 | `docs: close String clone acceptance (SPEC-0236)` |

所有工作在 `feature/spec-0236-string-clone` 范围封闭交付；本合同的 in-progress 状态不宣称本地测试或 CI 已完成。

## 9. 未决问题

无语义未决项。本地实现与所列定向验证不等于远程 CI 完成；当前未获新分支发布授权，
只做本地阶段提交，保持 active / in-progress。

保留的 native 边界：Borrow `Rc<String>` 参数的 `.value.clone()` 仍确定性返回
UnsupportedNode（单/多文件负测），不把 handle-slot pointer 当 control block；本轮不扩大
旧 Rc / nullable ABI。owned Rc payload、String 参数与 owned/Borrow 容器元素不受此限制。
`String?` inline ABI、safe-call 以及既有未封闭 receiver 形状仍为原边界。

## 10. 验证记录

| 验收项 / 命令（目标与过滤器） | 结果（实际测试数） | 未运行原因 / 复用证据 |
|---|---|---|
| 工具链 | Rust 1.96.0 / LLVM 21.1.8 / Clang 21.1.8 | x86_64 Linux + glibc；统一 target 复用，未运行 macOS |
| Red：`cargo test -p lang-frontend --test string_clone` | 0 passed / 4 failed | 未实现时 MemberReceiver deferred / 元素移动诊断；同选择修复后 4 passed，后续扩至 14 |
| Red：native `string_clone` | 首轮 2 failed | 其中一个 fixture 的数字 Unicode escape 未定义，先改为合法 `\0`；容器 receiver L0136 为真实功能缺失 |
| 审查新增投影 Red：`cargo test --locked -p lang-codegen --lib string_clone` | 8 passed / 2 failed | 临时容器元素与 unit Rc payload InvalidSsa；修复后同范围全绿，后续扩展字段与边界负测 |
| `cargo test --locked -p lang-frontend --test string_clone --test ownership_checking --test ownership_containers --test ownership_construction --test ownership_closures --test ownership_rc --no-fail-fast` | 83 passed，0 failed / ignored / filtered | 14 clone、30 ownership、12 containers、8 construction、15 closures、4 Rc |
| `cargo test -p lang-frontend --test string_clone --test ownership_rc --test type_checking --test type_callable --test multifile_type_checking --test multifile_ownership_checking` | 170 passed / 5 failed，退出101 | 实际先运行multifile ownership 71/71，再运行multifile types 99/104；未使用no-fail-fast，失败后其余选择未执行 |
| `cargo test -p lang-frontend --test string_clone --test ownership_rc --test type_checking --test type_callable` | 118 passed，退出0 | 补跑前批未执行项：当时clone13、Rc4、type_checking82、type_callable19；追加unit drop断言后clone单独14/14，最终83项批次再次覆盖 |
| 5项 multifile types 基线 | 未修复 | 与 SPEC-0228 已登记名称/失败一致：companion_constant_initializers_publish_stable_ordinary_typed_facts、top_level_initializers_publish_stable_cross_file_symbol_and_expression_types、deferred_explicit_constructor_type_arguments_publish_no_construction_fact、cross_file_when_diagnostics_cover_shape_order_coverage_and_branch_join、unit_lambda_diagnostics_stop_jumps_and_returns_at_callable_boundary |
| `cargo test --locked -p lang-frontend --lib ownership_checking::checker::drop_planner:: -- --nocapture` | 96 passed，81 filtered，0 failed / ignored | drop planner共享路径、循环/快照/实例回放 |
| `cargo build --locked -p lang-cli` | passed，退出0 | 实际Linux CLI构建 |
| `cargo check --locked --workspace --all-targets` | passed，退出0 | 最终 Rust 全target编译 |
| `cargo clippy --locked --workspace --all-targets -- -D warnings` | passed，退出0 | 无新增 lint allow；新负测初版 expect_err 缺 Program:Debug 已修正后复验 |
| `cargo fmt --all -- --check` | passed，退出0 | 最终格式门禁，新增capture/负测已包括 |
| `cargo test --locked -p lang-codegen -p lang-cli --no-fail-fast` | 563 passed，0 failed / ignored / filtered | codegen493、CLI48单元+3format+9native+6project、4codegen doc-tests；包含clone结果move capture与提前return |
| native allocation/drop counters | passed | 非空 dynamic/clone/static-clone 4次malloc精确匹配4次free，empty clone不分配；拒绝静态free/重复free/泄漏；allocation failure先abort |
| frontend全量 / macOS / 远程CI | 未运行 | 遵循定向门禁；当前宿主仅Linux；本地阶段尚未发布。此前3项call-argument基线失败未重跑，也未声称修复 |
| `python3 -m unittest discover -s scripts/tests -v` | 通过，26 tests | 2026-10-01；含 current 版本唯一性、页元数据与 proposal 候选边界回归 |
| `python3 scripts/check_docs.py` | 通过，405 Markdown files | 文档结构检查不替代语义等价证明 |
| `python3 scripts/gen_spec_dag.py` | 已生成 | 当前 3 live + 213 archive；全图 216 Specs |
| v0.38 归档与 SHA-256 账本核对 | 16/16 通过 | 对 main d3e64a4 原页仅机械重写跨目录相对链接；其余 14 个领域页仅 v0.38→v0.39 |
| PR 必需 CI | 未完成 | 不以本地定向测试替代最终提交 CI |
