# Koven v0.34：Copyable、布局、构造与结构移动

> **性质**：规范性语言规范 · **状态**：current（v0.34） · **读取时机**：实现或评审 Copyable、Box、有限布局、构造和结构化移动时 · **唯一真源**：本页

本页是现行 Koven v0.34 规范的一部分。规则正文优先于示例；未在本页定义的相邻概念通过链接转交给对应领域页面。

## `Copyable` 推导与显式 Opt-out 边界

`Copyable` 完全由字段结构自动、递归推导。v1 不提供用户手动实现、否定或覆盖该
推导的语法，也不接受 `nocopy` 或等价修饰符。需要不可复制语义时，类型必须包含实际的
MoveOnly 分量；名称或 lint 约定不能改变类型能力。任何显式 opt-out 都需要由后续版本重新
定义关键字、推导和兼容性，不能从保留字或实现细节推断。

---

## 条件 `Copyable`、布局与结构化解构

### 封闭的 `Copyable` 判定

`Copyable` 是由编译器绑定的内建能力身份，不按源码拼写识别。判定对已替换的实际类型递归
进行，结果精确区分 `Copyable`、`MoveOnly`、`Unknown` 与 `Error`：`MoveOnly` 表示在当前
静态类型下没有复制证明、按值使用必须按移动处理；`Unknown` 只保留给尚待后续选择的 deferred
类型；`Error` 只抑制同根级联。v1 使用以下封闭规则：

- 数值类型、`Boolean`、`Char`、`Unit` 和 bottom type `Nothing` 满足 `Copyable`。`Nothing`
  没有可构造的有效值，因此是平凡满足；这也使只有 `null` 值域的 `Nothing?` 满足规则。
- `T?` 当且仅当 `T` 满足 `Copyable`；nullable wrapper 不引入 retain、clone 或唯一析构。
- `value class C<A...>` 当且仅当按实际类型实参替换后，每个主构造器字段类型都满足
  `Copyable`。字段的 `val` / `var` 不参与判定。
- 有限 `enum class E<A...>` 当且仅当按实际类型实参替换后，每个 case 的每个 payload 类型
  都满足 `Copyable`；无 payload 的 enum 因而满足。case type 沿用其 root enum 实例的能力，
  不形成可独立声明或实现的 marker。
- 类型参数只在其声明具有编译器绑定的 `Copyable` 上界时满足；`T : SomeInterface`、`T : Any`、
  `T : Transferable` 或无上界在当前泛型体内均为 `MoveOnly`。对类型实参检查 `Copyable`
  上界时使用本节同一判定。
- `String`、普通 `class`、具名 `object`、函数 / lambda 类型和内建 `Box<T>` 均为
  `MoveOnly`，不得因字段或类型实参可复制而提升。`Any` 也为 `MoveOnly`：其静态类型不能
  证明动态 payload 可复制。裸 interface / capability 不是合法 runtime value type；若因恢复
  到达能力查询则为 `Error`。deferred type 为 `Unknown`，error type 为 `Error`。
- 用户声明的同名 `Copyable` 或 `Box` 不获得内建身份，也不能冒充能力或 intrinsic 类型。

该规则只产生类型能力事实。赋值、传参、返回或解构后是否构成 move-after-use 仍由 Phase 3
检查；类型能力阶段不建立所有权状态机。

### 有限内联布局图

类型声明的内联边由 `value class` 主构造器字段与 `enum class` case payload 产生；nullable
包装对布局递归是透明边。普通 `class`、具名 `object`、函数 / lambda、内建 `Box` 与动态
顺序容器都是固定大小的间接 handle，打断内联环。类型参数本身不构成声明递归边；沿未被
handle 打断的路径再次遇到同一 value/enum 名义声明时，无论类型实参是否变化都构成无限
内联递归（例如 `A<T>` 包含 `A<List<T>>` 仍非法），避免为变化中的实例无限展开。实际类型
实参替换仍用于判断边上的 nullable / handle 类别与 `Copyable` 条件。

