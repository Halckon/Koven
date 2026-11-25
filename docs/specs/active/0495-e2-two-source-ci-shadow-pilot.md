# SPEC-0495: E2 两源 Unit CI 门禁的可撤回影子试点与双宿主 A/B 对账

> **性质**：CI 基础设施变更合同 · **状态**：in-progress · **读取时机**：实施、运行、核验或撤回 E2 两源 Unit 影子提速试点时 · **唯一真源**：本 Spec

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-CI-0495` |
| 所属 Phase | 工程与 CI 基础设施；不改变语言 Phase |
| 语言规范 | 现行 [Guide v0.47](../../guide/README.md) |
| 批准依据 | 2026-11-24 用户下发五点收口实施建议（首批 `scripts/check_e2_two_source_unit_ci.sh` 冷/热闭环后，授权推进第二批 `scripts/check_e2_replay_ci.sh` 2 配置 / 28 次调用独立锁定产物试点） |
| 前置 Spec | 无 |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 首批 `e2-two-source-unit` 与第二批 `e2-replay` 远端双宿主冷/热 A/B 实跑证据均已全绿闭环；待用户确认是否正式切换现网步骤或归档试点 |
| 影响范围 | `.github/workflows/ci-e2-shadow-pilot.yml`、`scripts/e2_two_source_unit_manifest.json`、`scripts/e2_replay_manifest.json`、`scripts/e2_two_source_shadow_pilot.py`、`scripts/tests/test_e2_two_source_shadow_pilot.py`、`docs/development/testing.md` |
| 语言语义变更 | 否 |

## 1. Goal

在不改动正式 `.github/workflows/ci.yml` 门禁以及 `scripts/check_e2_two_source_unit_ci.sh`、`scripts/check_e2_replay_ci.sh` 现网执行路径的前提下，按两批递进为 E2 两套 CI 专属测试脚本（首批 `check_e2_two_source_unit_ci.sh`：5 配置 / 179 条目 / 原 358 次 Cargo 调用；第二批 `check_e2_replay_ci.sh`：2 配置 / 14 条目 / 原 28 次 Cargo 调用）交付**独立可一键关停的非门禁影子试点工作流、分套件共享清单、跨配置防混用唯一产物锁与真实 Cargo 启动环境探针、防漏跑硬校验执行器及双宿主隔离冷/热 A/B 对账工具**。

## 2. 背景

`scripts/check_e2_two_source_unit_ci.sh` 当前按 5 个构建配置遍历 179 个测试条目，每条分别执行一次 `--list` 和一次运行，共发起 358 次 `cargo test --locked -j1` 子进程调用（并非 358 次重编译）。直接修改现网门禁存在漏跑测试、拿错异构 feature 产物、丢失 Cargo 注入运行环境（工作目录、`CARGO_*` 变量、动态库搜索路径与实际 LLVM/clang）或引发额外 OOM/磁盘超限的风险。因此首批仅在独立分支与非门禁工作流上建立可复查的双宿主 A/B 影子试点。

## 3. 范围与需求

1. **有界影子范围与执行策略**：
   - 仅覆盖 `scripts/check_e2_two_source_unit_ci.sh` 的 5 个构建配置与 179 个执行条目；保留 `-j1` 编译并发限制；不并行预热 5 套配置，不跨配置混用产物。
   - 候选臂对每个配置执行一次 `cargo test --locked -j1 ... --no-run --message-format=json` 构建，随后按原顺序和原过滤器（`--exact` / `--ignored`）逐条启动独立测试二进制进程（先 `--list`、再运行）。
   - 现行 `Targeted Tests` 原脚本继续照常运行，影子试点结果不替代任何正式门禁。
2. **唯一产物锁定与真实 Cargo 启动探针**：
   - 解析 Cargo JSON 流，按宿主/目标三元组、`package_id`、`target.name` 与 `target.kind`、激活 `features`、`profile.test`、构建环境及 flags 精确匹配；仅接受唯一且 `executable` 非空并实际存在的匹配，0 个或多个均立即判失败。
   - 通过真实 Cargo 启动探针（`target.<triple>.runner`）采集 Cargo 实际拉起测试进程时的工作目录、白名单环境变量（含相关 `CARGO_*` 与工具链变量）、动态库搜索路径（`LD_LIBRARY_PATH` / `DYLD_*` 及动态链接解析结果）与实际 `llvm-config`/`clang` 身份，并与直调启动探针逐项比对一致后才允许执行候选测试；严禁打印全量环境或任何凭据秘密。
   - CI 专属故障用例仅在真实 GitHub Actions（`CI=true` 且 `GITHUB_ACTIONS=true`）执行；本地仅允许 `--list-only`，严禁伪造 `CI` 或 `GITHUB_ACTIONS`。
3. **防漏跑与预期失败硬校验**：
   - 同一份结构化清单（`scripts/e2_two_source_unit_manifest.json`）同时驱动影子基线臂（A）与候选臂（B），并强制校验与 `scripts/check_e2_two_source_unit_ci.sh` 的配置、顺序、完整测试名及 `--exact`/`--ignored` 参数逐项等价。
   - 清单支持正常通过与预期失败两类条目；每条必须先验证 `--list` 恰好命中 1 条目标测试，再验证运行阶段实际执行恰好 1 次（`running 1 test` 且 `passed + failed == 1`、`ignored == 0`），绝不因进程退出码为 0 放过 `0 passed` 漏跑。
   - A/B 对账逐条比较退出状态、成功/失败分类及预期失败证据子串，不把预期失败当作跳过；清单漂移、环境不等价、漏跑或错产物一律判影子失败，并在报告中保留 `(host, config_id, test_name)` 定位证据。
4. **双宿主隔离冷/热 A/B 与遥测记录**：
   - 在 `ubuntu-26.04`（使用与核心门禁一致的固定工具链镜像 digest 与 `--user 1001:1001`、`debug=1`）和 `macos-15`（`setup-llvm`、`Rust 1.96.0`、`debug=2`）对同一源码 SHA 进行 A/B 对照。
   - A（`baseline`）与 B（`candidate`）使用各自独立的 `CARGO_TARGET_DIR` 与精确匹配缓存键（无 `restore-keys` 前缀回退），禁止恢复现网 `test` job 缓存，禁止 A/B 互相恢复；冷跑从空 target 起并断言 cache miss，热跑断言本臂精确键命中。
   - 分平台记录：原 `ci.yml` E2 步骤参考耗时与整 job 排队/墙钟（标明预热共享 target 仅供参考，不与隔离冷跑直接相减）、影子构建/探针/列举/运行分阶段耗时、排队与整 job 墙钟、子进程峰值 RSS、磁盘占用及 OOM/重试计数。
5. **一键关停机制**：
   - 独立工作流 `.github/workflows/ci-e2-shadow-pilot.yml` 支持仓库变量 `KOVEN_E2_SHADOW_PILOT_DISABLED=true` 零提交一键停用、`workflow_dispatch` 显式开关及单文件回滚。

## 4. 非目标

- 不修改 `.github/workflows/ci.yml`、不修改 `scripts/check_e2_two_source_unit_ci.sh` 与 `scripts/check_e2_replay_ci.sh` 现网执行逻辑，不替代任何 required CI 门禁。
- 不动 Clippy、正式缓存与并发度、Preview 独立 consumer、`check_generated_owners.py` G4/G5、D10（`check_d10_catalog_identity_ci.sh` 等）/E2/E3 编译器代码、Parser Lean/Kani 与差分测试及内核原型入口。
- 不以 `20–25 分钟` 作为首批验收线，不将单次运行耗时波动宣称为稳定提速结论；P1/P2/P3 重构另案推进。

## 5. 验收标准

- [x] 首批共享清单 `scripts/e2_two_source_unit_manifest.json`（5 配置 / 179 条目）与第二批清单 `scripts/e2_replay_manifest.json`（2 配置 / 14 条目）分别与对应 shell 脚本通过自动等价核验
- [x] 唯一产物锁定、跨配置异构 feature 二进制防混用锁、跨套件对账拒绝、真实 Cargo 启动环境探针、秘密过滤、防漏跑（0 命中 / 多命中 / 退出 0 但未执行）、预期失败分类与证据核验均具备红绿单测覆盖
- [x] 本机真实工具链下完成两套套件的 `baseline` 与 `candidate` 双模式 `--list-only` 实跑及 `compare` 对账（未伪造 `CI`/`GITHUB_ACTIONS`）
- [x] 首批 `e2-two-source-unit` 远端双宿主（`ubuntu-26.04` 容器与 `macos-15`）完成 `-j1` 与 `default` 多核两轮隔离冷跑（cache miss）与热跑（exact cache hit）A/B 实跑及遥测记录对账
- [x] 第二批 `e2-replay` 远端双宿主完成独立 target / 独立缓存键 / 独立产物锁的冷跑与热跑 A/B 实跑对账
- [x] Architecture 确认无需更新（不涉及编译器阶段或语言实现变化）；`docs/development/testing.md` 同步分套件影子试点边界说明
- [x] 表面语法增量门禁：`N/A`（纯 CI 影子基础设施，无表面语法变更）

## 6. 技术方案与边界

- **清单层**：`scripts/e2_two_source_unit_manifest.json`（`suite_id=e2-two-source-unit`，5 配置 / 179 条目）与 `scripts/e2_replay_manifest.json`（`suite_id=e2-replay`，2 配置 / 14 条目）各自独立声明配置及用例元数据。
- **执行与探针层**：`scripts/e2_two_source_shadow_pilot.py` 提供 `verify-manifest`、`run --manifest <path> --mode {baseline,candidate} --candidate-jobs {manifest,default,<N>}`、`compare` 与 `collect-workflow-metrics` 子命令，内置跨配置同 target 异构 feature 可执行文件路径与 SHA256 互异强校验、跨套件账本拒绝校验、Cargo `target.<triple>.runner` 探针模式及宿主机 CPU/RAM 配额采样。
- **工作流层**：`.github/workflows/ci-e2-shadow-pilot.yml` 按 `(suite, os, arm)` 物理隔离 `CARGO_TARGET_DIR`（`target-e2-shadow-<suite>-<arm>`）与 `actions/cache` 精确缓存键（`e2-shadow-pilot-v2-suite-<suite>-...`），支持 `workflow_dispatch` 选择套件及一键停用。

## 7. 实施计划

1. [x] 建立隔离工作树与 `SPEC-0495` 合同登记 → 验证：`python3 scripts/check_docs.py`
2. [x] 生成首批 179 条共享清单并编写先行失败单测 `scripts/tests/test_e2_two_source_shadow_pilot.py` → 验证：单测按预期原因失败
3. [x] 实现 `scripts/e2_two_source_shadow_pilot.py` 与 `.github/workflows/ci-e2-shadow-pilot.yml` → 验证：单测与本地门禁全绿
4. [x] 完成首批 `e2-two-source-unit` 远端双宿主 `-j1` 冷/热与 `default` 多核冷/热共 4 组 8 轮 A/B 实测闭环 → 验证：全部 `179/179 PASS`、`0` OOM
5. [x] 生成第二批 `scripts/e2_replay_manifest.json`（2 配置 / 14 条目），补齐跨配置独立产物防混用锁单测（先红后绿）与本机真实工具链 `--list-only` 双臂 A/B 实跑 → 验证：12 组单测全绿、本机 `14/14 PASS`
6. [x] 推送触发远端双宿主第二批 `e2-replay` 隔离冷/热 A/B 实跑并记录账本 → 验证：远端双宿主冷/热均 `14/14 PASS`、`0` OOM

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | 共享清单、影子试点执行/探针/对账器、非门禁 A/B 工作流、单测与 `SPEC-0495` 登记 | `ci(e2): add reversible two-source shadow pilot and A/B harness (SPEC-0495)` |

## 9. 未决问题

- 无

## 10. 验证记录

| 验收项 / 命令（目标与过滤器） | 结果（实际测试数） | 未运行原因 / 复用证据 |
|---|---|---|
| `python3 scripts/e2_two_source_shadow_pilot.py verify-manifest` | 通过：精确校验 5 个配置、179 个有序条目与 `scripts/check_e2_two_source_unit_ci.sh`（SHA256 `a5e0c95e0e5e5173...`）逐项一致 | 无伪造 `CI` / `GITHUB_ACTIONS` |
| `python3 -m unittest discover -s scripts/tests -p test_e2_two_source_shadow_pilot.py -v` | 通过：20 组测试全绿（含第二批 `e2_replay_manifest.json` 14 项脚本对齐、异构 feature 可执行文件复用拒绝、跨套件对账拒绝，以及 PR #463 审阅 8 项 P2 回归契约：执行分支漂移拦截、比较作业跨 attempt 重试解析、仅恢复 registry 拒标 warm、冷热混合对账拒绝、参考 CI 运行工作流/事件/merge-SHA 核验、失败重跑旧账本清理、Mach-O/ELF CPU 架构与可执行类型强校验、macOS `dyld` 实际加载 `libLLVM` UUID+SHA256 探针） | 先红后绿（TDD） |
| 本机真实工具链隔离 target `--list-only` A/B 实跑（macOS arm64，Rust 1.96.0，LLVM 21.1.8，`cfg2-codegen-test-two-source-unit` 7 项 + `cfg5-frontend-lib-e2-support` 3 项） | 通过：10 项 `compare` 等价 `PASS`；候选臂真实锁定 Mach-O 产物（`389f877562f1c5ee`、`5fc921c2b27db8c2`）并通过 Cargo 启动探针核验 `cwd`、`CARGO_*`、`DYLD_FALLBACK_LIBRARY_PATH` 与 `libLLVM.dylib 21.1.8`；10 次直调 `--list` 共 0.07s，峰值子进程 RSS 3433.3 / 3435.4 MiB，target 1.61 GiB，OOM/重试 0 | 临时目录 `/tmp/koven-spec-0495-local-ab` 验证后已清理，不入库 |
| 现有工作流与门禁策略单测（`test_llvm_install_bounds`、`test_ubuntu26_ci`、`test_pr_ci_reuse`、`test_check_ci_results`、`test_ci_debug_profile`） | 通过：96 项通过，确认正式 `ci.yml` 合同零受影响 | `test_ci_toolchain_image` 中 7/8 通过，1 项因本机 macOS 缺 GNU `sha256sum` 失败（与本分支无关，Ubuntu runner 原生提供） |
| `python3 scripts/check_docs.py`、`python3 scripts/check_rust_sizes.py --base main`、`git diff --check` | 全部通过：1052 份 Markdown 结构与库存一致，2712 个手写 Rust 文件无新增尺寸债 | 无 Rust 源码改动 |
| 远端双宿主首轮隔离冷跑 A/B（[Draft PR #463](https://github.com/Halckon/Koven/pull/463)，head `0aff2819`，merge `3b48a47c`，[run 45392390454 attempt 1](https://github.com/Halckon/Koven/actions/runs/45392390454/attempts/1)） | **全部通过（双宿主各 5 配置 / 179 条目 `PASS`，`equivalent=true`）**：两臂均验证空 target（`0 B`）与 `cache_hit=false`（`cold`）。Linux：A 总墙钟 `432.25s`（整 job `477s`、排队 `2s`）vs B 总墙钟 `407.50s`（`build+probe 386.55s`、`list+run 18.73s`，整 job `452s`、排队 `3s`），峰值 RSS `4839.9 / 4840.8 MiB`，target `2.42 / 2.42 GiB`，OOM/重试 `0`。macOS：A 总墙钟 `1127.71s`（整 job `1268s`、排队 `5s`）vs B 总墙钟 `931.90s`（`build+probe 895.39s`、`list+run 35.03s`，整 job `1009s`、排队 `6s`），峰值 RSS `3570.9 / 3418.5 MiB`，target `4.19 / 4.19 GiB`，OOM/重试 `0` | 单轮墙钟差值仅供参考，不作稳定提速结论 |
| 远端双宿主同 SHA 第二轮热跑 A/B（同 head `0aff2819` / merge `3b48a47c`，[run 45392390454 attempt 2](https://github.com/Halckon/Koven/actions/runs/45392390454/attempts/2)） | **全部通过（双宿主各 5 配置 / 179 条目 `PASS`，`equivalent=true`）**：四臂均精确命中本臂缓存键（`cache_hit=true`，`warm`）。Linux：A `444.93s`（整 job `475s`、排队 `3s`）vs B `531.50s`（`build+probe 503.14s`、`list+run 25.62s`，整 job `570s`、排队 `3s`），峰值 RSS `4839.6 / 4832.2 MiB`，target `2.42 / 2.42 GiB`，OOM/重试 `0`。macOS：A `1264.16s`（整 job `1373s`、排队 `258s`）vs B `1212.58s`（`build+probe 1168.35s`、`list+run 42.52s`，整 job `1316s`、排队 `3s`），峰值 RSS `3416.8 / 3419.0 MiB`，target `4.19 / 4.19 GiB`，OOM/重试 `0` | 见下方实测机制记录 |
| 同 SHA 正式 `ci.yml` 参考运行（[run 45392389988](https://github.com/Halckon/Koven/actions/runs/45392389988)） | **全部通过（`CI Passed`）**：`Targeted Tests (ubuntu-26.04)` 排队 `3s`、整 job `2141.0s`（`35m41s`）、原 E2 步骤 `488.0s`；`Targeted Tests (macos-15)` 排队 `15s`、整 job `3705.0s`（`61m45s`）、原 E2 步骤 `722.0s` | 共享预热 target 参考值，不与隔离冷/热跑直接相减 |
| 远端双宿主候选臂默认多核（`--candidate-jobs default` vs 基线 `-j1`）隔离冷跑 A/B（head `ea7b39fd`，merge `cde72aa5`，[run 45414108436 attempt 1](https://github.com/Halckon/Koven/actions/runs/45414108436/attempts/1)） | **全部通过（双宿主各 5 配置 / 179 条目 `PASS`，`equivalent=true`）**：两臂均验证空 target（`0 B`）与 `cache_hit=false`（`cold`），且 `v2` 容器相对路径修复后双宿主均完整保存隔离 target 缓存（Linux `425 MiB`、macOS `783 MiB`）。**Linux（`4 vCPU / 15.6 GiB RAM`）**：A（`-j1`）总墙钟 `565.97s`（整 job `665s`、排队 `3s`）vs B（`default` 4 核）总墙钟 **`312.96s`**（`build 284.74s` + `probe 3.65s` + `list 2.34s` + `run 20.02s`，整 job `414s`、排队 `3s`，同轮缩短 `253.00s` / **`-44.7%`**），峰值子进程 RSS `4833.7 / 5597.1 MiB`，target `2.42 / 2.42 GiB`，OOM/重试 `0`。**macOS（`3 vCPU / 7.0 GiB RAM`）**：A（`-j1`）总墙钟 `1047.26s`（整 job `1129s`、排队 `3s`）vs B（`default` 3 核）总墙钟 **`870.27s`**（`build 816.55s` + `probe 11.20s` + `list 5.22s` + `run 35.66s`，整 job `951s`、排队 `640s`，同轮缩短 `176.99s` / **`-16.9%`**），峰值子进程 RSS `3416.4 / 3173.0 MiB`，target `4.19 / 4.19 GiB`，OOM/重试 `0` | 验证默认多核构建下无 OOM 且内存余量充裕 |
| 远端双宿主候选臂默认多核（`--candidate-jobs default` vs 基线 `-j1`）第二轮热跑 A/B（head `8620573a`，merge `53075c34`，与 `ea7b39fd` 代码零差异，[run 45419926580 attempt 2](https://github.com/Halckon/Koven/actions/runs/45419926580/attempts/2)） | **全部通过（双宿主各 5 配置 / 179 条目 `PASS`，`equivalent=true`）**：四臂均精确命中 `v2` 本臂缓存键（`cache_hit=true`，`warm`），**启动时 `initial_disk.target_dir_bytes` 在 Linux 为 `2.42 GiB`、macOS 为 `4.19 GiB`（证实容器缓存修复生效）**。**Linux（`4 vCPU / 15.6 GiB RAM`）**：A（`-j1`）总墙钟 `419.58s`（整 job `476s`、排队 `3s`，因复用第三方依赖缓存较冷跑基线缩短 `146s`）vs B（`default` 4 核）总墙钟 **`331.55s`**（`build 301.28s` + `probe 3.95s` + `list 2.49s` + `run 21.61s`，整 job `387s`、排队 `3s`，同轮缩短 `88.04s` / **`-21.0%`**），峰值 RSS `4836.6 / 5603.2 MiB`，target `2.42 / 2.42 GiB`，OOM/重试 `0`。**macOS（`3 vCPU / 7.0 GiB RAM`）**：A（`-j1`）总墙钟 `1337.62s`（整 job `1452s`、排队 `4s`）vs B（`default` 3 核）总墙钟 **`1169.21s`**（`build 1110.71s` + `probe 10.28s` + `list 5.85s` + `run 40.22s`，整 job `1285s`、排队 `4s`，同轮缩短 `168.41s` / **`-12.6%`**），峰值 RSS `3440.7 / 3304.8 MiB`，target `4.19 / 4.19 GiB`，OOM/重试 `0` | 首批 `e2-two-source-unit` 全部 4 组双宿主冷/热证据闭环 |
| 第二批 `python3 scripts/e2_two_source_shadow_pilot.py verify-manifest --manifest scripts/e2_replay_manifest.json` 及本机真实工具链隔离 `--list-only` A/B（macOS arm64 `10c / 32 GiB`） | **全部通过（2 配置 / 14 条目 `PASS`，`equivalent=true`）**：精确核验与 `scripts/check_e2_replay_ci.sh`（SHA256 `057d04466c39c3a3...`）逐项一致；基线 `-j1` `309.01s` vs 候选 `default` **`180.58s`**（`build+probe 180.01s`，14 次直调 `--list` 仅 **`0.18s`**，缩短 `-41.6%`），两套异构 feature 配置分别锁定互异 Mach-O 二进制并通过 Cargo 启动探针，峰值 RSS `4882.7 / 6425.1 MiB`，OOM/重试 `0` | 临时目录 `/tmp/koven-spec-0495-replay-ab` 验证后已清理，不入库 |
| 第二批 `e2-replay` 远端双宿主隔离冷跑 A/B（head `594367bd`，merge `44d6debb`，[run 45517415215 attempt 1](https://github.com/Halckon/Koven/actions/runs/45517415215/attempts/1)） | **全部通过（双宿主各 2 配置 / 14 条目 `PASS`，`equivalent=true`）**：四臂均验证独立空 target（`0 B`）与 `cache_hit=false`（`cold`），独立锁定 `e2-two-source-test-support` 与 `+loan-observation-test-support` 两份互异可执行文件。**Linux（`4 vCPU / 15.6 GiB RAM`）**：A（`-j1`）总墙钟 `368.34s`（整 job `432s`、排队 `2s`）vs B（`default` 4 核）总墙钟 **`251.09s`**（`build 246.39s` + `probe 2.12s` + `list 0.21s` + `run 1.06s` = **纯 `list+run` 仅 `1.27s`**，整 job **`314s`**、排队 `3s`，同轮缩短 `117.25s` / **`-31.8%`**），峰值 RSS `4605.1 / 5404.8 MiB`，target `1.46 / 1.46 GiB`，OOM/重试 `0`。**macOS（`3 vCPU / 7.0 GiB RAM`）**：A（`-j1`）总墙钟 `820.35s`（整 job `914s`、排队 `4s`）vs B（`default` 3 核）总墙钟 **`626.70s`**（`build 617.60s` + `probe 6.67s` + `list 0.35s` + `run 1.46s` = **纯 `list+run` 仅 `1.81s`**，整 job **`735s`**、排队 `10s`，同轮缩短 `193.64s` / **`-23.6%`**），峰值 RSS `3235.8 / 3215.4 MiB`，target `2.75 / 2.75 GiB`，OOM/重试 `0` | 套件级 `CARGO_TARGET_DIR` 与缓存键与首批物理隔离 |
| 第二批 `e2-replay` 远端双宿主同 SHA 热跑 A/B 及正式 `ci.yml` 参考耗时（同 head `594367bd` / merge `44d6debb`，[run 45517415215 attempt 2](https://github.com/Halckon/Koven/actions/runs/45517415215/attempts/2)，参考 [CI run 45517415414](https://github.com/Halckon/Koven/actions/runs/45517415414)） | **全部通过（双宿主各 2 配置 / 14 条目 `PASS`，`equivalent=true`）**：四臂精确命中 `e2-replay` 专属缓存键（`cache_hit=true`，`warm`，启动时恢复 Linux `1.46 GiB` / macOS `2.75 GiB` target）。**Linux（`4 vCPU / 15.3 GiB RAM`）**：A（`-j1`）总墙钟 `271.26s`（整 job `325s`、排队 `4s`）vs B（`default` 4 核）总墙钟 **`201.77s`**（`build 198.02s` + `probe 1.74s` + `list 0.14s` + `run 0.66s` = **纯 `list+run` 仅 `0.81s`**，整 job **`243s`**、排队 `2s`，同轮缩短 `69.49s` / **`-25.6%`**），峰值 RSS `4606.1 / 5329.4 MiB`，target `1.46 / 1.46 GiB`，OOM/重试 `0`。**macOS（`3 vCPU / 7.0 GiB RAM`）**：A（`-j1`）总墙钟 `775.61s`（整 job `865s`、排队 `3s`）vs B（`default` 3 核）总墙钟 **`649.28s`**（`build 641.62s` + `probe 5.17s` + `list 0.42s` + `run 1.45s` = **纯 `list+run` 仅 `1.88s`**，整 job **`729s`**、排队 `4s`，同轮缩短 `126.34s` / **`-16.3%`**），峰值 RSS `3190.8 / 3288.5 MiB`，target `2.75 / 2.75 GiB`，OOM/重试 `0`。同 SHA 正式 `ci.yml`（`CI Passed`）原 `Experimental E2 replay faults (CI only)` 步骤参考耗时：Linux `344.0s`（整 job `2385.0s`）、macOS `537.0s`（整 job `3995.0s`，共享预热 target 仅供参考） | 第二批 `e2-replay` 冷/热双宿主证据全部闭环 |
| PR #463 8 项 P2 审阅修复后双宿主实机冷/热复验（head `94eeaf08`，merge `5c8c133d`，`git_tree=50d0b946`，[shadow run 45529296024 attempts 1 & 2](https://github.com/Halckon/Koven/actions/runs/45529296024)，同 SHA 正式 [CI run 45529296052](https://github.com/Halckon/Koven/actions/runs/45529296052)） | **全部通过（冷/热各双宿主 `14/14 PASS`、`CI Passed`）**：实机验证 `dynamically_verified_exec_branch=true`、`compare-run-downloads`、`has_target_deps` 暖缓存强校验、Mach-O `ARM64` / ELF `x86_64` 架构强校验及 macOS `dyld` 真实加载 `libLLVM.dylib` UUID+SHA256 探针。**冷跑（`attempt 1`）**：Linux `373.37s → 272.00s`（`-27.2%`，`list+run 1.31s`）、macOS `711.78s → 520.32s`（`-26.9%`，`list+run 1.65s`）；**热跑（`attempt 2`）**：Linux `292.13s → 239.46s`（`-18.0%`，`list+run 1.21s`，整 job `342s → 284s`）、macOS `756.18s → 629.94s`（`-16.7%`，`list+run 2.09s`，整 job `849s → 713s`），全量 `0` OOM / `0` 重试。同 SHA 正式 `ci.yml`（`CI Passed`）：Linux 整 job `2456s`（原 `e2_replay` 步骤 `378s`）、macOS 整 job `4122s`（原 `e2_replay` 步骤 `571s`） | 8 项 P2 审阅问题已全部修复并完成实机闭环 |

### 10.1 双宿主实测归因与边界记录

1. **覆盖与语义等价全部成立**：
   - **首批 `e2-two-source-unit`**（5 配置 / 179 条目）：双宿主共 4 组 8 轮 A/B（`-j1` 冷/热、`default` 多核冷/热，合计 `8 × 2 × 179 = 2,864` 次条目验证）全部 `179/179 PASS`。
   - **第二批 `e2-replay`**（2 配置 / 14 条目）：双宿主共 2 组 4 轮 A/B（`default` 多核冷/热，合计 `4 × 2 × 14 = 112` 次条目验证）全部 `14/14 PASS`，两套异构 feature 二进制独立锁定无混用，纯 14 条 `list + run` 直调总耗时仅 **Linux `0.81s～1.27s`、macOS `1.81s～1.88s`**。两批全量运行均零漏跑、零错产物、零环境探针失配、零额外 OOM 或磁盘超限。
2. **进程派生开销 vs 单核重编主导占比**：
   - 去除 353 次冗余 Cargo 包装调用后，候选臂纯 `list + run` 179 条用例仅需 **Linux `18.7s～25.6s`、macOS `35.0s～42.5s`**（对比基线臂仅 `run` 阶段即需 Linux `27.9s～46.1s`、macOS `74.7s～94.9s`，外加 macOS 上百秒 `--list` Cargo 进程启动开销）。
   - 在 `-j1` 约束下，总耗时的 **93%～96%**（Linux `383s～520s`、macOS `886s～1160s`）花在单核编译 5 套异构 feature/target 配置；且云端 Runner 宿主机性能存在明显波动，证实单轮墙钟差不能脱离并发配置单独解读。
3. **候选臂启用 Cargo 默认多核（`default`）后的扩展性与内存安全**：
   - **Linux（`ubuntu-26.04` 标准 Runner：`4 vCPU / 15.6 GiB RAM`）**：候选臂放开默认 4 核构建后，5 套配置总构建耗时降至 `284.74s`，总墙钟从同轮基线 `565.97s` 降至 **`312.96s`（`-44.7%` / `1.81x`）**；子进程峰值 RSS 从 `4.72 GiB` 小幅升至 `5.47 GiB`（约占整机 `15.6 GiB` 内存的 `35%`），零 OOM。
   - **macOS（`macos-15` 标准 M1 Runner：`3 vCPU / 7.0 GiB RAM`）**：实测宿主机配额为 **3 核 / 7 GiB RAM**（而非 4 核），候选臂总墙钟从同轮基线 `1047.26s` 降至 **`870.27s`（`-16.9%` / `1.20x`）**，峰值单子进程 RSS `3.10 GiB`，零 OOM；同时观测到 macOS Runner 池并发槽位排队可达 `640s`（`10m40s`）。macOS 多核加速比低于 Linux 的直接原因是：(a) 物理核心数仅为 3 核且整机内存 7 GiB；(b) 每套配置均串行受限于 `lang-frontend`（`27.5 万`行单 `lib.rs`）→ `lang-codegen`（`52.8 万`行单 `lib.rs`）两条巨型单体 crate 编译链，`rustc` 前端解析/类型/借用检查阶段为单线程，仅后端 LLVM codegen units 可吃满多核。
4. **热跑缓存行为与容器路径实测记录**：
   - 首轮容器缓存因 YAML `${{ github.workspace }}`（宿主机 `/home/runner/work/...`）与容器内挂载点（`/__w/...`）不一致仅命中 `~/.cargo`（`2.18 MiB`）；在 `v2` 改用工作区相对路径 `target-e2-shadow-${{ matrix.arm }}` 后，Linux 与 macOS 均已完整保存隔离 target 缓存（分别为 `425 MiB` 与 `783 MiB`）。
   - 同时由首轮 macOS 热跑（精确恢复 `783.5 MiB` 缓存 / `4.19 GiB` target）已证实：因 `actions/checkout@v7` 刷新 `.rs` 源码 `mtime` 且 `CARGO_INCREMENTAL=0`，`target/` 缓存仅能跳过第三方依赖重编，自有巨型 crate 仍会重编；真正决定编译墙钟的杠杆是**放开多核构建**与后续**收敛异构 feature 重编次数 / 拆分巨型单体测试目标**。
