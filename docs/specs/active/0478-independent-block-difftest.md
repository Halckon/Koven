# SPEC-0478：普通块表达式离线差分

> **性质**：交付合同 · **状态**：in-progress · **读取时机**：实施或验收 F4a 普通块表达式差分时 · **唯一真源**：本页验收账本

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| 所属 Phase | Phase 1 测试 |
| 语言规范 | [Guide v0.47](../../guide/README.md) |
| 基线 | `main@6cc03907` |
| 分支 | `feature/spec-0478` |

不依赖 SPEC-0477，不修改生产 Parser、Lexer、公开 API 或 Guide；复用现有双宿主 Targeted Tests，不新增 job。
不覆盖 D1–D3、诊断恢复、完整表达式或独立 Lexer。与生产侧共享 Lexer，不共享续行和 AST 逻辑。

## 交付合同

1. 以显式离线命令运行测试。使用 Python 3.12 或更新版本和预先取得的 PyPI `lark-1.2.2-py3-none-any.whl`；
   加载前校验 SHA-256 `c2276486b02f0f1b90be155f2c8ba4a8e194d42775786db622faccd652d8e80c`。
   [PyPI](https://pypi.org/project/lark/1.2.2/) wheel 为纯 Python、MIT、无必需传递运行依赖；不安装、不联网、
   不随仓库分发。固定文法和白名单输入，拒绝任意用户文法。接入前审查 OSV 与上游公告均无命中（2026-11-18），
   不视作安全证明。参考批次有 30 秒截止时间，每个输入最多 64 个事件（含 EOF）。
2. 首个检查点：`{a\n-b}` 是两个表达式元素，`{a/*雪\r\n*/.b}` 是一个成员访问，且 CRLF 事件 `[7,9)`
   隶属 `[2,11)` 的原始块注释。漏发前者换行须被结构比较检出；截短后者须被流校验检出，不能拿 AST 代替。
3. 完整首片：16 个固定输入（11 个可接受、2 个语法拒绝、2 个词法拒收、1 个范围外）；四个三槽模板枚举
   `a/b` 共 32 个；位置平移不改变归一结构。保留元素顺序、名称、操作符与子边，独立检查 Span、owner 和 EOF。
   原型的 4 个流故障、3 个结构故障以及注入歧义均须检出；内部错误、超限、超时不算语法拒绝。
4. 本地只运行固定和生成语料。故障注入、超时与事件上限负对照只由现有双宿主 Targeted Tests 显式运行。
   CI 先下载固定 wheel，再启动测试；参考解析不联网且导入前仍验哈希。按仓库规则完成 PR 门禁与归档。

## 本地运行

在仓库根目录执行；将路径替换为预先取得的固定 wheel。默认使用 `python3`，也可通过
`KOVEN_OFFLINE_PYTHON` 指向受支持的解释器。显式执行时缺 wheel 或哈希不匹配会失败，不会跳过。

```bash
KOVEN_LARK_WHEEL=/absolute/path/lark-1.2.2-py3-none-any.whl \
  cargo test --locked -p lang-frontend --test parser_offline_block_diff fixed_and_generated_corpus -- --ignored --exact
```

macOS AArch64 / Python 3.15.2 / Rust 1.96.0：先省略 `NL` 投影，测试在 `{a\n-b}` 的事件合同处失败；
补投影后通过两个输入和两个故障对照。审阅后将批次截止时间移至 Rust 父进程，文件承载 stdin/stdout，
固定系统 shell 在受监督的子进程内解析并启动解释器；无输出等待负对照验证超时回收及实际耗时上界。
合同假定本机 `/bin/sh` 可启动，不承诺中断内核阻塞。解释器禁用系统 site，只加载验哈希后的私有 wheel 副本。
批次目录在校验后的系统 `/tmp` 中创建，不使用 `TMPDIR`；设为可共享写入的 `TMPDIR` 的对照通过。直调参考入口确认 64 个事件可接受、65 个拒绝、错误哈希和未禁用 site 均拒绝。
现有 `parser_expression` 与 `parser_block_line_continuation` 共 85 passed；目标 Clippy、格式检查通过。
完整 16 个固定输入（11 接受、2 语法拒绝、2 词法拒收、1 范围外）、32 个唯一模板实例和位置平移对照
已实跑。分流后本地再次精确选择 `fixed_and_generated_corpus`：1 passed，4 filtered out；
目标 Clippy 通过。四个 `ci_fault_` 测试已编译并列入 CI 选择，覆盖换行流、结构差分、歧义、超时和事件上限。
歧义和越界须以不同的非零状态退出，不得输出语法拒绝或成功结构。本轮未在本地执行故障注入，双宿主 CI 尚未触发。

## 检查点

- [x] 本地最小端到端及故障对照
- [ ] 完整首片语料与资源合同
- [ ] Linux 完整生产／参考差分、必需 CI 与归档
