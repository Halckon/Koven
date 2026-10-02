# SPEC-0245: 具体普通 class 的资源析构闭环

> **性质**：变更合同 · **状态**：done · **读取时机**：实施或验收资源 lexical drop 与用户 deinit 时 · **唯一真源**：本 Spec 范围与验收；语言语义由 Guide 定义

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-RESOURCE-0245` |
| 所属 Phase | Phase 2 类型事实、Phase 3 所有权、Phase 4 SSA/LLVM/native |
| 语言规范 | 已启用 [Guide v0.40](../../guide/README.md) |
| 批准依据 | 用户批准按现行演进计划分阶段实现、验证并持续发布 Draft PR 与双平台 CI |
| 前置 Spec | 从 main `17a7a0e` 独立实施；用户合并 PR #11 后按要求合并 main `8eb2cd3`，保留 SPEC-0244 原语能力 |
| 前置 ADR | 无新增架构决定；复用现有 Borrow 与 drop ABI |
| 语言语义变更 | 否 |
| 影响范围 | frontend type/ownership、codegen SSA/LLVM/native、文档与门禁 |

## 1. Goal

在不改变 [deinit receiver/body/字段契约](../../guide/08-class-family-members.md#deinit-成员语法与资源析构契约)
及[双轨析构](../../guide/10-ownership-borrowing-drop.md#双轨析构策略纯内存与资源类型)的前提下，
交付普通、具体、非泛型 class 的端到端资源清理；支持普通 class 递归持有资源字段。
不能把 parser 接受、类型分类或 LLVM 函数存在单独称作析构已执行。

## 2. 范围与非目标

- 两条类型入口发布可信 deinit 身份/body/只读 receiver descriptor 和递归资源分类。
- 两条 ownership 入口区分资源词法清理与纯内存 ASAP，消费/移动不产生重复义务；
  scope、return、break、continue 按现行路径规则生成 drop。
- lowering 只消费已验证 descriptor，隐藏 body 仍使用既有 Shared Borrow receiver ABI；
  typed SSA 检查精确 owner/function 身份，LLVM 在字段逆序清理及 free 之前调用用户 body。
- 用 UTF-8 stdout 与真实 object/link/run 证明 body/字段/变量顺序、可读字段、移动与控制退出；
  Abort 不展开，纯内存既有 ASAP 回归不改变。
- 首片不声称 generic、nullable、Box/容器、value/enum 或 closure 包装资源的 native 完整支持，
  不支持的资源实例须显式拒绝，不得默默省略 body 或提前 ASAP。
- 不新增 NLL、借用返回、raw pointer、异常展开、语言 surface 或普通析构调用。
- 分支不对称消费外层资源时，single frontend 保留条件义务而 backend 精确拒绝；
  unit frontend deferred 阻断。循环消费外层资源的旧运输同样 deferred；
  不能借分支末提前释放绕开 lexical 合同。

## 3. 验收账本

下表为合并 main `8eb2cd3` 后的验收。首轮独立提交 `2dfc238` 曾通过 codegen575、frontend lib181、stage61 targets/771；用户随后合并 PR #11，现保留两项能力重新验证。

| 验收项 | 测试目标/命令 | 实际结果 |
|---|---|---|
| TDD 类型 API / 泛型 | `resource_deinit_type_facts` | API 缺失编译红；泛型分类 4 红后修复；最终 11 passed，含 cache Eq/Debug、return 正反例 |
| TDD lexical cleanup | `ownership_resource_deinit` | 初始 7 项生命周期行为红；最终 26 passed，含纯内存 generic/closure ASAP、只读 this 与资源×root 原语 |
| TDD body / readonly | 单文件 body、unit body/fixed-point、unit return context | 初始无 body、错误 ASAP、wrapper 被接受及 return L0086 红测；实施后相应测试全绿；私有 enclosing-loop 红→绿 |
| SSA/LLVM/native | `cargo test --locked -p lang-codegen` | 634 passed + 4 compile-fail doctests，0 failed/ignored；含真实 object/link/run |
| 所有权扩大回归 | `ownership_checking` / `ownership_closures` / `ownership_iteration` / `multifile_ownership_checking` / 两入口 two-phase | 31 / 15 / 184 / 72 / 18 / 27 passed；未放宽既有断言 |
| frontend lib | `cargo test --locked -p lang-frontend --lib` | 184 passed，0 failed/ignored |
| CLI / LSP | 各 crate `cargo test --locked -p` | 66 / 26 passed |
| stage integration | `bash scripts/check_stage_integration.sh` | 62 targets / 793 passed，0 failed/ignored/filtered |
| Guide | `bash scripts/check_guide_litmus.sh` | 187 frontend + 14 codegen = 201 passed；codegen 筛选另有 1888 filtered，不计作通过 |
| 静态/格式 | workspace all-targets check、严格 clippy、fmt | 全通过，`-D warnings`；无新增 lint 豁免 |
| 文档/Python/diff | `check_docs.py`、`unittest discover -s scripts/tests -v`、`git diff --check` | 453 Markdown / 45 tests / whitespace 全通过 |
| Draft PR 与双宿主 | exact head 的 macOS/Ubuntu PR CI | head `43254d66` 的 PR #12 run `36968063513` completed/success，8/8 jobs；以下记录双平台实测 |

native 的 stdout 证明 scope/body/字段逆序、字段在 body 中可读、Value delivery、temporary、
return/break/continue、替换及 Abort 不展开。单文件 nested fixture 6 allocations/6 frees，
unit 两入口各 4/4，均逐 pointer 验证。unit 另验证 whole-this shared reborrow、
String clone、nested 字段 loan 穿过后续控制实参；反转 inputs 保持 SSA/LLVM/object 确定性。
条件 outer-owner 不在已支持 native 范围；拒绝点和既有输出保全有专门负测。

实现中抓到并修复 nullable closure 被误作 resource unknown、泛型 callable 被资源 pass 抢先解析
及两入口 readonly diagnostic 差异；既有测试断言和实例预算保持。测试 fixture 的 String-only
println 与未支持 List member 访问改用合法 top-level Borrow probe，保留 capture/ASAP 断言。

### main 同步与原语交叉

用户于 2026-10-02 合并 PR #11 后，先保存完整 Git bundle，再合并 `8eb2cd3`。
3 个 Rust 注册/模型冲突同时保留 deinit 与 root exchange 字段/模块，3 个生成图冲突重生成，
未改两者生产语义。新增资源×replace/swap 交叉测试验证提交无提前析构、旧 owner 返回与 Borrow
temporary 唯一交付、交换后按变量声明逆序清理实际对象、pending return/break/continue 与 Abort。
最终资源 ownership26 + 原语14、native8 实际通过；native 单文件与两条 unit 入口均逐 pointer
验证 9 allocations/9 frees。Owner ID 精确运输、旧值作为外层调用前缀遇 return/Abort、
直接 return replace 的唯一交付都有独立断言，既有 main 类型/原语回归全量保留。

## 4. 交付规则

本地工具链 Rust 1.96.0、LLVM/Clang 21.1.8，Linux x86_64/glibc；共享 Cargo target 串行，
`CARGO_INCREMENTAL=0`。阶段 checkpoint 与 Git bundle 保留。完成 bounded acceptance 后归档，
验证归档最终 head 的双平台 CI；保持 Draft，由用户决定合并。无全量 frontend 通过声明，
不改无关历史失败、ignore 或性能预算。

## 5. 初始实现 head 双平台 CI

[PR #12](https://github.com/Halckon/Koven/pull/12) 的 head
`43254d66a5e01beba3fca987d5601e658eea3082`、tree
`ddb11d83ef9052d2143b34c10b6e23b0fd72dcf5` 与本地验证树一致；66 个 blob SHA 逐一核对。
[CI run 36968063513](https://github.com/Halckon/Koven/actions/runs/36968063513)
于 2026-10-02 completed/success，8/8 jobs success，无 job skip。两平台 check、严格 clippy、
Core、stage、Guide 与 CI Passed 均实际执行，原语和资源 native 交叉用例均通过。

| 宿主 | frontend lib | codegen / doctests | CLI / LSP | stage | Guide |
|---|---|---|---|---|---|
| Ubuntu 24.04 | 184 | 634 / 4 | 66 / 26 | 62 targets / 793 | 187 frontend + 14 codegen |
| macOS 14 | 184 | 633 passed、1 既有 ignored / 4 | 65 / 26 | 62 targets / 793 | 187 frontend + 14 codegen |

macOS 的唯一 ignored 是既有 LLDB source-breakpoint 测试，CI 无 debugserver task-port 权限；
不计作 debugger 验证通过，没有新增 ignore。Guide 的 codegen 筛选另有 1888 filtered，均不计作通过。

本次归档只迁移 Spec、更新索引/inventory/生成图及已验证事实，不再改 Rust 或 Rust 门禁。
归档后 head 仍执行完整双平台 PR CI；实际最终结果回写 PR，不继续产生自引用账本提交。
PR 保持 Draft，合并由用户决定。
