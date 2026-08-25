# SPEC-0190：公开单文件 `kovenc build/run`

## 1. 元数据

| 字段 | 值 |
|---|---|
| 状态 | `done` |
| Goal ID | `KOV-P6-190` |
| 所属 Phase | Phase 6 |
| 批准依据 | 当前持续 Goal“继续推进 guide 主线，分阶段实施 specs，尝试验证 hello world 程序”的站立授权 |
| 前置 Spec | SPEC-0039、0042、0043、0184、0189 `done` |
| 前置 ADR | ADR-0010、ADR-0012 `accepted` |
| 阻塞项 | 无；单文件显式 entry pipeline、linker 与 stdout 已完整实现 |
| 影响范围 | `lang-cli` 参数/构建编排、CLI 集成测试、Architecture/Roadmap |
| 语言语义变更 | 否 |

## 2. Goal

公开两条最小、确定的单文件命令：

```text
kovenc build <source.ko> --entry <name> -o <executable>
kovenc run <source.ko> --entry <name>
```

两者复用现有 frontend→verified SSA→LLVM object→Clang link 主线；`run` 还要精确转发程序
stdout/stderr 与退出状态。仓库外 Hello World 必须只通过公开 `kovenc` 进程完成。

## 3. 范围

- 参数顺序固定；未知/缺失/重复选项返回 usage exit 2，不猜测默认 entry 或输出名。
- `build` 拒绝已存在 output，使用调用方输出目录中的唯一临时 object，成功或失败都清理该 object。
- `run` 使用进程拥有的唯一临时目录，完成后清理 object/executable；程序 stdout/stderr 原样返回。
- frontend diagnostics 继续服从全局 `--message-format=human|json`，内部错误与 IO/link/launch 失败
  输出单条稳定 CLI error，不泄漏 panic。
- build 成功退出 0 且 stdout/stderr 为空；run 返回子程序退出码（无法表示时返回 1）。

## 4. 非目标

- 不实现隐式 `main`、默认输出、任意参数排列、多 source、package/import、manifest、增量构建、
  cache、cross target、程序参数或环境配置。
- 不改变 linker、entry shape、语言诊断 schema 或标准库语义。

## 5. 验收标准

- [x] 参数/IO/output-exists/entry/frontend/link/run 错误边界均有测试。
- [x] human 与 JSON Lines frontend diagnostics 不混入成功 stdout。
- [x] 公开 `build` 生成可执行文件，外部启动后 stdout 精确为 `Hello, World!\n`。
- [x] 公开 `run` 对同一外部源码直接返回精确 stdout、空 stderr 与 exit 0；临时产物清理。
- [x] Architecture/Roadmap/Spec、workspace 五项基线与独立 SPEC-0190 提交完成。

## 6. 实施计划

1. [x] 从 repository bootstrap 提取可复用的 build-only 边界与可渲染 diagnostics。
2. [x] 实现固定参数的 build/run command 与临时产物生命周期。
3. [x] 增加真实 `kovenc` 进程正反验收并同步事实文档。
4. [x] 运行基线、审查并提交。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | build/run 参数、pipeline、外部 Hello World 与文档 | `feat(cli): add single-file build and run (SPEC-0190)` |

## 8. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-25 前置审计 | 通过 | SPEC-0189 已以 `5009a67` 提交；公开 CLI 是剩余 Hello World 门禁 |
| `cargo test -p lang-cli --test native_cli` | 通过 | 公开 build/run 外部 Hello World 与正反 CLI 矩阵 |
| `cargo fmt --all -- --check` | 通过 | 全 workspace 格式基线 |
| `cargo check --workspace` | 通过 | 五个 member 均通过 |
| `cargo clippy --workspace --all-targets -- -D warnings` | 通过 | 无 warning |
| `cargo test --workspace` | 通过 | 全量通过；1 项既有 LLDB task-port 权限测试按设计 ignored |
| `cargo build -p lang-cli` | 通过 | 公共 `kovenc` debug target 构建成功 |
