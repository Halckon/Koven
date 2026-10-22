# SPEC-0348: Conformance 种子集入库

> **性质**：实施合同 · **状态**：done · **读取时机**：维护 `conformance/` 种子集或矩阵 runner 时 · **唯一真源**：本片实施范围与验收；语义以 Guide 为准

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P0-0348` |
| 所属 Phase | Phase 6（工具：conformance 输入与矩阵 runner） |
| 语言规范 | [Guide v0.46 本版边界](../../guide/15-conformance-and-staging.md#v046-启用边界) |
| 批准依据 | 2026-10-22 用户批准 v1 路线图并要求按计划开始实施（路线图条目 V0） |
| 基线 | SPEC-0343 分支头 `176221de` |
| 分支 | `feature/spec-0348-conformance-corpus` |
| 前置 Spec | [SPEC-0343](0343-top-level-const-only.md)（仅分支顺序，无技术依赖） |
| 语言语义变更 | 否 |

## 1. Goal

把 review 使用的 154 个 probe 与六个验收程序的源码放进仓库，作为固定的 conformance 种子与
管线切换门槛；附带可复现的矩阵 runner 与一份有来源的基线结果。此前这些源码只在本地
`.local-artifacts`，SPEC-0341 因此只能用附件和场景描述做验收。

## 2. 范围

- `conformance/probes/*.ko`：154 个 probe 原样复制，仅按 Guide v0.46 把 `p28_companion`、
  `q12_companion_const` 的 `companion object {` 改为 `companion {`。`p23_global_var` 的顶层
  `var` 保留不变，按 v0.46 期望 L0167。
- `conformance/baseline-aef3f4f8.tsv`：PR #120 head `aef3f4f8` 上的矩阵结果原样副本；TSV 不加
  注释行，来源记在 `conformance/README.md`。
- `scripts/conformance_matrix.sh`：由 review 用的 `matrix.sh` 改写。默认在仓库根解释相对
  路径、遍历 `conformance/probes`，`KOVENC` 默认 `target/release/kovenc`，输出同样的三列
  TSV；新增 `abort:<message>`（退出码 134 且 stderr 首行以 `abort: ` 开头）与 `timeout`
  两个记号，其余记号不变。
- `conformance/programs/`：六个验收程序（词频、配置解析、表达式求值、RAII 状态机、二进制
  编解码、链表/栈），各含 `main.ko` 与精确 stdout 的 `expected.txt`，按 Guide v0.46 编写。
- `conformance/README.md`：用途、布局、运行方式、命名、结果记号、基线来源与 v0.46 改动清单。
- 登记：Spec 库存、归档索引与路线图 V0。

## 3. 非目标

- 不接入 CI，不把矩阵结果设为门禁（V2）。
- 不要求验收程序今天能编译或运行；它们是 V1 的目标。
- 不按 Guide 每章补齐正反例（V2）；不修改编译器、Guide 或其他 probe 的源码。

## 4. 验收标准

- [x] probe 共 154 个，与导出源逐字节相同，只有上述两个文件的 companion 写法不同。
- [x] 基线 TSV 154 行，与导出的 `matrix-aef3f4f8-154.tsv` 逐字节相同。
- [x] runner 可执行，`bash -n` 通过；用替身编译器覆盖 `OK`、`OK(silent)`、`abort:<message>`、
  `abort(exit N,no msg)`、`FE`、`BE`、`ERR` 各分支。
- [x] 六个验收程序各 40–120 行，不读输入，输出确定，只使用 Guide v0.46 已定义的 v1 能力。
- [x] 文档检查见 §5。

## 5. 实际验收（2026-10-22）

未运行 `cargo`（构建机由其他任务独占）；本片不改 Rust 代码。

| 检查 | 结果 |
|---|---|
| `diff -r` 导出 probe 与 `conformance/probes` | 仅 `p28_companion`、`q12_companion_const` 各一行不同 |
| `cmp` 导出基线与 `conformance/baseline-aef3f4f8.tsv` | 相同 |
| runner 替身测试（7 个分支 × 两模式） | 输出符合记号表 |
| runner 冒烟：本地旧构建 `kovenc`（2026-10-19，早于 PR #120 合并与 v0.46 Parser） | 154 行；单文件 67 个 `OK`、project 82 个 `OK`；companion 两例按旧 Parser 报 L0077，结果未入库 |
| `python3 scripts/gen_spec_dag.py`、`gen_roadmap.py`、`check_docs.py` | 通过 |

编写验收程序时发现的 Guide 留白（未自行调和，待用户决定）：

- Guide 13 的 `String` 封闭表面没有长度、字符访问或遍历，逐字符处理只能以 `Char` 序列内嵌输入；
  同节“native lowering 必须拒绝插值”与 Guide 04 已启用的插值规则冲突。
- 内建 `Ok`/`Err` 的 `is` 目标写法：Guide 09 示例用 `is Ok`，Guide 06 要求枚举外部写限定名；
  程序采用 `Ok(...)` 构造与 `is Ok` / `is Err` 匹配。
- 没有从 enum case 移出 MoveOnly payload 的通用形式（只有 `?` 的特例），状态机因此把资源句柄
  放在 class 字段而非 enum payload 中。

未运行：远端 CI、按新构建重跑矩阵、验收程序的编译与运行。
