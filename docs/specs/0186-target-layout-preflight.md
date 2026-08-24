# SPEC-0186：目标布局预检

## 1. 元数据

| 字段 | 值 |
|---|---|
| 状态 | `done` |
| Goal ID | `KOV-P4-186` |
| 所属 Phase | Phase 4 |
| 语言规范 | 现行 [v0.28 Phase 4](../guide/06-roadmap.md#phase-4llvm-代码生成)、[§3 内联值](../guide/01-design-decisions.md#3-值类型引用类型与装箱) |
| 批准依据 | 当前持续 Goal“继续推进 guide 主线，分阶段实施 specs”的站立授权 |
| 前置 Spec | SPEC-0033、SPEC-0035、SPEC-0036、SPEC-0038 `done` |
| 前置 ADR | [ADR-0007](../adr/0007-llvm-toolchain-and-first-target.md)、[ADR-0008](../adr/0008-internal-value-and-allocation-abi.md) `accepted` |
| 阻塞项 | 无；源码 nominal/enum/Box 类型到稳定用户诊断的映射继续等待 SPEC-0183/0184 |
| 影响范围 | `lang-codegen` LLVM target-layout 预检、type map 与测试；Architecture、roadmap |
| 语言语义变更 | 否；只实现现行 guide 与 ADR 已要求的目标相关失败边界 |

## 2. Goal

完成后，`lang-codegen` 在创建任何 LLVM 复合类型前，先按当前 `TargetMachine` 的 primitive、
pointer 与 `size_t` 布局验证 verified SSA 中的 aggregate、closure environment、container header
及 element stride；布局算术溢出或超过目标 `size_t` 可表示范围时返回确定的结构化 adapter
错误，不进入 LLVM 类型构造、GEP 或 allocator lowering，也不改写成隐式 `Box`。

## 3. 范围与需求

- 新增职责独立的 target-layout preflight。输入只包含 verified SSA `Module`、LLVM `Context` 和
  与最终 module 相同的 `TargetData`；输出是按 `SsaTypeId` 排序的受检 size/alignment 事实或
  `InvalidLayout` 错误。
- Boolean、整数、pointer、pointer-width integer 的基础 size/alignment 必须从真实
  `TargetData` 查询；record padding 使用这些目标事实和受检 `u128` 算术计算，不复制 AArch64
  常量表。目标对象上限由 default address space 的 pointer-width unsigned `size_t` 表示范围导出。
- aggregate 按字段声明顺序递归计算；heap owner/shared reference/function pointer 只采用 pointer
  形状，不递归展开 payload；closure 为 function pointer + inline environment；顺序容器 header
  为既有二/三字段表示，element stride 使用同一 element alloc-size 事实；ZST 保持 size 0。
- inline cycle、跨 module type ID、未定义 heap owner等仍由现有 SSA verifier 拒绝；preflight 不
  复制结构 verifier，也不把非布局错误重新分类。
- `TypeMap::lower` 必须在创建 opaque struct、设置 body 或查询任何复合布局前运行 preflight；
  合法布局继续由 LLVM verifier 和现有 target-derived offset 测试复核。
- 错误至少稳定区分布局算术溢出与超过 target object-size 上限，并携带失败的 `SsaTypeId` 和
  size/alignment/stride quantity；相同输入重复运行产生相同错误。

## 4. 非目标

- 不接入尚未实现的源码 nominal/enum/Box constructor、projection、destructuring 或 DropFact；
  不据此完成 SPEC-0183/0184。
- 不分配新的 `Lxxxx` 诊断码，不把只有 IR-local identity 的失败虚报成已有源码 `Span`。0184
  接入来源类型后必须把本 Spec 的结构化错误映射为用户诊断，roadmap 对应总验收在此前不勾选。
- 不实现 enum tag/payload、nullable layout、多目标 codegen、公开 FFI ABI、large-stack/copy
  warning、优化、隐式 boxing 或运行时 fallback。
- 不改变现有容器运行期 `length * stride` overflow→abort 语义；本 Spec 只拒绝编译期已确定的
  类型布局非法。

## 5. 验收标准

- [x] 合法 padded/nested aggregate、heap-recursive handle、closure、二/三字段 container header、
      Copyable/MoveOnly ZST 的 preflight 事实与 LLVM `TargetData` 实际结果一致。
- [x] 以紧凑递归 type graph 构造的超大聚合在 LLVM 复合类型创建前稳定返回
      `InvalidLayout`；覆盖超过 target 上限和受检算术溢出，不 panic、不截断、不生成 LLVM 文本。
- [x] container element stride 与 aggregate size 使用同一受检事实；非法 element 不进入 buffer
      GEP/allocation，合法 ZST stride 仍为 0。
- [x] 既有 aggregate/container/closure LLVM 正反矩阵与 verifier 保持通过，合法产物文本确定。
- [x] `lang-codegen` 窄测试和 workspace 五项标准基线通过；production 文件符合 1000 行软上限，
      Spec/Architecture/roadmap 只记录实际完成事实。

## 6. 技术方案与边界

- 新建 `llvm::layout` 模块保存不含 Inkwell composite lifetime 的 `TargetLayoutPlan`。计算器只
  依赖现有封闭 SSA type universe，并利用 type builder 已保证的“内联依赖先声明”按
  `SsaTypeId` 源序线性计算；forward/cyclic/foreign/无 storage 依赖仍 fail-loud，不递归消耗
  Rust 调用栈，也不复制结构 verifier。
- primitive/pointer 对齐从 `TargetData` 查询；record 使用标准 ABI 的逐字段 align-up、checked
  add 和尾部 align-up。代表性布局必须逐项与同一 target 上 LLVM 实际 struct layout 对照，
  防止计算规则与 backend 漂移。
- `TypeMap` 消费 plan 的 element alloc-size 作为 container stride，并继续使用 LLVM
  `TargetData` 查询实际 aggregate field offset；preflight 是安全门禁，不成为 frontend/SSA 的
  target-dependent 类型事实。
- 结构化错误保持在 crate 内 LLVM adapter 边界；未来 0184 只负责附加源码 identity/Span 和
  用户诊断，不重新计算布局。

## 7. 实施计划

1. [x] 建立 target-layout plan、受检 record 算法和结构化错误 → 验证：primitive/padded/nested、
   超限/溢出与确定性单元矩阵。
2. [x] 在 `TypeMap` 最前置接入 plan，并让 container stride 消费受检结果 → 验证：错误先于
   composite type body、aggregate/container/closure LLVM 回归。
3. [x] 运行 workspace 基线并同步 Architecture、roadmap 与 Spec 验收 → 验证：退出状态、文件
   规模和文档事实一致。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | target-layout preflight、type-map 接线、测试与完成记录 | `feat(codegen): preflight target layouts (SPEC-0186)` |

## 9. 未决问题

- 无。源码诊断映射是已登记的 0184 交接，不阻塞 IR-local preflight；若现有封闭类型无法仅从
  target primitive/pointer 事实精确复现 LLVM record layout，应停止并回到 ADR-0008，不得用
  硬编码 target 常量或先构造可能非法的复合 LLVM 类型绕过。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-25 前置审计 | 通过 | 0033/0035/0036/0038 `done`、ADR-0007/0008 `accepted`；ASAP drop/source warning 仍依赖 0183/0184，layout preflight 可独立实施 |
| `cargo test -p lang-codegen layout --lib` | 通过 | 5 项 layout 定向测试；合法复合形状与 target 实际结果一致，4096 层内联图迭代完成，`2^64` 超限和 `u128` 算术溢出稳定失败 |
| `cargo clippy -p lang-codegen --all-targets -- -D warnings` | 通过 | 0 warnings；`llvm/layout.rs` 335 行，全部触及 production 文件低于 1000 行软上限 |
| `cargo test -p lang-codegen --all-targets` | 通过 | 101 passed；0 failed / ignored / filtered，aggregate/container/closure/object/debug 回归全部通过 |
| 2026-08-25 workspace 五项标准基线 | 通过 | fmt、workspace check、Clippy `-D warnings`、workspace all-target test 与 `lang-cli` build 均退出 0 |
