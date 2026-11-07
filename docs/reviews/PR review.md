我已经把仓库拉到本地，#69、#70、#71 三个 PR 都合并了。现在读进度账本和这三个 PR 的 diff，继续 review。上一条命令因为 `cut` 截断中文字符，输出不是合法 UTF-8，我重跑一遍，改成安全解码，继续跑文档检查、CI 脚本测试，再核对版本戳。三个 PR 的代码和 SPEC 账本我读完了，没有编译、没有跑 Rust 测试，也没查到合并后 main 的 CI 状态（GitHub API 被限流）。我实际跑了 `check_docs.py`（609 个文件通过）以及 PR #70 的两组 Python 测试（21 和 31 个，全过）。

## PR review

**#69（SPEC-0288 Map + 普通借用，391 个文件，+44k 行）**

- **范围过大，混入 N1a**：同一个 PR 里带了 SPEC-0289 的范围代码（`range.rs`、`type_range_extension`、`range_project_cli`），还有一个"登记 SPEC-0289 wiring 增长"的提交。后来 SPEC-0290 的 R6 发现"N1a 休眠"的说法与实际入口不符，根源就在这里。
- **合并后修出 4 个 codegen 缺陷**（R1–R4）：
  - Map owner 的 SSA 值跨 CFG 失效。
  - `MapPut`/`MapRemove` 的 drop glue 依赖偶然出现的作用域 Drop。
  - Group 包裹的借用返回 Call 被拒绝。
  - unit 路径下 `owner.field` 实参缺调用帧。
- **CI 选择有漏洞**：23 个 frontend 集成测试目标存在，但没进入 CI（R5）。所以 0288 "已验收归档"的证据强度弱于它的表述。

**#70（SPEC-0293，文档追加提交复用真实代码 CI）**

- 整体设计谨慎：失败时退回跑全量，用 merge parents 做指纹，按 attempt 取 job。
- 两处代价：
  - 只有 `.md` 和 `.svg` 算文档，所以 `docs/development/evidence/*.log` 和 `*.json` 这类证据文件会被当作代码，纯证据提交复用不了。而 #71 就带了 38 个这类文件。
  - 查证据只看 workflow 最近 100 个 PR run，不翻页。这是安全的退化，只增加成本。

**#71（SPEC-0290 修复，同时夹带 SPEC-0289）**

- **分支名和内容不符**：`fix/spec-0290` 里有 5 个 N1a 功能提交（receiver ABI、std take、drop/dropLast，外加 Guide v0.43 接入）。这违反了你们自己的"一个 Spec 对应一个能力"，也不利于 bisect。
- **R1 的修复测试偏薄**：只覆盖了常量条件的 `if (true) {..} else {..}`，没有 `when`、`&&`/`||` 短路、`?:` 或带 `return`/`break` 的实参。实现靠 `temporaries` 和 `pending_call_loans`（以表达式下标为键）加 `retained` 标志暂存，比较脆，值得抽成专用的 carry-slot 类型。
- **`ungroup` 辅助逻辑**：在三处重复，连注释都是复制的。
- **已知能力缺口**：SPEC-0289 §20 记录，含 carrier 绑定的函数里，`(-2147483647-1)` 这类二元算术仍会被 `UnsupportedNode` 拒绝。所以负数 Abort 只验证了字面量，计算出来的数量走不到。
- **文档漂移**：
  - Guide 里残留旧版本戳，我在 main 里逐处核实了：`15` 第 93 行写"不属于 v0.42"，`01` 第 68 行写"v0.40 没有使用它的产生式"，`03` 第 294 行和 `05` 第 350 行还有"v0.37"。
  - `evolution-status.md` 还停在 v0.41 和 2026-10-04 的基线。
  - `roadmap.md` 仍写"当前主线是 M1A"，而 M1A 早已完成。
  - `check_docs` 通过只说明结构没问题，不能发现这些语义上的过期。

## 里程碑进度

repo 没有总量口径，下面是我按 `post-governance-milestones.md` 和各 Spec 重建的状态，不给假百分比：

| 里程碑         | 状态                                                         |
| -------------- | ------------------------------------------------------------ |
| M0 交接        | 基本完成（P2 成本预算仍延期）                                |
| M1A 多文件程序 | 完成（PR46）                                                 |
| M1B 文本处理   | 只完成 a（argv 词频，0274）；b（文本输入、String API）未开始 |
| M2B 借用结果   | 首个范围完成（0287/0288）；多来源、条件借用仍延后            |
| M3A 顺序集合   | 进行中：list 的 add/removeAt/insertAt 等和运行时长度构造已完成；N1a 的 take/drop/dropLast 已合并但 0289 仍 in-progress |
| M3B Map        | 基础 native + ASan/LSan 完成；迭代、词频改用 Map、只读 Map 构造（0291 草案）未做 |
| M4 安全验证    | M4a、M4b 首片完成；macOS、UBSan、外审未做                    |
| M5 度量与分发  | 候选包首片（0272）完成；性能度量和正式发布未做               |
| M6 线程        | 未开始                                                       |

## 下一步建议

1. **收口 0289**：先确认 `6667635` 之后 main 的 CI 是绿的。再补三类证据：更广的 CFG（含 `for` 的 break/return 清理）、carrier 函数里的二元算术、String/资源的实际 allocation/free 身份。然后归档。
2. **做一次文档对账**：只改文档，刷新上面那四个版本戳和两个过期账本（evolution-status、roadmap）。
3. **功能上**按 r3.1 的顺序推进：显式 consume 基础和复制能力 → N1b/filter。
4. **M1B-b 需要你先拍板**：Str、长度/切片/格式化这些 String API 要先做 proposal，再启用 Guide 增量，不能直接实现。
5. **过程上**：以后每个 Spec 一个 PR，并给 R1 补上 `when`、短路和提前退出的控制流回归。