编译器必须按声明顺序建立确定性图。每个循环强连通分量只报告一条 L0116；多个循环分量按
最早声明顺序各报告一条。分量内以源码顺序 DFS 得到首条代表环，primary 为闭环边，labels
按环上其余边的声明顺序排列。被判定为无限内联布局的名义类型保留其 `TypeId` 供错误恢复，
但布局状态与 `Copyability` 均为 `Error`，不得流入后续 DataLayout / LLVM 类型构造。目标相关
的实际大小、对齐和对象大小上限仍属于 Phase 4。

### 内建 `Box` 身份与实参边界

v1 的 `Box` 是由 `TypeEnvironment` 显式绑定的 intrinsic type constructor，不按名称字符串
特判。它精确接受一个类型实参，且该实参必须能静态证明为一个具体 `value class` 名义实例；
普通 `class`、enum、interface、object、基础类型、函数类型与类型参数均不合法。当前上界
语言没有“是 value class”这种 kind bound，因此即使 `T : Copyable` 也不足以让 `Box<T>`
合法；泛型代码必须在具体 value-class 实例已知的位置使用 `Box`。

`Box<T>` 自身始终 `MoveOnly`，并打断内联递归。源码中声明 `class Box<T>` 只产生普通名义
class，不取得 intrinsic 语义；外部环境没有绑定 intrinsic `Box` 时，编译器不得按拼写猜测。
intrinsic `Box` 的实参数量不是 type-kind 约束：零个或多于一个类型实参沿用 L0091
`type argument arity`，只有数量为一但 type kind 不合法时才使用 L0117。

### 局部结构化解构

局部 `val (a, b) = expression` 的 initializer 只类型
检查和求值一次。源类型为 `value class` 时，分量精确对应主构造器字段的声明顺序，绑定数
必须与字段数相等；不能缺少或多出分量。typed 结果记录稳定的 statement identity、源类型、
按序的绑定 symbol / 分量类型，以及整次操作是 `Copy` 还是 `Consume`：源类型满足
`Copyable` 时为 `Copy`，否则为 `Consume`。

这里的 `Consume` 只是交给 Phase 3 的原子所有权动作描述；类型检查不判定解构后再次使用、
字段析构或部分移动。非 `value class` 的 `componentN()` 选择依赖尚未实施的一般 member/call
选择，继续保留为专用 deferred reason，不猜测结构分量。

