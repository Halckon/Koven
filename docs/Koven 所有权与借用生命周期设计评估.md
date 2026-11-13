# Koven 所有权与借用生命周期设计评估

评估基准：2026 年 11 月 7 日 · Koven Guide v0.47 · GitHub 主仓库当前规范及实现文档

## 一、总体结论：Koven 的方向合理，但它选择了一条不同于 Rust 的复杂性分配路径

结合 Koven 当前的所有权规范、编译器架构、借用返回、资源析构和 E2/E3 细化层设计，我认为：

Koven 最有价值的设计，不是把 Rust 的生命周期语法隐藏起来，而是通过缩小可表达的借用范围，尝试让大部分应用开发不需要面对 Rust 式的生命周期问题。

这个方向值得继续，而且与 Koven 追求 Kotlin 风格开发体验、原生性能、无追踪式 GC 的定位一致。

但需要明确三个判断：

1. 开发者心智负担有机会明显低于 Rust。 Koven 默认借用、调用期 Loan、唯一来源的 Borrow Return、不提供任意可存储引用，能够消除很多 Rust 用户需要理解的生命周期组合。
2. 编译器实现未必比 Rust 简单。 Koven 的 ASAP 析构、资源类型词法析构、可变位置、闭包捕获、控制流所有权运输，以及限定借用结果的终止证明，已经形成相当复杂的系统。
3. 当前完备性与 Rust 存在显著差距。 Koven 已有大量真实前端、SSA、LLVM/native 的有界支持，但仍有不小的组合能力缺口。不能把某一套测试通过等同于完整生命周期模型已得到验证。

我认为 Koven 未来最合适的路线是：

保留现有的轻量显式所有权模型，借鉴 Swift 的渐进式所有权和 Mojo 的 Origin 来源表达方式，再引入有限的 NLL 能力；不建议直接复制 Rust 完整生命周期泛型体系。

### 综合设计评分

下面是针对语言设计方向的主观工程评估，不是性能跑分，也不是形式化安全证明。

日常开发心智

8/10

默认 Borrow，较少生命周期标注

设计一致性

8/10

单一所有者与来源约束明确

潜在编译器可维护性

5/10

生命周期和 Drop 规划责任偏复杂

当前实现成熟度

5/10

大量有界能力，完整组合仍待验证

安全模型的理论潜力

8/10

保守拒绝和独占借用提供良好基础

通用系统编程表达能力

6/10

缺乏可存储引用与通用生命周期表达

这里有一个重要区分：设计潜力较高，并不意味着当前实现的安全性已经得到同等级验证。

## 二、Koven 当前所有权设计是否合理？

### 2.1 核心模型：Owned Value + Borrow + Inout

Koven 的当前 Guide 使用三种主要参数语义：

| 模式     | 语义             | 所有权变化                   | 开发者需要考虑         |
| -------- | ---------------- | ---------------------------- | ---------------------- |
| `own`    | 接收独立拥有的值 | MoveOnly 转移、Copyable 复制 | 调用后原值是否还能使用 |
| `borrow` | 共享只读借用     | 不转移                       | 借用期间不能破坏原值   |
| `inout`  | 独占可变借用     | 不转移                       | 必须保证独占访问       |

无标记参数默认为 `borrow`，而 `inout` 调用使用 `&place` 表示可变位置。

以下代码为设计示意，展示参数约定，并非逐条已在当前 CLI 验证的测试程序。

```
fun inspect(item: Data) {
    // 默认只读借用
}

fun update(inout item: Data) {
    // 独占修改
}

fun consume(own item: Data) {
    // 接管所有权
}
```

在使用端：

```
inspect(data)
update(&data)
consume(data)
```

与 Rust 对比：

```
fn inspect(item: &Data) {}
fn update(item: &mut Data) {}
fn consume(item: Data) {}

inspect(&data);
update(&mut data);
consume(data);
```

### 我的评价

这种设计的合理性较高。

它把参数的主要借用意图放在声明端，而不是要求调用者在每次只读调用时使用 `&`。

对日常函数而言，很多开发者只需要理解：

- 普通参数用于读取；
- `inout` 用于修改；
- `own` 用于转移或接收独立值。

与 Rust 相比，这降低了调用点的视觉负担，也能减少无意义的所有权选择。

但它并没有彻底消除心智负担：

当需要把值存入对象、返回给调用者、传递到另一个线程，或者在闭包中长期保存时，开发者仍然需要认识到“谁拥有这个值”。

这种限制是必要的。否则只是将生命周期错误从编译期移到运行期。

### 2.2 Koven 通过限制借用范围来降低复杂度

这是 Koven 与 Rust 最大的设计分歧。

Rust

引用与生命周期

可作为类型关系表达

`&T` / `&mut T`

生命周期泛型、引用字段

广泛的零复制组合能力

Koven

调用期 Loan

借用范围优先由调用确定

`borrow` / `inout`

唯一来源 `from` 借用返回

