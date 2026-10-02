# ADR-0026: Linux x86_64 本机目标扩展

> **性质**：架构决策记录 · **状态**：accepted · **读取时机**：修改受支持宿主、LLVM target、系统链接或平台验收时 · **唯一真源**：本 ADR

## 状态

accepted

## 接受依据

2026-10-01，用户在确认修复基线问题时明确授权：“是的，如果新增一个linux taget不麻烦，可以新增一个”。
本决策将该授权限定为新增 Linux x86_64 + glibc 本机目标，不扩展为任意平台或交叉编译支持。
实施与验收由 [SPEC-0228](../../archive/specs/0228-linux-x86-64-native-host.md) 记录。

## 背景

[ADR-0007](0007-llvm-toolchain-and-first-target.md) 固定 LLVM 21.1.x / Inkwell 0.10.0，首个
host/target 为 AArch64 macOS；[ADR-0010](0010-first-native-object-and-linker-contract.md)
固定 LLVM 直接生成 object、CLI 通过系统 C driver 链接的职责边界。Linux 开发环境无法执行
Mach-O object，也不应在测试中静默生成与宿主不匹配的产物。

新增受支持宿主需要同时封闭 triple、TargetMachine、DataLayout、object 格式、系统 runtime
与测试工具，而不是只替换一个 triple 字符串。现有两个目标均采用 64 位指针，允许复用已有
内部值/runtime 表示，不引入新的语言语义或公共 ABI 契约。

## 决策

### 支持矩阵与选择

| 构建并运行 Koven 的宿主 | 唯一本机 target triple | Object 格式 | 生产链接 driver |
|---|---|---|---|
| AArch64 macOS | `aarch64-apple-darwin` | Mach-O 64-bit AArch64 | `/usr/bin/clang` |
| x86_64 Linux + glibc | `x86_64-unknown-linux-gnu` | ELF64 x86_64 | `/usr/bin/cc` |

- 按 Koven 编译器自身的宿主配置选择目标；每次 codegen 只使用所选宿主的 triple、
  TargetMachine 与其 DataLayout，三者必须一致。保留 LLVM 直接生成 object 的单一路径。
- 非上述宿主在 LLVM adapter 边界返回结构化 `Target` 错误，不默认回退到 Darwin、Linux
  或任意 LLVM 默认目标，也不把内部目标错误报告为 frontend 语言错误。
- 不增加 CLI `--target`、sysroot、linker discovery、交叉编译或通用多 target 配置。
  Linux AArch64、Linux musl、macOS x86_64、Windows 与其他平台仍不在支持范围内。

### 工具链与依赖

- 保持 LLVM 21.1.x、Inkwell 0.10.0、llvm-sys 211 及现有动态链接策略；不升级依赖版本。
- Inkwell 继续禁用默认 `target-all`，在既有 `llvm21-1-prefer-dynamic`、`target-aarch64`、
  `no-libffi-linking` 基础上增加 `target-x86`。仅初始化所选宿主所需的 backend。
- `LLVM_SYS_211_PREFIX` 继续指向包含 `bin/llvm-config` 的 LLVM 21.1.x 开发安装；
  所选 backend 必须存在，LLVM 动态库必须能被构建及运行环境加载。不使用 Rustc 私有 LLVM。
- 新增 feature 复用既有依赖的许可与供应链边界，不引入新 crate、下载器、JIT 或 libffi。
  成本是额外链接 X86 backend，并在 Linux 维护真实 object/link/run 与工具链兼容证据。

### 系统链接与测试工具

- macOS 保留 `/usr/bin/clang`；Linux 使用 `/usr/bin/cc`，由系统 C driver 选择启动对象、
  glibc 与平台 linker。生产流程不要求 Clang 解释 LLVM IR，不直接调用 `ld`。
- CLI 仍使用 `Command` 参数数组，沿用 `<driver> <object> -o <executable>`、错误分类、
  caller-owned 路径和失败时保留已有输出的契约。
- 需要改写或插桩 LLVM IR 的测试属于测试专用路径。Linux 使用
  `LLVM_SYS_211_PREFIX/bin/clang` 的匹配 Clang 21，prefix 工具缺失时回退到 PATH 中的
  `clang`（同样必须兼容 LLVM 21 IR）；不能把 `/usr/bin/cc` 或旧版系统 Clang 当成
  LLVM 21 IR reader。macOS 保留现有 `/usr/bin/clang` 测试路径；CLI linker 测试使用
  与生产一致的本机系统 C driver。
- debug metadata 沿用 [ADR-0011](0011-first-dwarf-line-mapping.md) 的行表边界与 SourceMap
  契约。macOS 保留既有 `/usr/bin/lldb --batch` 断点验收；Linux 自动验收使用
  `LLVM_SYS_211_PREFIX/bin/llvm-dwarfdump` 21.1（缺失时回退到 PATH 中的匹配工具）
  核实真实 ELF 产物的 DWARF 文件/行表，并实际运行该程序。
  行表检查不能宣称 Linux 调试器源码断点、单步或变量查看已经验收。

## 替代方案

### 仅绕过 Linux 上的 native 测试

不采用。跳过不会建立受支持的编译/运行路径，也无法证明现有 runtime 在新宿主上工作。

### 直接采用任意 LLVM 默认 target

不采用。它可能扩大到未验证架构、libc 或 object 格式，也无法为 CLI 的系统 driver 选择提供
明确合同。新宿主须单独扩展矩阵并验收。

### 一并提供 `--target` 与跨编译配置

不采用。交叉编译还需要 sysroot、目标 libc、linker、runtime 与执行环境选择，超出本次
新增一个本机目标的授权和最小实现范围。

### Linux 生产链接固定 Clang 21

不采用。生产 object 已由 LLVM TargetMachine 生成，系统 C driver 足以处理本机启动对象和
glibc 链接；Clang 21 的 IR 兼容要求只属于插桩测试，不扩大生产链接前提。

## 后果

收益：

- Linux x86_64 + glibc 可使用相同 frontend、SSA、runtime 和 CLI 生成并运行本机产物。
- 目标选择、布局、object 格式和 driver 有封闭矩阵，不会静默混用 Mach-O 与 ELF。
- 保持已有阶段、内部 ABI 和原子发布边界，不引入目标相关语言规则。

代价与风险：

- 开发环境须安装 LLVM 21.1.x 开发库和宿主 C 工具链；完整测试还需匹配 Clang 与平台检查工具。
- 双 backend feature 的编译/链接成本增加，新增目标必须补充真实 native 证据。
- Linux 验收不能替代 macOS 回归；未运行平台必须明确披露，不从静态审查推定通过。
- ELF/DWARF 行表验收不等于完整调试器支持或可移植的独立二进制分发保证。

## 关联

- 相关 Spec：[SPEC-0228](../../archive/specs/0228-linux-x86-64-native-host.md)
- 局部扩展的 ADR：[ADR-0007](0007-llvm-toolchain-and-first-target.md) 的单宿主/target feature
  范围、[ADR-0010](0010-first-native-object-and-linker-contract.md) 的 object 格式与 link driver
  范围；两份 ADR 的其余决定继续生效，不整份取代或归档
- 相关 ADR：[ADR-0011](0011-first-dwarf-line-mapping.md)（既有 debug metadata 合同）
- 取代的 ADR：无
- 被以下 ADR 取代：无
