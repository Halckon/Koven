# U0 迁入前硬门槛证据账本：AL1/AL2 前提映射与 UB1–UB11 捕获实测

> **性质**：非规范迁入前验证账本 · **状态**：未启用（AL1/AL2 已直接接入路线 L 主参考检查器 `checker.py` 与独立原型 `u0_raw_model.py`，C/消毒器代理套件均已固化入仓可复跑，待团队正式评审签核；U0 合同继续留在 `docs/proposals/`） · **读取时机**：核对 [U0 合同 §9](unsafe-aliasing-contract-outline.md#9-验证与检测矩阵) 与 [OD5 收口条件](kernel-open-decisions-2026-11.md#5-2026-11-27-koven-design-三轮讨论收口纪要od3od6) 两条迁入硬门槛的当前进展、待补前置与后续 U1/FF1 验收项时 · **唯一真源**：本页维护路线 L 主参考检查器（`checker.py` + `cases/Z48.lk`）与 AL1/AL2 独立参考解释器原型、预 U1 阶段可复跑 C/消毒器代理套件登记，以及 U0 迁入前待补项与 U1/FF1 阶段验收的分界

本文不修改或启用任何 Guide 规则，不批准 Spec/ADR，不将 [U0 合同](unsafe-aliasing-contract-outline.md) 提前迁入 `docs/compiler-specs/`，也不修改 `crates/`。

## 1. 硬门槛一：AL1 / AL2 附加前提已接入路线 L 主参考检查器与独立原型

U0 §9 第一条硬门槛要求“路线 L 的参考检查器能把 AL1、AL2 表述为对其模型的附加前提”。目前已在两个层级完成可执行建模与回归验证：
1. **路线 L 主参考检查器（`scripts/loan_kernel/checker.py` + `scripts/loan_kernel/cases/Z48.lk` + `scripts/tests/test_loan_kernel.py`）**：直接在主检查器中接入 `addressOf(&x)` / `addressOf(&mut x)` / `addressOf(p)`（在目标 owner 上建立 place 级析构屏障 `drop_barrier`，于 `drop_unneeded` / `settle` 中阻止 `early_drop` 提前析构，与 `DP.Verifier` 联合验证）及 `raw_read(p)` / `raw_write(p)`（按派生权限、`RawCell` AL7 例外、重叠存活 loan 与 owner 存活/移出/作用域析构/部分移出字段状态产出 `AL1` / `AL2` 诊断，并配有关闭 `drop_barrier` 的变异回归测试）。
2. **独立参考解释器原型（`scripts/loan_kernel/u0_raw_model.py` + `scripts/tests/test_u0_raw_model.py`，18 条单测）**：进一步覆盖分配实例 `alloc_id` 复用、完整借用派生历史 `loan_history`、聚合与嵌套字段双向 `ptr_read`/`ptr_write` 初始化状态机与 FS8 失败注入双析构（`UB4`）。

| U0 条款 | 形式化附加前提（表述为参考检查器约束） | 对应原型模块与单测 | 当前状态 |
|---|---|---|---|
| **AL1（不与存活 loan 冲突 + AL7 `RawCell` 例外）** | 对裸指针解引用事件 `raw_read(p)` / `raw_write(p)`：① 在 `checker.py` 中核对裸指针派生模式（`shared` vs `excl`）、`RawCell`（`cell class`，对应 `K19`）例外与 `live_loans(state)` 重叠冲突，报 `AL1`；② 在 `u0_raw_model.py` 中进一步校验 `alloc_id` 隔离、派生借用范围与完整 `loan_history` 祖先存活状态，报 `UB1` | `checker.py` + `cases/Z48.lk` + `test_loan_kernel.py`（`test_u0_al1_al2_premises_in_reference_checker`）；`u0_raw_model.py` + `test_u0_raw_model.py`（`test_al1_*` 共 10 条） | **已接入主参考检查器与独立原型，均可复跑** |
| **AL2（不晚于 owner 析构/释放 + `addressOf` 屏障 + `ptr_read`/`ptr_write` 未初始化槽）** | ① 在 `checker.py` 中 `addressOf(...)` 为目标 owner 设置 `drop_barrier=True`（阻止 `early_drop` 提前析构，经 `DP.Verifier` 与变异测试验证），owner 整体移出、离开内层作用域析构或字段部分移出后经 `raw_read`/`raw_write` 访问报 `AL2`；② 在 `u0_raw_model.py` 中进一步覆盖同名重分配 `alloc_id` 失效（`UB2`）、`ptr_read`/`ptr_write` 双向初始化状态机与析构义务归属（`UB5`） | `checker.py` + `cases/Z48.lk` + `test_loan_kernel.py`；`u0_raw_model.py` + `test_u0_raw_model.py`（`test_al2_*` 共 7 条） | **已接入主参考检查器与独立原型，均可复跑** |
| **AL3 / FS3 / FS8（析构义务恰好一次与失败注入）** | 在每个 FS1 操作处注入可展开失败并按词法逆序运行待执行析构，复现 U0 §4 列表中部插入的错误次序（先搬移制造重复句柄再 `clone()` 失败 → `UB4`）与正确次序（先 `clone()` 再仅用 FS4 搬移 → 零违规） | `u0_raw_model.py`（`simulate_fs8_insert_order`）+ `test_u0_raw_model.py`（`test_fs8_ub4_double_drop_and_safe_insert_order`） | **独立原型可复跑** |

## 2. 硬门槛二：UB1–UB6、UB9–UB11 预 U1 可复跑反例与捕获实测

**证据边界声明**：现行 Koven v0.47 尚无用户可写的 `unsafe` / `RawPtr` / `extern "C"` 表面。本表登记的两套预 U1 证据均已**固化入仓且可一键复跑**：
1. **`scripts/loan_kernel/checker.py`（`cases/Z48.lk`）与 `scripts/loan_kernel/u0_raw_model.py` + `scripts/tests/test_u0_raw_model.py`**（18 条单测）：覆盖 `AL1`/`AL2`/`AL7`、`UB1`、`UB2`、`UB4`、`UB5`、`FS8`。
2. **`scripts/loan_kernel/u0_sanitizer_harness.py` + `scripts/tests/test_u0_sanitizer_harness.py`**（10 组 C ABI / `clang` ASan/UBSan/TSan/终止基线用例；缺少 `clang` 时自动跳过）：覆盖 `UB2`、`UB3`、`UB4`、`UB6`（TSan 数据竞争）、`UB9`（FFI2/FFI4 及 FFI5 终止基线对照）、`UB10`（越界/未对齐）与 `UB11`，一键复跑命令为 `python3 -m unittest scripts.tests.test_u0_sanitizer_harness`（或直接执行 `python3 scripts/loan_kernel/u0_sanitizer_harness.py`）。

| UB 编号 | 违反场景（对齐 U0 §6 / §8） | 捕获器与仓库可复跑入口 | 实测结果（macOS AArch64） | 阶段验收状态 |
|---|---|---|---|---|
| **UB1** | 持有存活 `Shared` loan 时经裸指针写非 `RawCell` 对象；`Shared` 派生链（含父借用已结束）提权写；`Exclusive` 派生写与存活子 loan 冲突；`RawCell`（`Shared`/`Cell` 路径）与存活 `Exclusive` loan 冲突 | `checker.py`（`cases/Z48.lk`）+ `u0_raw_model.py` / `test_u0_raw_model.py` | 主参考检查器 `Z48.lk` 捕获 `AL1`；独立原型 10 条 AL1/AL7 单测全部通过，稳定捕获 `UB1` | **预 U1 主检查器+原型可复跑**（U1 后接入 W13 解释器复跑） |
| **UB2** | 释放后解引用（`free(p)` 后读 `*p`）；以及 owner 析构、整体移出或同名重分配后经悬垂 `RawPtr` 读写 | `u0_sanitizer_harness.py`（`UB2_uaf_asan`）+ `checker.py`（`cases/Z48.lk`）+ `u0_raw_model.py` | ASan `exit=-6` 捕获 `heap-use-after-free`；主检查器与原型捕获 `drop_barrier`/`AL2`/`UB2` | **预 U1 代理+主检查器+原型可复跑** |
| **UB3** | 有效性不变量破坏（向 `_Bool` 槽位写入字节 `2` 后按 `bool` 读取） | `u0_sanitizer_harness.py`（`UB3_invalid_bool_ubsan`） | UBSan `exit=-6` 捕获 `load of value 2, which is not a valid value for type '_Bool'` | **预 U1 代理可复跑** |
| **UB4** | 同一析构/释放义务履行两次（双释放 + FS8 失败展开双析构） | `u0_sanitizer_harness.py`（`UB4_double_free_asan`）+ `u0_raw_model.py` | ASan `exit=-6` 捕获 `attempting double-free`；原型 FS8 注入单测捕获 `UB4` | **预 U1 代理+原型可复跑** |
| **UB5** | 读取刚分配未初始化存储、`ptr_read` 移出后未 `ptr_write` 即读取/析构，以及聚合根与子字段的双向未初始化访问 | `u0_raw_model.py` / `test_u0_raw_model.py` | 原型单测通过，稳定捕获单槽与聚合双向 `UB5`，合法 `ptr_read -> ptr_write` 零违规（注：macOS AArch64 无 LLVM MSan） | **预 U1 原型可复跑**（U1 后在 Linux x86_64 补 MSan 交叉复跑） |
| **UB6（真实数据竞争）** | 双 `pthread` 线程无同步并发读写同一非原子 `int` | `u0_sanitizer_harness.py`（`UB6_data_race_tsan`） | TSan `exit=-6` 捕获 `ThreadSanitizer: data race` | **预 U1 TSan 代理可复跑**（满足 OD5 TSan 数据竞争捕获要求） |
| **UB6（弱内存重排）** | `Relaxed`/`Acquire`/`Release` 弱内存序交错 | Loom / GenMC 风格内存模型并发检查器 | 本轮未运行（不与 TSan 互代） | **U1 验收欠项**（按 OD5 不挡 U0 迁入，单列跟踪） |
| **UB9（FFI2）** | C 函数留存 Koven 借用指针，返回并释放后写入 | `u0_sanitizer_harness.py`（`UB9_ffi2_retained_borrow_asan`） | ASan `exit=-6` 捕获 `heap-use-after-free` | **预 U1 代理可复跑** |
| **UB9（FFI4）** | 非 NUL 结尾的 Koven `String` 字节缓冲直接传给 `strlen` | `u0_sanitizer_harness.py`（`UB9_ffi4_non_nul_string_asan`） | ASan `exit=-6` 捕获 `heap-buffer-overflow` | **预 U1 代理可复跑** |
| **UB9（FFI5）** | Koven 回调在 C 栈帧内触发失败时不得跨 C 帧展开，须直接终止进程 | `u0_sanitizer_harness.py`（`UB9_ffi5_abort_termination_baseline`） | 仅观测到 `exit=-6`（`SIGABRT`）进程终止基线；**未构造跨 C 帧展开路径或阻断展开的边界逻辑** | **基线对照可复跑，FFI5 边界捕获待验**（留给 FD2/FF1 具备展开路径后的边界测试） |
| **UB10（越界）** | 堆数组越界读 `arr[4]`（分配 4 元素） | `u0_sanitizer_harness.py`（`UB10_oob_asan`） | ASan `exit=-6` 捕获 `heap-buffer-overflow` | **预 U1 代理可复跑** |
| **UB10（未对齐）** | 按 1 字节偏移后的未对齐地址读取 `long long`（要求 8 字节对齐） | `u0_sanitizer_harness.py`（`UB10_misalign_ubsan`） | UBSan `exit=-6` 捕获 `load of misaligned address` | **预 U1 代理可复跑** |
| **UB11** | 对分配块内部偏移指针 `p + 1` 调用 `free` | `u0_sanitizer_harness.py`（`UB11_interior_dealloc_asan`） | ASan `exit=-6` 捕获 `attempting free on address which was not malloc()-ed` | **预 U1 代理可复跑** |

## 3. U0 迁入前待补项与 U1 / FF1 阶段验收的分界

为避免“U0 迁入要求先跑真实 Koven `unsafe` 源码，而启动 U1 又要求 U0 先迁入”的循环前置，同时如实反映当前状态，明确区分两个阶段：

1. **U0 迁入 `docs/compiler-specs/` 前仍待完成的前置**：
   - **团队正式定稿评审与签核**：硬门槛一（`checker.py` + `cases/Z48.lk` 与 `u0_raw_model.py`）与硬门槛二预 U1 代理套件（`u0_sanitizer_harness.py`）均已固化入仓可复跑；由团队正式评审并决定何时将 [U0 合同](unsafe-aliasing-contract-outline.md) 从 `docs/proposals/` 迁入 `docs/compiler-specs/`。
2. **U0 迁入并启动 U1 / FF1 / FD2 之后的阶段验收项（不阻塞 U0 迁入）**：
   - **真实 Koven 源码复跑**：待 U1 / FF1 引入 `unsafe`、`RawPtr` 与 `extern "C"` 后，将 §2 的 C 代理与原型用例替换为 Koven 源码用例（补齐 FFI1/FFI3），在双宿主 CI（含 Linux x86_64 MSan）下复跑登记；
   - **FFI5 跨 C 帧阻断展开边界测试**：待 FD2/FF1 引入可展开路径后，构造回调内普通失败并在 C 边界验证阻断展开、直接终止进程；
   - **UB6 弱内存模型检查**：Loom / GenMC 级弱内存序检查（按 OD5 单列为 U1 验收欠项，不与 TSan 互代）。

## 4. 验证与未运行项

- **已运行**：
  - `/opt/homebrew/bin/python3 -m unittest scripts.tests.test_loan_kernel scripts.tests.test_failure_model scripts.tests.test_u0_raw_model scripts.tests.test_u0_sanitizer_harness scripts.tests.test_f5_od4_grammar_probe`（及 `/usr/bin/python3` Python 3.9 兼容复跑）。
  - `/opt/homebrew/bin/python3 scripts/check_docs.py && /opt/homebrew/bin/python3 -m unittest scripts.tests.test_check_docs`。
- **未运行**：未运行 Linux x86_64 MSan、FFI5 跨 C 帧阻断展开测试或弱内存模型检查（Loom/GenMC）；未修改 `crates/` 或 `docs/guide/`；未运行 `cargo test`。
