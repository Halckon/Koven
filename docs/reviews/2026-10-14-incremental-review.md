# Koven 增量 Review：2026-10-13 晚 → 2026-10-14

> 区间：main `614a7a7f`（10-13 22:23，PR #89）→ `b43a475b`（10-14 18:08，PR #99）
> 上一版：[仓库 Review 与里程碑差距](2026-10-13-repo-milestone-review.md) · [Guide 设计审计](2026-10-13-guide-v0.43-design-audit.md) · [v1 范围](2026-10-13-v1-scope.md)
> 方法：读 diff 与 8 份新 Spec；分别从两个基线源码构建 `kovenc` release，用同一套 95 个 probe（原 85 个 + 新增 10 个针对本区间 Spec 的）对比两个版本。
> 未做：`cargo test`、CI、sanitizer、macOS。
> 后续：[10-15 增量审计](2026-10-15-incremental-review.md)。

## 0. 结论

1. **这一天交付了 8 个 Spec（0311–0318），全部是单文件模式下"控制流 × 类型"组合的补洞。** 用户可见的新能力只有一项：单文件模式下非泛型 value class 的只读方法。
2. **project 模式在 95 个 probe 上零变化；单文件模式新增通过 6 个**，全部落在 0311（Map 条件 continue）和 0316（value class 只读方法）上。
   原 71 个"按 Guide 合法"基础程序的单文件通过数从 25 升到 26，project 仍是 35。
3. **方向与上一版 review 的建议相反。** 新增生产代码约 2,140 行，其中 84%（约 1,790 行）进了单文件管线 `ssa/lower_frontend/`，unit 管线只加了 25 行，frontend 和 Guide 零改动。
   上一版建议的止血（前端门禁、`Result`/`Pair` 注入、abort 打印消息）和两项结构性工作（管线合并、通用 lowering）都还没开始。
4. **有一处向好的信号**：PR #95 冻结了"M2B 实用化收尾"清单，并把 List/Map 高层 API、Map 遍历、`forEach`/`map`/`filter`、文件 IO 明确移到"后续 v1 独立里程碑"，与 v1 范围文档的划分一致。

## 1. 本区间交付

| PR | Spec | 内容 | 范围限定（摘自 Spec） |
|---|---|---|---|
| #90 | 0311 | single Map owner 条件 `continue` 运输 | 单 Map、单条件 continue |
| #91 | 0312 | owned Sequential source 条件 `continue` 证明 | 单 owner、单 loop |
| #92 | 0313 | single Map owner 条件 `return` 清理 | Return 无 phi incoming |
| #93 | 0314 | owned Sequential source 条件 `return` 证明 | 三种顺序容器、本地声明或 own 参数 |
| #94 | 0315 | 范围绑定函数内的 Int `!= <= > >=` | 此前只开放了 `==` 和 `<`；不含 `&&`/`||`、非 Int、when/loop/value-if |
| #95 | — | M2B 实用化收尾清单（文档） | 5 项完成条件，目前均未勾选 |
| #96 | 0316 | single value class 普通 Borrow 只读方法 | 非泛型、返回 owned Int/Boolean；初版意外放行的 3 个真实源码被重新收紧为拒绝 |
| #97 | 0317 | 同根范围绑定内的单层 `&&`/`||` | 同一词法块、同 root、单层、只限 Boolean 局部/实参/if 条件 |
| #98 | 0318 | Map 条件 `return` operand 内的最后使用清理 | temporary `Array<Int>`、单 Map、返回 owned Int |
| #99 | — | 0317 历史云端验证范围更正（文档） | — |

规模：135 个提交；crates +13.1k/−0.3k 行，其中生产代码约 +2.1k、测试与 fixture 约 +11k（约 1:5）；docs +3.5k/−2.1k（大部分是重新生成的依赖图 SVG）。

## 2. 实测对比

两个版本在同一台机器、同一套 probe 上运行。只列出结果有变化的条目：

| probe | 内容 | 614a7a7f 单文件 / project | b43a475b 单文件 / project |
|---|---|---|---|
| p44 | value class 方法 `this.v * 100` | BE / ✅ | ✅ / ✅ |
| t01 | value class 方法读裸字段 | BE / ✅ | ✅ / ✅ |
| t02 | value class 方法返回 Boolean | BE / ✅ | ✅ / ✅ |
| t04 | value class 方法带参数 | BE / ✅ | ✅ / ✅ |
| t09 | value class 方法调用另一方法 | BE / ✅ | ✅ / ✅ |
| t05 | Map 在 for 内条件 continue | BE / ✅ | ✅ / ✅ |

没有变化、但能说明边界的新 probe：

| probe | 内容 | b43a475b 单文件 / project | 说明 |
|---|---|---|---|
| t03 | 泛型 value class 方法 | BE / BE | 两种模式都不行 |
| t08 | value class 方法返回 String | BE / ✅ | 0316 只开放 Int/Boolean 结果 |
| t10 | Map 在 for 内**嵌套** if 后 break | BE / ✅ | 0310 只做了单层条件 break，多一层嵌套就回到拒绝 |
| p44b | 普通 class 方法 | BE / ✅ | 0316 只覆盖 value class |
| t06、t07 | 循环内 return、普通 `&&`/`||` | ✅ / ✅ | 本区间之前就已可用 |

**所有新增通过都是"project 模式早已支持、单文件模式补齐"。** 也就是说，这一天的工作在功能面上是在追平第二条管线，而不是扩展语言能力。
t10 说明了按组合切片的代价：0310 和 0311 分别处理了"条件 break"和"条件 continue"，但"嵌套条件 break"仍需另一个 Spec。

