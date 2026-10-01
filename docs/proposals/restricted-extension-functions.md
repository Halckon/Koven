# 受限扩展函数候选设计

> **性质**：非规范候选设计 · **状态**：未启用 · **读取时机**：仅在评审标准库算法声明能力或扩展函数语法时 · **唯一真源**：现行语义仍以 [现行 guide](../guide/README.md) 为准

本文不修改、取代或启用 Koven v0.37，也不批准 guide、Spec 或 ADR，不授权实现。它给出一个
**受限形态的扩展函数**候选：只为标准库预声明类型、且只在标准库内声明。它是
[集合算法所有权候选设计](collection-algorithm-ownership.md) 的前置依赖。

## 1. 问题

集合算法需要 Kotlin 式的 member 调用语法：

```kotlin
list.filter { it == a }
list.consume().filter { it == a }
```

但 `docs/guide/12` 规定算法"用目标语言实现"（Phase 5），而 `List<T>` / `View<T>` 是编译器
绑定的 intrinsic 类型，**无法用普通 member 声明算法**——现行语法没有"给已有类型添加方法"
的能力。

## 2. 现行事实

**扩展函数被有意去掉。** `docs/guide/15` 原文：

> 真实 Kotlin 的 `val x: Int get() = ...` / `var y: Int set(value) { ... }`（自定义属性访问器）
> **在本语言 v1 中不支持**，和**扩展函数**一样列入"去掉的语法糖"清单……与"去掉语法糖、
> 保留核心"的项目定位一致。

**但 receiver mode 语法已经存在。** `docs/guide/08` 已定义 `borrow` / `inout` / `own` 三种
instance receiver mode：

```kotlin
class Buffer(var size: Int) {
    fun inspect(): Int = this.size
    borrow fun sameInspect(): Int = this.size
    inout fun clear(): Unit { this.size = 0 }
    own fun finish(): Int = this.size
}
```

并规定：

- 缺省 marker 与显式 `borrow` 规范化为 `Borrow`；`inout` 是 exclusive non-owning receiver；
  `own` 是内部 `Value` receiver；
- receiver 是**隐藏的第一个 callable operand**；
- receiver marker **只在** class/value/interface/enum/object 的 instance-function slot 提交，
  顶层 function 与其他声明位置"仍定向拒绝"。

同一页还写明"本节不改变……**extension receiver**……grammar"——即 extension receiver 目前
不存在，但已被识别为一个独立语法面。

**因此缺口是精确的**：receiver mode 的语义与拼写已就绪，缺的只是"receiver 是显式类型"与
"允许出现在顶层"。

## 3. 候选设计：受限扩展

### 3.1 两条限制

| 限制 | 内容 | 目的 |
|---|---|---|
| L1 | receiver 类型只能是编译器绑定的 intrinsic 类型 | 避免"用户类型 + 扩展"带来的重载、遮蔽与解析复杂度 |
| L2 | 扩展函数**只在标准库（`koven` 包）声明** | 不需要 import、作用域与遮蔽规则 |

**L1 的候选类型集合**（已确认包含全部 builtin 名义类型与 intrinsic 构造器）：

| 类别 | 类型 |
|---|---|
| 顺序容器 | `Array<T>`、`List<T>`、`MutableList<T>`、`View<T>` |
| 字符串 | `String` |
| 间接层 | `Box<T>` |
| 共享所有权 | `Rc<T>` |

判定依据是"由 `TypeEnvironment` 绑定的编译器身份"，不是源码名称——用户声明的同名类型不获得
扩展函数能力，与 `String` / `Rc` / `Box` 的既有 intrinsic 规则一致。

**暂不包含**：数值类型、`Boolean`、`Char`、`Unit`（其操作为语言内建，且 operand 到 String
的转换契约尚未封闭）、`value class` / `enum class` / 普通 `class` / `interface` / `object`
（用户可声明）、函数类型。

两条限制合起来，把 15 页担心的复杂度来源（任意类型 × 任意包 × 用户扩展）全部排除。

### 3.2 语法

在既有 `function_declaration` 前增加可选 receiver：

