# SPEC-0040：DWARF 源码行表与首个 LLDB 验收

## 1. 元数据

| 字段 | 值 |
|---|---|
| 状态 | `in-progress` |
| Goal ID | `KOV-P4-040` |
| 所属 Phase | Phase 4 |
| 语言规范 | 现行 [v0.28 Phase 4](../guide/06-roadmap.md#phase-4llvm-代码生成) |
| 批准依据 | 当前持续 Goal“继续推进 guide 主线，分阶段实施 specs”的站立授权 |
| 前置 Spec | SPEC-0039 `done`；SPEC-0033/0034 verified SSA/LLVM 前置链已完成 |
| 前置 ADR | [ADR-0004](../adr/0004-source-span-position-model.md)、[ADR-0007](../adr/0007-llvm-toolchain-and-first-target.md)、[ADR-0010](../adr/0010-first-native-object-and-linker-contract.md)、[ADR-0011](../adr/0011-first-dwarf-line-mapping.md) `accepted` |
| 阻塞项 | 无；本机 `/usr/bin/dwarfdump` 与 `/usr/bin/lldb` 均来自 Apple LLVM 21，首个 target/debugger 前提已满足 |
| 影响范围 | `lang-codegen` LLVM debug metadata、object emission API 与测试；Architecture、roadmap |
| 语言语义变更 | 否；只实施 ADR-0011 的首个 target 行表映射，不新增源码语义或调试表达式协议 |

## 2. Goal

完成后，编译器能够用显式 `SourceMap` 把 verified SSA origin 映射为未优化 AArch64 Mach-O
object 的确定性 DWARF 行表，并让 LLDB 按真实 `.ko` 文件和行号命中 Koven function；foreign
source、synthetic glue 与尚未定义的变量/类型调试不会被静默伪造。

## 3. 范围与需求

### 3.1 Debug source 输入与预检

- debug-enabled object/text lowering 接收 `&SourceMap` 和显式 native entry；`Program` 不持有
  source owner，现有无 debug LLVM 文本入口保持不变。
- 在创建 LLVM module/debug metadata 前遍历单 module 的函数、block、block parameter entity、
  instruction/result entity 与 terminator origin；每个 span 必须能从传入 map 取得 source name
  和起点 1-based line/column，并可受检转换为 `u32`。
- foreign map、未知 source 或坐标溢出返回独立 `LlvmAdapterError::Debug`，object 路径不得产生
  文件；错误不进入 frontend `Diagnostic`，不 panic。

### 3.2 DWARF line-table metadata

- 新增职责单一的 `llvm::debug` 模块，封装 compile unit、按 source name 确定排序的 `DIFile`、
  Koven `DISubprogram`、origin→`DILocation` 与 finalize 生命周期；不把 Inkwell DI 类型泄漏到 SSA。
- 按 ADR-0011 使用 debug metadata version flag、`DW_LANG_C` fallback、producer `kovenc`、
  `LineTablesOnly` 和 `is_optimized=false`。entry origin source 是 compile unit 主文件，source name
  原样进入 filename，directory 为空。
- Koven function 使用源码名作为 display name、现有 `f{id}.{name}` 作为 linkage name；function
  origin 决定 subprogram line，block/PHI、instruction、terminator 分别使用自身 origin 起点。
  synthetic origin 映射 anchor。
- C `main` wrapper、runtime/drop/container helper 不建立伪造 Koven subprogram；各 FunctionLowerer
  在进入 source instruction/terminator 前显式设置 location，在 synthetic 边界清除 location。
  DIBuilder 在 LLVM verify 与 object write 前 finalize。

### 3.3 产物与调试器验收

- 提供 test-only debug LLVM rendering，锁定 compile unit/file/subprogram/location、display 与
  linkage name、Unicode/CRLF 行列、synthetic anchor、多 source 排序和重复 lowering 确定性。
- debug object 继续复用 SPEC-0039 的同一 verified module lowering、TargetMachine、entry wrapper
  与 object API；`/usr/bin/dwarfdump --debug-line` 必须观察到 `.ko` 文件及预期行记录。
- 测试在专用临时目录写入与 `SourceMap` 快照完全相同的 `.ko` 文件，生成 object、经
  `/usr/bin/clang` 链接，然后以 `/usr/bin/lldb --batch` 按文件/行设置断点并运行；退出成功，
  输出证明 breakpoint resolved/hit、Koven frame 与目标 source line。测试只匹配必要稳定片段，
  不固化 LLDB 全部本地化文本。

## 4. 非目标

- 不生成局部变量、参数值、Koven 类型图、泛型实例展示、closure capture 展开、容器 pretty
  printer、表达式求值、watchpoint、宏/inlining origin 或优化后 location list。
- 不定义 Koven 正式 DWARF language code，不宣称 C/Kotlin 源码或 ABI 兼容，不直接调用
  `llvm-sys` 绕过 Inkwell 安全 API。
- 不加入 `-g` CLI flag、source-path remap、compilation directory、嵌入源码、split DWARF、dSYM、
  strip/codesign、ELF/COFF、其他 CPU/OS/debugger 或发布包。
- 不实现源码 `main` 选择、完整 `.ko`→CLI 流水线、标准库 bootstrap、析构点接线、constructor、
  `for`、receiver、Map 或 Phase 5 API。
- 不新增依赖、crate、公开稳定 ABI 或 frontend 诊断码。

## 5. 验收标准

- [x] debug preflight 对全部 origin 使用同一 `SourceMap`；foreign map/无效位置在 LLVM metadata
      与 object 写盘前返回 `Debug` 错误，失败不产生 object。
- [x] debug LLVM 文本包含唯一 compile unit、按 ADR-0011 映射的 file/subprogram/location；
      Unicode/CRLF、multi-source、synthetic anchor 与重复运行确定性矩阵通过。
- [ ] Koven function 的 display/linkage name 和 internal linkage 保持区分；entry wrapper、runtime
      与 helper 不获得伪造 Koven source subprogram/location，LLVM verifier 通过。
- [ ] 同一 TargetMachine 生成的 arm64 Mach-O object 经 `dwarfdump --debug-line` 可见真实 `.ko`
      文件和预期行记录，既有唯一 `_main`/正常运行契约不退化。
- [ ] 真实 executable 经 `/usr/bin/lldb --batch` 按 `.ko` 文件/行设置断点并运行，断点成功解析、
      命中 Koven frame 且报告预期源码位置。
- [ ] `lang-codegen` 窄测和 workspace 五项标准基线通过；production 文件遵守 1000 行软上限，
      Spec/Architecture/roadmap/ADR 索引只记录实际完成事实。

## 6. 技术方案与边界

- `llvm::debug` 先把 SSA origin 解析为不含 Inkwell lifetime 的 owned source/position plan，再创建
  DI metadata；这样 foreign map 和坐标溢出在 module lowering 前 fail-loud，测试也可独立锁定
  排序与 anchor 规则。
- `ModuleLowerer` 接收可选 debug emitter；函数声明阶段绑定 `DISubprogram`，FunctionLowerer 只在
  现有 instruction/terminator lowering 边界设置 location，不让各 scalar/aggregate/runtime
  子模块各自计算源码位置。
- 现有 `render_verified_program` 保持无 debug；新增 crate-private/test-only debug render 复用同一
  `lower_verified_module`。object emission 改为显式接收 `SourceMap` 并生成首版行表，调用点数量
  当前只限 SPEC-0039/0040 测试。
- `dwarfdump`/LLDB orchestration 只存在于 target-specific 测试；生产 codegen 不启动调试器，
  `lang-cli` linker driver 边界不扩张。

## 7. 实施计划

1. [x] 建立 debug source preflight 与 metadata emitter，接入共享 module/function lowering → 验证：
   debug IR、foreign map、multi-source、Unicode/CRLF、synthetic anchor 与 LLVM verifier 矩阵。
2. [ ] 让 object emission 携带行表并增加 dwarfdump/LLDB 真机验收 → 验证：Mach-O 行表、源码断点、
   Koven frame、正常退出及既有 object 符号契约。
3. [ ] 运行 workspace 基线、同步 Architecture/roadmap/Spec 并审查 staged diff → 验证：实际退出
   状态、文件规模、文档与实现一致。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | SourceMap preflight、DWARF compile unit/file/subprogram/location 与 debug IR 验收 | `feat(codegen): emit DWARF line tables (SPEC-0040)` |
| 2 | Mach-O dwarfdump、真实 LLDB 验收、Architecture 与 done 记录 | `test(codegen): debug native Koven objects (SPEC-0040)` |

## 9. 未决问题

- 无。完整变量/类型调试、正式 DWARF language code 与 source-path remap 是明确非目标；若首版
  行表必须依赖其中任一项才能工作，应停止并修订 ADR，而不是扩大本 Spec。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-25 前置审计 | 通过 | SPEC-0039 `done`；ADR-0004/0007/0010/0011 `accepted`；Inkwell 0.10 提供 DIBuilder/LineTablesOnly API；本机 Apple LLVM 21 提供 dwarfdump/LLDB |
| `cargo test -p lang-codegen --all-targets`（debug IR slice） | 通过 | 89 项；新增 SourceMap preflight、DW_LANG_C line tables、multi-source、Unicode/CRLF、synthetic anchor、display/linkage name 与确定性矩阵 |
| `cargo clippy -p lang-codegen --all-targets -- -D warnings`（debug IR slice） | 通过 | 无 warning；`llvm/adapter.rs` 996 行，debug plan/emitter 位于独立 241 行模块 |
