# SPEC-0227：跨文件常量 SSA 与 native 交付

> **性质**：实施 Spec · **状态**：in-progress · **读取时机**：接入 unit 常量 native 时 · **唯一真源**：本 Spec

| 字段 | 值 |
|---|---|
| 状态 | `in-progress` |
| Goal ID | `KOV-P4-227` |
| 所属 Phase | Phase 4 |
| 语言规范 | [v0.36 §36](../../guide/05-declarations-callables.md#36-无运行时存储的关联常量与封闭求值) |
| 批准依据 | 用户持续授权分阶段实施；2026-09-13 前置完成后迁入 active |
| 前置 Spec | SPEC-0198/0199/0208/0209/0210/0226 `done` |
| 前置 ADR | ADR-0007/0008/0010/0018/0020 `accepted` |
| 阻塞项 | 无；SPEC-0226 已交付完整 owned capability 与物化/drop/短路事实 |
| 影响范围 | `lang-codegen` unit SSA planning/lowering/verifier/native；必要 CLI 编排与测试；Architecture |
| 语言语义变更 | 否 |

## 1. Goal

完成后，专用 unit native 路径消费同一次 const-enabled typed 与 owned 产物，将运行时常量
use 降为标量值或独立 String literal temporary，生成并运行具有精确结果与清理行为的本机产物。

## 2. 背景与范围

- 复用单文件常量 lowering、IR-local Char、String literal 构造及既有 unit planning/native 管线。
  不重新解析/求值 initializer，不从 AST 恢复缺失的物化或 drop 事实。
- 新入口验证完整 inputs/names/environment/typed/owned 身份链及 executable gate；旧基础
  入口保持原能力边界，不通过可伪造转换接受新事实。
- scalar use 使用 typed ConstValue 精确 width/signedness/Unicode scalar；String use 按
  Phase 3 物化与控制流位置生成独立 owner，执行已验证 drop/transfer。
  同时消费调用前缀 pending temporary：后续实参提前 return/break/continue 时结束借用并清理，
  Abort 不清理，已提交 Value 不重复清理。命名 MoveOnly Value operand 也可能以实参 expression
  为 pending owner；不得只缓存 AST category 为 Temporary 的表达式而漏掉该事实。
  String 插值按 [String 最小操作边界](../../guide/13-program-runtime-standard-library.md#封闭的最小操作)
  确定性拒绝；Phase 3 对插值输入发布清理事实，不代表 Phase 4
  已获得 operand 转 String 的转换协议，也不授权本 Spec 实现该协议。
- 短路必须消费专用 owned 的 source-qualified 执行计划：左侧正常完成后 RHS 为
  Always/Never/Conditional；不得通过 AST 猜测缺失计划。分支编号沿用 0=true、1=false，
  AND RHS 为 0、OR RHS 为 1；消费 skip/RHS 的 BranchExit 清理，并保留 RHS 退出后的 skip 后继。
  基础入口既有单边 move 的 MissingFact 拒绝不因新入口接入而被隐式解除。
- 常量声明及初始化依赖无运行时存储、global/init guard、namespace capture 或退出析构。
  不能为跨文件读取引入 singleton 或稳定常量地址。
- 使用既有 process entry（含 argv）、object 原子发布与链接/执行流程；必要 CLI 编排仅负责
  选择正确阶段入口，不承载语义或重新推导事实。

## 3. 非目标

不实现通用 CTFE、关联函数调用、runtime globals、跨 compilation-unit ABI、新 LLVM type
体系或 v0.37；不修改已完成 0198/0199 的验收含义。

## 4. 验收标准

- [x] import 后选择及绝对路径覆盖顶层常量和五类关联 namespace；全部 11 种常量类型 native 输出正确。
- [ ] 跨文件 chain 与重复 use 保留精确值，短路/control 保持 typed/owned 执行位置。
- [ ] String 多次读取产生独立临时 owner；借用、转移、返回及 Abort 的清理与 literal 对照一致，
  不重复清理已转移 owner；使用既有 IR 或计数证据核实分配/释放，而非仅检查退出码。
- [ ] IR/verifier 不出现常量声明 storage/global/init 或 namespace capture；Char 保持既有 Char 契约。
- [ ] 失败 typed/owned、混合分析及缺失物化事实在写出前被拒绝；失败保留既有输出文件。
- [ ] 正逆 source inputs、重复构建产生确定性结果；argv entry 与普通 unit 非常量最近回归通过。
- [ ] 新旧入口 capability 编译契约、native 正反例和必要 CLI 编排验证通过，Architecture/验收同步。
- [x] 常量参与的 String 插值（含嵌套和提前退出输入）在 lowering 确定性拒绝并保留源码 Span。

## 5. 实施与提交

1. [ ] 显式消费新 owned capability 并接 unit planner → 验证：身份/缺事实拒绝与现有基础边界。
2. [ ] 接 scalar/String 物化与 cleanup lowering → 验证：IR/verifier 及 literal 对照。
3. [ ] 完成 native build/run、argv/确定性/原子输出矩阵 → 验证：上述逐项证据与归档。

每步形成可构建的独立切片，提交包含 `SPEC-0227`；不要把 SPEC-0226 的实现混入本 Spec 提交。

## 6. 验证记录

前置完成后先定位已有 unit lower/native test support；新增直接 constant suite，按影响选
最近消费者，不重复单文件已稳定验收。公开入口追加编译契约和
[分层验收](../../development/testing.md)要求的 workspace 编译检查；不运行 frontend 全量测试。
目标工具不可用时明确记录阻塞，不将 IR 或编译通过写成 native 运行通过。

| 验收项 | 结果 | 原因 |
|---|---|---|
| `cargo test -p lang-codegen --lib ssa::unit_plan_tests` | 47 passed，355 filtered，0 failed/ignored | planner 改造前基线；身份/可达性/单态化/确定性契约 |
| `cargo test -p lang-codegen --lib ssa::unit_constant_tests` | 9 passed，403 filtered，0 failed/ignored | 十标量、27例String、12例短路及AND/OR单边move、调用和二元前缀、5例插值重复拒绝；精确payload/SSA width与signedness/Char，input顺序，未使用initializer/函数排除及重分析/environment/path身份拒绝；各切片说明见下文，完整namespace矩阵待后续 |
| `cargo test -p lang-codegen --lib ssa::unit_constant_tests::string_uses` | 修复后 1 passed，403 filtered，0 failed/ignored；追加绝对路径矩阵由下行验证通过 | 9 场景 × import Name / 绝对 Member / literal，共 27 例；独立 owner、精确 bytes、逆序 drop、返回/Value 不重复清理、verified LLVM |
| `cargo test -p lang-codegen --lib ssa::unit_` | 193 passed，233 filtered，0 failed/ignored | Inout receiver 切片后 planner、unit lowering 与 unit LLVM 契约；含 String/短路/Borrow |
| `cargo test -p lang-codegen --lib native::unit_tests::unit_object` | 2 passed，410 filtered，0 failed/ignored | 基础跨 package 实际链接运行/原子替换与失败保留目标；不证明常量 native |
| `cargo check -p lang-codegen --lib` | `28759c5` 通过 | 生产库编译；没有跨 crate API 变化，不追加 workspace check |
| `cargo clippy -p lang-codegen --all-targets -- -D warnings`、fmt、docs/diff | 本次通过 | 当前 native 入口切片门禁；docs 353 Markdown，inventory 未变化 |
| `cargo test -p lang-codegen --lib native::unit_tests::constants` | 8 passed，418 filtered，0 failed/ignored | 首批 UTF-8 concat/println、argv 入口形状、正逆与重复 object、失败保留通过；六类 namespace、11 类型及动态 drop 计数通过；完整退出组合仍待补；复用 `native::unit_tests` 的 sibling temporary/原子输出夹具 |
| `cargo test -p lang-codegen --doc native::emit_native`、`cargo check --workspace --all-targets` | 4 passed，0 failed/ignored/filtered；workspace check 通过（27.37s） | 新旧 capability 双向隔离及跨 crate API 编译门禁 |
| `cargo test -p lang-cli --test project_cli --test native_cli`、`--bin kovenc project_build::tests`、build/clippy/fmt | 集成 5+8 passed，entry 1 passed（46 filtered），0 failed/ignored；build/clippy/fmt 通过 | 专用 capability 选择、实际常量 build/run、argv 内容传递；保留基础路径和诊断/输出前置规则 |

合同复核与门禁：独立边界审查通过，已显式补入顶层常量验收；docs check（353 Markdown）、
文档检查器 21 项测试与 diff check 通过。该记录属于 draft 建立时的合同检查。2026-09-13 前置完成，本次迁入 active；Rust/native 验收尚未执行。


### 当前接入边界

现有 `native::emit_native_unit_object` 经 `unit_plan::validate_unit_inputs`、
`unit_lower::lower_scalar_unit_with_entry` 再到 verified LLVM 与 sibling object 原子发布。
planner/lowerer 的内部 helpers 已改为消费只读 typed/owned facts；基础入口仍先校验完整身份链。
私有 `lower_unit_from_facts` 与内部 `plan_unit_instances_from_facts` 复用既有算法；
公开 `emit_native_constant_unit_object` 已接入专用 typed/owned，未增加到基础 capability 的转换。
短路、物化和 cleanup 只消费 SPEC-0226 产物；单文件
`ssa/lower_frontend/constant.rs` 可复用精确常量到 SSA 的转换逻辑，不能重新求值 AST。
专用入口按身份、entry shape、SSA 的顺序检查，最后原子发布 object。


内部 driver 切片：入口身份检查与实例上限不变，仅解包只读 facts 并机械移除一层 getter。
直接调用内部 helper 的 9 处测试参数初次编译报 E0308，已同步为 `.types()` / `.ownership()`；
这是夹具接口迁移，不是行为失败。独立复核通过机械归一对照，未发现算法漂移、错误顺序变化
或身份绕过。第一步专用入口尚未接通，实施清单保持未勾选。


私有标量切片：`unit_lower::constant::lower_constant_unit_with_entry` 仅在 crate 内可见，先核对
source index、typed 的 inputs/names/environment 及 owned 的 typed 身份，再进入共享 driver。
每个 use 按 source-qualified identity 找到 typed descriptor 与 owned materialization，并全字段
相等后才生成 Boolean/整数/Char constant。unit storage 补独立 Char 类型映射；不从声明
initializer 重新求值。普通函数和 closure lowerer 均携带专用 owned 引用。
`24a5cd7` 当时仍拒绝 String 物化和短路；`9341541` 接入 String 直接路径，本次开始消费短路计划。
不可达函数的短路不阻塞当前 entry。
新公开 native 入口仍未开放。初次测试因入口缺失 E0432；修复 unsigned 夹具的 `u` 后缀后，
明确复现 Char 缺少 unit storage mapping 的 UnsupportedNode，再补实现与精确类型断言。

本次独立复核再次确认：十标量类型断言、身份门禁与不可达函数隔离无新增阻断。
未运行常量 native build/run；基础 native 沿用 `28759c5`，没有修改公开 native 编排。


String 直接切片：复用已全字段核对的 typed/owned descriptor 生成普通 StringLiteral，
String Name/Member 二元操作数通过同一 materialization 路径，不查询声明 binding。
新增矩阵先复现 `= TEXT` UnsupportedNode；接通后复现 `consume((TEXT))` MissingFact，
将 Temporary delivery source 仅对已发布常量 use 穿透 Group 归一，call/argument identity 不变。
普通 literal/variable 或无常量计划仍使用原来源。独立复核未发现新阻断。
LLVM 辅助函数初选要求 Unit process entry，已改用不要求 native entry 的 verified program
检查；这不是生产缺陷。此切片不证明插值、pending 前缀控制退出或常量 native build/run。


短路切片：专用路径要求完整 source-qualified 执行计划，核对 left/right 和 RHS branch 编号；
LHS 始终先求值，退出直接传播。Never 返回 LHS，Always 执行 RHS；Conditional 复用既有
carried owner/loan CFG，分别消费 RHS 与 skip 的 BranchExit，RHS 退出不删除 skip 后继。
基础入口没有专用计划，继续保持原有单边 move 的 MissingFact 边界。
首例静态跳过测试先因 UnsupportedNode 失败；实施后 4 项公开私有入口测试通过，随后补
左右 Boolean 控制表达式中的 Nothing 退出用例（漏加 stop helper 与直接用 Nothing 作为 Boolean 运算 operand 的 L0085 均为夹具修正）。用户定义 Nothing 返回函数又触及既有 UnsupportedNode，因此最终使用 compiler-bound `error(TEXT)` 的 Boolean 控制表达式验证 Abort；不宣称用户 Nothing callable 已支持。最终 4 项专用 suite 与 181 项 unit 契约均通过。
独立复核确认 rebind 从 carried 快照重建 pending/temporary；外层 pending String 穿过
动态短路 return/Abort 仍需后续专门验收，例如 `view(TEXT, flag && if (flag) { return } else { true })`，
不能以单边 move 通过替代该清理组合，也不将本切片描述为完整常量 native 交付。


Pending 同步调用切片：私有求值帧记录前缀起点、loop depth 与实际创建的 loan 槽位，
只在 return/break/continue 跨出帧时逆序 BorrowEnd；兄弟 CFG 从 carried 快照恢复，帧 metadata
不随单条终止边改写。先结束 loan，再截断退出帧 pending 槽，随后消费已有 UnitDropFact。
复用借用参数的 loan 不由 callee 结束，Abort 不清理。专用 Value operand（含命名 MoveOnly）
在参数求值期间重新登记 temporary owner；只有所有参数完成后才移除该 Value 前缀。

先复现外层 Borrow prefix + 短路 return 的 UnsupportedNode。实现后 Borrow/Value × AND/OR ×
return/Abort 八例和借用参数、命名 Value、break、continue 四例通过 SSA/LLVM；最终补精确
BorrowEnd/drop 数量及借用输入不结束断言。Abort 夹具原以局部声明结尾产生 Unit，修为
直接 `error(TEXT)` 表达式；类型诊断不算生产故障。
独立复核要求保留 receiver/function-value 控制退出 guard（其前置 loan 尚不在参数帧中），
已保留；又确认截断先结束所有选中 loan、保持兄弟状态与外层帧索引，无新增阻断。
外层 pending temporary 内求值循环仍受既有 loop guard 限制，不能由 loop-depth 筛选逻辑
推断已支持。该切片未覆盖插值、String 二元前缀退出和上述额外组合；未运行常量 native。

String 二元前缀切片：先复现常量左操作数跨右侧普通 if 后的 `InvalidSsa`；此前运算仍使用
分支前的 ValueId。现在将左 view 放入 pending 槽，并在右侧完成后读取重绑定 entity，再
移除自身槽位。操作数的 Diverged 向上传播，正常路径继续消费 `AfterBinaryOperands`。
27 例覆盖常量/literal/命名 String × concat/equal/not-equal × 普通 if/return/Abort，
断言精确 drop 数量；另外 5 例覆盖嵌套二元、左侧 return 分支、外层 Borrow 调用、break
与 continue，均通过 SSA/LLVM verifier。新增测试没有逐 owner 断言清理顺序。
独立复核检查了重绑定、退出传播、嵌套槽截断及基础入口兼容，未发现阻断。
本切片没有公开跨 crate API 变化；未运行 workspace check、frontend 全量或常量 native。
receiver/function-value 前缀退出及外层 temporary 内循环仍待后续实现/验收。

插值边界校正：此前范围把 Phase 3 的插值输入清理事实误写为 Phase 4 实现要求，与现行
guide 的确定性拒绝要求冲突。以 guide 为准修正此处合同；不启用新的转换/格式化语义。
现有 `decode_plain` 拒绝插值，生产代码无需修改；专用常量 lowering 增加拒绝契约测试，
覆盖 String/Boolean 常量、嵌套、return 和 Abort 输入，重复分析检查错误种类及完整插值 Span。
9 项常量 lowering 测试、clippy、fmt 与 docs/diff 检查通过；独立复核确认合同遵循 guide，
未发现阻断。Span 断言覆盖源码范围文本，未另断言 SourceId；公开 native 输出原子性仍待验收。


公开 native 入口切片：新 API 接受专用 typed/owned，按共享身份门禁、entry shape、SSA lowering、
LLVM/target 验证、sibling temporary 原子发布的顺序执行；没有到基础 capability 的转换。
独立复核指出初版先 lower 会让泛型 entry 的 UnsupportedSource 覆盖 InvalidEntry，新增断言
复现后修正顺序；泛型夹具初次使用错误的参数位置导致 parser 诊断，修为 `fun <T> generic`。
首批 native 用例实际链接运行 UTF-8 concat/println 与 argv entry shape，逐字节比较正逆输入和
重复 object，并检查泛型/非 Unit entry、插值及 foreign typed 失败后保留既有 object、无临时泄漏。
argv 夹具未读取参数内容，不代表 argv 内容传递矩阵已经覆盖。六类 namespace、11 类型、
动态 owner/drop 计数、完整退出组合与 CLI 接入仍待后续；本 Spec 保持 in-progress。

本次 native 精确错误分类及 9 项常量 SSA 回归通过；独立复核确认顺序修正后无新增阻断。
4 项公开 API compile-fail、workspace all-target check、codegen clippy、fmt、docs/diff 均通过；未运行 frontend 全量测试。


跨文件 native 矩阵切片：覆盖顶层、object、class/value class/interface/enum companion，
每类均检查 import 与绝对路径读取的全部 11 种常量类型，以逐项 marker 验证实际比较结果。
另检查跨文件 Int 常量依赖链、Char 不等及含 NUL 的 String 输出。Char 精确 Unicode scalar
由已有 SSA payload 验收补足，native 的 expected 也通过常量物化，不称为独立码点 oracle。
先复现普通 runtime Char literal 的既有 UnsupportedNode；fixture 改为独立常量对照后，
再次复现 unit Char equality 的 UnsupportedNode。生产仅把 Char 纳入 equality/not-equal，
复用已有 SSA/LLVM Compare，未扩展字符算术、排序或普通字符字面量。
独立复核未发现阻断；完整 namespace/类型矩阵已补齐，动态 owner/drop 计数、剩余退出组合
及 CLI 编排仍待后续。没有公开 API 变化，本次不重复 workspace 编译或 frontend 全量测试。


动态 String cleanup 切片：测试专用 LLVM 注入 malloc/free/drop 计数，再由 clang 链接运行。
常量与 literal 对照均有 9 个 literal owner、2 个 concat owner，共 11 次 drop；只有两个
concat buffer 分配，free 逐 live 指针核对，包含跨文件 Borrow、Value 转移及返回。
聚合 drop 计数结合 SSA verifier 使用，不独立宣称逐 literal identity 的唯一清理。
return 用例要求 pending concat 的 3 drops、1 alloc/free，callee 输出 marker 必须未出现；
Abort hook 要求仅 concat 输入已清理（2 drops、1 alloc、0 free），随后 _Exit，正常退出的
析构检查会拒绝 Abort 用例误走正常路径。此计数不独立证明 Abort message literal 求值。

完整对照先发现 grouped literal 借用完成后接 if 的 InvalidSsa：临时 SSA 诊断明确显示已
Drop 的 %v0 仍作为两条 edge 参数。原因是 Temporary drop 只移除单个 expression alias。
现在复用 transfer 的精确 origin 校验与同 ValueId alias 清除，再发出原 Drop；临时诊断
已移除。新增基础入口三层 Group 后接 CFG 的单 Drop/verified LLVM 回归。
独立复核确认修复来源匹配不放宽，并补强 return 夹具以排除执行空 callee 的假阳性。

最终 5 项常量 native、187 项 unit SSA/LLVM 契约通过；完整退出组合与 CLI 接入仍待后续。


CLI 接入切片：先消费统一 typed diagnostics，基础 validate 成功保持旧路径；否则由
validate_constants 发布专用能力，继续专用 ownership diagnostics/validate，再选择 entry
并调用专用 native。私有 entry helper 读取 raw types，但所有调用方都已完成对应 gate。
首例合法常量 build 曾退出 1 且无诊断，接线后成功构建并运行跨文件 String concat；
run 的 argv entry 实际通过常量 INDEX 读取 Unicode 参数，stdout 精确匹配。
非法 Byte 常量在新目标路径返回源码文件诊断且不生成产物；已有目标则先被 CLI 的
output-exists 规则拒绝并保留原字节。两项证据分开，不把输出前置拒绝称为常量诊断。
诊断断言锁定来源文件，未锁定具体诊断码。独立复核未发现接线或错误顺序阻断。
此切片未修改公开跨 crate API，不追加 workspace check；未运行 frontend 全量测试。


共享 receiver 前缀切片：先复现 receiver 参数 return 的 UnsupportedNode，再将专用 Borrow
receiver 放入包围普通参数的求值帧，只登记实际创建 loan 的 carried 槽位；按参数→receiver
顺序结束 loan，再消费 frontend drop。基础入口、Value/Inout receiver 及 function-value 的
退出 guard 保留；借用传入 loan 不误结束，Abort 不展开。
新增命名 owned/借用输入/临时 Host × return/Abort 六例精确 drop 数量与 LLVM 验证，另有
嵌套调用、break、continue 三例 verifier 检查。独立复核确认帧弹出与索引范围无新增阻断，
这些用例不声称逐 loan 顺序或循环运行时计数。补 native return 用例要求 callee 和后续代码
均不输出 marker，只输出提前退出前的 before。

Native 夹具初次使用 `p.Host()` 触及构造器限定名 L0080，改为 import Host 后构造；
这是夹具边界修正，不修改名称解析。当前 189 项 unit 契约通过，其余五项 native 沿用本轮结果。

修正后共享 receiver native return 用例通过；本次无公开 API/CLI 变化，不重复 workspace 或 CLI 门禁，未运行 frontend 全量。


显式 Value receiver 切片：先复现 owned Host 参数 return 的 UnsupportedNode。根据既有
frontend `register_value_argument`，以显式 receiver expression 登记调用提交前的 MoveOnly
owner；参数全部完成、receiver 重绑定后再通过精确 origin helper 撤销 temporary。
return/break/continue 消费已有 drop facts，Abort 不展开；Copyable 不额外建立 owner。
8 例覆盖命名/带括号临时 receiver × Borrow/Value 实参 × return/Abort 的精确 drop 数量；
另有显式 this、Copyable value class、break/continue 的 SSA/LLVM 验证。191 项 unit 契约通过。
新增 native true/false 分支检查提前 return 与正常提交的 callee/continuation marker。
独立复核确认 origin、Group、提交时机和 guard 无新增阻断；隐式 Value this、条件 StaticSelf、
Inout、function-value 及外层 temporary 跨循环仍待后续。本次无公开 API 变化。

最终 7 项常量 native 通过；没有 CLI 变化，不重复 CLI/workspace 门禁，未运行 frontend 全量测试。


Inout receiver 切片：先复现参数 return 的 UnsupportedNode，开放后发现正常写回路径的
HiddenLinearLiveIn/PlaceUnavailable。writeback 的 original 与 Place 现在同时进入 pending
槽并重绑定，保持原可写存储；CFG 扩展 carried access，LLVM 非 entry Place 使用 pointer phi。
不重建 RootPlace，不放宽 verifier，也不开放函数 entry Place ABI。
新增 heap/Copyable inline × return/Abort/break/continue × Borrow/Value 的 16 个实参
组合及独立 return 用例；MoveOnly inline 的 8 个组合继续确定性拒绝，因为 RootPlaceTake
尚只验证直接 RootPlace。native 两次运行检查 Copyable inline 在提前 break 后读到 1、
正常写回后读到 2。独立代码复核确认存储 identity、退出清理和零尺寸 RootPlace 的 pointer 表示，
不扩称零尺寸容器派生 place 已获验收。193 项 unit 契约、8 项常量 native 通过。
MoveOnly inline、隐式 Value this、条件 StaticSelf、function-value 和外层 temporary 跨循环
仍未完成；不重复 CLI/workspace 门禁，未运行 frontend 全量测试。
