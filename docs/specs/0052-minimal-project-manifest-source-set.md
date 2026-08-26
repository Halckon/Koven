# SPEC-0052：最小 project manifest 与本地 source-set provider

## 1. 元数据

| 字段 | 值 |
|---|---|
| 状态 | `draft` |
| Goal ID | `KOV-P6-052` |
| 所属 Phase | Phase 6 |
| 语言规范 | 现行 v0.31；候选 v0.32 §32 |
| 批准依据 | 无；v0.32 尚未启用 |
| 前置 Spec | SPEC-0025 待完成 |
| 前置 ADR | ADR-0005 `accepted`；ADR-0020、[ADR-0022](../adr/0022-minimal-project-manifest-source-discovery.md) 待接受 |
| 阻塞项 | v0.32 启用；SPEC-0025 `done`；ADR-0020/0022 `accepted` |
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
- 不新增 workspace crate，不改变现有单文件 `kovenc build/run`。

## 5. 验收标准

- [ ] version 1 正例覆盖一个/多个 root、同 package 多 root、空 root、项目目录整体搬迁和乱序枚举；
  snapshot 的 root/source key 与排序保持一致。
- [ ] manifest 反例覆盖非法 TOML、schema/version/name、缺失/空/重复 roots、未知字段与未来
  dependency/target/entry 字段，均形成稳定 project operational error。
- [ ] filesystem 反例覆盖 root 缺失/非目录/不可读/逃逸/重叠、symlink、不合法 UTF-8 路径或源码、
  重复 physical source 和读取失败；frontend 不被调用且不产生 `Ldddd`。
- [ ] 只纳入普通 `.ko`，不跟随 symlink、不读取被忽略的其他扩展；source text 与 presentation
  path 正确保留。
- [ ] `lang-cli` 窄测试、workspace 五项基线和 Architecture 同步完成。

## 6. 技术方案与依赖边界

- 新增 `lang-cli/src/project/`，分离 manifest value validation 与 filesystem discovery；不创建抽象
  provider trait，输出直接适配 SPEC-0025 的中性 source input。
- TOML 候选使用 `toml = 1.1.4+spec-1.1.0`，关闭 default features，只启用 `std`、`parse`，通过
  `toml::Table` 手动验证严格 schema，不引入 serde/display/preserve-order。该版本上游声明 MSRV
  1.85、license `MIT OR Apache-2.0`，低于仓库 Rust 1.96；实施时仍须检查 crate/build script、
  传递依赖、`cargo tree`、lockfile 与许可证差异后才能正式准入。
- 不引入 `walkdir`：首版递归和 symlink policy 可由 `std::fs` 以小型、显式状态机完成。所有集合
  输出前排序，错误携带 manifest/source path 与具体类别，不压成无结构字符串。

上游核对：[toml 1.1.4 feature 列表](https://docs.rs/crate/toml/1.1.4+spec-1.1.0/features)、
[workspace MSRV/license](https://github.com/toml-rs/toml/blob/toml-v1.1.4/Cargo.toml)。

## 7. 实施计划

1. [ ] 完成依赖准入与严格 manifest model → 验证：schema/value 正反单测、`cargo tree`。
2. [ ] 实现安全 root/source discovery 与 stable snapshot → 验证：临时目录、顺序置换、IO/path 矩阵。
3. [ ] 接 SPEC-0025 source input adapter → 验证：snapshot identity/文本等价测试，不运行 frontend。
4. [ ] 同步 Architecture/Spec 验收并跑 workspace 五项基线。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | manifest parser、filesystem discovery、snapshot 与完成文档 | `feat(cli): load project source sets (SPEC-0052)` |

## 9. 未决问题

- 无；target/entry/dependency 与公开 CLI 明确不在本 Spec。状态门禁由 v0.32、SPEC-0025 与
  proposed ADR 表达。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-26 roadmap/provider 审计 | 通过 | 占位 Goal 已物化；因 v0.32/0025/ADR 未生效保持 draft |
| 2026-08-26 `toml` 候选审计 | 部分通过 | 版本/features/MSRV/license 已核对；build script、传递依赖与 lockfile 留到实施准入 |
