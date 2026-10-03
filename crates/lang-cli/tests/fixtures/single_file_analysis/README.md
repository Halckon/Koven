# SPEC-0253 迁移前单文件 CLI oracle

这些 `.ko` 与完整 `.human.stderr` / `.json.stderr` golden 在生产迁移前捕获，
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
