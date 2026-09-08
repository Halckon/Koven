# Koven v2 interface 值与动态分发候选设计

> **性质**：非规范候选设计 · **状态**：未启用 · **读取时机**：仅在评审 v2 interface 值或动态分发时 · **唯一真源**：现行语义仍以 [v0.34 guide](../guide/README.md) 为准

本文不修改、取代或启用 Koven v0.34，也不批准 guide、Spec 或 ADR，不授权实现。

## 1. 结论摘要

v2 候选方向应以 Kotlin 风格的使用体验表达动态 interface，同时保留 Koven 对所有权和成本的
显式控制：

1. interface 名称出现在泛型 bound 或 supertype 位置时，继续表示**静态实现约束**；
2. interface 名称出现在 local、字段、参数、返回值、函数类型分量或容器元素等值类型位置时，
   候选语义是**动态存在类型（existential value）**；
3. 源语言以 `Shape` 作为唯一规范拼写，不要求用户写 `dyn Shape`；`dyn` 是否继续保留为关键字，
   留给未来 guide 决定，不同时提供两套等价表面语法；
4. 参数继续使用现有默认 `Borrow`、显式 `own` 和 `inout` 契约表达所有权，不把所有权包装进
   `Box<dyn ...>`；
5. `List<Shape>` 候选地表示拥有一组固定大小 existential handle 的异构集合，`Rc<Shape>`
   候选地表示共享拥有的动态 interface；
6. `<T : Shape>` 仍表示单态化静态分发。裸 `Shape` 与泛型 `T` 的差别应成为 Koven
   静态/动态分发的主要表面边界；
7. 泛型型变应晚于 existential 类型和 ABI，只为只读视图、消费者接口及函数类型引入最小规则；
8. Koven 的单态化泛型不需要照搬 Kotlin `reified`。未来若需要 downcast、序列化或插件注册，
   应独立设计显式 `TypeId` / `TypeInfo<T>` 能力。

现行 v0.34 仍按
[v0.34 一致性边界](../guide/15-conformance-and-staging.md)与[类型/接口规则](../guide/03-types-generics.md) 执行：v1 不存在
interface runtime value，本文不能作为实现依据。

## 2. 问题的第一性原理

单态化只回答“某个泛型实例中的 `T` 是什么”。它不能让一个运行期长度的连续容器直接内联
不同大小、对齐和析构规则的具体值。开放世界异构集合必须把每个元素规范化为固定大小表示，
并携带调用其行为所需的信息。

排除封闭 `enum class` 后，可用的实现家族包括 vtable、operation dictionary、闭包表或
`TypeId` 分支。它们表面名称不同，但都属于 existential/type-erasure 机制。HList、异构 tuple
只能表达编译期固定类型与长度；ECS/按类型分桶适合批处理，但不是 `List<Shape>` 的同一语义。

因此 v2 的核心问题不是“是否使用 `dyn` 这个词”，而是：

- 动态值由谁拥有，何时 drop；
- Borrow、Value 与 Inout 如何映射到 existential；
- 未知 payload 如何获得固定大小的可存储表示；
- 哪些 interface 方法能进入稳定的动态调用表；
- 哪些转换可能分配，以及源码如何让该成本可辨认。

## 3. 候选表面语义

### 3.1 静态约束与动态值按位置区分

```kotlin
interface Shape {
    fun area(): Double
}

// 默认 Borrow；动态分发，不取得 payload 所有权。
fun printArea(shape: Shape) {
    println(shape.area())
}

// 单态化；调用目标静态确定。
fun <T : Shape> printAreaStatic(shape: T) {
    println(shape.area())
}

// owned existential；不同分支可以交付不同具体实现。
fun createShape(round: Boolean): Shape =
    if (round) Circle(10.0) else Rectangle(10.0, 20.0)

class Scene(
    val primary: Shape,
    val shapes: List<Shape>
)
```

建议固定以下判别：

| 源码位置 | 候选含义 |
|---|---|
| `<T : Shape>` | 静态 interface requirement |
| `class Circle : Shape` | 静态 conformance 声明 |
| 参数 `shape: Shape` | Borrow existential |
| 参数 `own shape: Shape` | Value/owned existential |
| 参数 `inout shape: Shape` | exclusive、非 owning existential place |
| local、字段或返回值 `Shape` | owned existential |
| `List<Shape>` | 拥有 existential 元素的容器 |
| `Rc<Shape>` | 候选共享 existential owner；须另行封闭 ABI |

interface 名称因语法位置而承担两种相关但不相同的角色。Parser 不必为此新增 type syntax；
名称解析和类型检查必须把 static requirement 与 existential type 保存成不同内部类型，不能再次
折叠成同一个 `Nominal`。

### 3.2 contextual existential conversion

只有明确存在 interface expected type 时，才建议把具体值转换为 existential：

```kotlin
val shape: Shape = Circle(10.0)

val shapes: List<Shape> = listOf(
    Circle(10.0),
    Rectangle(10.0, 20.0)
)
```

