# SPEC-0285: MutableList 元素指定索引插入与向后平移扩容 (`MutableList.insertAt`)

> **性质**：变更合同 · **状态**：done · **读取时机**：追溯 MutableList 元素指定索引插入与向后平移扩容操作设计与实现时 · **唯一真源**：本 Spec

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P4-0285` |
| 所属 Phase | Phase 2 预声明成员识别；Phase 3 独占借用与元素所有权转移；Phase 4 SSA、边界检查、几何扩容、内存平移与 native 运行 |
| 语言规范 | 现行 [Guide v0.41](../../guide/README.md)；[集合与解构](../../guide/12-collections-destructuring.md) |
| 批准依据 | 用户持续推进里程碑授权；Guide v0.41 §12 与 §8 动态容器变异操作 |
| 前置 Spec | SPEC-0280、SPEC-0281、SPEC-0282、SPEC-0283、SPEC-0284 已 done 并合入 main |
| 前置 ADR | [ADR-0008](../../adr/accepted/0008-internal-value-and-allocation-abi.md)、[ADR-0016](../../adr/accepted/0016-interprocedural-borrow-abi.md) |
| 关联 ADR | 无新增 ABI |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` 容器成员与调用检查、所有权独占借用；`lang-codegen` SSA 原语、verifier、LLVM lowering 与 native 测试 |
| 语言语义变更 | 否（落实 Guide v0.41 §12 与顺序集合 M3A 动态列表变异原语） |

## 1. Goal

在单文件与 compilation unit 两入口交付 `MutableList<T>.insertAt(index: Int, element: T): Unit`：
- 调用者能够对 `MutableList<T>` 实例调用 `.insertAt(index, element)`，在指定索引处插入新元素：
  - 若 `index < size`，将原 `[index, size)` 范围的元素向后平移 1 个槽位至 `[index + 1, size + 1)`；
  - 若 `index == size`，新元素直接写入末尾（等价于 `add(element)`）；
  - `size` 递增 1；
- 索引边界：当 `index < 0` 或 `index > size` 时确定性运行时 abort；
- 容量与扩容：
  - 若插入前 `size == capacity`，触发几何扩容（若 `capacity == 0` 初始化为 4，否则翻倍为 `2 * capacity`），分配新缓冲区并迁移已有元素后再执行插入；
  - 若 `size < capacity`，就地使用既有缓冲区执行平移与写入；
- 元素所有权契约：
  - `MoveOnly` 元素将其所有权由实参处转移给容器管理；
  - `Copyable` 元素交付其副本；
  - 内存平移为底层 relocation，不执行多余 drop 亦无内存泄漏；
- 借用检查：receiver 要求 `Inout`（`Exclusive` 借用），调用期间与活跃元素借用互斥，编译期报告 L0135；
- 析构时：容器析构时逆序析构 `[0, size)` 范围内的全部现存有效元素，新插入的元素作为容器合法成员参与析构。

## 2. 背景与需求

Koven v0.41 Guide §12 明确规范：
> “`MutableList<T>` 是可增长、可缩减且元素可替换的独占 owning container；只有它提供改变 `size` 的操作。”
> “`MutableList<T>` 扩容或重排时可以把已初始化元素移动到新缓冲区。该操作是 relocation / move 而不是用户可观察的复制；旧位置不再拥有值。存在任何有效元素借用时，不得执行会改变元素地址或销毁该元素的扩容、缩容、删除、替换或重排。”

此前：
- SPEC-0280 完成了尾部追加扩容 `add(element)`；
- SPEC-0281 完成了整表清空 `clear()`；
- SPEC-0282 完成了指定索引移出 `removeAt(index)`；
- SPEC-0283 与 SPEC-0284 分别完成了双端移出 `removeLast()` 与 `removeFirst()`。

目前顺序集合（M3A）在任意位置插入元素的核心变异原语尚处于缺口状态。`MutableList.insertAt(index: Int, element: T): Unit` 是与 `removeAt(index)` 对偶的基础代数操作，补齐了动态数组的完整功能原语闭环。

## 3. 范围与技术方案

1. **Phase 2 类型检查**：
   - 在 `container_operations.rs` 与 `checker/container.rs` 中识别 `MutableList` 的预声明成员方法 `insertAt`；
   - 检查参数数量（恰好 2 个）与参数类型（第一个为 `Int`，第二个可赋值给 `T`）；
   - 返回类型为 `Unit`；
   - 不接受类型实参；
   - 对 `Array` 或 `List` 调用 `insertAt` 报告 `codes::INVALID_CONTAINER_MEMBER`。
