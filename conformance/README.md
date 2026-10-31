# Conformance 种子集

本目录是 Koven v1 的固定 conformance 种子：154 个 review probe 与六个验收程序的源码。
它承担两件事：

- **回归基准**：每次 review 或管线改动都在同一套 probe 上运行，结果与已记录的基线逐行比较；
- **管线切换门槛**：单文件与 project 两种模式在同一 probe 上的结果必须一致，新管线不得让
  基线中已经通过的 probe 退化（见 `docs/roadmap/v1.json` 的 E2、V0、V2）。

语言语义以 [Guide v0.46](../docs/guide/README.md) 为准；本目录只保存输入与结果，不定义语义。

## 布局

| 路径 | 内容 |
|---|---|
| `probes/*.ko` | 154 个独立小程序，每个都有 `fun main(): Unit`，成功时打印明确输出 |
| `expectations.json` | 154 个 probe 与六个验收程序的机器可读期望（强 oracle、`oracle_origin` 溯源标注、`all_diagnostics` 多诊断码列表、源文件 SHA-256 与 v0.46 基线豁免标注） |
| `baseline-aef3f4f8.tsv` | 154 个 probe 在 PR #120 head `aef3f4f8` 上的结果（来源见下文） |
| `programs/<name>/main.ko` | 六个验收程序（V1 目标） |
| `programs/<name>/expected.txt` | 对应程序 stdout 的精确期望 |
| `../scripts/conformance_matrix.sh` | 矩阵 runner（支持 TSV 模式与 `--gate` 门禁转发） |
| `../scripts/conformance_gate.py` | 公开 CLI 入口、Markdown 审计账本渲染与门面导出（`1000` 行软上限） |
| `../scripts/conformance_catalogs.py` | 静态规范目录、诊断码/记号注册表与章节/批次分类规则（`1000` 行软上限） |
| `../scripts/conformance_expectations.py` | `expectations.json` 模式校验、静态目录不变量校验与期望推导（`1000` 行软上限） |
| `../scripts/conformance_runner.py` | 双模式执行器（`single`/`project`）、逐字节 Oracle 比较器与套件评估（`1000` 行软上限） |
| `../scripts/conformance_evidence.py` | 离线证据包防篡改校验、双宿主一致性比对与 TSV 矩阵核查（`1000` 行软上限） |
| `../scripts/conformance_audit.py` | 章节极性、P2 门槛、Litmus、路线图阶段与 V3 Sanitizer 覆盖聚合（`1000` 行软上限） |
| `../scripts/tests/test_conformance_gate.py` / `test_conformance_audit.py` | 门禁执行与对账审计自测（含 `1000` 行单一职责软门禁守卫） |

## 运行

### 1. 强 oracle 回归验收门禁（推荐）

```sh
scripts/conformance_matrix.sh --gate \
  --cli target/release/kovenc \
  --baseline conformance/baseline-aef3f4f8.tsv \
  --evidence .local-artifacts/conformance-gate
```

也可以按单个或若干个 probe、批次前缀（`--batch`）、Guide 章节（`--chapter`，支持 `01`..`13` 简写）或验收程序（`--program`）局部采样，并通过 `--jobs <N>`（环境变量 `KOVEN_CONFORMANCE_JOBS`）控制确定性并发度：

```sh
python3 scripts/conformance_gate.py \
  --cli target/release/kovenc \
  --probe p04_overflow --probe p28_companion --no-programs

python3 scripts/conformance_gate.py \
  --cli target/release/kovenc \
  --batch e --chapter 06 --no-programs

# 离线校验单个证据包或比对 macOS / Linux 双宿主证据包一致性
python3 scripts/conformance_gate.py \
  --verify-evidence .local-artifacts/conformance-gate \
  --baseline conformance/baseline-aef3f4f8.tsv

python3 scripts/conformance_gate.py \
  --compare-evidence .local-artifacts/conformance-macos .local-artifacts/conformance-linux \
  --baseline conformance/baseline-aef3f4f8.tsv

# 离线输出语料库强 Oracle、P2 门槛、V2 章节就绪度与 V3 Sanitizer 覆盖 JSON 摘要
python3 scripts/conformance_gate.py --audit-summary
```

在 GitHub Actions CI（`.github/workflows/ci.yml`）中，`docs` 任务自动通过 `scripts/check_docs.py` 校验 `conformance/expectations.json` 与 `conformance/baseline-aef3f4f8.tsv` 的源码/期望 SHA-256 及条目完整性；`test` 任务在 `macos-15` 与 `ubuntu-24.04` 双宿主上自动执行 `scripts/conformance_gate.py --baseline conformance/baseline-aef3f4f8.tsv` 并归档 `conformance-<os>-<sha>` 证据包。

门禁行为与产物（`--evidence <dir>`）：

