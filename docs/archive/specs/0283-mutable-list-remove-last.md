# SPEC-0283: MutableList 尾部元素快速移出 (`MutableList.removeLast`)

> **性质**：变更合同 · **状态**：done · **读取时机**：追溯 MutableList 尾部元素移出操作设计与实现时 · **唯一真源**：本 Spec

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P4-0283` |
| 所属 Phase | Phase 2 预声明成员识别；Phase 3 独占借用与元素移出；Phase 4 SSA、边界检查与 native 运行 |
| 语言规范 | 现行 [Guide v0.41](../../guide/README.md)；[集合与解构](../../guide/12-collections-destructuring.md) |
| 批准依据 | 用户持续推进里程碑授权；Guide v0.41 §12 与 §8 已规范 `MutableList.removeLast` 语义与移出元素所有权契约 |
| 前置 Spec | SPEC-0280、SPEC-0281、SPEC-0282 已 done 并合入 main |
| 前置 ADR | [ADR-0008](../../adr/accepted/0008-internal-value-and-allocation-abi.md)、[ADR-0016](../../adr/accepted/0016-interprocedural-borrow-abi.md) |
| 关联 ADR | 无新增 ABI |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` 容器成员与调用检查、所有权独占借用；`lang-codegen` SSA 原语、verifier、LLVM lowering 与 native 测试 |
| 语言语义变更 | 否（落实 Guide v0.41 §12 与顺序集合 M3A 既有规范） |

## 1. Goal

在单文件与 compilation unit 两入口交付 `MutableList<T>.removeLast(): T`：
- 调用者能够对 `MutableList<T>` 实例调用无参 `.removeLast()`，移出末尾槽位（`size - 1`）处的元素并将其所有权按值返回给调用方，`size` 递减 1；
- 边界检查：当 `size == 0` 时确定性运行时 abort；
- 内存性能优势：相比 `removeAt(index)`，移出末尾元素无需任何后续元素平移（无需 `memmove`），时间复杂度为精确的 $O(1)$，底层缓冲区与 `capacity` 保持不变；
- 元素所有权契约：
  - `MoveOnly` 元素移出所有权交付给调用点表达式，不执行多余 drop 亦无内存泄漏；
  - `Copyable` 元素交付其副本；
- 借用检查：receiver 要求 `Inout`（`Exclusive` 借用），调用期间与活跃元素借用互斥，编译期报告 L0135；
- 析构时：容器析构时仅逆序析构 `[0, size - 1)` 范围内的现存有效元素，已被移出的末尾元素由其新宿主负责释放，绝不重复释放。

## 2. 背景与需求

Koven v0.41 Guide §12 明确规范：
> “`MutableList<T>` 是可增长、可缩减且元素可替换的独占 owning container；只有它提供改变 `size` 的操作。”
> “只有保持容器初始化不变量的显式操作（例如 `MutableList.removeAt`）或消费整个容器的迭代才能移出不可复制元素；现行标准库未在本页之外隐式获得其他 API。”

在顺序集合（M3A）日常使用场景（如栈结构模拟、逆序弹出、缓冲区削减等）中，从尾部弹出元素是最频繁的缩容操作。虽然 `removeAt(size - 1)` 亦可完成尾部移出，但提供原生的 `.removeLast()` 具有显著价值：
1. 语言表达更简洁直观，无需调用者显式书写 `list.removeAt(list.size - 1)`；
2. 避免了潜在的动态计算下溢风险；
3. 后端执行路径高度优化：直接计算 `size - 1` 索引并取出，跳过任何内存平移检查与 `memmove` 指令，是常数时间 $O(1)$ 经典弹出。

## 3. 范围与技术方案

1. **Phase 2 类型检查**：
   - 在 `container_operations.rs` 与 `checker/container.rs` 中识别 `MutableList` 的预声明成员方法 `removeLast`；
   - 检查参数数量（恰好 0 个）；
   - 返回类型为容器的元素类型 `T`；
   - 对 `Array` 或 `List` 调用 `removeLast` 报告 `codes::INVALID_CONTAINER_MEMBER`。
2. **Phase 3 所有权检查**：
   - 对 receiver 建立 `Exclusive` (Inout) 借用，调用期间锁定容器及其所有元素；
   - 若存在未结束的元素借用或容器借用，报告 `codes::BORROW_CONFLICT` (L0135)；
   - 方法调用产物为 `Value`，调用方获得被移出元素的所有权；在 `CallReturn` 处发出 ASAP 清理检查。
3. **Phase 4 SSA 与 LLVM Native**：
   - SSA 操作：`Operation::ContainerRemoveLast { owner }`；
   - Verifier 校验：owner 必须是 `MutableList` 类型；指令返回移出的元素值以及更新后的容器值；
   - LLVM lowering：
     - 解构 3 字段 header struct: `{ buffer, size, capacity }`；
     - 边界检查：若 `size == 0`，调用 abort；
     - 索引固定为 `last_index = size - 1`；
     - 取出 `element = buffer[last_index]`；
     - 更新 `size = size - 1`，将更新后的 header 写回容器绑定；
     - 返回 `element`。

## 4. 非目标

- 不支持传入可选索引或参数（有参删除由 `removeAt(index)` 承担）；
- 不在 `removeLast()` 时自动缩容（shrink）；
- 不引入异常展开。

## 5. 验收标准

- [x] G1: `MutableList<Int>.removeLast()` 连续从尾部弹出元素，验证返回值正确，`size` 逐次减 1，直至变为空列表。
- [x] G2: `MutableList<Resource>.removeLast()` 移出 MoveOnly 资源，验证移出资源生命周期由接收方接管，容器内剩余资源在容器退出时逆序析构，零内存泄漏与双重释放。
- [x] G3: 对空 `MutableList`（`size == 0`）调用 `removeLast()` 触发确定性运行时 abort。
- [x] G4: 活跃元素借用期间调用 `.removeLast()` 产生编译期 L0135 诊断。
- [x] G5: 针对 `Array` / `List` 调用 `.removeLast()` 产生无效成员编译期诊断。
- [x] G6: 双宿主（macOS arm64 / Linux x86_64）native 测试全绿，通过架构及尺寸门禁。

## 6. 验证记录

- **Phase 1 (规范与拓扑)**:
  - Commit: `eefa0ee`
  - 产物：`docs/specs/active/0283-mutable-list-remove-last.md`，更新拓扑并运行 `check_docs.py` 通过。
- **Phase 2 (类型检查)**:
  - Commit: `b6db76f`
  - 验证命令：
    - `cargo test -p lang-frontend type_containers` -> 23 passed; 0 failed
    - `cargo test -p lang-frontend multifile_type_checking` -> 140 passed; 0 failed
- **Phase 3 (所有权检查)**:
  - Commit: `15c4a78`
  - 验证命令：
    - `cargo test -p lang-frontend ownership_containers` -> 29 passed; 0 failed
- **Phase 4 (SSA Lowering 与 Native 执行)**:
  - Commit: `5c462ed`
  - 验证命令：
    - `cargo test -p lang-codegen container_lowering` -> 13 passed; 0 failed
    - `cargo test -p lang-codegen unit_lower_container` -> 17 passed; 0 failed
    - `cargo test -p lang-codegen unit_container_remove_last` -> 4 passed; 0 failed
    - `cargo clippy -p lang-codegen --all-targets -- -D warnings` -> passed
    - `cargo fmt --check` -> passed
    - `python3 scripts/check_rust_sizes.py --base origin/main` -> passed
