# ADR-0018：UTF-8 String owner runtime ABI

## 状态

proposed

## 接受依据

不适用（`proposed`）。本 ADR 依赖尚未启用的 v0.31 候选 §31；只有用户明确启用 v0.31
并指定其取代 v0.30 后，才能依据有效授权审计并改为 `accepted`。

## 背景

frontend 已把 `String` 识别为不可变、MoveOnly 且 Transferable 的 builtin nominal identity，
也已检查 String literal、`String + String`、`==` / `!=`、普通参数与返回。Phase 4/5 当前只有
SPEC-0189 的 `PrintLiteral` 路径：literal bytes 直接进入静态 LLVM constant，变量、参数、返回、
连接和动态输出均没有一般 runtime value。

候选 v0.31 §31 要求 plain literal 与动态 String 共享同一 typed SSA identity，并让唯一 owner、
Borrow、drop、连接、相等和 stdout adapter 可由 verifier 观察。ADR-0008 已固定 target
`DataLayout`、系统分配与 abort 边界，但没有决定 String 的字段、静态/动态存储 provenance 或
drop glue。若各 lowering 路径自行选择 Rust `String`、C string 或裸字节指针，就无法同时保证
内嵌 NUL、目标位宽、ASAP 析构和不重复释放。

## 决策

### SSA identity 与内部布局

- target-independent typed SSA 新增专用 `StringOwner` identity。它始终是 MoveOnly owner，不能
  复用 `Aggregate`、`HeapOwner`、`SharedOwner` 或宿主字符串类型；Borrow/Inout 仍通过既有 loan
  ABI 观察同一 owner。
- LLVM 内部值使用按目标 `DataLayout` 构造的 `{ bytes: ptr, length: usize, capacity: usize }`。
  `usize` 使用目标 pointer width；字段 offset、整体 size/alignment 和 allocation size 必须经
  target preflight，不能使用 host `usize` 或 Rust `String` 布局。
- `length` 是内容字节数，不包含终止符；ABI 不要求 NUL 终止，U+0000 是普通 UTF-8 字节。
  所有 live String 的 `bytes[0..length]` 必须是合法 UTF-8 且在 owner/loan 有效期内可读。
- `capacity == 0` 表示非 owning 的静态只读 storage provenance，drop 不释放 `bytes`；
  `capacity > 0` 表示唯一 heap buffer owner，必须满足 `capacity >= length`，并在唯一 drop point
  精确 `free(bytes)` 一次。首个动态创建路径分配精确容量，因此 concat 的非空结果满足
  `capacity == length`；该等式不是后续公共 ABI 承诺。
- 空串使用统一的非解引用 canonical pointer 加 `length == capacity == 0`；不得依赖系统 allocator
  对零字节分配的偶然行为。内部 ABI 不承诺 C FFI 或跨编译器版本稳定性。

### 创建、操作与析构

- `StringLiteral(bytes)` 创建 `StringOwner`：非空 literal 指向 module-private constant 且
  capacity 为 0；空 literal 使用 canonical empty。literal 仍产生普通 owner obligation，只是
  drop glue 按 provenance 成为 no-op。
- `StringConcat(left, right)` 左到右读取两个 shared loan/value view，不消费具名 operand；先做
  target-width checked length addition。空结果使用 canonical empty，非空结果集中分配精确字节
  数并按顺序复制；溢出或 allocation failure 在发布结果 owner 前 abort。
- `StringEqual(left, right)` 先比较 length，再按字节比较内容并产生 Bool；不分配、不 normalize，
  `!=` 由 typed operation 结果取反或等价专用 operation 表达。
- `PrintString(value)` 接受 active shared loan，按 length 写出全部内容后再写 ASCII LF；短写沿用
  SPEC-0189 的 abort 边界。`error(String)` 的 message 仍只需完成普通求值与 Borrow，随后进入
  既有 Abort effect，不由本 ADR 新增 stderr 输出协议。
