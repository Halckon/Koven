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
| 阻塞项 | 无外部语义阻塞；剩余实现与验收缺口见末尾记录 |
| 影响范围 | `lang-codegen` unit SSA planning/lowering/verifier/native；必要 CLI 编排与测试；frontend receiver/callee/capture 的 pending drop 事实修复；Architecture |
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
- 前端保持 function-value callee 与 shared capture 来源存活到参数全部完成；已有 native
  named function-value 路径接入 callee 求值帧。
  参数退出前先清理 pending alias，再消费 frontend drop。lambda 的 Borrow String 参数
  复用既有 shared Loan ABI；非 Name callee、带捕获的 borrowed closure、其他 MoveOnly Borrow
  参数不由此隐式开放。
- 常量声明及初始化依赖无运行时存储、global/init guard、namespace capture 或退出析构。
  不能为跨文件读取引入 singleton 或稳定常量地址。
- 隐式具体 MoveOnly Value `this` 的调用提交前清理由 Phase 3 发布既有 `This(owner)`
  drop fact，沿用 pending owner 的逆序和循环边界；Phase 4 保留该 owner 到参数全部完成。
  条件 StaticSelf 同样发布 pending receiver 义务及其在普通 drop 序列中的位置；后端按具体
  Copyable/MoveOnly 实例化消费，不从 codegen 猜补缺失事实或清理顺序。
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
| `cargo test -p lang-codegen --lib ssa::unit_` | 203 passed，239 filtered，0 failed/ignored | 内层循环前缀切片后 planner、unit lowering 与 unit LLVM 契约；含 String/短路/Borrow |
| `cargo test -p lang-codegen --lib native::unit_tests::unit_object` | 2 passed，410 filtered，0 failed/ignored | 基础跨 package 实际链接运行/原子替换与失败保留目标；不证明常量 native |
| `cargo check -p lang-codegen --lib` | `28759c5` 通过 | 生产库编译；没有跨 crate API 变化，不追加 workspace check |
| `cargo clippy -p lang-frontend -p lang-codegen --all-targets -- -D warnings`、fmt、docs/diff | 严格 clippy 受既有 `items_after_test_module` / `filter_map_bool_then` 阻塞；fmt、docs/diff 通过 | `constant_value.rs` 的 HEAD 已有相同布局；docs 354 Markdown，inventory 未变化；例外检查见末尾 |
| `cargo test -p lang-codegen --lib native::unit_tests::constants` | 14 passed，428 filtered，0 failed/ignored | 首批 UTF-8 concat/println、argv 入口形状、正逆与重复 object、失败保留通过；六类 namespace、11 类型及动态 drop 计数通过；新增显式 Value receiver 循环退出八例计数，完整退出组合仍待补；复用 `native::unit_tests` 的 sibling temporary/原子输出夹具 |
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


显式 Value receiver 循环退出验收切片：新增常量/literal × Borrow/Value 实参 ×
break/continue 八例 native 动态计数。MoveOnly inline receiver 的 String 字段与 pending
实参分别持有一个 concat buffer，要求两次分配及逐 live 指针释放；四个 concat 输入、
两个结果和循环后成功 marker 共七次 String drop。循环次数区分 break 与 continue，
精确 stdout 同时排除错误提交 callee、执行调用后代码和误生成提前 return。
本切片不修改生产 lowering；不证明 Inout 写回或隐式 receiver 的支持。

初次 fixture 在调用 initializer 后直接接普通调用触发 L0013 trailing-token；将异常
marker 改为局部 val initializer，不把该失败记作生产清理缺陷。独立审查发现原空 stdout
oracle 会漏掉错误 return，已增加循环后成功 marker 并完成复核。

继续实施前确认的 Phase 3 缺口：`drop_planner` 的 `ImplicitThis` Value 分支仅清空
`state.this`，未登记 pending owner，后续参数退出无法发布该 receiver 的 drop。
当前 codegen guard 保持拒绝；不得仿照显式 receiver 登记 expression temporary 来猜补事实。
此缺口应作为后续 frontend 修复切片独立验收，不改写已归档 SPEC-0226 的历史记录。

本切片验证：`cargo test -p lang-codegen --lib native::unit_tests::constants` 为 9 passed、
418 filtered、0 failed/ignored；codegen all-targets clippy（`-D warnings`）、fmt、docs
（354 Markdown）及 diff 检查通过。未运行 frontend 全量及 CLI/workspace 门禁：本次仅新增 native
验收夹具，无生产代码或公开 API 变化。


隐式具体 Value `this` 修复切片：frontend 先复现 return 路径缺失 `This` ControlTransfer
事实；codegen 先复现同类参数退出的 UnsupportedNode。pending 队列改为保存既有 drop target，
隐式 receiver 不伪造 expression identity；参数退出先清理后建 String temporary，再清理 This。
专用 codegen 保留 current receiver 到参数完成，CFG 合并 receiver/pending alias，提交后才移交。
正常提交与 Abort 不增加 caller receiver drop；基础入口和条件 StaticSelf 边界保持。

