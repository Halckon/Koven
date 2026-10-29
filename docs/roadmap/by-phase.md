# Koven v1 Roadmap：按编译 Phase

> **性质**：生成视图 · **状态**：current（生成自 [v1.json](v1.json)） · **读取时机**：查看某一编译阶段承担哪些 v1 工作时 · **唯一真源**：[v1.json](v1.json)；语义归 Guide，验收证据归各 Spec

<!-- 生成文件：请编辑 docs/roadmap/v1.json 后运行 python3 scripts/gen_roadmap.py，不要手改本页。 -->

基线：main 5b7f3e97（PR #185 强 Oracle 审计账本与 13 章覆盖 + PR #186 SPEC-0356 when 家族 + PR #187 SPEC-0358 赋值与 inout + V2 Slice 6 未实现 Probe 路线图阻塞项对账）· Guide v0.46 · 更新：2026-10-27 · 其他视图：[按阶段](by-stage.md) · [按领域](by-area.md) · 维护规则见 [README](README.md)

只列 v1 范围内条目；● 表示该条目需要在对应 Phase 产出或验收。Phase 定义见 [一致性与实施边界](../guide/15-conformance-and-staging.md)。

## 总览

| Phase | ✅ 已交付 | ◐ 部分 | ▶ 进行中 | ☐ 未开始 | ❓ 待决策 | ⏸ 延后 | 合计 |
|---|---:|---:|---:|---:|---:|---:|---:|
| Phase 0 骨架 | 1 | 0 | 0 | 0 | 0 | 0 | 1 |
| Phase 1 Parser | 2 | 0 | 0 | 2 | 0 | 0 | 4 |
| Phase 2 名称/类型 | 24 | 8 | 2 | 24 | 0 | 0 | 58 |
| Phase 3 所有权 | 18 | 7 | 2 | 10 | 0 | 0 | 37 |
| Phase 4 SSA/native | 21 | 8 | 3 | 19 | 0 | 0 | 51 |
| Phase 5 标准库 | 2 | 0 | 0 | 9 | 0 | 0 | 11 |
| Phase 6 工具 | 7 | 1 | 1 | 1 | 0 | 0 | 10 |

## 矩阵