不建议从无 expected type 的不同 concrete arguments 自动推导公共 interface：

```kotlin
// 不应静默推导为 List<Shape>。
val shapes = listOf(Circle(10.0), Rectangle(10.0, 20.0))
```

显式类型标注本身就是动态擦除和潜在分配的源码信号。这样可以保留 Kotlin 式自然用法，同时
避免编译器通过 common-supertype 推导悄悄改变存储表示。

形成 Borrow existential 时只建立调用期 loan，不复制或移动 payload。形成 owned existential
时沿用现有 Value delivery：`Copyable` concrete value 复制进新 payload，`MoveOnly` concrete
value 转移 owner，原 place 随后不可用。这里是表示改变的 existential conversion，不应被描述
成表示保持的普通 subtype upcast。

### 3.3 容器不是通过型变获得异构性

`List<Circle>` 的缓冲区按具体 `Circle` 表示单态化；`List<Shape>` 的缓冲区则保存固定大小的
existential handle。两者不是表示相同的类型，因此不建议允许隐式转换：

```kotlin
val circles: List<Circle> = loadCircles()
val shapes: List<Shape> = circles // 候选规则：拒绝
```

未来可以提供显式逐元素擦除，或只读的动态借用视图；二者必须分别定义求值、分配、loan 和
迭代成本，不能伪装成零成本泛型协变。

`listOf` 在 expected type 为 `List<Shape>` 时，应按源码顺序对每个元素执行一次 Value
existential conversion；索引读取仍只 Borrow 元素，消费式 remove 才返回 owned existential，
`MutableList.add`/replace 则继续按 Value delivery 取得新元素所有权。

## 4. 所有权与运行时表示

### 4.1 最小表示候选

首版 ABI 可从以下逻辑表示开始，精确字段顺序、对齐和调用约定仍须由 ADR 决定：

```text
BorrowInterface = { data_pointer, vtable_pointer }
OwnedInterface  = { owner_pointer, vtable_pointer }
SharedInterface = { control_block_pointer, vtable_pointer }

VTable = {
    method_slots...,
    drop_glue,
    layout_or_deallocation_metadata
}
```

- Borrow interface 不承担 drop，loan 不得超出来源 owner；
- owned interface 默认按 MoveOnly 处理，通过 vtable 精确 drop；
- `inout Shape` 指向完整 existential place，可以按现行 Inout 规则以另一个满足 `Shape` 的
  payload 替换整个值，但不能移出后留下未初始化洞；
- shared interface 是否采用专用 `Rc<Shape>` 表示、是否允许复用 control block，必须与现行
  `Rc<T>` ABI 一并设计；
- vtable 首版不应默认携带 `TypeId`、反射名称或 downcast 入口。

### 4.2 普通 class 与 value class

普通 `class` 已具有间接 owner。将它借用或移动进 interface value 时，可以复用现有 payload
allocation，只增加 interface vtable 信息，不应机械形成双重 `Box`。

`value class` 是内联值：

- 只在同步 Borrow 参数中使用时，可以用指向现有 place/temporary 的 fat borrow，不必分配；
- 进入字段、返回值、escaping closure 或 `List<Shape>` 时需要稳定的 owned representation；
- v2 首版可以使用间接 payload 作为简单语义基线，后续通过逃逸分析、栈提升或 small-buffer
  optimization 消除不可观察分配；
- 优化不得改变 move、drop、对象身份、求值顺序或其他已经定义的可观察语义。

该规则会成为现行“容器不自动增加间接层”的明确 v2 例外，必须先由新 guide 启用，不能作为
后端优化偷偷引入。

## 5. 动态兼容性门禁

interface 可以继续用于静态泛型，即使它不能形成动态值。仅当 interface 被用于值类型位置时，
检查以下动态兼容性：

- 所有使用点的 interface 类型实参完整且具体；
- 进入 vtable 的方法没有未绑定方法级类型参数；
- `Self` 不出现在无法擦除的参数、返回值或字段位置；
- receiver 的 Borrow、Value、Inout 模式具有确定调用 ABI；
- consuming receiver、默认方法和父 interface 方法具有确定 thunk/drop 行为；
- associated const 保持类型级静态访问，不占用实例 vtable slot；
- interface 继承的 slot 顺序、重复 requirement 合并和 upcast 规则确定且无歧义；
- 跨线程使用时，existential contract 能证明 payload 满足 `Transferable`；未来
  `Shareable` 另行定义。

诊断应指出具体不兼容成员，例如“`Factory` 不能作为值类型：`create<T>` 无法动态分发”，
而不是只报告缺少 `dyn` 关键字。

## 6. 型变与 `reified`

### 6.1 型变后置

型变决定类型关系，不提供 existential 存储。建议在动态 interface、容器元素表示和借用返回
规则稳定后，仅引入以下最小集合：

- 只读 Borrow view/producer 候选协变；
- consumer 候选逆变；
- 函数 Borrow 参数候选逆变、返回值候选协变；
- `inout`、可变容器、内部可变类型和拥有连续 `T` 存储的容器保持 invariant；
- `own` 参数的型变必须先证明不会改变 move/drop 契约，不沿用 Borrow 参数结论。

