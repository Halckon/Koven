# 单文件 CLI 完整进程 oracle

## SPEC-0253 原始捕获

原 `.ko` 与初始完整 `.human.stderr` / `.json.stderr` golden 由迁移前冻结的旧生产二进制捕获，
来源为 `feature/spec-0253` 的归档基线提交 `e6dfea4e31f9d9ad1907ff646f7f4910f480ce61`
（父 main `82610ab`），不是迁移后的 façade。

- 工具链：Rust/Cargo 1.96.0，`cargo build -p lang-cli --locked --offline`
- 已复制旧生产 `kovenc`，SHA256：
  `a31b2a381c73a33d5ccb8e6c0a7098193b703941b048cc5dc9e0b1002c60fd6f`
- 在独立目录用相对输入名 `source.ko`，避免机器路径进入 golden
- 捕获命令：`kovenc --message-format=<human|json> <build|run> source.ko`
  加 `--entry <main|invalid|absent>` 或不加 entry；build 再加 `-o out`
- 五个 stage fixture 包含下游错误；旧进程只输出最早阶段的全部诊断
- 128 个失败 stage/entry/format/operation 组合逐字节一致，退出码均为 2，stdout 为空
- `success.ko` 同时捕获常量字符串 native 成功、missing entry、invalid codegen entry
- Unicode/CRLF 按原始字节保存；测试不得把 CRLF 归一化

测试只读取捕获结果，不提供从新生产重录 golden 的自动更新开关。

`.gitattributes`仅对`success.ko`与`unicode.ko`保留原始CRLF字节（`-text`），并把CR视为
行尾；普通行尾空白、文件末尾空行、tab前space检查继续启用，不扩大其他文件的例外。

## SPEC-0341 D10 A2 compilation-unit 迁移

原九份 `.ko`（包括 Unicode/CRLF）全部原字节保留，不删除后续阶段错误。
候选生产 source `2bac18bf92fa62e64d7556e5add9886081f60475`，由共享 target 实际重编并核
四个 crate 的 `fresh:false` / rustc 来源；CLI SHA256 为
`d80c1e7c376b6b65e8063d4c05a66a6b1e74846fe54f84aad0308d26d64e9240`。

在相对路径 `source.ko` 上，八份失败原源逐份执行 `kovenc --message-format=<human|json>
build source.ko -o program`，完整 stdout 为空、退出 2、无产物。仅以下差异逐项核对后迁移：

- lexer：保留两个 L0001，名称前缀另保留 parser 的 L0018 与 names 的 L0080。
- parser：保留两个 L0018，名称前缀另保留 L0080。
- ownership / const_move：保留每个 L0131 的 primary 与 moved-here label，补齐选定 `own`
  参数声明 label。Unit `contracts::source_parameter_span` 与 `dataflow::ensure_available`
  提供该同源位置，不从输出倒推新语义。
- names / typed / const_typed / unicode：双 renderer 全文与原 golden 逐字节一致，未改文件。
- 既有 128 个失败 stage/entry/format/operation 硬断言组合全部保留，未以此 build-only
  观察代替完整回归运行；success、absent、invalid 原 oracle 亦未修改。

新增 `bodyless.ko` 精确保留已观察的 `fun main(): Unit\n` 17 字节。单文件默认/显式 main
在旧、新 CLI 上均为完整 UnsupportedSource / UnsupportedNode 操作错误（退出 2）；project
按其入口 body 过滤合同给完整 unsupported process shape 操作错误（退出 1）。两种 renderer
都仍使用对应完整人类操作错误文本。硬测试保两个宿主的区别，不接受任意错误或只核两边拒绝。
bodyless 硬测试仅 build 六次并核完整输出及无产物，不执行产物。

观察来源：独立 build-only 原始收据的 SHA 清单与逐项 producer 审查保存在本次外部验收记录。
观察不是测试通过证明；无自动重录开关，后续变更仍需保原字节、说明 producer 与完整断言。
