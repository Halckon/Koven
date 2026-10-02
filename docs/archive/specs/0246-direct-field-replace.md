# SPEC-0246：owned local 普通 class 一级字段 replace

> **性质**：实施 Spec · **状态**：done · **读取时机**：实现或验收有界一级字段 replace 时 · **唯一真源**：本 Spec

| 字段 | 值 |
|---|---|
| 状态 | `done` |
| Goal ID | `KOV-P4-246` |
| 所属 Phase | Phase 3 ownership；Phase 4 SSA / LLVM / native |
| 语言规范 | [Guide10 所有权与原子置换](../../guide/10-ownership-borrowing-drop.md#原地置换原子原语replace-与-swap) |
| 批准依据 | 用户持续批准按现行 Guide10 开发有界 field replace，并按“本地验证 → Draft PR → 双平台 CI”发布；合并由用户决定 |
| 前置 Spec | SPEC-0244（done）的 owned root 原语与既有 typed intrinsic 身份链 |
| 前置 ADR | ADR-0006、ADR-0008（均 accepted） |
| 关联 ADR | 无新增 ADR；复用 typed SSA、block parameters 与现有 class payload ABI |
| 阻塞项 | 无；有界本地验收与首轮本 PR 双平台 CI 完成；归档 head 复验由 PR 跟踪 |
| 影响范围 | `lang-frontend` ownership；`lang-codegen` 两入口 lowering、SSA verifier、LLVM/native；当前文档 |
| 语言语义变更 | 否 |

## 1. Goal 与基线

从 fresh main `8eb2cd3668608395889358df5260f6b13568454b` 建立独立
`feature/spec-0246-direct-field-replace`，为 owned local、普通非泛型 class 的一级可变字段
实现 `replace(&owner.field, new)`，使旧字段值作为唯一 owned 结果交付，父 owner 始终完整
初始化且正常继续可用。receiver 绑定可以是 `val` 或 `var`；可写性来自字段 `var`，不把
owned root replace 的 mutable binding 要求错误套到 class handle。

PR #12 的 deinit 切片在此基线尚未合并，本片不依赖、不合入，也不借用其析构语义或验收。
Guide 已允许更广 place，本片只闭合下述实现范围，不修改 Guide 或声称完整支持 Guide 示例。

## 2. 范围与不变量

- 只消费 compiler-bound `Replace` descriptor、既有 aggregate projection 与真实 loan/value
  delivery facts；同名用户函数不能获得内建执行权限。
- receiver 限 owned local、普通非泛型 class，投影恰好一个已解析 `var` 字段，允许 Group；
  class 声明与使用处限同 source；不重求值 receiver，不按源码名称或后端布局猜测 field identity。
- 第一实参建立 immediate exclusive loan；新值完整求值前旧字段仍 initialized。新值只求值
  一次，交付沿普通 Value 的 Copy / Move / Temporary，所有 operand 正常继续后才 commit。
- 正常 commit 返回旧 owner，不触发普通字段 assignment 的 old-value drop，不消耗父 owner。
  返回旧值可借用、移动或返回，且只能承担一次最终释放义务；父对象随后清理新字段一次。
- Copyable 输入和返回值保持独立快照，不把共享 SSA Value 或父 owner alias 当成可变存储共享。
- `return` / `break` / `continue` 取消尚未提交的前缀，先结束 loan，再消费已有清理事实；
  内层循环跳转保留外层 pending owner/loan。`Abort` 不 commit、不 unwind、不调用清理。

## 3. 技术合同

### Phase 3：独立字段能力

`FieldReplaceOwnershipPlan` / `UnitFieldReplaceOwnershipPlan` 独立于
`OwnershipPrimitiveOwnershipPlan` / `UnitOwnershipPrimitiveOwnershipPlan`。公开
`field_replacements()` / `field_replacement(expression)` 只读查询；保留 typed descriptor、
精确字段 place、父类型与新值交付类别，unit identity 必须 source-qualified。

正常继续才发布字段 commit；错误或 blocking deferred 清空可执行事实。unit 基础及
constant-enabled gate 均核对 source、descriptor、exclusive loan、参数合同与新值交付。
var/local/projection 合法性由封闭 producer 证明；gate 通过 call-qualified 原已批准 target
快照防止字段/receiver 身份漂移，不把所有同类型 sibling 当作授权。不允许 typed descriptor
单独授予执行能力。字段计划
不能进入 root 查询，root 计划也不能被字段 lowering 接受。

### Phase 4：显式字段交换

`HeapFieldExchange { owner, field: usize, loan, replacement }` 只返回一个旧字段值。
它消耗 active exclusive loan 与 MoveOnly replacement，保持父 `owner` identity 可用。
verifier 必须证明 loan 对应同一 owner 的精确
`HeapPayloadPlace → FieldPlace(field)` 路径；同类型、alias root 集合相同或字段下标相同
都不能替代此证明。经过 CFG 的 owner/loan 按每条 edge 成对验证。

拒绝错误 owner/field、越界、root loan、entry Inout loan、reborrow、失效/shared loan、
双消费、新值仍被借用、旧值泄漏、父 owner 泄漏和错误结果 arity/type；root 专用操作仍拒绝
字段 place，不能借本片放宽 `RootReplace` / `RootSwap` 的 provenance 规则。

两条 lowering 入口都保留 source order、pending owner/loan 与提前退出边；LLVM 在 exclusive
保护下先取旧字段再写新字段，中间不插入 drop、retain、clone、分配或用户调用。复用现有
payload 布局和内部 ABI；“原子”不是跨线程 CPU 原子指令。

## 4. 非目标

- nested fields、`this` / 隐式字段、Borrow/Inout 等 non-owning receiver、参数或 global receiver、
  temporary receiver、index place、generic receiver、value-class receiver
- 同一父 owner 的字段 loan 活跃期间进行 sibling read/clone/嵌套 exchange，包括
  `replace(&h.a, replace(&h.b, new))`；
  现有 SSA AliasRoots 将父对象各字段视作共同 root，后端须确定性拒绝。这是本片实现边界，
  不把 Guide 可证明 disjoint 的 sibling places 改写为语言非法
- 普通 class 字段的 Borrow 实参（如 `println(h.state)`）仍是既有 unsupported；native 可用
  再次 replace 提取新字段或读取 Copyable `h.id` 证明父对象继续可用
- unit 跨 source class `var` 字段可变性实测 L0134 的既有缺口不扩；跨文件 `make()` replacement
  已覆盖。single Unit 字段构造缺事实、constructor `Node → Node?` 隐式包装、unit `Rc<Int>?`
  名义字段 layout Unsupported 保留；显式 nullable local 初始化后的交换正例不证明前述入口
- 普通 field 值直接用于复杂 `&&` 的既有 MissingFact 不扩；native 先读 current local 再比较，
  不以该快照用例声称新增短路支持
- field swap、closure 字段及含 closure 的未闭合 owner/provenance、普通 Inout 参数 ABI
- deinit / 资源词法析构、raw pointer、借用返回、NLL、跨线程共享或新的语言/ABI 决策
- 修复无关 multifile type 或完整编辑器 corpus 历史失败；运行或声称 frontend 全量通过

## 5. 验收标准

- [x] 两入口正反例验证 `val` / `var` receiver、独立 field/root facts、旧 owner 唯一、
  L0131 重复消费、L0134 不可变字段、L0135 重叠、自读/自移动冲突、L0122 非 place 与 source-qualified 身份。
- [x] unit 基础与 constant-enabled gate 拒绝损坏字段事实，错误原子清空、source 身份与顺序稳定。
- [x] 两入口 SSA 生成唯一 `HeapFieldExchange`；verifier 正反例覆盖精确 loan 路径、CFG 成对
  provenance、消费/泄漏/类型/结果 arity，root 专用回归不放宽。
- [x] single/unit native 实际 object/link/run 覆盖求值一次、父 owner 后续可用、旧值借用/返回、
  Copyable 快照、early return / break / continue 与 Abort 无 unwind。
- [x] 分配/释放插桩逐 pointer 检查新旧字段和父对象恰好释放一次，不能只比较总计数。
- [x] 定向 frontend、codegen/native 及按共享路径选择的回归、workspace check、严格 clippy、fmt。
- [x] 文档结构、inventory/生成图、diff 检查与当前 Architecture 同步。
- [x] 一个 Draft PR 发布至 main，macOS/Ubuntu 本 PR 实际 CI 完成；未执行/filtered/ignored 单列。

## 6. 实施与交付

1. [x] 先加入 frontend API 与 native 行为红测，记录基线失败及测试夹具错误。
2. [x] 独立 ownership capability、两入口降低与 SSA/LLVM 已落地且直接验证通过；
   global/captured receiver 与 paired target 变异均有实际红绿证据。
3. [x] 完成有界本地验收，补齐下表与 Architecture，保存可恢复的独立提交/bundle。
4. [x] 发布 Draft PR 并完成首轮双平台 CI；按有界验收归档，不自动合并或改 ready。

已保存初始 checkpoint `dc5040b`、完整实现 `a4ae359` 及验证过的完整 bundle；不将恢复点
当作最终验收通过。提交不混入 PR #12，最终修复、门禁与发布记录继续留在本分支。

## 7. 验证账本

| 验收项 / 目标或过滤器 | 实际结果 | 限制与后续 |
|---|---|---|
| 实施前 frontend `ownership_field_replace` | 编译失败：18 处缺失字段 ownership API | 先有红测；不是 18 个运行期失败测试，也不计作通过 |
| 实施前 single native `field_replace_native` | 4 个预期 Unsupported 失败；另 1 个重复 source 名夹具失败 | 夹具名称已修正；修正不代表实现或 native 通过 |
| 最终 frontend `ownership_field_replace` / lib `field_replace` | 13 integration + 3 lib passed | 包含 global/captured 排除、错误清空、source identity 与 paired 变异拒绝 |
| 5 项 frontend 相关 suite | 145 passed | `ownership_checking` 72、`multifile_ownership_checking` 31、field 11、`ownership_primitives` 14、`type_ownership_primitives` 17；非全量 |
| codegen lib `field_replace` | 25 passed | 包含 3 项既有匹配、22 项新增；不将过滤数或后续批次相加 |
| codegen lib `field_exchange` | 14 passed | 13 项 SSA + 1 项 LLVM；LLVM 内含 38 个 case，不记为 38 个独立测试 |
| single `field_replace_native` | 6 passed | 真实 object/link/run、逐 pointer 计数、提前退出及 Abort 无 unwind |
| 后续 unit field replace 直接测试 | 14 passed | 含 SSA/native、跨文件 replacement、基础/constant-enabled 两入口，不与早先过滤批次相加 |
| Member-as-Place 父 owner 存活 TDD | single/unit 曾提前 ASAP 释放，已修复且直接验证通过 | 投影不得视为父 owner 完整 Value 交付 |
| temporary receiver Phase 2 | 两入口按非 place 报 L0122 | 现有准确阶段；不是后端错误或新 borrow 语义 |
| global/captured receiver TDD | 各 1 项精确红测，修复后最终 suite 全通过 | 分别限制 local scope 与 captured provenance；root 原语行为不改 |
| unit paired target TDD | 1 项红测后通过 | plan+loan 配对改成同类型 val sibling 或另一 local，均被 per-call 快照拒绝 |
| workspace check / 严格 clippy / fmt | 最终源码通过 | all-targets、`-D warnings`；没有新增 lint 豁免 |
| 最终 core | frontend lib 186；codegen 631 + 4 compile-fail doctests；CLI 66；LSP 26 passed | Linux 实际 object/link/run；所有测试通过、无新增 ignore |
| 最终 `bash scripts/check_stage_integration.sh` | 61 targets / 769 passed | 已执行新字段 suite，仍非 frontend 全量 |
| 最终 `bash scripts/check_guide_litmus.sh` | 187 frontend + 14 codegen = 201 passed | 另有 1879 filtered，不计通过；known gaps保持 |
| `python3 scripts/check_docs.py` | 453 篇 Markdown 通过 | 最终实现与 inventory/四份生成图同步；仅结构验收 |
| `python3 -m unittest discover -s scripts/tests -v` | 45 passed | 最终文档/CI policy 脚本测试 |
| `git diff --check` | 通过 | 最终实现与验收文档 |
| 首轮本 PR macOS / Ubuntu CI | run 36973062892 completed / success，8/8 jobs success，无 skip | exact head `6bd7ede4`；两宿主 core/stage/Guide 全部成功，明细见下节 |

全部本地命令使用 Rust 1.96.0、LLVM/Clang 21.1.8、Linux x86_64/glibc；
Cargo 共用一个 target 且串行，`CARGO_INCREMENTAL=0`。workspace check 与 clippy 均为
`--locked --workspace --all-targets`，clippy 追加 `-- -D warnings`；没有降低既有断言或预算。

## 8. 历史失败与保留边界

SPEC-0242 的五项 `multifile_type_checking` 和五项完整编辑器 corpus 历史失败原样保留，
名称与证据见[演进实施账本](../../specs/evolution-status.md#已知独立基线失败)及
[编辑器精确边界](../../specs/active/0242-automatic-borrow-call-migration.md#编辑器精确边界)。这些历史结果
不算本片重跑；未运行 frontend 全量，不新增 ignore、不降低断言或放宽门禁。


## 9. 首轮远端完成证据与归档

[Draft PR #13](https://github.com/Halckon/Koven/pull/13) 的首轮发布 head
`6bd7ede462a1f6b964ad5b872583d4a31e88b299` 已完成
[pull_request run 36973062892](https://github.com/Halckon/Koven/actions/runs/36973062892)。
2026-10-02 读回 exact head、event `pull_request`、status `completed`、conclusion `success`；
全部 8 个 job 成功，无跳过的必需 job，两宿主 core、stage、Guide step 均为 success。

remote tree `ec48532bdd58f2cf16589fee6ef006d672692bb9` 与本地已验证提交
`f029f8cbcd0fbf04ace8adda18c58a9d3d94bc79` 完全一致；62 个 blob 逐一核对，并经
`git fetch` 后的 tree/diff 再验证。该证据不借用其他 PR，基线 main `8eb2cd3` 仍未包含
PR #12 deinit；本片保持独立。

| 宿主 | frontend lib | codegen / doctests | CLI / LSP | stage | Guide |
|---|---|---|---|---|---|
| Ubuntu 24.04 | 186 | 631 / 4 | 66 / 26 | 61 targets / 769 | 187 frontend + 14 codegen = 201 |
| macOS 14 | 186 | 630 passed、1 既有 ignored / 4 | 65 / 26 | 61 targets / 769 | 187 frontend + 14 codegen = 201 |

每个平台 Guide 另有 1879 filtered，不计入通过数。macOS 唯一 ignored 是既有
`lldb_hits_a_koven_source_breakpoint_and_reports_the_frame`，原因是 CI 缺少 debugserver
/task-port 权限；它不算调试器通过，没有新增 ignore。未运行的 frontend 全量、五项
multifile type 与五项完整编辑器 corpus 历史失败及全部有界实现限制保持。

本 Spec 按声明的 direct-field replace 切片完成验收并归档，PR 保持 Draft、合并由用户决定。
归档变更只同步本 Spec 状态/路径、现行事实、inventory 与生成图，不改变已验证的 Rust 或
门禁脚本。最终归档 head 发布后还会再验证一次双平台 CI；最终结果只更新 PR，避免为记录
最后一轮 CI 反复产生账本提交。首轮成功不冒充尚未执行的最终归档 head 结果。
