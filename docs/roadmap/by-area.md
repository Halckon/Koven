# Koven v1 Roadmap：按领域

> **性质**：生成视图 · **状态**：current（生成自 [v1.json](v1.json)） · **读取时机**：查看某一语言或工程领域的 v1 完成度时 · **唯一真源**：[v1.json](v1.json)；语义归 Guide，验收证据归各 Spec

<!-- 生成文件：请编辑 docs/roadmap/v1.json 后运行 python3 scripts/gen_roadmap.py，不要手改本页。 -->

基线：main 3b202f86（PR #120 已合并，D10 统一管线）· Guide v0.46 · 更新：2026-10-22 · 其他视图：[按阶段](by-stage.md) · [按 Phase](by-phase.md) · 维护规则见 [README](README.md)

## 总览

| 领域（仅 v1 范围） | ✅ 已交付 | ◐ 部分 | ▶ 进行中 | ☐ 未开始 | ❓ 待决策 | ⏸ 延后 | 合计 |
|---|---:|---:|---:|---:|---:|---:|---:|
| 词法与语法 | 1 | 0 | 0 | 0 | 0 | 0 | 1 |
| 类型、泛型与声明 | 5 | 0 | 0 | 7 | 0 | 0 | 12 |
| 控制流 | 2 | 1 | 1 | 2 | 0 | 0 | 6 |
| 类、接口与成员 | 2 | 1 | 0 | 0 | 0 | 0 | 3 |
| 空安全与错误值 | 1 | 1 | 0 | 3 | 0 | 0 | 5 |
| 所有权、借用与析构 | 5 | 0 | 0 | 4 | 0 | 0 | 9 |
| 函数类型与闭包 | 0 | 1 | 0 | 1 | 0 | 0 | 2 |
| 集合与迭代 | 6 | 0 | 0 | 3 | 0 | 0 | 9 |
| 文本与 IO | 2 | 0 | 0 | 4 | 0 | 0 | 6 |
| 标准库与运行时 | 2 | 0 | 0 | 5 | 0 | 0 | 7 |
| 编译器结构 | 4 | 0 | 1 | 1 | 0 | 0 | 6 |
| 工具链 | 1 | 0 | 0 | 0 | 0 | 0 | 1 |
| 安全验证 | 1 | 1 | 0 | 1 | 0 | 0 | 3 |
| 验收、发布与文档 | 4 | 3 | 0 | 3 | 0 | 0 | 10 |

## 词法与语法

| 状态 | ID | 内容 | 阶段 | 优先级 | Guide | Phase | Spec / 依据 |
|---|---|---|---|---|---|---|---|
| ✅ 已交付 | B02 | Lexer/Parser 与错误恢复：完整文件组合、换行分句与续行、上下文 TypeRef | S0 已交付基线 | v1 必做 | 已定义 | 1 | [0231](../archive/specs/0231-contextual-type-ref-trials.md) [0234](../archive/specs/0234-block-newline-continuation.md) [0242](../archive/specs/0242-automatic-borrow-call-migration.md) |

## 类型、泛型与声明

