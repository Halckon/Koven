# SPEC-0052：最小 project manifest 与本地 source-set provider

## 1. 元数据

| 字段 | 值 |
|---|---|
| 状态 | `done` |
| Goal ID | `KOV-P6-052` |
| 所属 Phase | Phase 6 |
| 语言规范 | 现行 v0.32 §32 |
| 批准依据 | 2026-08-27 当前持续 Goal 授权先审计 roadmap、再按依赖图推进；复核确认只依赖 SPEC-0025 Stage 1 已完成的 source-unit input contract |
| 前置 Spec | SPEC-0025 Stage 1 `done`（提交 `357049f`、`a030c09`）；Stage 2 import/visibility 与本 provider 无关 |
| 前置 ADR | ADR-0005、ADR-0020、[ADR-0022](../../adr/accepted/0022-minimal-project-manifest-source-discovery.md) `accepted` |
| 阻塞项 | 无 |
| 影响范围 | `lang-cli` project manifest/source discovery、workspace dependency、tests；Architecture/Roadmap |
| 语言语义变更 | 否 |

## 2. Goal

完成后，`lang-cli` 能从调用方显式提供的 version 1 `project.toml` 安全加载一个本地项目，生成
与 SPEC-0025 compilation-unit 输入兼容、顺序确定的 immutable base source-set snapshot；不执行
frontend 分析、依赖解析或构建。

## 3. 范围与需求

- 实施 ADR-0022 的严格 schema/version/project/source-roots 解析，拒绝未知字段。
- 显式读取 manifest-relative roots，安全递归发现普通 `.ko`，不跟随 symlink；把 IO、路径、
  UTF-8、overlap/duplicate 错误表示为具体 project operational error。
- root identity 与 logical path 精确遵守 ADR-0005；先完整验证/读取，再按稳定 source key 排序并
  发布 snapshot，不让 filesystem 枚举顺序或绝对目录进入 identity。
- 保留 source text 与 presentation path，但不构造新的 package/DeclarationId，不解析 entry。
- provider API 接受显式 manifest path，不查询 cwd 或祖先；首版作为 `lang-cli` 内部可测试模块，
  公开 project build/check 命令留给 SPEC-0054。

## 4. 非目标

- 不做 Koven lex/parse/name/type/ownership、package directive 诊断、entry 选择、SSA/LLVM/link/run。
- 不实现 dependency/registry/download/lock、target、workspace、ignore/glob、外部/generated root、
  symlink source、watch、缓存或 LSP overlay。
- 不提供对加载期间并发重命名、替换或改写项目树的事务性 filesystem snapshot；调用方须保证
  单次加载期间 manifest/source tree 静止，静态树的 symlink/no-escape 检查仍是强制契约。
- 不新增 workspace crate，不改变现有单文件 `kovenc build/run`。

## 5. 验收标准

- [x] version 1 正例覆盖一个/多个 root、同 package 多 root、空 root、项目目录整体搬迁和乱序枚举；
  snapshot 的 root/source key 与排序保持一致。
- [x] manifest 反例覆盖非法 TOML、schema/version/name、缺失/空/重复 roots、未知字段与未来
  dependency/target/entry 字段，均形成稳定 project operational error。
- [x] filesystem 反例覆盖 root 缺失/非目录/不可读/逃逸/重叠、root symlink 拒绝、root 内
  symlink 忽略、不合法 UTF-8 路径或源码、重复 physical source 和读取失败；
  frontend 不被调用且不产生 `Ldddd`。
- [x] 只纳入普通 `.ko`，不跟随 symlink、不读取被忽略的其他扩展；source text 与 presentation
  path 正确保留。
- [x] `lang-cli` 窄测试、workspace 五项基线和 Architecture 同步完成。

## 6. 技术方案与依赖边界

- 新增 `lang-cli/src/project/`，分离 manifest value validation 与 filesystem discovery；不创建抽象
  provider trait，输出直接适配 SPEC-0025 的中性 source input。