前端测试初版误从顶层 declaration 索引找 member，随后又把同类方法的 receiver drop 混合统计；
最终通过 receiver fact 取 owner 并仅断言 ControlTransfer，明确复现 0/1 差异后再实施。
独立静态审查核对逆序清理、loan 结束、CFG alias 与提交时机，未发现阻断；按建议增加
嵌套两个正常分支合流的 MoveOnly/Copyable SSA 用例。本切片不声称支持隐式 receiver 的
全部循环退出，尤其不允许 MoveOnly this 经 continue 后重新消费。

`cargo test -p lang-frontend --test multifile_ownership_checking --test multifile_constant_ownership --no-fail-fast`
通过 64 + 20 项，0 failed/ignored/filtered，含 receiver 清理顺序与既有 pending operand 契约。

`cargo test -p lang-codegen --lib ssa::unit_ -- --quiet` 通过 195 项，235 filtered，
0 failed/ignored；nested Copyable 夹具首次使用空 value class 触发 L0077，改为带 Int 字段后通过。

`cargo test -p lang-codegen --lib native::unit_tests::constants -- --quiet` 通过 10 项，
420 filtered，0 failed/ignored。新增常量/literal × Borrow/Value × return/正常提交八例，
receiver 字段与实参分别持有 concat buffer，共两次分配并逐 live 指针释放；return 七次 String
drop（含 entry 成功 marker），正常提交八次（另含 relay continuation marker）。精确输出
排除错误提交、错误跳过调用及 continuation。native 夹具初次出现 L0013，改用局部 val 接住
输出调用后通过；该解析问题没有归为生产 ownership 缺陷。

无公开 API 或 CLI 编排变化，本切片未重复 workspace/CLI 门禁，未运行 frontend 全量测试。

严格双 crate all-targets clippy 在未修改的 `type_checking/constant_value.rs:134` 因
`items_after_test_module` 失败；已核对 HEAD 在测试模块后声明三个 accepts helper 的相同布局。
保留原文件，不以功能修复夹带无关重排。fmt、docs（354 Markdown）及 diff 检查通过。

追加 `-A clippy::items_after_test_module` 的诊断性检查仍因既有测试中的
`filter_map_bool_then` 失败；已核对 HEAD 的同一代码。本次不扩散 lint 豁免，也不宣称
frontend all-targets 严格 clippy 通过。

补充严格检查通过：`cargo clippy -p lang-frontend --lib -- -D warnings` 与
`cargo clippy -p lang-codegen --all-targets -- -D warnings`。前者只证明生产库 lint，不代替
受上述两个既有问题阻塞的 frontend 全目标门禁。Spec 继续保持 in-progress。


条件 StaticSelf 前缀切片：专用后端先复现参数 return 的 UnsupportedNode；前端新增顺序字段
后先按旧 planner 复现 `preceding_drops = 3`，而所需顺序为 newer、argument、receiver、older，
receiver 应位于两条普通 drop 之后。现在 PendingOwner 保存完整 OwnedThis，条件 receiver
在参数求值期间暂存；正常提交恢复模板义务，退出时发布带位置的 conditional fact。
专用后端延迟具体 MoveOnly receiver 的移交，按 ordinary fact 索引交错消费条件 drop，
Copyable 跳过，位置越界拒绝；不按已生成 Drop 指令数推导该位置。

前端初版夹具把 named owner 的 value_origin 误认为 initializer literal，已改为声明名，
随后确认上述 3/2 顺序差异。SSA 类型矩阵首次误用保留字 value 作字段名触发 L0019，
已改为 item；这些夹具错误不记为生产清理缺陷。独立审查核对 pending 模板恢复、
MoveOnly 提交、Copyable 跳过、CFG alias 及 ordinary/conditional 交错，未发现代码阻断；
指出两处旧 Architecture 描述，已同步修正。

`cargo test -p lang-frontend --test multifile_ownership_checking --test multifile_constant_ownership --no-fail-fast -- --quiet`
为 65 + 20 passed，0 failed/ignored/filtered。本切片新增公开只读事实字段，因此追加 workspace
all-targets 编译检查；不以未修改公开函数签名为由跳过消费者检查。

`cargo test -p lang-codegen --lib ssa::unit_ -- --quiet` 为 198 passed、236 filtered；
`cargo test -p lang-codegen --lib native::unit_tests::constants -- --quiet` 为 11 passed、
423 filtered，均 0 failed/ignored。SSA 包含显式/隐式 × heap MoveOnly/inline MoveOnly/Copyable
× return/Abort 十二例、精确四 owner 的 Drop origin 顺序、MoveOnly break、Copyable continue
及嵌套 this。native 新增显式/隐式 × Borrow/Value × inline MoveOnly/Copyable × return/正常
提交十六例，用 concat 分配/逐指针释放和精确 String drop 计数验证，stdout 同时区分调用和后续
代码是否执行。这些证据不等于所有无正常出口的 interface default、所有循环组合已验收。

`cargo check --workspace --all-targets` 通过（9m41s）；frontend `--lib` 和 codegen
`--all-targets` 的严格 clippy（`-D warnings`）通过。fmt、docs（354 Markdown）及 diff 检查
通过。frontend all-targets 严格 clippy 的两项既有 lint 仍未修改，本轮不重复已知失败；
未运行 frontend 全量测试及 CLI 集成测试（无 CLI 编排变化）。Spec 仍为 in-progress。