```ebnf
extension_function_declaration =
    [ visibility_modifier ], [ method_receiver_mode ],
    "fun", [ type_parameter_list ],
    type_ref, ".", Identifier,
    "(", [ value_parameter, { ",", value_parameter } ], ")",
    [ ":", type_ref ],
    [ "=", expression | block ] ;
```

示例：

```kotlin
borrow fun <T> List<T>.filter(pred: (T) -> Boolean): View<T>
borrow fun <T> List<T>.take(n: Int): View<T>
borrow fun <T> View<T>.filter(pred: (T) -> Boolean): View<T>
inout  fun <T> MutableList<T>.addAll(own other: List<T>): Unit
own    fun <T> List<T>.consume(): ConsumingView<T>
```

与既有 grammar 的关系：`method_receiver_mode` 与 08 页共用同一拼写；`type_ref "." Identifier`
是新增的唯一结构；其余部分复用 `function_declaration`。

### 3.3 receiver mode 与所有权

| mode | 调用点 | 语义 |
|---|---|---|
| 缺省 / `borrow` | `list.filter { ... }` | receiver 是隐藏的第一个 `Borrow` operand；源保持可用 |
| `inout` | `&list.addAll(x)` | exclusive non-owning receiver；调用点写 `&`（与 08 页一致） |
| `own` | `list.consume()` | receiver 是内部 `Value` operand；源 binding 之后不可用 |

三者复用 08 页已确立的语义，不新增所有权规则。

### 3.4 解析与可见性

- **member 优先于扩展函数**：若 receiver 类型已有同 shape 的 member，选择 member；
- **不因 receiver mode 形成重载**：与 08 页"receiver mode 不参与 overload shape"一致；
- **扩展函数不参与 import**：L2 已把声明限制在标准库，因此它们作为 prelude 的一部分对所有
  用户代码可见，不存在"哪个扩展函数在作用域内"的问题；
- **不能访问 receiver 的 `private` member**：只能通过公开表面（`size`、索引、迭代）操作。
  这条不是限制而是保护——它保证扩展函数不会破坏封装。

### 3.5 与 intrinsic 类型的交互

扩展函数只能使用 receiver 的**公开表面**。以 `List<T>` 为例，可用的是 12 页已预声明的核心
原语：`size`、索引 place、借用迭代 provider。因此普通算法（`sum`、`count`、`joinToString`
等）可以用纯 `.ko` 实现。

**但涉及 intrinsic 类型构造的算法仍需编译器支持**：`filter` 返回 `View<T>`，而 `View<T>` 是
intrinsic 类型，用户代码无法构造它。这类算法需要编译器提供构造能力（例如"按索引集合建立
视图"的 intrinsic operation）。这一点在
[集合算法所有权候选设计](collection-algorithm-ownership.md) §7 中说明。

## 4. 与完整 Kotlin 式扩展的差异

| 能力 | 完整 Kotlin 式 | 本候选（受限） |
|---|---|---|
| receiver 类型 | 任意类型 | 仅 intrinsic 类型（L1） |
| 声明位置 | 任意包 | 仅标准库（L2） |
| 用户可声明 | ✅ | ❌ |
| 需要 import / 作用域 / 遮蔽规则 | ✅ | ❌ |
| 可访问 receiver 的 `private` | ❌ | ❌ |
| 与 member 的优先级 | member 优先 | member 优先 |

**本候选明确不提供**：用户自定义扩展、跨包扩展、扩展属性、扩展的 import 与遮蔽语义。

## 5. 与 15 页决定的关系

本候选要求**把"扩展函数"从 15 页的"去掉的语法糖"清单中移除**，同时**保留自定义属性
访问器仍在清单中**。

理由有两条，必须同时成立才值得推翻该决定：

1. `docs/guide/12` 已把集合算法定为"用目标语言实现"（Phase 5），这要求算法能用源码声明；
2. 若改为"算法由编译器预声明为 intrinsic member"，则与第 1 条冲突，且标准库演进需要修改
   编译器。

15 页给出的去掉理由（复杂度收益比低）针对的是**完整 Kotlin 式扩展**。本候选通过 L1 + L2
把复杂度来源排除，属于"去掉语法糖、保留核心"定位下的可接受增量。

### 5.1 授权状态

用户已明确**授权放宽 15 页关于扩展函数的决定**，但**实际放开（修改 guide）在具体实施时
进行**。在此之前：

