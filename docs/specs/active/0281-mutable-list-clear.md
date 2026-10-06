# SPEC-0281: MutableList 逆序元素清理与缓冲区复用 (`MutableList.clear`)

> **性质**：变更合同 · **状态**：in-progress · **读取时机**：实施或评审 MutableList 清空操作时 · **唯一真源**：本 Spec

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P4-0281` |
| 所属 Phase | Phase 2 预声明成员识别；Phase 3 独占借用；Phase 4 SSA、逆序元素清理与 native 运行 |
| 语言规范 | 现行 [Guide v0.41](../../guide/README.md)；[集合与解构](../../guide/12-collections-destructuring.md) |
| 批准依据 | 用户持续推进里程碑授权；Guide v0.41 §12 与 §8 已规范 `MutableList.clear` 与逆序元素析构不变量 |
| 前置 Spec | SPEC-0280 已 done 并合入 main |
| 前置 ADR | [ADR-0008](../../adr/accepted/0008-internal-value-and-allocation-abi.md)、[ADR-0016](../../adr/accepted/0016-interprocedural-borrow-abi.md) |
| 关联 ADR | 无新增 ABI |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` 容器成员与调用检查、所有权独占借用；`lang-codegen` SSA 原语、verifier、LLVM lowering 与 native 测试 |
| 语言语义变更 | 否（落实 Guide v0.41 §12 既有规范） |

## 1. Goal

在单文件与 compilation unit 两入口交付 `MutableList<T>.clear(): Unit`：
- 调用者能够对 `MutableList<T>` 实例调用 `.clear()`，将容器中的已有元素从高索引到低索引逆序析构（包括 MoveOnly `Resource` 与堆字符串），并将 `size` 重置为 0；
- 保持已有分配的缓冲区和 `capacity` 不变，实现清空后再次追加的零额外重分配；
- 精确遵守所有权借用不变量：对 `MutableList` 的活跃借用（包括元素借用）与 `.clear` 互斥，编译期报告 L0135；
- 析构时：若清空后未再追加元素，容器退出作用域时仅释放缓冲区，不重复析构已清理的元素；若清空后重新追加元素，仅析构新追加的元素。

## 2. 背景与需求

Koven v0.41 Guide §12 明确规范：
> “`MutableList<T>` 是可增长、可缩减且元素可替换的独占 owning container；只有它提供改变 `size` 的操作。”
> “只有保持容器初始化不变量的显式操作（例如 `MutableList.removeAt`）或消费整个容器的迭代才能移出不可复制元素；现行标准库未在本页之外隐式获得其他 API。”

Guide §08:
> `inout fun clear(): Unit { this.size = 0 }`

此前 SPEC-0280 交付了 `MutableList.add` 顺序追加与几何翻倍扩容；为了使 `MutableList` 具备完整的生命周期与循环复用能力，必须打通 `MutableList.clear`，以支持高效且无内存泄漏的动态集合清空与重用。

## 3. 范围与技术方案

1. **Phase 2 类型检查**：
   - 在 `container_operations.rs` 中识别 `MutableList` 的预声明成员方法 `clear`；
   - 检查参数数量（恰好 0 个）；
   - 返回类型为 `Unit`；
   - 对 `Array` 或 `List` 调用 `clear` 报告 `codes::INVALID_CONTAINER_MEMBER`。
2. **Phase 3 所有权检查**：
   - 对 receiver 建立 `Exclusive` 借用，调用期间锁定容器及其所有元素；
   - 若存在未结束的元素借用或容器借用，报告 `codes::BORROW_CONFLICT` (L0135)；
   - 在 `CallReturn` 处发出 ASAP 清理检查。
3. **Phase 4 SSA 与 LLVM Native**：
   - SSA 操作：`Operation::ContainerClear { owner }`；
   - Verifier 校验：owner 必须是 `MutableList` 类型，消费旧 owner 并产生更新后的容器值；
   - LLVM lowering：
     - 解构 3 字段 header struct: `{ buffer, size, capacity }`；
     - 若 `size > 0` 且元素类型具备 drop glue，循环调用 drop glue 逆序释放 `[0, size)` 中的元素；
     - 更新 `size = 0`，保持 `buffer` 与 `capacity` 不变；
     - 组装更新后的 header 写回容器绑定。

## 4. 非目标

- 不实现任意索引删除（`removeAt(index)`）或局部切片截断；
- 不在 `clear()` 时强制缩小（shrink_to_fit）缓冲区；
- 不引入异常展开；所有清理操作必须是不可失败的确定性提交。

## 5. 验收标准

- [ ] G1: `MutableList<Int>.clear` 从非空列表清空后 `size == 0`，再次追加元素索引访问正确。
- [ ] G2: `MutableList<Resource>.clear` 与 `MutableList<String>.clear`：包含 `deinit` 的资源类型清空时按逆序准确析构，清空后作用域退出无二次释放与内存泄漏。
- [ ] G3: 活跃元素借用期间调用 `.clear` 产生编译期 L0135 诊断。
- [ ] G4: 针对 `Array` / `List` 调用 `.clear` 产生无效成员编译期诊断。
- [ ] G5: 双宿主（macOS arm64 / Linux x86_64）native 测试全绿，通过架构及尺寸门禁。

## 6. 验证记录

| 验收项 | 目标与过滤器 | 结果 | 证据 |
|:---|:---|:---|:---|
| 类型与成员检查 | `cargo test -p lang-frontend --test type_containers` | 待执行 | |
| 所有权借用冲突 | `cargo test -p lang-frontend --test ownership_containers` | 待执行 | |
| SSA 容器操作与 Verifier | `cargo test -p lang-codegen mutable_list_clear` | 待执行 | |
| 双宿主 Native 执行与析构 | `cargo test -p lang-codegen unit_container_clear` | 待执行 | |
