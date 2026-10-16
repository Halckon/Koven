# SPEC-0328: IntRange / LongRange 普通值首片

> **性质**：实施合同 · **状态**：in-progress · **读取时机**：准备 v0.44 D 首片接口与普通源码红测时 · **唯一真源**：本 Spec 的实施范围；语言合同以 Guide 为准

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P2-0328` |
| Phase | 类型与所有权身份先行；SSA、LLVM、native 分片验收 |
| 基线 | 正常合入实际 main `8a02572fbb490deb614009a84228bf5e06347f52`，已继承 0325/0326/0327 |
| 分支 | 本地 `feature/spec-0328-integer-range-values`；授权远端 `fix/spec-0328-integer-range-values` |
| 规范 | [Range 身份](../../guide/03-types-generics.md#简单变量赋值与整数-range-身份)、[操作](../../guide/04-expressions-operators.md#整数-range-与显式整数转换) |
| 当前阶段 | 前端、值SSA/LLVM与有限Linux native已本地验收；远端CI待验，for另片 |

## 1. 范围与拆片责任

承接已启用的 IntRange / LongRange 非泛型环境身份、同型端点 `..` / `..<` 构造、
普通 Copyable/Transferable 值存储、`in` / `!in` 及同型序列相等。
不复制 Guide；公开构造器、额外成员、排序、hash、step、切片、用户 provider 均不扩展。
不同 Range 类型与已定型 Int/Long 不隐式转换；既有字面量定型和 Map 包含性保留。

数值 Range 的 `for` 是已启用但本片不交付的独立后继，由本片作者继续认领：
在本片值身份/ABI稳定后，另立实施 Spec，按[独立迭代合同](../../guide/12-collections-destructuring.md#整数-range-与-map-的独立迭代合同)
闭合无 loan 的值交付、终点判断、continue/break/return 与 body owner 清理。
不以首片 done 宣称 D 全部完成，不预占后继编号。
0327 已随实际 main 8a02572f 正常合入；其 main CI 异常仍优先处理。

## 2. 初始接口缺口与表示约束（历史起点）

- `type_checking/model.rs` 的 BuiltinType 尚无这两个公开身份；名称环境、single/unit 类型表、
  TypeRef 解析、Copyability/Transferable 分类须一致接线，禁止按用户声明名字冒认。
- single `checker/expression.rs` 与 unit `bodies/checker/operators.rs` 的 Range 运算仍未完成。
  新事实应保留操作种类、真实 operand ID、端点类型及 Range 身份；后端消费事实而非重推 AST。
- 现 `iteration.rs` / `compilation_unit/iteration.rs` 的顺序 provider 是 Borrow 交付；
  `lang-codegen/src/ssa/provider.rs` 以 LoanId 和 length/cursor 表示，不能借它实现数值 Range。
  既有 RangeView / SSA Range carrier 是 View 借用机制，必须保持独立。
- 内部候选为定宽端点与 empty 标记的普通值布局，具体 SSA 类型/ABI 在生产前独审。
  可考虑 `{first, last, empty}`：半开仅在 start < end 后计算 end - 1；闭区间 MAX 不加一。
  相等按空标记或有效端点判断，不逐元素展开、不以 Int/Long length 表示全域基数。
  此候选不是公开字段、构造器或可观察布局合同。
- 先保留现有两个 TypeId 体系；不为此重构所有标量、借用 provider 或容器布局。

## 3. 拟定首批 ordinary frontend exact

以下为原六项验收计划；已落具体 exact 名称与实际红证见后文。
helper 仅普通源码经真实 lex/parse/names/raw types/complete API，两入口观察收齐后断言；
无 facts/IR 构造或变异、无语言产物执行。新 API 未存在时先采现有公开查询事实，
不得引用未来 API 产生编译失败后称语言红。

1. `same_type_endpoints_publish_range_identity`：已定型 Int/Long 参数分别构造闭/半开区间，
   用真实 TypeRef/类型表核结果，complete 成功；不能只断无诊断或把两入口 TypeId 互比。
2. `range_values_cross_storage_boundaries`：局部复制再用、普通参数/own 参数与返回、普通字段、
   既有支持的容器 element 路径分别采证；明确每个边界，不能用局部成功概括全部存储。
   示例 `fun make(a: Int, b: Int): IntRange = a..b`；未知类型当前可能先名称拒绝，分账。
   显式查询 Copyable/Transferable 分类并以普通调用/复制验证，不启动线程；own 只是参数采样。
3. `range_membership_requires_matching_endpoint_type`：Int/IntRange、Long/LongRange 的 in/!in
   结果 Boolean；Int/Long 混用、非 Range RHS 与现有 Map 包含性成对控制。
4. `range_equality_preserves_nominal_type_boundaries`：同型闭/半开混合 ==/!= 为 Boolean；
   IntRange/LongRange 不相容，不用静态类型绿冒称序列值比较已执行。
5. `invalid_range_endpoints_preserve_source_diagnostics`：已定型 Int/Long 混用、Byte/Short、四种
   unsigned、Float/Double、Char、Boolean、String、nullable 端点拒绝；端点错型锁 Guide L0085，其他前置诊断先采完整 code/span。
   不放宽 null 推导或 nullable 支持；用户同名类型仍按真实名称解析规则处理。
6. `range_values_do_not_publish_extra_apis`：公共构造调用、first/last/step/size/hash 成员不开放；
   `IntRange<Int>` 非泛型拒绝与 Map key 的 Hashable 约束拒绝另验，缺 hash 成员不等于
   不满足 Hashable。均与合法构造配对，避免以 Range 身份尚不存在误计专属拒绝门通过。

运行模板：`cargo test --locked --offline -p lang-frontend --test integer_range_values NAME -- --exact --nocapture --test-threads=1`。
落 helper 后再冻结 head/tree/输入 SHA 与唯一 exact 清单，独审后按授权执行；串行绝对 target。
本计划不授权宽 target、聚合、故障注入、校准或 fuzz；测试自身编译失败不算语言红。
隐式 `1..2` 表达式事实与显式 IntRange/LongRange 身份分组采证：后者当前名称拒绝可正是
本片缺身份的真实红，必须保留阶段与完整诊断；只将无关源码拼写/解析错误列为 harness 前置失败。

## 4. 后续值语义与资源验收

- 真实源码 FE → ownership → SSA → verify → LLVM；先核无 owner loan、Copyable 身份及
  存储/传参/返回/字段/容器实际支持范围，未支持边界须结构化拒绝并登记，不能静默遗漏。
- 单文件与项目各独立成功 native：动态端点构造、空/单元素/反向/半开、in/!in、==/!=；
  同型不同写法 `1..2 == 1..<3`，不同形态的空区间相等，非空端点不同不等。
- Int/Long MIN/MAX、MAX..MAX、MIN..<MIN、反向 MAX..<MIN、全域闭区间，避免任何
  MAX+1、空区间无条件 end-1 或基数溢出。用显式数学 oracle，不靠实现自身 roundtrip。
- 端点及 membership 两 operand 的带输出普通函数验证源码顺序、各一次；活跃 String owner
  与 pending 参数跨构造/比较后仍可读，正常退出精确清理。有限样例不外推所有析构顺序。
- for 未完成仍拒绝，不把 Range 值装成 View/provider 来凑绿。旧 View、Map 包含性、
  整数比较与存储 ABI 的受影响普通 exact 必须逐 helper 审核后另冻结。

## 5. 交付门

接口与六项红证已明确，现 active 登记并分片实施；前端阶段通过不能替代存储 ABI/native 交付。
新增 target 必须同步 CI 选择与真实新增数量/唯一预算，执行实际 main base 的尺寸检查，
不得抬阈值或挤行。最终工程、双 host CI、归档 head CI 与真实 main 验收分账。
备份用 binary/full-index format-patch 增量及 base/head/tree/SHA/恢复说明；不修改 E 固定包。

## 6. 前端红证与真实主线同步

首红 `86e4623f` / tree `3112d1b0` 三exact各1行为红，11输入稳定；32入口观察收齐，
无编译/解析前置失败。隐式构造Deferred(Call)/L0165；显式身份single L0080、unit四例停names门；
包含/相等父Deferred(ControlJoin)，原Range诊断保留，能力/算法尚未穿过身份缺口。
仓外red3收据SHA `18e2a3a78b9ca802c30325f82153bd8df1fd78cecb6c1156a4557d9e4d390ad5`。
正常merge `8843af8e` 双父 `30504b87`/真实main `8a02572f`，只解索引/图冲突；
E生产/归档及A终止性后继责任保留。独立 `74499028` 只修README_CN两处v0.44标签。

后三exact `c8867978` 各1行为红，11输入稳定，storage16/非法端点60/API40入口完整；
storage八unit停名称门；错型端点names-clean仍Deferred(Call)/L0165，缺预期L0085。
API合法身份正控先红，18负控仅观察，不计非泛型/Hashable/member拒绝验收。
无编译/解析前置失败，所有权/native未测。仓外next3收据SHA
`7f9f7d4eafdfeb2506279810180f9911f18d54d307fe511b02ec251cb2787746`。

## 7. 前端实施切片

环境绑定的两种 builtin 身份显式登记 Copyable/Transferable；不加入 numeric、Hashable 或 const 白名单。
两入口运算事实保存原 operand ID、类型和操作种类，随 trial 回滚/排序/validate/final admission 接线。
ABI 完成前，lower 在规划前按真实类型使用与 descriptor 结构化拒绝，包含 nullable 包装；
这只是阶段边界，后续必须关闭字段、容器元素/Map 值及普通参数/返回等承诺存储路径。

首次 FE-stage8 在 cb2816cb 编译报两处 E0624：新兄弟模块不能访问原私有 operand 诊断方法。
仅第1命令启动、0测试执行，35输入未变；第2–8未跑。后继只将两方法可见性改为 pub(super)，不改诊断或算法。

89b1b70e 的原六项 FE exact 各1命中成功；第7 lower 测试编译 E0277，原因是 expect_err 要求成功 Program 实现 Debug。
该项0执行，第8未跑；改测试提取 err 后再 expect，不为测试给生产 Program 添加 Debug。原输入35项稳定。

090e721f 同8实际各1成功，35输入每轮前后相同；六FE加44个真实lower边界入口与无Range近邻，
只证明FE阶段与结构化暂拒，不证明Range SSA/LLVM/native。收据SHA
`2563e19c441ee1ec8b647bb359d4ab21b5f60ef827552f18232c545438e3a7de`。
后继 tests-only 锁真实descriptor的kind/operand/result、两种精确builtin身份、原L0085 operator span；
公共构造L0143、成员L0165、非泛型L0082与Hashable L0161按实际源span收紧，加入普通重载trial唯一事实控制。

## 8. FE 阶段固定备份（非完成交付）

实跑 head `b1fd0d344964220632bd1bf736265069793799f5` / tree `49b5bebcc4a6b0abd17cac087af0911c1e2fab4d`：
强化8项与共享5项共13个唯一 ordinary exact 全部各1命中；核心35、共享41输入每轮前后稳定。
共享反查覆盖 ALL 环境绑定、local/unit seed 尾序与数量、普通类型表、single Map contains/get/remove
及 unit nullable ==/!= 的确定性事实；未发现旧16项硬码快照。Map此项不证明binary in或unit完整准入。
四工程 fmt/check/clippy(frontend/codegen/cli)/build CLI 均 exit0；只编译不执行语言产物。
实际 main `8a02572f` 的完整尺寸门禁0错误，docs699通过。执行连接短暂中断后读取原五轮完整收据，未重复启动。
仓外 `spec0328-evidence`：核心收据SHA `55bba4fd9677023c562dffa32adf111a493d26d368f8aea124dfa0908fed14c5`，
共享收据SHA `be7bd5728024399eb183bf566b256e0ae4b9911d5ee7b5b0dab36c6908bbb60a`，
工程收据SHA `3b2ffdf4df8c71836922ab4b5b96392689a4eea38369433cce82bf6bb1b20a71`。

本阶段真实 Range 源码通过前端/所有权后仍在lower结构化Unsupported；普通无Range程序可生成LLVM。
尚无 Range 的SSA值ABI、LLVM运算或native成功证据；CI亦未运行，Spec保持in-progress。
后继必须验证普通存储位置与Map查询实际消费；Map专用presence/payload结果不等于一般nullable handle，
独立Range?现有边界分账，不能笼统排除Map.get/remove或以丢弃返回值冒称完成。

## 9. 后端首批普通源码采证

三个独立exact先收齐两入口：构造/包含/相等40入口；局部、参数返回、字段、List及
Map.requireValue/get/remove实际消费32入口；调用顺序与连续半开/外层join的SSA→LLVM8入口。
逐源记录names、types、ownership、lower到达阶段；Map查询若前置拒绝单列，不用其他case覆盖。
初始断言仅定位lower/LLVM缺口；注册身份、单次operand与实际PHI前驱待实现后收紧。
仅真实前端→ownership→lower/verify及内存LLVM，不执行产物；for provider仍独立后继。

`274b0d30` 首3各1命中真实红，40/32/8共80观察均通过names/types/complete/ownership，
再被现有pending guard结构化拒绝；Map三种读取无前置拒绝，但尚未检验解除guard后的MapResult ABI。
LLVM未到达、无native；37输入每轮前后相同。仓外backend-red3收据SHA
`ce553cc11f923cc3869e94d6652ec62d62039443524a652216a1a55b8bb5c225`。

## 10. 后端值 ABI 与本地验收

私有注册表把真实 Range TypeId 映射到 signed i32/i64 端点，底层使用 Copyable aggregate
`{first,last,empty}`，不复用 View carrier，也不按显示名称识别。三种 SSA operation 核注册身份、
operand/result与shape；双入口按真实descriptor/AST/source顺序各lower一次，!in/!=沿BooleanNot。
LLVM半开仅nonempty块计算bound−1，empty块安全常量，ready真实PHI承接后续指令/外层边；
闭区间不计算MAX+1，包含/相等直接使用empty与端点，不产生长度或逐元素循环。

MapResult已有presence/payload布局实际承载Range get/remove，found后参与in/相等；native另验
get missing、remove缺失及已有key移出后二次remove。requireValue保持显式borrow val再Copyable读取；
构造/比较在辅助函数的Unit入口已通。原同函数包含Binary的四条源码仍因旧borrow-result全函数gate
结构化Unsupported，作为后继borrow控制流责任保留，不以Range名称放宽；普通val直绑四例唯一L0163。
一般Range?参数/局部null八入口仍Unsupported；unit真实nullable表达式补intern仅修MissingFact，
既有MapResult cache优先，不开启通用nullable ABI。数值Range for仍由本片作者另Spec继续认领。

阶段9adc backend8各1全绿（收据877dcc10…392b0），Linux云两native入口严格68行断言成立；
此前828ee的unit独立nullable null MissingFact保留为真实实现缺口，9adc微补丁后原矩阵通过。
最终追加remove缺失使native预期72行；CFG helper空行问题在静审发现并修复，错误版未执行。
最终实跑 `d9ce65ab` / tree `c2850805`：19普通exact各1绿、86输入前后相同；四工程均exit0。
真实8a base尺寸1166文件/38旧债/0错误，docs699与唯一composition成功路径Ran1/OK。
仓外final19收据SHA `b34dc2d2615b32ab0d474b4653f5eea176531e1f35824ee455ca67b11720811a`；
native72行/exit0由已审断言证明，无独立导出子进程log，仅当前Linux云两入口。
7个畸形SSA verifier exact仅编译，实际执行留CI；新增两个integration target精确预算107→109，
其中canonical_type_tables由CI预核发现原共享回归未被选择而补齐。尚无本片远端CI结论，不归档。

## 11. 旧 Range 门槛回归收口

静态反查旧 frozen/complete/CLI/多文件尾表达式发现六个过期 oracle；`a829ef4e` 六exact各1真实红。
Range值根现在为IntRange，原p34源码for仍唯一L0159（40..45），未启用数值provider。
0324的L0165用户文案改由普通`1 to 2`未完成Call根（30..36）继续验证，旧历史Spec不改写。
`aa46fe47` 统一25项24绿1红：多文件complete列表仍残留两个已完成Range根（实际7/预期9）；
`e394d780` 仅删这两项，保留其余七项诊断及raw Range身份，不修改生产或放宽其他门槛。
最终实跑 `e394d780` / tree `bc7e743b`：25普通exact各1绿、97输入前后相同，四工程均exit0；
唯一composition成功路径、实际8a base尺寸与docs检查通过。两CLI补充exact合计8次build，
同时锁Call L0165与p34 L0159的human/json、code/span/message/details，无新增产物执行。
仓外final25-v2收据SHA `465fb4a07fc6ec0e7e61b4e74ca160106191b4027daf6f94ab11144cf84e9ff5`；
工程收据SHA `b284554bc4f8c9e9ba3058674a5e80875179e61347d75933cd40dcbd8cba87a6`。
远端待验35个唯一exact：本地25、CI-only verifier七项、既有CLI拒绝聚合三项；后者含run模式，
本轮不在云本地执行。保留此前19绿及六红/24绿1红原始收据，尚未取得本片远端CI结论。
