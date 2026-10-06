# SPEC-0284: MutableList 头部元素快速移出与队列弹出 (`MutableList.removeFirst`)

> **性质**：变更合同 · **状态**：done · **读取时机**：追溯 MutableList 头部元素移出与队列弹出操作设计与实现时 · **唯一真源**：本 Spec

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P4-0284` |
| 所属 Phase | Phase 2 预声明成员识别；Phase 3 独占借用与元素移出；Phase 4 SSA、边界检查与 native 运行 |
| 语言规范 | 现行 [Guide v0.41](../../guide/README.md)；[集合与解构](../../guide/12-collections-destructuring.md) |
| 批准依据 | 用户持续推进里程碑授权；Guide v0.41 §12 与 §8 动态容器变异操作 |
| 前置 Spec | SPEC-0280、SPEC-0281、SPEC-0282、SPEC-0283 已 done 并合入 main |
| 前置 ADR | [ADR-0008](../../adr/accepted/0008-internal-value-and-allocation-abi.md)、[ADR-0016](../../adr/accepted/0016-interprocedural-borrow-abi.md) |
| 关联 ADR | 无新增 ABI |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` 容器成员与调用检查、所有权独占借用；`lang-codegen` SSA 原语、verifier、LLVM lowering 与 native 测试 |
| 语言语义变更 | 否（落实 Guide v0.41 §12 与顺序集合 M3A 队列消费原语） |

## 1. Goal

在单文件与 compilation unit 两入口交付 `MutableList<T>.removeFirst(): T`：
- 调用者能够对 `MutableList<T>` 实例调用无参 `.removeFirst()`，移出首个槽位（索引 `0`）处的元素并将其所有权按值返回给调用方，剩余元素 `[1, size)` 向左平移一位填补空缺，`size` 递减 1；
- 边界检查：当 `size == 0` 时确定性运行时 abort；
- 元素所有权契约：
  - `MoveOnly` 元素移出所有权交付给调用点表达式，不执行多余 drop 亦无内存泄漏；
  - `Copyable` 元素交付其副本；
- 借用检查：receiver 要求 `Inout`（`Exclusive` 借用），调用期间与活跃元素借用互斥，编译期报告 L0135；
- 析构时：容器析构时仅逆序析构 `[0, size - 1)` 范围内的现存有效元素，已被移出的首部元素由其新宿主负责释放，绝不重复释放。

## 2. 背景与需求

Koven v0.41 Guide §12 明确规范：
> “`MutableList<T>` 是可增长、可缩减且元素可替换的独占 owning container；只有它提供改变 `size` 的操作。”
> “只有保持容器初始化不变量的显式操作（例如 `MutableList.removeAt`）或消费整个容器的迭代才能移出不可复制元素；现行标准库未在本页之外隐式获得其他 API。”

在顺序集合（M3A）日常使用场景（如队列 Queue、广度优先遍历 BFS、任务工作表 Worklist 等）中，从头部弹出元素是最典型的 FIFO 消费模式。虽然 `removeAt(0)` 亦可完成该操作，但原生提供无参 `.removeFirst()` 具有显著价值：
1. 语言表达自解释、对称且直观：与 `removeLast()` 共同构成双端消费的核心对偶原语；
2. 无需显式传递魔数索引 `0`，调用更简洁安全；
3. 后端执行路径高度专化：索引确定为常量 0，边界检查只需确认 `size > 0`，无需运行时检查负索引或上界计算；首元素直接读取后对 `[1, size)` 执行整块向左 `memmove` 前移。

## 3. 范围与技术方案

1. **Phase 2 类型检查**：
   - 在 `container_operations.rs` 与 `checker/container.rs` 中识别 `MutableList` 的预声明成员方法 `removeFirst`；
   - 检查参数数量（恰好 0 个）；
   - 返回类型为容器的元素类型 `T`；
   - 对 `Array` 或 `List` 调用 `removeFirst` 报告 `codes::INVALID_CONTAINER_MEMBER`。
