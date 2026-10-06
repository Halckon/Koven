# SPEC-0280: MutableList 顺序追加与动态扩容 (`MutableList.add`)

> **性质**：变更合同 · **状态**：done · **读取时机**：实施或评审 MutableList 动态增长操作时 · **唯一真源**：本 Spec

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P4-0280` |
| 所属 Phase | Phase 2 预声明成员识别；Phase 3 独占借用与元素转移；Phase 4 SSA、动态扩容原语与 native 运行 |
| 语言规范 | 现行 [Guide v0.41](../../guide/README.md)；[集合与解构](../../guide/12-collections-destructuring.md) |
| 批准依据 | 用户持续推进里程碑授权；Guide v0.41 §12 已规范 `MutableList.add`、所有权契约与缓冲区扩容不变量 |
| 前置 Spec | SPEC-0275、SPEC-0276、SPEC-0278、SPEC-0279 已 done 并合入 main |
| 前置 ADR | [ADR-0008](../../adr/accepted/0008-internal-value-and-allocation-abi.md)、[ADR-0016](../../adr/accepted/0016-interprocedural-borrow-abi.md) |
| 关联 ADR | 无新增 ABI |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` 容器成员与调用检查、所有权独占借用；`lang-codegen` SSA 原语、verifier、LLVM lowering 与 native 测试 |
| 语言语义变更 | 否（落实 Guide v0.41 §12 既有规范） |

## 1. Goal

在单文件与 compilation unit 两入口交付 `MutableList<T>.add(element: T)`：
- 调用者能够对 `MutableList<T>` 实例调用 `.add(element)`，将元素追加到末尾，`size` 递增 1，并在容量不足时自动完成缓冲区重分配与已有元素安全搬迁（relocation）；
- `element` 满足 Value 所有权契约：`Copyable` 元素交付 owned copy，`MoveOnly` 元素转移所有权；
- 精确遵守所有权借用不变量：对 `MutableList` 的活跃借用（包括元素借用）与 `.add` 互斥，编译期报告 L0135；
- 析构时：逆序析构 `[0, size)` 中的元素并释放当前缓冲区，无内存泄漏与双重释放。

## 2. 背景与需求

Koven v0.41 Guide §12 明确规范：
> “空的 `MutableList<T>()` 配合取得元素所有权的 `add` 等 Phase 5 API，从未知数量的运行时数据源逐步构造动态容器。”
> “`MutableList.add` 取得元素所有权（声明端 `own` 映射的 `Value` 契约），因此调用点写作 `list.add(element)` 即可，无论 `element` 是已有 place 还是临时值都不需要标注；`Copyable` 元素交付 owned copy，否则移动。所有形式都不得隐式 clone、retain 或装箱。”
> “`MutableList<T>` 扩容或重排时可以把已初始化元素移动到新缓冲区。该操作是 relocation / move 而不是用户可观察的复制；旧位置不再拥有值。存在任何有效元素借用时，不得执行会改变元素地址或销毁该元素的扩容、缩容、删除、替换或重排。”

此前 SPEC-0274 交付的词频程序受限于容器无法动态增长，必须使用有界数组与二次扫描；SPEC-0279 完成了运行时长度构造。为了满足 M3A 顺序集合实用化及后续 M1B-b 动态文本处理的需求，必须打通 `MutableList.add` 的端到端实现。

## 3. 范围与技术方案

1. **Phase 2 类型检查**：
   - 在 `container_operations.rs` 中识别 `MutableList` 的预声明成员方法 `add`；
   - 检查参数数量（恰好 1 个）与类型匹配（`T`）；
   - 返回类型为 `Unit`；
   - 对 `Array` 或 `List` 调用 `add` 报告 `codes::INVALID_CONTAINER_MEMBER`。
2. **Phase 3 所有权检查**：
   - 对 receiver 建立 `Exclusive` 借用，调用期间锁定容器及其所有元素；
   - 消费 `element`（MoveOnly 移入，Copyable 复制）；
   - 若存在未结束的元素借用或容器借用，报告 `codes::BORROW_CONFLICT` (L0135)。
3. **Phase 4 SSA 与 LLVM Native**：
   - SSA 操作：`Operation::ContainerAppend { owner, element }`；
   - LLVM lowering：
     - 检查 `size == capacity`；
     - 若容量已满，计算新容量 `new_cap = if cap == 0 { 4 } else { cap * 2 }`（受 `max_logical_length` 约束）；
     - 分配新缓冲区，将已有 `size` 个元素 memcpy / 逐槽搬迁至新缓冲区，释放旧缓冲区；
     - 在 `new_buffer[size]` 处写入 `element`；
     - 更新 `size = size + 1`，将更新后的 header 写回容器绑定。

## 4. 非目标

- 不实现 `MutableList` 任意索引插入（`insert(at, element)`）或删除（`removeAt` / `clear`），留待后续独立切片；
- 不实现切片、借用返回视图或自定义集合接口；
- 不引入异常展开；容量溢出或分配失败按规范确定性 abort。

## 5. 验收标准
 
- [x] G1: `MutableList<Int>.add` 从空列表（`mutableListOf()` / `MutableList<Int>()`）追加单元素与多元素，验证 `size`、索引访问与遍历正确。
- [x] G2: `MutableList<String>.add` 与 `MutableList<Resource>.add`：MoveOnly 与含 `deinit` 的资源类型追加，验证所有权转移与逆序析构（零泄漏）。
- [x] G3: 活跃元素借用期间调用 `.add` 产生编译期 L0135 诊断。
- [x] G4: 针对 `Array` / `List` 调用 `.add` 产生无效成员编译期诊断。
- [x] G5: 双宿主（macOS arm64 / Linux x86_64）native 测试全绿，通过架构及尺寸门禁。
 
## 6. 验证记录
 
| 验收项 | 目标与过滤器 | 结果 | 证据 |
|:---|:---|:---|:---|
| 类型与成员检查 | `cargo test -p lang-frontend --test type_containers` | PASS | 12 passed，覆盖 `container_add_rejects_array_and_list` 等负例及类型推导 |
| 所有权借用冲突 | `cargo test -p lang-frontend --test ownership_containers` | PASS | 20 passed，覆盖 `mutable_list_add_conflicts_with_active_element_borrow` (L0135) 与 `mutable_list_add_transfers_move_only_elements` |
| SSA 容器操作与 Verifier | `cargo test -p lang-codegen mutable_list_add` | PASS | 2 passed，覆盖单文件与多文件编译单元 SSA lower 到 `ContainerAppend` 并通过 verifier 校验 |
| 双宿主 Native 执行与析构 | `cargo test -p lang-codegen unit_container_append` | PASS | 3 passed，验证空/非空初始容量动态倍增追加及 MoveOnly 元素逆序析构零泄漏 |
