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
| `probes/*.ko` | 154 个独立小程序，每个都有 `fun main(): Unit`，成功时打印一行 |
| `baseline-aef3f4f8.tsv` | 154 个 probe 在 PR #120 head `aef3f4f8` 上的结果（来源见下文） |
| `programs/<name>/main.ko` | 六个验收程序（V1 目标） |
| `programs/<name>/expected.txt` | 对应程序 stdout 的精确期望 |
| `../scripts/conformance_matrix.sh` | 矩阵 runner |

## 运行

```sh
KOVENC=target/release/kovenc scripts/conformance_matrix.sh > matrix.tsv
diff conformance/baseline-aef3f4f8.tsv matrix.tsv
```

- 脚本可在任意目录执行，相对路径一律按仓库根解释。`KOVENC` 默认 `target/release/kovenc`；
  第一个参数可替换 probe 目录，默认 `conformance/probes`；`PROBE_TIMEOUT` 设置单次运行秒数，
  默认 60。
- 每个 probe 运行两次：单文件模式 `kovenc run <probe>.ko`，以及把源码放进
  `src/app/main.ko`（前置 `package app`）的单文件 project，以 `--entry app.main` 运行。
- 输出三列 TSV：名称、单文件结果、project 结果。结果文件放在仓库外或 `.local-artifacts/`，
  不提交；只有经确认的基线才以 `baseline-<commit>.tsv` 入库。

## 结果记号

| 记号 | 含义 |
|---|---|
| `OK` | 退出码 0 且有输出 |
| `OK(silent)` | 退出码 0 但没有任何输出 |
| `FE:Lxxxx` | 输出中第一个前端诊断码 |
| `BE:<kind>` | 后端 lowering 拒绝（`lowering failed with <kind>`） |
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
- 汇总：单文件与 project 各 89 个 `OK`，两模式结果逐行一致。

该基线在 Guide v0.46 启用前产生，运行的是下一节改动**之前**的 probe 源码；与改动后的源码
比较时，下一节列出的 probe 应按“期望”一列判读。

## v0.46 带来的 probe 变化

| Probe | 处理 | 原因 | 期望 |
|---|---|---|---|
| `p28_companion` | `companion object {` 改为 `companion {` | Guide v0.46 只接受匿名 `companion { }`，`companion object` 由 Parser 以 L0077 拒绝 | 关联函数调用成功 |
| `q12_companion_const` | 同上 | 同上 | 关联常量读取成功 |
| `p23_global_var` | 源码不变 | v0.46 顶层只允许 `const val`，顶层 `var counter` 由 Phase 2 以 L0167 拒绝（SPEC-0343） | 两模式均为 `FE:L0167` |
| `p16_result_q` | 删除自声明的 `enum class Result<T, E>`，其余不变 | v0.46 起 `Result`/`Ok`/`Err` 属于内建环境（SPEC-0349）；用户任一 package 的同名顶层声明以 L0150 拒绝 | 不再因声明冲突失败；`?` 仍待 N08 |
| `p16b_result_string` | 同上 | 同上 | 不再因声明冲突失败；String 载荷 lowering 另计 |
| `p37_is_generic_case` | 同上 | 同上；无实参 `is Result.Ok` 从 subject 推断 root 实参（SPEC-0349） | 前端不再报 L0111 |

以下 probe 未改动，但含义受 v0.46 影响，判读时留意：

- `p02_interp` 使用字符串插值；v0.46 已启用插值的类型规则（Guide 04），但 Guide 13 仍保留
  “native lowering 拒绝插值”的旧句，二者待 Guide 澄清。

## 验收程序（V1 目标）

| 目录 | 题目 | 覆盖面 |
|---|---|---|
| `word_count` | 词频 | `MutableMap` 插入序遍历、`String.clone()`、`in` 包含检查 |
| `config_parse` | 配置解析 | 逐字符解析 `key = value`、注释与空行、`Display` 实现、`Result` 与 `?` |
| `expr_eval` | 表达式求值 | 递归下降解析、`Box` 递归 enum、`Result` 传播与除零报错 |
| `raii_state_machine` | RAII 状态机 | `deinit` 资源、`replace`、词法作用域逆序析构、enum 状态迁移 |
| `varint_codec` | 二进制编解码 | ZigZag + LEB128、具名位运算、`UByte` 转换、截断与超长报错 |
| `linked_stack` | 链表/栈 | 拥有所有权的 class 节点、`replace` 接合、原地反转、`Box` cons 列表 |

这些程序按 Guide v0.46 编写，是 V1 的**目标**而不是当前实现能力：今天不能编译或运行并不算
失败，只说明对应能力尚未实现。每个程序不读输入、输出确定；验收以 stdout 与 `expected.txt`
逐字节相同、退出码 0 为准，V1/V3 另要求双宿主运行与 ASan/LSan 干净。现行 `String` 没有字符
访问 API，因此需要逐字符处理的程序把输入以 `Char` 字面量序列内嵌。