| 状态 | ID | 内容 | 阶段 | 优先级 | Guide | Phase | Spec / 依据 |
|---|---|---|---|---|---|---|---|
| ✅ 已交付 | B03 | 名称与类型事实：单文件与 compilation-unit、package/import、名义/泛型/interface、单态化 | S0 已交付基线 | v1 必做 | 已定义 | 2 | [0197](../archive/specs/0197-multifile-type-checking.md) [0247](../archive/specs/0247-multifile-baseline.md) [names-and-types](../architecture/names-and-types.md) |
| ✅ 已交付 | B06 | const val 封闭求值、数值字面量（radix/下划线）与整数具名位运算 | S0 已交付基线 | v1 必做 | 已定义 | 2,3,4 | [0226](../archive/specs/0226-unit-constant-materialization-ownership.md) [0227](../archive/specs/0227-unit-constant-native-lowering.md) [0229](../archive/specs/0229-extended-numeric-literal-values.md) [0240](../archive/specs/0240-integer-bitwise-execution.md) |
| ✅ 已交付 | L10 | 顶层声明收窄：只允许 const val；顶层运行时 val/var 结构化拒绝（D4） | P0 止血 | v1 必做 | 需新增 | 2 | [0343](../archive/specs/0343-top-level-const-only.md) [global-and-static-state](../proposals/global-and-static-state.md) |
| ☐ 未开始 | N05 | Double/Float 运算与 Char 比较的 native 支持 | P2 补齐现有 Guide 能力 | v1 必做 | 已定义 | 4 | — |
| ☐ 未开始 | L1 | 值语义协议 Equatable/Hashable/Display（Comparable 只给内建数值、Char、String）；value class/enum 自动推导相等与哈希；Map 键 K : Equatable + Hashable（D1） | P3 语言增量 | v1 必做 | 需新增 | 2,4,5 | — |
| ✅ 已交付 | L3 | 八种整数之间的 checked 转换（越界 Abort） | P3 语言增量 | v1 必做 | 已定义 | 2,3,4 | [0327](../archive/specs/0327-checked-integer-conversions.md) |
| ☐ 未开始 | L3b | 转换族其余部分：OrNull 变体、浮点↔整数 checked、Char 码点 | P3 语言增量 | v1 必做 | 需新增 | 2,4 | — |
| ✅ 已交付 | L9b | 简单变量普通赋值的类型、Unit 结果与收窄事实失效 | P3 语言增量 | v1 必做 | 已定义 | 2 | [0325](../archive/specs/0325-simple-variable-assignment-types.md) |
| ☐ 未开始 | L9c | is E.Case 从 subject 推断类型实参；L0086→L0084 诊断码统一 | P3 语言增量 | v1 建议 | 需新增 | 2 | — |
| ☐ 未开始 | L18 | value class 局部解构改为按字段名匹配（D15） | P3 语言增量 | v1 必做 | 需新增 | 1,2 | — |
| ☐ 未开始 | S2 | 泛型推导读取 lambda 返回值（两段式） | P3 语言增量 | v1 建议 | 需新增 | 2 | — |
| ☐ 未开始 | S4 | 无后缀整数字面量在二元运算中随对侧整数类型定型 | P3 语言增量 | v1 建议 | 需新增 | 2 | — |
| ⏸ 延后 | X06 | 静态不可变存储（顶层 static val，G2） | X v1 之外 | v1.x | 需新增 | 2,3,4 | [global-and-static-state](../proposals/global-and-static-state.md) |
| ⏸ 延后 | X11 | 泛型多 bound、where、associated type、扩展函数；const 泛型；默认参数/vararg/data class | X v1 之外 | v2 | 需新增 | 1,2 | [restricted-extension-functions](../proposals/restricted-extension-functions.md) |

## 控制流

| 状态 | ID | 内容 | 阶段 | 优先级 | Guide | Phase | Spec / 依据 |
|---|---|---|---|---|---|---|---|
| ▶ 进行中 | E1b | 声明/函数终止性：val 初始化全分支 return 与 L0088 入口一致 | P0 止血 | v1 必做 | 已定义 | 2 | [0326](../archive/specs/0326-closed-control-branch-join.md) |
| ◐ 部分 | N02 | when：Int/String/Char/MoveOnly enum subject、enum 方法内 when (this)、project 模式 enum case 条件 | P2 补齐现有 Guide 能力 | v1 必做 | 已定义 | 2,3,4 | [0241](../archive/specs/0241-return-control-operands.md) [0320](../archive/specs/0320-enum-case-borrow-categories.md) [0326](../archive/specs/0326-closed-control-branch-join.md) |
| ☐ 未开始 | L7 | MoveOnly enum 的消费式匹配：owned subject 分支获得 payload owned 绑定 | P3 语言增量 | v1 必做 | 需新增 | 2,3,4 | — |
| ✅ 已交付 | L9a | 封闭 if/when 结果 join：不再合并到 Any（D3） | P3 语言增量 | v1 必做 | 已定义 | 2 | [0326](../archive/specs/0326-closed-control-branch-join.md) |
| ✅ 已交付 | L15 | while 保守流事实：条件为真的 body 事实与写入/回边/break 失效 | P3 语言增量 | v1 必做 | 已定义 | 2 | [0330](../archive/specs/0330-while-conservative-flow.md) |
| ☐ 未开始 | S5 | while/for 循环标签 | P3 语言增量 | v1 建议 | 需新增 | 1,2,4 | — |

