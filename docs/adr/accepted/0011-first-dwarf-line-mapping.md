# ADR-0011: 首个 DWARF 源码行表映射

> **性质**：架构决策记录 · **状态**：accepted · **读取时机**：任务涉及本 ADR 决策边界时 · **唯一真源**：本 ADR

## 状态

accepted

## 接受依据

当前持续 Goal“继续推进 guide 主线，分阶段实施 specs”的站立授权。

## 背景

现行 guide 要求 Phase 4 生成 DWARF，并在首个支持平台上用调试器完成源码断点和单步验收。
ADR-0004 已确定 `Span` 只保存 `SourceId` 与 UTF-8 字节范围，行列必须通过所属
`SourceMap` 在适配边界计算；ADR-0007/0010 已确定 LLVM 21、AArch64 macOS、Mach-O object
与系统 Clang 链接链路。

当前 typed SSA 的函数、block、instruction 与 terminator 都保存 `Origin`，其中 source origin
直接携带 `Span`，synthetic origin 携带 source anchor。但 `Program` 不拥有 `SourceMap`，现有
object emission 也不接收它；只读取 `Span` 无法验证 source identity、取得文件名或计算行列。
同时 Koven 尚无已登记的标准 DWARF language code，完整变量、类型、表达式求值和优化后位置
跟踪也没有对应规范。SPEC-0040 因此需要一个不污染 SSA、又能真实调试首个未优化 object 的
最小映射契约。

## 决策

### 输入与所有权边界

- debug-enabled object emission 显式接收生成这些 SSA origin 的同一个 `SourceMap`；`Program`
  继续保持 target/debug-format independent，不持有源码文本、路径或 LLVM metadata。
- 在创建任何 debug metadata 或写 object 前，codegen 必须验证所有函数、block、instruction、
  entity 与 terminator origin 都能由该 `SourceMap` 解析。foreign source、无效位置及行列超过
  DWARF/LLVM `u32` 范围是结构化 codegen 错误，不伪装成用户类型诊断，也不产生 object。
- 无 debug 的 LLVM 文本入口保持现有行为与快照；首个 debug object 入口不引入通用优化配置、
  路径重映射框架或新的 workspace 依赖。

### 首版 metadata 范围

- 首版使用 Inkwell 0.10 的 `DebugInfoBuilder` 生成 `DWARFEmissionKind::LineTablesOnly`，设置 LLVM
  要求的 debug metadata version module flag，producer 记录为 `kovenc`，`is_optimized=false`。
- LLVM 21/Inkwell 没有 Koven 的标准 language 枚举。首版使用 `DW_LANG_C` 作为仅供通用调试器
  读取行表的互操作 fallback；它不表示 Koven 与 C 源码、类型、ABI 或表达式语义兼容。未来
  获得正式 language code 或采用可维护的 user-language code 时必须由后续 ADR 取代。
- 首版只承诺 compile unit、file、Koven function subprogram 与 instruction line location；不生成
  局部变量、参数值、完整类型图、lexical variable scope、内联调用栈、宏展开位置或调试器表达式
  求值契约。函数 subroutine metadata 使用最小无类型行表形态。

### 文件、函数与位置映射

- native entry 的函数 origin 所属 source 是 compile unit 主文件。其余被 SSA origin 引用的
  source 各建立一个 `DIFile`；除 API 强制先建立的主文件外，其余 metadata 按
  `SourceMap::source_name` 排序创建，禁止依赖 `SourceId` 分配值或 source 加载顺序。
- `SourceMap` 的用户可见 source name 原样写入 `DIFile.filename`，directory 保持空；codegen
  不读取当前目录、不把 source name 解释或规范化为文件系统路径，也不嵌入源码文本。调用方
  可以显式提供绝对或逻辑名称，调试器的 source-map 配置由后续 CLI/工具 Spec 负责。
- Koven function 的 `DISubprogram` 使用源码函数名作为 display name、现有确定性 LLVM symbol
  作为 linkage name，并以函数 origin 起点作为声明行。普通 Koven function 仍是 internal；
  debug metadata 不改变符号 linkage 或入口 ABI。
