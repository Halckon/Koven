# 普通 owned-unit 交接的动态计数与性能对照

> **性质**：有界测量证据 · **状态**：current · **读取时机**：复核 SPEC-0249 的 index 次数、性能与输入保全时 · **唯一真源**：本页及所链接的原始计数/采样 JSON；验收状态见 Spec

2026-10-02 UTC，x86_64 Linux/glibc。结论：普通旧 native 的动态 index 为4→1，旧 lower 为2→1，
factory 为1，预建 view 的 native 与独立 lower 各为0。两个固定输入和两个精确 counter 结果一致。
同期耗时样本噪声较大，不能据此宣称提速、回归或性能等价；不从 index 次数推导百分比收益。

[完整计数与全部采样 JSON](evidence/owned-unit-handoff-measurement.json)保留 before/after native、
独立 lower 的完整符号/计数、104 条原始进程样本、统计、核验结果、fixture manifest 与来源 hash。
JSON 内原值逐字段保全，可独立复算计数差与采样统计；大型 profile/binary 不进入仓库，
本页不建立长期 benchmark 框架，也不代替 [SPEC-0249](../archive/specs/0249-owned-unit-borrowed-handoff.md) 的行为/CI 验收。

## 固定代码与输入

- before：`7b3ac11fe1770339f2c5170981839477dae8cbf5`，tree `156c8bacde463bbe66c5639206a3e9994392744f`
- after 本地实现 commit：`f543a28d17c0cd21ca3332396f50841ade3db7a4`，tree `46987782c6518c22815e608dec3a3e8d64b6fbde`
- 本地文档 head `510e012b8bb043b1a65db4ea0455f2559ab3088e` 的 crates/Cargo 与 after 相同。
  远端发布可能产生不同 commit SHA；届时按 tree 与源码差分核对映射，不虚构本地 SHA 的远端链接
- Rust/Cargo 1.96.0；Rust 内置 LLVM22.1.2，coverage reader 为官方匹配的
  `22.1.2-rust-1.96.0-stable`；Inkwell native backend 使用 LLVM21.1.8，两者不混用
- small 为2个物理文件；representative 为32文件合成负载：31 provider packages×8个可达
  scalar 函数＋1 entry。后者不代表真实用户项目；root identity 固定 `root`，输入顺序固定

两 fixture 的路径、字节数、逐文件 SHA 与 before 完全相同；manifest SHA 分别是：
`9e661908e686b73c00208396513705d67b877a0fa42b971059cb90e53b4e1b6a`（small）、
`3f070262e824ebcefbe0b2201c56a86d28d6fb8448ab522d9621195f8f2f58b5`（representative）。
旧 legacy harness 逐字节未变，SHA 为
`3a62326727e97f17089e2bb8cf3eb66f94294d9d29aff4fd0d3488b0fc174385`。

## Native、factory 与独立 lower 计数

coverage 使用 `-C instrument-coverage`、test opt0/debug0、关闭 incremental、jobs2。
每个 mode 独立 fresh process/profraw/profdata，执行1个 exact 测试；只在同一 probe、同一
fixture 内扣除 setup。factory 与 view emit/lower 在同一位置构造 view，再用两者差消除工厂。

下表两 fixture 的数字完全相同；每个格均由 `index_compilation_unit` 入口与
`CompilationUnitIndex::new` constructor 分别计得，两个 counter 相等。

| 测量路径 | setup | 显式 index 校准 | legacy emit/lower | factory | view emit/lower mode 总计（含 setup＋factory） |
|---|---:|---:|---:|---:|---:|
| before native | 4 | 5 | 8 | — | — |
| after native | 4 | 5 | 5 | — | — |
| after view native | 4 | 5 | — | 5 | 5 |
| before 独立 lower | 4 | 5 | 6 | — | — |
| after 独立 lower | 4 | 5 | 5 | 5 | 5 |

最后一列是整个 mode 的总计5，减去 factory mode 的总计5才是预建 view 下游增量0。
由同 probe 的差值得到：native 4→1；lower 2→1；factory 1；view native 0、view lower 0。
显式 index 校准均为+1。native/factory 的 after 14进程全部1 passed/0 failed/0 ignored/0 filtered；
独立 lower 16进程全部1 passed/0 failed/0 ignored，原 before 738/after 739 个 libtest 被 filtered，
未在 probe 中执行。不能将这些测量进程表述为完整 suite 回归。

独立 lower 使用测量 worktree 的临时 `#[cfg(test)]` module 调用现有 crate-private API，
没有扩大 production visibility、增加生产 counter 或复制算法；测后已撤下 hook，tracked diff 为空。
两次定向 libtest 构建12.17s/11.82s，未重跑或改写既有104进程性能样本。

抽取先对完整 demangled 名称作唯一匹配，再按 type/address/size 对应唯一 mangled symbol，
从 coverage 精确取 function count 并核 source entry region：index.rs 169:1–172:61、
model.rs constructor 393:5–399:14，与入口 region counter 一致。不会用包含式名称匹配算到 closure。
这里只证明两个合法成功输入；不能推广为非法输入早退路径的 entry/constructor 次数相等。

## 同期无插桩性能

before 使用原封存 release binary；after 用相同 legacy harness 重建。两者 release opt3/debug0、
空 `RUSTFLAGS`、关闭 incremental、jobs2，toolchain/依赖锁/fixtures 相同。没有使用插桩耗时，
也没有以 new-view harness 替换 after 的性能工作量。

