# SPEC-0270: 单文件局部 MoveOnly 绑定交接

> **性质**：修复合同 · **状态**：done · **读取时机**：修复或验收局部 owner 移动后控制流时 · **唯一真源**：本页

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P4-270` |
| 所属 Phase | Phase 4；native 验证覆盖 Phase 5 |
| 语言规范 | [Guide v0.40 所有权](../../guide/10-ownership-borrowing-drop.md) |
| 批准依据 | 用户持续实施里程碑及修复真实验证发现的授权 |
| 前置 Spec | SPEC-0245 |
| 前置 ADR | 无 |
| 影响范围 | single-file lowering 局部绑定；定向 native 回归 |
| 语言语义变更 | 否 |

## 1. Goal 与基线

资源局部变量移动到新绑定后，条件 return/后继控制流只运输当前仍拥有该值的绑定，
合法源码通过 SSA 验证并在 native 按既有前端 cleanup facts 恰好析构一次。
分支 `fix/spec-0270`，main 基线 `7c093bb6fbb8e4c0b590b18c7b167d2f094b4c1a`。
M4b 的未提交生成器发现 V2 local→local_moved 后条件 return 触发 InvalidSsa；
CLI 单文件 build 同样失败。0269 保留该样本，本修复不吸收其未完成验证设施。

## 2. 原因与边界

`lower_local_variable` 建立新 symbol 前只注销 temporary，旧 named binding 仍指向同一 ValueId。
控制流按 binding 运输 MoveOnly owner，导致同一 owner 进入同一 edge 两次；
SSA verifier 拒绝。修复根据 frontend 已解析 copyability，复用既有交付 helper 注销来源 owner，
不从 AST 猜析构，不对 CFG 单独去重掩盖旧绑定，不改变 Copyable 别名行为。
Nullable wrapping 与 group initializer 保留当前语义；不重构 unit lowering 或扩展其支持范围。

## 3. 验收账本

| ID | 要求 | 实际结果 |
|---|---|---|
| R1 | 新 native 回归先复现 InvalidSsa；MoveOnly 移动链、group 与条件 true/false 实际执行 | resource_deinit_红测28 passed/4 InvalidSsa失败；修复后32 passed，含4新增native与1新增SSA |
| R2 | 精确 stdout 与逐指针唯一释放；旧绑定不重复运输，嵌套scope后继续CFG无悬空owner | 两条移动链×true/false精确stdout及构造ID释放序通过；scope后CFG通过；原生成失败2例经新CLI实际build/run匹配原oracle |
| R3 | Copyable alias 旧/新变量均可读；nullable wrap 和已存在 local 相关回归 | Copyable标量/泛型原值仍可读、纯class nullable包装计数通过；lower_frontend_tests 66 passed |
| R4 | resource_deinit、field_replace 与相关 SSA/lowering 定向套件；fmt/Clippy/尺寸/docs | resource_deinit_32、lower_frontend_tests66、field_replace26均0 failed/ignored；Clippy/fmt/尺寸/docs511通过，Python docs/尺寸84通过 |
| R5 | 独立审阅，Architecture 当前事实、Spec证据/归档、最终PR CI通过并合并 | 独立完整审阅无阻塞问题，确认精确5行例外；resource-deinit事实同步；实现head双宿主PR CI通过，据此归档；最终归档head仍须CI通过后合并 |

## 4. 执行与验证

先失败回归，再单点实现；只使用同一个串行 Cargo target。修改共享 owner helper 调用点后
检查对应赋值/调用/聚合路径，选择定向证据，不默认 frontend 全量。
Rust 超限既有文件如增长须有精确例外，避免为本修复重构不相关代码。
当前只有原始生成样例和单文件 CLI 失败证据，根因先由静态数据流定位；
不能将其写成已通过回归或已经验证更深 verifier 诊断。实际命令与结果随后追加。

## 5. 本机验证记录

macOS arm64、Rust1.96.0、LLVM21.1.8，共享目标目录，以下Cargo均带`--locked --offline`：

- `cargo test -p lang-codegen resource_deinit_ -- --nocapture`：修复前4条InvalidSsa失败；修复后32 passed/793 filtered/0 ignored。
- `cargo test -p lang-codegen ssa::lower_frontend_tests:: -- --nocapture`：66 passed/759 filtered/0 ignored。
- `cargo test -p lang-codegen field_replace -- --nocapture`：26 passed/799 filtered/0 ignored。
- 上述命令另启动integration binary，实际0 matched/2 filtered，不算新增覆盖；套件之间有重叠，不累加为互斥总量。
- `cargo clippy -p lang-codegen --all-targets --no-deps -- -D warnings`、`cargo fmt --all -- --check`通过。
- `cargo build -p lang-cli`通过；原2个生成失败case仅将entry函数名改main，经CLI真实build/native，exit0、完整stdout匹配原oracle、stderr为空。证据`/private/tmp/0270-cli-fixed`；这是未提交开发树，不冒充clean SHA验收。
- `python3 -m unittest scripts.tests.test_check_docs scripts.tests.test_check_rust_sizes`：84 passed；docs511、git diff检查通过。
- `python3 scripts/check_rust_sizes.py --base 7c093bb6fbb8e4c0b590b18c7b167d2f094b4c1a`：772手写Rust、45旧超限、0生成，有限例外后通过；baseline未提升。

最初测试夹具存在Int直接println的L0084和不支持的nullable类型登记，已改成合法已支持形式；
这些夹具失败不计入InvalidSsa红测。未运行本地全量frontend/codegen或workspace check，
后续远端required CI单独记录。独立审阅核源码事实、借用/Copyable/替换前交付与计数器oracle，
未重复执行Cargo；资源nullable、复合泛型和unit lowering不因此取得新支持结论。

## 6. 双宿主实现验收与归档

[PR47 CI37208929731](https://github.com/Halckon/Koven/actions/runs/37208929731)关联实现head
`c68f8c36ef6e599120b64564326750069fe1e652`，required jobs全部success，无未决；
唯一job-level skip为editors=false的Tree-sitter corpus，汇总确认合法。
Linux codegen 825 passed/0 ignored，Mac824 passed/1 ignored；忽略项是既有
`llvm::debug_tests::lldb_hits_a_koven_source_breakpoint_and_reports_the_frame`的debugserver权限限制。
新增4条native与`single_resource_deinit_local_move_chain_transports_only_current_owner`
在两宿主均实际执行且ok。其余required组合、教程及Linux既有检测步骤成功，不能概括frontend全量。

两宿主实际checkout synthetic merge `1c5bd828a20e2aaf6730fb21c8368c5a02b25d44`，
GitHub commit API核对该merge和实现head的tree均为`d2c692e7462400beb0b591ef573f360dd15ef9cd`。
按此实现证据归档；本次归档提交仍需独立最终PR CI通过才合并，交付状态见
[PR47](https://github.com/Halckon/Koven/pull/47)，本页不提前宣称归档head或主干CI已运行。
