# ADR-0028: 前端封闭发布泛型函数体具体类型身份

> **性质**：长期工程决策 · **状态**：accepted · **读取时机**：评审泛型body类型身份的生产方和资源边界时 · **唯一真源**：本页

## 状态

accepted

## 接受依据

2026-10-05，按用户持续实施里程碑、满足前置并行及按实际情况调整草稿的站立授权接受。
独立准备审阅指出共享DAG成本风险，纳入memo/迭代深链及确定性计数后窄复审无新增实质阻塞。
接受的是工程决定，G1–G7实现与验收仍未执行；不启用新的语言语义。

## 背景

Guide要求泛型body在类型参数环境中检查、调用按symbol identity结构替换。
跨文件调用已发布具体签名，body中首次构造的List<T>却没有对应List<Int> identity；
后端的只读find因此返回MissingFact。将类型补全放进backend会破坏typed/ownership owner边界。

## 决策

具体类型身份由frontend唯一可变canonical arena建立，再随原sealed typed产物发布。
body/trial全部结束且无类型错误后，只消费已提交的source-qualified类型与call descriptors，
复用现有结构替换；保持模板facts、旧canonical ID、ownership analysis身份及后端只读查询。

首片支持普通顶层source函数直接调用子图；真实闭合调用是种子，未调用的symbolic模板
不任选具体类型。完整body类型需求包括Nullable、表达式、局部symbol和TypeRef，不限容器。
归一化先于已有concrete owner field layout materialization，不重检AST或重选overload。

cache每种子按确定性实例key有界展开，复用既有1024上限；保存合法canonical前缀。
Phase4继续从实际entry选择实例图、执行现有预算及recipe仲裁，不新增frontend资源诊断、
延期错误capability或planner入口early-limit。边界仅在pop未处理specialized超限key时转有限frontier；计数到限不丢弃此前顺序中的
nongeneric。frontier保证现recipe collector所用concrete第一跳，后续维持原symbolic固定点；
不声明全实际recipe图已展开，覆盖须由SPEC真实kind/Span负例验证。
新cache闭合性/结构替换及读取新增类型的field-layout concreteness按canonical UnitTypeId
缓存completed结果，共享DAG不重复按路径展开；深链使用迭代工作栈。substitution memo以
具体实例环境为作用域，保持原结构替换/intern规则，不跨不同实参污染身份。节点访问计数
证明工作量，实例数或timeout不能替代类型图遍历证据。

实际支持边界和验收由SPEC-0276维护，accepted不等于对应功能已经实现。

## 替代方案

- backend intern或重检泛型body：产生第二个类型真源、破坏sealed owner/Phase边界，拒绝。
- 只查签名或unused concrete声明补种：不能解决真实body-only程序，拒绝。
- 无界遍历全部泛型图：合法增长型源码会无限扩张，拒绝。
- 任一unused root超限就前端整体报错：把实际entry不可达源码提升为资源失败，拒绝。
- root超限后整体撤销canonical且planner前快速拒绝：可能抢先MissingFact或改变既有recipe
  优先级，拒绝；保留正常前缀并由原planner仲裁。

## 后果

收益是保持frontend唯一类型真源、跨文件symbol身份和后端只读合同，补齐body-only普通函数。
成本是frontend需维护有限实例cache与frontier覆盖；静态闭合种子可产生当前entry未使用的
合法semantic canonical类型，不据此宣称runtime可达或全部generic member路线受支持。
预算与错误优先级兼容必须由完整kind/Span负例验证，不以测试总数或无panic替代。

## 关联

- 相关 Spec：[SPEC-0276](../../specs/active/0276-unit-generic-body-type-normalization.md)
- 相关 ADR：[0020 compilation unit](../accepted/0020-multifile-compilation-unit.md)、[0024 growing recipe](../accepted/0024-reject-parameter-growing-runtime-recipes.md)
- 取代的 ADR：无
- 被以下 ADR 取代：无
