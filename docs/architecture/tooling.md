# CLI、LSP、Formatter 与编辑器 Grammar

> **性质**：当前实现事实 · **状态**：current · **读取时机**：修改 CLI、project、LSP、formatter 或编辑器 grammar 时 · **唯一真源**：对应代码、协议测试和 grammar fixture

## `kovenc` CLI

`lang-cli` 负责 frontend/codegen 编排、诊断输出、链接和进程退出码，不承载语言分析算法。

当前公开路径包括：

- 单文件 `build` / `run`，接收显式 source、entry 和输出选择；
- `format` / `format --check`，结果写 stdout，不原地改文件；
- `--message-format=json` 的版本化 JSON Lines 语言诊断；
- 显式 `project.toml` 的本地 project build/run。

project loader 严格解析 version 1 manifest，验证 source root、logical path、symlink/overlap 和 physical
file identity，再发布不可变、稳定排序的 source-set snapshot。build 注册原 SourceMap 后将其移动进
frontend `analyze_unit_names`，取得拥有源码/语法/环境/名称事实的只读 `UnitNameSnapshot`；
首个非空诊断 gate 仍在完整 names 前缀后。typed 非空 gate 留宿主，随后调用
frontend `analyze_basic_unit_ownership` 共享 basic validation→普通 ownership 推进；宿主继续
验证 raw ownership，成功后进入单 object、link 和原子 executable 发布。CLI 不从 cwd 或
祖先目录猜 manifest。typed diagnostics 统一先于 entry 选择；基础 capability 验证成功时沿用
基础 ownership/native，含常量时由 frontend 专用 gate 发布 typed/owned capability，再调用
`emit_native_constant_unit_object`。同一只读 entry shape helper 服务两条已验证路径。

单文件 bootstrap 使用 frontend `analyze_single_file` 固定推进五阶段，宿主在每个阶段的
原始 diagnostics gate 上拒绝任意非空集合；不会先跑完后继再挑首错。纯门面接收原
SourceMap/SourceId与一次创建的name/type环境，返回同轮raw产物，不负责IO、诊断渲染、
entry或native能力。`SingleFileTypedView`字段封闭，只借出本轮parsed/names/typed；
observer的临时借用不能作为返回值逃逸，`SingleFileAnalysis<T>`消费式拆出原raw产物。
CLI typed observer为no-op，内部错误逐项映射回原BootstrapError。

CLI 生产链接按宿主使用 macOS `/usr/bin/clang` 或 Linux `/usr/bin/cc`，通过 `Command`
参数数组链接已有 native object，不调用 shell 或让外部 driver 重新编译 LLVM IR。
链接启动失败与进程失败保留现有结构化错误；受支持目标在 codegen 边界先行校验。

实现入口是 `crates/lang-cli/src/main.rs`、`native_command.rs`、`project/`、`project_build.rs` 和
`project_command.rs`；对应覆盖位于 `native_cli`、`project_cli` 与 `format_cli` integration suites。

## LSP

`lang-lsp` 是标准 stdio server，声明 UTF-16 position encoding、full-document sync、诊断和
`textDocument/definition`。

它有两种明确模式：

- legacy 单文档：每个打开 URI 独立调用同一 `analyze_single_file`，五阶段gate不拒绝用户
  诊断；typed observer在ownership之前从封闭只读view构建DefinitionIndex，随后按原
  parsed/names/typed/owned聚合诊断（parser已含lexer，不重复添加）；
- `koven.sourceSet` version 1：初始化 payload 提供 immutable base sources，open/change 形成 overlay，
  每次候选更新重建共同 compilation-unit snapshot，其名称前缀由 frontend `UnitNameSnapshot` 拥有。

source-set 模式不读取磁盘。`unit_session` 组合唯一名称 owner，后续阶段借其 sources/inputs 与
validated_names；同次 standard_environments 的 type 半边留宿主。names recovery 保留有效导航，
typed diagnostics 先由宿主收集，再调用同一 `analyze_basic_unit_ownership`；成功以
`into_types()` 保存原 typed 而不克隆，NotBasic 取回原 boxed recovery。const（包括仅声明未读）
不进入基础 ownership；raw ownership 即使有诊断或 deferred 仍保留。typed recovery
继续向 definition 提供有效事实。
完整 snapshot 与全部诊断 payload 准备成功、逐项发布成功后才替换内存状态；分析、映射或发送
失败保留 last-good。已发消息不能撤回，此边界不是 wire 事务。诊断按 target URI 分组并稳定
发布，definition 直接消费已保存的 name/type target，不重新解析 package/import 或 overload。

`position_adapter` 集中处理 Span 与 UTF-16 的双向映射，拒绝 surrogate pair 中间位置和越界 cursor。
实现入口位于 `crates/lang-lsp/src/analysis.rs`、`source_set.rs`、`unit_session.rs`、`definition.rs` 和
`diagnostic_adapter.rs`；对应覆盖位于 `lang-lsp` 的模块与 integration tests。

## Formatter

`lang_frontend::formatting::format_source` 先运行生产 Lexer 和 file Parser；任一语言诊断都会阻止部分
格式化结果。成功路径复用 lexeme slice，规范 horizontal whitespace 和四空格缩进，同时保持 comment、
string segment 与 LF/CRLF 字节。它不排序声明、不折行、不合并空行。

实现入口是 `crates/lang-frontend/src/formatting.rs`；对应覆盖位于 frontend 的 `formatting` 和 CLI
的 `format_cli` integration suites。

