# ADR-0019：参数化进程入口与 argv owner bridge

## 状态

accepted

## 接受依据

2026-08-26，用户明确启用 guide v0.31 取代 v0.30，并明确接受 ADR-0018、ADR-0019。

## 背景

ADR-0010 与 SPEC-0039 已固定首个进程入口：backend 接收一个已解析的零参数 Koven entry，
LLVM object 额外生成 `i32 @main()`，调用 entry 后返回 0。ADR-0010 明确把参数化入口和环境
参数排除在外。v0.30 §30.1 随后发布了第二种 conventional shape：
`fun main(args: Array<String>): Unit`，要求排除 executable name、保持顺序、拒绝非法 UTF-8，
由 native wrapper 拥有 Array/String 并以 Borrow 调用后析构。

这项能力不能只是放宽 CLI 的 shape 检查。它同时跨越目标 C process ABI、一般 String owner、
顺序容器 header、Borrow 调用 ABI、部分构造失败和 CLI `run` 参数转交。若 wrapper 直接拼接
宿主 pointer 或绕开统一 entry plan，frontend/SSA verifier 无法确认调用签名，Array/String 的
drop 也会脱离既有 ABI。

## 决策

### Verified native entry plan

- object emission 把原先单个 `FunctionId` 扩展为经过独立验证的 native entry plan。plan 只有
  `NoArguments` 与 `BorrowedArguments` 两种封闭 shape，均携带已解析的 Koven function identity；
  后一种还携带唯一的 `Array<String>`、String SSA type identity。
- `NoArguments` 必须继续验证 `() -> Unit`，并保持 ADR-0010 的 `i32 @main()` wrapper，不因本
  ADR 改变既有 object、显式 `--entry` 或零参数 conventional main 行为。
- `BorrowedArguments` 必须验证 entry 恰有一个 function-scoped shared loan 参数，目标精确为
  `Array<String>`，返回为空；Array kind、String identity 和 Borrow mode 都从 typed SSA facts
  取得，LLVM adapter 不按源码名称、参数名或 struct shape 猜测。
- native entry plan 是 platform wrapper 的验证边界，不新增用户可调用的 Koven function、raw
  pointer 类型或公共 FFI ABI。plan verifier 必须在 LLVM function/type 构造和 object 写盘前运行。

### C process wrapper 与 argv 预检

- `BorrowedArguments` 为首个 AArch64 macOS target 生成唯一 external C ABI
  `i32 @main(i32 argc, ptr argv)`；`argv` 按平台 C process ABI 解释为 `char **`，每项为 NUL
  终止的宿主字节串。wrapper 不接收或暴露 `envp`。
- wrapper 只遍历 `argv[1]` 到 `argv[argc - 1]`，因此 Koven Array 不包含 executable name；元素
  顺序保持不变。`argc == 0` 或 `argc == 1` 都形成空参数 Array；负 `argc`、存在参数时的 null
  argv/element，或目标长度/count 无法表示均进入 operational failure，不调用 Koven entry。
- wrapper 先完成无分配的全量预检：取得每个参数的 NUL 前字节长度，使用 compiler-owned、
  locale-independent 的 UTF-8 validator 检查合法性，并完成 Array buffer 和每个 String 长度的
  target-width checked arithmetic。任一失败都在创建第一个 Koven owner 前返回非零状态。
- 首个实现的 operational failure 返回内部确定常量 `1`，不生成 Koven `Ldddd` 诊断、不调用
  `error()`、不使用 replacement character，也不承诺 stderr 文本。非零是 v0.30 的可观察边界；
  具体数值不是源语言或公开机器诊断协议。

### Owner 构造、调用与清理

- 预检全部成功后，wrapper 使用 ADR-0018 的 heap-backed `StringOwner` 创建边界逐项复制 bytes；
  不让 Koven String 借用宿主 argv storage，也不把它标成 static provenance。allocation failure
  与受检分配溢出沿用 ADR-0008/0018 的 abort + no-unwind 边界。
- `Array<String>` 精确复用 ADR-0008/SPEC-0036 的 `{buffer pointer, logical length}` header 和单一
  连续 buffer。零参数使用既有 sentinel；非空参数只分配一次 Array buffer，再按正序写入完整
  String owner。wrapper-local header storage 不获得第二个 heap allocation。
