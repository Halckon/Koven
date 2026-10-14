# SPEC-0316: single value-class 普通 Borrow 只读方法

> **性质**：有界变更合同 · **状态**：done · **读取时机**：追溯 single 普通 receiver 方法首片验收时 · **唯一真源**：本页合同与验收；语言语义以 Guide 为准

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P4-0316` |
| 所属 Phase | Phase 4 typed SSA 与 native |
| 语言规范 | [Guide v0.43](../../guide/README.md)、[Receiver](../../guide/08-class-family-members.md#instance-receiver-与静态分发) |
| 批准依据 | 2026-10-14 用户“按建议顺序继续实施”；具体合并仍需批准 |
| 前置 Spec | [SPEC-0309](0309-borrow-value-class-fields.md) |
| 基线 | 官方 fetch actual main `16b4464f4d012eb4813e264b480de4198520aeee`；独立 `feature/spec-0316` |
| 影响范围 | single 方法收集、实例规划、receiver Loan ABI、真实字段读取及普通源码验收 |
| 语言语义变更 | 否 |

## 1. 冻结首片

先以无 Map 普通源码证明 single 非泛型 value class 的显式普通 Borrow receiver 方法调用，
默认/显式 Borrow、`this.field`/裸字段读取返回 owned Int/Boolean。方法身份、receiver 来源、
参数映射与 loan 清理由 typed/ownership 同链事实决定，不根据方法名重选目标。
不得只删除 receiver 拒绝而缺失方法收集、entry loan、调用 receiver-first ABI 或 cleanup。

named owner、Borrow 参数、temporary receiver 按已有事实分别验收，不从一例推定全部支持。
同 receiver 隐式调用只在不扩大合同/CFG时列入后续已验证范围；最后独立核对0299组合。
泛型、Inout/Value receiver、borrow-result receiver、条件借用及一般 CFG 不在本片；
unit 既有成功不回退。MoveOnly 字段不复制，本片仅读取可复制 Int/Boolean 字段。

## 2. 验收账本

| 验收项 | 目标/过滤器 | 实际结果 |
|---|---|---|
| 独立无Map红测/同源unit正控 | codegen lib `ssa::single_readonly_method_tests::`；Int/Bool × literal/constant × single/unit | 实际8项：single4项精确call Span UnsupportedNode红测；unit basic/constant4项原版LLVM/native正控通过，0 ignored，生产零改 |
| source receiver/entry及字段Loan identity、结束顺序 | 同模块普通Program只读断言 | 52项定向全部通过：receiver-first/独立显式槽、field Symbol/slot、child loan、MoveOnly temporary同root结束后唯一drop |
| named/Borrow参数/temporary、参数顺序和owner复用 | 普通源码源矩阵 | single/unit、literal/真正替换常量、implicit this、同名不同owner/顶层、逆序named实参、未调用非目标method通过；52项中含同源0299组合 |
| 原诊断与unit边界、0299后继组合 | planner、deinit、0309字段、0299及unit Borrow | 最终窄门后共享70项通过；frontend ownership/receiver/call映射22项通过；CLI native3+project1项通过（新single2项覆盖7个真实build/run变体） |
| 静态门禁 | fmt、codegen+CLI strict clippy、docs/diff、尺寸base16b4464f | strict clippy两crate all-targets、fmt、docs669页、diff与尺寸通过；1099个Rust文件，旧欠账38个，不提高任何cap |
| 双宿主CI与交付 | Draft PR96、精确实现head物理CI及归档 | 实现head双宿主CI已通过，详见§4；归档head CI、具体合并授权及actual-main核对仍待完成 |

single literal/constant使用既有同一constant-aware分析入口；unit basic与validated constant
unit分别验证，不把一个入口成功冒充另一个。native仅静审的正常source→原版LLVM→执行；
不跑坏IR、注入、校准、sanitizer、聚合脚本或stub。原始日志、JSON、冻结hash留仓外。
本机Linux结果不替代macOS CI，失败/未运行/零命中不记通过。

首次三轮保留：single每轮4项均为预期exact红测；unit正控先因空package lookup、第二轮文本替换未命中、第三轮package/logical path不匹配失败。修正测试helper为显式package app与app/readonly-method.ko后，第四轮4红/4绿；非生产修复，不把先前oracle失败改称产品红测。四轮源码前后hash均核对一致。

## 3. 实现与冻结证据

方法收集从 typed callable 的 SymbolId 找回对应 AST，只有真实可达 Source call 入实例队列；
顶层/closure既有roots保持。方法显示名加入 callable身份仅防LLVM名字碰撞，不以名字重选路由。
普通receiver独立进入entry和DirectCall，借入entry loan不结束，新建loan使用既有pending运输/清理。
复用receiver只接受实际SSA entry BlockParameter；non-null事实、迭代借用、field receiver、
borrow-result alias不在本片。ValueClass this字段限typed Int/Boolean；Class deinit原有路径保持。

- local `40503e84` tests-only保留首8项exact红绿；`6566a0b0`只搬声明收集；
  `55898f9d`扩展真实源码矩阵；`a4c09265`接线；`a0ddf45f`锁Loan/边界；
  `464da0a8`收紧receiver来源；`1a60c864`补field identity、temporary drop和CLI。
- nullable直接/多层Group以及for元素3个真实source在初版接线后意外lower成功，保留失败红测；
  窄门后同源码精确call Span `UnsupportedNode`，actual main16b同3例也为同诊断。
  此证据说明范围越界已收紧，不声称复现内存错误或验证一般CFG。
- original main16b另4个边界测试（12个source）及新CLI2项实际运行：边界通过、CLI按预期
  exit2 UnsupportedSource/UnsupportedNode失败。String字段两例旧版在外层call拒绝，
  新版在实际this.text投影拒绝；不把不同Span写成完全相同诊断。
- order/MoveOnly常量对照最初仅附加未使用常量，已改为真实替换receiver构造输入并断言替换命中；
  首8项的Int/Boolean常量红绿本来即为真实常量输入，不受此测试补强影响。
- 最终生产输入：52项本片 + 70项共享 = 122项codegen，全部0 ignored；frontend22项与CLI4项
  另列，均不是全crate验收。仓外每轮日志与source SHA256前后核对，失败日志保留。

macOS/完整CI、故障注入/校准、sanitizer及聚合门禁未在cloud执行；本地验证不冒充这些结果。

## 4. 双宿主实现验收与归档边界

[PR96](https://github.com/Halckon/Koven/pull/96) 的精确实现 head
`3bb74ce2d35c4e36228270b6c64ec48225a13d65`、tree
`4cc7830264779570620504ef6eee10f21b0e489f`，base
`b1165b0f430b5e8c3bcd669d5129fce6b4a9e1a2`，由
[CI run 38528485581 / attempt 1](https://github.com/Halckon/Koven/actions/runs/38528485581)
完成物理验收；16项checks与同head/suite/job/attempt一致，15成功，Tree-sitter因
editors路径未变而skip，不计为执行通过。

- Ubuntu/macOS逐名核对本片52项、共享70项、CLI4项、frontend22项均实际ok；无缺项、
  零命中或将filtered计为通过。完整codegen为Linux1892 passed、macOS1891 passed，均0失败；
  macOS既有 `lldb_hits_a_koven_source_breakpoint_and_reports_the_frame` 因debugserver权限
  ignored，单独保留，不计通过。
- 双宿主preview producer→artifact API→独立consumer的ID/digest与归属一致；归档包、binary、
  manifest全部成员的SHA/size/mode及三方manifest一致。每宿主无源码checkout的独立安装
  实际执行12条命令，stdout/stderr/exit、cleanup和sentinel核对通过；预装Cargo/Rust存在，
  但consumer命令没有源码构建，不声称完全没有工具链。
- 独立只读验收receipt SHA256为
  `74d6b26f32b064b53b8c204ad16afde6e443ceb04c84505fa7f556b059376013`；
  归档前复核其中35份日志/制品/元数据hash一致。原始日志、JSON与制品不入库。

本次只把已验收的有界实现标为done并迁移，保留§2–3的本地失败、原诊断与非目标。
归档提交为上述真实远端head的单父docs-only后继；归档head CI、用户具体合并批准、
merge及actual-main CI仍独立待核，不把实现head成功冒充交付完成。
