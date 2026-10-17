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
当前仅静态定位：recognizer 的 owned MoveOnly 条件比专用 entry 的 HeapOwner 表示门更宽。
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

已建立真实 main 上的独立工作树与分支，编号0332未在该基线占用。尚无本片历史对照执行、
生产修改、行为绿或修复交付结论。后续按小片独审并及早分支备份，合并范围另按用户授权推进。
