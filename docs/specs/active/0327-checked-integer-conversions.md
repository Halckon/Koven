# SPEC-0327: 显式 checked 整数转换

> **性质**：实施合同 · **状态**：in-progress · **读取时机**：准备 v0.44 E 的普通前端红测时 · **唯一真源**：本 Spec；语言合同见 Guide

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P2-0327` |
| Phase | Phase 2 类型身份先行；Phase 3/4 按后继验收分片闭合 |
| 基线 | 实际 main `5a32550a0ea956c1d66fdb65ae9bf2aeb6681684`（正常合并继承A/B） |
| 分支 | 本地 `feature/spec-0327-integer-conversions`；授权远端 `fix/spec-0327-integer-conversions`，保留发布映射 |
| 规范 | [Guide整数转换](../../guide/04-expressions-operators.md#整数-range-与显式整数转换) |
| 当前状态 | 合A/B主干后13普通exact与4工程通过；仅FE阶段，后端转换未实现 |

## 1. 批准范围与依赖

实现已启用的八整数builtin完整8×8零参数toXxx转换，包括identity。
按数学值域检查后转换；越界沿既有Abort，不先截断，不按位宽假定signed/unsigned总成功。
不启用Float/Double、Char、Boolean、String、nullable特例、OrNull、const或新输出保证。
不改变普通用户同名成员的选择规则，不把名字本身当intrinsic身份。
A/B主干交付优先；本草案可中断，后续在真实main同步后冻结测试输入，不混入其发布分支。

## 2. 最小实现分片

- 复用既有IntegerOperationKind/descriptor链，集中定义转换名到目标builtin的映射。
  descriptor必须分别发布receiver源类型、目标结果类型、真实receiver/call身份；不沿用
  当前Invert的result==receiver硬编码。两入口保留各自TypeId，避免大范围类型体系重构。
- final admission沿已有integer operation角色处理，ownership只分析真实receiver一次。
  未完成或非法receiver继续原诊断/Deferred拒绝，不通过伪造事实取得资格。
- 后端/SSA具体转换表示与verifier合同先独审再定；lowering消费typed身份，不重新看成员名。
  内部zext/trunc等ABI适配不等于用户checked转换，也不顺手改容器/argv路径。
- Phase 4成功值域才执行符号扩展/零扩展/截断，失败复用现Abort；不承诺stderr/固定退出码、
  unwind或drop。此阶段计划不授权本地运行Abort/native。

## 3. 拟定 ordinary frontend 精确矩阵

拟新独立integration target `checked_integer_conversion`。所有helper仅普通源码→真实
lex/parse/names/raw types/complete API；不构造或修改facts/IR，不运行语言产物。
每case先收齐single/unit观察再断言，解析/名称前置失败与语言行为红分账。
精确函数与helper已落独立测试文件，冻结后须独审，以下不是已执行名单：

1. `all_integer_pairs_publish_target_types`：64个独立普通函数
   `fun convert(value: SRC): DST = value.toDst()`，源/目标来自八整数封闭表；
   各自真实类型表核call目标builtin、receiver源builtin，descriptor的receiver/call AST身份；
   complete成功。未来新增descriptor API尚不存在时，先用现公开query采真实缺事实，
   不写引用未实现API导致0执行的伪红；API断言在生产接口固定后补齐。
2. `identity_conversions_remain_explicit_intrinsics`：八同型转换与既有inv成对，目标同型但
   转换与Invert身份必须区分；禁止仅检查无诊断。
3. `conversion_call_shape_rejects_arguments_and_type_arguments`：非零参数/显式类型实参、
   未知toXxx/OrNull，保留普通调用匹配诊断及真实span；不预猜未经采证的错误码。
4. `non_integer_receivers_do_not_gain_conversion_members`：已定型Float/Double、Char、Boolean、
   String、Int?参数作为receiver；只分析，不触碰浮点native限制；complete不得放行。
5. `user_named_conversion_members_keep_nominal_dispatch`：普通class的toInt方法仍按用户调用
   选择，与整数intrinsic正控成对，不抢占同名方法或给任意结构体转换身份。
6. `checked_conversion_calls_remain_outside_const_evaluation`：const val调用拒绝与普通runtime
   函数调用接受成对；字面量值恰可表示也不得自动加入const白名单。

调用模板：`cargo test --locked --offline -p lang-frontend --test checked_integer_conversion NAME -- --exact --nocapture --test-threads=1`。
串行、固定绝对target、HEAD/tree/inputSHA前后核；每项实际1命中，不扩大target或全suite。
新helper/fixture和具体错误oracle必须在运行前固定并审查，现尚无运行许可名单。

## 4. 后续成功/失败验收边界

前端64对通过不表示native64对完成。后续另冻结普通成功native（合法交集0/1、
signed最小值扩展、UInt最大值→Long、ULong identity）与receiver求值一次/顺序证据。
越界负signed→unsigned、同宽unsigned上半域→signed、窄化边界必须先审有界子进程helper；
不以编译期报错替代runtime Abort，也不把故障注入/校准混入本地普通测试。
inv、普通同名成员、const与非法receiver均保留近邻回归。SSA/verifier/ownership的受影响
普通source精确回归另审；不为了跑测试扩展未批准功能。

## 5. 交付门

首批红证与接口方案通过后已active登记，前端候选仍须独审与真实绿测；本片结束前闭合所宣称各Phase，
未闭合后端时不以Phase2绿色标记整个转换功能done。工程检查、实际base尺寸门禁、
双hostCI与正常归档/合并分别记录。备份使用有明确base/head/顺序/SHA的format-patch增量；
merge拓扑单独登记，不以线性补丁冒充保真恢复。

首轮非法调用/receiver仅锁结构化拒绝并采完整诊断；既有成员缺失不能证明未来专属参数门已闭合。
const 对照先要求 runtime 成功，失败时不把成员不存在误计 const 白名单通过。

## 6. 首轮真实红证

固定 `72d5afed` / tree `52c55ae8` 串行六 exact 各实际1命中，13输入前后相同；
64对128入口观察收齐，当前转换均无descriptor，complete拒L0165或L0080。
四红为全配对、identity、用户同名配对中的整数intrinsic、const配对中的runtime正控；
用户普通toInt两入口已成功，inv观察已收齐。非法调用/receiver两绿仅证明当前结构化拒绝，
不能以成员尚不存在证明转换专属参数门完成；const断言尚未越过runtime红，不计白名单通过。
没有编译/解析/名称前置失败；未运行ownership/native/Abort，后续先冻结精确诊断与接口方案。
原始收据在仓外 `spec0327-evidence/red6-receipt.json`，不将日志或重复源码入库。

当前候选只发布CheckedConvert源/目标/AST身份并按kind验证；final admission复用既有角色。
两条lowering在事实验证后对合法CheckedConvert明确UnsupportedNode，不生成IntegerNot或cast；
畸形事实仍MissingFact。此边界需普通源码lower-only正控验收，未运行前不记通过。

首次active迁移将状态误写active，docs门禁拒绝；现修为治理规定in-progress，不计首次通过。

候选49cd首条因4处emit参数String/&str编译E0308而0执行，余7未跑；独立机械引用修复，原日志保留。

`5ba8426b`六FE exact各1通过、21输入同；lower测试编译E0308误传validated而非recovery只读视图，
第7命令0执行、第8未跑。现仅修helper公开types()访问，后端仍Unsupported，不计完整交付。

`28b11c30`同head六FE+两lower-only exact各1通过，21输入同；128配对入口观察已收齐，
identity/异型lower均在真实调用span返回UnsupportedNode，无emit/native；后端转换仍未交付。
负例据raw收紧完整code/span并核原诊断保全；参数与const恢复附加L0080如实保留，
不把此旧恢复现象修成任意gate豁免。下一步待A真实main同步后的共享inv与工程验收。

后继正常同步A实际main5a32550a，仅7处docs索引/四图冲突，生产自动合并；新head尚待统一复验。

## 7. 合A/B主干后的FE阶段备份点

`885fc487` / tree `a0c8aa67` 正常双父为原E `b496115b` 与真实main `5a32550a`；
A/B实现与归档均保留，仅7处当前索引/四图冲突，Rust自动合并无冲突。
同head13exact=6FE+2lower-only+5既有inv普通回归，各实际1命中、25输入前后同；
5inv仅raw/ownership回归，不冒称其经过complete。fmt、workspace/all-targets check、
frontend/codegen/cli三包all-targets clippy、cli build四工程均成功；实际base尺寸与docs亦通过。
仓外main-sync13与engineering-main-sync13收据绑定argv/logSHA；旧两次编译0执行均保留。
本阶段仅供增量备份，Spec保持in-progress；未实现转换SSA/LLVM或运行native/Abort，
尚未接入新增target的最终CI名单，不以13绿或build成功表示完整E交付。

## 8. 后端普通源码首红计划

先独立4exact：双入口64对lower/verify、具名receiver调用一次、活跃String/pending实参、
双入口64对动态LLVM生成/verify。只读取真实typed/owned/SSA，不改facts/IR、不运行产物。
首红先锁真实UnsupportedNode缺口；新私有operation尚无API，精确双result/Abort边、
receiver call结果身份、carry/rebind及i128原值界限谓词在接口冻结后补oracle，未补不验收。
后端拟复用checked双结果(value,failed)和原成功/Abort CFG；先以source数学值判界，
LLVM内部i128不进入语言/ABI，失败result不得交付。native成功及有界Abort另审名单。
新增FE target接CI时同步唯一/精确预算composition断言，实际base尺寸门禁不可省略。

`eb26265f`后端首4exact各1命中真实红，29输入同；128/2/4/128次双入口观察均
在转换调用span返回UnsupportedNode，FE/ownership前置均通过；LLVM生成尚未到达，
不计IR验收。仓外backend4-red-receipt保留，后继先SSA/lower再LLVM，各自强化oracle。

`eb059787`仅抽取两入口各自checked CFG；`486c1a5c`新增私有CheckedIntegerConvert，
source/target由真实operand/result类型决定，双result为目标整数与Boolean，failed导Abort。
原arithmetic及unit pending operands/loan的carry/rebind保持；LLVM暂显式Unsupported。
后继普通oracle锁128对源/目标、失败边与成功使用、receiver call结果身份；旧2lower边界
改验真实SSA成功后LLVM明确拒绝。此SSA阶段尚未执行，不据此声称LLVM/native已实现。

SSA候选首次actualbase尺寸检查发现render.rs 1001>1000；独立抽出原四个标量命名纯函数，
不提高阈值、不挤行，保留首失败日志。此机械修不改renderer输出或转换语义。

`f14fe27c`阶段6exact各1、42输入同：single LLVM拒绝边界绿，其余5红。single的64对及
receiver/live/pending均产出verified SSA；unit因body type plan未登记转换失败Boolean而
MissingFact。第4首次实际进入LLVM并明确Unsupported。仅补真实CheckedConvert的Boolean
类型登记，不改其它表达式推导；原日志保留，修后同6重新验证。

`956da303`同6复验5绿1红，43输入前后同、各1命中；两入口128对SSA及receiver唯一call、
live/pending普通样例通过，旧2边界到LLVM明确拒绝。第4实际LLVM Unsupported仍红，
不计后端完整交付；carry仅限现样例与verifier/CFG，未宣称全部loan/owner组合覆盖。

LLVM候选`c8e797a4`从原source符号扩展/零扩展到内部i128，比较目标闭界后形成failed，
再以无poison截断产生仅成功路径可用的值。新增动态128对IR断言锁扩展符号、实际min/max、
谓词先于cast和Abort；live/pending样例进一步锁原owner双边交付及成功参数重绑。尚未运行，
本阶段仍无native值/越界Abort执行证据。