## 类、接口与成员

| 状态 | ID | 内容 | 阶段 | 优先级 | Guide | Phase | Spec / 依据 |
|---|---|---|---|---|---|---|---|
| ◐ 部分 | N06 | interface bound 泛型调用、companion 函数调用、interface 实现与默认方法（单文件） | P2 补齐现有 Guide 能力 | v1 必做 | 已定义 | 2,3,4 | [0323](../archive/specs/0323-final-admission-static-recovery.md) |
| ✅ 已交付 | N10 | 单文件独有缺口：普通 class 方法、泛型 value class 方法、class inout 方法、if 作值 | P2 补齐现有 Guide 能力 | v1 必做 | 已定义 | 4 | [0316](../archive/specs/0316-single-readonly-value-methods.md) |
| ✅ 已交付 | L19 | companion 语法改为 `companion { }`，语义不变（不产生对象）（D13） | P3 语言增量 | v1 必做 | 需新增 | 1,2 | [0342](../archive/specs/0342-companion-block-syntax.md) |
| ⏸ 延后 | X07 | dyn/interface 值、匿名对象 | X v1 之外 | v2 | 需新增 | 2,3,4 | [v2-interface-values-and-dynamic-dispatch](../proposals/v2-interface-values-and-dynamic-dispatch.md) |

## 空安全与错误值

| 状态 | ID | 内容 | 阶段 | 优先级 | Guide | Phase | Spec / 依据 |
|---|---|---|---|---|---|---|---|
| ✅ 已交付 | L12 | error()/!!/越界/溢出 abort 时向 stderr 打印消息与源码位置（D12） | P0 止血 | v1 必做 | 已定义 | 4,5 | [0347](../archive/specs/0347-abort-output.md) |
| ◐ 部分 | N03 | nullable 全路径：标量 T?、nullable 参数、Elvis、?.、!! | P2 补齐现有 Guide 能力 | v1 必做 | 已定义 | 2,3,4 | [0220](../archive/specs/0220-compilation-unit-pointer-nullable-storage-lowering.md) [0296](../archive/specs/0296-owned-null-comparison-cleanup.md) |
| ☐ 未开始 | N08 | Result + 后缀 ? 的 Int 与 String payload 全路径 | P2 补齐现有 Guide 能力 | v1 必做 | 已定义 | 2,3,4 | [09-nullability-errors](../guide/09-nullability-errors.md) |
| ☐ 未开始 | L17 | 丢弃 Result 返回值报诊断（must-use）（D14） | P3 语言增量 | v1 必做 | 需新增 | 2 | — |
| ☐ 未开始 | L20 | 可能 abort 的标准 API 必须提供不 abort 版本：getOrNull、checked 运算（返回 T?）等 | P3 语言增量 | v1 必做 | 需新增 | 2,4,5 | [v0.46-language-decisions](../proposals/v0.46-language-decisions.md) |

## 所有权、借用与析构