- 完整 Array owner 建立后，wrapper 对它建立一次 shared loan并调用参数化 Koven entry；该调用
  复用 ADR-0016 的 Borrow pointer ABI。正常返回后结束 loan，按逆索引顺序 drop String 元素并
  释放 Array buffer，最后返回 0。
- Koven `error()`、runtime size/OOM failure 或其他 abort 不 unwind，因而不执行 wrapper cleanup；
  这与现行 runtime 一致。UTF-8/shape operational failure 已由先行无分配预检避免部分 owner。

### CLI `run` 参数转交

- public run 增加单一 separator 形状：
  `kovenc run <source.ko> [--entry <name>] [-- <program-arg>...]`。separator 前仍完全使用现有
  compiler 参数规则；separator 后的每个宿主 `OsString` 按原始平台字节交给生成的 executable，
  不能先经 `to_string_lossy` 或 shell 拼接。
- `--` 后可以为空。零参数或显式 entry 的 process wrapper 可以忽略宿主参数，和直接运行构建
  产物时的行为一致；只有 conventional 参数化 main 把它们转换为 Koven Array。
- `kovenc build` 语法不变。用户直接启动构建产物时，平台自然提供同一 `argc/argv` ABI。

## 替代方案

### 让 CLI 在编译时把参数烘焙进 object

不采用。build 产物必须在未来每次启动时接收当次 argv；烘焙会让 `build` 与 `run` 产生不同
程序语义，也不能支持用户直接运行 executable。

### 把宿主 `char **` 直接作为 `Array<String>` Borrow

不采用。C argv 是逐项 pointer、NUL 终止且不携带 Koven String/header/provenance；它既不是连续
String owner buffer，也无法满足 UTF-8、drop 和 Borrow ABI。

### 在构造过程中逐项验证 UTF-8 并回滚

不采用首版。它需要可验证的 partial Array owner 和反向清理状态机，而现有 ContainerConstruct
只发布完整 owner。先全量无分配预检可让非法输入在 owner 创建前失败，保持边界更小。

### 所有 entry 一律改成 `main(argc, argv)`

不采用。零参数 wrapper 已稳定实现和验收，强制改签名没有语言收益并扩大回归面。两种 native
plan 仍各自只生成一个 external `main`。

### 通过环境变量或 JSON 传参

不采用。它改变平台命令行语义、编码和顺序，并引入未批准的 IO/parser runtime。

## 后果

收益：

- 参数化 entry 的 shape、Array/String identity 与 Borrow mode 在 LLVM 前可验证；
- argv 不借用宿主 pointer、不使用 replacement character，并复用既有 String/Array/drop ABI；
- 两阶段预检避免为 operational failure 引入 partial-owner IR；
- `build` 产物直接执行和 `kovenc run -- ...` 走同一真实 process ABI。

代价与风险：

- codegen 需要新的 native entry plan/verifier 和第二种 C wrapper；
- UTF-8 validator、checked argv 扫描与 Array/String 构造会增加 wrapper/runtime IR；
- 首个 bridge 仍只承诺 ADR-0007 的 AArch64 macOS target；Windows wide argv 和其他平台入口需
  新目标 ADR 扩展，不能套用 `char **`；
- 非法 UTF-8 只形成进程 operational failure，没有 Koven source span 或稳定机器诊断 payload。

## 关联

- 首个实施 Spec：[SPEC-0194](../specs/0194-parameterized-main-argv.md)
- 前置 Spec：SPEC-0192 `done`
- 相关 ADR：[ADR-0007](./0007-llvm-toolchain-and-first-target.md)、
  [ADR-0008](./0008-internal-value-and-allocation-abi.md)、
  [ADR-0010](./0010-first-native-object-and-linker-contract.md)、
  [ADR-0016](./0016-interprocedural-borrow-abi.md)、
  [ADR-0018](./0018-string-owner-runtime-abi.md)
- 取代的 ADR：无；扩展 ADR-0010 明确预留的参数化入口边界
- 被以下 ADR 取代：无
