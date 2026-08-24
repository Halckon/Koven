# SPEC-0036：顺序容器连续缓冲区与运行时基元

## 1. 元数据

| 字段 | 值 |
|---|---|
| 状态 | `in-progress` |
| Goal ID | `KOV-P4-036` |
| 所属 Phase | Phase 4 |
| 语言规范 | 现行 [v0.28 Phase 4](../guide/06-roadmap.md#phase-4llvm-代码生成)、[§8 顺序容器](../guide/01-design-decisions.md#8-顺序容器内存表示与索引语义array--list--mutablelist) |
| 批准依据 | 当前持续 Goal“继续推进 guide 主线，分阶段实施 specs”的站立授权 |
| 前置 Spec | SPEC-0023、SPEC-0030、SPEC-0035 `done` |
| 前置 ADR | [ADR-0008](../adr/0008-internal-value-and-allocation-abi.md) `accepted` |
| 阻塞项 | 无；Phase 5 尚未定义的增删、重排与 relocation effect 明确排除 |
| 影响范围 | `lang-codegen` 顺序容器 SSA type/operation、verifier、LLVM type/runtime adapter 与测试；Architecture |
| 语言语义变更 | 否；实现现行 guide 和 ADR-0008 已封闭的后端表示与运行时基元 |

## 2. Goal

完成后，`lang-codegen` 能用 target-independent typed SSA 表示 `Array<T>`、`List<T>`、
`MutableList<T>` 的唯一 owner、完整构造、length、checked element place、替换和 drop，并按目标
`DataLayout` 生成固定 header、单个连续缓冲区、受检系统分配、先检查后寻址、逆序元素析构与
唯一释放；ZST 保持逻辑索引和析构次数，且不生成逐元素 `Box` 或第二种容器表示。

## 3. 范围与需求

### 3.1 SSA 类型与操作

- 建立 IR-local 顺序容器 kind 和元素类型；三个容器类型始终为 MoveOnly，类型 identity 包含
  kind 与具体元素类型，不包含 target size、alignment、stride 或物理 pointer。
- 支持列表式完整构造，以及以直接 initializer callable 表示的运行时长度完整构造；两者只在
  操作成功后产生完整 owner，不向 SSA 暴露可观察的部分初始化 owner。
- 支持读取 logical length、以 signed `Int` 索引建立 checked element place，以及 `Array` /
  `MutableList` 的原子替换。`List` 替换、元素类型不匹配、错误 initializer 签名与非容器 owner
  必须由 verifier 拒绝。
- container operation 必须复用现有 MoveOnly、place、loan、CFG edge 与 drop verifier；形成
  element place 不消费 owner，整体 move 后不得继续访问，存在冲突 loan 时不得替换或 drop。

### 3.2 LLVM 表示与 runtime

- `Array<T>` / `List<T>` lower 为 `{ptr, size_t}`，`MutableList<T>` lower 为
  `{ptr, size_t, size_t}`；header 是 first-class aggregate，不含 storage tag、small-buffer 或
  inline elements，参数/返回 ABI 不触发额外 heap allocation。
- 从目标 `DataLayout` 获取元素 store size、ABI alignment 与 allocation stride；非空非 ZST
  buffer 只执行一次受检 `capacity * stride` 和一次 `malloc`，overflow/OOM/负长度进入
  `abort` + `unreachable`，不生成 unwind cleanup。
- 列表式构造按 operand 顺序写入；运行时长度构造在分配成功后按 `0..<length` 顺序调用
  initializer。checked-index 必须在 GEP 前完成 signed negative 与 unsigned upper-bound 检查。
- ZST 或零容量使用 module-private 对齐 sentinel，不调用 `malloc`/`free`，不把零 stride GEP
  当成可解引用字节；logical length、checked-index 和 MoveOnly ZST 的逐元素逆序 drop 次数仍保留。
- 正常 container drop 从 `length - 1` 到 `0` 对 MoveOnly 元素调用既有 type-directed drop glue，
  随后只对真实 allocation 调用一次 `free`；Copyable 元素跳过元素 glue，但仍释放 buffer。
- element replacement 先完成 checked-index，再把旧值载入临时、写入新值，最后 drop 旧值；
  写入完成后不保留失败分支。

### 3.3 分阶段交接

- 本 Spec 使用手工构造且通过 verifier 的 SSA 验收 runtime/codegen，不接入尚未覆盖容器的完整
  frontend→SSA lowering；后续 frontend 接线必须消费 SPEC-0023/0030 已发布 descriptor，不能
  按函数名或 AST 形状猜测容器。
- 直接 initializer callable 只建立 SPEC-0036 所需的调用边界；捕获 closure environment 仍由
  SPEC-0038 承接。Phase 5 增删、删除、扩容、重排和 relocation effect 不进入本 Spec。

## 4. 非目标

- 不实现 `MutableList.add/removeAt`、扩缩容、重排或任意 Phase 5 API；不建立猜测性的
  relocation operation。
- 不实现 Map、String、iterator/`for` provider、closure capture、源码 constructor 选择、object、
  linker、DWARF、public FFI 或容器优化。
- 不引入逐元素 allocation、隐式 `Box`、small-buffer optimization、动态 `alloca`、引用计数、
  allocator hook、异常展开或部分构造 cleanup。
- 不在本 Spec 分配目标相关大栈帧/大复制 warning；该 roadmap 条目保持独立候选 Goal。

## 5. 验收标准

- [ ] 顺序容器 type/kind/element identity、MoveOnly 能力、跨 module ID 与确定 debug text 正反矩阵通过。
- [ ] 列表式/运行时长度构造、length、checked element place、替换和 drop 的 operation contract 与
      CFG ownership 正反矩阵通过；`List` mutation 和 move 后访问被拒绝。
- [ ] AArch64 LLVM IR 锁定两字段/三字段 header、单个连续 allocation、受检大小、OOM/负长度
      abort，以及不存在逐元素 allocation、storage tag 和动态 alloca。
- [ ] checked-index 的负值/上界失败均在 GEP 前进入 abort；合法 Copyable/MoveOnly 元素 place
      load/borrow/store 使用相同 buffer 表示。
- [ ] 正常 drop 对 MoveOnly 元素逆序调用 glue 并唯一 free；Copyable 元素不调用 glue；ZST 不
      malloc/free、仍按 logical length drop，空容器不访问元素 storage。
- [ ] 合法 module 通过自建 verifier 与 LLVM verifier并产生确定文本；人工损坏 SSA 在 LLVM
      construction 前被拒绝。
- [ ] `lang-codegen` 窄测及 workspace 标准基线通过；生产 Rust 文件遵守 1000 行软上限，
      Architecture、Spec 索引和 roadmap 只记录实际完成事实。

## 6. 技术方案与边界

- container type model 与构造 API 放在独立 SSA 模块；局部 operation contract、线性 ownership、
  render 和 LLVM adapter 延续现有职责分层，不把 container runtime 堆入已接近软上限的文件。
- SSA 使用完整 owner operation，不公开 partially-initialized buffer token；abort 路径不要求清理，
  正常路径由 verifier 保证 owner 恰好 move/consume/drop 一次。
- LLVM container layout/runtime 复用 ADR-0008 已建立的 `TypeMap`、`RuntimeAbi` 与系统符号边界；
  allocator、abort、free 的声明仍只有一个集中入口。
- element place alias root 追溯到 container owner；物理地址只用于 codegen，SSA loan identity 不
  通过 pointer equality 推导。

## 7. 实施计划

1. [x] 建立顺序容器 SSA type、operation、render 与 verifier → 验证：类型、操作、ownership、
   alias 与确定性矩阵。
2. [x] 扩展 LLVM type map、固定 header 与连续 buffer allocation/构造 → 验证：DataLayout、
   overflow/OOM/负长度、单 allocation 与无第二表示矩阵。
3. [ ] 实现 length、checked-index、element place/replace 与 container drop → 验证：检查先于 GEP、
   loan/mutation、逆序 drop、ZST sentinel 与唯一 free 矩阵。
4. [ ] 运行 workspace 基线、同步 Architecture/roadmap/Spec 验收并审查 staged diff → 验证：实际
   退出状态、文件规模和文档一致性。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | container SSA type/operation、render 与 verifier | `feat(codegen): model sequential container operations (SPEC-0036)` |
| 2 | fixed header、continuous buffer allocation 与构造 | `feat(codegen): allocate sequential container buffers (SPEC-0036)` |
| 3 | checked element place、replace、drop/ZST runtime 与 done 验收 | `feat(codegen): lower sequential container runtime (SPEC-0036)` |

## 9. 未决问题

- 无。若直接 initializer callable 无法在不提前实现 closure environment 的情况下形成完整后端
  边界，应保持列表式和 buffer runtime 可验证，并把捕获 initializer 接线显式交给 SPEC-0038；
  不得引入临时函数对象 ABI。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-25 前置审计 | 通过 | SPEC-0023/0030/0035 `done`、ADR-0008 `accepted`；Phase 5 relocation API 明确排除 |
| `LLVM_SYS_211_PREFIX=/opt/homebrew/opt/llvm@21 cargo test -p lang-codegen --all-targets`（SSA slice） | 通过 | 65 项；新增 6 项 container kind/element identity、结构 identity cycle、构造/generate/length/place/replace/drop、List mutation、move-after-drop 与 loan 冲突矩阵 |
| `LLVM_SYS_211_PREFIX=/opt/homebrew/opt/llvm@21 cargo clippy -p lang-codegen --all-targets -- -D warnings`（SSA slice） | 通过 | 无 warning；生产 `model.rs` 901 行、`verify_ownership.rs` 906 行，后续 LLVM/runtime 职责不继续堆入这两个文件 |
| 2026-08-25 workspace 标准基线（SSA slice） | 通过 | fmt、workspace check、workspace clippy `-D warnings`、workspace all-target test 与 `lang-cli` build 全部退出 0 |
| `LLVM_SYS_211_PREFIX=/opt/homebrew/opt/llvm@21 cargo test -p lang-codegen --all-targets`（buffer slice） | 通过 | 68 项；新增固定二/三字段 header、单连续 allocation、受检 size/OOM/负长度、direct initializer loop 与 ZST sentinel 三项 LLVM verifier/确定性矩阵 |
| `LLVM_SYS_211_PREFIX=/opt/homebrew/opt/llvm@21 cargo clippy -p lang-codegen --all-targets -- -D warnings`（buffer slice） | 通过 | 无 warning；`adapter.rs` 969 行，aggregate/container/entity/runtime/type-map 按职责拆分且生产文件均低于 1000 行软上限 |
| 2026-08-25 workspace 标准基线（buffer slice） | 通过 | fmt、workspace check、workspace clippy `-D warnings`、workspace all-target test 与 `lang-cli` build 全部退出 0 |
