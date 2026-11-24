# SPEC-0495: E2 两源 Unit CI 门禁的可撤回影子试点与双宿主 A/B 对账

> **性质**：CI 基础设施变更合同 · **状态**：in-progress · **读取时机**：实施、运行、核验或撤回 E2 两源 Unit 影子提速试点时 · **唯一真源**：本 Spec

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-CI-0495` |
| 所属 Phase | 工程与 CI 基础设施；不改变语言 Phase |
| 语言规范 | 现行 [Guide v0.47](../../guide/README.md) |
| 批准依据 | 2026-11-24 用户明确下发五点三轮收口实施建议：首批仅针对 `scripts/check_e2_two_source_unit_ci.sh` 建立独立可撤回影子试点 |
| 前置 Spec | 无 |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 待远端 GitHub Actions 双宿主冷/热 A/B 实跑证据补齐后决定是否推进下一批或归档 |
| 影响范围 | `.github/workflows/ci-e2-shadow-pilot.yml`、`scripts/e2_two_source_unit_manifest.json`、`scripts/e2_two_source_shadow_pilot.py`、`scripts/tests/test_e2_two_source_shadow_pilot.py`、`docs/development/testing.md` |
| 语言语义变更 | 否 |

## 1. Goal

在不改动正式 `.github/workflows/ci.yml` 门禁与 `scripts/check_e2_two_source_unit_ci.sh` 现网执行路径的前提下，为 `scripts/check_e2_two_source_unit_ci.sh`（5 个构建配置、179 个执行条目、原 358 次 Cargo 调用）交付一套**独立可一键关停的非门禁影子试点工作流、共享清单、唯一产物锁定与真实 Cargo 启动环境探针、防漏跑硬校验执行器及双宿主隔离冷/热 A/B 对账工具**。

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

- 不修改 `.github/workflows/ci.yml`、不修改 `scripts/check_e2_two_source_unit_ci.sh` 现网执行逻辑，不替代任何 required CI 门禁。
- 本批不覆盖 `scripts/check_e2_replay_ci.sh`（2 配置 / 28 次调用），不动 Clippy、正式缓存与并发度、Preview 独立 consumer、`check_generated_owners.py` G4/G5、D10/E2/E3 编译器代码、Parser Lean/Kani 与差分测试及内核原型入口。
- 不以 `20–25 分钟` 作为首批验收线，不将单次运行耗时波动宣称为稳定提速结论；P1/P2/P3 重构另案推进。

## 5. 验收标准

- [x] 共享清单 `scripts/e2_two_source_unit_manifest.json` 精确覆盖 5 个配置、179 个有序测试条目，并与 `scripts/check_e2_two_source_unit_ci.sh` 通过自动等价核验
- [x] 唯一产物锁定、真实 Cargo 启动环境探针、秘密过滤、防漏跑（0 命中 / 多命中 / 退出 0 但未执行）、预期失败分类与证据核验均具备红绿单测覆盖
- [x] 本机真实工具链下完成 `baseline` 与 `candidate` 双模式 `--list-only` 实跑及 `compare` 对账（未伪造 `CI`/`GITHUB_ACTIONS`）
- [x] 远端双宿主（`ubuntu-26.04` 容器与 `macos-15`）完成隔离冷跑（cache miss）与热跑（exact cache hit）A/B 实跑及遥测记录对账
- [x] Architecture 确认无需更新（不涉及编译器阶段或语言实现变化）；`docs/development/testing.md` 同步影子试点边界说明
- [x] 表面语法增量门禁：`N/A`（纯 CI 影子基础设施，无表面语法变更）

## 6. 技术方案与边界

- **清单层**：`scripts/e2_two_source_unit_manifest.json` 声明 5 个配置及 179 条用例元数据（含 `expected_outcome` 与 `expected_failure_evidence`）。
- **执行与探针层**：`scripts/e2_two_source_shadow_pilot.py` 提供 `verify-manifest`、`run --mode {baseline,candidate}`、`compare` 与 `collect-workflow-metrics` 子命令，内置 Cargo `target.<triple>.runner` 探针模式（仅采集白名单键、工作目录、动态库路径与 `llvm-config`/`clang` 版本身份）。
- **工作流层**：`.github/workflows/ci-e2-shadow-pilot.yml` 运行双宿主 `arm: [baseline, candidate]` 隔离矩阵及汇总 `compare` 任务，使用 `actions/cache/restore@v4` + `actions/cache/save@v4` 精确键隔离冷/热阶段。

## 7. 实施计划

1. [x] 建立隔离工作树与 `SPEC-0495` 合同登记 → 验证：`python3 scripts/check_docs.py`
2. [x] 生成 179 条共享清单并编写先行失败单测 `scripts/tests/test_e2_two_source_shadow_pilot.py` → 验证：单测按预期原因失败
3. [x] 实现 `scripts/e2_two_source_shadow_pilot.py`（清单核验、Cargo JSON 唯一产物锁、真实 Cargo 启动探针、防漏跑执行器、A/B 对账与遥测） → 验证：单测全绿
4. [x] 编写 `.github/workflows/ci-e2-shadow-pilot.yml` 与 `docs/development/testing.md` 边界说明，补齐工作流契约单测 → 验证：`scripts/tests` 全套单测通过
5. [x] 本机执行 `--list-only` 双臂实跑对账、`check_docs.py`、`check_rust_sizes.py` 与 `git diff --check` → 验证：本地门禁全绿

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
| `python3 -m unittest discover -s scripts/tests -p test_e2_two_source_shadow_pilot.py -v` | 通过：9 组测试全绿（覆盖清单漂移、非 CI 拒绝、秘密过滤、环境探针失配、0/多产物拒绝、0/多 list 命中拒绝、退出 0 未执行拒绝、预期失败分类与 witness 校验、A/B 对账与工作流隔离） | 先红后绿（TDD） |
| 本机真实工具链隔离 target `--list-only` A/B 实跑（macOS arm64，Rust 1.96.0，LLVM 21.1.8，`cfg2-codegen-test-two-source-unit` 7 项 + `cfg5-frontend-lib-e2-support` 3 项） | 通过：10 项 `compare` 等价 `PASS`；候选臂真实锁定 Mach-O 产物（`389f877562f1c5ee`、`5fc921c2b27db8c2`）并通过 Cargo 启动探针核验 `cwd`、`CARGO_*`、`DYLD_FALLBACK_LIBRARY_PATH` 与 `libLLVM.dylib 21.1.8`；10 次直调 `--list` 共 0.07s，峰值子进程 RSS 3433.3 / 3435.4 MiB，target 1.61 GiB，OOM/重试 0 | 临时目录 `/tmp/koven-spec-0495-local-ab` 验证后已清理，不入库 |
| 现有工作流与门禁策略单测（`test_llvm_install_bounds`、`test_ubuntu26_ci`、`test_pr_ci_reuse`、`test_check_ci_results`、`test_ci_debug_profile`） | 通过：96 项通过，确认正式 `ci.yml` 合同零受影响 | `test_ci_toolchain_image` 中 7/8 通过，1 项因本机 macOS 缺 GNU `sha256sum` 失败（与本分支无关，Ubuntu runner 原生提供） |
| `python3 scripts/check_docs.py`、`python3 scripts/check_rust_sizes.py --base main`、`git diff --check` | 全部通过：1052 份 Markdown 结构与库存一致，2712 个手写 Rust 文件无新增尺寸债 | 无 Rust 源码改动 |
| 远端双宿主首轮隔离冷跑 A/B（[Draft PR #463](https://github.com/Halckon/Koven/pull/463)，head `0aff2819`，merge `3b48a47c`，[run 45392390454 attempt 1](https://github.com/Halckon/Koven/actions/runs/45392390454/attempts/1)） | **全部通过（双宿主各 5 配置 / 179 条目 `PASS`，`equivalent=true`）**：两臂均验证空 target（`0 B`）与 `cache_hit=false`（`cold`）。Linux：A 总墙钟 `432.25s`（整 job `477s`、排队 `2s`）vs B 总墙钟 `407.50s`（`build+probe 386.55s`、`list+run 18.73s`，整 job `452s`、排队 `3s`），峰值 RSS `4839.9 / 4840.8 MiB`，target `2.42 / 2.42 GiB`，OOM/重试 `0`。macOS：A 总墙钟 `1127.71s`（整 job `1268s`、排队 `5s`）vs B 总墙钟 `931.90s`（`build+probe 895.39s`、`list+run 35.03s`，整 job `1009s`、排队 `6s`），峰值 RSS `3570.9 / 3418.5 MiB`，target `4.19 / 4.19 GiB`，OOM/重试 `0` | 单轮墙钟差值仅供参考，不作稳定提速结论 |
| 远端双宿主同 SHA 第二轮热跑 A/B（同 head `0aff2819` / merge `3b48a47c`，[run 45392390454 attempt 2](https://github.com/Halckon/Koven/actions/runs/45392390454/attempts/2)） | **全部通过（双宿主各 5 配置 / 179 条目 `PASS`，`equivalent=true`）**：四臂均精确命中本臂缓存键（`cache_hit=true`，`warm`）。Linux：A `444.93s`（整 job `475s`、排队 `3s`）vs B `531.50s`（`build+probe 503.14s`、`list+run 25.62s`，整 job `570s`、排队 `3s`），峰值 RSS `4839.6 / 4832.2 MiB`，target `2.42 / 2.42 GiB`，OOM/重试 `0`。macOS：A `1264.16s`（整 job `1373s`、排队 `258s`）vs B `1212.58s`（`build+probe 1168.35s`、`list+run 42.52s`，整 job `1316s`、排队 `3s`），峰值 RSS `3416.8 / 3419.0 MiB`，target `4.19 / 4.19 GiB`，OOM/重试 `0` | 见下方实测机制记录 |
| 同 SHA 正式 `ci.yml` 参考运行（[run 45392389988](https://github.com/Halckon/Koven/actions/runs/45392389988)） | **全部通过（`CI Passed`）**：`Targeted Tests (ubuntu-26.04)` 排队 `3s`、整 job `2141.0s`（`35m41s`）、原 E2 步骤 `488.0s`；`Targeted Tests (macos-15)` 排队 `15s`、整 job `3705.0s`（`61m45s`）、原 E2 步骤 `722.0s` | 共享预热 target 参考值，不与隔离冷/热跑直接相减 |

### 10.1 双宿主实测归因与边界记录

1. **覆盖与语义等价成立**：双宿主冷/热共 4 轮 A/B（合计 `4 × 2 × 179 = 1,432` 次条目验证）全部 `179/179 PASS`，零漏跑、零错产物、零环境探针失配、零额外 OOM 或磁盘超限（target 稳定在 Linux `2.42 GiB` / macOS `4.19 GiB`）。
2. **进程派生开销 vs 单核重编主导占比**：
   - 去除 353 次冗余 Cargo 包装调用后，候选臂纯 `list + run` 179 条用例仅需 **Linux `18.7s～25.6s`、macOS `35.0s～42.5s`**（对比基线臂仅 `run` 阶段即需 Linux `27.9s～36.1s`、macOS `74.7s～94.9s`，外加 macOS 上百秒 `--list` Cargo 进程启动开销）。
   - 但总耗时的 **93%～96%**（Linux `383s～499s`、macOS `886s～1160s`）花在 `-j1` 单核编译 5 套异构 feature/target 配置；且云端 Runner 宿主机 CPU 波动显著（如 Linux 候选臂同一源码构建耗时在两次运行间从 `383.2s` 波动至 `499.0s`，反超基线臂 `86.6s`），证实单轮墙钟差不能直接当成稳定提速结论。
3. **热跑缓存行为实测记录**：
   - `macos-15` 精确恢复了 `783.5 MiB` 压缩缓存（启动时 `target_dir_bytes = 4.19 GiB`），但因 `actions/checkout@v7` 刷新了工作区 `.rs` 源码 `mtime` 且 `CARGO_INCREMENTAL=0`，Cargo 仅复用了第三方依赖，仍对自有 workspace（`lang-frontend` / `lang-codegen`）执行了重编。
   - `ubuntu-26.04` 容器任务中 YAML `${{ github.workspace }}` 解析为宿主机路径 `/home/runner/work/Koven/Koven` 而容器内挂载点为 `/__w/Koven/Koven`，因此缓存仅命中 `~/.cargo`（`2.18 MiB`）；即便恢复完整 `target`（如 macOS 所示），Cargo 在新 checkout `mtime` 下同样会重编自有 crate。后续若需跨 run 复用编译产物，需从编译并发度（去除 `-j1`）或收敛异构 feature 组合入手，而非依赖 `target/` 目录缓存。
