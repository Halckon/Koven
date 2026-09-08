# SPEC-0192：一般 UTF-8 String runtime

## 1. 元数据

| 字段 | 值 |
|---|---|
| 状态 | `done` |
| Goal ID | `KOV-P5-192` |
| 所属 Phase | Phase 2/3/4/5 纵向切片 |
| 语言规范 | 现行 [v0.31 §31](../guides/v0.34-pre-restructure/01-design-decisions.md#31-一般-utf-8-string-owner-与最小运行时表面v031) |
| 批准依据 | 2026-08-26 用户明确启用 v0.31、接受 ADR-0018/0019，并批准实施 SPEC-0192 |
| 前置 Spec | SPEC-0042、0043、0184、0189、0195 `done` |
| 前置 ADR | [ADR-0018](../../adr/accepted/0018-string-owner-runtime-abi.md) `accepted` |
| 关联 ADR | ADR-0006、0008、0016 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` String/drop facts，`lang-codegen` String SSA/verifier/LLVM/runtime，native tests，Architecture/Roadmap |
| 语言语义变更 | 否；实施已启用的 v0.31 §31 |

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
节点的边界；v0.31 与 ADR-0018 已生效，本 Spec 已获准并完成实施。

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

- [x] frontend/ownership 正反测试证明 literal、local、参数/返回、capture 和 aggregate/container
      String 使用既有唯一 owner、Borrow 与 ASAP drop 契约。
- [x] String SSA operation/render/model/ownership verifier 正反矩阵通过；非法类型、非 active loan、
      move 后使用、重复 drop 与 corrupt provenance 在 LLVM 前被拒绝。
- [x] LLVM IR 锁定目标布局、static/empty/heap provenance、checked concat、byte equality、动态
      stdout 和精确 drop/free；无宿主 Rust/C string、隐式 retain、unwind 或 NUL 扫描。
- [x] native build/run 精确覆盖 ASCII、Unicode、U+0000、空串、连接/相等、跨函数传递/返回以及
      aggregate/Rc/顺序容器的正常与提前退出清理。
- [x] interpolation、`String?` 与未发布 member API 确定性拒绝且无 compiler panic或残留产物。
- [x] Architecture、Guide/Roadmap、Spec 验证记录与 workspace 标准基线同步。

## 7. 技术方案与边界

严格实施 accepted 后的 ADR-0018。先建立 target-independent String SSA identity、operation 和
verifier，再接 frontend facts，最后接 LLVM/runtime；不得先在 `println`、binary operator 或
Array lowering 中加入互不兼容的裸 pointer 特例。

SPEC-0189 的 `PrintLiteral` 可在迁移完成后删除或作为 lowering 内部优化保留，但 public typed
SSA 只能有一套 String owner/print 契约；若保留优化，测试必须证明它不改变 owner/drop、输出或
诊断。现有 Borrow ABI 由 SPEC-0195/ADR-0016 复用，不在本 Spec 重新定义。

## 8. 实施计划

1. [x] 建立 StringOwner layout/operation/verifier 与 target preflight → 验证：SSA 正反矩阵和
   LLVM layout 单元测试。
2. [x] 接 plain literal、局部、参数/返回、concat/equality 和 drop facts → 验证：frontend 与
   lowering 窄测试。
3. [x] 接 LLVM allocation/copy/compare/print/drop 及 aggregate/container glue → 验证：IR 与真实
   native build/run 矩阵。
4. [x] 同步 Architecture、Roadmap、验证记录并运行 workspace 标准基线 → 验证：文档与事实一致。

## 9. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | String owner SSA、layout 与 verifier | `feat(codegen): verify String owners (SPEC-0192)` |
| 2 | frontend/LLVM/runtime/native 闭环与完成文档 | `feat(std): lower general String runtime (SPEC-0192)` |

## 10. 未决问题

- 无。

## 11. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-26 roadmap 前置审计 | 通过 | 0042/0043/0184/0189/0195 均 `done`；v0.31 已启用，ADR-0018 已接受 |
| `cargo test -p lang-frontend --test ownership_checking` | 通过 | 15/15；String binary operand/drop facts 正反覆盖 |
| `cargo test -p lang-codegen --lib` | 通过 | 151 passed、1 ignored；ignored 为既有 sandbox debugserver 权限测试 |
| `cargo test --workspace` | 通过 | workspace 全量基线；既有环境相关 ignored 项保持原状 |
| `cargo check --workspace` | 通过 | 全 workspace 类型检查 |
| `cargo clippy --workspace --all-targets -- -D warnings` | 通过 | 零 warning |
| `cargo fmt --all -- --check` | 通过 | Rust 格式基线 |
| `git diff --check` | 通过 | 无 whitespace error |
| 真实 native link/run | 通过 | 动态 String、复合 owner/Rc、Array/List/MutableList 与 move closure 精确输出和正常/提前退出清理 |