每 fixture：3个 warmup 四进程块＋10个 measured 四进程块；每块包含 before/after×setup/emit。
四种确定性顺序循环，每4块使各 role 占每个位置一次。合计104进程，24 warmup＋80 measured，
全部成功；没有删 outlier 或按结果加跑。下表每 cohort/mode 的有效样本均 n=10，时间单位 ms。

| 输入 | cohort/mode | wall 中位 [min,max] | emit 内部窗口中位 [min,max] | peak RSS 中位 KiB |
|---|---|---|---|---:|
| 2文件 | before/setup | 12.346 [11.138,15.948] | 空操作 | 55,042 |
| 2文件 | after/setup | 12.008 [10.700,15.965] | 空操作 | 55,002 |
| 2文件 | before/emit | 15.210 [14.655,22.828] | 3.154 [2.911,4.311] | 64,310 |
| 2文件 | after/emit | 15.189 [14.160,24.088] | 3.048 [2.861,4.191] | 64,418 |
| 32文件 | before/setup | 22.072 [20.514,26.413] | 空操作 | 56,600 |
| 32文件 | after/setup | 21.985 [20.895,24.994] | 空操作 | 56,424 |
| 32文件 | before/emit | 51.457 [50.175,60.817] | 28.683 [28.374,36.761] | 69,386 |
| 32文件 | after/emit | 52.210 [49.284,60.410] | 28.598 [26.949,36.527] | 69,198 |

按相同 block 配对的 after−before 差额；负号仅表示该样本 after 较短：

| 输入 | emit wall 差额中位 [min,max] ms | emit 内部差额中位 [min,max] ms |
|---|---|---|
| 2文件 | +0.167 [-2.760,+2.182] | -0.086 [-1.269,+0.908] |
| 32文件 | -0.533 [-8.911,+9.170] | -0.606 [-7.580,+6.822] |

以上原始样本范围均跨0，不是统计置信区间。预登记 setup 噪声阈值为
`(max-min)/median > 20%` 或 sample CV `> 10%`：2文件 before 的 process/内部 setup 均触发，
after 只 process 触发；32文件 before 两者均触发，after 只内部 setup 触发。
因此只能确证本次重复 index 构建减少，不能对耗时改善、回归或等价下结论。

wall 用 monotonic ns，包括进程/动态库/libtest 启动、setup、operation、drop 与退出；
Rust 内部窗口含该进程第一次 LLVM 初始化。每次都是 fresh process，warmup 只使 OS/文件缓存
趋热，不等于 warm LLVM session。CPU 窗口已协调互斥，仍无绑核、cold-OS 或共享宿主无干扰保证。
RSS 为 `wait4` child 绝对高水位，不是 alloc/free 次数，也不证明稳定内存节约。

## 产物一致性、复核与限制

同 fixture 的 before-release、after-release、after-legacy-coverage、after-view-coverage 四份 object
bytes 一致，JSON保留各项 SHA；small 为 `efb3cd38b2c28ce274910c2b8bbe5dddf396405f0da009c0b527e9b228699da8`，
representative 为 `3219b36ae252eeabb7650343c70e4fc35975c4fe8da915c8b124de6be31ef928`。
前后各经系统 `/usr/bin/cc` 链接并真实执行，stdout 严格为 `42\n`/`248\n`、stderr 空、退出成功；
链接/运行不进入 compiler 计时窗口。

JSON 所有 source JSON hash、完整测量包 manifest hash 与 frozen before binary hash 已保留；
本页制作时重新从104原始 rows核对24/80划分及所有分组 n/median/min/max。
计数差按前表同 probe相减；性能配对按 fixture、phase、block、cohort、mode 对齐，不能相减
两 cohort 的独立中位数冒充配对中位数。完整原始 profile、binary、probe 与复现脚本单独保留，
未把大型二进制纳入仓库；重测需使用准确 source tree、匹配工具及新证据目录，不覆盖本次样本。

本测量未覆盖 macOS、const、失败链计数、真实项目性能、cold-OS 或 alloc/free 差分。
生产、原始测量及仓内摘要/JSON 的独立窄核均无 finding，本地行为门禁有各自证据；
Draft PR 及 exact-head 双宿主 CI 尚待完成，不因本页完成而将 Spec 标为 done。


## 首轮发布与归档后继

以上测量与“Draft/CI尚待”保留为测量交付时的历史。后继本地 `ffcef525` 与 PR30 首轮
远端 `410ed04c94608798d66bdebd2b3e6423cdf2b2cf` 完整 tree 相同：
`192363b74005a9d40f660328b98ca4f65a2c61f4`；[该 head CI](https://github.com/Halckon/Koven/actions/runs/37054717054)
双宿主9/9 jobs实际成功。测量原值不变，0249按有界 Goal完成归档；
[归档账本](../archive/specs/0249-owned-unit-borrowed-handoff.md#8-首轮-exact-head-双宿主验收与归档2026-10-02)
记录逐名合同与非目标。归档文档新 head 的最终 CI 留 [PR30](https://github.com/Halckon/Koven/pull/30)核验，
不将首轮绿灯当作归档 head 已通过，也不将本片等同于整个治理计划完成。
