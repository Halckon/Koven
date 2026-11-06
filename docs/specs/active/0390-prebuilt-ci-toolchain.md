# SPEC-0390: 预置 Linux CI 工具链镜像

> **性质**：CI 基础设施变更合同 · **状态**：in-progress · **读取时机**：构建、发布或切换核心 CI 镜像时 · **唯一真源**：本 Spec

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-CI-0390` |
| 所属 Phase | 工程基础设施；不改变语言 Phase |
| 语言规范 | [Guide v0.47](../../guide/README.md) |
| 影响范围 | Linux 核心 CI 工具链供应；macOS 与 Preview 独立验收保持 |
| 语言语义变更 | 否 |

## 1. 目标与授权边界

基于 main `3f88b1aa6e7365480d4ed1cafcb1e8f2e00da772`，回应 PR #322 的 Ubuntu
LLVM 包下载超时。用户批准先做独立 PR、验证后再切 CI。预置 Ubuntu 26.04 x86_64、
Rust 1.96.0、LLVM 21.1.8；减少每个核心 job 重复安装，不降低任何门禁。
镜像仍需首次下载和构建；不保证消除所有网络故障或给出未经测量的节省比例。

第一阶段 #324 已增加镜像输入、只读 PR 验收及可信 main 发布流程，并经用户明确批准
正常合入。用户随后选择直接在 PR #322 中迁移；第二阶段基于 main `c80e9e38`，
仅切换 Linux Clippy/Targeted Tests，使用已
发布的真实完整 index digest，不改保护规则、不让 PR 获得发布凭据。

## 2. 镜像输入与校验

- Ubuntu 官方 linux/amd64 manifest 固定 digest，不使用可漂移的 tag 作为内容身份。
- 构建复用 `install_ci_llvm_ubuntu26.sh`，保留签名源、七个直接包的精确版本与
  amd64/amd64v3 SHA256 白名单、拒绝降级及 sanitizer archive 校验。
- Rust 五个组件来自官方 2026-05-28 发布，各自固定 SHA256，校验后才解包/执行安装。
  工具放 `/opt/rust`，不依赖 Actions 会替换的 HOME；不内置 Cargo 缓存或仓库代码。
- 构建 context 仅 Dockerfile 和三个公共脚本。记录安装包清单与脚本哈希；其余系统依赖
  由签名 APT 验证完整性，最终闭包由镜像 digest 固定，不宣称所有依赖都逐项锁版或重建字节相同。
- 离线 readiness 严格查版本、LLVM backend、LLDB SB API、系统 C driver 与 ASan/LSan
  archive。readiness 不自动下载或修复环境，不能替代真实 native 验收。

## 3. 验收、发布与切换顺序

1. PR 的独立 workflow 仅有 contents read，构建并以精确 image ID 运行原 Linux
   workspace check/Clippy、完整 bounded composition、词频、conformance、sanitizer
   和 generated-owner 门禁。Clippy 与测试 target 分开，测试保持 debug=1、incremental=0。
   默认容器安全设置不放宽；编译前先做真实 C/LLDB launch 预检。Linux 调试启动
   显式保留 ASLR，不请求容器禁止的 personality 变更；源码断点、文件/行号/函数断言
   全部保留，macOS 启动参数不变。其他阻断仍报失败，不跳过测试。
2. 原 CI 的 macOS/Linux 和 Preview24 producer/独立 consumer 保持；镜像相关文件变化
   触发现有门禁。PR 镜像构建成功不等于完整双平台或 Preview 验收成功。
3. 第一阶段经审核合入后，仅主仓库 main 的手动 dispatch 可发布。构建/验收 job 无
   packages write；单独发布 job 不 checkout，只加载同次验证成功的 artifact/image ID。
   先核该精确源码 main push CI 的双宿主与 Preview 实跑成功，再给同一镜像打 tag、push。
4. 发布后按 digest 拉取，要求 image ID 不变并再次离线 readiness；核匿名读取，供 fork PR
   使用。若 GHCR 默认私有导致匿名读取失败，发布已发生但未获消费资格，应明确报告。
   不自动修改包访问权限。标签只供追溯，后续 job 必须写真实 digest。
5. 第二阶段在 PR #322 中提交核心 Linux container 切换，显式 bash、digest 隔离缓存，保持原 job 名、
   条件、所有命令与汇总契约。实际验证 Actions HOME、checkout、cache restore/save、artifact、
   LLDB 源断点、sanitizer。Linux 使用 `--user 1001:1001` 保持真实权限错误测试；
   checkout 前核 UID、HOME/workspace/temp/environment-file 可写，失败即停止，不能
   chmod/chown 补救。工具仍位于 `/opt/rust/bin`，CARGO_HOME 明确使用 Actions HOME。
   仅 Linux cache key 追加完整 image digest 与 verifier hash；macOS 原 key 与安装保持。
   macOS 与 Preview 不切镜像；未完成此项前不关闭本 Spec。

## 4. 单一验收账本

| 验收项 | 命令/来源 | 实际结果 |
|---|---|---|
| 云端受影响源码编译 | Rust 1.96.0/LLVM21.1.8 `cargo check --locked --workspace --all-targets` | 通过；Debian13，不是 Ubuntu 容器验收 |
| 严格 Clippy 与实际测试目标编译链接 | workspace Clippy；codegen/CLI/LSP/std `cargo test --no-run` | 通过；默认 debug=2 完成实际编译链接，未执行 Rust 测试 |
| 失败关闭与接线合同 | `python3 -m unittest scripts.tests.test_ci_toolchain_image -v` | 8 项通过，含错误 Rust payload、非精确 main CI 拒绝及保留 ASLR/必须命中断点；C probe 真实编译，LLDB 用 stub，不冒充容器运行 |
| 全部脚本合同、文档、语法、尺寸 | scripts unittest、docs checker、bash/YAML、size、diff | 脚本合同 504 项通过；839 Markdown、语法、尺寸与 diff 通过，39 项历史尺寸债务无新增 |
| PR 镜像实际构建与全部 Linux gate | head `e7e8ab02610cc6ca8d65cb4faac5e5f8487a40fa` 的 [run 42148114344](https://github.com/Halckon/Koven/actions/runs/42148114344/job/127664415700) | 全部通过：真实 C/LLDB 与源码断点、2623 codegen、56 native_cli、40 词频命令、ASan/LSan 正反例、16 generated-owner。Conformance 无 regression/wrong_output/internal_error，保留 22 项 known_unimplemented；不代表完整语言实现。初始 run 42146554505 的 personality 失败已修复，未放宽容器安全设置 |
| Linux LLDB 启动回归与受影响目标编译 | `cargo test --offline --locked -p lang-cli --test native_cli final_debug_artifacts::linux_source_debugger_preserves_aslr -- --exact`；CLI Clippy/build | 新增回归修复前失败、修复后 1 项通过；native_cli 实际编译链接、严格 CLI Clippy 和 CLI build 通过。真实源码断点依赖 CI，本地无 LLDB |
| 原双宿主和两套 Preview | head `e7e8ab0` 的 [run 42148114385](https://github.com/Halckon/Koven/actions/runs/42148114385) | Linux/macOS Clippy 与 Targeted Tests、两个 producer 和两个独立 consumer、CI Passed 全部通过。#324 正常合入 main `c80e9e38a126623af70aebbd9a6ff83c8063ceb0`，tree 与验收头完全一致 |
| 可信 main 发布、真实 digest、匿名读取 | main `c80e9e38` 的 [CI](https://github.com/Halckon/Koven/actions/runs/42153007409) 与 [发布 job](https://github.com/Halckon/Koven/actions/runs/42153917943/job/127696200046) | 双平台与 Preview 全部通过；同一验收 artifact 的 image ID 经 save/load、push/pull 保持不变，发布 index digest 为下列消费身份；离线 readiness 和匿名检查通过。第一次因 main 未完成而关闭，第二次 GHCR denied 留下平台 manifest；未改权限/内容的一次有界重试完成完整 index，原 denied 根因未证实 |
| 核心 job 切换、真实 Actions container/cache 验收 | PR #322 的直接迁移组合 | 已实现固定 digest、非 root、离线校验及 Linux cache 隔离；新合同修复前失败、修复后通过。真实 Actions container/缓存/上传及双平台与 Preview CI 待候选精确头，不能宣称切换完成 |

已发布消费身份：
`ghcr.io/halckon/koven-ci-toolchain@sha256:b5ce8a4479579ecc57194e385f13109a52022114f0e19cab93fc426bdbb9500e`。
它是含平台镜像及证明材料的完整 index，不以已上传的子 manifest 替代。
