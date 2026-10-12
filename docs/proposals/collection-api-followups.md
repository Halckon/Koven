# 集合剩余 API 与迭代合同（候选）

> **性质**：非规范候选设计 · **状态**：未启用 · **读取时机**：评审未选顺序集合与 Map API 时 · **唯一真源**：本页的剩余 API 范围；来源核心与算法分别由链接候选维护

本页接收原[M3A](../archive/development/sequential-collections-spec-draft.md)和
[M3B](../archive/development/map-collections-spec-draft.md)未承接范围。0275/0276/0279–0285、
0287/0288及0295的选定能力不重新列待实施；现行API以[Guide12](../guide/12-collections-destructuring.md)为准。

## 尚未选定的范围

- 显式容量控制（如预留容量）及真实程序需要的其他基础操作；capacity不是已批准公共属性。
- Map更广查询、遍历、输出顺序与provider失效；词频程序重扫argv不证明开放Map迭代。
- 每操作封闭receiver/参数mode、求值顺序、提交点、旧/新值owner、活跃loan与relocation、
  大小/布局溢出、分配失败、用户代码调用及drop次序，不套用其他语言API。
- String、Resource、嵌套容器和零大小布局按真实可构造类型选范围；内部ZST不冒充源语言支持。

原C01–C08及K矩阵未选部分保留：空/边界/增长或删除、Copyable/MoveOnly正反例、前缀return
与正常提交、借用/provider失效、精确清理、泛型/多文件身份、独立参考及双入口/native回归。
Spec只选其Goal相关项，不将整张旧矩阵视为全部完成或重跑。

## 复用与条件依赖

借用查询、Missing/Found(null)、Map→List→字段统一复用[M2B](general-borrow-access-results.md)。
算法/物化/N1b复用[collection r3](collection-algorithm-ownership-r3.md)，通用复制复用
[clone候选](explicit-clone.md)；r3未承接Map算法或迭代器协议。
consume、只读Map转换和已有具体正确性缺口由[Spec索引](../specs/README.md)定位，不因本页激活。
Set、开放Iterator/Iterable与视图分别选定，不捆入Map基础能力。
