# SPEC-0299: owned Map 删除结果的非消费借用

> **性质**：变更合同 · **状态**：in-progress · **读取时机**：准备窄化owned Map结果的只读Borrow时 · **唯一真源**：本页的有界实施范围；语义以Guide为准

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P4-0299` |
| 所属 Phase | Phase 3事实消费、Phase 4 nullable place/loan与native |
| 语言规范 | [Guide v0.43](../../guide/README.md)、[Map](../../guide/12-collections-destructuring.md)、[所有权](../../guide/10-ownership-borrowing-drop.md) |
| 批准依据 | 2026-10-12按已确认0300→0299/0298顺序，从真实main `08c6b8dc` 在独立 `fix/spec-0299` 分支本地实施；发布与CI另行确认 |
| 前置 Spec | [SPEC-0295](../../archive/specs/0295-map-argv-word-frequency.md)、[SPEC-0296](../../archive/specs/0296-owned-null-comparison-cleanup.md) |
| 前置 ADR | [ADR-0016](../../adr/accepted/0016-interprocedural-borrow-abi.md)、[ADR-0017](../../adr/accepted/0017-nullable-handle-ssa-abi.md) |
| 阻塞项 | 非消费payload place/proof、真实owner关联及调用期loan事实须先证明 |
| 影响范围 | 两入口窄化Map结果的Borrow实参、SSA/verifier直接消费者 |
| 语言语义变更 | 否；不启用新nullable表示或借用语法 |

## 1. Goal 与重起草来源

对`remove`已交付的具名owned nullable结果，在已验证非空分支同步交付只读Borrow，
支持其MoveOnly payload而不复制、clone、retain或消费原owner；调用结束后该owned结果仍按
原事实存活、可再次合法使用并恰一次清理。

2026-10-12原未发布草案丢失，本页依据已核边界重新起草，不是原稿恢复。原审计见
[历史摘要](../../archive/migrations/pr72-74-followup-audit.md)；本页不重用其旧静态数字作为新验收。

## 2. 起点与范围

0295的两入口桥接核验typed non-null proof和注册Map结果后，只为Copyable payload复制
inner再借用；其Copyable门不是所有Borrow的语言限制。现行`get`/下标的MoveOnly按值查询
仍必须拒绝。owned `remove`返回、`!!`消费和本片非消费Borrow是不同合同。

首片为具名whole-root删除结果、非nullable V及同步Borrow参数。核对声明/窄化类型、symbol、
真实结果owner、非空proof与支配关系；保留已有表示，不将presence/payload aggregate
直接解释为inner RootPlace。loan依赖被删除值的新owner，不误绑Map entry或key。

String、现有可存储MoveOnly值及Resource分别核范围；原已支持分流保留，不能为统一表面
制造新复制。相同值的连续Borrow调用后仍可合法读取/消费，调用结束只结束借用，不drop payload。
缺失分支不能建立可读place。沿用frontend事实；事实/ABI不足时停下，不以去掉Copyable判断代替实现。

## 3. 非目标

不扩MoveOnly get、不改变withValue/requireValue、条件借用或nullable V的owned三态表示；
不做通用nullable ABI、普通Resource?工厂、字段/元素/temporary结果全覆盖、Inout或逃逸Borrow。
不顺带重写0296判空最后使用清理或0300循环owner运输，不改变普通Borrow/Copy语义。

`break`/`continue`及其他实参提前退出仍是独立后继候选，非本片验收；
后继必须先证明精确loop目标、未求值后缀、前缀owner/loan及provider逐边清理，不由普通调用成功推定。

## 4. 冻结首片与最小验收

首片只选直接 `if (result != null)` / `if (result == null) ... else ...` 与透明括号条件，
不扩大短路条件、任意when、循环、borrow val或其他实参提前退出。single现有直接条件proof分流
与unit普通条件分别先跑同源红测；不能由两处Copyable门推定两入口全部失败。
类型先分String、含String的MoveOnly value class、Resource class三种既有表示，
Copyable Int/Boolean/value class保留原桥。消费式 `!!` 只作借用完成后的独立对照。

## 5. 最小验收与交付

- [ ] single/unit分别建立具名remove结果窄化Borrow的普通源码红测，区分现有成功路径与真正缺口。
- [ ] String、MoveOnly、Resource及Copyable对照的SSA/verifier、未改写LLVM和正常native验证；
  新增MoveOnly payload的非消费Borrow路径不得出现Copy/Clone/retain或消费式NullableTake；
  Copyable对照保留既有合法复制桥，不要求重写为同一实现。
- [ ] 同一非空结果连续两次Borrow后再合法消费或scope清理；独立可观察资源事件无提前/重复析构。
- [ ] 缺失、错误窄化、借用期间冲突/逃逸及MoveOnly get保持拒绝；原Copyable桥和0296判空回归保持。
- [ ] 独立分支按红测→最小实现→直接回归→Architecture/验收推进；正式发布和CI另行闭环。

## 6. 唯一验收账本

基线 `08c6b8dcbb8e69f20a7dd82bbd0426b0df97c4c9`（tree `3b081d41227420bd61ca7751436d24147923d4aa`）。
0300已由PR79合并；其actual-main CI [38259510177](https://github.com/Halckon/Koven/actions/runs/38259510177)
启动时pending，不作为0299验收。本片尚未运行功能验证；以下逐项登记真实结果。

| 验收项 | 测试目标/过滤器 | 实际结果 |
|---|---|---|
| plain/group single/unit普通源码基线 | 待建立定向测试 | 未运行 |
| String/inline MoveOnly/Resource非消费proof与清理 | 待红测后确定 | 未运行 |
| Copyable及0296/0297/0300直接回归 | 按受影响合同选择 | 未运行 |
| 文档/格式/静态检查与独立审查 | 待实际执行 | 未运行 |

不运行fault injection、calibration、sanitizer、reducer、aggregate drivers或畸形IR/runtime注入。
普通stdout不替代完整泄漏或内存安全证明；正式发布、双宿主CI及合并不在本地完成声明内。

## 7. 实施中已确认的独立边界

2026-10-12 普通源码同源红测：single plain 三种表示的 SSA/verifier/LLVM 已成功，
single grouped 和 unit plain/group 合计九例失败；因此没有将全部十二例标为 lowering 红测。
Resource single plain 的 native 另在未改生产代码的真实基线 `08c6b8dc` 重现错误：
两次 `observe(removed)` 读取 String 字段只输出空行，而后续 `removed!!` 的同字段读取输出 `value`。
这是真实 native 红测，不能由先前 SSA 验证成功覆盖。

最小已批准修复限定现有 `NullableHandle -> Shared view` LLVM adapter：
其交付必须与既有 HeapOwner Borrow 参数相同，为**存放 handle 的 storage 地址**，不是裸 heap handle。
使用函数 entry 的 stack slot，每次分支执行写入当前 handle；不改 ABI 定义、Guide、verifier 或 runtime，
不 clone/retain，也不扩大一般 nullable place 支持。需保留非 Map NullableHandle 与 aggregate 分流直接回归。

unit 新 proof 首片进一步限于非嵌套直接表达式语句的同步 `Owned`-result 调用（`Unit` 是普通 owned 结果模式）。
同一调用多个 Borrow operand 及已登记 prefix 仍须通过。Borrow/Carrier 结果与特殊 receiver 的
key/index/element 嵌套调用不在首片内；它们的未登记 Rust 局部 receiver 运输仍是0299后继未完成项，
不能以此位置 gate 宣称原草案广义目标已全部完成。此 gate 只约束新增 unit proof，不改变旧 single 或 Copyable 路径。

inline 核心同源 callee 仅输出标签，验收仅证明真实 Shared call proof 与后续 `!!` 后字段值；
不把标签宣称为借入 payload 读取。独立辅助读取分别核 single 显式 receiver 的 Int 字段、unit 的 `this` 方法；
两入口另用源码显式 `value.text.clone()` 读取 String 字段，区分源码显式 clone 与桥隐式复制。
unit 直接 `value.n` / `value.text` 与 single `value.number()` 的既有 callee lowering 缺口作为后继保留，未认领修复。
