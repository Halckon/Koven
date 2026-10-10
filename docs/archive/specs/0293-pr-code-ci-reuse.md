# SPEC-0293: PR 普通文档追加提交复用真实代码 CI

> **性质**：变更合同 · **状态**：done · **读取时机**：实现或验收 PR 增量 CI 时 · **唯一真源**：本 Spec

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P6-0293` |
| 所属 Phase | Phase 6 交付门禁 |
| 语言规范 | 现行 [Guide](../../guide/README.md)，不改变语言行为 |
| 批准依据 | 2026-10-10 用户要求增加 PR 增量机制，避免代码 CI 成功后仅推送文档重跑代码 CI |
| 前置 Spec | 无 |
| 前置 ADR | 无 |
| 关联 ADR | 无新增长期架构决定 |
| 阻塞项 | 功能验收已完成；归档提交的完整 CI 与合并交付仍须通过 PR #70 完成 |
| 影响范围 | CI workflow、Python 证据判定与汇总、普通 fixture 测试 |
| 语言语义变更 | 否 |

## 1. Goal

同一 PR 在代码门禁真实成功后，后续仅普通文档变化可复用该成功证据；不把未运行、失败、
取消、进行中或其他 PR 的结果当作成功。本来纯文档的 PR 继续遵守路径豁免。

## 2. 合同

- 原累计路径判定保留；增量复用独立于路径输出，不伪造 `rust=false`。
- main/manual 强制门禁与 feature/fix push 成本策略保持；push 成功永不成为 PR 证据。
- 成功证据绑定同仓、同 PR、同 head 仓库、workflow ID/路径、最新 attempt、精确 base SHA、
  历史 head 与 synthetic merge。通过 Git 对象重新核对两个 checkout 模式的实际输入。
- 输入以全 Git tree 的 mode/type/path/object ID 确定，只有明确普通 Markdown/SVG 文档可豁免。
  Guide、compiler-specs、tutorial、标准库、所有 scripts、workflow、依赖、LICENSE、
  preview-candidate 及未知路径均不是普通文档输入。
- 逐一核对实际 required job，包含两宿主 check/clippy/test 与需要时的 producer/consumer、
  editor/fmt；成功的 evidence job 不能替代这些 job 的 `success`。
- workflow 固定 `run-name` 仅由 GitHub event/PR number 生成，API `display_title` 必须精确匹配。
  历史 head/merge 指纹包含同一 workflow 定义；REST `pull_requests=[]` 不丢失身份，非空矛盾则拒绝。
- evidence job 的名称携带 v1/PR/base/head/merge/指纹；只在实际门禁成功时成功，复用 run 不产生
  新证据。连续文档提交必须最终回到真实执行 run，不允许循环或传递式自证。
- 只读 REST API、attempt 专属 jobs，重复/缺失/不完整页/未知状态不获复用；较新同 PR
  run 失败、取消或尚未成功时直接保守重跑。查找最多最近 100 个 PR run，窗口外证据不猜测。
- API/权限/Git 对象不可用时运行既有 required jobs；汇总时再次独立读取证据失败则汇总失败。
- docs 在每次 PR 执行，rust-size/dependencies 保持无条件必需；复用 jobs 在 UI 显示 skipped，
  JSON/step summary 提供来源 run URL、base、指纹与回退原因。旧 preview 不宣称为新 head 构建。

## 3. 非目标

不修改 Rust；云开发环境不执行 Cargo、故障注入与校准。用户已允许 GitHub Actions CI
运行既有故障注入/校准，但本地 fixture 验证不调用它们。发布、PR、远端验收与合并另按用户授权推进。不缓存伪造成功，不使用写权限或 `pull_request_target`，不建立跨 PR 共享证明。
本次从 main `ef60f2f` 建立独立 `feature/spec-0293` 分支，不夹入 N1a/0290 改动。

## 4. 验收与记录

| 验收项 / 命令 | 结果 | 未运行原因 / 证据 |
|---|---|---|
| 新 `test_pr_ci_reuse.py` 普通 metadata fixture 首次运行 | 失败：模块尚未实现 | 测试先于实现创建，不是故障注入 |
| 相同输入、错误身份、base/tree 改变、required job 缺失/失败/取消/skipped | 通过 | 下行同一次 fixture 运行 |
| `python3 -m unittest discover -s scripts/tests -p test_pr_ci_reuse.py -v` | 21/21 通过 | 普通 metadata / 临时 Git fixture；含连续复用、状态屏障、attempt/分页/API 回退 |
| `python3 -m unittest discover -s scripts/tests -p test_check_ci_results.py -v` | 30/30 通过 | 必需 job 严格检查及独立复核 |
| `test_check_docs.py` / `python3 scripts/check_docs.py` / `git diff --check` | 37/37、598 Markdown、whitespace 通过 | YAML 解析、Python py_compile 亦通过；未运行 actionlint |
| 远端 PR #70 首轮真实完整 CI | 16/16 jobs 成功 | [run 38064383882](https://github.com/Halckon/Koven/actions/runs/38064383882)，head `189d1b4`；双宿主 check/clippy/test、preview producer/consumer、物理证据与最终汇总均成功 |
| 同 PR 普通文档追加提交真实复用 | 通过 | [run 38065460124](https://github.com/Halckon/Koven/actions/runs/38065460124)，head `42d56e9`；`reuse=true`，来源 run `38064383882` attempt 1；代码 jobs skipped，docs/尺寸/依赖与最终独立汇总 success |
| 归档 inventory 改变后的代码输入回退 | 待 PR CI 验证 | 本次迁移同时修改 `scripts/check_docs.py`，必须不复用旧指纹；不将待运行记录计为通过 |

## 5. 远端证据身份

- PR：[#70](https://github.com/Halckon/Koven/pull/70)；两次验收 base 均为 `ef60f2fcd07f07dc92d8a204a9c7dd2e40494c3c`。
- 物理执行 head：`189d1b4f05255028a13661d8cbf79201cc8d95e1`；synthetic merge：`eb7598fcd42fe1457be9638e80edd0bdaa4313e7`。
- 文档复用 head：`42d56e9bebec31693ed6649b667cf8bc15da224b`；synthetic merge：`c3060baf8b8751eb040e115d4b09f5130aaab534`。
- 两轮输入指纹均为 `de6ee38134bea489ee74d567f754f982dd3511611286345f01a1480434e413d6`。
- 第二轮重新完成 docs、尺寸、依赖与 CI Passed；物理证据 job skipped，未生成传递式证据。
- 归档与最终合并状态以 PR 对应 head 的实际检查为准，不把此前成功冒充新 head 成功。

## 6. 参考与限制

GitHub 官方合同：[PR checkout 与 fork 权限](https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows#pull_request)、
[workflow runs](https://docs.github.com/en/rest/actions/workflow-runs)、
[attempt 专属 jobs](https://docs.github.com/en/rest/actions/workflow-jobs)。
外部 runner 镜像/浮动 action tag 随时间变化不在 Git tree 内；本机制复用原 CI 验证结论，
不承诺重新观察外部服务的当前状态。main/manual 仍可强制重新执行。
编译器 Architecture 不变；开发验收规则同步本次 CI 合同。
