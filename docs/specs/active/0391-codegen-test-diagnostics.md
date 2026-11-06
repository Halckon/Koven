# SPEC-0391: Codegen 测试诊断落盘与有界失败摘要

> **性质**：测试基础设施修复合同 · **状态**：in-progress · **读取时机**：维护 codegen 测试输出时 · **唯一真源**：本 Spec

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-TEST-0391` |
| 所属 Phase | 测试基础设施；不改变语言 Phase |
| 语言规范 | [Guide v0.47](../../guide/README.md) |
| 语言语义变更 | 否 |

## 范围与成功标准

原全量测试在 libtest 结果输出阶段阻塞终端写入。修复普通 SSA 测试族的大对象打印，
包括 Range、while、Map provider/consume、nullable/Rc、整数转换与共享 facts/query；
native 的大源码诊断及 LLVM/native 大 payload 断言使用同一辅助实现。
不调整测试输入、覆盖、原断言谓词、native 执行策略或 CI 选择，不改生产诊断与编译语义。

每个测试线程首次输出时以 create_new 创建独立临时文件，原始记录通过 BufWriter 写入后
逐条 flush；libtest 只收到一次文件路径。完整源文本、SSA、LLVM、阶段报告和失败原值保留。
失败摘要最多 240 字符；文本相等断言仍逐字节比较，失败记录左右全文、首个不同字节的
位置以及有界 Unicode 安全上下文，保留尾换行/CRLF/EOF 的区分。泛型 equality 保持标准
assert_eq 的 eq 语义、操作数单次求值和成功时详情懒求值；调用位置保留在测试调用方。
文件创建或写入失败必须使测试失败，不能丢证据后报绿。

文件自动保留，不新增清理机制；系统临时目录不保证长期存储。专用 PR120 stdout 导出协议、
native/sanitizer 产物路径与简短状态提示保持。目标 SPEC-0389 正在退役的 unit_lower 私有
子树不纳入本轮批量替换；不声称所有 panic/unwrap 或专用导出协议均已改成有界输出。

## 验收账本

| 验收项 | 证据与状态 |
|---|---|
| 长 Unicode/多行失败有界且全文保留 | 首轮 helper 红绿验证；新增全文比较先 2 failed / 7 passed，再修复 |
| equality 与普通 assert_eq 同义 | 自定义 ne/eq 不一致的红测先失败，修复后通过；成功详情不求值 |
| 并行隔离、追加、写失败、字节相等/EOF/Unicode/调用位置 | 12 个 helper tests 全通过；独立 rustc 子进程确认调用位置 |
| 测试保全与扩大验证 | 检查到的 1171 个既有测试声明不变；独立审阅确认谓词和输入保全；201 passed、0 failed、0 ignored、2434 filtered |
| 独立审阅 | 完整审阅及修正复审通过；补齐遗漏 IR 输出、移除双重失败包装、修正 equality 语义 |
| 格式、Clippy、尺寸、文档 | fmt、Clippy all-targets -D warnings、尺寸、文档、diff 检查通过；尺寸策略47项测试通过 |
| 全量/native/CI-only/Linux | 未运行全量及 Linux；CI-only 用例不在本地定向选择中，不删除/ignore/修改原门禁 |
| SPEC-0389 集成验收 | 已合入 cb51678d；相同201项测试及 Clippy/fmt/docs/尺寸检查通过，CI待验收 |

## 本机验证输入与方法

2026-11-06，macOS AArch64，Rust 1.96.0、LLVM 21.1.8，基底 `c80e9e38` 加本分支变更。
`CARGO_TARGET_DIR` 显式设为当前工作树 `target`，编译 jobs=2、test-threads=2；开发期间不复用目标工作树 target。

- `cargo test --locked --offline -p lang-codegen --lib --no-run` 编译完整单元测试目标；
  复用产物以 `--exact --test-threads=2 --show-output` 执行201个具名测试。
- 选择 diagnostics 全部12项、Range/while、整数转换、Rc nullable 两族、普通 Map provider
  与 exits（不选择 `ci_contracts`）、Map consume 和 nullable fixture，以及本次文本比较所在
  的直接非 native 测试；nullable consume 只选根模块三项，不选 resource/runtime whole。
  这是本机定向验收，未改变源码中的测试注册或 CI 选择。
- 实跑201项全部通过，测试耗时28.34秒；终端82,819字节、866行、最长行362字符；
  145份文件保留139,513,871字节完整诊断。此为本次定向选择，不是原历史全量运行的耗时对比。
- 前一批55项通过的同次 `--show-output` 输出25,320字节、292行，完整文件31,853,687字节；
  后续扩大输入，不能将两次绝对体积直接解释为回归或节省率。
- `cargo clippy --locked --offline -p lang-codegen --all-targets -- -D warnings`；
  `cargo fmt --all -- --check`；`python3 scripts/check_rust_sizes.py --base HEAD`；
  `python3 scripts/check_docs.py`；`git diff --check`。
- `while_flow_tests/proof_cases.rs` 中本次涉及的负诊断 oracle 提取为 `rejection_diagnostics.rs`，
  保留原错误码、数量、Span 与标签断言。三个既有超限测试文件仅增加一行宏 import，分别登记
  aggregate_operation_tests=1036、container_operation_tests=1032、verify_scalar_tests=1393 的精确
  例外；独立审阅已核对。不为一行 import 移动无关测试，不增加预留额度；全仓39个历史超限文件仍报告。

用户已授权提交修复、合入 SPEC-0389 工作树并推送对应远端分支；不授权合入 main。
本 Spec 保持 active，远端 CI 与主干交付尚未验收。

## 目标工作树集成验证

用户授权后，修复提交 `d70a27fc` cherry-pick 到 `wt-spec-0389`，生成 `cb51678d`，
父提交为 `390fc42a`。两个交叠测试文件仅替换诊断宏，目标 lowering 调用与断言输入保持。
文档冲突按目标原26项 active记录追加0391，再生成 roadmap/依赖图；独立审阅确认原记录全保留。

目标工作树重新编译完整 lib test 目标，复跑与开发分支逐项相同的201个具名测试：
201 passed、0 failed、0 ignored、2446 filtered，26.58秒。输出82,819字节、866行，
完整诊断139,514,969字节；所有数据来自目标分支本次运行。
Clippy all-targets -D warnings、fmt、845份 Markdown 文档检查、相对父提交的尺寸与 diff 检查通过。
随后仅追加本段验收记录，不改变已验证 Rust 输入。

交付目标为远端 `feature/spec-0389-refine-container-element-closure-range-retirement`。
不合入 main；未运行全量 codegen/native/CI-only 或 Linux，不能将本机定向结果称为全量 CI 通过。
