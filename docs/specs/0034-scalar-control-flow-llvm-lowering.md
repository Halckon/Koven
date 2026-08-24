# SPEC-0034：标量与控制流经 typed SSA lower 到 LLVM IR

## 1. 元数据

| 字段 | 值 |
|---|---|
| 状态 | `in-progress` |
| Goal ID | `KOV-P4-034` |
| 所属 Phase | Phase 4 |
| 语言规范 | 现行 [v0.28 Phase 4](../guide/06-roadmap.md#phase-4llvm-代码生成) 与 §16 checked integer 语义 |
| 批准依据 | 当前持续 Goal 的站立授权；2026-08-24 LLVM 兼容门禁已实际通过 |
| 前置 Spec | SPEC-0033 `done`；SPEC-0019、0021、0029、0177、0174 已由其前置链覆盖 |
| 前置 ADR | [ADR-0006](../adr/0006-typed-ssa-block-parameters.md)、[ADR-0007](../adr/0007-llvm-toolchain-and-first-target.md) 均 `accepted` |
| 阻塞项 | 无；后续步骤仍须按本 Spec 验收，不能由 smoke 代替 |
| 影响范围 | `lang-codegen` frontend→SSA lowering、scalar/control SSA operation、LLVM adapter、依赖与测试；Architecture |
| 语言语义变更 | 否；只实施现行 guide 已封闭的标量、控制流、checked overflow/除零与 abort 语义 |

## 2. Goal

完成后，`lang-codegen` 能把无 frontend error、且属于本 Spec 封闭子集的单文件顶层标量函数，
从 `ParsedFile` / `NameResolution` / `TypedFile` / `OwnershipCheckedFile` lower 为通过 SPEC-0033
verifier 的 typed SSA，再把该 SSA 映射为通过 LLVM verifier 的 LLVM IR。两个阶段均有独立、
可查询的内部错误，不允许绕过自建 SSA 直接生成 LLVM。

## 3. 范围与需求

### 3.1 输入门禁与 frontend→SSA

- lowering 入口显式接收同一 source 的解析、名称、类型和所有权产物；source/environment identity
  不一致、任一 frontend diagnostics 非空或本次使用节点具有 blocking deferred fact 时返回
  `LoweringError`，不产生可执行 SSA。
- 本 Spec 封闭 Unit、Boolean 与 Byte/UByte/Short/UShort/Int/UInt/Long/ULong；类型映射使用
  IR-local Unit/Boolean/有符号或无符号 8/16/32/64-bit type。Float/Double/Char/String、nullable、
  aggregate/class/container/closure/interface/dyn 不伪装成整数或 opaque scalar。
- lower 非泛型顶层具名函数，以及 SPEC-0177 已给出具体实例 identity 的纯标量泛型实例；实例
  图必须有确定排序与显式递归/增长门禁，不能按源码名称合并 overload 或实例。
- 支持标量参数、局部 `val`/`var`、赋值、block、`if`/Boolean `when`、`while`/`for` 已封闭的
  标量控制形式、`return`/`break`/`continue`、直接单态 call、`error()`/`Nothing` abort。Borrow
  的 Copyable scalar 可按只读值 lower；`inout`、member/delegation receiver 与借用返回继续遵守
  frontend deferred 边界，不在本 Spec 猜测 ABI。
- 支持整数/Boolean literal、名称、group、guide 已定义的前缀、算术、比较、相等、逻辑与短路
  运算。必须保留 AST/source origin、源码求值顺序与控制边；unsupported typed node 返回明确
  `UnsupportedNode`，不能 panic、静默跳过或产生部分有效函数。
- 每个完成函数先运行 SPEC-0033 verifier。lowering bug 返回包含 source origin 与 SSA ID 的
  内部错误；不复用 frontend `Diagnostic` 或 `Lxxxx`。

### 3.2 SSA operation 与 checked integer

- 扩展最小 scalar operation contract 以覆盖所需前缀、除法、取余、完整比较和直接 call；
  operand/result type、signedness、call signature 与 effect 继续由 verifier 锁定。
- `+`/`-`/`*` 是 checked operation；有符号和无符号溢出均走 abort edge，不允许在 SSA 或
  LLVM 中退化成 wrapping。`/`/`%` 检查零除数；有符号 MIN / -1 同样 abort。
- Boolean `&&`/`||` 与条件表达式形成 CFG 和 block parameters，不以 eager bitwise 运算替代。
  `Nothing` 路径使用 `Abort` terminator，不生成正常 successor 或 cleanup/unwind edge。
- Unit function 在 SSA/LLVM 边界使用一致的零结果约定；Unit 表达式不伪造可寻址 payload。

### 3.3 SSA→LLVM IR

- LLVM adapter 只接收已验证的 SSA，映射 module/function/basic block/value/type/operation/
  terminator；block parameters 在 LLVM block 首部形成有序 PHI incoming，不泄漏回 SSA model。
- LLVM Boolean 为 `i1`，整数按精确 bit width/signed operation 映射；signedness 不改变 LLVM
  integer type identity，但决定比较、除法、取余和 overflow intrinsic。
- checked add/sub/mul 使用对应 LLVM overflow intrinsic 或等价显式检查；失败路径统一 lower
  为不可返回的 abort/trap，不生成异常展开。LLVM verifier 必须在产物返回前通过。
- debug text 需确定：相同输入重复 lowering 的 LLVM IR 在忽略 LLVM 自身非语义句柄地址后
  完全一致。错误不得包含机器绝对路径或随机集合顺序。
- 首个 target 仅为 ADR-0007 的 `aarch64-apple-darwin`；本 Spec 不生成 object、不调用 linker、
  不执行 JIT，也不宣称产物可分发。

## 4. 非目标

- 不实现 aggregate/value class/class/Box/layout、heap/runtime、顺序容器、closure environment、
  object/companion、DWARF、object emission、链接或 CLI 编译流水线；分别由 SPEC-0035–0040 承接。
- 不实现 Map、Phase 5 API、receiver 所有权、callable reference、safe call、nullable lowering、
  exception/unwind、优化 pass、constant folding 或跨 target codegen。
- 不以临时 C transpilation、JIT、解释执行或直接 AST→LLVM 绕过现行流水线。

## 5. 验收标准

- [ ] frontend identity/diagnostic/deferred 门禁有独立正反例；非法输入不产生 SSA/LLVM module。
- [ ] Unit/Boolean 与全部 8/16/32/64-bit signed/unsigned integer 映射、literal 边界和类型替换
      由稳定测试锁定。
- [ ] 顶层标量函数、局部绑定/赋值、direct call、if/Boolean when、loop、break/continue/return
      lower 后通过自建 verifier；source origin、实例 key 与求值顺序可查询且确定。
- [ ] checked add/sub/mul、零除数、signed MIN/-1、比较和 short-circuit 正反矩阵在 SSA 与 LLVM
      文本中保留 abort edge，未出现 wrapping 或异常展开。
- [ ] diamond、loop backedge、多 return 与 block parameters 生成合法 PHI incoming；人工损坏的
      SSA 被自建 verifier 拦截，不进入 Inkwell。
- [ ] LLVM module verifier 通过合法矩阵并拒绝 adapter 人工损坏产物；重复生成文本相同。
- [ ] 依赖只存在于 `lang-codegen` LLVM adapter；manifest/lockfile、feature、license/build.rs
      审计与 ADR-0007 一致，无 LLVM 类型泄漏到 frontend/SSA 公共模型。
- [ ] `lang-codegen` 窄测和 workspace 标准基线通过；Architecture、Spec/ADR 索引与 roadmap
      只同步实际完成事实。

## 6. 技术方案与边界

- `ssa` 继续保持 target-independent；frontend lowering、SSA operation 扩展和 LLVM adapter
  按职责拆分，生产文件遵守 1000 行软上限。
- frontend `TypeId`、`SymbolId`、AST ID 与 callable instance key 只存在于 lowering context；
  完成后 SSA 只保存 IR-local ID/type/origin。
- LLVM Context/Module/Builder 生命周期封装在一次 adapter 调用中；不建立全局 context，不把
  Inkwell value 放入 AST/TypedFile 或持久缓存。
- 每个阶段先做完整输入门禁，再构造局部产物并在成功后提交；失败不返回“部分合法”module。

## 7. 实施计划

1. [x] 安装/验证 LLVM 21 工具链与 Inkwell 最小 smoke matrix，接受 ADR-0007。
2. [x] 扩展 scalar SSA operation/verifier contract → 验证：checked arithmetic/call 正反矩阵。
3. [ ] 建立 frontend→SSA identity/type/function/body lowering → 验证：标量直线函数窄测。
   - [x] 完成无泛型顶层 expression-body 函数的 identity/diagnostic/deferred 门禁、标量类型、
         literal/name/group/prefix/checked arithmetic/comparison/direct-call lowering，并在返回前运行
         SPEC-0033 verifier。
   - [x] 完成直线 block、嵌套 block、局部 `val`/`var`、普通/复合赋值与显式 return lowering；
         block 尾部仍遵守 Unit 语境，不引入尾表达式值。
   - [ ] 接续具体泛型实例 lowering 后完成本步。
4. [ ] lower branch/loop/return/short-circuit 与 block parameters → 验证：CFG/PHI 前置矩阵。
5. [ ] 实现 SSA→LLVM type/function/operation/terminator adapter → 验证：LLVM verifier/text matrix。
6. [ ] 运行 workspace 基线、同步事实并审查依赖/diff → 验证：实际退出状态与独立提交。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | LLVM/Inkwell 工具链 smoke、ADR 与依赖锁定 | `build(codegen): pin LLVM 21 toolchain (SPEC-0034)` |
| 2 | scalar SSA operation/verifier 扩展 | `feat(codegen): extend scalar SSA contracts (SPEC-0034)` |
| 3 | frontend scalar/control-flow→SSA lowering | `feat(codegen): lower scalar frontend to SSA (SPEC-0034)` |
| 4 | verified SSA→LLVM IR adapter与事实文档 | `feat(codegen): lower scalar SSA to LLVM (SPEC-0034)` |

## 9. 未决问题

- frontend 当前对部分合法但非本 Spec 标量子集的节点只提供通用 typed facts；实现前需逐项
  确认公开 getter 足够，不为方便扩大整个 AST/typed model 的可变或 `pub` 边界。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-24 边界审计 | 通过 | 确认 SPEC-0034 采用 frontend→verified SSA→verified LLVM IR 的单一垂直 Goal；object/link/run 排除 |
| `/opt/homebrew/opt/llvm@21/bin/llvm-config --version/--host-target/--shared-mode` | 通过 | LLVM 21.1.8；`arm64-apple-darwin25.2.0`；shared |
| `LLVM_SYS_211_PREFIX=/opt/homebrew/opt/llvm@21 cargo check -p lang-codegen --all-targets` | 通过 | Inkwell 0.10.0 / llvm-sys 211.0.1 编译链接 |
| `LLVM_SYS_211_PREFIX=/opt/homebrew/opt/llvm@21 cargo test -p lang-codegen --all-targets` | 通过 | 25 项；含确定 AArch64 标量 IR 与 LLVM verifier 正反例 |
| `otool -L <lang-codegen test binary>` | 通过 | 动态链接 `/opt/homebrew/opt/llvm@21/lib/libLLVM.dylib` 21.1.8；测试 binary hash 不作为稳定接口 |
| workspace 标准基线（均设置 `LLVM_SYS_211_PREFIX`） | 通过 | fmt、check、Clippy `-D warnings`、all-targets test、`lang-cli` build 均退出 0 |
| `cargo test -p lang-codegen --all-targets`（设置 LLVM prefix） | 通过 | 28 项；五类 checked arithmetic、显式 abort CFG、六类 comparison、Boolean not、direct-call 正反矩阵 |
| `cargo clippy -p lang-codegen --all-targets -- -D warnings`（设置 LLVM prefix） | 通过 | operation verifier 拆分后无 warning，全部生产文件低于 1000 行软上限 |
| 2026-08-25 workspace 标准基线（均设置 LLVM prefix） | 通过 | fmt、check、Clippy `-D warnings`、all-targets test、`lang-cli` build 均退出 0 |
| `cargo test -p lang-frontend --test ownership_checking` | 通过 | 14 项；新增同 source 下跨 environment、跨 name-analysis 与跨 typed-analysis 混用拒绝 |
| `cargo test -p lang-codegen --all-targets`（设置 LLVM prefix） | 通过 | 31 项；真实 Lexer→Parser→名称→类型→所有权→SSA 流水线覆盖顶层 expression-body 标量函数、命名实参 direct call、checked abort CFG、分析链混用和 unsupported body 拒绝 |
| `cargo clippy -p lang-codegen --all-targets -- -D warnings`（设置 LLVM prefix） | 通过 | frontend→SSA 首个切片无 warning；生产 lowering 文件低于 1000 行软上限 |
| 2026-08-25 frontend→SSA 检查点 workspace 标准基线（均设置 LLVM prefix） | 通过 | fmt、check、Clippy `-D warnings`、all-targets test、`lang-cli` build 均退出 0 |
| `cargo test -p lang-codegen --all-targets`（设置 LLVM prefix） | 通过 | 32 项；新增直线/nested block、局部 `val`/`var`、checked 复合赋值、显式 return 与 Unit fallthrough 真实流水线覆盖 |
| `cargo clippy -p lang-codegen --all-targets -- -D warnings`（设置 LLVM prefix） | 通过 | block/local/return 切片无 warning；生产 lowering 文件 977 行，未超过软上限，CFG 切片前拆分职责 |
| 2026-08-25 block/local/return 检查点 workspace 标准基线（均设置 LLVM prefix） | 通过 | fmt、check、Clippy `-D warnings`、all-targets test、`lang-cli` build 均退出 0 |