## TextMate

`editors/textmate/syntaxes/koven.tmLanguage.json` 提供 `.ko` 的词法高亮近似；
`editors/textmate/tests/lexical-contract.tsv`、`highlight.ko`、`reserved.ko` 和 `scopes.tsv` 锁定代表性
scope。Node verifier 执行 grammar regex，frontend integration test 用同一数据对照生产 Lexer。
对应覆盖位于 TextMate 的 Node tests 与 frontend 的 `textmate_grammar` integration suite。

## Tree-sitter

`editors/tree-sitter/grammar.js` 是手写 grammar 入口；`src/grammar.json`、`node-types.json` 和
`parser.c` 是锁定 CLI 生成并提交的产物。`src/scanner.c` 集中拒绝硬关键字、未来保留字和解构 `_`。
scanner 通过明确的 external token 与语境 lookahead 识别参数模式、loop、move、to/by；
in/is/as 与 identifier 使用同一整词扫描，避免 input 等名称在运算符位置被拆开；
!in/!is 同时检查字符邻接和整词边界。
命名参数前缀优先于未分组赋值。真实 CLI corpus 与 Python XML 树回归覆盖这些节点、
词区间和非法输入恢复；Tree-sitter 用于编辑器 concrete syntax，生产编译仍只使用 Rust Lexer/Parser。
对应覆盖位于 Tree-sitter 的 Node/corpus tests 与 frontend 的 `tree_sitter_grammar` integration suite。

上述测试的命令与范围选择统一见[开发测试指南](../development/testing.md)。

## CI 工程门禁

当前 workflow 定义 macOS 14 / Ubuntu 24.04 双宿主 check、严格 clippy和组合测试，单次fmt。
组合入口依次执行core（含lang-std）、ownership_iteration、stage、剩余guide_litmus及教程；
保留独立入口，没有skip开关，frontend integration共77个唯一target，组合共10次Cargo调用。
外部Rust编译合同通过当前integration executable的Cargo 1.96 fingerprint选择实际依赖rlib；
同目录旧flags产物可以保留，未知或歧义身份失败，不按时间戳推断。
`editors/**`与教程修改触发Rust路径；内部直接依赖检查为无条件required job。
独立 editor job 对 editor/相关门禁与词法调用规则修改运行锁定的 Tree-sitter 0.26.12，
检查重新生成的产物无漂移并执行完整 corpus 与实际树回归；main/manual 强制运行，
required summary 拒绝该运行时却跳过的结果。
当前教程以Markdown fence为源码真源，15正例、4完整JSON负例与2 planned分开，四组argv使执行合同共22项；
跨文件合同只引用fence ID，沿现行project.toml/entry协议运行，定向选择不改变CI默认完整选集。
新增5项合同在固定 `4ce0eb8` 基线Mac通过真实build/artifact/run或精确JSON验收；该基线分支内多个resource的
`InvalidSsa` build失败单独保留为planned，不能用函数作用域正例替代。后续PR49实现head的双宿主CI实际执行全部22项合同；源码与原始结果
见 [SPEC-0271](../archive/specs/0271-tour-combination-coverage.md)；两个planned不计通过。
LLVM setup action 统一校验所需工具，CI 汇总策略拒绝必需 job
意外跳过。配置与本地验证不代表远端已运行；实际交付证据见
[SPEC-0239](../archive/specs/0239-linux-ci-gates.md)，使用规则见[测试与分层验收](../development/testing.md)。

## Native sanitizer 检测设施

`native_sanitizer_tests` 将合法 Koven Cell fixture 经现有 frontend/verified SSA/LLVM 管线
生成 IR；私有 Inkwell helper 对全部有定义函数添加 ASan 属性，并分别在实际字段读取、
分配初始化和 Cell drop glue 注入地址错误。对应 Clang 后置 IR 必须在指定函数出现检查；
去掉属性但保留 ASan flags 的对照必须无该检查。原有 allocator counter 独立验证一个 owner
恰好释放一次，避免它的全局 live pointer 干扰泄漏检查。

`scripts/check_native_sanitizers.py` 的 Linux x86_64 入口运行 ASan 正反与关闭对照，
另以 `-fsanitize=leak` 验证正常/删除 free/关闭检测；固定 LLVM21.1.8 和 compiler-rt。
LSan两组通过测试专用C入口和Linux linker `--wrap=main` 在worker执行原Koven main，
精确匹配fixture无参数main ABI，join后返回原退出码，让包含Cell指针的线程栈先退出根集合；原LLVM不改，
默认stack/register/TLS扫描保留。线程创建/join失败直接成为普通执行错误，不触发退出时泄漏检测。
Linux driver 对导出 Cargo 整体限时，并对 sanitizer 命令逐项限时；超时终止对应子进程组。
普通 codegen suite 的 counter 调用没有独立期限。源文件、IR、精确命令、版本和结果文件保留到
CI artifact。该必需步骤属于现有 Linux test job，失败传递到 required summary。
本机已验证资源计数、三层 IR 检查与属性关闭红测；Linux CI实际通过8组ASan与4组LSan动态对照。
LSan曾因残留主线程栈指针视为可达而漏报；worker入口已验证故意泄漏报告4字节/1对象，
正常与关闭检测对照无报告。macOS仅有普通IR/counter测试证据，动态ASan/LSan未验收。
UBSan 对 Koven IR 未覆盖；不声明栈 lifetime、容器逻辑长度、并发或完整内存安全证明。
