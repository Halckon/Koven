# SPEC-0045：单线程共享 `Rc<T>` owner

## 1. 元数据

| 字段 | 值 |
|---|---|
| 状态 | `in-progress` |
| Goal ID | `KOV-P5-045` |
| 所属 Phase | Phase 2/3/4/5 纵向切片 |
| 语言规范 | 现行 [`guide/01-design-decisions.md` §30.2](../guide/01-design-decisions.md#302-rct-的共享所有权契约) |
| 批准依据 | 当前持续 Goal“继续推进 guide 和分阶段实施 specs，先审计 roadmap，再根据依赖图推进”的站立授权 |
| 前置 Spec | SPEC-0042、0028、0035、0183、0185、0188、0184 `done` |
| 前置 ADR | [ADR-0015](../adr/0015-shared-owner-runtime-abi.md) `accepted` |
| 关联 ADR | ADR-0006、0007、0008 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` Rc typed/ownership facts，`lang-codegen` shared-owner SSA/LLVM/runtime，`lang-cli`/`lang-std` native 验收，Architecture/Roadmap |
| 语言语义变更 | 否；实施 v0.30 已批准语义 |

## 2. Goal

完成后，以下源码经过完整 frontend→verified SSA→LLVM→object/link/run 主线：

```kotlin
value class Point(val x: Int)

fun main(): Unit {
    val first = Rc(Point(1))
    val second = first.share()
    println("shared")
}
```

每次 `.share()` 显式增加一个单线程 strong owner，普通赋值仍移动；每个 handle 在 ASAP drop
自动 release，最后一个 handle 精确析构 payload 并释放 control block。

## 3. 范围与需求

### 3.1 Frontend typed facts

- 把 compiler-bound `Rc(value)` / `Rc<T>(value)` 接入 construction，参数名 `value`、mode `Value`，
  允许一个结构上可存储的 payload 类型，并发布 `ConstructionTarget::IntrinsicRc`。
- 只为 intrinsic Rc receiver 识别 `.share()` 与 `.value`，发布稳定 operation identity、receiver、
  payload type 和 Borrow/Value effect；源码同名 class/member 不获得 intrinsic 行为。
- `Rc<T>` 保持 MoveOnly、恒不满足 Transferable；类型实参/参数映射/expected type 使用既有
  construction 与调用诊断，不增加隐式 copy 或 retain。

### 3.2 Ownership facts

- Rc construction 沿用 ordered Value delivery，并建立 shared-owner root drop obligation。
- `.share()` shared-read receiver、不消费源 owner，建立新的 MoveOnly root obligation；源 owner
  move/drop 后禁止再次 share。
- `.value` 只建立与 Rc owner 重叠的 shared payload loan；允许 Borrow 和 `T: Copyable` read，
  拒绝 Inout、MoveOnly owned read、payload move 与 owner 存活期外逃逸。

### 3.3 SSA/verifier 与 LLVM runtime

- 新增 target-independent `SharedOwner` type 和 `SharedAllocate`、`SharedRetain`、
  `SharedPayloadPlace` operations；model/operation/ownership verifier 锁定类型与新 owner obligation。
- LLVM control block、checked allocation、retain overflow abort、release-to-zero payload drop/free
  精确遵守 ADR-0015；counter 非原子且不暴露。
- nullable Rc 使用 null niche；ZST、嵌套 Rc、Copyable/MoveOnly payload 和循环不发生编译器
  panic。strong cycle 不承诺回收。

### 3.4 Native/stdlib 验收

- 标准分析环境提供唯一 Rc intrinsic identity；不要求在 `prelude.ko` 伪造普通 class。
- 真实 CLI build/run 覆盖 construction、一次/多次 share、不同 drop 顺序和 payload 值读取；
  LLVM/进程验收证明一次 allocation、显式 retain、每 handle release 与最后一次 free。

## 4. 非目标

- 不实现 Arc、Weak、Shareable、cycle collector、用户可见 count/retain/release、interior
  mutability 或跨线程共享。
- 不实现源语言 Arena、一般 instance receiver、属性 getter、operator overloading或自定义析构器。
- 不改变普通 class/Box 的独占 ABI，不新增 runtime crate、公开 FFI ABI 或 allocator API。

## 5. 验收标准

- [x] valid Rc construction/share/value typed facts 与源码同名负例通过；参数形状错误复用现行诊断。
- [x] 普通赋值移动、`.share()` 保留源 owner、payload Borrow/Copy 和禁止 MoveOnly 移出有正反例。
- [x] Rc 恒 MoveOnly/非 Transferable，并与 closure/call/drop facts 保持确定性。
- [x] SharedOwner SSA 类型/operation/render/verifier 正反矩阵通过。
- [ ] LLVM IR 锁定 `{usize,payload}` target layout、非原子 checked retain、release-to-zero drop/free、
      nullable/ZST/nested payload 与 verifier-before-LLVM。
- [ ] 真实 `kovenc build/run` Rc 程序退出 0，输出精确且无临时产物泄漏。
- [ ] Architecture、Roadmap、Spec 与 workspace 标准基线同步，最终提交均包含 `SPEC-0045`。

## 6. 技术方案与边界

typed Rc operation 使用独立 descriptor，不伪造普通 `CallDescriptor` 或 field projection；ownership
消费该 descriptor 后发布 share/payload loan facts。SSA 只接收已验证的 intrinsic identity，
新增 shared-owner type 与 operation 放在现有 aggregate/owner 模块旁，LLVM retain/release glue
复用集中 runtime adapter。每个阶段失败都不发布部分下游事实。

## 7. 实施计划

1. [x] Rc construction/share/value typed facts与 Phase 2 正反测试。
2. [x] Rc ownership/share/payload loan/drop facts与 Phase 3 正反测试。
3. [x] SharedOwner SSA type/operation/render/verifier 与直接 IR 测试。
4. [ ] LLVM control block、retain/release/drop glue 与布局/runtime 测试。
5. [ ] frontend→SSA lowering、真实 native build/run、Architecture 与 workspace 基线。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | Rc typed/ownership facts | `feat(frontend): model Rc ownership facts (SPEC-0045)` |
| 2 | SharedOwner SSA/verifier | `feat(codegen): verify shared owner operations (SPEC-0045)` |
| 3 | LLVM Rc runtime ABI | `feat(codegen): lower shared owners to LLVM (SPEC-0045)` |
| 4 | frontend lowering、native 验收与完成文档 | `feat(std): complete Rc native pipeline (SPEC-0045)` |

## 9. 未决问题

- 无。Arc/Weak/Arena 是已明确排除的后续能力，不阻塞本 Spec。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-26 前置审计 | 通过 | 全部前置 Spec `done`；v0.30 已生效；ADR-0015 `accepted` |
| `cargo test -p lang-frontend --tests` | 通过 | 完整 frontend 单元、集成、fixture 与 adversarial 矩阵；新增 Rc typed/ownership/parser 测试全部通过 |
| `cargo check --workspace` | 通过 | Rc 在下一阶段 SharedOwner SSA 落地前由 codegen 明确返回 `UnsupportedNode`，workspace 不接收错误的 Box 映射 |
| `cargo fmt --all` / `git diff --check` | 通过 | frontend 第一切片格式与 whitespace 基线通过 |
| `cargo test -p lang-codegen` | 通过 | 112 passed，1 个既有 LLDB 权限测试 ignored；SharedOwner 类型、operation、render、类型/ownership verifier 正反矩阵通过 |
