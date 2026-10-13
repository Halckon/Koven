# SPEC-0305: 多 Borrow 参数的唯一来源直接返回

> **性质**：变更合同 · **状态**：done · **读取时机**：实施或验收多参数普通借用返回时 · **唯一真源**：本 Spec

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P4-0305` |
| 所属 Phase | Phase 4 |
| 语言规范 | 现行 [Guide10](../../guide/10-ownership-borrowing-drop.md#普通借用结果与显式局部绑定)、[Guide05](../../guide/05-declarations-callables.md) |
| 批准依据 | 2026-10-13 用户要求先收尾当前里程碑；在既有 M2B 合同内继续补实现，不转向新集合 API |
| 前置 Spec | [SPEC-0288](../../archive/specs/0288-map-native-execution.md) |
| 前置 ADR | [ADR-0016](../../adr/accepted/0016-interprocedural-borrow-abi.md)、[ADR-0029](../../adr/accepted/0029-ordinary-borrow-result-continuation.md) |
| 关联 ADR | 无新 ABI 或长期架构决定 |
| 阻塞项 | 实现与首次物理PR CI已验收；最终归档head复用、merge与actual-main CI待独立验收 |
| 影响范围 | lang-codegen single/unit lowering、verifier、正常源码/native测试及实现快照 |
| 语言语义变更 | 否 |

## 1. Goal 与基线

全部形参 Borrow 的直接具名函数可从任意唯一参数交付普通 shared 结果，在现有表达式体、
直线 borrow val 范围内保持真实 storage、来源延续和正确清理。不是完整 M2B 完成。

从 actual main `f7a4d15cecfa71b2fb8a4e9d4648d56e53702c05`、tree
`c24fa330381b4de7e3b5113c41c6dd6d3bef2f9a` 建立 `feature/spec-0305`。
用户另行批准的 Guide 旧尾句与两份 README 事实纠错作为独立首提交，只有三路径7+/6-，
不放宽语言/实现边界。0304只改变文档库存流程，本片通过 inventory.md 登记。

## 2. 有界合同

- 来源必须来自 typed `Parameter(i)` 与实际 ownership origin；全部参数 Borrow，无receiver。
- 按源码顺序求值每个实参，再按 argument→parameter mapping 排列 ABI operands；命名顺序
  不能改变来源。返回payload类型可以与来源根类型不同，保留已支持字段、泛型与透明Group。
- 保留每实参新建loan的精确身份；CallReturn只结束非来源的本次loan及temporary清理。
  source/result及其新祖先继续到前端指定结束点，先结果/子loan、再来源、最后owner drop。
- 同owner的多个shared实参不按root合并；复用既有borrow loan时不得结束外部loan。
- SSA签名保存真实来源下标；verifier同时验证全部Loan operand活性、参数/结果类型、唯一
  source→result alias和父子依赖。LLVM复用既有storage-pointer ABI，不复制/retain payload。
- single与真正跨文件unit一致。unit loans_ending_at仅按call枚举全部loan，不代表来源已结束；
  必须与精确BorrowSourceLoan的call/argument身份合用，不能整体结束或整体延长。

## 3. 非目标

receiver/Inout来源、混合Value参数、block返回、caller CFG/loop运输、间接调用、条件借用、
多来源、一等可存储借用、新集合API、完整NLL或新storage能力。保持结构化拒绝及真实Span。
禁止本地故障注入/校准、sanitizer、malformed IR/allocator改写、故障驱动和聚合驱动。
验收使用正常源码、真实SSA/verifier、原版LLVM/object/link/run；不运行计数IR重写helper。

## 4. 实施

1. 普通source红测覆盖来源首/中/末、命名反排、同类型不同root、同owner重复借用、wrapper、
   String/Resource字段/Copyable/MoveOnly storage。失败必须发生于真实lowering。
2. 两入口签名预检与orchestrate运输真实i；调用消费mapping/source/end/drop事实。
   unit call helper保留每实参新建loan归属；borrow result只延续来源链。
3. verifier核全部operand，保持别名与返回链限制；源码正常产物断言不修改IR。
4. 同选择转绿后运行直接消费者与已支持单参数回归，更新Architecture和本账本。
5. 本地小提交、仓库外恢复包；外部发布另等授权。真实CI/归档/合并按实际证据闭环。

## 5. 验收账本

| 验收项 | 命令/选择 | 实际结果 |
|---|---|---|
| 精确main与独立纠错 | raw commit SHA、tree、双父核验；docs checker/diff | main精确恢复；三路径纠错无冲突；655 Markdown通过 |
| 正常源码红测 | `cargo test --locked --offline -p lang-codegen --lib multi_parameter_borrow -- --nocapture` | 初轮4个新测试失败于真实UnsupportedNode；已有2项旧边界通过 |
| 来源/参数/终止SSA证明 | 同一精确filter，正常Program断言及原版LLVM验证 | 16组×single/unit已通过；来源、全部operand活性、wrapper与结果终止有正常trace断言 |
| native真实输出与deinit | 同一filter中的两个native矩阵 | 16组×single/unit原版native通过，stdout精确、stderr空、exit0 |
| 原单参数与拒绝边界 | 最终34项精确普通source回归；包含同filter10项 | 全通过；wrapper -n边界先记录真实失败再修复，!!/短range/具体callable保留拒绝 |
| Rust与文档静态检查 | fmt、lang-codegen严格clippy、docs/尺寸policy/diff | fmt、严格clippy通过；docs657与尺寸guard/diff通过 |
| 独立审查及复验 | 代码/来源/清理审查；精确filter10项及5项普通共享消费者 | 无未解决阻断；15项实际通过，见下方输入身份 |
| 首次双宿主物理PR CI | PR83 run38347444657 attempt1 | success；精确输入与平台边界见§9 |
| 归档head复用、合并及main CI | 精确后续head/merge/main | 待实际远端验收，不由首次CI推定 |

基线准备在相同Rust生产树上实际运行：boundary 2项通过（证明尚未支持）、frontend相关
三个suite共22项通过、8组候选×single/unit在borrow marker拒绝、4个已有单参数native
对照成功；原始输入与日志在仓库外。它们不代表本片已实现或已通过新源码测试。

## 6. 剩余边界

只有本片明确的多Borrow参数形状可关闭；其余M2B入口逐项留后继，不以文档归档宣称完成。
没有现行Guide歧义；若实际实现需要新语义或ABI决定，暂停报告，不从语言经验补规则。

## 7. 本地实现检查点

首轮新增4个测试均在第一个真实source失败；既有boundary 2项证明旧拒绝。实现后发现
MoveOnly用例的普通unit observer读取字段属于既有未接通路径，将该observer改成已有
Packet固定marker观察；Resource字段借用返回仍独立真实验收。没有以此宣称普通unit字段参数
读取已支持。新增wrapper负号正向穿过旧门的红测后，以实参局部预检及block不变检查封闭。
单参数/字段/Group、unit普通call/CFG及稳定View metadata消费者等最终34项通过；10项新filter含16组两入口SSA
和16组两入口native，并保留错误来源/活跃来源移动及辅助CFG/range/callable边界。

独立审查建议增加caller已有borrow val两实参复用、既有结果重复借用与泛型String实际payload，
三组均已通过；SSA断言区分本次新建loan和需继续使用的外部辅助loan，native读真实payload。

源码、target、工具链及原始日志在仓库外保留；没有修改IR/allocator或执行任何故障负载。
fmt、Rust尺寸guard（actual-main为base）与diff通过；single orchestrate为998行，旧大文件
verify_ownership从1827降至1823行，没有新增尺寸例外。lang-codegen all-targets严格clippy通过；
本地检查点未运行完整frontend/workspace/codegen、macOS或远端CI，当时仍in-progress等待
授权发布与实际CI。下方§9追加首次物理CI事实，不改写该本地边界，也不宣称完整M2B。

## 8. 独立审查与冻结输入

最终实现测试提交为`d2a3bcf2e1e98df6ae0f8c76edf94ada06f1ee03`，tree为
`17fbd9662407836b9202e111eb258aa3be2f04d3`。独立审查核对具名mapping、真实Loan/source、
unit逐实参清理、同owner/既有loan复用与wrapper CFG，未留阻断项。该精确提交实际复跑
新filter10项及5项正常消费者：临时range call、命名range call后续实参return、原版LLVM
range drop算法native、single/unit Map泛型native，全部通过。无计数helper或故障负载。

后续本地末提交只更新本Spec验收账本，Rust/fixture/工具链输入与上述测试提交相同。
恢复包保留完整Git bundle、源码、补丁、提交原文、输入散列及作者/独立复验日志；不含target。
Cargo target已释放；未执行GitHub推送、PR或任何远端状态写入。

## 9. 首次物理PR CI验收与有界归档

[PR83](https://github.com/Halckon/Koven/pull/83)的首次物理
[run38347444657](https://github.com/Halckon/Koven/actions/runs/38347444657) attempt1为success，
`reuse=false`。run于2026-10-13 04:38:46Z创建/开始；API `updated_at`及最后job的
`completed_at`均为04:53:58Z，run本身没有`completed_at`字段。

- 真实远端实现head为`c8a5fc982da87a2ca95d7f6131ca043d4bd1cf64`，tree为
  `ac534e087f037de7293b87aa5b42ff25f7499cfa`，与本地冻结实现字节一致。
- base为`f7a4d15cecfa71b2fb8a4e9d4648d56e53702c05`；真实synthetic merge为
  `13eabfa6b169173a880c46294e71f5f59431c94f`，有序双父为base、实现head，tree同上。
- 物理PR共15个job成功；Tree-sitter因实际`editors=false`合法skipped，不算物理通过。
  实现head的30个check runs全部completed，无未决项；feature push合法skip不冒充PR执行。
- 双宿主34/34指定普通测试逐名匹配通过，含全部10个多参数借用测试及16组两入口SSA/native；
  完整codegen在Linux为1417 passed，在macOS为1416 passed及1项既有LLDB ignored。
  macOS的Linux-only sanitizer执行/上传按平台跳过，不计作macOS实跑通过。
  docs实跑657 Markdown、336项scripts测试通过；两宿主check/clippy、适用门禁与preview通过。
- 两宿主preview的producer/API artifact digest/consumer实际下载及独立安装链均核对；
  没有另行下载ZIP复核包内字节，不把preview或普通stdout称作完整内存安全证明。
- Code CI Evidence与CI Passed真实成功，固定base下独立重算的两种checkout代码指纹为
  `41ef8207814850527d5aa3f94f0f53395ef864411721caeef765906a10dda5a8`。
  首次独立收据SHA256为`63f3cf2b7912217c44e519a9cf52c7c594133bd1f996f53355a9d11e99ceadc9`；
  原始API、job日志及逐名测试/preview证据保留仓库外，不把大量运行产物写入当前树。

据此仅归档本Spec明确的多Borrow唯一来源直线实现片，更新库存为0 active、289 archive。
receiver/Inout/Value混合、复杂CFG/loop/间接调用、条件借用、完整NLL及其余M2B后继仍开放。
归档只改Markdown/SVG及库存声明，代码、测试、脚本、workflow和Guide保持实现head原字节。
最终归档head是否真实复用、当前docs门禁、merge及actual-main全物理CI仍待独立验证；
本地输入相同不代替远端事件与证据复核，不在此记录尚未发生的`reuse=true`或main成功。

## 10. 纯文档归档本地检查点

在真实远端实现head上恢复完整commit身份后，本次仅迁移Spec、更新索引/架构链接、库存及
DAG。657 Markdown门禁通过；五个精确普通Python文件共124项通过：inventory24、docs41、
DAG1、PR reuse26、CI结果合同32；两遍DAG四产物字节相同，完整diff whitespace通过。
本轮未调用Cargo，不运行任何故障/校准/aggregate驱动。库存精确为0 active、289 archive，
draft仍为2份；过滤后的代码输入与首次物理head保持相同。该本地对照不等于远端复用成功，
最终归档head的真实reuse/docs/merge/main验收仍pending。
