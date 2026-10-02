# 流水线与 Workspace

> **性质**：当前实现事实 · **状态**：current · **读取时机**：修改 crate 边界、编译阶段或构建编排时 · **唯一真源**：`Cargo.toml`、代码与测试

## Workspace 边界

根 workspace 固定包含五个 member，依赖保持单向且无环：

| Crate | 当前职责 | 直接 workspace 依赖 |
|---|---|---|
| `lang-frontend` | Source/Span、Lexer、Parser/AST、名称、类型、所有权、诊断、formatter | 无 |
| `lang-codegen` | typed SSA、验证、frontend lowering、LLVM、object 生成 | `lang-frontend`、`inkwell` |
| `lang-cli` | `kovenc` 命令、诊断渲染、project discovery、链接与运行 | `lang-frontend`、`lang-codegen` |
| `lang-lsp` | stdio LSP、单文档和 source-set 会话、诊断与 definition adapter | `lang-frontend` |
| `lang-std` | Cargo 边界及 `koven/prelude.ko` | 无 |

共享 edition、MSRV、发布属性、lint 和依赖版本在根 `Cargo.toml` 集中维护；实际版本只以该文件和
`Cargo.lock` 为准。LLVM 只出现在 `lang-codegen` 内，frontend 类型和 AST 不暴露 LLVM 表示。

## 编译数据流

```text
SourceMap
  → LexedFile
  → ParsedFile / SyntaxAst
  → CompilationUnitIndex / CompilationUnitNames
  → CompilationUnitTypes
  → CompilationUnitOwnership
  → OwnedCompilationUnitView（普通路径）/ 独立 const gate
  → unit plan / typed SSA Program
  → verified LLVM module
  → object
  → linked executable
```

单文件分析保留对应的 `NameResolution`、`TypedFile` 和 `OwnershipCheckedFile` 入口；project、LSP
source-set 与多文件 native 路径使用 compilation-unit 产物。每一阶段显式接收前一阶段结果和共享
身份，不通过全局可变状态交换事实。

可执行后端只消费 validated compilation-unit gate。普通源码错误留在 recovery 产物中；来源、分析
环境或阶段身份不匹配属于内部错误，不会伪造成语言诊断。

## 普通 owned-unit 交接

frontend `ownership_checking::owned_compilation_unit_view` 接收 sources、inputs、validated names、
environment、validated types/ownership，返回 `OwnedCompilationUnitView<'view, 'parsed: 'view>`。
六字段私有且不可变；五个 getter 只读 sources/inputs/names/types/ownership，环境只保留借用。
调用方拥有 SourceMap、ParsedFile 和阶段产物，临时 inputs 与 view 在调用栈建立，不复制 AST、
不存回自引用 snapshot。const 和 recovery 类型不能进入 factory，也没有 unchecked 或 mutable 入口。

工厂一次重建 index，依次核 source/input、names index、signatures 保存的 index/names owners/
environment owner、owned 对应的 typed body owner，随后丢弃 rebuilt；下游读取 names 既有 index。
签名 `is_compatible_with_index` 是 frontend crate-private 共享比较；旧 public compatibility 仍自行
建 index。合法 clone、输入重排及同 T0 重做 ownership 保持有效，fresh T1 与 O0 仍拒绝。

ordinary native 从 view 直接到 view lower、私有 raw-facts driver 和 planner，不回旧校验入口。
旧 native 保持八参签名，先调用用户 `entry.into()` 恰一次，再 factory 并转新入口；旧 lower/
planner 保留受限可见性并各自通过 factory。CLI project ordinary 分支在原 entry selection 后建 view，
const fallback、诊断分流、独立 const capability/gate 和共享 raw-facts driver 保持原边界。

普通 unit 发布顺序保持 factory→entry shape→lower/SSA verify→native entry plan→reserve→
LLVM/layout/object emission→atomic commit；旧 native 的用户 Into 在 factory 前完成。
身份、entry、SSA 与 entry plan 失败不 reserve；LLVM/layout 检查仍在 sibling 上执行。
N1 只证明 commit 失败清理，尚未注入真正 LLVM emission 失败；H1/H2 证明 CLI 邻层保护。

[原合同基线](../development/unit-handoff-contract-baseline.md)的八项身份已保留并扩双路，另加 Into 顺序测试；
实际通过范围、Display 逐字 oracle、reserve 与生命周期证据见[0249 账本](../specs/active/0249-owned-unit-borrowed-handoff.md#6-唯一验收账本)。
[动态计数与同期采样](../development/owned-unit-handoff-measurement.md)分别验收；次数减少已实测，耗时噪声不支持性能收益结论。

## 目标与产物

当前 native 路径按编译器宿主选择目标，不提供 `--target` 或交叉编译：

| 宿主 | LLVM target | 目标文件 | CLI 链接 driver |
|---|---|---|---|
| AArch64 macOS | `aarch64-apple-darwin` | Mach-O 64-bit AArch64 | `/usr/bin/clang` |
| x86_64 Linux + glibc | `x86_64-unknown-linux-gnu` | ELF64 x86_64 | `/usr/bin/cc` |

LLVM 21.1.x / Inkwell 0.10.0 启用 AArch64 与 X86 backend feature。LLVM adapter 的
`native_target_triple` 验证宿主架构、系统、GNU 环境与 64 位指针组合，由同一 TargetMachine
设置 triple、DataLayout 和生成 object；不支持的宿主返回 `LlvmAdapterError::Target`。
两个目标均使用 generic CPU、无额外 CPU feature、PIC 与默认 code model。系统 C driver
负责平台启动对象与系统库。
object 和 executable 使用同目录临时文件并在成功后原子发布；失败不覆盖旧目标。
支持范围的长期决定见 [ADR-0026](../adr/accepted/0026-linux-x86-64-native-host.md)。

标准库公共源码位于 `crates/lang-std/koven/`。CLI 将 prelude 与用户 source-set 作为显式编译输入，
而不是让 frontend 隐式读取文件系统。

## 不变量

- `SourceId`、AST ID、symbol ID、type ID 和 SSA entity ID 都只在各自 owner 内有效。
- 稳定输出按逻辑身份排序，不依赖 source 加载顺序或随机哈希迭代。
- frontend 只发布语义事实；codegen 不重新执行名称选择或类型推断。
- CLI/LSP 负责 IO 和协议适配；库层不直接打印用户诊断。
