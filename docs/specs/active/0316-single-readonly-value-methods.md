# SPEC-0316: single value-class 普通 Borrow 只读方法

> **性质**：有界变更合同 · **状态**：in-progress · **读取时机**：实施 single 普通 receiver 方法时 · **唯一真源**：本页合同与验收；语言语义以 Guide 为准

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P4-0316` |
| 所属 Phase | Phase 4 typed SSA 与 native |
| 语言规范 | [Guide v0.43](../../guide/README.md)、[Receiver](../../guide/08-class-family-members.md#instance-receiver-与静态分发) |
| 批准依据 | 2026-10-14 用户“按建议顺序继续实施”；具体合并仍需批准 |
| 前置 Spec | [SPEC-0309](../../archive/specs/0309-borrow-value-class-fields.md) |
| 基线 | 官方 fetch actual main `16b4464f4d012eb4813e264b480de4198520aeee`；独立 `feature/spec-0316` |
| 影响范围 | single 方法收集、实例规划、receiver Loan ABI、真实字段读取及普通源码验收 |
| 语言语义变更 | 否 |

## 1. 冻结首片

先以无 Map 普通源码证明 single 非泛型 value class 的显式普通 Borrow receiver 方法调用，
默认/显式 Borrow、`this.field`/裸字段读取返回 owned Int/Boolean。方法身份、receiver 来源、
参数映射与 loan 清理由 typed/ownership 同链事实决定，不根据方法名重选目标。
不得只删除 receiver 拒绝而缺失方法收集、entry loan、调用 receiver-first ABI 或 cleanup。

named owner、Borrow 参数、temporary receiver 按已有事实分别验收，不从一例推定全部支持。
同 receiver 隐式调用只在不扩大合同/CFG时列入后续已验证范围；最后独立核对0299组合。
泛型、Inout/Value receiver、borrow-result receiver、条件借用及一般 CFG 不在本片；
unit 既有成功不回退。MoveOnly 字段不复制，本片仅读取可复制 Int/Boolean 字段。

## 2. 验收账本

| 验收项 | 目标/过滤器 | 实际结果 |
|---|---|---|
| 独立无Map红测/同源unit正控 | codegen lib `ssa::single_readonly_method_tests::`；Int/Bool × literal/constant × single/unit | 实际8项：single4项精确call Span UnsupportedNode红测；unit basic/constant4项原版LLVM/native正控通过，0 ignored，生产零改 |
| source receiver/entry及字段Loan identity、结束顺序 | 同模块普通Program只读断言 | 待实施 |
| named/Borrow参数/temporary、参数顺序和owner复用 | 普通源码源矩阵 | 待实施 |
| 原诊断与unit边界、0299后继组合 | 按实际gate反查codegen/frontend/CLI目标 | 待选择 |
| 静态门禁 | fmt、codegen strict clippy、docs/diff、尺寸base16b4464f | 待运行 |
| 双宿主CI与交付 | Draft PR、归档、具体合并授权及actual-main核对 | 未运行 |

single literal/constant使用既有同一constant-aware分析入口；unit basic与validated constant
unit分别验证，不把一个入口成功冒充另一个。native仅静审的正常source→原版LLVM→执行；
不跑坏IR、注入、校准、sanitizer、聚合脚本或stub。原始日志、JSON、冻结hash留仓外。
本机Linux结果不替代macOS CI，失败/未运行/零命中不记通过。

首次三轮保留：single每轮4项均为预期exact红测；unit正控先因空package lookup、第二轮文本替换未命中、第三轮package/logical path不匹配失败。修正测试helper为显式package app与app/readonly-method.ko后，第四轮4红/4绿；非生产修复，不把先前oracle失败改称产品红测。四轮源码前后hash均核对一致。
