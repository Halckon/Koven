# U0 迁入前硬门槛证据账本：AL1/AL2 前提映射与 UB1–UB11 捕获实测

> **性质**：非规范迁入前验证账本 · **状态**：未启用（预 U1 阶段双门槛原型与代理实测已闭合，U0 合同仍留在 `docs/proposals/`，待正式评审与 U1 源码复跑后再迁入 `docs/compiler-specs/`） · **读取时机**：核对 [U0 合同 §9](unsafe-aliasing-contract-outline.md#9-验证与检测矩阵) 与 [OD5 收口条件](kernel-open-decisions-2026-11.md#5-2026-11-27-koven-design-三轮讨论收口纪要od3od6) 两条迁入硬门槛的当前完成度与剩余阻塞项时 · **唯一真源**：本页维护 AL1/AL2 向路线 L 参考检查器的附加前提原型映射、预 U1 阶段参考解释器与 C/消毒器代理反例实测记录，以及迁入前剩余项

本文不修改或启用任何 Guide 规则，不批准 Spec/ADR，不将 [U0 合同](unsafe-aliasing-contract-outline.md) 提前迁入 `docs/compiler-specs/`，也不修改 `crates/`。

## 1. 硬门槛一：AL1 / AL2 在路线 L 参考检查器中的附加前提原型

U0 §9 第一条硬门槛要求“路线 L 的参考检查器能把 AL1、AL2 表述为对其模型的附加前提”。现已在 `scripts/loan_kernel/u0_raw_model.py` 补齐最小可执行参考解释器原型，并由 `scripts/tests/test_u0_raw_model.py`（7 条单测，随全套 146 条内核原型单测通过）验证下列附加前提：

| U0 条款 | 形式化附加前提（表述为参考检查器约束） | 对应原型模块与单测 | 当前状态 |
|---|---|---|---|
| **AL1（不与存活 loan 冲突 + AL7 `RawCell` 例外）** | 对裸指针解引用事件 `raw_read(p)` / `raw_write(p)`（逻辑来历绑定至 place `π` 与派生模式 `m ∈ {Shared, Exclusive, Cell}`）：① `raw_read(p)` 要求 `π` 及所有前缀/后缀上不存在**其他**重叠存活的 `Exclusive` loan（对应 `K1`）；② `raw_write(p)` 在 `m = Exclusive` 时要求派生源祖先独占 loan 仍存活且不存在其他重叠存活的 `Shared`/`Exclusive` 子 loan（对应 `K1`/`K4`）；在 `m = Shared` 且 `π` 不含 `RawCell` 时直接报 `UB1`；仅当 `RawCell`（`is_cell=True`，对应 `K19`）允许经 `Shared` loan 写入且仍受 `Exclusive` loan 约束 | `u0_raw_model.py`（`U0RawInterpreter`）+ `test_u0_raw_model.py`（`test_al1_*` 共 4 条） | **原型已落地并通过** |
| **AL2（不晚于 owner 析构/释放 + `addressOf` 屏障 + `ptr_read`/`ptr_write` 未初始化槽）** | `address_of(π)` 在根 owner 上设置 place 级析构屏障 `drop_barrier`（阻止提前析构并报 `AL2_BARRIER`，将其析构点提升至至少词法作用域结束）；owner 析构或整体移出后经悬垂 `RawPtr` 读写报 `UB2`；`ptr_read(p)` 将存活槽位置为 `Uninit`（非释放），允许 `ptr_write(p)` 恢复为 `Init`，但在 `Uninit` 状态下读取或析构报 `UB5` | `u0_raw_model.py` + `test_u0_raw_model.py`（`test_al2_*` 共 2 条） | **原型已落地并通过** |
| **AL3 / FS3 / FS8（析构义务恰好一次与失败注入）** | 在每个 FS1 操作处注入可展开失败并按词法逆序运行待执行析构，复现 U0 §4 列表中部插入的错误次序（先搬移制造重复句柄再 `clone()` 失败 → `UB4`）与正确次序（先 `clone()` 再仅用 FS4 搬移 → 零违规） | `u0_raw_model.py`（`simulate_fs8_insert_order`）+ `test_u0_raw_model.py`（`test_fs8_ub4_double_drop_and_safe_insert_order`） | **原型已落地并通过** |

## 2. 硬门槛二：UB1–UB6、UB9–UB11 预 U1 可执行反例与捕获实测

**证据边界声明**：现行 Koven v0.47 尚无用户可写的 `unsafe` / `RawPtr` / `extern "C"` 表面，因此本轮在 macOS AArch64 宿主上以 **`u0_raw_model.py` 参考解释器原型**（覆盖 `UB1`、`UB2`、`UB4`、`UB5`、`FS8`）与 **C ABI / `clang` 消毒器代理反例**（覆盖 `UB2`、`UB3`、`UB4`、`UB6` TSan、`UB9` FFI2/FFI4/FFI5、`UB10`、`UB11`，临时目录运行后已清理）完成预 U1 阶段双轨实测；待 U1 落地后须替换为 Koven 源码编译产物在双宿主 CI 下复跑。

| UB 编号 | 违反场景（对齐 U0 §6 / §8） | 实测捕获器与运行方式 | 实测结果（macOS AArch64） | 迁入门槛状态 |
|---|---|---|---|---|
| **UB1** | 持有存活 `Shared` loan 时经裸指针写非 `RawCell` 对象；或 `Exclusive` 派生写与存活子 loan 冲突；或 `RawCell` 与存活 `Exclusive` loan 冲突 | `scripts/loan_kernel/u0_raw_model.py`（`test_u0_raw_model.py`） | 4 条 AL1/AL7 单测全部通过，稳定捕获 `UB1` | **预 U1 原型已跑通**（U1 后接入 W13 解释器复跑） |
| **UB2** | 释放后解引用（`free(p)` 后读 `*p`）；以及 owner 析构/整体移出后经悬垂 `RawPtr` 读写 | `clang -fsanitize=address -g -O0` + `u0_raw_model.py` | ASan `exit=-6` 捕获 `heap-use-after-free`；原型单测捕获 `AL2_BARRIER` 与 `UB2` | **预 U1 代理+原型已跑通** |
| **UB3** | 有效性不变量破坏（向 `_Bool` 槽位写入字节 `2` 后按 `bool` 读取） | `clang -fsanitize=undefined -fno-sanitize-recover=all -g -O0` | `exit=-6`；捕获 `runtime error: load of value 2, which is not a valid value for type '_Bool'` | **预 U1 代理已跑通** |
| **UB4** | 同一析构/释放义务履行两次（双释放 + FS8 失败展开双析构） | `clang -fsanitize=address -g -O0` + `u0_raw_model.py` | ASan `exit=-6` 捕获 `attempting double-free`；原型 FS8 注入单测捕获 `UB4` | **预 U1 代理+原型已跑通** |
| **UB5** | 读取刚分配未初始化存储，或 `ptr_read` 移出后未 `ptr_write` 即读取/析构 | `scripts/loan_kernel/u0_raw_model.py`（逐槽位 `Init`/`Uninit` 状态机） | 原型单测通过，稳定捕获 3 处 `UB5` 且合法 `ptr_read -> ptr_write` 零违规（注：macOS AArch64 无 LLVM MSan） | **预 U1 原型已跑通**（U1 后在 Linux x86_64 补 MSan 交叉复跑） |
| **UB6（真实数据竞争）** | 双 `pthread` 线程无同步并发读写同一非原子 `int` | `clang -fsanitize=thread -pthread -g -O0`（`TSAN_OPTIONS=halt_on_error=1:exitcode=66`） | `exit=-6`；捕获 `WARNING: ThreadSanitizer: data race` / `SUMMARY: ThreadSanitizer: data race` | **预 U1 TSan 代理已跑通**（满足 OD5 TSan 数据竞争捕获要求） |
| **UB6（弱内存重排）** | `Relaxed`/`Acquire`/`Release` 弱内存序交错 | Loom / GenMC 风格内存模型并发检查器 | 本轮未运行（不与 TSan 互代） | **U1 验收欠项**（按 OD5 不挡 U0 迁入，单列跟踪） |
| **UB9（FFI2）** | C 函数留存 Koven 借用指针，返回并释放后写入 | `clang -fsanitize=address -g -O0` | `exit=-6`；捕获 `ERROR: AddressSanitizer: heap-use-after-free` | **预 U1 代理已跑通** |
| **UB9（FFI4）** | 非 NUL 结尾的 Koven `String` 字节缓冲直接传给 `strlen` | `clang -fsanitize=address -g -O0` | `exit=-6`；捕获 `ERROR: AddressSanitizer: heap-buffer-overflow` | **预 U1 代理已跑通** |
| **UB9（FFI5）** | Koven 回调在 C 栈帧内触发 `abort()`，验证进程直接终止不跨 C 帧展开 | `clang -g -O0` | `exit=-6`（`SIGABRT` 直接终止进程） | **预 U1 代理已跑通** |
| **UB10（越界）** | 堆数组越界读 `arr[4]`（分配 4 元素） | `clang -fsanitize=address -g -O0` | `exit=-6`；捕获 `ERROR: AddressSanitizer: heap-buffer-overflow` | **预 U1 代理已跑通** |
| **UB10（未对齐）** | 按 1 字节偏移后的未对齐地址读取 `long long`（要求 8 字节对齐） | `clang -fsanitize=undefined -fno-sanitize-recover=all -g -O0` | `exit=-6`；捕获 `runtime error: load of misaligned address ... for type 'long long', which requires 8 byte alignment` | **预 U1 代理已跑通** |
| **UB11** | 对分配块内部偏移指针 `p + 1` 调用 `free` | `clang -fsanitize=address -g -O0` | `exit=-6`；捕获 `ERROR: AddressSanitizer: attempting free on address which was not malloc()-ed` | **预 U1 代理已跑通** |

## 3. U0 迁入 `docs/compiler-specs/` 前剩余项

按 OD5“不靠搬目录降低门槛”原则，[U0 合同](unsafe-aliasing-contract-outline.md) 目前仍保留在 `docs/proposals/`。预 U1 阶段的 AL1/AL2 原型与 UB1–UB6/UB9–UB11 代理+原型实测已全部闭合，正式迁入 `docs/compiler-specs/` 并启动 U1 前仍需：

1. **正式评审签核**：由团队对 [U0 合同](unsafe-aliasing-contract-outline.md) 与本证据账本完成定稿评审，决定何时迁入 `docs/compiler-specs/`。
2. **U1 / FF1 源码复跑与 Linux MSan 交叉验证**：在 U1 / FF1 引入真实 `unsafe`、`RawPtr` 与 `extern "C"` 后，将上述 C 代理反例与原型用例替换为 Koven 源码用例（补齐 FFI1/FFI3），并在双宿主 CI（含 Linux x86_64 MSan）下复跑登记。

## 4. 验证与未运行项

- **已运行**：
  - `/opt/homebrew/bin/python3 -m unittest scripts.tests.test_u0_raw_model scripts.tests.test_loan_kernel scripts.tests.test_failure_model scripts.tests.test_capability scripts.tests.test_drop_plan scripts.tests.test_origin_accounting`：`Ran 146 tests ... OK`（含新增 7 条 U0 AL1/AL2/UB1/UB2/UB4/UB5/FS8 单测与既有 139 条内核原型单测）。
  - macOS AArch64 下 `clang`（ASan / UBSan / TSan / `SIGABRT`）实跑 10 组预 U1 代理反例全部按预期捕获并退出 `-6`（临时目录已清理）。
  - `/opt/homebrew/bin/python3 scripts/check_docs.py && /opt/homebrew/bin/python3 -m unittest scripts.tests.test_check_docs`。
- **未运行**：未运行 Linux x86_64 MSan 或弱内存模型检查（Loom/GenMC）；未修改 `crates/` 或 `docs/guide/`；未运行 `cargo test`。
