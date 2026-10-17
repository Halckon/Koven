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

## 6. 三历史首证与最小修复

2026-10-17，以用户 `inspect(own owner: Rc<Int>?)` 原文分别选择真实 single `inspect`
和 unit `p.inspect`。0330前、0330合并、当前main三基线各自独立target、同一Rust1.96/LLVM21.1.8、
`CARGO_INCREMENTAL=0`；六格前端/complete/ownership及真实Nullable Rc身份锚均通过。
0330前双格SSA/LLVM成功；0330合并与当前main双格均在完整while拒绝为UnsupportedNode，
single45..76、unit55..86。三collector成功只代表采证完成，真实回归由逐格outcome确认。
原先共享target有限check保留历史，不用其fresh结果证明三个版本独立编译。

两端recognizer各在原身份验证后按真实NullableHandle内表示分类：SharedOwner回原普通while，
HeapOwner仍走原专用entry；缺映射/类型为MissingFact，其他不支持表示仍结构化拒绝。
未捕获entry错误兜底，未改proof、cleanup、ABI或LLVM。原用户strict双SSA/LLVM及五旧邻控
在5734c906头通过；独立查询再于014483c4证明真实参数owner经break/零轮两条正常路径
完整返回，所有实际入边与Return同源，不凭同型或仅最终verifier认身份。

## 7. 代表退出、旁支边界与执行证据

最初四payload reader探索输入在836eb46f为single4成功、unit4 lower InvalidSsa/None；
只读临时trace在一次原verify失败闭包打印真实错误，已独立精确撤回。
当前四unit reader确有nullable wrapper直接投影shared payload的问题，return另有体内
原owner重复nullable.wrap。0330前break与return整fixture的双入口采证均为single成功、unit
同阶段拒绝；未对旧内部错误做trace，不据整fixture结果断言两内部原因均始于哪个提交。
§5的精确阶段表保留源与欠账责任，并非语言合同或支持成功验收。

独立presence四源保留normal/continue赋null回边、break交付原owner及return null正常退出，
reader只判null。72d6c71a的8格完整SSA/LLVM成功；9a4fb336的2普通native exact各执行4产物，
真实Rc(41)/null输入均按既定三行golden、空stderr及exit0通过。程序运行0.25/0.19s，
helper无内置硬超时，不冒称超时、动态free计数或泄漏检测证据。
62a53507同头4exact复验保持原strict、原identity、presence8成功，以及明确的4+4边界表。

关键独核收据：原六回归`f9bb3425…e6f2`、原身份`2e14ee4d…b0e8`、
presence8`81057e8e…abc5`、native2`5badf91f…51c2`、阶段4`59c22375…cc9`。
完整逐输入绑定及原始日志仓外保留；success输出由已审helper硬断，不冒称单独子进程raw。

f50ae280 的独立 presence 身份查询八格均成功：以真实AST/来源符号、Rc类型、交付合同及
Named Drop target/value_origin 连接实际SSA owner、替换null、回边与Return链；normal/continue
结束当前owner后携新null回边，break返回原owner，return-null结束未交付源owner，reader与caller
交付亦逐实体核对。查询失败不跳过原verify/LLVM。收据`e298b736…3915`已独核；FE DropPoint
仅打印观察，未以数字ID硬锁时序，执行清理顺序证据来自独立SSA查询，不外推动态释放次数。

最终工程首轮在 f50ae280 的 fmt/check 通过，clippy 因本片测试 helper 的单处
collapsible_if 失败；该轮 build 与61回归未启动。原收据`97a3bf69…fe6e`保留。
独立 f7e214ff 仅将原同一 Function/name 判据合并为 let-chain，不增加 allow 或改弱身份要求；
后继 v2 工程与61清单另行冻结，不沿用首轮成功结论。

v2 在 f7e214ff / tree f5237c62 完成 fmt、workspace all-targets check、clippy -D warnings
及 workspace build，四项全0，check/clippy/build分别18.03/28.17/18.52秒。
收据`37050d73…1b92`已独核，2504个tracked输入、工具/runner/manifest前后稳定。
同头61 ordinary exact全部实际命中1项并exit0，收据`8c620e47…0dd2`；47个普通SSA/LLVM
exact含一个明确现状表，14个正常native exact含12个既有CLI控制和2个本片聚合exact，
后两项共8条真实产物路径。原strict/owner身份、presence八格及cleanup身份均保持，
现状表只证明4single成功+4unit精确旧阶段，不能写成所有输入LLVM成功。

固定远端监视71项：上述61普通、9个既有私有guard及1个原collection-only。
所有项有实际定义与无filter CI接线；collector成功只代表完整采证，不计能力成功oracle。
双宿主CI尚待执行，不将本地普通结果代替远端私有门；本片仍active，未归档或合并。
后继明确承担unit nullable Rc payload投影与while体内原owner返回重复包装欠账；
修复时应更新§5精确表，不能将该表当永久语言限制。0331继续独立暂停。
