# 手写 Rust 物理行软上限

> **性质**：工程门禁合同 · **状态**：current · **读取时机**：新增、扩展或拆分 Rust 文件及评审尺寸例外时 · **唯一真源**：本页定义政策，`scripts/rust-size-policy.json` 保存逐文件基线与例外，`scripts/check_rust_sizes.py` 执行检查

## 范围与度量

1000 物理行是评审软上限，覆盖仓库内全部手写 `.rs`：生产、私有单元测试、integration、
helper、build script 和 fixture。统计 LF 分隔的物理行，包含空行、注释、内嵌字符串与末尾无
换行的非空片段；CRLF 与 LF 计数一致。不以 LOC/SLOC、函数数或格式化后的行数代替。

脚本读取 Git tracked 与非 ignored untracked 文件；已删除文件不违规，未加入 Git 的新文件
也不能漏检。symlink `.rs` 明确失败。只扫描 Rust；其他语言不因本政策自动获得类似门禁。

禁止压行、删空行、缩断言或 `include!` 碎片拼接来凑 999。rustfmt 与人工 move-aware review
仍需核验职责、断言与可读性；行数脚本不声称能自动识别所有绕过或证明语义保全。

## 基线、增长与例外

首个基线固定在 PR15/16 合并后的 `main 4383509dbfb805f774581a29136fc46dd62504a4`：
596 个手写 Rust 文件中49个超过1000行，无登记的生成 Rust。LSP `server.rs` 已是499行，
不再沿用批准计划在旧 main 上记录的50个超限文件。批准计划与旧验收原文保持历史事实。

- 初次接入时，base 没有 policy，`baseline` 必须精确等于比较基底的所有手写超限文件及真实尺寸
- 后续 baseline 只能删除、降低额度，或伴随 Git 识别的 rename 同步路径；不能新增/提高额度
- 已有超限文件只有在不超过 baseline 额度和 base 实际尺寸两者较小值时直接通过
- 降到1000以内后再次超限、缩小后重新增长、新超限、copy、无可识别来源的迁移，都需显式例外
- 删除文件不违规；baseline 可以保留历史项，但不能借其给删除后重建的文件重新授权
- 不提供“重新生成baseline”覆盖当前欠账的命令。每次 policy diff 都是必须审阅的工程变更

`exceptions` 使用精确路径，逐项必填 `max_lines`（大于1000的有限整数）、`reason`（为何
当前不可合理拆分）、`owner`（负责人或模块归属）、`split_plan`（后续职责拆分方向）、
`review`（复查时机或可判定条件）。行数不得超过额度；下一次超出额度必须修改例外并复审。
例外授权额度以内的后续变化，不代表可以继续塞入新职责；评审仍看理由与拆分计划是否成立。
路径失效应删除或随迁移更新记录。未知字段、重复JSON key、空值与路径通配符均拒绝。
初始护栏批次没有为历史欠账批量伪造负责人或永久例外。当时 `exceptions` 为空；
后继[iteration测试迁移](drop-iteration-test-migration.md)登记三个完整场景的有限例外，
各自锁实际尺寸并给出拆分方向/复查条件，不能回填成baseline。

生成 Rust 仅允许在 `generated` 中逐文件列出精确路径、`inputs`（生成输入）、`generator`
（生成命令/工具）、`version`（固定版本或提交）。报告单列其物理行数，不机械拆生成产物。
不能同时登记为手写baseline/exception，也不按文件名或注释自动猜测“生成物”。
将手写文件改列生成物需要审阅真实生成链，不能用新增登记静默绕开增长限制。

## 比较基底与 rename 边界

本地从项目根执行：

```bash
# 先更新远端引用，避免把过期 origin/main 当当前目标。
git fetch origin main
python3 scripts/check_rust_sizes.py --base origin/main
python3 -m unittest discover -s scripts/tests -v
```

`--base` 必填且必须能解析为commit。脚本取它与当前HEAD的唯一merge-base，打印完整SHA，
避免目标分支新提交混入本分支比较；缺对象、浅历史缺共同祖先或多个merge-base均失败，
不会回退成“未发现增长”。检查工作树内容，包括已暂存与未暂存编辑；CI使用确切PR head。

rename 只使用 Git `--find-renames=50%` 的一对一身份，关闭rename数量截断，不推测copy。
已暂存的 `git mv` 加上同步baseline key 可以继承旧额度；迁移后仍不得比旧文件实际尺寸增长。
重写过多导致 Git 未识别、或新目标尚未加入index时，按新文件处理并要求例外。
启发式识别不是语义等价证明，review必须确认迁移而非恰巧相似的不同职责。

## CI 接线与交付边界

`Rust Physical Line Guard` 在所有已配置的 push、pull_request、workflow_dispatch 事件必跑，
不依赖路径过滤，所以Rust、脚本、policy、workflow和纯文档变化都会触发。它独立运行尺寸
policy tests和真实仓库检查，`CI Passed` 拒绝该job失败、取消、缺失或跳过。

PR checkout明确使用head SHA、完整历史，以事件base SHA计算merge-base；push使用before SHA。
新分支的全零before或manual事件使用origin/main。完整fetch缺少旧对象时仅补取所需base，
权限仍为contents read；无法取得就失败，不静默换成HEAD。main与manual的原有完整矩阵、
PR的双宿主矩阵、feature/fix push成本策略均保留。新护栏本身只需Ubuntu/Python。

本批不改Rust源码、Cargo target、依赖、语言语义、测试断言或原有ignore。尺寸通过不证明
职责划分合理、性能提升或测试完整。冷/热compile/link、RSS与重复样本仍未测；下一次有界
拆分继续遵守[测试规则](testing.md)与[整体执行账本](engineering-governance-progress.md)。