- TOML 实现使用 `toml = 1.1.4+spec-1.1.0`，关闭 default features，只启用 `std`、`parse`、
  `serde`，通过 `toml::Table` 手动验证严格 schema；`serde` 是该版本公开 `Table`/`Value` 所需
  feature，不引入 derive，也不启用 display/preserve-order。该版本上游声明 MSRV 1.85、license
  `MIT OR Apache-2.0`，低于仓库 Rust 1.96；实施时仍须检查 crate/build script、传递依赖、
  `cargo tree`、lockfile 与许可证差异后才能正式准入。
- 不引入 `walkdir`：首版递归和 symlink policy 可由 `std::fs` 以小型、显式状态机完成。所有集合
  输出前排序，错误携带 manifest/source path 与具体类别，不压成无结构字符串。
- Unix 使用 `(device, inode)`，Windows 使用 `(volume serial, file index)` 作为 physical-file
  identity；宿主无法提供可靠 identity 时 fail closed，不以 canonical path 冒充 hard-link identity。

上游核对：[toml 1.1.4 feature 列表](https://docs.rs/crate/toml/1.1.4+spec-1.1.0/features)、
[workspace MSRV/license](https://github.com/toml-rs/toml/blob/toml-v1.1.4/Cargo.toml)。

## 7. 实施计划

1. [x] 完成依赖准入与严格 manifest model → 验证：schema/value 正反单测、`cargo tree`。
2. [x] 实现安全 root/source discovery 与 stable snapshot → 验证：临时目录、顺序置换、IO/path 矩阵。
3. [x] 发布可直接交给后继 driver 的 root identity/logical path/text/presentation input → 验证：
   与 SPEC-0025 Stage 1 字段逐项等价，不在本 provider 内运行 frontend。
4. [x] 同步 Architecture/Spec 验收并跑 workspace 五项基线。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | manifest parser、filesystem discovery、snapshot 与完成文档 | `feat(cli): load project source sets (SPEC-0052)` |

## 9. 未决问题

- 无；target/entry/dependency 与公开 CLI 明确不在本 Spec。SPEC-0025 Stage 2 的 exact-import/
  visibility 只影响后继 frontend analysis，不影响 filesystem provider。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-26 roadmap/provider 审计 | 通过 | v0.32/ADR-0020/ADR-0022 已生效；仍因 SPEC-0025 未完成保持 draft |
| 2026-08-26 `toml` 候选审计 | 部分通过 | 版本/features/MSRV/license 已核对；build script、传递依赖与 lockfile 留到实施准入 |
| 2026-08-27 前置重审 | 通过 | SPEC-0025 Stage 1 已发布所需 root/logical/source input；Stage 2 不被本 Spec 消费，移除过度门禁并依据站立授权进入 `in-progress` |
| `cargo tree -p lang-cli --locked --offline -e features` | 通过 | `toml` 仅启用 `parse`、`serde`、`std`；未启用 display/preserve-order |
| dependency metadata/source/OSV 审计 | 通过 | 新 lock 节点 license 为 MIT/Apache-compatible、MSRV ≤ 1.85、无 build script；crate 源码 unsafe 边界已检查；OSV exact-version query 无记录；本机未安装 `cargo-audit`/`cargo-deny` |
| `cargo test -p lang-cli --locked --offline project::tests` | 通过 | macOS 执行 16 个 project tests；非 macOS Unix 的真实非 UTF-8 traversal 用例因平台 cfg 未执行，纯转换边界已执行 |
| 独立实现复核 | 通过 | root/error 顺序、Windows segment prefix、logical overlap、physical identity fail-closed 与静态 symlink 契约通过；并发 mutation 明确为首版 operational assumption |
| `cargo check --workspace --all-targets --locked --offline` | 通过 | 初次发现损坏的 `lang-codegen` check cache，定向 `cargo clean -p lang-codegen` 后重建并最终退出 0 |
| `cargo clippy --workspace --all-targets --locked --offline -- -D warnings` | 通过 | workspace 全 target 无 warning |
| `cargo test --workspace --all-targets --locked --offline` | 通过 | 全部执行成功；1 个既有 LLDB task-port 权限测试 ignored |
| `cargo build -p lang-cli --locked --offline` | 通过 | `kovenc` build 基线退出 0 |
| `cargo fmt --all -- --check` | 通过 | Rust 格式检查退出 0 |
| `git diff --check` | 通过 | 无空白错误；用户未跟踪的 `hello` / `hello.ko` 不在本 Goal diff |
