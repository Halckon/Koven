# D10/E2 剩余决策与复用清单

> **性质**：决策盘点 · **状态**：current · **读取时机**：审议本轮 ADR 草案或安排 D10/E2 后续时 · **唯一真源**：本页只记录 2026-11-10 核验范围；语义、决定和实施以所链接真源为准

## 范围与精确基线

用户要求“建议为D10/E2的剩余工作，先起草好完整需要的ADR”，并强调优先动态 owner/借用、
不要重复开发。本次仅起草，没有接受新决定、修改实现、推送或合并。

- 远端 main：`3fe1920a6735b71f5a330ea8a8549371615fce88`，包含 #408。
- [#390](https://github.com/Halckon/koven/pull/390) 已合；merge
  `054f0ee1` 是上述 main 的祖先。其已完成 owner/borrow 切片不可重新开发。
- 原主工作树仍在 `cc1e5b41`，另有未跟踪 `scripts/koven_cbm_eval.sh`；未修改。
- 本次独立分支 `feature/spec-0341-remaining-adrs-20261110` 从精确 main 新建，只编辑本文、
  三份 proposed ADR、ADR 索引与文档库存门禁。SPEC-0341 与 E2 继续 in-progress/active。
- 仓库内 `.agents/skills` 无本任务可读取的技能文件；遵守根与 docs 的 AGENTS 及 ADR-0001。

这是当前剩余架构空白的完整分类，不声称穷尽未来实现中新发现的语义冲突。若实现暴露
Guide 留白，应暂停相应部分并另行决策，不能由草案补写语言语义。

## 已有决定：复用，不新建重复 ADR

| 问题 | 现有决定 | 后续性质 |
|---|---|---|
| 单文件与 project 唯一 Unit 管线 | [0031](accepted/0031-single-file-compilation-unit-adapter.md)、[SPEC-0341 §1–6](../specs/active/0341-shared-lowering-core.md#1-goal) | 迁移消费者、保能力、删除旧 checker/lower_frontend；实现/测试 |
| refine 唯一 lowering、显式所有权与 SSA block parameters | [0032](accepted/0032-ownership-refinement-layer.md)、[0006](accepted/0006-typed-ssa-block-parameters.md) | 复用，不再设计另一套 IR/按家族 lowerer |
| Borrow/Inout ABI、唯一来源普通结果延续 | [0016](accepted/0016-interprocedural-borrow-abi.md)、[0029](accepted/0029-ordinary-borrow-result-continuation.md) | 支持已定义模式、身份/结束边补齐；不重新决定 last-use/NLL |
| closure thunk、Borrow callable view、三层逃逸检查 | [0034](accepted/0034-refine-closures-and-callable-views.md) | 复用已有边界；Value 存储/跨 layout/线程仍是另行范围 |
| 递归 closure native 环境句柄 | [0025](proposed/0025-recursive-closure-environment-handles.md)，仍 proposed | 复用既有提案，不能默认接受或重编一个 ABI ADR |
| Exclusive capture 持久根槽 | [0036](proposed/0036-exclusive-closure-capture-storage.md)，仍 proposed | 复用；与 SPEC-0392/E3 协同，不把实现范围并吞到 E2 |
| 一次 root Replace 后立即 Break 的独立出口 | 本地 accepted 0037，接受 `03851b2a6fe8695059e2cfa88880759ed019c6ff`；末头见下表 | 未进 main；只复用其限定范围，不扩成共同出口/phi/suffix |

0037 编号保留，不复制其文件到本分支，不改 accepted 库存冒充已合入。新草案从 0038 起。
未来整合 0037 时合并其真实提交和库存，不另建同号文件或重写接受记录。

## 新起草的三项长期决策

| 草案 | 需决定的内容 | 明确不包含 |
|---|---|---|
| [0038 路径化 owner 运输](proposed/0038-path-qualified-owner-transport.md) | 跨 catalog/控制边的开放输入输出、Choice/phi、循环回边与共同消费证明 | 不决定新 drop 语义、不把 0037 两出口视作共同结果、不擅自选尾句限制 |
| [0039 动态 capture 实例](proposed/0039-dynamic-capture-instance-provenance.md) | 形成时冻结实例/槽来源，Owned/Shared 义务、retained source 与实际调用交接 | 不重新选择 0025 native ABI、不接受 0036、不启用任意 opaque/递归能力 |
| [0040 完整 loan 生命周期](proposed/0040-complete-loan-lifecycle-evidence.md) | 真实动作统一顺序、parent/result/forward 关系、覆盖认证与真正 absence | 不启用条件借用、完整 NLL、一等引用或 runtime 借用计数 |

0039 还登记了必须先核清的层间顺序：accepted 0034 的 SSA closure-drop-before-loan-end，
与当前 Unit 观察“逆序 End 组连续且紧邻环境 Drop”的动作映射；不擅自认定等义或改序。

三者可分开审议：0038 的无借用 String 首片与 0040 的普通调用首片可独立推进；0039 的
动态 capture 需要二者的身份/边/清理合同接合。每份均有候选方案、替代方案、验收和待决项。

**首个明确选择**：0038 列出“只保留两出口 / 唯一实际尾句消费 / 通用后缀”三个选项。
推荐第二项作为首片，但当前未选择。唯一尾句是观察能力的有界范围，不是 Koven 语言规则，
更不能用现有 planner 从 P 取值的一次执行伪证 Q 分支已消费。

## 在途成果与防重复账本

以下为本机读取到的精确提交，不代表已推送、已整合或本次重新验收。除明确写已合者外，
均不在上述 main 的祖先链；以 `git show <完整 SHA>` 读取，无未验证的远端链接。

| 现有分支/成果 | 精确 HEAD | 处置 |
|---|---|---|
| `feature/spec-0341-loop-replace-break` | `ef5ded79e4350873be019bc4c178106025af3dec` | 0037 与独立出口已本地完成；不重新开发 |
| `fix/spec-0341-rc-temporary-receiver` | `103ed130c66cbb24fdd080d6f1c017802ed6c6bf` | 前后端 temporary receiver 已有候选；复用其本地验收记录 |
| `feature/spec-0341-direct-parameter-reborrow` | `06d72a7b32fb7865c51fcfab83895d65646b5a1b` | 参数 Shared reborrow 结束已做；是既有 ABI 的实现片 |
| `feature/spec-0341-owned-call-temporary-current` | `fd6b6acfd823959d1577b44cf9fc3968f97ed086` | owned call temporary 候选，复用 |
| `feature/spec-0341-clone-result-loan` | `5aa191f412a2819eab3fed3dcdee0ba89bf57578` | cloned temporary 结束候选，复用 |
| `feature/spec-0341-concat-result-loan` | `bb2767ddc1d1f9b27a050718e1ead1530ffac093` | concat temporary 结束候选，复用 |
| `feature/spec-0341-rc-share-result-loan` | `2b3929e3aa660c8c851959fb4a81e6a53790a8dd` | Rc.share 返回值借用结束候选，复用 |
| `feature/spec-0341-exclusive-borrow-auxiliary` | `3ac51b741e511184a4f099d790567ca40db6ca62` | 已为 main 祖先，不能再次当未开发工作 |
| `feature/spec-0341-exclusive-parameter-prefix` | `3fe1920a6735b71f5a330ea8a8549371615fce88` | 名称存在但 HEAD 仅 main；不按分支名虚报实现 |
| `feature/spec-0341-invariant-join-suffix` | `216dc17484f611dbabce221a1936e8dffae5e49a` | §170：8 个已跟踪文件未提交，独立脏树，只读保全 |
| `feature/spec-0341-parent-binding-receiver` | `44500fbf52a676d0c018d819fb7589a17c4ac008` | 8 个已跟踪文件未提交，独立脏树，只读保全 |

`feature/spec-0341-inout-forward-source` 与 `feature/spec-0341-owned-call-temporary` 的 HEAD
仍为旧 main `cc1e5b41`，不把占位分支视作完成片。后者已有 current 后继，勿双开实现。
此表核了提交/祖先与脏状态，未重跑上述任何 Rust/native 测试，也不继承其 CI 作为新组合通过。

## 仅需实施/测试，不需要额外 ADR

- 已定义 loan/owner 模式下的参数、receiver、Group、临时值、clone/concat/Rc 等准入补片，
  若只接通真实事实且不引入新协议，归 SPEC-0341 或已有具体 Spec；不要一形状一 ADR。
- §170 的不变 join suffix 与 parent-binding receiver：先保全并完成原工作，不复制重写。
- 诊断 kind/message/Span/label/order 对照、raw 失败撤销、source/analysis 身份负控、
  旧测试两种 Unit 入口迁移、Single helper 删除与 dead_code 豁免撤除：按现有合同做。
- 保留完整原源码和独立 oracle；只有 Unit 提供同强度资源/顺序/absence 证据才退休旧见证。
  删除旧实现本身不能证明动态 owner/借用已完成，也不能靠两模式共同拒绝凑一致。
- E3 各家族退休、0025/0036 ABI 实施、Value callable 存储/跨 layout/线程入口与 E2 分账。
  若某项具体 Legacy 消费者证明 E2 退休确依赖其中能力，再补其依赖；不把全部 P2 功能列作前置。

## 推荐审议与实施顺序

1. 先核对并整合已完成 owner/borrow 候选，保全两棵脏树；具体发布/合并另按授权执行。
2. 审议 0038 的两出口后继选择和基础路径合同，同时审议 0040 的覆盖/顺序合同。
3. 接受后立精确首片 Spec：先 bounded owner 的实际共同消费、普通 loan 的完整顺序/覆盖；
   不因草案存在自行实施、不等待无关 ABI 决策才继续已被接受的实现片。
4. 再审议并分批落实 0039 的非递归/循环/嵌套实例来源；递归与 Exclusive native 各复用
   0025/0036 的审议和实施，不偷渡接受。
5. 按 SPEC-0341 §6 完成消费者库存、唯一生产责任、严格两入口差分、资源证据和精确头
   双宿主 CI/独立复审后，再判断是否删除剩余 Legacy 并关闭 E2。现在不关闭。

本次实际文档验收：`python3 scripts/check_docs.py` 通过（876 Markdown），`git diff --check`
通过。独立复审发现的 capture 顺序、callee 求值和内部槽交接表述已修正，并保留阶段映射
待核点。未运行 Rust、故障注入、native 或新官方 CI；文档通过不等于协议已实现或被接受。