| 状态 | ID | 内容 | 阶段 | 优先级 | Guide | Phase | Spec / 依据 |
|---|---|---|---|---|---|---|---|
| ✅ 已交付 | B04 | 所有权事实：move/loan/drop/capture、结构移动、容器 element place、跨文件所有权 | S0 已交付基线 | v1 必做 | 已定义 | 3 | [ownership](../architecture/ownership.md) |
| ✅ 已交付 | B08 | replace/swap：owned root 与一级字段 | S0 已交付基线 | v1 必做 | 已定义 | 2,3,4 | [0232](../archive/specs/0232-ownership-primitive-type-facts.md) [0244](../archive/specs/0244-root-ownership-primitives.md) [0246](../archive/specs/0246-direct-field-replace.md) |
| ✅ 已交付 | B09 | deinit 双轨析构：body→字段逆序、控制退出与唯一回收 | S0 已交付基线 | v1 必做 | 已定义 | 2,3,4 | [0245](../archive/specs/0245-resource-deinit.md) [0273](../archive/specs/0273-control-body-resource-cleanup.md) [resource-deinit](../architecture/resource-deinit.md) |
| ✅ 已交付 | B10 | receiver 两阶段借用 | S0 已交付基线 | v1 必做 | 已定义 | 2,3,4 | [0243](../archive/specs/0243-receiver-two-phase-borrows.md) [receiver-borrows](../architecture/receiver-borrows.md) |
| ✅ 已交付 | B13 | M2B 普通借用返回：borrow T from param、多参数、混合 Value、同步 Borrow 实参、value class 字段/只读方法 | S0 已交付基线 | v1 必做 | 已定义 | 2,3,4 | [0305](../archive/specs/0305-multi-parameter-borrow-result.md) [0306](../archive/specs/0306-mixed-value-borrow-result.md) [0307](../archive/specs/0307-synchronous-borrow-result-arguments.md) [0309](../archive/specs/0309-borrow-value-class-fields.md) [0316](../archive/specs/0316-single-readonly-value-methods.md) |
| ☐ 未开始 | N01 | 赋值与可变性：字段赋值 obj.f = v、inout 标量参数赋值、inout class 参数、泛型 class 的 inout 方法 | P2 补齐现有 Guide 能力 | v1 必做 | 已定义 | 2,3,4 | [0325](../archive/specs/0325-simple-variable-assignment-types.md) |
| ☐ 未开始 | N09 | borrow T from this（泛型类 receiver 来源） | P2 补齐现有 Guide 能力 | v1 必做 | 已定义 | 2,3,4 | — |
| ☐ 未开始 | L11 | 泛型体内未约束 T 的析构轨道（按资源轨道词法作用域析构） | P3 语言增量 | v1 必做 | 需新增 | 2,3 | — |
| ☐ 未开始 | S7 | 只读 borrow var 游标（链表/树的迭代遍历） | P3 语言增量 | v1 建议 | 需新增 | 2,3 | — |
| ⏸ 延后 | X08 | 统一非逃逸类型：可存储/条件/多来源借用、inout 返回、用户迭代协议、Str/StringView 与切片 | X v1 之外 | v2 | 需新增 | 2,3,4 | — |

## 函数类型与闭包

| 状态 | ID | 内容 | 阶段 | 优先级 | Guide | Phase | Spec / 依据 |
|---|---|---|---|---|---|---|---|
| ◐ 部分 | N04 | 闭包：lambda 直接作实参、函数引用 ::f、move 闭包返回与存储 | P2 补齐现有 Guide 能力 | v1 必做 | 已定义 | 2,3,4 | [0278](../archive/specs/0278-borrowed-closure-escape-verification.md) [borrowed-closure-escape](../architecture/borrowed-closure-escape.md) |
| ☐ 未开始 | S1 | 非逃逸闭包以 exclusive loan 可变捕获 var（D7） | P3 语言增量 | v1 必做 | 需新增 | 2,3,4 | — |

## 集合与迭代