| 状态 | ID | 内容 | 阶段 | Guide | P0 | P1 | P2 | P3 | P4 | P5 | P6 |
|---|---|---|---|---|---|---|---|---|---|---|---|
| ✅ 已交付 | B01 | Cargo workspace、索引式 AST、诊断与 fixture 骨架 | S0 | — | ● |  |  |  |  |  |  |
| ✅ 已交付 | B02 | Lexer/Parser 与错误恢复：完整文件组合、换行分句与续行、上下文 TypeRef | S0 | 已定义 |  | ● |  |  |  |  |  |
| ✅ 已交付 | B03 | 名称与类型事实：单文件与 compilation-unit、package/import、名义/泛型/interface、单态化 | S0 | 已定义 |  |  | ● |  |  |  |  |
| ✅ 已交付 | B04 | 所有权事实：move/loan/drop/capture、结构移动、容器 element place、跨文件所有权 | S0 | 已定义 |  |  |  | ● |  |  |  |
| ✅ 已交付 | B05 | typed SSA → LLVM → 本机可执行文件；AArch64 macOS 与 x86_64 Linux glibc 双宿主 | S0 | — |  |  |  |  | ● |  |  |
| ✅ 已交付 | B06 | const val 封闭求值、数值字面量（radix/下划线）与整数具名位运算 | S0 | 已定义 |  |  | ● | ● | ● |  |  |
| ✅ 已交付 | B07 | String：字面量、拼接、内容比较、String.clone、println(String) | S0 | 已定义 |  |  | ● | ● | ● |  |  |
| ✅ 已交付 | B08 | replace/swap：owned root 与一级字段 | S0 | 已定义 |  |  | ● | ● | ● |  |  |
| ✅ 已交付 | B09 | deinit 双轨析构：body→字段逆序、控制退出与唯一回收 | S0 | 已定义 |  |  | ● | ● | ● |  |  |
| ✅ 已交付 | B10 | receiver 两阶段借用 | S0 | 已定义 |  |  | ● | ● | ● |  |  |
| ✅ 已交付 | B11 | 非泛型递归 Box enum 的构造、运输与析构 | S0 | 已定义 |  |  | ● | ● | ● |  |  |
| ✅ 已交付 | B12 | M1A 多文件程序贯通：跨文件字段可变性、直接字段 Borrow、unit for | S0 | 已定义 |  |  | ● | ● | ● |  | ● |
| ✅ 已交付 | B13 | M2B 普通借用返回：borrow T from param、多参数、混合 Value、同步 Borrow 实参、value class 字段/只读方法 | S0 | 已定义 |  |  | ● | ● | ● |  |  |
| ✅ 已交付 | B14 | Array/List/MutableList：构造、索引、Borrow 遍历、add/clear/removeAt/insert 等 | S0 | 已定义 |  |  | ● | ● | ● |  |  |
| ✅ 已交付 | B15 | Map/MutableMap 基础：put/get/remove、scoped 借用访问、条件 break/continue/return | S0 | 已定义 |  |  | ● | ● | ● |  |  |
| ✅ 已交付 | B16 | N1a 连续范围 carrier：take/drop/dropLast 与范围绑定局部控制流 | S0 | 已定义 |  |  | ● | ● | ● |  |  |
| ✅ 已交付 | B17 | Rc<T> 核心路径与 Koven prelude（error、Pair、Result 源码） | S0 | 已定义 |  |  | ● | ● | ● |  |  |
| ✅ 已交付 | B18 | kovenc build/run、本地 project、JSON Lines 诊断、LSP、formatter、TextMate 与 Tree-sitter | S0 | 已定义 |  |  |  |  |  |  | ● |
| ▶ 进行中 | B18d | CLI 最终调试产物与双宿主源码断点 | P0 | 已定义 |  |  |  |  | ● |  | ● |
| ✅ 已交付 | B19 | 可执行 tour 与 Guide Litmus 门禁 | S0 | — |  |  |  |  |  |  | ● |
| ✅ 已交付 | B20 | Linux ASan/LSan 有界接线与资源程序随机生成首片（M4b） | S0 | — |  |  |  |  | ● |  |  |
| ✅ 已交付 | B21 | 双宿主 preview 包与独立安装验收 | S0 | — |  |  |  |  |  |  | ● |
| ✅ 已交付 | B22 | argv 词频程序（Map 版）与 main(args) | S0 | 已定义 |  |  | ● | ● | ● |  | ● |
| ✅ 已交付 | B25 | Guide 拆分首片：Compiler Contracts 独立真源（D11） | S0 | — |  |  |  |  |  |  |  |
| ✅ 已交付 | E1 | 前端硬门禁：Deferred/Error 不进入所有权与 codegen；未知成员报诊断；内部错误结构化；L0165 用户措辞 | P0 | — |  |  | ● |  |  |  |  |
| ▶ 进行中 | E1b | 声明/函数终止性：val 初始化全分支 return 与 L0088 入口一致 | P0 | 已定义 |  |  | ● |  |  |  |  |
| ✅ 已交付 | E4 | Result/Ok/Err 注入内建环境（单文件与 project 相同）；kovenc check 与 --help | P0 | 已定义 |  |  |  |  |  | ● | ● |
| ✅ 已交付 | L10 | 顶层声明收窄：只允许 const val；顶层运行时 val/var 结构化拒绝（D4） | P0 | 需新增 |  |  | ● |  |  |  |  |
| ✅ 已交付 | L12 | error()/!!/越界/溢出 abort 时向 stderr 打印消息与源码位置（D12） | P0 | 已定义 |  |  |  |  | ● | ● |  |
| ✅ 已交付 | V0 | 154 个 probe 与六个验收程序源码入库，作为 conformance 种子与切换门槛的固定集合 | P0 | — |  |  |  |  |  |  | ● |
| ✅ 已交付 | G46 | Guide v0.46 决策批：E4、D1、D2、D4、D7、D9、D12–D15、L14 与错误模型 | P0 | 需新增 |  |  |  |  |  |  |  |
| ▶ 进行中 | E2 | 管线合并：单文件走 compilation-unit 管线，删除重复的 checker/lower_frontend（D10） | P1 | — |  |  | ● | ● | ● |  |  |
| ▶ 进行中 | E3 | 通用所有权细化层：typed + ownership facts → 显式 move/drop/loan 指令 + CFG → SSA（D10） | P1 | — |  |  |  | ● | ● |  |  |
| ◐ 部分 | N01 | 赋值与可变性：字段赋值 obj.f = v、inout 标量参数赋值、inout class 参数、泛型 class 的 inout 方法 | P2 | 已定义 |  |  | ● | ● | ● |  |  |
| ◐ 部分 | N02 | when：Int/String/Char/MoveOnly enum subject、enum 方法内 when (this)、project 模式 enum case 条件 | P2 | 已定义 |  |  | ● | ● | ● |  |  |
| ◐ 部分 | N03 | nullable 全路径：标量 T?、nullable 参数、Elvis、?.、!! | P2 | 已定义 |  |  | ● | ● | ● |  |  |
| ◐ 部分 | N04 | 闭包：lambda 直接作实参、函数引用 ::f、move 闭包返回与存储 | P2 | 已定义 |  |  | ● | ● | ● |  |  |
| ☐ 未开始 | N05 | Double/Float 运算与 Char 比较的 native 支持 | P2 | 已定义 |  |  |  |  | ● |  |  |
| ◐ 部分 | N06 | interface bound 泛型调用、companion 函数调用、interface 实现与默认方法（单文件） | P2 | 已定义 |  |  | ● | ● | ● |  |  |
| ◐ 部分 | N07 | Box.value / unbox() 与 Rc.value 字段投影 | P2 | 已定义 |  |  | ● | ● | ● |  |  |
| ☐ 未开始 | N08 | Result + 后缀 ? 的 Int 与 String payload 全路径 | P2 | 已定义 |  |  | ● | ● | ● |  |  |
| ☐ 未开始 | N09 | borrow T from this（泛型类 receiver 来源） | P2 | 已定义 |  |  | ● | ● | ● |  |  |
| ✅ 已交付 | N10 | 单文件独有缺口：普通 class 方法、泛型 value class 方法、class inout 方法、if 作值 | P2 | 已定义 |  |  |  |  | ● |  |  |
| ☐ 未开始 | L1 | 值语义协议 Equatable/Hashable/Display（Comparable 只给内建数值、Char、String）；value class/enum 自动推导相等与哈希；Map 键 K : Equatable + Hashable（D1） | P3 | 需新增 |  |  | ● |  | ● | ● |  |
| ☐ 未开始 | L2 | 字符串插值降级："${x}" 要求 x : Display（D2） | P3 | 需新增 |  |  | ● |  | ● |  |  |
| ✅ 已交付 | L3 | 八种整数之间的 checked 转换（越界 Abort） | P3 | 已定义 |  |  | ● | ● | ● |  |  |
| ☐ 未开始 | L3b | 转换族其余部分：OrNull 变体、浮点↔整数 checked、Char 码点 | P3 | 需新增 |  |  | ● |  | ● |  |  |
| ✅ 已交付 | L4 | IntRange/LongRange 可存储 Copyable 值、in / not in、升序基本 for | P3 | 已定义 |  |  | ● | ● | ● |  |  |
| ☐ 未开始 | L7 | MoveOnly enum 的消费式匹配：owned subject 分支获得 payload owned 绑定 | P3 | 需新增 |  |  | ● | ● | ● |  |  |
| ☐ 未开始 | L8 | Box 访问面措辞修正：box.value 为只读 projection place、unbox(): T | P3 | 需新增 |  |  | ● |  |  |  |  |
| ✅ 已交付 | L9a | 封闭 if/when 结果 join：不再合并到 Any（D3） | P3 | 已定义 |  |  | ● |  |  |  |  |
| ✅ 已交付 | L9b | 简单变量普通赋值的类型、Unit 结果与收窄事实失效 | P3 | 已定义 |  |  | ● |  |  |  |  |
| ◐ 部分 | L9c | is E.Case 从 subject 推断类型实参；L0086→L0084 诊断码统一 | P3 | 需新增 |  |  | ● |  |  |  |  |
| ☐ 未开始 | L11 | 泛型体内未约束 T 的析构轨道（按资源轨道词法作用域析构） | P3 | 需新增 |  |  | ● | ● |  |  |  |
| ✅ 已交付 | L13 | Map 插入序借用遍历（D8） | P3 | 已定义 |  |  | ● | ● | ● |  |  |
| ✅ 已交付 | L13b | MutableMap.consume 整体移交为只读 Map | P3 | 已定义 |  |  | ● | ● | ● |  |  |
| ✅ 已交付 | L15 | while 保守流事实：条件为真的 body 事实与写入/回边/break 失效 | P3 | 已定义 |  |  | ● |  |  |  |  |
| ☐ 未开始 | L16 | 同步、非逃逸 Borrow 回调的 forEach 与 List/Map 高层基础 API | P3 | 需新增 |  |  | ● | ● | ● | ● |  |
| ☐ 未开始 | L17 | 丢弃 Result 返回值报诊断（must-use）（D14） | P3 | 需新增 |  |  | ● |  |  |  |  |
| ☐ 未开始 | L18 | value class 局部解构改为按字段名匹配（D15） | P3 | 需新增 |  | ● | ● |  |  |  |  |
| ✅ 已交付 | L19 | companion 语法改为 `companion { }`，语义不变（不产生对象）（D13） | P3 | 需新增 |  | ● | ● |  |  |  |  |
| ☐ 未开始 | L20 | 可能 abort 的标准 API 必须提供不 abort 版本：getOrNull、checked 运算（返回 T?）等 | P3 | 需新增 |  |  | ● |  | ● | ● |  |
| ☐ 未开始 | L14 | 把 v1 不做的内容移出 Guide：unsafe/extern/RawPtr、线程/channel 预声明 API（D6） | P3 | 需新增 |  |  |  |  |  |  |  |
| ☐ 未开始 | S1 | 非逃逸闭包以 exclusive loan 可变捕获 var（D7） | P3 | 需新增 |  |  | ● | ● | ● |  |  |
| ☐ 未开始 | S2 | 泛型推导读取 lambda 返回值（两段式） | P3 | 需新增 |  |  | ● |  |  |  |  |
| ☐ 未开始 | S3 | owned 集合算法：map/filter/fold/sort/contains/indexOf/reverse | P3 | 需新增 |  |  | ● | ● | ● | ● |  |
| ☐ 未开始 | S4 | 无后缀整数字面量在二元运算中随对侧整数类型定型 | P3 | 需新增 |  |  | ● |  |  |  |  |
| ☐ 未开始 | S5 | while/for 循环标签 | P3 | 需新增 |  | ● | ● |  | ● |  |  |
| ☐ 未开始 | S6 | Map 带初值构造 mapOf("a" to 1) | P3 | 需新增 |  |  | ● | ● | ● |  |  |
| ☐ 未开始 | S7 | 只读 borrow var 游标（链表/树的迭代遍历） | P3 | 需新增 |  |  | ● | ● |  |  |  |
| ☐ 未开始 | L5 | 文本 API：size、Char 迭代、byteAt、substring、indexOf、split、trim、lines、startsWith、toInt(): Int?、StringBuilder | P4 | 需新增 |  |  | ● |  | ● | ● |  |
| ☐ 未开始 | L6 | IO 与进程：readLine(): String?、File.readText/writeText: Result、eprintln、exitProcess(code): Nothing（D9） | P4 | 需新增 |  |  | ● |  | ● | ● |  |
| ☐ 未开始 | L21 | main 可返回 Result<Unit, E>（E : Display）：Err 时 stderr 输出 `error: …`，退出码 1（D9） | P4 | 需新增 |  |  | ● |  | ● | ● |  |
| ☐ 未开始 | T1 | 受限线程：thread(move { }) : Thread<T>、join()、channel（多生产者单消费者）；drop 时隐式 join；worker abort 即进程 abort；只转移所有权（D5） | P4 | 需新增 |  |  | ● | ● | ● | ● |  |
| ☐ 未开始 | L6b | time/sleep/math 基础 API | P4 | 需新增 |  |  | ● |  | ● | ● |  |
| ☐ 未开始 | V1 | 六个验收程序（词频、配置解析、表达式求值、RAII 状态机、二进制编解码、链表/栈）双宿主运行正确且 ASan/LSan 干净 | P5 | — |  |  |  |  |  |  | ● |
| ◐ 部分 | V2 | Conformance 矩阵：按 Guide 每章正反例，合法 100% 运行、非法 100% 前端诊断、0 ICE | P5 | — |  |  | ● | ● | ● |  |  |
| ◐ 部分 | V3 | Linux ASan/LSan 覆盖整个矩阵与六个验收程序 | P5 | — |  |  |  |  | ● |  |  |
| ☐ 未开始 | V4 | M4b 随机生成扩展到 E3 之后的通用 lowering | P5 | — |  |  |  |  | ● |  |  |
| ◐ 部分 | V5 | README、教程、Architecture 按实测能力重写；Guide 不含“当前实现到哪一步”的句子 | P5 | — |  |  |  |  |  |  |  |
| ◐ 部分 | V6 | v1.0 发行：双宿主安装包、使用材料与已知限制 | P5 | — |  |  |  |  |  |  | ● |