- **分离保存原始字节**：每个目标在单文件（`kovenc run <file>.ko`）和单文件 project（自动前置 `package app\n` 写入 `src/app/main.ko`，以 `kovenc run --project project.toml --entry app.main` 运行）两种模式下，分别将 `stdout` 与 `stderr` 原始字节写入 `raw/<name>.<mode>.stdout` / `.stderr`，不合并流、不截断尾换行，并在 `.status.json` 记录退出码与 `timed_out` 状态。
- **元数据与摘要绑定**：`results.json` 绑定编译器二进制路径、SHA-256、Git 提交、宿主平台（`system`/`machine`）、原始源码 SHA-256，以及 project 模式显式变换记录（`prepend_package_header` 头字节、SHA-256 与变换后源码 SHA-256）；同时自动生成可读的确定性执行摘要 `SUMMARY.md`（由 `--verify-evidence` 一并校验防篡改）。
- **比较器状态分类**：
  - `pass_run`：退出码 0、stderr 为空、`stdout` 与 `expectations.json`（或验收程序的 `expected.txt`）逐字节相等（含尾换行），计入能力通过。
  - `pass_diagnostic`：非零退出、stdout 为空、未发生 `ICE`/`BE` 后端错误，`stderr` 行首 `error[Lxxxx]` 序列精确匹配期望的诊断码列表（默认 `[diagnostic]`，多诊断探针如 `p25_linked_iter` 由 `all_diagnostics` 显式列出），计入能力通过。
  - `pass_abort`：SIGABRT（退出码 134）、stdout 为空、stderr 首行精确匹配 `abort: <message>`，计入能力通过。
  - `known_unimplemented`：两模式一致命中允许的未实现记号（如 `BE:UnsupportedNode`、`BE:UnsupportedSource`、`BE:MissingFact`、`FE:L0165`），门禁放行但**绝不计入能力通过**。
  - `unreviewed`：尚未人工确认强 oracle 的条目；严格门禁下直接阻断，禁止把当前编译器输出自动录成正确答案。
  - `wrong_output` / `mode_mismatch` / `regression` / `internal_error` / `unexpected_pass`：任何退出码 0 但输出不符、缺尾换行、两模式记号或字节不一致、期望通过或历史基线 `OK` 退化为失败（含“两模式都拒绝”）、或出现 `BE:InvalidSsa` / `ICE:ownership` / `timeout` / `ERR`，门禁一律以非零状态失败。

### 2. 三列 TSV 矩阵兼容模式

```sh
KOVENC=target/release/kovenc scripts/conformance_matrix.sh > matrix.tsv
```

- 脚本可在任意目录执行，相对路径一律按仓库根解释。`KOVENC` 默认 `target/release/kovenc`；
  第一个参数可替换 probe 目录，默认 `conformance/probes`；`PROBE_TIMEOUT` 设置单次运行秒数，
  默认 60；若设置 `KOVEN_CONFORMANCE_EVIDENCE=<dir>` 则同时落盘保存原始 `.stdout`/`.stderr`/`.status.json`、`matrix.tsv` 与 `manifest.json`。
- 输出三列 TSV：名称、单文件结果、project 结果。结果文件放在仓库外或 `.local-artifacts/`，
  不提交；只有经确认的基线才以 `baseline-<commit>.tsv` 入库。可通过 `python3 scripts/conformance_gate.py --check-matrix matrix.tsv --baseline conformance/baseline-aef3f4f8.tsv` 离线比对任意三列矩阵与 `expectations.json` / 历史基线。

## 结果记号