function-value 调用前缀切片：新增测试先复现 lambda Borrow String 参数的 UnsupportedNode，
接入既有 shared Loan ABI 后再复现专用入口的参数退出 guard。前端测试确认 callee 缺少
return 路径清理，以及 borrowed closure 的 shared 来源误在 BranchExit 析构。现在保留 callee
root 到 CallReturn，活 closure 的 shared 来源同样受保护；后端为 callee 增加外层求值帧，
参数退出先清理 pending alias，再消费 frontend drop，Abort 不展开。

前端定向回归为 66 + 20 passed，0 failed/ignored/filtered。SSA 首次广回归为 200 passed、
1 failed：旧拒绝矩阵仍把 Borrow String lambda 列为不支持，已换成仍未开放的 Borrow Host
参数，并保留带 capture borrowed closure、Inout、非 Name callee 等既有边界。此前测试夹具
还修正了 local symbol 查询与私有 ID 构造；这些夹具错误不计作生产缺陷。
带捕获的 borrowed closure native 路径仍未开放，本轮 shared 来源保护是前端事实修复。

SSA/LLVM 重跑 201 passed、237 filtered，0 failed/ignored。Borrow String 的参数规划为基础与
常量入口共用；参数控制退出仅放开常量专用入口。新增覆盖 move capture 与无 capture callee、
return/Abort、break/continue、兄弟分支及后续重复调用。
native 首轮 11 passed、1 failed：新增夹具把内联 closure 环境误算为堆分配，实际唯一分配为
concat。按 ADR-0009 修正分配预期为 1，保留 String drop 正常 5 / Abort 2、逐指针释放和 stdout
断言；未修改 runtime 或计数器实现。

native 重跑为 12 passed、426 filtered，0 failed/ignored；新增 Borrow/Value × return/正常提交/
Abort 六例均通过精确 stdout、String drop 与逐指针分配/释放断言。独立复审核对 callee 帧与
参数帧的退出顺序、CFG 重绑定、shared capture 来源保护、基础入口 guard 和 Borrow String
边界；修正旧拒绝夹具与内联环境分配预期后复审，无新的阻断问题。

本切片 frontend `--lib`、codegen `--all-targets` 严格 clippy（`-D warnings`）通过；
fmt、docs（354 Markdown）及 diff 门禁通过。无公开 API 或 CLI 编排变化，未重复 workspace
check、CLI 集成及 frontend 全量测试；frontend all-targets 的两项已知基线 lint 仍未修改。
Spec 保持 in-progress，外层 temporary 内循环及完整退出矩阵等剩余项继续按切片验收。


内层循环前缀切片：新增测试先复现外层 Borrow concat 前缀中的 `loop { break }` 被
UnsupportedNode 拒绝。LoopJump 改为保存完整出口快照，循环 header/body/false edge 与回边
传递 pending owner、loan、place；循环退出保留外层 temporary，内层 jump 不结束外层调用帧。
while 条件内有 CFG 时终结器写入实际 condition 出口，不固定写入原 header。

独立审查发现 Copyable named var 与已求值实参不可在循环中永久共用槽位；新增用例复现
赋值后的 MissingFact，循环入口现分离 Copyable 实参快照与变量当前值。首轮 unit 广回归
201 passed、1 failed：既有 String 二元前缀 break 清理 owner 后仍有 pending alias，跳转现在
保留入口槽位、截断循环内新增槽位，loan 仍先按调用帧结束，不跳过 owner 事实检查。

修复后 `cargo test -p lang-codegen --lib ssa::unit_ -- --quiet` 为 203 passed、239 filtered，
0 failed/ignored；新增 Borrow/Value × break/continue/natural while/条件 CFG 八例，以及
Copyable 前缀快照。独立复审确认 pending 快照分离、内层 jump 槽位截断、loan-end 与 owner
清理顺序、while 条件出口和完整 loop exit 状态，未发现新的阻断问题。

native 首轮 13 passed、1 failed，失败为 while 夹具的 L0013 解析诊断；将赋值置于输出声明前，
保持单次输出与原循环次数后，定向十例通过。完整
`cargo test -p lang-codegen --lib native::unit_tests::constants -- --quiet` 重跑 14 passed、
428 filtered，0 failed/ignored。新增 Borrow/Value × inner break/continue/natural while/return/Abort
十例，精确 stdout 区分循环次数和调用是否提交，String drop 与逐指针计数验证 owner 的存活和
释放；另一个 native 用例让错误覆盖 Copyable 首实参触发 Abort，验证快照保持原值。

`cargo clippy -p lang-codegen --all-targets -- -D warnings`、fmt、docs（354 Markdown）和 diff
检查通过。无公开 API、frontend 或 CLI 变更，未重复 workspace check、frontend 测试及 CLI
集成。本切片只证明上述普通内层循环路径；完整 receiver/function-value 循环组合与本 Spec
其他未勾选合同仍待逐项审计，状态保持 in-progress。
