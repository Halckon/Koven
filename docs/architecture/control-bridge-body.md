# 跨控制桥接后的独立 body 消费

> **性质**：当前实现事实 · **状态**：current · **读取时机**：修改顺序control活根桥接后的Replace/返回值清理 · **唯一真源**：frontend实际事件与codegen发射许可 · [入口桥接](unit-loop-counter-audit.md)

## 合同范围

桥接的前control退出来源仍是静态整组替代，不选择某个entry/first-body token作为下一入口。
额外body产品以目标根位置作为形式Header义务：每根一次直接String Replace，返回旧值绑定
后按真实drop fact清理，正常输出来自实际String定义。具体形状识别与旧RootTokenTransaction
共享，但不创建或复活旧入口owner ID，也不恢复旧Complete观察。

实际condition完成和body-entry是必要前置。后续literal、Replace、绑定、Drop及正常尾部
必须全部发生且身份、顺序、资源集合一致；不完整只撤销body产品，已完成入口桥接不因此
升级为权限。body产品绑定next control、目标根顺序及callable，携带独立producer catalog和
现有参数化normal transport规则。静态定义不是跨运行迭代持续存在的owner实例。

## 后端消费

选择有完整normal transport的桥接后，逐step维护Ready→Replaced→Bound→Dropped状态：

- Replace发射前核实际body块、当前根槽、call身份及真实String producer；之前各step必须已Dropped。
- 发射后核同一指令位置、span、根与replacement以及返回old槽，才能接受Replaced。
- 返回绑定必须由该call初始化且对应真实drop target；实际Alias唯一发射后才进入Bound。
- Drop必须使用同一前端fact和实际绑定槽；沿原统一drop发射一次，不增加第二份清理。
- 所有step均Dropped、实际body Replace数精确且入口桥接CFG审计通过后，才记录body完成。

该链与已有counter/header/owner drop许可互斥，不重选其他产品补缺。某step失败可保留此前
局部发射记录，但不得有整组body完成记录或成功返回的lowering结果。代码生成仍使用原有
checked arithmetic与通用控制流；此处不单独证明scalar success/abort、第二control退出
或整个函数的owner完整性。

## 验证与未覆盖

直接/逆序Alias及部分live-root桥接使用真实typed-SSA、LLVM验证和零轮/多轮native。
普通测试核实际三阶段消费数及整组完成；仅CI故障注入检查缺Replace/绑定/Drop许可、错根
第二根失败、错误指令位置及重复Drop提交。本地只编译这些故障测试，不运行。

该实现是受限body的实际消费约束，不是一般跨轮归纳、动态路径trace、嵌套控制新权限，
也不宣称这些源码此前不能通过旧lowering执行。


## 第二控制的 checked tail 接线

可选counter-tail身份仅在上述normal body和真实减一/guard写回均完成后封存；保留精确
control、callable、guard/counter及四个表达式身份，不造旧入口token。后端复用已有独立
checked-expression receipt核减一指令、正常success与terminal abort边、唯一前驱以及
success→header。正常分支必须按counter写回→大于零比较→guard写回排列，只有精确四条
scalar指令且不读写活根；header条件使用实际guard槽。整body及入口审计通过后才记录tail完成。

CI-only负控覆盖abort误接header、错误回边、陈旧比较、错误counter写回、缺receipt及
success多余前驱；本地仅编译。
真实native覆盖两段非正零轮及正数多轮；不声称运行触发了overflow abort。tail本身不提供
第二control初值比较完整证明、false-exit清理许可或一般跨轮owner归纳。


## 第二控制 false-exit 分区与清理

仅完整normal body和已完成tail可附接真实false出口分区：复用drop_loop_exit新增的实际
facts及出口ValueState，每个原目标根恰好属于live或LoopExit(next_control)的named Drop，
不从AST推断清理，不造退出owner token。缺失或重复事件不发布该附加分区。

后端在既有统一Drop发射点消费独立退出permit，核精确fact、根槽、finished块、指令位置
和span，前项必须完成；发射后核唯一实际Drop并标记一次性完成，与其他Drop许可互斥。
false边必须唯一抵达finished，再跳至真实exit；finished只有分区规定的Drop，不能碰live根。
全项消费后才记录分区完成；仅live为空才记录all-dead。没有增加第二次Drop发射。

真实native覆盖两段0/2轮、第二段全死/部分live/全live三种分区。CI-only负控覆盖缺permit、
错误指令及重复提交，仅编译不执行。live根的后缀清理仍走通用lowering，此分区不授予
其独立suffix许可，也不是第二control初值或一般跨轮完整证明。已消费的旧fact不认领
同名变量重新初始化后的新Drop；原fact重复仍拒绝，native覆盖该代际边界。最终再次逐项
核Drop实际根槽，并用CI-only错误退出边/晚期错根负控防止发射后篡改留下完成记录。