2. **Phase 3 所有权检查**：
   - 对 receiver 建立 `Exclusive` (Inout) 借用，调用期间锁定容器及其所有元素；
   - `index` 参数为无标记值实参（`Value` 模式）；
   - `element` 参数遵循声明端 `own` 映射的 `Value` 契约（MoveOnly 转移所有权，Copyable 复制）；
   - 若存在未结束的元素借用或容器借用，报告 `codes::BORROW_CONFLICT` (L0135)。
3. **Phase 4 SSA 与 LLVM Native**：
   - SSA 操作：`Operation::ContainerInsertAt { owner: ValueId, index: ValueId, element: ValueId }`；
   - Verifier 校验：owner 必须是 `MutableList` 类型，index 为 Int 类型，element 为容器元素类型；指令返回更新后的容器值；
   - 所有权验证：检查 owner 活跃借用冲突，消费 owner、index 与 element，注册更新后的新容器值；
   - LLVM lowering：
     - 解构 3 字段 header struct: `{ buffer, size, capacity }`；
     - 边界检查：若 `index < 0 || index > size`，调用 abort；
     - 扩容检查：若 `size == capacity`，分配翻倍新缓冲区，将已有旧元素 `memcpy` 至新缓冲区并释放旧缓冲区；
     - 平移：若 `index < size` 且 `stride > 0`，使用 `memmove` 将 `active_buffer[index..size]` 向右平移 1 个槽位至 `active_buffer[index + 1..size + 1]`；
     - 写入：将 `element` 写入 `active_buffer[index]`；
     - 更新：`size = size + 1`，构造更新后的 header 返回。

## 4. 非目标

- 不支持批量范围插入（如 `insertAll`）；
- 不在 `insertAt` 中引入扩容衰减（decay）或缩小容量策略；
- 不引入异常展开。

## 5. 验收标准

- [x] G1: `MutableList<Int>.insertAt` 覆盖在头部（`index = 0`）、中间（`0 < index < size`）以及尾部（`index = size`）插入元素，验证列表元素顺序正确，`size` 递增。
- [x] G2: 当插入前 `size == capacity` 时（含从空列表 `capacity == 0` 开始连续插入），验证几何扩容正确执行，不丢失已存在元素且容量增长符合预期。
- [x] G3: `MutableList<Resource>.insertAt` 插入 MoveOnly 资源，验证资源所有权正确移入容器，容器退出时全部元素逆序析构，零内存泄漏与双重释放。
- [x] G4: 索引越界检查：对 `index < 0` 或 `index > size`（注意 `index == size` 合法）调用 `insertAt` 触发确定性运行时 abort。
- [x] G5: 借用冲突检查：在活跃元素借用期间调用 `.insertAt` 产生编译期 L0135 诊断。
- [x] G6: 针对 `Array` / `List` 调用 `.insertAt` 产生无效成员编译期诊断；错误参数数量或类型产生相应诊断。
- [x] G7: 双宿主（macOS arm64 / Linux x86_64）native 测试全绿，通过架构及尺寸门禁。

## 6. 验证记录

- **Phase 1 (规范与拓扑)**:
  - Commit: `ed2fd02`
  - 产物：`docs/specs/active/0285-mutable-list-insert-at.md`，更新拓扑并运行 `check_docs.py` 通过。
- **Phase 2 (类型检查)**:
  - Commit: `43e49b3`
  - 验证命令：
    - `cargo test -p lang-frontend --test type_containers` -> 32 passed; 0 failed
    - `cargo test -p lang-frontend --test multifile_type_checking` -> 144 passed; 0 failed
- **Phase 3 (所有权检查)**:
  - Commit: `b9306c0`
  - 验证命令：
    - `cargo test -p lang-frontend --test ownership_containers` -> 35 passed; 0 failed
- **Phase 4 (SSA Lowering 与 Native 执行)**:
  - Commit: `283ba05`
  - 验证命令：
    - `cargo test -p lang-codegen mutable_list_insert_at` -> 2 passed; 0 failed
    - `cargo test -p lang-codegen unit_container_insert_at` -> 4 passed; 0 failed
    - `cargo test -p lang-codegen container` -> 136 passed; 0 failed
    - `cargo clippy --all-targets` -> passed
    - `cargo fmt --check` -> passed
    - `python3 scripts/check_rust_sizes.py --base origin/main` -> passed
