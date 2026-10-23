# SPEC-0352: CLI project source-set v1 导出

> **性质**：交付合同 · **状态**：done · **读取时机**：实现与验收编辑器项目输入前置时 · **唯一真源**：本次交付范围

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | KOV-P6-0352 |
| 所属 Phase | Phase 6 |
| 里程碑 | CORE-V1-M3 / 编辑器 SPEC-0007 生产方前置 |
| 语言规范 | Guide v0.45 |
| 前置 Spec | SPEC-0052、SPEC-0187 已交付 |
| 批准依据 | 用户明确授权补齐上游前置并完成两端 PR 闭环 |

## 1. 范围与行为

新增 `kovenc source-set --project <project.toml>`，复用现有 project loader，输出
[ADR-0021](../../adr/accepted/0021-lsp-explicit-source-set-protocol.md)的 source-set v1 对象，
供宿主原样放入 `initializationOptions.koven.sourceSet`；不另加 envelope 或修改 v1 字段。
roots 和 sources 的 identity、排序、完整 UTF-8 text 均由现有 loader 产生；URI 使用经过
percent encoding 的绝对 canonical file 路径，仅作展示定位，不进入 root/logicalPath identity。
支持当前 macOS/Linux 宿主；空 source set 合法。重复导出同一输入字节一致。
导出前复用 frontend 既有逻辑路径校验，并拒绝 v1 禁止的反斜杠；无效路径作为操作错误退出，不静默改写。
manifest 与 source discovery 规则继续由 ADR-0022 唯一规定；不扫描额外目录、不解析源码、
不选择 entry、不自动注入标准源码、不启动 LSP、不修改输入。

## 2. 错误与发布

参数只接受固定 `--project <path>`：用法错误退出2；loader、路径编码等操作错误退出1。
失败只写 stderr，stdout 为空；完整 snapshot 转换成功后才序列化发布，不输出部分成功。
`--message-format=json` 不把导出结果包装为语言诊断，操作错误仍不伪造 Ldddd。
语法错误的 UTF-8 源码也可导出；后续 LSP 负责诊断。每次执行重新读取磁盘，不能缓存旧 base。

## 3. 复用与依赖审阅

项目发现复用 `load_project_source_set`，JSON 使用既有 serde_json；标准库无 URI 编码器。
复用 Cargo.lock 已有的 fluent-uri 0.1.4 PathEncoder，将其作为 CLI 的直接依赖并在根集中固定；
不增加锁内包，不手写通用 URI 编码。MIT 许可，edition2021兼容本仓库 Rust1.96；
仅依赖已锁 bitflags1.3.2，无 build.rs、过程宏或原生下载，禁默认 features。
0.1.4 的编码接口由 `unstable` feature 导出；只启用该必要 feature，精确锁版并把调用局限在
URI 呈现适配函数，升级须复验编码矩阵，不向跨 crate API 泄漏该类型。
组件内部有 unsafe，但本片不调用 unsafe API；编码只接受已验证 UTF-8 的 canonical路径。
不升级到另一版本；审查 pinned 源码和许可不等于完整漏洞审计或跨平台兼容证明。

## 4. 验收账本

| 验收项 | 目标 | 实际结果 |
|---|---|---|
| 输出/顺序/Unicode URI/完整文本/空项目 | source_set_cli | 先 RED 4 例未知命令失败；实现后 4/4 通过，路径补充后 5/5 通过 |
| 无效输入无部分输出、磁盘增删改重新导出 | source_set_cli | 5/5 通过，含无效目录/反斜杠路径先 RED 后 GREEN |
| 原 project 命令与 CLI 单元测试 | project_cli、lang-cli bin | 串行 39/39、58/58 通过；首次并行 7 例 native 超时，未计通过 |
| fmt、CLI clippy、workspace check、CLI build | 对应 Cargo 门禁 | fmt check、CLI all-targets clippy -D warnings、workspace all-targets check、CLI/LSP build 通过 |
| 真实 CLI→LSP 初始化消费 | 本机真实 stdio 联合探针 | 编码 URI、跨文件诊断、dirty overlay、close 恢复、更新 base 后新会话重放均通过；消费者接入另验 |
| 文档/依赖/尺寸门禁、独立复核、双宿主 CI | CI 与脚本 | 本地门禁通过；独立复核发现路径不兼容已修复并复核；实现 head `a9154483` 的完整双宿主 CI [39929521401](https://github.com/Halckon/Koven/actions/runs/39929521401) 通过，含 preview producer/independent consumer |

## 5. 输入与交付

基于 main `55eb03809e930b25d1dc489404964db03314b081`，main CI 39925506271 成功。
保留原始 dc37 工作区，独立分支 `feature/spec-0352-source-set-export` 实施。
本片不启用 v0.46、不扩大语言能力；插件仓库继续单独维护 SPEC-0007 状态。
生产方验收、归档、最终 head CI、PR 合并及实际 main CI 均完成后才算闭环。

本机探针输入：CLI SHA-256 `267c8150343a3d790ce5f2f8f5f1710428533514a2481d44c3130aae423c6313`、
LSP `e77b514dca2543fe2ad5e20090e06983fb9524a081dbd292a8a9ffeff6ec284d`。
探针脚本 SHA-256 `2a9a04400cd7de43974fcdd65d0d089a466e3d2435e3a0c7817c09e77a0441b8`，
原始脚本/日志暂存本机 `/private/tmp`，非长期证据；长期行为证据以入库测试与 CI 为准。
尺寸门禁报告既有 38 份超限文件，无新增超限；不把旧欠账记为已清理。

frontend `compilation_unit_index` 14/14 通过。真实消费长期回归已提交至编辑器 [7573083](https://github.com/koven-lang/koven-editors/commit/7573083)，其 [CI 39932379501](https://github.com/koven-lang/koven-editors/actions/runs/39932379501) 的真实 CLI/LSP 任务通过；两 IDE 后续任务独立验收。
本次按实现 head 验收归档；归档 head 的 CI、PR138 合并及实际 main CI 继续作为交付门禁，不把待执行结果写成通过。
