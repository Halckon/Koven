# SPEC-0327: 显式 checked 整数转换

> **性质**：实施草案 · **状态**：draft · **读取时机**：准备 v0.44 E 的普通前端红测时 · **唯一真源**：本 Spec；语言合同见 Guide

| 字段 | 值 |
|---|---|
| 状态 | draft |
| Goal ID | `KOV-P2-0327` |
| Phase | Phase 2 类型身份先行；Phase 3/4 按后继验收分片闭合 |
| 基线 | 实际 main `aa42bdc5f1f3172b22f1ff7f708155b1aa077f10`（从020ff草案正常合并） |
| 分支 | `feature/spec-0327-integer-conversions`；已继承主干0325；不包含未合0326实施 |
| 规范 | [Guide整数转换](../../../guide/04-expressions-operators.md#整数-range-与显式整数转换) |
| 当前状态 | 六项普通前端首轮4红/2初始拒绝控制绿；未实施生产 |

## 1. 批准范围与依赖

实现已启用的八整数builtin完整8×8零参数toXxx转换，包括identity。
按数学值域检查后转换；越界沿既有Abort，不先截断，不按位宽假定signed/unsigned总成功。
不启用Float/Double、Char、Boolean、String、nullable特例、OrNull、const或新输出保证。
不改变普通用户同名成员的选择规则，不把名字本身当intrinsic身份。
A/B主干交付优先；本草案可中断，后续在真实main同步后冻结测试输入，不混入其发布分支。

## 2. 最小实现候选（尚未实施）

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

首批红证与接口方案通过后再active登记与最小生产；本片结束前闭合所宣称各Phase，
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
