# U0 迁入前硬门槛证据账本：AL1/AL2 前提映射与 UB1–UB11 捕获实测

> **性质**：非规范迁入前验证账本 · **状态**：未启用（进行中；部分实测已落盘，U0 合同仍留在 `docs/proposals/`，未迁入 `docs/compiler-specs/`） · **读取时机**：核对 [U0 合同 §9](unsafe-aliasing-contract-outline.md#9-验证与检测矩阵) 与 [OD5 收口条件](kernel-open-decisions-2026-11.md#5-2026-11-27-koven-design-三轮讨论收口纪要od3od6) 两条迁入硬门槛的当前完成度与剩余阻塞项时 · **唯一真源**：本页维护 AL1/AL2 向路线 L 参考检查器的附加前提映射、预 U1 阶段 C/消毒器代理反例实测记录，以及 UB1/UB5/W13 剩余缺口

本文不修改或启用任何 Guide 规则，不批准 Spec/ADR，不将 [U0 合同](unsafe-aliasing-contract-outline.md) 提前迁入 `docs/compiler-specs/`，也不修改 `crates/` 或 `scripts/loan_kernel/`。

## 1. 硬门槛一：AL1 / AL2 在路线 L 参考检查器中的附加前提映射

U0 §9 第一条硬门槛要求“路线 L 的参考检查器（[借用内核演算](loan-kernel-calculus.md)与[析构义务原型](drop-obligation-model.md)）能把 AL1、AL2 表述为对其模型的附加前提”。下表给出两条条款到现有演算规则（`K1`–`K26`、`drop_plan`）的精确映射，以及仍需在原型侧补齐的扩展点：

| U0 条款 | 形式化附加前提（表述为参考检查器约束） | 对应现有原型机制 | 当前状态与剩余缺口 |
|---|---|---|---|
| **AL1（不与存活 loan 冲突 + AL7 `RawCell` 例外）** | 对裸指针解引用事件 `raw_read(p)` / `raw_write(p)`（其逻辑来历绑定至目标 place `π` 与派生模式 `m ∈ {Shared, Exclusive, Cell}`）：① `raw_read(p)` 等价于在 `π` 上的一次瞬时 `Shared` 访问，要求 `π` 及所有前缀/后缀上不存在**其他**重叠存活的 `Exclusive` loan（对应 `K1`）；② `raw_write(p)` 在 `m = Exclusive` 时要求派生源祖先独占 loan 仍存活且不存在其他重叠存活的 `Shared`/`Exclusive` 子 loan（对应 `K1`/`K4`）；在 `m = Shared` 且 `π` 不含 `cell`（`RawCell`）时直接报 `UB1`；仅当 `m = Cell`（对应 `K19` 内部可变单元）允许经 `Shared` loan 写入 | `checker.py` 冲突矩阵（`K1`/`K4`）、内部可变单元标记（`cell` / `K19`）与借用派生树已存在 | **前提可表述，原型语句未接入**：`scripts/loan_kernel/` 尚未引入 `RawPtr` 值、`addressOf(π)` 派生模式标注与 `raw_read`/`raw_write` 事件；接入时须验证与现有 `K1`/`K4`/`K19` 无规则冲突 |
| **AL2（不晚于 owner 析构/释放 + `addressOf` 屏障 + `ptr.read()` 未初始化槽）** | 对 `addressOf(π)` 建立 place 级析构屏障：将 `π` 的根 owner 析构点从 ASAP（最后一次安全使用后）提升至**至少当前词法作用域结束**（`drop_barrier(root(π))`）；任何 `raw_read(p)`/`raw_write(p)` 必须发生在 `root(π)` 尚未执行 `drop` 且未被整体移出之前（否则 `UB2`）。若 `ptr_read(p)` 移出槽位值，将 `π` 的初始化状态置为 `Uninit`（允许随后 `ptr_write(p, v)` 恢复为 `Init`，但在 `Uninit` 状态下触发 `raw_read` 或隐式 `drop` 报 `UB5`） | `drop_plan` 的析构时机规划、条件移出/部分移出状态机（`D1`–`D5`）与 `replace` 槽位清账（`K26`）已存在 | **前提可表述，原型语句未接入**：需在 `drop_plan` 与 `checker.py` 中增加 `drop_barrier` 标记及 `Uninit` 槽位的 `ptr_read`/`ptr_write` 转移，以验证“屏障下不得提前析构”与“未回填槽位禁止析构/读取” |

## 2. 硬门槛二：UB1–UB6、UB9–UB11 预 U1 可执行反例与捕获实测

**证据边界声明**：现行 Koven v0.47 尚无用户可写的 `unsafe` / `RawPtr` / `extern "C"` 表面，因此本轮在 macOS AArch64 宿主上以 **C ABI / 消毒器代理反例**实跑 `clang` ASan、UBSan 与 TSan（临时目录编译运行后已清理），验证目标工具链能否稳定捕获对应的运行时未定义行为；待 U1 落地后须替换为 Koven 源码编译产物重跑。

| UB 编号 | 违反场景（对齐 U0 §6 / §8） | 实测捕获器与编译参数 | 实测结果（macOS AArch64） | 迁入门槛状态 |
|---|---|---|---|---|
| **UB1** | 持有存活 `Shared` loan 时经裸指针写非 `RawCell` 对象 | W13 参考解释器（借用栈 / Tree Borrows 风格） | 本轮未运行（macOS C ASan/UBSan 不追踪借用栈别名） | **未闭合（阻塞 U0 迁入）**：需 W13 参考解释器或 `loan_kernel` AL1 扩展实跑 |
| **UB2** | 释放后解引用（`free(p)` 后读 `*p`） | `clang -fsanitize=address -g -O0` | `exit=-6`；捕获 `ERROR: AddressSanitizer: heap-use-after-free` | **预 U1 代理已跑通**（U1 后换 Koven 源码复跑） |
| **UB3** | 有效性不变量破坏（向 `_Bool` 槽位写入字节 `2` 后按 `bool` 读取） | `clang -fsanitize=undefined -fno-sanitize-recover=all -g -O0` | `exit=-6`；捕获 `runtime error: load of value 2, which is not a valid value for type '_Bool'` | **预 U1 代理已跑通** |
| **UB4** | 同一析构/释放义务履行两次（双释放） | `clang -fsanitize=address -g -O0` | `exit=-6`；捕获 `ERROR: AddressSanitizer: attempting double-free` | **预 U1 代理已跑通**（`deinit` 计数双析构另由 W13 解释器覆盖） |
| **UB5** | 读取或析构未初始化存储（刚 `alloc` 或 `ptr.read()` 移出后未 `ptr.write`） | W13 参考解释器逐字节初始化位图 / Linux x86_64 MSan | 本轮未运行（macOS AArch64 不支持 LLVM MSan） | **未闭合（阻塞 U0 迁入）**：需 W13 初始化位图或 Linux x86_64 MSan 实跑 |
| **UB6（真实数据竞争）** | 双 `pthread` 线程无同步并发读写同一非原子 `int` | `clang -fsanitize=thread -pthread -g -O0`（`TSAN_OPTIONS=halt_on_error=1:exitcode=66`） | `exit=-6`；捕获 `WARNING: ThreadSanitizer: data race` / `SUMMARY: ThreadSanitizer: data race` | **预 U1 TSan 代理已跑通**（满足 OD5 TSan 数据竞争捕获要求） |
| **UB6（弱内存重排）** | `Relaxed`/`Acquire`/`Release` 弱内存序交错 | Loom / GenMC 风格内存模型并发检查器 | 本轮未运行（不与 TSan 互代） | **U1 验收欠项**（按 OD5 不挡 U0 迁入，单列跟踪） |
| **UB9（FFI2）** | C 函数留存 Koven 借用指针，返回并释放后写入 | `clang -fsanitize=address -g -O0` | `exit=-6`；捕获 `ERROR: AddressSanitizer: heap-use-after-free` | **预 U1 代理已跑通** |
| **UB9（FFI4）** | 非 NUL 结尾的 Koven `String` 字节缓冲直接传给 `strlen` | `clang -fsanitize=address -g -O0` | `exit=-6`；捕获 `ERROR: AddressSanitizer: heap-buffer-overflow` | **预 U1 代理已跑通** |
| **UB9（FFI5）** | Koven 回调在 C 栈帧内触发 `abort()`，验证进程直接终止不跨 C 帧展开 | `clang -g -O0` | `exit=-6`（`SIGABRT` 直接终止进程） | **预 U1 代理已跑通** |
| **UB10（越界）** | 堆数组越界读 `arr[4]`（分配 4 元素） | `clang -fsanitize=address -g -O0` | `exit=-6`；捕获 `ERROR: AddressSanitizer: heap-buffer-overflow` | **预 U1 代理已跑通** |
| **UB10（未对齐）** | 按 1 字节偏移后的未对齐地址读取 `long long`（要求 8 字节对齐） | `clang -fsanitize=undefined -fno-sanitize-recover=all -g -O0` | `exit=-6`；捕获 `runtime error: load of misaligned address ... for type 'long long', which requires 8 byte alignment` | **预 U1 代理已跑通** |
| **UB11** | 对分配块内部偏移指针 `p + 1` 调用 `free` | `clang -fsanitize=address -g -O0` | `exit=-6`；捕获 `ERROR: AddressSanitizer: attempting free on address which was not malloc()-ed` | **预 U1 代理已跑通** |

## 3. U0 迁入 `docs/compiler-specs/` 前剩余阻塞清单

按 OD5“不靠搬目录降低门槛”原则，[U0 合同](unsafe-aliasing-contract-outline.md) 目前仍保留在 `docs/proposals/`，待下列三项闭合后再迁入 `docs/compiler-specs/`：

1. **AL1 / AL2 原型接入**：在 `scripts/loan_kernel/` 补齐 `addressOf`（place 级析构屏障）、`raw_read`/`raw_write` 与 `ptr_read`/`ptr_write` 最小语句及正反例测试，实证 AL1/AL2 与现有 `K1`/`K4`/`K19`/`K26` 无冲突。
2. **UB1 与 UB5 捕获闭合**：通过 W13 参考解释器原型（借用栈 + 逐字节初始化位图 + 析构计数；UB5 可辅以 Linux x86_64 MSan）补齐 UB1 与 UB5 的可执行捕获记录。
3. **FFI1 / FFI3 与 Koven 源码复跑**：在 U1 / FF1 引入真实 `unsafe` 与 `extern "C"` 后，将上述 C 代理反例替换为 Koven 源码用例并在双宿主 CI 下复跑登记。

## 4. 验证与未运行项

- **已运行**：
  - macOS AArch64 下 `clang`（ASan / UBSan / TSan / `SIGABRT`）实跑 10 组预 U1 代理反例（`UB2`、`UB3`、`UB4`、`UB6` TSan、`UB9` FFI2/FFI4/FFI5、`UB10` 越界与未对齐、`UB11` 全部稳定捕获并退出 `-6`；临时目录已清理）。
  - `/opt/homebrew/bin/python3 scripts/check_docs.py && /opt/homebrew/bin/python3 -m unittest scripts.tests.test_check_docs`。
- **未运行**：未运行 UB1 借用栈解释器、UB5 MSan（macOS AArch64 不支持）或弱内存模型检查（Loom/GenMC）；未修改 `crates/`、`docs/guide/` 或 `scripts/loan_kernel/`；未运行 `cargo test`。
