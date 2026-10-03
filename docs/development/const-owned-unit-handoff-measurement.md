# const owned-unit 交接的动态 index 计数

> **性质**：有界测量证据 · **状态**：current · **读取时机**：复核 SPEC-0254 的 index 次数与产物保全时 · **唯一真源**：本页及原始计数 JSON；验收状态见 Spec

2026-10-03 UTC，x86_64 Linux。两个固定 const fixtures 实测：旧native交接index增量4→1，
旧lower2→1；工厂1，预建view native/lower均0。入口和constructor两个精确counter一致。
本片未做可支持性能结论的耗时/RSS实验；probe附带计时不作性能证据，不能从次数减少
声称提速、内存节约或性能等价。

[完整计数与复现资料](evidence/const-owned-unit-handoff-measurement.json)保留两cohort的源码
commit/tree、probe源码/hash、fixture完整字节/hash、精确符号/source region、计数、提取脚本、
构建参数与object/link/run结果。大型binary/profraw/profdata/coverage export不进入仓库。
本页不新增生产counter或长期benchmark框架，不替代[唯一验收账本](../specs/active/0254-const-owned-unit-borrowed-handoff.md)。

## 固定来源与方法

- before真实main：`c6b84ecb46563b7de2bfeb9f481e75cf4a861323`，tree `d5a305f0ce877b1630c1e32887eb645bb8e39735`
- after本地实现：`0d5ba88f760f805a95947f868607b945b1c66b5c`，tree `4ffd6bdab15586120ad8c3f5639c8a083a571e60`
- Rust/Cargo1.96.0，Rust内置LLVM22.1.2及匹配官方coverage reader；native LLVM/Clang21.1.8
- test opt0/debug0、`-C instrument-coverage`、incremental0、jobs2，全部构建串行
- small为2文件；representative为31个provider与1个entry共32文件，均含String/Boolean常量、
  可达跨文件调用与短路；它是合成输入，不代表真实项目性能

两个独立detached worktree临时附加同职责test-only probe，直接调用原crate-private lower，
不复制算法或扩大可见性。probe及attachment留证后撤下，两工区tracked/untracked diff均为空。
每个cohort/fixture/mode独立fresh process，执行1个exact probe；在同probe同fixture内扣setup。
view下游减去factory mode，不能减去setup冒充下游0次。

精确匹配完整demangled名称，再以type/address/size找到唯一mangled符号；coverage function
count与该符号的source entry region count相等。分别核`index_compilation_unit`和
`CompilationUnitIndex::new`，没有把closure或包含同名字符串的函数计入。

## 原始总计与差分

下表两个fixtures、入口与constructor两counter完全一致：

| cohort | setup | 显式index | old lower | old native | factory | view lower | view native |
|---|---:|---:|---:|---:|---:|---:|---:|
| before | 4 | 5 | 6 | 8 | — | — | — |
| after | 4 | 5 | 5 | 5 | 5 | 5 | 5 |

显式index校准增量1；旧native 8−4→5−4，即4→1；旧lower6−4→5−4，即2→1。
factory5−4=1，view lower/native均5−5=0。setup中的ownership checker index不属于优化删除项。
成功输入双counter一致不能推广到非法输入早退；失败副作用另由native exact reserve oracle证明。

before8与after14共22个fresh processes全部1 passed/0 failed/0 ignored；分别有739/748个
原libtest被filtered，这些profile不被当作完整suite回归证据。第一次fixture把package app
放在逻辑entry目录，names gate以L0146拒绝；修正声明为entry后才冻结成功的两套输入，
失败stdout/stderr/profile单独保留，没有作为成功样本。

## 产物与边界

同fixture的before旧native、after旧native与after新view native三份object字节完全相等。
small SHA256 `0706a3d44a61c2720455b1ccb165cf01c617745596bca455d197a073fe8ed3fe`，
representative `a65fdb95a774517ffde5e9848738f478f506c3ac16475deef75c1b70240a6456`。
六份均用Clang21.1.8真实link/run；small stdout为`const-0-界\n`，representative逐行
`const-0-界`至`const-30-界`，stderr空、exit0，完整字节结果在JSON中。

这些是Linux成功路径的结构计数与产物等价证据，不覆盖macOS、失败index次数或性能。
本地完整行为门禁与独立review另记Spec；尚未发布Draft PR或执行本片精确head双宿主CI。
