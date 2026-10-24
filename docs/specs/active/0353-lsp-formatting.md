# SPEC-0353: LSP 全文格式化适配

> **性质**：交付合同 · **状态**：in-progress · **读取时机**：实施与验收编辑器格式化前置时 · **唯一真源**：本次交付范围

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | KOV-P6-0353 |
| 所属 Phase | Phase 6 |
| 里程碑 | 编辑器 M2 / SPEC-0010 生产方前置 |
| 语言规范 | 现行 Guide v0.45 |
| 批准依据 | 用户持续授权实施编辑器完整路线图及其主仓库前置；不扩大语言语义 |
| 基线 | main `4ffb6f17`，独立分支 `feature/spec-0353-lsp-formatting` |

## 1. 行为合同

声明标准 `documentFormattingProvider`，处理 `textDocument/formatting`，两种会话模式均
复用 `lang_frontend::formatting::format_source`，不在 LSP 或插件重写格式化算法。
只格式化当前已打开文档；source-set 中未打开的 base、未知 URI 和已关闭文档返回 null，
不读取磁盘。输入来自当前已提交 snapshot 的 SourceMap/SourceId。
成功且内容改变时返回一个覆盖全文的 TextEdit，UTF-16 范围复用 position_adapter；内容
未变时返回空数组。词法/语法诊断返回 null，不提供部分结果；类型/所有权诊断不阻止
语法合法源码格式化。内部错误返回 InternalError，非法请求字段返回 InvalidParams。

格式结果与同份源码的 CLI/frontend 字节一致，沿用四空格缩进、原注释/字符串与 LF/CRLF。
客户端 FormattingOptions 不覆盖语言工具链的固定样式；不新增可配置 formatter、换行
规范化或尾行策略。测试显式验证不同合法 options 不产生另一套结果。

## 2. 同步与版本边界

处理请求时读取已提交文档快照，不修改 overlay、版本或磁盘；应用编辑后的 didChange
继续走既有分析发布流程。source-set 保留严格递增版本及拒绝重复打开的既有保护；
单文档保留已冻结的同版本/旧版本 change 与重复 open 接受行为，不借本片改变 lifecycle。
标准 formatting 请求/结果没有版本字段，服务端串行处理不能保证客户端接收后文本未变。
插件 SPEC-0010 必须单独验证请求期间用户编辑时 SDK 丢弃旧结果；本 Spec 不假称服务端
已经保证客户端原子应用，也不引入私有版本扩展。

## 3. 非目标与复用

不实现 range/on-type formatting、后台工作池、取消调度、语言规则变化、磁盘写入或 IDE
接线。复用现有 lsp-types、serde_json、SourceMap 与位置适配，不新增依赖；限定 crate
内部接口。读取公开入口、直接调用方和既有会话测试后实施。

## 4. 验收账本

| 验收项 | 方法 | 实际结果 |
|---|---|---|
| capability 与两种模式全文结果 | `server::tests::formatting` | 初始 2 项因 MethodNotFound 失败；最终 8 项全部通过 |
| LF/CRLF、Unicode EOF、注释、插值、幂等 | 同组格式化测试，与真实 frontend 比较并核验范围 | 通过，含无隐式应用与多源非首 overlay |
| 语法错误/语义错误、未知/关闭/base 文档 | 同组格式化测试 | 通过，含真实类型错误与 L0131 所有权诊断 |
| 连续编辑、项目旧/重复版本与重复 open、单文档历史行为 | 同组测试及明确普通回归选集 | 通过，另 6 项既有 lifecycle、legacy 与 position_adapter 精确回归通过 |
| 真实 stdio 与 CLI 一致 | `--test formatting_stdio`；同头 CLI/LSP 探针 | stdio 1 项双模式通过；两模式各 6 输入与 CLI 输出/错误边界一致 |
| 工程门禁与尺寸 | fmt all check、LSP all-targets clippy -D warnings、workspace all-targets check、CLI/LSP build、docs、diff、sizes | 全部通过；1923 手写 Rust 文件，38 项既有超限，本次未新增例外 |
| 独立审查、PR CI、归档、实际 main CI | 独立上下文检查实现、测试与进程清理，交付门禁另行记录 | 两轮审查通过；PR CI、归档与实际 main CI 待执行 |

生产方通过不等于三个 IDE 联合验收完成；消费者状态仍在编辑器仓库维护。

## 5. 已发现边界

初始版本保护假设与主干 `legacy_single_file` 已冻结合同冲突，已撤回单文档版本校验变更，
保持历史接受行为；客户端防旧结果覆盖的验收不因此免除。首次本地全量 LSP 运行实际为
49 通过、2 失败，其中包含误执行的 CI-only 单文档断 transport 测试（失败），另有 source-set 发送失败
注入测试通过；两项均不计为合规普通本地
验收。失败来自已撤回的 lifecycle 变更，后续本地仅运行明确普通选集，完整故障覆盖留 CI。

## 6. 输入与验证边界

使用 `CARGO_TARGET_DIR=/private/tmp/koven-spec0352-target` 串行验证，LLVM 21 来自本机
`/opt/homebrew/opt/llvm@21`。真实探针 CLI SHA-256
`eb733958bb53611254f135e6109ad4a09afd6d2764fdfd77fb6023f7e849d411`，LSP
`a567b90b2dd992eb41e51f3535778841a2999cd5c38c286808fd95dbf2eba584`。
临时探针 SHA-256 `75265f7e7bae9d00910350bdc0cb7db4808fe7ac6ad63fb0eff30fa54d167ceb`；
原始脚本/日志在本机 `/private/tmp`，非长期证据，长期回归以入库协议与 stdio 测试为准。
内部错误映射经独立静态审查，本地未新增故障注入测试；完整既有故障覆盖留 CI。
