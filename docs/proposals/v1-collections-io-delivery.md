# v1 集合与常用标准库最小交付清单

> **性质**：非规范交付候选 · **状态**：规划已选、语义未启用 · **读取时机**：冻结下一片 v1 集合、同步IO、时间或数学库 Goal 时 · **唯一真源**：本页的最小验收清单与依赖顺序；详细 API 由链接候选维护

2026-10-15用户批准按建议顺序推进；本页基于actual main `3104baa47845a9ec738c06aa60cf61f5d1a3772e`
（tree `64ebc102c8237f74f87db8311b14949b9c7b0105`）静态核对，只推进合同准备。
0316–0318的M2B实用化及0319–0321的A/B/C修复均有限闭合，不表示广义M2B或v1全部完成。
当前无active合同，已有0291/0292仍draft；本页不新增编号、不启用Guide、不授权未审生产语义。

## 最小清单与依赖顺序

| 顺序 | 交付目标 | 可复用输入与最小验收 | 冻结前必须决定 |
|---|---|---|---|
| 1 | 集合初始化：保留listOf，首候选非空mapOf | 空/单/多条、重复key、源码求值次序、Copyable/MoveOnly/nullable与失败前缀；见[初始化候选](collection-api-followups.md#非空map初始化首候选) | 条目参数表面、类型推断、重复key与完整owner移交；不能用consume转换替代初始化需求 |
| 2 | Map的for遍历；现有顺序for不重做 | 空/多条、键值访问、break/continue/return的选定范围；元素loan、遍历期间变异拒绝与退出清理 | 输出顺序、key/value binding mode、provider失效；不默认哈希顺序或开放Iterator |
| 3 | 同步、非逃逸、Borrow的forEach首片 | 先复用顺序provider，再复用选定Map遍历合同；每元素调用一次、0元素不调用、借用结束再推进 | 回调形状、捕获/逃逸、局部return与Abort边界；不引入非局部return、async或持久迭代器 |
| 4 | std同步文件读取/写入最小合同 | [文本与IO候选](text-input-and-io.md)的空/UTF-8/失败/限额/清理矩阵，公开project与single入口 | readText/writeText的返回错误形式、编码/路径、覆盖/短读写与资源责任；API冻结后可与集合实现并行 |
| 5 | map/filter与代表应用 | 算法按Borrow/owned分别选片；最后以真实文件输入→集合处理→输出应用联合验收 | consume链对应0292，借用filter/N1b及复制物化另有前置；不能一并启用 |

只有步骤2依赖新Map provider；顺序forEach可先复用现有provider。文件IO不依赖全部M2B泛化，
不因排在表中第4行而等待所有集合算法完成。代表应用先固定期望输出、错误与资源边界，
不能由“能运行”外推所有API组合；stdin等新增常用库范围见下节，网络、异步IO与通用Clone仍在本清单外。

## 常用标准库补充清单

2026-10-15用户补充stdin/time/sleep/math等常用库范围；以下为v1待交付目标，不宣称已有实现。
总体按集合→同步IO→time/math推进；只存在局部依赖，已冻结的独立合同可并行，首片Map候选不扩。
公开包名/API拼写仍待合同决定，不预设`std`命名空间、系统ABI或泛型重载已经可用。

| 类别 | 基线静态事实 | 待冻结的最小合同与验收 |
|---|---|---|
| stdin | 未在当前std/标准环境找到行读取API | 行读取区分空行、EOF与错误；UTF-8/非法字节、换行保留策略、限额与关闭责任；复用IO错误/资源合同 |
| stdout/stderr | 已有String Borrow的println写全部字节再LF；不是完整输出流API，error不保证stderr内容 | 不换行写、stderr写与flush的API/错误；短写、U+0000、失败和输出顺序，不能把CLI转发stderr当标准库写入能力 |
| 时间 | 未找到标准Duration、wall-clock或monotonic API | Duration表示/单位/溢出，墙钟与单调计时分别定用途；精度、回拨、平台范围与可比较性，不引入时区/日历 |
| sleep | 未找到标准阻塞sleep API | 依赖Duration，负值/零/超额、被中断后的行为及允许误差；验证阻塞与不忙等，不承诺精确唤醒。异步delay待并发合同 |
| 基础math | 整数运算/比较可复用，不等于已有min/max/abs/clamp库 | 先整数适用类型与min/max/abs/clamp；abs最小负数、clamp反向上下界与溢出策略需明确 |
| 浮点math | single字面量lowering仍明确拒绝Float；未据此声称任何完整浮点执行链可用 | 浮点基础类型/运算/ABI先核齐，再排round/sqrt/pow/trig；NaN/无穷/负零、舍入及域错误逐项定，不从函数名字导入宿主语义 |
| String/解析格式化/argv | String与argv已有局部实现；不是通用文本处理库已齐 | 盘点concat/clone/字节长度及现有argv；解析、整数/浮点格式化和文本操作按真实缺口选片，通用toString/Str不自动启用 |

代码锚点：[标准println环境](../../crates/lang-frontend/src/type_checking/mod.rs)、
[argv构造](../../crates/lang-codegen/src/llvm/entry.rs)、
[single literal门](../../crates/lang-codegen/src/ssa/lower_frontend.rs)、
[当前std声明](../../crates/lang-std/koven/prelude.ko)。现行println/argv/String合同见
[程序与标准库](../guide/13-program-runtime-standard-library.md)。以上只是固定main的静态核对，未重跑测试。
新增IO详细错误与清理只在[IO候选](text-input-and-io.md)冻结；本表不复制实现合同。

## 已有能力与实际缺口

- `listOf`空/非空、Array/List/MutableList普通for已有native源码矩阵：
  [unit container size](../../crates/lang-codegen/src/native/unit_container_size_tests.rs)、
  [unit for](../../crates/lang-codegen/src/native/unit_for_tests.rs)、
  [single for](../../crates/lang-codegen/src/native_sequential_for_tests.rs)。只复用其明确provider/控制流范围。
- `take/drop/dropLast`已有[标准库Koven实现](../../crates/lang-std/koven/algorithms/ranges.ko)，不重新列待做。
- 当前两条[Map类型检查路径](../../crates/lang-frontend/src/type_checking/checker/map.rs)均拒绝带参构造；
  [迭代provider](../../crates/lang-frontend/src/type_checking/iteration.rs)只有顺序容器/View，不接纳Map。
- std当前只有prelude与ranges声明/算法；Guide的File示例不是readText/writeText实现验收。
- 普通unit字段赋值、single nullable Resource工厂与更广CFG仍是[独立缺口](../architecture/lowering-support.md#现存入口差异)。
  仅在选定代表应用实际阻塞时另立有界合同，不以无限扩M2B作为全部v1工作前置。

## 既有draft与启用门禁

[0291](../specs/drafts/v0.44/0291-readonly-map-construction.md)只处理填充后的MutableMap.consume整体
转只读Map；[0292](../specs/drafts/v0.43/0292-immediate-consuming-sequence.md)处理链内立即消费。
二者都不等同于带参mapOf，也不因本页获得实施授权；其Guide/ADR阻塞继续保留。
本基线全树最大Spec为0322；正式冻结下一片时再核官方main/分支/PR库存并申请唯一编号。

各片依次通过：源码/现行Guide差异核对→候选独审→需变更语义时明确启用新Guide→独立Spec→
真实失败与最小实现→single/project basic/constant按适用范围逐项验证→双宿主CI→有限归档。
云端仅审定普通源码exact与工程检查；本规划阶段只做docs/diff静态检查。故障注入/校准不在云端运行。
日志与完整dump仓外保留；Spec只记输入身份、有限结论、失败和未覆盖边界。