| 状态 | ID | 内容 | 阶段 | 优先级 | Guide | Phase | Spec / 依据 |
|---|---|---|---|---|---|---|---|
| ✅ 已交付 | B14 | Array/List/MutableList：构造、索引、Borrow 遍历、add/clear/removeAt/insert 等 | S0 已交付基线 | v1 必做 | 已定义 | 2,3,4 | [0182](../archive/specs/0182-sequential-for-lowering.md) [0279](../archive/specs/0279-runtime-length-container-native.md) [0280](../archive/specs/0280-mutable-list-add.md) [0281](../archive/specs/0281-mutable-list-clear.md) [0282](../archive/specs/0282-mutable-list-remove-at.md) [0283](../archive/specs/0283-mutable-list-remove-last.md) [0284](../archive/specs/0284-mutable-list-remove-first.md) [0285](../archive/specs/0285-mutable-list-insert-at.md) [0321](../archive/specs/0321-mutable-list-element-delivery.md) |
| ✅ 已交付 | B15 | Map/MutableMap 基础：put/get/remove、scoped 借用访问、条件 break/continue/return | S0 已交付基线 | v1 必做 | 已定义 | 2,3,4 | [0287](../archive/specs/0287-m2b-and-map-type-system.md) [0288](../archive/specs/0288-map-native-execution.md) [0294](../archive/specs/0294-map-put-control-exit.md) [0297](../archive/specs/0297-map-key-return-cleanup.md) [0299](../archive/specs/0299-owned-map-result-borrow.md) [0300](../archive/specs/0300-single-map-loop-owner-carry.md) [0310](../archive/specs/0310-single-map-conditional-break.md) [0311](../archive/specs/0311-single-map-conditional-continue.md) [0313](../archive/specs/0313-single-map-conditional-return.md) [0318](../archive/specs/0318-map-return-operand-last-use.md) [map-native-execution](../architecture/map-native-execution.md) |
| ✅ 已交付 | B16 | N1a 连续范围 carrier：take/drop/dropLast 与范围绑定局部控制流 | S0 已交付基线 | v1 必做 | 已定义 | 2,3,4 | [0289](../archive/specs/0289-n1a-range-carrier.md) [0298](../archive/specs/0298-range-binding-local-control-flow.md) [0315](../archive/specs/0315-range-int-comparisons.md) [0317](../archive/specs/0317-range-normal-short-circuit.md) |
| ✅ 已交付 | L4 | IntRange/LongRange 可存储 Copyable 值、in / not in、升序基本 for | P3 语言增量 | v1 必做 | 已定义 | 2,3,4 | [0328](../archive/specs/0328-integer-range-values.md) [0329](../archive/specs/0329-integer-range-iteration.md) |
| ✅ 已交付 | L13 | Map 插入序借用遍历（D8） | P3 语言增量 | v1 必做 | 已定义 | 2,3,4 | [0331](../archive/specs/0331-map-borrowed-iteration.md) |
| ✅ 已交付 | L13b | MutableMap.consume 整体移交为只读 Map | P3 语言增量 | v1 必做 | 已定义 | 2,3,4 | [0291](../specs/active/0291-readonly-map-construction.md) |
| ☐ 未开始 | L16 | 同步、非逃逸 Borrow 回调的 forEach 与 List/Map 高层基础 API | P3 语言增量 | v1 必做 | 需新增 | 2,3,4,5 | [collection-api-followups](../proposals/collection-api-followups.md) |
| ☐ 未开始 | S3 | owned 集合算法：map/filter/fold/sort/contains/indexOf/reverse | P3 语言增量 | v1 建议 | 需新增 | 2,3,4,5 | [collection-algorithm-ownership-r3](../proposals/collection-algorithm-ownership-r3.md) |
| ☐ 未开始 | S6 | Map 带初值构造 mapOf("a" to 1) | P3 语言增量 | v1 建议 | 需新增 | 2,3,4 | — |
| ⏸ 延后 | X04 | N1b、filter 视图、consume 链；step/downTo 与反向迭代 | X v1 之外 | v1.x | 需新增 | 2,3,4 | [0292](../specs/drafts/v0.43/0292-immediate-consuming-sequence.md) |

