# E3 所有权细化层对账（2026-11-21）

> **性质**：当前交付账本 · **状态**：current · **读取时机**：核对 E3 交付与兼容路径 · **唯一真源**：下列 Git 快照及指向的测试 · [历史发现与证据](ownership-refinement-gaps.md)


[PR #454](https://github.com/Halckon/Koven/pull/454) 已合入 `main`（`c27bfd57`）。
下表记录已经交付的固定形态；后文的本地候选与基点是合入前的历史证据，不是当前待合清单。

| 项目 | 主干已交付的固定形态与边界 |
|---|---|
| A2 · Shared 接收者返回 Boolean | owned 非 Resource、一次无实参调用走 Refine；限定形态有 Protocol 2 收据。 |
| A2 · 一个显式整数字面量 | owned 非 Resource、一个无名称、模式标记、括号或后缀的 Int 字面量传给 `own Int`；验收核对真实实参 SSA Value。 |
| A2 · 单次 Shared 接收者的 Borrow 转发 | 零实参、Int 结果；转发原借用，不新建本地借用终止或所有者释放，也无后端 Protocol 2 收据。 |
| A2 · 同一 owner 两次 Shared 调用 | 同方法、两次零实参调用；借用身份各自独立，最后只释放一次 owner，无后端 Protocol 2 收据。末次 Complete2 不追认首条借用。 |
| A3 · owned nullable `when` | [SPEC-0455](../archive/specs/0455-refine-owned-nullable-when.md) 已交付；原 Node 12、Box 6、Rc 6 个函数实际走 Refine。后续 nullable place 需单独的 Borrow/Inout/字段/元素合同。 |
| A4 · 原 inline `Int?` + `replace` | 原固定源码的普通入口走 Refine，LLVM 通过，native 输出 `ok`。`UnsupportedNode` 是强制旧路径的测试结果，不能据此推出所有可空值与 `replace` 组合均已支持。 |
| 构造器内混合 return/break | 有既有独立触发条件的用例走 Refine；无该条件的用例仍是 Compatibility 对照。只在继续分支构造并释放，不扩大生产路由。 |

[#454 PR-event CI](https://github.com/Halckon/Koven/actions/runs/44981089506)
为 `8207b212` / `03699c3a` 的组合提供证据；在
[macOS](https://github.com/Halckon/Koven/actions/runs/44981089506/job/137081749471) 和
[Linux](https://github.com/Halckon/Koven/actions/runs/44981089506/job/137081749515)
的 Targeted 日志均核对到四个 A2 正例、近邻路由负控、构造器正控和 receiver 索引规模回归实际通过。
合并后的 [main push CI](https://github.com/Halckon/Koven/actions/runs/45038013512)
也已在 `d6976cf0` 完成；[macOS](https://github.com/Halckon/Koven/actions/runs/45038013512/job/137256223593)
和 [Linux](https://github.com/Halckon/Koven/actions/runs/45038013512/job/137256223571)
的 Targeted 日志均确认上述四个 A2 正例、近邻路由负控及 receiver 索引规模回归实际通过。
这些证据只覆盖对应的固定形态，不代表全部接收者形态都完成迁移。

上述主干快照尚有**实际兼容路径**，包括三次 Shared 调用、不同被调方法（两者的窄形态见下方本地候选）、
分组接收者、带额外参数或重复调用的 Borrow 转发、
命名／分组／计算实参以及借用形参等固定近邻；Box 和 Resource 接收者有单独有界负控。
这些是后续迁移和取舍的输入，不是任意合法形态的穷尽清单，也不是扩大准入的授权。

### 已合候选的验收项与后续退役条件

- 已合候选同时包含 D10 迁移及 E2 的普通 Unit ABI 注册；未引入 E2 DirectRange 试验，
  不将 E2 试验规划算作 E3 已交付能力。
- 四个原始正例和兼容负控的精确测试名如下。
  公共前缀为 `ssa::loan_region_ssa_tests::receipts::`，下列测试已包含自身的源身份、SSA、LLVM
  和相应 native 行为，不另加重复的 native 过滤器：

  - `receiver_boolean::boolean_receiver_refines_and_preserves_identity_and_owner_release`
  - `receiver_argument::explicit_scalar_receiver_refine_preserves_callee_argument_and_owner_release`
  - `receiver_borrowed::borrowed_receiver_refines_and_forwards_entry_loan_without_owning_release`
  - `receiver_twice::two_shared_receiver_calls_release_one_owner_after_both_loans_end`
  - `receiver_route::neighboring_receiver_shapes_keep_existing_routes`（负控）

- 只有实际生产兼容调用消失，才退役旧函数体路径及迁移专用门槛；保留双方共用的类型、闭包与
  常量入口。保留可定位当前剩余调用的见证，不能按目录名或 Spec 归档量判断完成。

## 早期固定见证复核（2026-11-21）

以下为 #454 合入前的历史记录，主干现状以上表为准。这里只重分类列出的固定源码，不把相邻语法、借用或 owner 形态一并标成完成。
A3、A4 两个测试模块在主干同步点 `8207b212` 与本地复核树 `570a3633` 中无差异。

| 见证 | 复核后的路径与证据 |
|---|---|
| A2 | 本地候选 `0620936c` 只将既定的 owned Boolean 零参数 caller 迁到 Refine。复核树 `570a3633` 为借用参数、同 owner 两次共享调用、一个显式标量实参三项保留原 Compatibility 路由，并补了两种 Unit 布局中的 SSA/LLVM 及 native owner 释放见证（§7）；其余形态不推论。 |
| A3 | [原始 owned nullable 源的路由与验证](../../crates/lang-codegen/src/ssa/refine_subset_tests/nullable_when.rs)：原普通入口验证 `Node` 12 个、`Box` 6 个、`Rc` 6 个函数实际走 Refine，完整 SSA/LLVM 通过；另一项测试复核三份源的两种 Unit 布局。不覆盖借用位置或其他更宽的组合。 |
| A4 | [原 inline `Int?` + `replace`](../../crates/lang-codegen/src/ssa/refine_tests/map_condition_tests.rs)：普通入口下 `replace_then_select` 实际走 Refine，LLVM 通过，native 输出 `ok`。定点 `UnsupportedNode` 仍保留，仅在测试强制 per-family 路由时被观测；不再将此固定源码的普通路径算作未实现。 |

在 macOS 上使用复核树 `570a3633` 的 `lang-codegen --lib`、feature
`loan-observation-test-support` 精确复跑 A3 两项和 A4 一项，均通过。
这是定向本地复核，不代表生产路径已全部退役或远端 CI 已验收。

另有一项独立可行性探针：仅在显式标量实参见证的 caller 里加入空的
`Int?` 局部绑定作为既有 Refine 路由条件。两种 Unit 布局实际由通用 Refine
实现完成原接收者、实参与每个释放动作，无 Protocol 2 receipt。
在探针基线 `461256d4` 上，原始无附加条件的源码仍走 Compatibility。这证明基本
lowering 能力，不是该形态直接准入或实际 receipt 所需证明的替代。

### A2 单个显式 Int 字面量本地迁移

候选提交为 `8d639f4b`。

以 `461256d4` 为基线保留原一条语句的 owned 非 resource caller，仅给
无名称、无模式标记、无括号的单个无后缀 `Int` 字面量补 Refine 准入；调用的
唯一直达参数须为 `own Int`，返回类型仍是 `Int`。基线对同一源码接受但不进入
Refine，作为红测。新路径不改变借用开始/结束或 owner drop 的产生，不增加
Protocol 2 的三个动作和五个 SSA 生命周期指令。

私有校验记录跟踪原字面量范围、值、已降低的 Slot 和实际 SSA 定义/Value，验证
该 Value 紧邻调用且是真实实参。双布局的观察 ON 核对原 Complete2 和 SSA
生命周期端点，真实 OFF 核对同一路由与等价 SSA/LLVM，native 核对同一 owner
唯一释放。独立 anchor 正控仍无 receipt；命名、借用形参、带括号及计算表达式
负控不继承此直达准入。本次标量迁移未改变借用接收者和多调用路由。

### A2 借用转发与双调用的独立 Refine 对照

本地测试提交 `62263079`、`a798dd97` 保留原始源码作 Compatibility 对照；
各只增加 `val anchor: Int? = null` 便经普通入口选中 Refine，不改变原样准入。
借用参数 caller 只用导入的 loan 作方法接收者，不创建或退休本地 loan，也不 Drop；
外层才建立、退休借用并释放原 owner。两路径均无后端 receiver receipt。
双调用依序使用同一 owner 的两条不同 loan，中间无 Drop，第二条退休后才唯一 Drop。
原样前端按调用范围分别为 Partial、Complete2，末次局部证书不覆盖第一次；
带声明的对照不符合「仅调用表达式」观察门槛，两条为 Unsupported，均无后端 receipt。
两种 Unit 布局的实际 SSA、LLVM 和按分配身份唯一释放均在 Mac 验证。
本地候选 `63973d9d` 迁移原样借用转发；`38828620` 迁移原样同 owner、同一直达方法的两次零实参 Shared Int 调用。两者均无后端 receipt，真实观察 ON/OFF 路由及 SSA/LLVM 等价。
双调用候选实际 ROp 和 SSA 分别核验两条独立 loan、只在第二条结束后释放原 owner，保留双布局 native 唯一释放与反例。Mac 和 Linux 定向各 70 项通过；末次 Complete2 仍不覆盖首条借用，不主张远端验收或整体退役。

## 本地工作树补账（2026-11-22）

对照 `8207b212` 与上述累计候选，按祖先和补丁等价性检查本地分支，
并读取 331 份可访问的注册工作树状态，跳过 93 份已失效或缺失的注册记录。
未发现漏纳的、已验证且属于本轮范围的 E3 生产改动；只恢复两项增量测试：

- `ae27ddad`：恢复原暂存的结构性循环提前返回构造器。与原 Mac 定向验收树 `942b019d`
  的测试文件完全一致；原证据覆盖两种 Unit 布局下实际 Refine、LLVM 与 native，
  不把该旧实跑归作组合候选的新运行。
- 定向运行树 `a43932e0`：原本地暂存的 Node nullable `when` 所有者身份验证 Mac 1/1 通过。
  在两种 Unit 布局下，核对两次读取的真实控制流都来自同一 owner；非空分支唯一取出并交付
  给实际 consume 调用。SSA 和 LLVM 验证通过；这是既有 Refine 行为的新回归测试，不是重新迁移 A3。

未纳入旧的整对象退出、nullable 写回和 pending-argument 探针：它们是未验收的研究，
不算生产修复。两个旧 post445 快照共有的 30 个暂存文件中有 18 个与当前候选逐字相同，
其余含后继拆分或路由差异，不用旧快照覆盖新实现。原工作树、分支和研究均保留。

### 本地跨源清理索引（2026-11-23）

`6b11a4b1` 按实际源码和函数范围预选普通 Unit 的清理事实；不同文件的相同偏移不会串源。
Mac 定向验证了同偏移路由正反例、单源 8/16 规模计数和跨源成对 receiver 的 SSA、LLVM 与 native。
这是 #455 后的独立本地变更，尚未合入或通过远端 CI，不沿用上述 A2 主干证据。

### 本地双方法 Shared 接收者迁移（2026-11-23）

普通 Unit 中同一个 owned 非 Resource、非泛型 class 根上的两次连续 Shared 调用，
各自以原类型事实核对零实参、Int 返回与非委托的直接方法目标；两个目标可以不同。
两条借用分别开始和结束，第二条结束后唯一释放原 owner。不同方法不获得后端 Protocol 2 收据，
也不沿用原同方法的专用审计。分组接收者、三次调用、带参数及混合 Int/Boolean 保持原路由。
Mac 定向验证了无额外触发条件的异方法原源码、同方法、近邻负控与索引规模，
并在异方法的两种 Unit 布局下核对观察开关、实际 Refine 操作、SSA、LLVM 和 native 唯一释放。
此候选未推送，尚无 Linux 或远端 CI 证据。

### 本地三次 Shared 接收者调用（2026-11-23）

同一 owned 根上的直达 Shared 调用可在上述条件下连续三次，仍逐调用查核真实借用与终止事实，
最后一次返回后唯一释放根，不产生新的 Protocol 2 收据。四次调用、分组接收者、带参数和
混合返回类型保留原路由。Mac 用两个本地表达式 ID 相同、但来源不同的函数核对各自的三个
实际方法目标、三次借用起止和唯一释放，并复核反转输入的 SSA/LLVM 产物与 native 释放顺序。
本地候选尚未推送，无远端 CI 证据。

### 本地多次 Shared 调用的括号接收者（2026-11-23）

两到三次调用的既有边界内，`(node)` 与 `((node))` 可与裸 `node` 混用。
只为验证具名根而去除分组；仍按原始外层表达式核对类型、借用起点和实际调用，
不将分组误认成临时值或独立 owner。单次调用、四次以上、非具名根仍不因本片改变路由。
这不是新的 Protocol 2 收据，也未修改独立的单次调用门禁。Mac 定向验证了跨文件重复 ID、
实际 SSA 借用起止与唯一释放、反转输入的 SSA/LLVM 等价及 native 顺序；无远端 CI 证据。

### #449：默认路由迁移不等于兼容实现退役

Map 构造和大小读取、Rc 构造及顺序容器大小读取的独立用例默认走 Refine。
普通 Unit 的混合函数若不满足整个 Refine 子集，仍可在 Compatibility 发出这些操作；
不可仅按默认路由测试删除对应实现。定向测试用带可空对象替换的实际普通函数验证这四种
SSA 操作及 LLVM/native 输出，未强制测试路由。此项本地证据不将其他 Map 操作外推为全覆盖。

## 局部组合候选验收（2026-11-22）

本地组合实测 `de7bc251b38a27cd0b80bdec3887425e6ffa656c`，树
`167c2c23307721481f12f2c5d147c37a6f7ad79f`，以 `8207b212` 为共同基点。
纳入 D10 `958149cf`、E3 `3d9a7fdf` 以及单独通过定向验证的一文件 E2 函数 ABI
注册 `aa7cc7a8`。未纳入旧 E2 聚合分支、#451 变更或 DirectRange 实验。

Mac 默认配置一次构建，两条精确测试均通过：

- `ssa::refine_subset_tests::temporary_sequential_for::mixed_constructor_loop::independently_anchored_mixed_constructor_preserves_both_native_edges_on_refine`：
  两种 Unit 布局中的实际调用者 `f` 走 Refine，SSA、LLVM 和 native 均确认只在继续分支构造并释放。
- `ssa::loan_region_ssa_tests::receipts::receiver_route::neighboring_receiver_shapes_keep_existing_routes`：
  15 份固定近邻源码 × 两种 Unit 布局均成功生成并验证 SSA/LLVM；未触发新的 Refine 路由或收据。

这只验证了此次组合范围内的路由正反例，不替代最终同一候选上的四项 A2、
默认／观察开关、Mac/Linux 与发布 CI 的验收，也不退役 Compatibility。
