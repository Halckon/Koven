# SPEC-0230：递归 Box enum 的 native 构造与析构

> **性质**：实施 Spec · **状态**：in-progress · **读取时机**：实现或验证递归 Box enum 存储时 · **唯一真源**：本 Spec

| 字段 | 值 |
|---|---|
| 状态 | `in-progress` |
| Goal ID | `KOV-P4-230` |
| 所属 Phase | Phase 4 |
| 语言规范 | [v0.38 Box enum 与递归布局](../../guide/11-copyability-layout-construction.md#内建-box-身份与实参边界) |
| 批准依据 | 2026-10-01 用户要求分阶段继续演进计划；本项属于已启用批次 1 的 Box enum |
| 前置 Spec | 既有 enum/Box SSA 与 native 基线 |
| 前置 ADR | [ADR-0008](../../adr/accepted/0008-internal-value-and-allocation-abi.md) |
| 阻塞项 | 无语义阻塞；按测试证明真实支持边界 |
| 影响范围 | codegen nominal type mapping、递归 drop glue、定向 SSA/native 测试 |
| 语言语义变更 | 否 |

## 1. Goal

已通过 frontend 类型与所有权检查的非泛型递归 enum，可由 Box 间接保存，在单文件和
compilation-unit 两入口完成构造、参数/返回运输和恰好一次递归析构的真实 native 执行。

## 2. 范围与边界

- 先覆盖具体非泛型 enum、无 payload case、标量 case 与含一个/多个 Box 的递归 case。
- Box 是递归布局的间接边；type mapping 不应因为先遇 enum 或先遇 Box 而得到不同结论。
- 分配/释放计数必须来自实际生成代码执行；仅编译、进程正常退出或空 enum 不足以证明递归 drop。
- 保留 existing validated frontend/SSA gate，不以通用 deferred 或跳过 drop 验证掩盖失败。
- 不增加 Box 解引用/拆箱表面、deinit、generic enum ABI、field move 或新的 borrowed return。
  这些独立边界不因本 Spec 的构造/析构证据变成已支持；演进计划的其余项继续保持未完成。
- nullable / Rc 包裹的递归 payload（例如 `Box<E>?`、`Rc<Box<E>>`）仍有提前定义 owner 的独立限制，本 Spec 不宣称支持；普通非递归包装另有回归测试。

## 3. 实施与验收

1. [x] single/unit 正例与计数测试，先记录原始失败。
2. [x] 最小修复间接递归 mapping / allocation 所需路径；复用既有 recursive drop glue，非法 scalar payload 仍拒绝。
3. [x] native 构造、运输和递归 drop 通过，重复输出确定。
4. [x] 受影响定向测试、fmt、严格 clippy、workspace check 与文档门禁通过。
5. [ ] 发布授权后的远端 CI 与 Spec 归档；CI Rust jobs 当前为 macOS，本机为 Linux。

## 4. 提交计划

`fix(codegen): lower recursive boxed enum storage (SPEC-0230)`，按实际修复和验收结果拆分提交。

## 5. 验证账本

| 验收项 / 命令 | 实际结果 | 未运行原因 |
|---|---|---|
| `cargo test -p lang-codegen --lib boxed_enum`（实施前） | 0 passed / 9 failed / 0 ignored | 构造器拒绝 tagged payload；递归 single mapping 报 UnsupportedNode |
| 同一命令（实施后） | 9 passed / 0 failed / 0 ignored | 1 model/verifier + 8 native，包含每入口两种函数签名顺序 |
| `cargo test -p lang-codegen --lib`（最终代码） | 489 passed / 0 failed / 0 ignored | 含审查后追加的两个 signature-only SSA 回归；共 11 个本 Spec 测试 |
| `cargo fmt --all -- --check` | 通过 | 最终代码 |
| `cargo check --workspace --all-targets` | 通过 | 验证跨 crate 消费者 |
| `cargo clippy -p lang-codegen --all-targets -- -D warnings` | 通过 | 未放宽 lint |
| `python3 scripts/check_docs.py` | 386 Markdown / 0 errors | 未放宽页面预算 |
| `python3 -m unittest discover -s scripts/tests -v` | 21 passed | inventory 修改对应检查器回归 |
| `git diff --check` | 通过 | 无 whitespace 错误 |
| macOS / 远端 CI | 未运行 | 当前仅 Linux 宿主，等待分支发布授权 |

native 证据包含无 payload / scalar enum cases、四层 Box 树、inline enum 根、跨文件返回/own
交付、SSA/LLVM 重复输出与 unit inputs 反序稳定性。计数分别为 2 与 45 次 malloc/free；每次
free 必须匹配当前 live pointer，结束时全部归零。inline root 不产生第四十六次分配。

独立审查确认普通非递归路径无静态回归，并指出 constructor 预登记会遮蔽签名的实际 mapping
起点；追加无 construction 的 `Box<Expr>` / `Expr` 首参数两顺序测试，直接验证两种入口。
另以无 construction 的 `Box<Wrapped>`、nullable Box、`Rc<Box<Wrapped>>` 和 `Array<Box<Wrapped>>`
验证 pending payload 最终完成定义。这些测试均包含在最终 489 项结果中。

最初的函数顺序测试把函数移到 enum 声明前，暴露单文件前向 enum case 查找 L0080 与 unit
缺失事实；本切片调整为 enum 声明保持在前、只切换 inline/Box 函数签名顺序。前向 enum
case 解析不属于 type mapping 修复，独立缺口保留，不把这一变化描述为已支持任意声明顺序。

## 6. 未决问题

无新增语言语义选择。若已有前端事实不足或 native 范围超出上述边界，记录具体缺口并新建
后继 Spec，不从 AST 或类型名称猜测缺失合同。