## 文本与 IO

| 状态 | ID | 内容 | 阶段 | 优先级 | Guide | Phase | Spec / 依据 |
|---|---|---|---|---|---|---|---|
| ✅ 已交付 | B07 | String：字面量、拼接、内容比较、String.clone、println(String) | S0 已交付基线 | v1 必做 | 已定义 | 2,3,4 | [0189](../archive/specs/0189-standard-println-output.md) [0236](../archive/specs/0236-explicit-string-clone.md) [string-clone](../architecture/string-clone.md) |
| ✅ 已交付 | B22 | argv 词频程序（Map 版）与 main(args) | S0 已交付基线 | v1 必做 | 已定义 | 2,3,4,6 | [0274](../archive/specs/0274-argv-word-frequency.md) [0295](../archive/specs/0295-map-argv-word-frequency.md) |
| ☐ 未开始 | L2 | 字符串插值降级："${x}" 要求 x : Display（D2） | P3 语言增量 | v1 必做 | 需新增 | 2,4 | — |
| ☐ 未开始 | L5 | 文本 API：size、Char 迭代、byteAt、substring、indexOf、split、trim、lines、startsWith、toInt(): Int?、StringBuilder | P4 文本与 IO | v1 必做 | 需新增 | 2,4,5 | [text-input-and-io](../proposals/text-input-and-io.md) |
| ☐ 未开始 | L6 | IO 与进程：readLine(): String?、File.readText/writeText: Result、eprintln、exitProcess(code): Nothing（D9） | P4 文本与 IO | v1 必做 | 需新增 | 2,4,5 | [text-input-and-io](../proposals/text-input-and-io.md) |
| ☐ 未开始 | L21 | main 可返回 Result<Unit, E>（E : Display）：Err 时 stderr 输出 `error: …`，退出码 1（D9） | P4 文本与 IO | v1 必做 | 需新增 | 2,4,5 | — |

## 标准库与运行时

| 状态 | ID | 内容 | 阶段 | 优先级 | Guide | Phase | Spec / 依据 |
|---|---|---|---|---|---|---|---|
| ✅ 已交付 | B11 | 非泛型递归 Box enum 的构造、运输与析构 | S0 已交付基线 | v1 必做 | 已定义 | 2,3,4 | [0230](../archive/specs/0230-recursive-boxed-enum-native.md) |
| ✅ 已交付 | B17 | Rc<T> 核心路径与 Koven prelude（error、Pair、Result 源码） | S0 已交付基线 | v1 必做 | 已定义 | 2,3,4 | — |
| ☐ 未开始 | E4 | Result/Ok/Err 注入内建环境（单文件与 project 相同）；kovenc check 与 --help | P0 止血 | v1 必做 | 已定义 | 5,6 | — |
| ☐ 未开始 | N07 | Box.value / unbox() 与 Rc.value 字段投影 | P2 补齐现有 Guide 能力 | v1 必做 | 已定义 | 2,3,4 | — |
| ☐ 未开始 | L8 | Box 访问面措辞修正：box.value 为只读 projection place、unbox(): T | P3 语言增量 | v1 必做 | 需新增 | 2 | — |
| ☐ 未开始 | T1 | 受限线程：thread(move { }) : Thread<T>、join()、channel（多生产者单消费者）；drop 时隐式 join；worker abort 即进程 abort；只转移所有权（D5） | P4 文本与 IO | v1 建议 | 需新增 | 2,3,4,5 | [thread-transfer](../proposals/thread-transfer.md) |
| ☐ 未开始 | L6b | time/sleep/math 基础 API | P4 文本与 IO | v1 必做 | 需新增 | 2,4,5 | [text-input-and-io](../proposals/text-input-and-io.md) |
| ⏸ 延后 | X05 | Weak<T> 与 Rc 环处理；Copyable 显式 opt-in/opt-out | X v1 之外 | v1.x | 需新增 | 2,5 | — |
| ⏸ 延后 | X09 | Shareable、Arc/Mutex/原子、scoped threads；Lazy 与共享可变全局（G3/G4） | X v1 之外 | v2 | 需新增 | 2,3,4,5 | [global-and-static-state](../proposals/global-and-static-state.md) |
| ⏸ 延后 | X10 | unsafe、RawPtr、extern "C"、标准库自写（D6） | X v1 之外 | v2 | 需新增 | 1,2,4,5 | — |