是否采用 Kotlin 的声明端 `out`/`in` 拼写，应由后续 guide 单独决定。首个型变设计不应同时
加入 use-site projection、star projection 和泛型型变推导。

### 6.2 不照搬 Kotlin `reified`

Koven 泛型实例已经单态化，编译器在实例内知道 `T` 的布局、drop glue 和静态 interface 实现，
因此不需要用 `reified` 修补 JVM 风格泛型擦除。

单态化不等于运行时 RTTI。如果未来确有 `is T`、downcast、通用序列化或插件注册需求，应另行
设计最小、显式、可裁剪的 `TypeId` / `TypeInfo<T>` 能力或 metadata dictionary。该能力不应
作为动态 interface vtable 的默认成本，也不应顺带启用全局反射。

## 7. 对比方案与取舍

| 方案 | 收益 | 代价或结论 |
|---|---|---|
| 每处写 `Box<dyn Shape>` | 动态性和分配较显眼 | Rust 化且重复所有权信息，不符合 Koven 表面目标 |
| 值位置直接写 `Shape` | Kotlin 风格；可复用现有参数 mode | **推荐候选**；必须严格定义 owned conversion 和潜在分配 |
| 声明 `dynamic interface Shape`，使用仍写 `Shape` | 在声明处一次性暴露动态约束 | 增加 interface 分类；静态可用但动态不兼容的成员组合更僵硬 |
| 标准库 `AnyShape` wrapper | 无需改变 interface type syntax | 建立第二套动态对象模型，长期与 interface/所有权重复 |
| 自动共同 interface 推导 | 最接近 GC 语言便利性 | 隐藏擦除、分配和表示变化，不建议 |
| `Any` + `TypeId` | 支持任意 payload/downcast | 丢失 `Shape` 静态能力保证，并提前引入 RTTI |
| 基类继承 | 传统 OO 用户熟悉 | 与 Koven 不支持 class 实现继承的护栏冲突 |

外部设计参照只用于理解取舍，不覆盖 Koven 规范：Kotlin 使用 declaration-site variance 与
inline `reified`；Rust 将 trait object 显式写为 `dyn Trait`；Swift 区分单一隐藏 concrete type
的 `some` 与 boxed existential 的 `any`；GHC existential constructor 展示了 payload 与操作
dictionary 打包的非 OO 形式。

- [Kotlin generics](https://kotlinlang.org/docs/generics.html)
- [Rust trait objects](https://doc.rust-lang.org/reference/types/trait-object.html)
- [Swift opaque and boxed protocol types](https://docs.swift.org/swift-book/documentation/the-swift-programming-language/opaquetypes/)
- [GHC existentially quantified data constructors](https://ghc.gitlab.haskell.org/ghc/doc/users_guide/exts/existential_quantification.html)

## 8. 建议的 v2 决策与实施顺序

本文只提供后续门禁，不创建或批准 Spec：

1. **新 guide**：明确 interface 在 static requirement/value type 两类位置的语义、canonical
   surface syntax、contextual conversion、动态兼容性和禁止隐式 common-interface 推导；
2. **runtime ABI ADR**：决定 Borrow/Owned/Shared existential layout、vtable slot、drop、父接口
   upcast、value-class payload 策略和 nullable niche；
3. **Phase 2 Spec**：建立独立 existential type identity、动态兼容性诊断、expected-type conversion
   与 overload 规则；
4. **Phase 3 Spec**：接入 Borrow/Value/Inout、move/drop、容器元素 place、closure capture 和
   `Transferable`；
5. **Phase 4 Spec**：增加 typed SSA existential operations、verifier、vtable emission、indirect
   call、drop 和 native 正反验收；
6. **容器集成 Spec**：启用 `List<Shape>`/`MutableList<Shape>` construction、index、replace、remove
   与精确析构；
7. **独立后继设计**：型变、共享 `Rc<Shape>`、RTTI/downcast、FFI 动态对象 ABI 和优化分别立项，
   不与首个动态 interface Goal 混合。

## 9. 新 guide 前必须回答的问题

- `dyn` 是继续作为未来保留字、移出关键字表，还是承担非等价的高级语义；
- owned value-class existential 的基线是否允许语义上的隐式 allocation；
- `Rc<Shape>` 是共享 payload/control block 的专用表示，还是 `Rc` 包裹 owned existential；
- 父 interface upcast 是否零成本，是否允许多父接口及怎样保证 slot 稳定；
- `own this`、`inout this` 和返回 `Self` 分别允许哪些动态调用；
- existential 是否默认 MoveOnly，以及能力接口如何约束 `Transferable`；
- 动态 interface 是否参与 `==`/hash，若参与由哪个显式接口提供；
- ABI 是否只保证编译单元内部稳定；公共 C FFI 不应默认暴露 Koven vtable。

上述问题未由用户明确启用的新 guide 回答前，现有“裸 interface 不是 runtime value type”的
实现与诊断应保持不变。