原 85 个 probe 的汇总（口径与上一版相同）：

| 模式 | 614a7a7f | b43a475b |
|---|---|---|
| project | 35 / 71 | 35 / 71 |
| 单文件 | 25 / 71 | 26 / 71 |
| 两种模式都可 | 23 / 71 | 24 / 71 |
| 应被前端拒绝的 9 个程序中得到前端诊断 | 4 / 9 | 4 / 9 |

## 3. 代码结构

新增生产代码集中在：

| 文件 | 新增行 | 所属 |
|---|---|---|
| `lower_frontend/constant_presence/source.rs` | 426 | 单文件管线 |
| `lower_frontend/map_loop_owner/carrier.rs` | 350 | 单文件管线 |
| `lower_frontend/constant_presence/source_return.rs` | 330 | 单文件管线 |
| `lower_frontend/readonly_receiver.rs` | 292 | 单文件管线 |
| `ssa/borrow_result_support/short_circuit.rs` | 172 | 共享（范围短路） |
| `ssa/single_readonly_method_boundaries.rs` | 121 | 单文件管线的能力门 |
| `lower_frontend/declarations.rs` | 113 | 单文件管线 |
| `lower_frontend/map_loop_owner/return_operand.rs` | 75 | 单文件管线 |
| `unit_lower/borrow_result.rs` | 25 | unit 管线 |

- 模块命名延续了"类型 × 结构"的模式（`map_loop_owner/return_operand`、`constant_presence/source_return`），这正是上一版 §4.2 指出的问题。
- 0316 的实现方式值得注意：初版接线后，nullable、多层 Group、for 元素三个真实源码"意外 lower 成功"，团队把门禁收窄，让它们重新报 `UnsupportedNode`。
  这符合"每片有界"的纪律，但也说明通用路径其实能处理更多情况，是能力门在限制它。如果有通用 lowering 和整体回归矩阵，这类"意外成功"应当被验证后保留，而不是关掉。
- 没有触及 frontend，因此上一版列出的前端漏报（未知成员、对 nullable 用 `?`、for range 的 ICE）全部原样存在。

## 4. 上一版建议的跟进状态

| 上一版建议 | 状态 | 说明 |
|---|---|---|
| E1 前端硬门禁、ICE 结构化 | ❌ 未开始 | r01/r02/s04/p34 结果不变 |
| E2 单文件并入 compilation-unit 管线 | ❌ 反向 | 84% 新增生产代码进入单文件管线 |
| E3 通用所有权细化层 | ❌ 未开始 | 继续按组合切片（0311–0314、0318） |
| E4 `Result`/`Pair` 注入、`kovenc check` | ❌ 未开始 | s01/s02 仍 L0080 |
| L12 `error()` 打印消息 | ❌ 未开始 | p45 仍无输出 |
| v1 范围 §5 冻结项 3"单文件管线不加新能力" | ❌ 未遵循 | 0311–0314 的 Phase 字段均写明"single SSA/native"，0316 也只做 single |
| v1 范围 §5 冻结项 4"不为单个控制流 × 类型组合开 Spec" | ❌ 未遵循 | 本区间 8 个 Spec 中 7 个属于此类 |
| 把高层集合 API、Map 遍历、文件 IO 放进 v1 里程碑 | ✅ 方向一致 | PR #95 已写入 Specs 索引 |
| Guide 审计的 15 处冲突、10 项语义缺口 | — 无变化 | Guide 本区间零改动，审计结论全部仍适用 |

上一版的 11 项决策（v1 范围文档 §7，D1–D11）尚未有答复。其中 **D10（管线合并与通用 lowering 是否优先于新功能）** 是这一天工作方向的直接分歧点，建议优先拍板。

## 5. M2B 收尾清单的评价

PR #95 冻结的 5 项收尾条件中，前三项对应的 Spec（0316、0317、0318）已合入，但清单本身尚未勾选，后两项（最终联合验收、边界逐项有去向）未开始。

- 清单把"边界逐项有去向"作为完成条件，这是好的做法，能防止有界切片无限延伸。
- 但收尾范围本身仍是组合式的：0317 只做同根单层短路，0318 只做 temporary `Array<Int>` + 单 Map + owned Int。
  按清单的完成条件收尾后，M2B 在实际程序中的可用面仍然很窄（例如 q17 泛型类 `borrow T from this` 仍被拒）。
- 建议：完成清单后，明确宣布 M2B 冻结，不再开新的 M2B 组合切片，把精力转到 P0/P1（止血与管线合并）。

## 6. 本区间的其他观察

- PR #99 主动更正了 0317 的历史云端验证范围，证据纪律是严格的。
- CI 和交付链条稳定：每个 PR 都有实现 head、归档 head、main 三层 CI 记录。
- 节奏上，8 个 Spec 在约 18 小时内完成并全部双宿主验收，执行力很强；问题在于投向。
  同样的投入如果用于 E2，单文件与 project 的差距（95 个 probe 中仍有 13 个"project 可、单文件不可"，另有 2 个反过来）可以整体消除，而不是一项一项追平。

## 附：复现

新增 probe（t01–t10）与两个版本的结果已放入本地 `.local-artifacts/review-2026-10-13/`：
`matrix-614a7a7f.tsv`、`matrix-b43a475b.tsv`。用 `KOVENC=<kovenc 路径> ./matrix.sh` 重跑。