| 记号 | 含义 |
|---|---|
| `OK` | 退出码 0 且有输出 |
| `OK(silent)` | 退出码 0 但没有任何输出 |
| `FE:Lxxxx` | 未发生 `ICE`/`BE` 错误时，`stderr` 行首 `error[Lxxxx]` 的首个前端诊断码 |
| `BE:<kind>` | 后端 lowering 或 native object 拒绝（`lowering failed with <kind>` 或 `native object <kind>:`，优先级高于 `FE:Lxxxx`） |
| `ICE:ownership` | 所有权检查内部错误泄漏 |
| `abort:<message>` | 程序以 SIGABRT 终止（退出码 134），stderr 第一行为 `abort: <message>`；按 [Abort 输出合同](../docs/guide/09-nullability-errors.md#abort-输出合同) 记录消息文本 |
| `abort(exit N,no msg)` | 非零退出且没有任何输出 |
| `timeout` | 超过 `PROBE_TIMEOUT` |
| `ERR` | 其他失败 |

`abort:<message>` 是本次新增的记号；旧基线产生时 runtime 尚未实现 abort 输出合同，因此基线中
的 abort 都记作 `abort(exit 1,no msg)`。

## Probe 命名

名称为 `<前缀><序号>_<主题>`（`w12`–`w19` 只有序号）。前缀表示 probe 加入的批次与关注面：

| 前缀 | 关注面 |
|---|---|
| `p` | 基础能力：标量、控制流、class/enum/value class、nullable、Result、闭包、容器、所有权等 |
| `q` | 对 `p` 中失败项的变体复测，缩小失败条件 |
| `r` | 未知成员访问，期望前端诊断 |
| `s` | 标准环境：内建 `Result`、`Pair`/`to` 与 `?` 的边界 |
| `t` | SPEC-0311–0318 的定向复测：value class 方法、Map/List 遍历中的跳转、短路求值 |
| `u` | enum case 作借用实参、循环中 `MutableList.add` 等增量复测 |
| `v` | v0.44 补全面：赋值结果、整数 Range、显式整数转换、nullable 循环、Map consume |
| `w` | Map/MutableMap：遍历顺序、覆盖与删除、遍历中的跳转与借用 |
| `x` | nullable class、String nullable 返回与条件中的借用实参复测 |
| `y` | `koven.algorithms` 的 `take` 与 N1a 范围借用 |
| `e` | enum subject `when` 在 compilation-unit 管线中的回退（E2 门槛） |

## 基线来源

`baseline-aef3f4f8.tsv` 是 review 矩阵 `matrix-aef3f4f8-154.tsv` 的原样副本：

- 编译器：PR #120（SPEC-0341，统一 compilation-unit 管线）head `aef3f4f8`；
- 环境：Linux x86_64，LLVM 21.1.8；
- 日期：2026-10-22；
- 汇总：单文件与 project 各 89 个 `OK`（另 3 个无消息 abort），两模式结果逐行一致。

该基线在 Guide v0.46 启用前产生，运行的是下一节改动**之前**的 probe 源码；不能把 `baseline-aef3f4f8.tsv` 与 Guide v0.46 修改后的源码直接做裸 `diff` 混比。`expectations.json` 中已对下节条目及后续合入能力标记 `v046_baseline_exempt: true`，由 `scripts/conformance_gate.py` 按强 oracle 判读。

## v0.46 带来的 probe 变化

| Probe | 处理 | 原因 | 期望 |
|---|---|---|---|
| `p28_companion` | `companion object {` 改为 `companion {` | Guide v0.46 只接受匿名 `companion { }`，`companion object` 由 Parser 以 L0077 拒绝 | 关联函数调用（当前 `BE:UnsupportedSource`，待 N06） |
| `q12_companion_const` | 同上 | 同上 | 关联常量读取成功（`cconst\n`） |
| `p23_global_var` | 源码不变 | v0.46 顶层只允许 `const val`，顶层 `var counter` 由 Phase 2 以 L0167 拒绝（SPEC-0343） | 两模式均为 `FE:L0167` |
| `p04_overflow` / `p45_abort_message` / `v12_convert_abort` | 源码不变 | v0.46 D12（SPEC-0355）实现 Abort 输出合同 | 两模式分别输出 `abort:integer overflow in +`、`abort:boom`、`abort:value out of range for ULong` |
| `p16_result_q` | 删除自声明的 `enum class Result<T, E>`，其余不变 | v0.46 起 `Result`/`Ok`/`Err` 属于内建环境（SPEC-0349）；用户任一 package 的同名顶层声明以 L0150 拒绝 | 不再因声明冲突失败；`?` 仍待 N08 |
| `p16b_result_string` | 同上 | 同上 | 不再因声明冲突失败；String 载荷 `when` 匹配由 SPEC-0368 闭环，运行输出 `good\n` |
| `p37_is_generic_case` | 同上 | 同上；无实参 `is Result.Ok` 从 subject 推断 root 实参（SPEC-0349） | 前端不再报 L0111，运行输出 `case-noargs\n` |

以下 probe 未改动，但含义受 v0.46 影响，判读时留意：

- `p02_interp` 使用字符串插值；v0.46 已启用插值的类型规则（Guide 04），但 Guide 13 仍保留
  “native lowering 拒绝插值”的旧句，二者待 Guide 澄清。

## 验收程序（V1 目标）

| 目录 | 题目 | 覆盖面 | 归属章节 | 当前状态 | 前置 Probe 就绪 | 路线图阻塞项 |
|---|---|---|---|---|---:|---|
| `word_count` | 词频 | `MutableMap` 插入序遍历、`String.clone()`、`in` 包含检查 | [12-collections-destructuring.md](../docs/guide/12-collections-destructuring.md) | `known_unimplemented` | 5 / 7 | `N02`, `N03` |
| `config_parse` | 配置解析 | 逐字符解析 `key = value`、注释与空行、`Display` 实现、`Result` 与 `?` | [09-nullability-errors.md](../docs/guide/09-nullability-errors.md) | `known_unimplemented` | 4 / 7 | `N02`, `N08`, `L1`, `L2` |
| `expr_eval` | 表达式求值 | 递归下降解析、`Box` 递归 enum、`Result` 传播与除零报错 | [11-copyability-layout-construction.md](../docs/guide/11-copyability-layout-construction.md) | `known_unimplemented` | 3 / 8 | `N02`, `N05`, `N07`, `N08`, `L7` |
| `raii_state_machine` | RAII 状态机 | `deinit` 资源、`replace`、词法作用域逆序析构、enum 状态迁移 | [08-class-family-members.md](../docs/guide/08-class-family-members.md) | `known_unimplemented` | 5 / 7 | `N01`, `N02` |
| `varint_codec` | 二进制编解码 | ZigZag + LEB128、具名位运算、`UByte` 转换、截断与超长报错 | [03-types-generics.md](../docs/guide/03-types-generics.md) | `known_unimplemented` | 5 / 7 | `N01`, `N08`, `L3b` |
| `linked_stack` | 链表/栈 | 拥有所有权的 class 节点、`replace` 接合、原地反转、`Box` cons 列表 | [10-ownership-borrowing-drop.md](../docs/guide/10-ownership-borrowing-drop.md) | `known_unimplemented` | 4 / 7 | `N01`, `N03`, `N07`, `L7` |

这些程序按 Guide v0.46 编写，是 V1 的**目标**而不是当前实现能力：今天不能编译或运行并不算
失败，只说明对应能力尚未实现。每个程序不读输入、输出确定；验收以 stdout 与 `expected.txt`
逐字节相同、退出码 0 为准，V1/V3 另要求双宿主运行与 ASan/LSan 干净。现行 `String` 没有字符
访问 API，因此需要逐字符处理的程序把输入以 `Char` 字面量序列内嵌。

## Guide v0.46 章节覆盖矩阵与语义推导对账（V2）

`expectations.json` 中每个 probe 与验收程序均通过 `guide_chapter` 绑定到对应的 Guide v0.46 规范章节，并由 `oracle_derivation` 记录逐条语义推导、前端诊断码 `crates/lang-frontend/src/diagnostic.rs` 常量或 Guide 09 abort 合同出处。完整 154 个 probe 与 6 个验收程序的逐项推导表见自动生成并受 `scripts/check_docs.py` 门禁保护的 [ORACLE_AUDIT.md](ORACLE_AUDIT.md)：

| Guide v0.46 章节 | Probe 总数 | `run_ok` | `compile_fail` | `runtime_abort` | `known_unimplemented` | 能力通过 (`capability_passed`) |
|---|---:|---:|---:|---:|---:|---:|
| [01-lexical.md](../docs/guide/01-lexical.md) | 4 | 1 | 0 | 0 | 3 | 1 / 4 |
| [02-names-files-packages.md](../docs/guide/02-names-files-packages.md) | 7 | 2 | 3 | 0 | 2 | 5 / 7 |
| [03-types-generics.md](../docs/guide/03-types-generics.md) | 11 | 6 | 1 | 1 | 3 | 8 / 11 |
| [04-expressions-operators.md](../docs/guide/04-expressions-operators.md) | 5 | 3 | 1 | 1 | 0 | 5 / 5 |
| [05-declarations-callables.md](../docs/guide/05-declarations-callables.md) | 4 | 2 | 2 | 0 | 0 | 4 / 4 |
| [06-blocks-control-flow.md](../docs/guide/06-blocks-control-flow.md) | 19 | 16 | 2 | 0 | 1 | 18 / 19 |
| [07-calls-lambdas-closures.md](../docs/guide/07-calls-lambdas-closures.md) | 8 | 6 | 1 | 0 | 1 | 7 / 8 |
| [08-class-family-members.md](../docs/guide/08-class-family-members.md) | 16 | 15 | 0 | 0 | 1 | 15 / 16 |
| [09-nullability-errors.md](../docs/guide/09-nullability-errors.md) | 16 | 12 | 1 | 1 | 2 | 14 / 16 |
| [10-ownership-borrowing-drop.md](../docs/guide/10-ownership-borrowing-drop.md) | 9 | 6 | 2 | 0 | 1 | 8 / 9 |
| [11-copyability-layout-construction.md](../docs/guide/11-copyability-layout-construction.md) | 3 | 2 | 0 | 0 | 1 | 2 / 3 |
| [12-collections-destructuring.md](../docs/guide/12-collections-destructuring.md) | 48 | 47 | 1 | 0 | 0 | 48 / 48 |
| [13-program-runtime-standard-library.md](../docs/guide/13-program-runtime-standard-library.md) | 4 | 3 | 0 | 0 | 1 | 3 / 4 |
| **合计** | **154** | **121** | **14** | **3** | **16** | **138 / 154** |