有限的零复制借用组合

设计取舍：Rust 把生命周期关系提升为通用类型能力；Koven 主要把它约束在函数调用和受限的局部借用关系中。

Koven 不提供以下通用能力：

- 任意可存储的引用类型；
- 用户定义的生命周期类型参数；
- 可以随意放入类或集合的借用字段；
- 任意跨控制流传播的借用值。

而是通过特定的 Borrow Return 合同处理常见场景：

```
fun view(borrow source: T): borrow U from source
```

以及：

```
borrow val item = view(source)
```

这里的 `from source` 很重要。

它让函数的返回值可以与一个明确的来源关联，编译器不必推导任意多来源生命周期之间的复杂关系。

这种设计实际上更接近 受限的 Origin-based Borrowing。

我赞成这一方向，特别是作为 v1 的正式语言边界。

但是必须接受代价：某些可以在 Rust 中安全表达的数据结构，在 Koven 中必须改用拥有值、索引、句柄或者共享所有权对象实现。

这不是简单的实现缺口，而是设计范围上的主动取舍。

### 2.3 Copyable / MoveOnly / Transferable 的区分

Koven 对能力进行结构推导，方向也是合理的。

它不要求开发者给所有普通数据类型编写移动、复制或跨线程转移实现，而是按字段和内建类型规则推导。

尤其应该保留以下区分：

| 能力           | 解决的问题                           |
| -------------- | ------------------------------------ |
| `Copyable`     | 是否允许产生独立的值副本             |
| `MoveOnly`     | 是否需要唯一所有权转移和清理         |
| `Transferable` | 是否允许把所有权安全转移到另一个线程 |

需要强调，Copyable 不等于栈分配，也不等于浅拷贝，更不等于没有运行时成本。

例如 Koven 的 `value class` 是一种值语义、平坦布局方向的类型，但在实际编译时，内联、寄存器分配或溢出到栈，仍需要由布局、逃逸分析及优化决定。

另外，Koven 现行 `Transferable` 只能承担类似 Rust `Send` 的角色，不能同时代表 `Sync`。

Guide v0.47 已明确把跨线程共享能力 `Shareable` 推迟到后续版本。这个决策合理：在共享原语和线程安全引用计数尚未完整定义之前，不应该过早承诺线程间共享安全。

### 2.4 双轨析构：合理，但也是当前设计中最大的复杂度来源之一

Koven 使用：

- Pure Memory：尽可能早地析构；
- Resource：维持到词法作用域结束，或显式消费时结束。

这一设计解决了一个非常现实的问题。

```
fun work() {
    val guard = mutex.lock()
    updateState()
    // guard 在作用域结束时才释放
}
```

如果单纯按照最后一次显式使用进行 ASAP Drop，那么 `guard` 可能过早解锁。

Koven 使用 Resource 词法析构，避免这种错误。

同时，对于普通 String、集合等纯内存对象，可以在最后一次安全使用后尽快释放资源占用。

这个设计是有实际价值的，尤其适合同时追求应用开发便利性与原生资源管理的语言。

但它也引入了一项成本：编译器不能仅依赖一个统一的“变量最后使用点”，而必须判断类型的传递性资源属性、Owner 的可用性，以及不同控制流上的 Drop 时机。

这部分复杂度我会在下面重点讨论。

## 三、当前 Koven 的所有权实现到了什么程度？

这里必须把三件事分开：

1. Guide 定义了什么；
2. 前端是否可以正确分析并产生事实；
3. SSA/LLVM 是否能够正确执行这些事实，并经过真实运行验证。