## 编译器结构

| 状态 | ID | 内容 | 阶段 | 优先级 | Guide | Phase | Spec / 依据 |
|---|---|---|---|---|---|---|---|
| ✅ 已交付 | B01 | Cargo workspace、索引式 AST、诊断与 fixture 骨架 | S0 已交付基线 | v1 必做 | — | 0 | [pipeline-and-workspace](../architecture/pipeline-and-workspace.md) |
| ✅ 已交付 | B05 | typed SSA → LLVM → 本机可执行文件；AArch64 macOS 与 x86_64 Linux glibc 双宿主 | S0 已交付基线 | v1 必做 | — | 4 | [0228](../archive/specs/0228-linux-x86-64-native-host.md) [0239](../archive/specs/0239-linux-ci-gates.md) [ssa-codegen-runtime](../architecture/ssa-codegen-runtime.md) |
| ✅ 已交付 | B12 | M1A 多文件程序贯通：跨文件字段可变性、直接字段 Borrow、unit for | S0 已交付基线 | v1 必做 | 已定义 | 2,3,4,6 | [0263](../archive/specs/0263-unit-field-mutability.md) [0264](../archive/specs/0264-unit-direct-field-borrow.md) [0265](../archive/specs/0265-unit-iteration-facts.md) [0268](../archive/specs/0268-unit-iteration-native.md) |
| ✅ 已交付 | E1 | 前端硬门禁：Deferred/Error 不进入所有权与 codegen；未知成员报诊断；内部错误结构化；L0165 用户措辞 | P0 止血 | v1 必做 | — | 2 | [0319](../archive/specs/0319-final-type-admission-diagnostics.md) [0323](../archive/specs/0323-final-admission-static-recovery.md) [0324](../archive/specs/0324-final-admission-user-messages.md) |
| ▶ 进行中 | E2 | 管线合并：单文件走 compilation-unit 管线，删除重复的 checker/lower_frontend（D10） | P1 结构 | v1 必做 | — | 2,3,4 | [0341](../specs/active/0341-shared-lowering-core.md) |
| ☐ 未开始 | E3 | 通用所有权细化层：typed + ownership facts → 显式 move/drop/loan 指令 + CFG → SSA（D10） | P1 结构 | v1 必做 | — | 3,4 | — |
| ⏸ 延后 | X12 | 协程/async 与自举 | X v1 之外 | v3+ | 需新增 | — | — |

## 工具链

| 状态 | ID | 内容 | 阶段 | 优先级 | Guide | Phase | Spec / 依据 |
|---|---|---|---|---|---|---|---|
| ✅ 已交付 | B18 | kovenc build/run、本地 project、JSON Lines 诊断、LSP、formatter、TextMate 与 Tree-sitter | S0 已交付基线 | v1 必做 | 已定义 | 6 | [0187](../archive/specs/0187-multifile-lsp-diagnostics-definition.md) [0251](../archive/specs/0251-lsp-unit-name-snapshot.md) [0267](../archive/specs/0267-editor-corpus-gate.md) [tooling](../architecture/tooling.md) |

## 安全验证

