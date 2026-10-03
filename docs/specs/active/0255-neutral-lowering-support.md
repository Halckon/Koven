# SPEC-0255: 中立 lowering error 与 String helper 边界

> **性质**：有界变更合同 · **状态**：in-progress · **读取时机**：复核两种 lowering adapter 的共同支撑边界时 · **唯一真源**：本页合同及验收账本

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P4-255` |
| 所属 Phase | Phase 4 SSA lowering；治理 P4 首个有界责任 |
| 语言规范 | 现行 [Guide](../../guide/README.md)，[String](../../guide/13-program-runtime-standard-library.md#string) |
| 批准依据 | 已批准[治理计划 P4](../../development/engineering-governance-plan.md#p4-共享纯内核与双轨收敛-l-条件阶段)及持续执行授权；2026-10-03 独立设计 review Approve |
| 前置 Spec | SPEC-0254 |
| 前置 ADR | 无 |
| 关联 ADR | 无；责任迁移属于获批实现细节，不新增长期架构决策 |
| 阻塞项 | 本地实现、独立最终 review 与精确 head 双宿主 CI 尚待 |
| 影响范围 | `lang-codegen::ssa` 私有错误与 literal helper、相关测试和事实文档 |
| 语言语义变更 | 否 |

## 1. Goal

让单文件与 compilation-unit lowering 共用的错误和静态 String 解码归属中立私有模块，
消除 unit 对 single adapter 的结构依赖，同时保持既有字节、来源、错误和能力边界。

## 2. 基线与前置

从真实 main `9ac49f3ad3b1c8b0e5c762413f56bde719bf11d0`、tree
`c79e4d87da25b026c6f5f61409df0e36aa3ca17b` 建立 `feature/spec-0255`。
[主干 run 37124524536](https://github.com/Halckon/koven/actions/runs/37124524536) 已验证9/9 jobs、
77/77 steps；raw日志97批，Linux2446 passed、macOS2444 passed及既有LLDB ignore1。
P3精确主干与独立设计两项前置均在生产修改之前满足。不因合并重复运行未修改的整套本地测试。

原 `LoweringError/Kind` 定义在 `lower_frontend`，由 `ssa` 根 re-export；实际可见性是
`pub(crate)`，不是 crate 外公开类型。`unit_lower` 从 single adapter 导入 `string_literal`，
两 driver 已共同使用同一 `decode_plain`，本片不声称新创共享解码或 constant evaluator。
三个 error 构造都仅装配 `kind` 与 `Some(span)`。

## 3. 实现合同

- 私有 `ssa::lowering_support` 拥有原错误 enum/struct、同构 const error 构造及
  `string_literal` 子模块；错误种类、字段、derive与可见性不变
- `ssa::{LoweringError, LoweringErrorKind}` 与原 single adapter 内类型路径继续 re-export；
  两 driver/planner 本地 helper 名经 import/alias 保持，调用点和 None span 构造不变
- decoder 逐字移动，保留按传入文本切片的原合同，不新增 SourceId 校验
- 原 validation、planner、driver、native映射、原子发布及错误优先级不变

## 4. 非目标

不统一两份具有不同 StaticSelf/nullable/nominal 能力的 concrete-type resolver；不迁 source lookup，
不新增 trait/crate/dependency/public capability。无需受控性能对照，不宣称速度、RSS或构造次数收益。
不增加 String interpolation、receiver或recovery能力；不关闭0182、其余P2/P4/P5或整个治理计划。
外部审计继续在整个计划之后。

## 5. 配对与拒绝矩阵

| 场景 | single 原分析链 | unit-basic validated链 | unit-const 专用链 |
|---|---|---|---|
| UTF-8/NUL/escape/Group、String owner/borrow/drop、Abort、泛型交付 | lower成功 | lower成功；逻辑同名旁源、source插入与inputs顺序置换 | 保留既有完整codegen const资源oracle |
| const String物化 | 原常量facts、lower成功 | 原typed validate拒绝，不伪造lower调用 | validate_constants/constant ownership后lower成功 |
| interpolation | lower UnsupportedNode及原String span | 同左 | 保留原专用插值拒绝oracle |
| 无效name/type | lower FrontendDiagnostics/None | names或types validation阻断 | 原专用validation阻断 |
| single多重失效 | source mismatch优先于analysis，再diagnostics | 保留既有factory provenance矩阵 | 保留0254 factory/native/lower provenance矩阵 |
| Borrow String receiver调用 | 原UnsupportedNode及call span | 原成功 | 不从basic结果推导专用新能力 |

新配对断言按逻辑source path、Span、StringOwner结构与definition→use关系归一，不比较裸TypeId，
不强制全SSA程序同形。receiver差异由旧实现实测发现并冻结，不为追求parity改变现有能力。
helper畸形输入单独锁MissingFact/None、invalid range/UTF-8边界、InvalidLiteral/Some(text span)
及interpolation/text先后优先级；这些不冒充能穿过validated入口的recovery输入。

## 6. 依赖门禁

有界词法guard扫描实际 unit_lower/unit_plan根与递归生产子树，禁止回依赖lower_frontend；
neutral support禁止adapter/planner/LLVM/model；model/types/verify及嵌套生产禁止adapter/support
及support-owned类型/函数符号。根facade/glob/alias也受检查；嵌套IR的父模块glob允许。
只排除测试文件与单个cfg(test)项，保留同文件后续生产；忽略注释、普通与raw string。
这是回归护栏，不宣称完整rustc依赖图。完整codegen suite在现有双宿主CI执行新测试。

真实红测以保留的 `lower_frontend::{LoweringError, LoweringErrorKind}` 路径临时替换unit
类型import；代码必须可编译，失败来自同一实际文件guard。恢复后同选择转绿。

## 7. 验收账本

| 验收项 / 命令或目标 | 实际结果 |
|---|---|
| A1 旧helper合同 `lang-codegen --lib lowering_support_tests` | 原生产路径6 passed；0 failed/ignored，748 filtered |
| A2 旧双入口合同 `lang-codegen --lib lowering_entry_contract_tests` | 8 passed；最终 `--lib lowering_` 57 passed/0 failed/ignored（含全部14项新合同与43项既有相关测试），705 filtered；source插入/input置换与String definition→use对照均通过。最初fixture拼写/能力假设修正不算生产缺陷红测 |
| A3 同组后测、字节迁移和旧路径兼容 | 待实施 |
| A4 依赖guard自测、实际树及真实reverse-import红/恢复绿 | 待实施 |
| A5 完整codegen及String/Abort/native/resource旧oracle | 生产修改后执行；不重复运行未改动基线全套 |
| A6 workspace all-targets check、codegen严格Clippy、fmt | 待执行 |
| A7 docs、全部Python policy、尺寸、diff | 待执行；额度仅收紧 |
| A8 独立最终review、exact-head双宿主CI | 未运行；不得由基线证据替代 |

## 8. 实施与交付

1. 已完成独立设计Approve与P3主干前置；先锁原helper和有界配对合同
2. 最小生产迁移与same-selection绿；依赖guard负例及可编译真实反向import红/恢复绿
3. 按A5–A7运行实际影响面门禁；补齐事实文档并做独立最终review
4. 0254归档及三处事实纠正随本批组合交付，不发独立文档PR；远端发布与精确CI另记

本地测试/合同、生产迁移及验收文档分别提交，均标注SPEC-0255；不发布中间阶段。
完成状态与归档仅在实际验收成立后更新，尚未运行项保留。
