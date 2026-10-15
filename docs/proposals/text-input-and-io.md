# M1B-b 文本输入与 IO（候选）

> **性质**：非规范候选设计 · **状态**：未启用 · **读取时机**：选择 argv 之后的文本处理切片时 · **唯一真源**：本页的未选输入与错误合同；现行语义见 Guide

argv首片和Map应用分别由0274、0295承接；本页只保留未选文本输入能力，不重开argv矩阵。
[原材料](../archive/development/text-processing-spec-draft.md)是冻结历史；实施时重核main、
选有界Goal并形成Spec，新API先满足Guide前置。

## 待决定范围

- 首片选stdin或文件之一；封闭流/文件owner、打开、短读、读失败与关闭，不默认两者齐备。
- 固定UTF-8、换行/空白/分词、U+0000、非法字节和大小/词数/计数上限，不静默替换非法字节。
- 明确特殊词转义或拒绝、确定输出顺序与整数转换，不依赖未定义的哈希遍历顺序。
- 区分Result、宿主operational failure与Abort，固定退出码/stderr；`error()`不自动提供精确错误文本。
- 公共逻辑属Koven `.ko`；最小runtime桥接封闭ABI/权限/资源责任，不能假定已有能力可包装libc。
- String长度、索引、slice、分词、格式化逐项选范围，不从concat/clone推导。Result构造/匹配、
  泛型布局和`?`的typed/ownership/native支持分别核验。

## 后继验收条件

原T4/T5及T-C–T-F未选要求继续保留：空/多行/UTF-8/非法字节/短读/失败/超额输入；确定输出
和退出；计数/容量边界及独立参考；字符串、条目、容器、IO资源清理；公共入口、同名声明、
来源身份和错误Span正反例。选择Result才验Ok/Err，选择`?`另验Err提前返回owner/loan/drop，
正常释放不能扩大为Abort展开。

选定合同先红测，再用双宿主真实project build/run及直接阶段测试验收；M1A和argv保持。
不用宿主实现代替Koven程序，不删已选失败用例。normalization、locale/case folding、正则、
异步IO、Str、通用toString和公开C FFI均非默认范围。

2026-10-15已按[v1交付清单](v1-collections-io-delivery.md)选择同步文件读写作为最小合同方向；
本页详细错误/编码/资源决策仍待冻结，stdin不自动捆入，现行Guide示例不构成实现验收。