| 状态 | ID | 内容 | 阶段 | 优先级 | Guide | Phase | Spec / 依据 |
|---|---|---|---|---|---|---|---|
| ✅ 已交付 | B20 | Linux ASan/LSan 有界接线与资源程序随机生成首片（M4b） | S0 已交付基线 | v1 必做 | — | 4 | [0266](../archive/specs/0266-native-sanitizer-wiring.md) [0269](../archive/specs/0269-generated-resource-programs.md) |
| ◐ 部分 | V3 | Linux ASan/LSan 覆盖整个矩阵与六个验收程序 | P5 验收与发布 | v1 必做 | — | 4 | [0266](../archive/specs/0266-native-sanitizer-wiring.md) |
| ☐ 未开始 | V4 | M4b 随机生成扩展到 E3 之后的通用 lowering | P5 验收与发布 | v1 建议 | — | 4 | [0269](../archive/specs/0269-generated-resource-programs.md) [native-safety-validation](../proposals/native-safety-validation.md) |
| ⏸ 延后 | X02 | macOS 动态检测、UBSan、外部审计（M4 其余） | X v1 之外 | v1.x | — | 4 | — |

## 验收、发布与文档

| 状态 | ID | 内容 | 阶段 | 优先级 | Guide | Phase | Spec / 依据 |
|---|---|---|---|---|---|---|---|
| ✅ 已交付 | B19 | 可执行 tour 与 Guide Litmus 门禁 | S0 已交付基线 | v1 必做 | — | 6 | [0238](../archive/specs/0238-guide-litmus-gate.md) [0262](../archive/specs/0262-current-tutorial-plan-coverage.md) [0271](../archive/specs/0271-tour-combination-coverage.md) [README](../tutorials/README.md) |
| ✅ 已交付 | B21 | 双宿主 preview 包与独立安装验收 | S0 已交付基线 | v1 必做 | — | 6 | [0272](../archive/specs/0272-preview-candidate-package.md) [0322](../archive/specs/0322-preview-libxml2-host-compatibility.md) |
| ✅ 已交付 | B25 | Guide 拆分首片：Compiler Contracts 独立真源（D11） | S0 已交付基线 | v1 建议 | — | — | [0233](../archive/specs/0233-parser-compiler-contracts.md) |
| ☐ 未开始 | V0 | 154 个 probe 与六个验收程序源码入库，作为 conformance 种子与切换门槛的固定集合 | P0 止血 | v1 必做 | — | 6 | — |
| ✅ 已交付 | G46 | Guide v0.46 决策批：E4、D1、D2、D4、D7、D9、D12–D15、L14 与错误模型 | P0 止血 | v1 必做 | 需新增 | — | [v0.46-language-decisions](../proposals/v0.46-language-decisions.md) |
| ☐ 未开始 | L14 | 把 v1 不做的内容移出 Guide：unsafe/extern/RawPtr、线程/channel 预声明 API（D6） | P3 语言增量 | v1 必做 | 需新增 | — | — |
| ☐ 未开始 | V1 | 六个验收程序（词频、配置解析、表达式求值、RAII 状态机、二进制编解码、链表/栈）双宿主运行正确且 ASan/LSan 干净 | P5 验收与发布 | v1 必做 | — | 6 | — |
| ◐ 部分 | V2 | Conformance 矩阵：按 Guide 每章正反例，合法 100% 运行、非法 100% 前端诊断、0 ICE | P5 验收与发布 | v1 必做 | — | 2,3,4 | — |
| ◐ 部分 | V5 | README、教程、Architecture 按实测能力重写；Guide 不含“当前实现到哪一步”的句子 | P5 验收与发布 | v1 必做 | — | — | — |
| ◐ 部分 | V6 | v1.0 发行：双宿主安装包、使用材料与已知限制 | P5 验收与发布 | v1 必做 | — | 6 | [0272](../archive/specs/0272-preview-candidate-package.md) [measurement-distribution](../proposals/measurement-distribution.md) |
| ⏸ 延后 | X03 | 性能基线与正式分发（M5 其余） | X v1 之外 | v1.x | — | — | [measurement-distribution](../proposals/measurement-distribution.md) |
