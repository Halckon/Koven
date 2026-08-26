# SPEC-0192：一般 UTF-8 String runtime

## 1. 元数据

| 字段 | 值 |
|---|---|
| 状态 | `draft` |
| Goal ID | `KOV-P5-192` |
| 所属 Phase | Phase 2/3/4/5 纵向切片 |
| 语言规范 | 非现行 [v0.31 候选 §31](../guide/01-design-decisions.md#31-一般-utf-8-string-owner-与最小运行时表面v031-候选)；现行版本仍为 v0.30 |
| 批准依据 | 无；候选 guide 未启用，当前持续 Goal 只授权先物化 draft |
| 前置 Spec | SPEC-0042、0043、0184、0189、0195 `done` |
| 前置 ADR | [ADR-0018](../adr/0018-string-owner-runtime-abi.md) `proposed`，尚未接受 |
| 关联 ADR | ADR-0006、0008、0016 |
| 阻塞项 | 用户明确启用 v0.31 取代 v0.30；ADR-0018 由 `proposed` 转为 `accepted` |
| 影响范围 | `lang-frontend` String/drop facts，`lang-codegen` String SSA/verifier/LLVM/runtime，native tests，Architecture/Roadmap |
| 语言语义变更 | 是；必须先启用 v0.31 候选 §31，不能按本草案反向修改现行语义 |

## 2. Goal

完成后，plain UTF-8 String literal 与动态 String 共享同一 MoveOnly owner 表示；String 可被持有、
Borrow 传参、Value 返回、连接、比较、动态输出并在既有 ASAP drop point 正确释放，从而成为
SPEC-0194 构造 `Array<String>` argv 的已验证 runtime 前置。

## 3. 背景

frontend 已能解析和检查 String literal、普通参数/返回、`+`、`==` / `!=`，ownership 也会为
MoveOnly String 建立 move、loan 和 drop facts；codegen 目前只接受 SPEC-0189 的 literal-only
`println` / `error` 特例，不能把一般 String lower 为 SSA/LLVM value。这造成已发布静态语义与
native 能力不对齐，并阻塞 v0.30 已定义但未实施的参数化 main。

roadmap 审计显示，SPEC-0193、0195、0196 已完成后，一般 String runtime 是当前能解锁最多后续
节点的边界；但候选 §31 与 ADR-0018 尚未生效，因此本文件只能保持 draft。

## 4. 范围与需求

- frontend→SSA 使用 builtin String identity 和既有 typed/ownership/drop facts；plain literal、
  local、Borrow 参数、Value 参数/返回与 move closure capture 不使用源码名称或 literal-only 特例。
- typed SSA 实施 ADR-0018 的 `StringOwner`、literal、concat、equal、print 与 drop operation，
  verifier 覆盖类型、loan、owner obligation、source order、move 后使用和重复 drop 正反矩阵。
- LLVM 使用 `{ptr, length, capacity}` 内部布局；literal/empty/heap provenance、target-width overflow、
  allocation failure、精确 free 与 verifier-before-LLVM 均有 IR 测试。
- `String + String` 左到右各求值一次，以 shared-read 方式产生新 owner且不消费具名 operand；
  `==` / `!=` 比较完整字节内容，不分配或 normalize。
- `println(String)` 从 literal-only lowering 迁移为任意 active String Borrow，写全部 bytes 和单个
  LF；保留同名源码函数隔离与短写 abort。`error(String)` 接受任意 String Borrow并进入既有
  Abort effect，不新增 message 输出保证。
- native build/run 覆盖 literal 赋值、跨函数 Borrow/Value/return、连接/相等、嵌入 NUL/Unicode、
  move closure capture，以及 String 作为已支持 value class/enum/Box/Rc/Array/List 元素的布局与
  正常/提前退出析构。
- unsupported 输入必须在 object 写盘前确定性失败且不 panic；失败不得留下部分 owner、object
  或 executable。

## 5. 非目标

- 不实现 `fun main(args: Array<String>)`、宿主 argv 转换或 executable-name 过滤；由 SPEC-0194
  单独承接。
- 不实现 interpolation、`toString`/formatting、String member、length/index/slice、builder、编码
  转换、intern、公开 FFI 或可恢复 allocation/IO error。
- 不实现 `String?` native ABI、SSO、copy-on-write、隐式 Rc/Arc、GC 或 cycle collector。
- 不新增 workspace runtime crate，不改变 `String` 的 MoveOnly/Transferable 或 `Copyable` 规则。

## 6. 验收标准

- [ ] frontend/ownership 正反测试证明 literal、local、参数/返回、capture 和 aggregate/container
      String 使用既有唯一 owner、Borrow 与 ASAP drop 契约。
- [ ] String SSA operation/render/model/ownership verifier 正反矩阵通过；非法类型、非 active loan、
      move 后使用、重复 drop 与 corrupt provenance 在 LLVM 前被拒绝。
- [ ] LLVM IR 锁定目标布局、static/empty/heap provenance、checked concat、byte equality、动态
      stdout 和精确 drop/free；无宿主 Rust/C string、隐式 retain、unwind 或 NUL 扫描。
- [ ] native build/run 精确覆盖 ASCII、Unicode、U+0000、空串、连接/相等、跨函数传递/返回以及
      aggregate/Rc/顺序容器的正常与提前退出清理。
- [ ] interpolation、`String?` 与未发布 member API 确定性拒绝且无 compiler panic或残留产物。
- [ ] Architecture、Guide/Roadmap、Spec 验证记录与 workspace 标准基线同步。

## 7. 技术方案与边界

严格实施 accepted 后的 ADR-0018。先建立 target-independent String SSA identity、operation 和
verifier，再接 frontend facts，最后接 LLVM/runtime；不得先在 `println`、binary operator 或
Array lowering 中加入互不兼容的裸 pointer 特例。

SPEC-0189 的 `PrintLiteral` 可在迁移完成后删除或作为 lowering 内部优化保留，但 public typed
SSA 只能有一套 String owner/print 契约；若保留优化，测试必须证明它不改变 owner/drop、输出或
诊断。现有 Borrow ABI 由 SPEC-0195/ADR-0016 复用，不在本 Spec 重新定义。

## 8. 实施计划

1. [ ] 建立 StringOwner layout/operation/verifier 与 target preflight → 验证：SSA 正反矩阵和
   LLVM layout 单元测试。
2. [ ] 接 plain literal、局部、参数/返回、concat/equality 和 drop facts → 验证：frontend 与
   lowering 窄测试。
3. [ ] 接 LLVM allocation/copy/compare/print/drop 及 aggregate/container glue → 验证：IR 与真实
   native build/run 矩阵。
4. [ ] 同步 Architecture、Roadmap、验证记录并运行 workspace 标准基线 → 验证：文档与事实一致。

## 9. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | String owner SSA、layout 与 verifier | `feat(codegen): verify String owners (SPEC-0192)` |
| 2 | frontend/LLVM/runtime/native 闭环与完成文档 | `feat(std): lower general String runtime (SPEC-0192)` |

## 10. 未决问题

- 门禁问题不是实现选择：只有用户明确启用 v0.31 并接受/授权接受 ADR-0018 后，本 Spec 才能
  从 `draft` 进入 `approved` / `in-progress`。

## 11. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-26 roadmap 前置审计 | 部分通过 | 0042/0043/0184/0189/0195 均 `done`；候选 §31 未启用，ADR-0018 仍 `proposed` |