[局部 `val` 解构语法](12-collections-destructuring.md#局部-val-解构语法)不定义占位、跳过或
丢弃分量：`val (_, x) = pair` 由 Parser 以 L0042 `unsupported destructuring form` 拒绝；
类型检查不改变这一语法，
也不会为 `_` 创建普通绑定或 typed 分量。
[block 与控制流规则](06-blocks-control-flow.md) 中 `for` binding
已经解析的 `_` 是仅属于 `for` 的专用 discard 形态；其迭代类型与所有权语义尚未启用，
不得反向扩展成通用解构占位符。

### 诊断与阶段边界

| 错误码 | 含义 | 主范围与关联信息 |
|---|---|---|
| L0115 | 类型实参不满足内建 `Copyable` 上界 | primary 为实参 TypeRef；label 指向上界声明 |
| L0116 | 直接或间接形成无限内联布局环 | primary 为闭环字段 / payload TypeRef；labels 按环中声明顺序列出其余边 |
| L0117 | intrinsic `Box` 的实参不是可证明的具体 `value class` 实例 | primary 为实参 TypeRef；无 intrinsic 绑定时不使用此诊断 |
| L0118 | `value class` 结构化解构绑定数与字段数不一致 | primary 为解构 pattern；label 指向类型声明并给出期望数量 |

本节不定义一般 callable/member/constructor 选择、独立 `componentN()` 调用、字段投影所有权、
move-after-use、drop、容器类型、`Transferable`、companion 或 codegen。L0115–L0118 具有稳定含义。

---

## 名义值与 Intrinsic `Box` 构造

### 可构造目标与唯一身份

- 普通 `class` 和 `value class` 的类型名称是其唯一主构造目标；普通 class 省略显式主构造器
  时等价于零字段构造器。`interface`、`object` 与 enum root 不是构造目标。源码类型名称与
  value 函数同名时沿用双命名空间既有规则：value binding 优先，只有没有 value binding 时
  才把 type binding 解释为 constructor，不能把二者合成 overload set。
- `enum class E` 的每个 case 使用既有 `EnumCaseId` 作为构造身份。有 payload 的 case 写
  `E.C(args)`，无 payload 的 case 仍写值表达式 `E.C`，不得为泛型推导改写成禁止的空 `E.C()`。
  enum 本体内允许既有短名。case 构造结果的公开静态类型始终是替换后的 root `E<...>`，
  不把仅供 smart cast 使用的 case type 暴露为值类型。前者在 `Call` expression 上发布
  descriptor；后者直接在解析后的 `Name` / `Member` expression 上发布零 operand descriptor，
  不能先伪装成函数值或普通 call 再补写 construction identity。
- intrinsic `Box` 只有在 `TypeEnvironment` 显式绑定时是构造目标；源码同名 class 继续是普通
  nominal constructor。`Box<T>(operand)` 与 `Box(operand)` 都只有一个稳定名称为 `element` 的
  `Value T` 参数，结果为
  `Box<T>`，并继续服从[内建 `Box` 身份与实参边界](#内建-box-身份与实参边界)。
- constructor 不声明或分配普通函数 `SymbolId`，也不伪装成 function value。typed target 使用
  `NominalId`、`EnumCaseId` 或 `IntrinsicTypeConstructor::Box`；callable reference、把构造器
  赋给变量和 constructor overload 都不在 v1 范围。

### 类型实参与受控 Expected-Result 推导

泛型 nominal/case/Box 构造只接受两种源码形态：完整显式类型实参，或完全省略。显式实参
写在现有 typed-call callee 后：`Pair<Int, String>(...)`、`Result.Ok<Int, Error>(...)`、
`Box<Point>(...)`；数量不匹配复用 L0091，不接受部分列表、`_`、默认实参或 `where`。

省略类型实参时按以下封闭顺序求解，不能交换步骤或从后续使用反推：

1. 先按源码顺序检查已经定型的非 lambda 构造 operand，并用
   [候选实例化与 Bound](07-calls-lambdas-closures.md#候选实例化与-bound)的精确结构匹配从对应
   字段/payload/Box 参数提取类型参数；无 expected type 的数字字面量先按
   [字面量规则](03-types-generics.md#相容字面量与运算符)默认定型。
2. 若仍有未决参数，只能读取在进入本次 construction 检查前已经独立确定、完整规范化且不含
   constructor-local / deferred unknown 的 expected type，例如显式变量/返回标注、已定型的
   enclosing context，或唯一且已经实例化的 callable 参数。只有其外层 identity 精确等于本次
   nominal root 或 intrinsic Box 时，才按声明顺序补齐类型实参。尚有多个 overload 候选时的
   candidate-local expected type，以及仍需由本次 construction 反向完成的外层泛型 callable
   参数，都不能作为第 2 步输入；此时必须由 operand 得到完整解或显式写出 constructor 类型
   实参。operand 与合法 expected type 对同一参数给出不同规范化 `TypeId` 时推导失败，不做
   common type、widening 或型变。
3. 获得完整替换后检查 interface、`Copyable`、`Transferable` bound，再用替换后的 Value 参数
   类型单向检查全部 operand 与 lambda。lambda body 不参与第 1/2 步推导；只剩 lambda 能提供
   类型信息时必须写显式类型实参或提供第 2 步允许的独立、完整同 root expected type。

这项 expected-result 规则是 constructor/case 的窄化例外，不改变
[泛型 Callable 实例化](07-calls-lambdas-closures.md#泛型-callable-实例化与-overload-lambda-隔离)的
单向规则。它使 `val ok: Result<Int, String> = Result.Ok(1)`、
`val empty: Empty<String> = Empty()` 和泛型无 payload case
`val none: Option<Int> = Option.None` 可确定；脱离同 root expected type 的 `Empty()` /
`Option.None`，或只有被第 2 步排除的 candidate-local expected 时使用 L0144，而不是发明
`Option<Int>.None` 新语法。成功实例 key 由构造 target
和声明顺序的完整类型实参组成，与源码是否显式无关。

### 参数映射、所有权与求值

- class/value-class 主构造器字段与 enum payload 按声明顺序形成稳定、可命名的参数；intrinsic
  Box 只有稳定参数名 `element`。位置/命名混排、重复、缺失、额外参数及 type/mode 检查复用
  [Typed call argument](07-calls-lambdas-closures.md#typed-call-argument)的 L0120–L0123 规则。
  字段 visibility 不删除 constructor 参数名；跨文件 constructor
  typed selection 由 compilation-unit 类型检查完成。
- 所有构造参数都是 `ParameterMode::Value`：class 字段与 enum payload 沿用
  [天然-owned 声明形态](08-class-family-members.md#声明头构造器字段与修饰符)，intrinsic Box
  只有编译器内建抽象签名；两者调用点都不写
  `own`。显式 `borrow` / `&` 与 Value 参数不匹配并复用 L0122。operand 按源码顺序各求值
  一次，命名映射不改变求值顺序；每个 operand 完成后立即按 `Copyable` 复制或按 MoveOnly
  移动到尚未发布的 construction owner。
- 成功 typed 产物保存 expression、稳定 target/instance key、结果类型，以及按参数声明顺序的
  field/payload symbol、Value mode、源码 argument 与 argument evaluation index。无 payload case
  发布零参数 construction descriptor。constructor 专项分派优先于 ordinary callable/function-value
  分派；成功 construction 不发布 `CallDescriptor`，也不把 type/case callee 或 receiver 当作运行时
  operand 求值。descriptor 属于[完整 typed trial 状态](07-calls-lambdas-closures.md#多-overload-候选中的-lambda)，
  失败或未唯一提交的 overload trial
  必须连同 nested construction facts 一起回滚，不能只删除诊断。
- 类型检查发布构造 target、参数映射和结果类型；所有权检查据此发布 Value delivery、temporary、
  move/copy、ASAP drop 与 construction root 的唯一 drop obligation。普通字段仍不形成可独立移动或
  析构的源码 place。Phase 4 按完整单态结果类型生成递归字段/payload drop glue，并消费已验证 facts，
  不重新推导参数映射或 owner liveness。

### `Result` Payload 与诊断

现行词法规范把 `value` 保持为硬关键字，因此核心 `Result` 声明固定为
`Ok(success: T), Err(error: E)`，不得使用不可解析的 `Ok(value: T)`。payload 名称不改变
`Result<T, E>`、postfix `?` 或错误传播语义。

| 错误码 | 稳定含义 | primary / 关联位置 |
|---|---|---|
| L0143 | type-position callee 不是可构造的 class/value class/case/intrinsic Box | primary 为 callee 名称；label 指向实际 type 声明（若有） |
| L0144 | constructor/case 无法从 operand 与合法的独立同 root expected type 得到完整一致的类型实参 | primary 为 constructor/case 名称；labels 指向未决/冲突类型参数声明 |
| L0145 | 单态 nominal/enum/Box 的 target size/alignment/payload storage 无法表示 | primary 为触发实例化的 constructor/type use；label 指向来源类型声明或超限字段/case |

L0091、L0093、L0115、L0141 继续分别表示 arity、interface、`Copyable`、`Transferable` bound；
L0120–L0123 继续表示参数名称/数量/mode/类型候选失败。L0145 只把 IR-local target layout
preflight 映射为源码诊断，不把 target 阈值变成新的静态类型或隐式 boxing 规则。frontend
继续保持 target / LLVM 无关；L0145 由 codegen/native 边界使用 frontend 集中诊断目录和来源
`Span` 构造，在调用 LLVM object emitter 前返回，不能把 target 布局判断倒灌到类型检查器。

---