我核对了当前主仓库的 [所有权架构](https://github.com/Halckon/Koven/blob/main/docs/architecture/ownership.md?utm_source=chatgpt.com)、[所有权细化层](https://github.com/Halckon/Koven/blob/main/docs/architecture/ownership-refinement.md?utm_source=chatgpt.com) 和 [SPEC-0341](https://github.com/Halckon/Koven/blob/main/docs/specs/active/0341-shared-lowering-core.md?utm_source=chatgpt.com)。

### 3.1 当前实现能力评估

| 能力                   | 当前情况                                      | 评价                     |
| ---------------------- | --------------------------------------------- | ------------------------ |
| Move / Copy 判断       | 已有生产前端和定向测试                        | 核心已建立               |
| Shared Borrow          | 调用期借用已接入                              | 较成熟                   |
| Inout Exclusive Borrow | 具备两阶段 Receiver 借用及部分复杂 place 处理 | 有界成熟                 |
| Borrow Return          | 单一 `from`、显式局部绑定、同步转发有实现     | 有界支持                 |
| Field / Index Borrow   | 支持若干稳定来源及元素 place                  | 组合缺口仍多             |
| Closure Capture        | 已有 capture 与逃逸检查、来源事实             | 复杂循环运输仍有缺口     |
| Pure Memory ASAP Drop  | 已有路径感知规划及事实输出                    | 需要继续证明组合正确性   |
| Resource Lexical Drop  | 已有 Resource 分类、deinit 和清理             | 已覆盖多个典型场景       |
| CFG / Loop 所有权合流  | 已实现多类有限场景                            | 非任意 CFG 完备          |
| 完整 NLL               | 明确未作为 v1 目标                            | 设计不提供               |
| 任意可存储引用         | 当前不支持                                    | 有意限制                 |
| 跨线程转移             | `Transferable` 规则已定义                     | 完整线程安全体系尚未交付 |

这张表不等于经过独立审计的实现完成清单，而是根据当前规范、Architecture 和测试说明判断出的能力边界。

### 3.2 当前编译器实际比表面设计复杂

Koven 已经形成多层的所有权处理结构：

Source / AST / Type Facts

语法、类型、调用参数契约、place 身份

Ownership Checking — Phase 3

Move / Borrow / Inout / Capture / Drop来源、可用性、终止点、控制流事实

Validated Ownership Facts

通过验证的只读所有权计划

SSA Refinement — Phase 4

CFG、slot、loan end、drop、owner transport

Typed SSA → Verifier → LLVM

原生内存及资源执行

这种分层本身是正确的。

尤其是两个设计值得保留：

第一，前端负责判断是否合法。 Codegen 不应该自行推导一个尚未被验证的 Borrow 或 Drop 行为。

第二，所有权事实是编译阶段之间的明确合同。 SSA Refinement 消费合法事实，将抽象的 owner、loan、cleanup 转为具体控制流和执行操作。

但当前有一个明显的工程风险：在 Single/Unit 迁移过程中，前端分析、owner transport 观察、细化层和旧 lowering 仍存在部分重叠责任。

如果这些重叠长期存在，每增加一个语言能力，都可能需要同时更新多个位置。

### 3.3 E2、E3 和 D10 对所有权完备性的影响

按照目前的架构，我建议这样理解三者的角色：

| 工作                | 主要解决什么                       | 对所有权的意义   |
| ------------------- | ---------------------------------- | ---------------- |
| E2 / 前端合同对账   | 所有权事实、来源与清理证据是否正确 | 保证判定可信     |
| E3 / SSA Refinement | 把合法的所有权语义转为可执行 SSA   | 保证执行正确     |
| D10 / 统一管线      | 收敛 Single / Unit 重复实现        | 避免长期语义分叉 |

这里的 E2 是依据你此前项目审计分工的归纳；仓库 SPEC-0341 也明确说过原始 E2/E3 审计正文尚需单独对账，不能把此归纳直接当作正式验收定义。

有一个新进展值得注意：

当前主线的 `ownership-refinement.md` 已记录大量 SPEC-0370～0390 的细化层迁移与旧路径退役。这说明 Koven 不是停留在“已经有一个 refine 原型”的状态，而是在持续将多个语义家族迁移到统一的细化框架。

但是 SPEC-0341 仍标记 `in-progress`，近期所有权相关 Draft PR 也继续明确区分静态检查与动态验证。

因此，不能仅凭细化层覆盖范围扩大，就宣布整个所有权管线已经完成统一、完全正确或者安全闭环。

当前最重要的工作，不是引入更多所有权语法，而是确保既有语义具有唯一、稳定、可验证的实现路径。

## 四、Koven 当前生命周期设计的真正复杂性

### 4.1 生命周期难度不只取决于有没有 `'a`

Rust 的生命周期系统经常被认为复杂，是因为用户需要处理生命周期参数。

但这是表现形式，而不是全部复杂性。

真正的问题包括：

- 值的所有者是谁？
- 同时存在几个别名？
- 哪些别名可以写？
- 借用能持续到什么时候？
- 控制流合并后值是否依然存在？
- 析构能否安全执行？
- 值被移动后，原来与它关联的引用会怎样？

Koven 删除了用户生命周期泛型，并没有删除这些语义问题。

它实际上将问题重新分配了：

| 复杂性          | Rust                       | Koven                    |
| --------------- | -------------------------- | ------------------------ |
| 局部 Move       | 编译器检查                 | 编译器检查               |
| 函数调用 Borrow | 显式或自动引用             | 默认 Borrow              |
| 多函数引用关系  | 生命周期类型关系           | 有界 `from` 来源         |
| 分支 Borrow     | NLL 分析                   | 有界来源与终止证明       |
| 可存储引用      | 支持并检查                 | 不支持                   |
| Borrow 与循环   | 通用 MIR 分析              | 有界 provider / CFG 规则 |
| Drop 时机       | 主要按所有权与词法析构语义 | 双轨 ASAP / Lexical      |
| 复杂泛型借用    | 生命周期与 trait 约束      | 大量形式尚不可表达       |

由此可以得出：

Koven 的复杂性上限因为语言能力受限而降低，但当前某些局部实现细节仍然相当复杂。

### 4.2 双轨 Drop 是最值得重新审视的设计

我赞成 Koven 区分 Pure 与 Resource 的意图，但不认为当前双轨模型已经是最简单的终态。

考虑：

```
class Connection {
    fun deinit() {
        close()
    }
}
```

以及：

```
class Wrapper {
    val connection: Connection
}
```

根据当前规范，资源分类具有递归传递性。

于是编译器不仅需要跟踪直接声明 `deinit` 的类型，还需要考虑：

- 包含资源字段的复合类型；
- 泛型容器中的资源；
- enum 不同分支中的资源；
- nullable 资源；
- closure 捕获资源；
- 资源被移动或替换；
- 控制流条件下的资源存在性。

如果资源分类正确性依赖这种递归传播，那么它实际上成为了类型系统的一部分。

这并非不可行，但与 Rust 的统一所有权机制相比，编译器需要维护两类析构时机。

此外，当前规范中 Resource / Pure 的定义也需要特别警惕某些依赖运行时清理、但没有用户自定义 `deinit` 的内建类型。

这些类型必须通过可信的内建分类或析构规则覆盖，不能简单地按“没有用户 `deinit`”认定为无需资源语义。

我不建议现在直接推翻双轨 Drop。 但建议长期采用“单一 Drop 规划器 + 两种 Lifetime Policy”的形式，而不是维持两套分别处理全部控制流的算法。

具体可以把类型规则抽象为：

```
DropPolicy:
    EarlyEligible
    LexicalRequired
```

其中：

- `EarlyEligible`：允许经过 liveness 证明后提前释放；
- `LexicalRequired`：不能仅因最后一次读取就提前释放；
- 显式 move、consume、replace 仍遵循各自语义。

这样既保留资源安全，也让底层 Drop 规划共享控制流算法。

### 4.3 普通 Borrow Return 的扩展边界

当前的 `from source` 可以解决大量简单借用返回。

但考虑：

```
fun select(
    borrow left: Data,
    borrow right: Data,
    flag: Boolean
): borrow Data {
    return if (flag) left else right
}
```

这是一个返回来源可能为 left 或 right 的函数。

Rust 可以通过生命周期关系约束表达这样的 API，但 Koven 的当前规则要求确定的单一来源。

因此该形式不能直接按现有合同表达。

未来可以选择几条路线：

| 路线                       | 复杂度 | 建议           |
| -------------------------- | ------ | -------------- |
| 继续禁止多来源             | 最低   | v1 默认        |
| 要求返回 owned value       | 低     | 大部分应用优先 |
| 显式单一来源 `from`        | 中低   | 当前模式       |
| 支持多个来源的联合生命周期 | 中高   | 后续可选       |
| 引入完整生命周期泛型       | 很高   | 不建议短期实施 |

还有一个容易低估的点：如果未来允许 borrowed value 进入集合、closure、对象字段和泛型，生命周期关系就会自然扩散到类型系统中。

那时 Koven 就不再是目前这种局部借用模型，而是在逐步重建 Rust 的一部分生命周期类型系统。

我建议设定一个长期不变量：

借用的复杂性不能无约束地穿透公开类型系统。

只有真实、常见、并且不能很好地通过 owning value 解决的需求，才考虑增加新的 borrowed 类型能力。

### 4.4 最值得提高的是借用终止推断，不是生命周期表达能力

Koven 已明确允许受限的 last-use 终止，但不提供完整 NLL。

这是一个很好的逐步演进起点。

例如：

```
borrow val item = view(data)

println(item)

// 如果未来不会使用 item
// 这里允许恢复 data 的独占权限
```

如果编译器能够可靠证明 `item` 不再使用，就不应该因为 `item` 仍处于词法作用域中而继续阻止对 `data` 的修改。

我建议首先针对这种情况推进：

1. 局部命名借用；
2. 无逃逸的只读借用；
3. 单一已知 `from`；
4. 结构明确的 `if`/`when`；
5. 可以追踪的简单循环。

不需要从第一天就支持任意多来源、多重 reborrow、closure capture 与复杂循环的完整组合。

这基本是有限 NLL + Origin Tracking 的路线。

## 五、Koven 与 Rust、Swift、Mojo、Zig 的对比

### 5.1 核心设计对比

| 维度         | Koven               | Rust              | Swift                 | Mojo                         | Zig             |
| ------------ | ------------------- | ----------------- | --------------------- | ---------------------------- | --------------- |
| 内存管理     | 所有权 + Drop       | 所有权 + Drop     | ARC + 值语义 + 所有权 | 所有权 + Drop                | 手动管理        |
| 默认参数     | Borrow              | 按值 Move/Copy    | 依类型和上下文决定    | 只读借用                     | 按声明传值/指针 |
| 可变借用     | Inout               | `&mut`            | `inout`               | `mut`                        | 可变指针        |
| 生命周期标注 | 无用户生命周期参数  | 必要时显式        | 常规代码不需要        | Origin 关系可显式            | 无静态借用证明  |
| 可存储借用   | 不支持              | 支持              | 有限的安全借用能力    | 支持受 Origin 限制的引用形式 | 可以存指针      |
| 借用检查     | 静态有界            | 静态通用          | 排他访问 + 所有权检查 | 静态生命周期检查             | 主要靠开发者    |
| 引用循环     | Rc 等机制需特别处理 | Rc/Arc 可形成循环 | ARC 需要 weak 等策略  | 依所用拥有/引用类型          | 开发者负责      |
| 适合方向     | 易用的原生通用开发  | 广泛系统编程      | 应用与平台开发        | 高性能、AI、系统计算         | 底层控制        |

Swift 的常规引用类型依靠 ARC，而其 `~Copyable` 非复制类型具有更严格的消费与析构规则。Mojo 目前已有较成熟的值所有权和 Origin-based 引用文档，但其语言和生态仍处于快速演进中。

Documentation+4



### 5.2 开发者心智负担对比

典型开发者学习与使用心智负担

主观相对指数：越低表示越轻。不是实测数据，针对普通安全业务代码，不含特殊 unsafe 场景。

![Chart](data:image/png;base64,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)

这里 Koven 的 4 分代表设计目标，而不是当前工具链实际体验。当前的 UnsupportedNode、能力组合边界和生态不成熟，会额外增加真实开发成本。

我会把心智负担再分为三个层次：

| 层次                       | 最有优势的模型          |
| -------------------------- | ----------------------- |
| 普通应用调用和数据处理     | Swift、目标状态的 Koven |
| 高性能值语义与可控数据传递 | Mojo、Koven             |
| 复杂引用结构和系统抽象     | Rust                    |
| 需要完全掌控分配与释放     | Zig、Rust unsafe        |

这里有个看似矛盾的事实：

Rust 的语法心智负担较高，但面对复杂引用结构，它的通用表达能力反而可能让整体工程更简单。

因为它能直接表达许多借用关系，不必重新设计成 owned copy、索引或共享句柄。

这也是 Koven 不应一味追求“永远比 Rust 少写生命周期语法”的原因。

如果代价是让常见安全模式完全无法表达，用户实际心智负担可能并不会更低。

## 六、与 Rust 的深入对比

Rust 最大的优点不是它有 `'a`，而是它建立了一套相对通用的规则：

- 唯一所有者；
- 不可变共享或独占修改；
- Move 后原值失效；
- 引用不能超出有效来源；
- 对控制流进行借用分析；
- 可将生命周期关系用于泛型、trait 和公开 API。

Rust 借用检查主要在 MIR 上进行，NLL 利用 CFG 与 region inference。这种中间表示在复杂分析前已经去除了许多高级语法结构。

GitHub+1



### Rust 的优势

Rust 能够支持非常广泛的模式，包括引用结构体、借用迭代器、带生命周期参数的 API、复杂泛型引用关系，以及底层零复制抽象。

这种表达能力对网络协议解析、数据库存储引擎、操作系统、复杂容器和库开发尤其重要。

### Rust 的代价

开发者必须主动理解类型关系。

例如：

```
struct View<'a> {
    data: &'a str
}
```

当多个对象相互借用、需要进行可变访问或跨 async 状态保存引用时，类型和生命周期关系可能变得困难。

即使没有显式生命周期标注，开发者也必须理解借用检查的限制。

### Koven 相对 Rust 的真正优势

不是同样功能写得更短，而是：

多数 API 默认不产生长期借用关系，开发者可以优先使用 owned value 完成工作。

这可能让 Koven 更适合：

- 普通服务端应用；
- CLI 工具；
- 业务规则与数据转换；
- 大量 String、List、Map 处理；
- 不强调自定义借用结构的原生工具；
- 对可预测资源清理有需求的中间件。

但如果目标是完全替代 Rust 的系统编程能力，我认为当前借用模型还不够。

尤其是在复杂零复制库、高性能解析器、内部引用结构体、无分配迭代器等场景中，Koven 的表达能力限制会比较明显。

Rust 自己也仍在推进 Polonius、place-based lifetimes、view types 和内部引用等能力，说明完整借用模型的设计空间至今仍存在难题。

GitHub+1



## 七、与 Swift 的对比：最值得学习的是渐进式所有权

[Apple Updates Swift Playgrounds With New Augmented Reality Challenge - MacRumors](https://images.openai.com/static-rsc-4/6Vhjv3mMpPn8ksWWwd7T_PoUKVT62-XfVRG2RRPnzlRBoy3mDLIIJBjoNQIanBh7NtY_CFLoY97bkkxJjToEJ-miOekS7ZUCp6X84zOBeZ4VAA0084nrOwkjL90t_typL-3PjPrQj8pQowj_1l8MUPyXHQ4KDxmO0mKJO5ItTjQ?purpose=inline)

Swift

Swift 更接近应用开发优先、渐进加入系统能力的路线。

Swift 的设计和 Koven 有一个重要共同点：不希望大部分普通开发者为每一个对象显式思考复杂生命周期。

但两者的基本出发点不同。

Swift 可以通过 ARC 管理类对象生命周期，并且把非复制类型、`borrowing`、`consuming`、`inout` 等机制逐步引入更底层的代码。

Koven 则把唯一所有权和 MoveOnly 放在语言的基础模型中，避免依赖通用 ARC 解决所有引用生命周期问题。

Swift 的 SIL 还区分严格、词法与优化生命周期，这对 Koven 的双轨 Drop 非常具有参考价值。

GitHub+1



### 对 Koven 的启示

我建议学习 Swift 的三个原则：

第一，日常代码保持低认知成本。 只有在确实进行所有权转移或独占修改时才暴露更多约束。

第二，让高性能能力逐步增强，而不是一开始要求所有用户掌握完整系统。

第三，把所有权验证与后端优化分开。 安全性依赖静态正确性，性能优化可以逐步改进。

但我不建议 Koven 为了像 Swift 一样易用，全面切换到 ARC。

那将改变 Koven 当前最有特色的内存管理设计。

## 八、与 Mojo 的对比：理念最接近，但 Mojo 的借用表达更广

[Modular: Mojo🔥 - It’s finally here!](https://images.openai.com/static-rsc-4/GwsnTz8h_J_UM6Q0lA2FcOF-6sZ__NuTrpxo98bdldZQd9Bs95U-yGxwsupdFdQyry3JAakVCXNgPpZO8zMvde9Q4W7d4iEI-6I-1ZsBqzA7d_B00WjmSQsvarnLMNM6nE4eltfcK-Zaiwh-7qzxpQmYMhDN05eYorx6k9mv_zI?purpose=inline)

Mojo

Mojo 以值所有权、参数调用约定、Origin 生命周期关系和原生高性能为主要设计方向。

从所有权模型来看，Mojo 是 Koven 最值得深入研究的对象。

Mojo 也不要求开发者像 Rust 那样在普通调用点频繁写借用符号，并提供 `mut`、拥有值传递和引用绑定。官方文档还定义了 Origin，用于判断引用从哪里来、能够持续多久。

Mojo+1



### Mojo 与 Koven 的关键区别

Mojo 的 Origin 不只是函数返回值的一个来源名字。

它可以构成更丰富的引用关系，并用于结构化的引用表达。

Koven 当前把借用返回严格限制在单一确定来源、非 owning 局部绑定以及受限的同步传递。

| 场景         | Koven                   | Mojo                   |
| ------------ | ----------------------- | ---------------------- |
| 普通只读调用 | 默认 Borrow             | 默认不可变引用         |
| 可变参数     | Inout                   | `mut`                  |
| 所有权转移   | `own`                   | 显式转移约定与操作     |
| 借用结果     | 单一 `from`             | Origin 关系            |
| 存储引用     | 不提供通用能力          | 有受限制的引用表达能力 |
| 生命周期语法 | 尽量隐藏                | 必要时表达 Origin      |
| 实现目标     | Kotlin 风格原生通用语言 | 高性能与系统计算       |

Mojo 的 Origin 机制说明：

可以借鉴 Rust 的安全借用原则，而不必直接照搬 Rust 的所有生命周期语法。

不过 Mojo 的 Origin 系统并不是免费的。更广泛的引用表达同样需要复杂的编译器来源和生命周期证明。

因此，我建议 Koven 学习 Mojo 的内部表示思想，而不是立刻引入同等范围的公开引用功能。

具体来说，Koven 的：

```
borrow U from source
```

完全可以作为未来 Origin 关系的简洁语法入口。

但内部应尽量统一来源身份，避免不同语言家族分别定义不兼容的 Origin 规则。

## 九、与 Zig 的对比：简单实现不等于安全开发简单

[Zig Logo PNG Vector (SVG) Free Download](https://images.openai.com/static-rsc-4/gk_-5noORuuqhTrJqwmVdkZfPbiXs08j_Nb1mAcHAMqRmjJA1h6-d0VC4PEVYkEyMvK2_3FgeCqJ7XFPnZKFeqUwTAF4nUHvmdBM4YcmUfQGM9fRf4TtrEfuBNEkNmAcoYJ-9Mhc4UBB6GLtyzJatr7RaC18XWbChElio18cfeY?purpose=inline)

Zig

Zig 通过显式分配器、`defer` 和手动资源管理提供高度控制，但不提供 Rust 式的通用静态借用安全保证。

Zig 的优势主要是直接和透明。

开发者很清楚分配器、指针和释放由谁管理，编译器不需要构建像 Rust/Koven 那样复杂的静态借用体系。

但这意味着更多正确性责任交给开发者。官方文档明确指出，指针有效期与内存所有权需要程序员和 API 合同负责。

The Zig Programming Language+1



Koven 不应该走向 Zig 式的“默认由开发者自己保证借用安全”。

否则就会失去当前语言设计最重要的卖点。

可以学习 Zig 的地方是资源清理的显式性，以及对分配和生命周期成本的透明呈现。

## 十、未来建议：优先做什么，不建议做什么？

我建议把 Koven 的所有权路线分为三个阶段。

阶段 A：先完成语义与实现闭环

最高优先级

v1 前应完成

将 D10/E2/E3 中所有权相关重复职责收敛，建立唯一的借用来源、移动可用性、Drop 计划和 SSA 消费合同。重点处理已经存在的组合缺口，而不是扩张语法。

阶段 B：增强有限 NLL 和 Origin Tracking

v1 稳定后，逐步推进

扩大单一来源 borrow result、局部 last-use、字段分离借用、简单 CFG 与循环借用能力，继续保持公开语言模型简单。

阶段 C：按实际需求引入高级借用能力

v2 或更后续

通过标准库实际需求驱动多来源 Borrow、受限借用 View、共享线程能力等设计；对可存储引用和生命周期泛型保持谨慎。

### 10.1 阶段 A：我认为最重要的六项验收

这些是建议新增或明确化的验收维度，不表示当前仓库没有任何对应测试。

| 验收方向    | 核心检查                              |
| ----------- | ------------------------------------- |
| Move 安全   | 不重复消费、不使用已移动值            |
| Borrow 安全 | 不产生冲突别名、不提前释放来源        |
| Drop 正确   | 每条正常执行路径按规则清理一次        |
| CFG 一致    | 分支、循环、提前退出保持事实守恒      |
| 语义一致    | 同一程序 Single / Unit 不产生不同结论 |
| Native 验证 | SSA、实际执行与内存检测互相印证       |

特别是最后一项：

Koven 已有大量针对 owner、drop、loan 的静态事实检查和 Native 回归，但资源安全性不应该只依赖程序输出正确或编译成功。

应保持 Linux 上 ASan/LSan 等内存验证、必要的运行时析构计数，以及独立的错误注入回归。

macOS Apple Silicon 不支持的 sanitizer 项目继续明确标记 partial，不应虚报全平台验证完成。

### 10.2 阶段 B：引入一个统一的 Lifetime IR

这是我最推荐研究的编译器内部方向。

Koven 已有 Ownership Facts、slot CFG 和 owner-aware SSA，不应为了实现这个方向再创造一条并行生产管线。

而是可以在现有 Phase 3 与 Refinement 交界处，逐步统一：

```
Owner
  ├── Origin
  ├── Place
  ├── Availability
  ├── Loan
  ├── Loan End
  └── Drop Policy
```

我建议建立五条可由编译器验证的不变量：

1. 每个 MoveOnly 值在任一执行路径上最多有一个有效 owner。
2. 每个有效 loan 必须关联真实来源；来源不能在其有效期间失效。
3. 同一重叠 place 不得同时存在冲突的共享和独占访问。
4. 每个需要析构、尚未被转移的 owner 在正常终止路径上应按规则清理一次。
5. Refinement 不得接受缺少完整来源或清理证明的操作。

这些不是五个新的语法特性，而是编译器需要长期维护的内部合同。

尤其是当前不同家族之间的 Source ID、Owner Token、Binding Epoch、Capture 来源和 SSA 当前值身份，应明确区分并通过有限的转换关系连接。

否则最容易发生的问题，是把“创建时来源身份”误当作“当前实际 owner 身份”。

### 10.3 不建议短期引入完整 Rust Lifetimes

原因主要不是编译难，而是会改变 Koven 的语言定位。

如果引入：

```
'a
'b
T<'a>
where 'a: 'b
```

类似的一整套生命周期泛型表达，那么不仅类型系统要改变，还会牵动：

- 泛型和接口；
- 借用字段与集合；
- Closure capture；
- async / 线程；
- 类型替换与 ABI；
- SSA 和 Drop；
- 标准库生命周期 API。

当前 Koven 还在推进既有所有权路径统一，在这个阶段引入完整引用类型系统，很可能显著拉长 v1 交付时间。

我认为要坚持一个原则：

新生命周期能力必须解决已明确存在的真实用户场景，而不是仅为了达到 Rust 的特性覆盖率。

## 十一、Koven 是否足以成为真正的高级原生语言？

我的判断是：所有权设计足以支撑这一目标，但前提是接受一个比 Rust 更有选择性的系统编程范围。

更具体一些：

| 目标场景             | 设计适配度 | 核心条件                     |
| -------------------- | ---------- | ---------------------------- |
| CLI / 工具开发       | 高         | 字符串、集合、文件 IO 完善   |
| 普通后端服务         | 高         | 网络、并发、异步与生态完善   |
| 中间件及数据处理     | 高         | 容器、资源管理和高吞吐验证   |
| IoT / 工控程序       | 中高       | FFI、资源句柄、实时性验证    |
| 数据库执行器         | 中高       | Buffer/View、低拷贝 API      |
| 高性能解析器         | 中         | 借用切片和零复制能力         |
| 游戏引擎底层         | 中         | 内存布局、资源图、FFI        |
| 操作系统与驱动       | 低至中     | unsafe、指针、布局和底层抽象 |
| 完整替代 Rust 系统库 | 低         | 通用引用与生命周期表达不足   |

以上衡量的是当前语言模型的适配度，而非这些场景都已经能用 Koven 生产部署。

以你熟悉的 Java/Kotlin 后端开发作为参照，Koven 如果希望成为一种有竞争力的替代语言，主要竞争优势不应该只是“没有 GC”。

真正重要的是：

- Kotlin 风格、较低的开发认知成本；
- 可预测的内存和资源管理；
- 原生部署、减少运行环境依赖；
- 足够完善的集合、网络、并发和标准库；
- 编译器能稳定解释错误原因，而不是频繁暴露内部能力缺口。

如果这些能实现，那么 Koven 即使不具备 Rust 全部生命周期表达能力，也依然可以具有明确价值。

## 十二、最终建议与取舍

我建议 Koven 的目标不要定义为“Rust 的简单版”，而应定义为：

以值所有权为基础、借用关系局部化、具有确定性资源管理能力的 Kotlin 风格原生语言。

这意味着：

| 设计决策                  | 我的建议                   |
| ------------------------- | -------------------------- |
| 默认 Borrow               | 坚持                       |
| 显式 `own` / `inout`      | 坚持                       |
| Copyable 自动推导         | 坚持，并严密定义成本与语义 |
| Transferable 结构推导     | 坚持                       |
| 单一 `from` Borrow Return | 保留，逐步完善             |
| 禁止任意存储借用          | v1 继续坚持                |
| 双轨 Drop                 | 保留语义，评估统一规划实现 |
| 有限 NLL                  | 建议逐步增强               |
| 完整 Rust Lifetimes       | 暂不引入                   |
| Origin 追踪               | 强烈建议统一内部模型       |
| E3 Refinement             | 继续推进，减少重复解释     |
| 复杂 Borrow / CFG         | 先补证明，再扩能力         |
| 性能、安全宣称            | 必须绑定实际测试与验收证据 |

### 我最关注的三个长期风险

风险一：语言简单，编译器却过度复杂。

如果每种类型与控制流组合都需要单独实现 owner transport 和 cleanup 特例，维护成本会不断增长。Koven 应尽早让通用来源、控制流、Drop 框架覆盖更多语言家族。

风险二：受限借用演变成大量不可预测的 Unsupported。

只要大部分开发者遇到无法借用的情况，都能通过 owned value、明确复制或稳定句柄自然解决，这个设计就是合理的。

如果用户经常需要根据编译器内部支持矩阵重新组织代码，就说明语言抽象还没有足够成熟。

风险三：ASAP Drop 使对象生命周期难以直观预测。

纯内存提前释放在性能上可能有收益，但不应让用户认为源码作用域内的任意对象都一定活着，或者误认为提前释放必然没有可观察影响。

需要严格规定对象资源分类、释放时机、调试行为和用户可依赖的生命周期保证。

### 结论

如果按三个不同目标排序，我的判断是：

- 开发者易用性： Koven 值得向 Swift 靠拢。
- 所有权内部建模： Koven 值得重点借鉴 Mojo。
- 安全与编译器验证： Koven 值得借鉴 Rust MIR Borrow Checker 的思想和验证纪律。

但不要把三套模型全都搬进来。

我最推荐 Koven 现在保持 Guide v0.47 的核心所有权语义稳定，优先完成 D10/E2/E3 的责任收敛、所有权事实完备性和原生验证，再决定是否扩充生命周期表达能力。

这条路线既能保护 Koven 当前最有特色的简洁性，也能避免语言尚未完成 v1，就承担接近 Rust 全部 Borrow Checker 的实现和维护成本。

主要核对来源：[Koven Guide 10](https://github.com/Halckon/Koven/blob/main/docs/guide/10-ownership-borrowing-drop.md?utm_source=chatgpt.com)、[Koven Ownership Architecture](https://github.com/Halckon/Koven/blob/main/docs/architecture/ownership.md?utm_source=chatgpt.com)、[Koven SSA Refinement](https://github.com/Halckon/Koven/blob/main/docs/architecture/ownership-refinement.md?utm_source=chatgpt.com)、[Rust Ownership](https://doc.rust-lang.org/book/ch04-00-understanding-ownership.html?utm_source=chatgpt.com)、[Swift Declarations](https://docs.swift.org/latest/documentation/the-swift-programming-language/declarations/?utm_source=chatgpt.com)、[Mojo Lifetimes and Origins](https://mojolang.org/docs/manual/values/lifetimes/?utm_source=chatgpt.com)。

评估限制：此次核对了 GitHub 主仓库当前规范、架构说明及相关 PR 记录，但没有实际拉取编译器运行新的测试矩阵或执行内存检测。因此所有关于实现成熟度的判断均按已记录证据的覆盖范围作保守评估，而非完整独立安全审计。
