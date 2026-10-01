# ADR-0010: 首个本机目标文件与链接器契约

> **性质**：架构决策记录 · **状态**：accepted · **读取时机**：任务涉及本 ADR 决策边界时 · **唯一真源**：本 ADR

## 状态

accepted

## 接受依据

当前持续 Goal“继续推进 guide 主线，分阶段实施 specs”的站立授权。

## 背景

ADR-0007 固定了首个 `aarch64-apple-darwin` target 与 LLVM 21 工具链，ADR-0008/0009 固定了
内部值、分配和 closure ABI。SPEC-0035–0038 目前只返回 verified LLVM IR 文本；SPEC-0039
需要生成 object、形成进程入口并链接可执行文件，但项目尚未决定 object 生成者、link driver、
入口 ABI、工具失败模型，以及 backend ABI 与源码语义的权限边界。

这些选择会同时影响 `lang-codegen`、`lang-cli`、构建测试和后续 bootstrap，不能留给局部调用点
自行决定。

## 决策

### Object emission

- `lang-codegen` 使用 ADR-0007 已配置的同一个 LLVM `TargetMachine` 直接输出 Mach-O object；
  不把 LLVM IR 文本交给外部 `clang` 重新编译。
- object emission 接收显式输出路径并返回结构化错误。它不读取当前目录、不选择项目布局，
  也不自行创建长期缓存。
- 生成 object 前必须再次通过自建 SSA verifier 与 LLVM verifier。target、triple、DataLayout
  与 object machine 不允许来自不同配置。

### 入口 ABI

- backend API 接收一个显式、已解析的 entry `FunctionId`，只接受 `() -> Unit`。它不按函数名、
  声明顺序或字符串 `"main"` 猜测源码入口。
- object 中额外生成唯一外部可见的 C ABI `i32 @main()` wrapper：调用指定的 Koven entry，
  正常返回时返回 `0`。Koven entry 和其余当前模块函数保持内部确定性符号。
- 参数化入口、环境参数、异步入口、多 entry、Windows CRT 入口和跨 package symbol mangling
  不在首个契约内。源码层如何选中 entry 必须由后续 guide/Spec 封闭后再接入该显式 API。

### Link driver

- 首个 macOS target 通过系统 C compiler driver `/usr/bin/clang` 链接，而不是直接调用 `ld`。
  driver 负责平台启动对象、系统库和 SDK 的常规选择；Koven 不复制平台私有 linker 参数。
- `lang-cli` 使用 `std::process::Command` 直接传递参数数组，固定最小参数形状为
  `clang <object> -o <executable>`；禁止 shell 拼接、隐式 glob 和从源码内容形成命令片段。
- 编排层显式拥有 object、输出和临时目录路径。临时文件必须位于单次编译专用目录；成功或
  失败后的清理由编排层负责，codegen 不删除调用方提供的产物。
- driver 不存在、启动失败、非零退出以及 signal termination 是彼此可区分的结构化错误；
  stderr 可作为不稳定的附加文本保存，但不得成为稳定诊断码或控制流依据。

### Runtime 与 `error()` 边界

- SSA `Abort` 和已有 checked runtime failure 继续调用 C `abort`，不生成 unwind table、landing
  pad 或 cleanup path；系统 driver 负责解析该 C runtime symbol。
- `error()` 是标准库顶层函数，不是关键字。SPEC-0039 不允许按名称把任意 `error` callable
  改写为 abort；只有后续标准库/predeclared environment 发布的稳定 identity 才能接到现有
  `Abort` backend primitive。

### 确定性与可观测性

- 相同 verified SSA、target 配置和 LLVM 版本必须产生相同 LLVM symbol 集与 object emission
  请求。平台 linker 自身的不可复现元数据不被虚构为语言语义保证，后续 reproducible-build
  Spec 可进一步约束。
- 测试至少验证 object 格式、外部 `main` 唯一性、wrapper 返回值、正常进程退出码、abort
  非零终止，以及 linker 失败不会被报告成源码类型错误。

## 替代方案

### 直接调用 `ld`

拒绝。它要求 Koven 固化 SDK、启动对象、平台系统库和版本相关参数，扩大首个 AOT 切片，且
不能改善语言语义。

### 让 `clang` 从 LLVM IR 生成 object 并链接

拒绝。它形成第二条 LLVM IR→object 管线，可能使用与 Inkwell 不同的 target/DataLayout，
削弱当前 verifier 与 TargetMachine 单一真源。

### 导出用户函数本身为 C `main`

拒绝。Koven `Unit` ABI 与 C `int` 返回契约不同，也会把平台入口 ABI 泄漏到普通 callable。

### 按顶层函数名自动选择入口

拒绝。当前 guide 尚未封闭重载、可见性、package、多文件和参数化 `main` 的选择规则；backend
字符串猜测会把未批准的语言语义固定进实现。

## 后果

收益：

- object 仍由唯一 LLVM target 配置生成，linker 职责保持在 CLI 编排边界；
- C ABI wrapper 隔离平台入口与 Koven callable ABI；
- SPEC-0039 可先用显式 verified SSA entry 完成真实 object/link/run 验收，而不等待多文件解析；
- `error()` 不会因同名用户函数而获得隐藏语义。

代价与风险：

- 首个可执行链路仅支持 macOS AArch64 和 `/usr/bin/clang`；跨平台 driver discovery 需新 ADR；
- 源码到 entry 的选择仍是明确门禁，SPEC-0039 的 backend 完成不等于 CLI 已能编译任意 `.ko`；
- 系统 linker 版本与 SDK 仍可能影响二进制字节级复现，当前只保证编译阶段请求确定。

## 关联

- 相关 Spec：SPEC-0039、SPEC-0040、SPEC-0042
- 取代的 ADR：无
- 被以下 ADR 取代：无

- 被以下 ADR 局部扩展：[ADR-0026](0026-linux-x86-64-native-host.md) 新增 Linux x86_64 + glibc
  本机目标及其 feature/object/link driver 范围；本 ADR 的其他决定继续生效。