- 该候选不自行修改 15 页或启用新规范，`current` guide 以现行唯一入口为准；
- 本候选仍是非规范候选，不构成实现依据；
- 实施时需要形成新 guide 版本，并把"扩展函数"从去掉清单中移除，同时保留自定义属性
  访问器仍在清单中。

该授权只针对扩展函数，不扩及其他"去掉的语法糖"条目。

## 6. 为完整扩展留出演进空间

受限形态是**阶段性选择**，不是终态。设计上必须保证未来能演进到完整 Kotlin 式扩展函数，
而不需要重写语法或推翻语义。以下四条不能做死。

### 6.1 语法必须是完整版的子集

`[visibility] [receiver_mode] fun Type.name(...)` 在完整版中保持不变；演进只放宽 `Type` 的
范围与声明位置，**不引入第二套语法**。因此：

- 不为受限形态设计专用拼写（如 `ext fun`）或专用关键字；
- `type_ref "." Identifier` 结构同时为未来的**扩展属性**（`Type.name: T`）留出位置，本候选
  不启用。

### 6.2 限制必须是"诊断"，不是"语法非法"

L1（receiver 类型）与 L2（声明位置）的边界应在**名称解析 / 类型检查阶段**判定并产生稳定诊断，
而不是在 Parser 阶段拒绝。这样演进时只需停止发出这些诊断，不需要改动 grammar。

### 6.3 名称解析规则必须是完整版的退化特例

受限版的"member 优先 + 无 import"应当是完整版规则的**特例**，而不是一套独立规则：

| 完整版规则 | 受限版表现 |
|---|---|
| member 优先于扩展 | 同（已一致） |
| 候选集由 import 与作用域决定 | 所有候选都在 prelude → 退化为"总是可见" |
| 遮蔽按作用域层级 | 单层作用域 → 无遮蔽 |

因此受限版实现时应**按完整版的解析模型组织**（候选集 + 优先级 + 作用域），只是候选集恰好
只有 prelude。这样放开 L2 时不需要重写解析器。

### 6.4 与 08 页 receiver mode 保持单一真源

receiver mode 语义、调用点标注规则与 overload shape 规则必须与 08 页共用同一套定义。演进到
完整扩展时这些规则不得重新定义，否则会出现"instance receiver 一套、extension receiver 另一套"
的分裂。

### 6.5 预期演进步骤

| 步骤 | 变更 | 不改动 |
|---|---|---|
| 1. 受限扩展（本候选） | 语法 + intrinsic receiver + 标准库声明 | — |
| 2. 放开 receiver 类型 | 移除 L1 诊断 | 语法、receiver mode、overload shape |
| 3. 放开声明位置 | 移除 L2 诊断，启用 import / 作用域 / 遮蔽 | 同上 |
| 4. 扩展属性（若需要） | 启用 `Type.name: T` | 同上 |

## 7. Phase 边界

| Phase | 需要做什么 |
|---|---|
| 1 | Parser：`type_ref "." Identifier` 的扩展函数声明；非 intrinsic receiver 或非标准库位置的定向诊断 |
| 2 | 名称解析：member 优先规则；扩展函数选择；receiver mode 契约规范化 |
| 3 | receiver 的 loan（Borrow/Inout）与源失效（own）；与既有 receiver 规则一致 |
| 4 | receiver 作为隐藏第一个 operand 的 ABI |
| 5 | 标准库算法实现 |

## 8. 待决策点

1. **放开到用户扩展的时机与步骤**：见 §6.5；放开时需要补齐 import / 作用域 / 遮蔽规则，
   本候选的 L2 会失效；
2. **非标准库位置的诊断**：需要稳定错误码，且文案要说明"扩展函数当前只允许标准库声明"；
3. **与 callable reference 的交互**：`::filter` 这类引用是否指向扩展函数；
4. **与 08 页 instance receiver 的关系**：两者是否共用同一套 receiver mode 规范（候选倾向
   共用，避免两套语义）。

## 9. 非目标

本文不提议在本候选内实现完整 Kotlin 式扩展函数、不提议扩展属性、不提议自定义属性访问器，
也不授权在 v1 内新增任何标准库算法。但设计上必须为其留出演进空间，见 §6。