2. **Phase 3 所有权检查**：
   - 对 receiver 建立 `Exclusive` (Inout) 借用，调用期间锁定容器及其所有元素；
   - 若存在未结束的元素借用或容器借用，报告 `codes::BORROW_CONFLICT` (L0135)；
   - 方法调用产物为 `Value`，调用方获得被移出首元素的所有权；在 `CallReturn` 处发出 ASAP 清理检查。
3. **Phase 4 SSA 与 LLVM Native**：
   - SSA 操作：`Operation::ContainerRemoveFirst { owner }`；
   - Verifier 校验：owner 必须是 `MutableList` 类型；指令返回移出的首元素值以及更新后的容器值；
   - LLVM lowering：
     - 解构 3 字段 header struct: `{ buffer, size, capacity }`；
     - 边界检查：若 `size == 0`，调用 abort；
     - 取出首元素 `element = buffer[0]`；
     - 若 `size > 1`，使用 `memmove` 将 `buffer[1..size]` 移动到 `buffer[0..size - 1]`；
     - 更新 `size = size - 1`，将更新后的 header 写回容器绑定；
     - 返回 `element`。

## 4. 非目标

- 不支持传入可选索引或参数（有参删除由 `removeAt(index)` 承担）；
- 不在 `removeFirst()` 时自动缩容底层缓冲区（底层 capacity 保持不变）；
- 不引入环形缓冲区（Ring Buffer）实现（v1 顺序容器按连续线性缓冲区定义）；
- 不引入异常展开。

## 5. 验收标准

- [x] G1: `MutableList<Int>.removeFirst()` 连续从头部弹出元素，验证 FIFO 顺序与返回值正确，`size` 逐次减 1，剩余元素索引正确前移，直至变为空列表。
- [x] G2: `MutableList<Resource>.removeFirst()` 移出 MoveOnly 资源，验证移出资源生命周期由接收方接管，容器内剩余资源在容器退出时逆序析构，零内存泄漏与双重释放。
- [x] G3: 对空 `MutableList`（`size == 0`）调用 `removeFirst()` 触发确定性运行时 abort。
- [x] G4: 活跃元素借用期间调用 `.removeFirst()` 产生编译期 L0135 诊断。
- [x] G5: 针对 `Array` / `List` 调用 `.removeFirst()` 产生无效成员编译期诊断。
- [x] G6: 双宿主（macOS arm64 / Linux x86_64）native 测试全绿，通过架构及尺寸门禁。

## 6. 验证记录

- **Phase 1 (规范与拓扑)**:
  - Commit: `430eb9b`
  - 产物：`docs/specs/active/0284-mutable-list-remove-first.md`，更新拓扑并运行 `check_docs.py` 通过。
- **Phase 2 (类型检查)**:
  - Commit: `9a11420`
  - 验证命令：
    - `cargo test -p lang-frontend --test type_containers` -> 27 passed; 0 failed
    - `cargo test -p lang-frontend --test multifile_type_checking` -> 142 passed; 0 failed
- **Phase 3 (所有权检查)**:
  - Commit: `d1a3f0f`
  - 验证命令：
    - `cargo test -p lang-frontend --test ownership_containers` -> 32 passed; 0 failed
- **Phase 4 (SSA Lowering 与 Native 执行)**:
  - Commit: `79df49f`
  - 验证命令：
    - `cargo test -p lang-codegen --lib container_lowering_tests` -> 14 passed; 0 failed
    - `cargo test -p lang-codegen --lib unit_lower_container_tests` -> 13 passed; 0 failed
    - `cargo test -p lang-codegen --lib container_remove_first_tests` -> 4 passed; 0 failed
    - `cargo clippy -p lang-frontend --all-targets -- -D warnings` -> passed
    - `cargo clippy -p lang-codegen --all-targets -- -D warnings` -> passed
    - `cargo fmt --check` -> passed
    - `python3 scripts/check_rust_sizes.py --base origin/main` -> passed
