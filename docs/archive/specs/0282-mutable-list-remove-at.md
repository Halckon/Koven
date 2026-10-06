# SPEC-0282: MutableList 索引元素移出与剩余元素前移压缩 (`MutableList.removeAt`)

> **性质**：变更合同 · **状态**：done · **读取时机**：实施或评审 MutableList 元素移出操作时 · **唯一真源**：本 Spec

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P4-0282` |
| 所属 Phase | Phase 2 预声明成员识别；Phase 3 独占借用与元素移出；Phase 4 SSA、边界检查、内存平移与 native 运行 |
| 语言规范 | 现行 [Guide v0.41](../../guide/README.md)；[集合与解构](../../guide/12-collections-destructuring.md) |
| 批准依据 | 用户持续推进里程碑授权；Guide v0.41 §12 与 §8 已规范 `MutableList.removeAt` 语义与移出元素所有权契约 |
| 前置 Spec | SPEC-0280、SPEC-0281 已 done 并合入 main |
| 前置 ADR | [ADR-0008](../../adr/accepted/0008-internal-value-and-allocation-abi.md)、[ADR-0016](../../adr/accepted/0016-interprocedural-borrow-abi.md) |
| 关联 ADR | 无新增 ABI |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` 容器成员与调用检查、所有权独占借用；`lang-codegen` SSA 原语、verifier、LLVM lowering 与 native 测试 |
| 语言语义变更 | 否（落实 Guide v0.41 §12 既有规范） |

## 1. Goal

在单文件与 compilation unit 两入口交付 `MutableList<T>.removeAt(index: Int): T`：
- 调用者能够对 `MutableList<T>` 实例调用 `.removeAt(index)`，移出指定索引处的元素并将其所有权按值返回给调用方，`size` 递减 1；
- 索引边界：当 `index < 0` 或 `index >= size` 时确定性运行时 abort；
- 内存布局：将 `[index + 1, size)` 的剩余元素向前平移 1 个槽位，底层缓冲区与 `capacity` 保持不变；
- 元素所有权契约：
  - `MoveOnly` 元素移出所有权交付给调用点表达式，平移过程为底层 relocation，不执行多余 drop 亦无内存泄漏；
  - `Copyable` 元素交付其副本；
- 借用检查：receiver 要求 `Inout`（`Exclusive` 借用），调用期间与活跃元素借用互斥，编译期报告 L0135；
- 析构时：容器析构时仅逆序析构 `[0, size - 1)` 范围内的现存有效元素，已被移出的元素由其新宿主负责释放，绝不重复释放。

## 2. 背景与需求

Koven v0.41 Guide §12 明确规范：
> “`MutableList<T>` 是可增长、可缩减且元素可替换的独占 owning container；只有它提供改变 `size` 的操作。”
> “只有保持容器初始化不变量的显式操作（例如 `MutableList.removeAt`）或消费整个容器的迭代才能移出不可复制元素；现行标准库未在本页之外隐式获得其他 API。”
> “`MutableList<T>` 扩容或重排时可以把已初始化元素移动到新缓冲区。该操作是 relocation / move 而不是用户可观察的复制；旧位置不再拥有值。存在任何有效元素借用时，不得执行会改变元素地址或销毁该元素的扩容、缩容、删除、替换或重排。”

此前 SPEC-0280 完成了追加扩容，SPEC-0281 完成了整表清空与缓冲区复用。本 Spec 交付单元素移出与就地压缩，打通动态集合的基础生命周期闭环。

## 3. 范围与技术方案

1. **Phase 2 类型检查**：
   - 在 `container_operations.rs` 中识别 `MutableList` 的预声明成员方法 `removeAt`；
   - 检查参数数量（恰好 1 个）与类型（`Int`）；
   - 返回类型为容器的元素类型 `T`；
   - 对 `Array` 或 `List` 调用 `removeAt` 报告 `codes::INVALID_CONTAINER_MEMBER`。
2. **Phase 3 所有权检查**：
   - 对 receiver 建立 `Exclusive` (Inout) 借用，调用期间锁定容器及其所有元素；
   - 若存在未结束的元素借用或容器借用，报告 `codes::BORROW_CONFLICT` (L0135)；
   - 方法调用产物为 `Value`，调用方获得被移出元素的所有权；在 `CallReturn` 处发出 ASAP 清理检查。
3. **Phase 4 SSA 与 LLVM Native**：
   - SSA 操作：`Operation::ContainerRemoveAt { owner, index }`；
   - Verifier 校验：owner 必须是 `MutableList` 类型，index 为 Int 类型；指令返回移出的元素值以及更新后的容器值；
   - LLVM lowering：
     - 解构 3 字段 header struct: `{ buffer, size, capacity }`；
     - 边界检查：生成分支，若 `index < 0 || index >= size`，调用 abort；
     - 取出 `element = buffer[index]`；
     - 若 `index + 1 < size`，计算待平移字节大小 `(size - 1 - index) * elem_size`，使用 `memmove`（或重叠安全搬迁）将 `&buffer[index + 1]` 平移到 `&buffer[index]`；
     - 更新 `size = size - 1`，将更新后的 header 写回容器绑定；
     - 返回 `element`。

## 4. 非目标

- 不实现任意范围批量删除（`removeRange`）或条件过滤（`removeAll`）；
- 不在 `removeAt()` 时自动缩容（shrink）；
- 不引入异常展开。

## 5. 验收标准

- [x] G1: `MutableList<Int>.removeAt` 从头部（`index = 0`）、中间和尾部（`index = size - 1`）移出元素，验证返回值与后续元素平移正确，`size` 逐次减 1。
- [x] G2: `MutableList<Resource>.removeAt` 移出 MoveOnly 资源，验证移出资源生命周期由接收方接管，容器内剩余资源在容器退出时逆序析构，零内存泄漏与双重释放。
- [x] G3: 越界索引（`index < 0` 或 `index >= size`）触发确定性运行时 abort。
- [x] G4: 活跃元素借用期间调用 `.removeAt` 产生编译期 L0135 诊断。
- [x] G5: 针对 `Array` / `List` 调用 `.removeAt` 产生无效成员编译期诊断。
- [x] G6: 双宿主（macOS arm64 / Linux x86_64）native 测试全绿，通过架构及尺寸门禁。

## 6. 验证记录

| 验收项 | 目标与过滤器 | 结果 | 证据 |
|:---|:---|:---|:---|
| 类型与成员检查 | `cargo test -p lang-frontend --test type_containers` | 通过 | 81 项类型测试通过，覆盖 removeAt 参数及成员拦截 |
| 所有权借用冲突 | `cargo test -p lang-frontend --test ownership_containers` | 通过 | 26 项所有权测试通过，覆盖 Inout 独占借用及活跃元素借用冲突 L0135 |
| SSA 容器操作与 Verifier | `cargo test -p lang-codegen mutable_list_remove_at` | 通过 | 单文件与编译单元 lowering 测试全通过，生成 ContainerRemoveAt |
| 双宿主 Native 执行与析构 | `cargo test -p lang-codegen unit_container_remove_at` | 通过 | 4 项 native 测试全部通过：首/中/尾连续移出、MoveOnly 零泄漏析构、清空复用、越界 abort |