- `Drop(StringOwner)` 在 capacity 为 0 时结束，在 capacity 大于 0 时把原 bytes pointer 精确交给
  ADR-0008 的集中 `free` adapter；verifier 继续阻止 move 后 drop、重复 drop 与 loan 存活期内
  提前 drop。

### 编译阶段与 runtime 边界

- frontend 只发布 builtin identity、decoded literal bytes、binary/call descriptor、ownership 与
  drop facts；不得发布 bytes pointer、capacity、allocator 或 LLVM struct。
- SSA operation/verifier 显式检查 String operand/result、loan mode、owner obligation 与 source
  order。LLVM adapter 不按 `String`、`println`、`error` 或运算符源码文本猜测行为。
- 不新增 workspace runtime crate。`lang-codegen` 复用现有 `malloc/free/write/abort` 声明边界，
  并生成 String 的单态 helper/drop glue；标准库源码不接触 raw pointer。
- future argv adapter 必须先验证每个宿主参数为合法 UTF-8，再创建 heap-backed `StringOwner`；
  验证失败作为 operational failure，不产生替换字符或部分 `Array<String>` owner。

## 替代方案

### 所有 literal 也复制到 heap

不采用。它能简化 drop 分支，但会让每个静态 literal 产生无语义必要的 allocation/copy。显式
capacity provenance 以一个 pointer-width 字段换取静态 storage 优化，并仍由统一 drop glue 验证。

### 只使用 `{ptr, length}` 并靠地址范围判断 provenance

不采用。地址范围不是 target-independent、不易由 verifier 证明，也会把 linker/loader 偶然布局
变成析构语义。显式 provenance 必须随值传递。

### 使用 NUL 终止的 C string

不采用。Koven String 允许 U+0000，连接和相等需要明确长度；扫描终止符既改变语义又引入重复
O(n) 成本，且不能表达 slice 的有效读范围。

### 复用 Rust `String` 或宿主 allocator 对象

不采用。编译器宿主布局、allocator 和 pointer width 不能成为目标程序 ABI，也无法跨目标生成
确定 LLVM IR。

### 默认使用 Rc/隐式共享 buffer

不采用。String 在候选 guide 中是 MoveOnly unique owner；隐式 retain 会违反 `Copyable` 与显式
共享成本原则，并把 cycle/thread 语义带入当前不需要的最小 runtime。

### 首版加入 small-string optimization

不采用。SSO 会增加 tagged representation、move/drop 分支和 target layout 状态，却不改变当前
语言可观察语义。后续只有在基准和兼容边界明确时才能通过新 ADR 评估。

## 后果

收益：

- plain literal、动态连接、参数/返回和容器元素使用同一 SSA/LLVM owner identity；
- 内嵌 NUL、目标位宽、静态 provenance 与 heap drop 均有明确可验证不变量；
- 不引入宿主对象、公开 FFI 承诺、隐式共享或新 workspace crate；
- SPEC-0192 完成后可为 SPEC-0194 的 UTF-8 argv owner 提供稳定内部创建边界。

代价与风险：

- 每个 String value 是三个 machine word，静态 literal 也携带 provenance 字段；
- concat 首版总是分配精确结果，没有 builder、capacity reuse 或 SSO；
- LLVM/helper 与 verifier 必须同时覆盖 static/heap/empty 三种存储状态；
- `String?` 不能复用 pointer-like null niche，仍需独立 inline-nullable ABI。

## 关联

- 首个实施 Spec：[SPEC-0192](../specs/0192-general-string-runtime.md)
- 解锁：SPEC-0194 参数化 main、后续一般 String/IO API
- 相关 ADR：[ADR-0006](./0006-typed-ssa-block-parameters.md)、
  [ADR-0008](./0008-internal-value-and-allocation-abi.md)、
  [ADR-0016](./0016-interprocedural-borrow-abi.md)
- 取代的 ADR：无
- 被以下 ADR 取代：无
