# SPEC-0002: 建立统一 source 与 Span 基础设施

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P0-002` |
| 所属 Phase | Phase 0 |
| 语言规范 | [`agent-language-design-guide-v0.4.md`](../guides/legacy/agent-language-design-guide-v0.4.md) |
| 前置 Spec | SPEC-0001 `done` |
| 前置 ADR | [ADR-0004](../../adr/accepted/0004-source-span-position-model.md) 必须为 `accepted` |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend`、Architecture |
| 语言语义变更 | 否 |

## 1. Goal

完成后，所有后续前端阶段可通过同一 source map 和半开字节 `Span` 精确定位 UTF-8 源码。

## 2. 背景

Token、AST 和诊断都依赖稳定的源码身份与范围。如果各阶段各自计算行列或切片，Unicode、
CRLF 和多文件输入会产生不一致。本 Spec 在 lexer 之前建立唯一基础设施。

## 3. 范围与需求

- 定义不与文件系统路径等同的 map-local `SourceId`，以及自带 `SourceId` 和 `[start, end)`
  半开字节范围的 `Span`。
- source map 持有唯一、稳定的用户可见名称与不可变 UTF-8 源文本，并提供受检的源码注册和
  切片接口；重复名称返回具体错误。
- 集中建立行起始索引；把字节偏移转换为统一的 1-based 行列展示位置。列号按从行首到目标
  offset 之前的 Unicode scalar value 数量加一计算；tab 计一个 scalar，视觉宽度另行处理。
- 明确处理 `\n`、`\r\n`、空文件、多字节 Unicode、EOF 空范围与跨行范围。
- 无效 source ID、逆序范围或越界范围返回具体错误，不对用户输入 `panic!`。
- 数据结构不依赖 parser、类型系统或 LLVM。LSP 后续复用 source identity、字节范围和行索引，
  但其协议坐标由适配层转换。

## 4. 非目标

- 不实现文件发现、module / import、增量缓存或路径规范化策略。
- 不实现诊断数据模型、颜色渲染、lexer 或 AST 语义节点。
- 不把 Unicode 字素簇 / grapheme 宽度、终端 tab 展开或机器诊断协议纳入本 Spec。

## 5. 验收标准

- [x] 单测覆盖 ASCII、UTF-8 多字节字符、空文件、EOF 和多行范围。
- [x] `\n` 与 `\r\n` 的行定位均有明确、稳定的期望值。
- [x] 无效 source ID、`start > end`、非字符边界与越界范围返回错误而不 panic。
- [x] 同一 source map 中的重复用户可见名称被明确拒绝。
- [x] 同一 source 和 byte offset 每次得到相同行列结果。
- [x] `cargo test -p lang-frontend --test source_span`（或实施后记录的等价精确 test target）
      通过，且输出确认至少执行一个测试。
- [x] workspace fmt、check、Clippy 和 test 基线通过。
- [x] Architecture 记录 source map、`SourceId`、`Span` 的所有权与调用边界。

## 6. 技术方案与边界

具体模型遵循 ADR-0004。`Span` 使用字节偏移是因为 Rust 字符串切片、lexer 游标和 LLVM
调试位置最终都需要稳定 byte range；行列只在展示边界计算。内部范围使用半开区间，避免
空范围和相邻 token 的 off-by-one。`SourceId` 只在所属 source map 和源码快照生命周期内
稳定；确定性产物不得依赖其数值或加载顺序，输出排序使用唯一的用户可见 source name 与
范围。

## 7. 实施计划

1. [x] 实现 `SourceId`、`Span` 及受检构造不变量 → 验证：范围边界单测
2. [x] 实现 source map、行索引与 UTF-8 / CRLF 位置换算 → 验证：位置表驱动测试
3. [x] 收紧错误路径与公开 API 文档 → 验证：非法输入测试、Clippy
4. [x] 更新 Architecture 和 Spec 验收记录 → 验证：全 workspace 基线

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | source / `Span` 实现、测试与 Architecture | `feat(frontend): add source span foundation (SPEC-0002)` |

## 9. 未决问题

- 终端视觉列宽和机器可读位置协议后续由诊断相关 Spec / ADR 决定；本 Spec 只固定内部字节
  范围，以及按 Unicode scalar value 计数的基础 1-based 展示位置。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo test -p lang-frontend --test source_span --locked --offline` | 通过 | 9 passed；0 failed / ignored / measured / filtered out |
| `cargo fmt --all -- --check` | 通过 | 无格式差异 |
| `cargo check --workspace --all-targets` | 通过 | 五个 member 的全部 target 检查成功 |
| `cargo clippy --workspace --all-targets -- -D warnings` | 通过 | 无 warning |
| `cargo test --workspace --all-targets` | 通过 | 共 10 passed；0 failed / ignored / measured / filtered out |
| `cargo build -p lang-cli` | 通过 | `kovenc` dev profile 构建成功 |
