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
