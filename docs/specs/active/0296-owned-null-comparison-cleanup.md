# SPEC-0296: owned nullable 最后判空与循环清理

> **性质**：有界变更合同 · **状态**：in-progress · **读取时机**：实施或验收 owned nullable 判空 cleanup 修复时 · **唯一真源**：本页

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P4-296` |
| 所属 Phase | Phase 3 既有 drop 事实交接、Phase 4 SSA/native |
| 语言规范 | [Guide v0.43](../../guide/README.md)、[所有权与双轨析构](../../guide/10-ownership-borrowing-drop.md)、[空安全](../../guide/09-nullability-errors.md) |
| 批准依据 | 用户要求继续下一里程碑；本片修复既有 MoveOnly Map.remove 普通源码缺口，本地实现与验证已授权，发布另由父任务确认 |
| 前置 Spec | SPEC-0288、SPEC-0295 |
| 前置 ADR | ADR-0017 |
| 影响范围 | 通用 nullable comparison 的两入口 lowering、普通源码回归、事实文档 |
| 语言语义变更 | 否 |

## 1. Goal 与基线

从 main `e168c485a5379764cf927433452f8b7059f7b334` 新建 `fix/spec-0296`，
修复具名 owned nullable 判空最后使用时提前消费 owner 的交接，使 MoveOnly Map.remove
结果在普通源码循环与非循环中按既有合同判空、借用或消费，并生成通过 verifier 的 native。
0295 [原始边界源码与失败](../../development/evidence/map-word-frequency-0295/known-boundary.json)
保留历史，不重写其当时结论。修复不得按 Map/String 名称、源码形状或循环特判。

## 2. 合同与范围

- presence 读取必须先于该读取对应的最后使用 drop；沿用前端 typed null descriptor、
  source-qualified ownership/drop facts 和已有 aggregate/pointer 表示，不重新猜测 liveness。
- 纯内存 MoveOnly 遵循现行 ASAP；资源类型遵循词法退出逆序析构。消费交付后原绑定不能重复 drop。
- 单文件、普通 unit 与常量 unit 使用同一不变量；循环回边保留必要 owner/loan，
  分支出口、判空、分组/交换操作数、消费及缺失结果均有正常源码覆盖。
- 先重放原项目并补最小红测，区分 for 根因与一般最后判空；任何更广问题单独记录。

## 3. 非目标

Guide/Map API/nullable 语义修改、Map 迭代、consume、新布局或新借用能力、LLVM/runtime 改写、
完整 CFG/NLL 重构、直接 temporary 二元判空的新前端清理事实、放松 verifier、性能宣称。云本机只跑正常源码、普通负例与静态检查；
不跑故障注入、校准、malformed IR、LLVM/allocator counter、reducer 或聚合驱动。双宿主 CI 留远端。

## 4. 验收账本

| ID | 验收项 / 目标 | 实际结果 |
|---|---|---|
| R1 | 原 0295 known-boundary public build/run，记录基线红与修复绿 | 待执行 |
| R2 | 正常源码 SSA/native：单文件及两 unit，String/Resource、for/非循环、分支、判空、消费与缺失 | 12/12，0 ignored；unit source 正逆排列，Resource 4 类 loop 退出，详见 matrix green |
| R3 | frontend 普通能力负例与相邻通用 nullable/Map 回归 | 待执行 |
| R4 | fmt、受影响 crate 严格 clippy、docs、diff、Rust 尺寸静态检查 | 待执行 |
| R5 | 独立审查、远端双宿主 CI、最终归档与精确 head 交付 | 待授权/远端执行；本机不宣称通过 |

## 5. 实施与提交

1. 合同与基线 red；核实通用事实消费边界。
2. 最小生产修复和同源定向回归；真实 native stdout 验证。
3. Architecture/验收记录和独立审查；获授权后由父任务负责 PR 与 CI 闭环。

每个逻辑切片独立提交并标记 SPEC-0296。共享 Cargo target 串行使用，不执行 cargo clean。
完成全套必需验收前保持 active；本机结果不替代远端 CI。

## 6. 调查与验证记录

编号已确认未占用；没有仓库级 `.agents/skills`。工具链 Rust 1.96.0、LLVM 21.1.8。
基线 native CLI 构建成功，原项目 build exit 1 / InvalidSsa。进一步对照确认 unit 最后 if 判空
也失败；single 直接 if 已有 NullableBranch 保护，但 Boolean 绑定判空同样失败。
原样 7 项正常源码测试为 1 通过、6 InvalidSsa 失败；最小 descriptor/表示接线修复后 7/7 通过。
原始命令、日志和 SHA256 见[本机 checkpoint](../../development/evidence/owned-null-cleanup-0296/local-checkpoint.json)。
循环原项目由 unit/basic 与 const 原样验收；single 外层 Map owner 进入 for 在更早的既有
UnsupportedNode 门禁被拒绝，另存正常源码，暂不纳入本片能力扩展或通过计数。

扩展探索另确认直接 `m.remove(1) != null` 不发布结果 Temporary drop，和 named 的提前
drop 不同；两入口仍为 InvalidSsa。该 Phase 3 缺口已交后继独立认领，不纳入本片绿验收。

扩展[正常源码矩阵](../../development/evidence/owned-null-cleanup-0296/local-matrix-green.json)
12/12：6 种相等/不等、左右/group 形态分别覆盖 String 与 MoveOnly Text 的 present/missing，
SSA 精确核对 MapRemove 的第二个 owned result 先读取 presence 后唯一次 drop；不复制 owner。
Resource 的消费通过既有显式 `!!` 交付，正常/continue/break/return 的词法次序由 stdout 验证。
条件分支中的 Resource 消费仍受既有 ResourceLifetime 门限制，不从本片推定通用支持。