- block parameter/PHI 使用 block origin；每条 LLVM instruction 使用对应 SSA instruction origin；
  terminator 使用 terminator origin。位置取 `Origin::span().start()`，并通过 ADR-0004 的统一
  1-based Unicode-scalar 行列换算。synthetic origin 使用其 anchor，不创建虚构文件或行号。
- C `main` wrapper、runtime declaration、drop helper、allocation/check helper 等没有独立 Koven
  source origin 的生成代码不获得伪造的 source subprogram/location；builder 跨这些边界必须清除
  先前 location，避免把最后一条用户位置泄漏给 synthetic instruction。

### 验证与调试器验收

- `DebugInfoBuilder` 必须在 LLVM verifier 与 object emission 前 finalize；带 debug metadata 的
  module 仍须通过现有 LLVM verifier，并由 ADR-0010 的同一 TargetMachine 生成 object。
- 自动测试锁定 debug metadata 存在、文件/行/列、display/linkage name、synthetic anchor、
  foreign `SourceMap` fail-loud，以及重复输入的 debug LLVM 文本确定性。
- AArch64 macOS 验收使用本机 `/usr/bin/lldb --batch` 加载 SPEC-0039 链接的真实可执行文件，
  按 `.ko` 文件与行号设置断点并运行；必须观察到断点命中、Koven frame 与预期源码位置。
  LLDB stderr 文案不作为稳定协议，只断言结构化命令结果与必要的稳定片段。

## 替代方案

### 把 `SourceMap` 存入 typed SSA `Program`

拒绝。源码快照属于分析输入所有者，SSA 只需稳定 origin；把文本和 source owner 塞入 IR 会让
target-independent verifier、测试构造器和未来序列化都承担不必要生命周期与格式责任。

### 只用 `Span` 字节偏移直接生成行号

拒绝。`Span` 不包含源码文本或行索引，也无法验证 map-local `SourceId`；自行重建或猜测行列会
违反 ADR-0004，并让 Unicode/CRLF 行为与诊断漂移。

### 首版生成完整变量和类型 DWARF

暂不采用。当前目标是源码断点和单步；完整类型图必须处理泛型实例、value/reference 表示、
closure environment、容器与 optimized location，范围远超 SPEC-0040，且容易把内部 ABI 固化为
长期用户调试协议。

### 使用 `DW_LANG_Kotlin` 或伪造 user language code

拒绝。Koven 只采用 Kotlin 风格语法，不承诺 Kotlin 兼容；伪造未登记 code 还需要绕过 Inkwell
的安全枚举边界。`DW_LANG_C` 仅作为 line-table reader fallback，并由 producer 明确标识 Koven。

### 给生成 wrapper/runtime helper 复用最近的 source location

拒绝。这会让调试器把 ABI glue、OOM/abort 或析构 helper 错报为用户源码语句，破坏单步和
backtrace 的可解释性。

## 后果

收益：

- 保持 SSA 与 LLVM/DWARF 解耦，同时复用唯一 `SourceMap` 行列真源；
- SPEC-0040 可以用现有显式 entry/object/link 链路完成真实源码断点和单步；
- line-table 范围小、可验证，不提前冻结变量、类型或表达式调试 ABI；
- 多 source 与 synthetic origin 的确定性和错误边界明确。

代价与风险：

- debug object 调用方必须持续持有并显式传入原 `SourceMap`；
- source name 不做路径重映射，移动构建目录后可能需要调试器 source-map 配置；
- `DW_LANG_C` 只保证通用行表互操作，调试器不会理解 Koven 表达式；
- 首版不能查看 Koven 局部变量或完整类型，后续扩展需新的 Spec，language code 改变需新 ADR。

## 关联

- 相关 Spec：SPEC-0040
- 相关 ADR：ADR-0004、ADR-0007、ADR-0010
- 取代的 ADR：无
- 被以下 ADR 取代：无
