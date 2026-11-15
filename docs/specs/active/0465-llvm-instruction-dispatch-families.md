# SPEC-0465：LLVM instruction 分发按 operation 家族归位

> **性质**：实施 Spec · **状态**：in-progress · **读取时机**：本批 LLVM instruction 分发职责整理时 · **唯一真源**：本批范围与验收，语言语义从属 Guide

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P1-0465` |
| 所属 Phase | Phase 4，LLVM adapter |
| 语言规范 | 现行 [Guide](../../guide/README.md)，无语义变更 |
| 批准依据 | 用户选择继续推进推荐项；沿fix/spec-0456与PR #433 |
| 前置 Spec | [0463](0463-ownership-operation-and-lowering-domains.md)、[0464](0464-native-interface-and-receiver-test-domains.md)，输入7085fbfc |
| 前置 ADR | [ADR-0007](../../adr/accepted/0007-llvm-toolchain-and-first-target.md) |
| 影响范围 | llvm/adapter.rs 与私有 instruction 家族模块、adapter 尺寸 policy |
| 语言语义变更 | 否 |
| 阻塞项 | 无 |

## 1. 问题与策略

`llvm/adapter.rs` 1777 行中约 999 行是单个 `lower_instruction`：77 个 match arm 覆盖 `Operation`
全部 99 个 variant。整函数搬迁会让新文件自身超 1000，因此按 operation 家族分布，而不是把
一个算法切成容量碎片：

- 顶层 `lower_instruction` 只保留原 `results` 预计算与**编译期穷尽**的家族分发（每个 variant
  恰好出现一次，`Operation::Constant(..)` 等 tuple 形式按 enum 定义生成）；
- 77 个 arm 正文逐字节搬入私有 `adapter/instruction/{text,scalar,call,aggregate,heap,container,place}.rs`，
  arm 内 pattern、顺序、`return Err`、`?`、cleanup 与 helper 调用不变；
- 家族方法接收 `Vec<ValueId>`（原绑定按值传入），因此正文无需任何 `&`/切片改写；家族内
  最后保留一条**显式内部错误**分支，误路由返回 `unsupported(...)` 而不 panic；
- arm 与家族的一一对应由仓库外机械脚本核对：每个 arm 正文在唯一家族文件内逐字节出现、
  分发覆盖 99 个 variant 且无重复、每个家族的 variant 集合等于分发分组。

## 2. 非目标

不改任何 arm 内的 LLVM 构造、ABI、块、edge、debug location 或错误文本；不合并/拆分 arm，
不调整家族归属以外的顺序，不新增 `unsafe`。不导入未合 #427，不扩大 receiver/borrow/nullable
或 E2/D10 能力，不为凑数删除合理保留模块。

## 3. 验收与证据

- [x] R1：77 arm 逐字节保全、99 variant 分发穷尽且唯一；家族集合与分发分组一致。
- [x] R2：`cargo check`/`--all-targets` 通过；`adapter.rs` 1777→779，七个家族模块均 < 1000。
- [x] R3：同一工作树 stash 前后各 **660/0/0**，FQN 集合与每过滤计数完全相同；feature 配置
  另 **660/0/0**；含真实 native object/link/run 与 LLVM container/object/runtime 用例。
- [x] R4：workspace check、双配置 Clippy（`-D warnings`）、fmt、47 项 policy、docs、尺寸/空白。
- [x] R5：只删除 `adapter.rs` 的 baseline/exception 两项，其他登记逐 JSON 相同；历史超限 22→21。
- [x] R6：Architecture 已同步（`ssa-codegen-runtime.md` 第 17 行分发家族与私有目录）。
- [ ] R7：账本后集中 push；精确新头 CI 另验，保持 active/Draft。

等价证据的正确表述：核心是 **77 个 arm / 99 个 variant 的集合相等**（每个 arm 正文在唯一家族
内逐字节出现、分发每组与家族 arm 集合相同），以及被删区域**逐行可归因**（999 行 = 1 空行 +
函数签名 + 27 行 `results` 预计算 + 2 行 match 花括号 + 967 行 arm；该区域无 doc comment、属性
或臂间注释，因此不存在未受检查的残留文本）。新文件合计多出 274 行脚手架（模块 doc/use/impl
头/方法签名/match 包装/内部错误分支），**行数算术不作为等价证据**。

真实失败与修正保留：首次 `cargo check` 的 95 个错误来自孙模块 `use super::*` 看不到 adapter
绑定，改为 `use super::super::*`；随后 `lower_call` 与既有 helper 同名冲突，家族方法统一加
`lower_instruction_` 前缀；再因原 match 是语句而家族 match 成了尾表达式，改为 `return Err`
加 `Ok(())`。每步都保留日志，未改任何 arm 正文或测试断言。

原始日志/arm 快照/机械脚本/前后 FQN 记录留仓库外。未跑本地完整 frontend/codegen/LLVM 慢组、
CI-only fault、真实变异校准或 sanitizer 全套。
