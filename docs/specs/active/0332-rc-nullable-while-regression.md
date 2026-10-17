# SPEC-0332: Rc nullable while 既有路径回归修复

> **性质**：实施合同 · **状态**：in-progress · **读取时机**：修复 nullable while 专用分派对 Rc 既有路径的影响时 · **唯一真源**：本片实施范围；语义以 Guide 为准

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P4-0332` |
| 基线 | 真实 main `864b6423849156c36fdad5e342cbf4bd09c3868d` / tree `031f2ddf2e0dce4a5797b3e005bb11942f94fa52` |
| 分支 | `fix/spec-0332-rc-nullable-while` |
| 前置 Spec | [SPEC-0330 while 流事实](../../archive/specs/0330-while-conservative-flow.md) |
| 合同 | [Guide09空安全](../../guide/09-nullability-errors.md)、[Guide10所有权](../../guide/10-ownership-borrowing-drop.md)、[nullable ABI](../../adr/accepted/0017-nullable-handle-ssa-abi.md) |

## 1. 问题与边界

用户报告普通 `own Rc<Int>?` 参数在 `while (owner != null) { break }` 后返回 owner，
可能因0330的专用 nullable while 分派在 single/unit 误收 SharedOwner 而后拒绝。
首轮仅静态定位：recognizer 的 owned MoveOnly 条件比专用 entry 的 HeapOwner 表示门更宽。
必须对0330前 `3a79fef25c3e2dc8827a410f96b65f01b3fbb5e2`、0330合并
`37e80526e54d88bc7146792d7b6e3d3fdd99a659`、本片基线分别取得真实双入口阶段结果，
才可将问题记为已证回归；不得从静态分支推断三个版本均达到某阶段。

本片恢复既有合法表示的普通 while 路由，不新增 Rc 特殊 proof、通用 nullable ABI、
借用规则或控制流框架。独立0331 Map 借用遍历分支已暂停，不将其未合生产带入本片。
0291保持 active，其未覆盖域不因本修复结清。

## 2. 最小实现合同

- 专用 recognizer 先完成既有真实 source/root/descriptor/型别身份验证，再只选择其已支持的表示。
- 已知合法但该专用实现不支持的 SharedOwner，应继续原普通 while 路由；不能在 entry 报错后兜底，
  不能将缺失类型映射、缺失事实或未知表示一律解释为普通路径。
- HeapOwner 专用 entry 的 proof 出生、当前 owner 身份、回边/退出清理、carrier 与 pending 拒绝保持。
- single/unit 分派规则对应，消费公开已验证前端事实，不从语法或名字假定 Rc 表示。

## 3. 验收

1. 一个普通 exact 完整收集示例的 single/unit 真实 FE、ownership、SSA/verifier、内存 LLVM 阶段；
   三历史输入分别冻结，不以 collector 成功代表两路径行为成功。
2. 修复后保留示例原成功目标；补 Rc nullable 的 normal、break、continue、return 代表清理与近邻。
   非空/空输入和正常终止的 native 必须先内容审与完整 SSA/LLVM 门通过，再绑定精确成功产物执行。
3. 独立查询真实 source/结果 owner 与正常退出清理身份；原 HeapOwner nullable while、Borrow carrier、
   pending loan 及旧 Rc 测试选精确邻控，避免仅无诊断掩盖共享路径回归。
4. 仅普通源码/查询和正常产物允许本地执行；私有事实损坏、计数、IR 注入及校准只交远端 CI。
5. 固定头的格式/有限编译、针对性普通回归、完整工程与双宿主 CI 分别留证；未执行项不得记绿。

## 4. 当前状态

本片实施与验收进行中，尚不代表完整验收或交付结论。额外探索输入的已观察阶段与
独立 presence-only 代表范围见下一节；后续固定头门禁、完整工程与双宿主 CI 分别留证。

## 5. 额外探索输入的实现边界

原用户 `while (owner != null) { break }; return owner` 的成功 strict 与独立身份查询
继续保持成功要求，不在此节改弱。后继四份 payload reader 探索输入首次真实运行未达到
八格成功目标：single 四格达到 verified LLVM；unit 四格均完成前端与所有权检查，但在
lower 返回 `InvalidSsa`、`span: None`。这些探索输入的原字节继续保留。

`bounded_payload_and_owner_return_sources_record_current_stage_boundary` 显式记录上述
精确 4+4 阶段表：先收齐八格，再只接受已观察到的成功阶段与特定 lower 错误，不接受
任意失败。它替代的是本片新增且从未通过的探索成功目标，不是既有用户成功回归。

当前 unit verifier 诊断表明，四例的 reader 均直接以 nullable wrapper 做 shared payload
投影；return 例另有 while 体内 `return owner` 对已有 nullable storage 再包装的问题。
修复前 break 整份输入也已观察到同一 lower 错误类别，但未取得旧内部 verifier 诊断；
不能据此宣称内部根因完全相同，也不能据 break 推断 return 第二项的历史来源。
这些是后继待修的实现欠账，不构成语言拒绝合同；相应路径修复时应更新此阶段表。

独立 presence-only 四源通过 Boolean null 比较观察结果；return 代表采用体内
`return null` 与正常源 owner cleanup，明确不结清上述体内原 owner 返回路径。
它们另有八格成功硬门，只有完整 SSA/LLVM 成功后才进入独立内容审的 native 验收。
