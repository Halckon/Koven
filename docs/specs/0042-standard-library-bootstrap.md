# SPEC-0042：标准库目标语言 bootstrap 闭环

## 1. 元数据

| 字段 | 值 |
|---|---|
| 状态 | `done` |
| Goal ID | `KOV-P5-042` |
| 所属 Phase | Phase 5 |
| 语言规范 | 现行 [v0.28 Phase 5](../guide/06-roadmap.md#phase-5最小标准库用目标语言自身编写) |
| 批准依据 | 当前持续 Goal“继续推进 guide 主线，分阶段实施 specs”的站立授权 |
| 前置 Spec | SPEC-0039 `done`；SPEC-0033/0034 frontend→SSA→LLVM 前置链已完成 |
| 前置 ADR | [ADR-0002](../adr/0002-bootstrap-workspace-layout.md)、[ADR-0007](../adr/0007-llvm-toolchain-and-first-target.md)、[ADR-0008](../adr/0008-internal-value-and-allocation-abi.md)、[ADR-0010](../adr/0010-first-native-object-and-linker-contract.md)、[ADR-0012](../adr/0012-standard-library-bootstrap.md) `accepted` |
| 阻塞项 | 无；SPEC-0040 的 LLDB 环境门禁不在本 Spec 依赖链上，通用源码入口/多文件/package/manifest 明确排除 |
| 影响范围 | `lang-frontend` 内建环境入口；`lang-codegen` source-analysis→object 公共边界；`lang-cli` bootstrap driver 与测试；`lang-std` Koven smoke source；Architecture、roadmap |
| 语言语义变更 | 否；只编排现有单文件 frontend、显式 entry backend 与仓库标准库源码，不发布新关键字、类型、诊断或通用 CLI/entry 规则 |

## 2. Goal

完成后，仓库能够从磁盘读取 `lang-std/koven/prelude.ko`，经真实 Lexer→Parser→名称→类型→
所有权→verified SSA→LLVM object→Clang link 流水线构建并运行其显式 bootstrap callable；公共
标准库实现仍只有 Koven 源码一份，错误输入不会产生或复用伪成功产物。

## 3. 范围与需求

### 3.1 唯一内建环境

- `lang-frontend` 发布显式构造标准编译环境的入口，按规范顺序声明全部 16 个 `BuiltinType`
  并绑定同一 `NameEnvironment` / `TypeEnvironment` identity；不使用全局 singleton。
- 重复构造结果顺序和身份内关系确定；CLI 与新增生产代码不得复制 builtin 数组。既有测试可在
  本 Spec 保持局部 fixture，不做无关批量重构。

### 3.2 显式 source entry 到 object

- `lang-codegen` 提供最小公共 object API，输入 `SourceMap`、`ParsedFile`、`NameResolution`、
  `TypedFile`、`OwnershipCheckedFile`、已解析的顶层 `SymbolId` entry 与输出路径。
- API 复用现有 `lower_scalar_file`、SSA verifier、DWARF object emission 和 ADR-0010 entry wrapper；
  不建立第二套 lowering。lowering 必须返回 entry symbol 对应的非泛型 `FunctionId`，不得按 SSA
  display name 反查。
- foreign analysis、未知/泛型/非顶层 entry、非 `() -> Unit`、frontend diagnostics、deferred、
  unsupported node、SSA/LLVM/object 失败均返回结构化 workspace API 错误，并在写盘前失败或确保
  本次输出不存在。

### 3.3 仓库内 bootstrap driver

- `lang-cli` 建立不进入用户参数协议的 bootstrap driver：从显式 source path 读取 UTF-8 bytes，
  使用标准环境执行全部 frontend pass，在名称事实中把配置的唯一顶层 function name 解析为
  `SymbolId`，调用 codegen API生成临时 object，再复用 `linker` 生成 executable。
- entry 缺失、同名 overload/非 function、签名错误或任一 frontend diagnostic 必须形成确定的
  driver error；不得默认选择 `main`、第一项或任意同名候选。driver 启动和进程非零退出保持
  独立错误类别。
- 临时路径由调用方测试拥有并精确清理；driver 不删除输入 `.ko`，不提交 object/executable，
  不引入 shell、glob、manifest 或多文件发现。

### 3.4 真实标准库 smoke

- `prelude.ko` 加入一个无参数、显式 `Unit` 的最小 bootstrap callable，源码自身必须通过完整
  frontend；不在 Rust 中复制其行为。
- `lang-cli` 测试从 workspace 中真实路径读取该文件，证明恰好枚举到预期 bootstrap source，
  生成并运行 executable，断言退出码 0；同时用受控坏输入锁定无 entry/错误签名/源码诊断不会
  链接或运行。

## 4. 非目标

- 不实现 SPEC-0043 的 prelude、`error()`、字符串/输出、Pair/Result、Box/Rc、集合 API、IO、
  thread/channel 或目标语言测试 runner。
- 不定义用户可见 `main`、`kovenc` 参数、项目 manifest、package/import 展开、多文件合并、增量
  缓存、安装目录、交叉编译或发布产物。
- 不扩大当前标量 frontend→SSA 封闭子集，不接 nominal constructor、`for`、instance receiver、
  Map、extern FFI 或尚未实现的标准库 intrinsic。
- 不新增 crate、第三方依赖、Rust runtime shim、`lang-std/build.rs` 或标准库 Rust 镜像。

## 5. 验收标准

- [x] 标准内建环境由一个 production 入口确定构造，全部 builtin identity/顺序与重复构造测试
      通过，CLI 没有复制 builtin 表。
- [x] codegen 公共 API 只接受匹配 analysis chain 的 resolved `SymbolId` entry；合法 `() -> Unit`
      生成含唯一 C `_main` 的 object，foreign/unknown/泛型/错误签名及诊断链在写盘前失败。
- [x] bootstrap driver 对 source read、frontend diagnostics、entry resolution、codegen、linker 与
      process exit 失败分层建模，不按任意 `main`/声明顺序猜测入口，不经 shell。
- [x] 磁盘上的 `lang-std/koven/prelude.ko` 经完整流水线生成 executable 并真实运行退出 0；测试
      证明非空 source 枚举、无 Rust 行为镜像、失败不复用旧产物。
- [x] `lang-frontend`、`lang-codegen`、`lang-cli`、`lang-std` 窄测与 workspace 五项标准基线通过；
      production 文件遵守 1000 行软上限，Spec/Architecture/roadmap/ADR 索引只记录实际事实。

## 6. 技术方案与边界

- 在 `BuiltinType` 旁维护唯一 `ALL` 顺序，并由 frontend helper 同步创建两个显式 environment；
  `TypeTable` 复用同一顺序，避免生产路径出现第二张表。
- `lower_frontend::orchestrate` 返回 crate-private `{ Program, FunctionInstanceKey→FunctionId }`
  产物；公共 codegen facade 只允许空类型实参的 entry key，随后调用既有 debug object emission。
  SSA model、FunctionId 与 Inkwell 类型不泄漏出 crate。
- CLI bootstrap driver 使用小型内部 error enum 保存阶段和底层错误，不把 stderr 或 debug 文本
  固化为稳定用户协议。测试 runner 负责创建唯一临时目录并在 drop 时清理。

## 7. 实施计划

1. [x] 建立标准内建环境与显式 source-entry codegen facade → 验证：environment identity、entry
   正反矩阵、object symbol 与失败不落盘。
2. [x] 实现 CLI bootstrap driver 和 `prelude.ko` smoke → 验证：真实文件全链路 link/run、entry/
   source/diagnostic/进程失败矩阵。
3. [x] 运行 workspace 基线、同步 Architecture/roadmap/Spec 并审查 staged diff → 验证：实际退出
   状态、文件规模、文档与实现一致。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | 内建环境、resolved source entry 与 codegen object facade | `feat(codegen): expose source object emission (SPEC-0042)` |
| 2 | CLI bootstrap driver、真实 Koven standard source 与完成记录 | `feat(std): bootstrap Koven standard sources (SPEC-0042)` |

## 9. 未决问题

- 无。通用源码 entry、完整 CLI 与多文件构建是明确非目标；若实现必须固定其中任一语义才能
  继续，应停止并修订 guide，而不是扩大本 Spec。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-25 前置审计 | 通过 | SPEC-0039 `done`；ADR-0002/0007/0008/0010/0012 `accepted`；SPEC-0040 不在依赖链；现有 scalar frontend→SSA、object emitter 与 linker driver 可复用 |
| `cargo test -p lang-frontend type_checking::tests::standard_environments_declare_every_builtin_once_in_canonical_order` | 通过 | 16 个 builtin 的唯一名称、规范顺序、type binding 与重复构造一致性通过；该命令还枚举了 frontend 全部 integration target，过滤项未冒充执行 |
| `cargo test -p lang-codegen --lib native_tests` | 通过 | 3 项；resolved Unit entry 生成 arm64 Mach-O 和唯一 `_main`，参数化/泛型/非 Unit/非 function/unknown、混用 analysis、frontend diagnostics 与 unsupported source 均失败不落盘 |
| `cargo clippy -p lang-frontend -p lang-codegen --all-targets -- -D warnings` | 通过 | 无 warning；新增 `native.rs` 155 行、`native_tests.rs` 约 230 行，既有超限 `type_checking/model.rs` 因复用 canonical builtin 表减少重复而未继续承载独立职责 |
| `cargo test -p lang-frontend --lib` | 通过 | 33 项；包含标准环境 production helper 的完整 frontend 单元矩阵 |
| `cargo test -p lang-codegen --all-targets` | 通过 | 93 项；source object facade 与既有 SSA/LLVM/object/debug/link-run 回归均通过 |
| `cargo test -p lang-cli --all-targets` | 通过 | 11 项；真实磁盘 `prelude.ko` bootstrap 退出 0，诊断、entry、路径、link 与 runtime abort 失败矩阵通过 |
| `cargo test -p lang-std --all-targets` | 通过 | 1 项；Cargo source-package 边界仍可验证，公共实现未增加 Rust 镜像 |
| workspace 五项标准基线 | 通过 | `cargo fmt --all -- --check`、`cargo check --workspace --all-targets`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets`、`cargo build -p lang-cli` 均以退出码 0 完成 |
| production 文件规模 | 通过 | 新增 `bootstrap.rs` 188 行、`native.rs` 155 行；新增职责均位于独立且低于软上限的模块。既有 `type_checking/model.rs` 1053 行是本 Spec 前已超限的集中模型表，本次只把其重复 builtin 初始化改为复用同一常量，未加入独立变化原因 |
