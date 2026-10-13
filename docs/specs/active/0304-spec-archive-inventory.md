# SPEC-0304: Spec 归档库存声明化

> **性质**：变更合同 · **状态**：approved · **读取时机**：实现或验收归档库存声明时 · **唯一真源**：本 Spec

| 字段 | 值 |
|---|---|
| 状态 | approved |
| Goal ID | `KOV-P6-0304` |
| 所属 Phase | Phase 6 交付门禁 |
| 语言规范 | 现行 [Guide](../../guide/README.md)，不改变语言行为 |
| 批准依据 | 2026-10-13 用户同意先做严格文档库存抽取，再继续标准库合同收敛 |
| 前置 Spec | [SPEC-0293](../../archive/specs/0293-pr-code-ci-reuse.md) |
| 前置 ADR | 无 |
| 关联 ADR | 无新增长期架构决定；沿用既有文档门禁与 CI 证据合同 |
| 阻塞项 | 首次物理 PR CI、纯归档复用、最终 PR checks 与 actual-main CI 待远端验收 |
| 影响范围 | docs checker、普通 Python fixture、Spec 库存与当前开发指令 |
| 语言语义变更 | 否 |

## 1. Goal

active→archive 仅改变 Spec 生命周期文档和严格声明的库存数据，不再为维护编号修改
`scripts/check_docs.py`；满足原同 PR、同 base、同代码输入的物理成功证据条件时，
归档提交可复用代码 CI，当前 docs 门禁仍实际执行并失败关闭。

基于实际 main `b63ec6a1254c561e4dacae510a9769cc99b304ba`、tree
`dbbbbbd944591006401e4ab55ce3a7a562897617` 创建 `feature/spec-0304`。
PR79/80/81 归档修改 checker 的纯 ID 集合，属于 SPEC-0293 既有代码输入回退边界。

## 2. 范围与合同

- 将原 active/archive 两个冻结集合逐 ID 迁至固定 `docs/specs/inventory.md`，不从文件扫描
  或导航链接推导 expected；draft 按版本库存与 ADR 库存保留原代码真源。
- 文档只承载一个严格 JSON fenced block，字段恰为 `active`、`archived` 两个字符串列表；
  ID 为四位 ASCII 数字，禁止重复键、重复 ID、集合相交、未知字段或非标准 JSON。
- 在通用 Markdown 读取之前检查固定路径；路径组件不得是 symlink，终点必须为无执行位的
  普通文件。缺失、不可读、损坏 UTF-8、无效声明均产生失败，不能以空集合、旧常量或扫描结果回退。
- 缺失/重复/未闭合/错误 fenced block 必须失败；声明不能提供代码、外部 include、路径规则、
  宽松开关或自定义状态规则。
- 保持实际集合与声明集合的双向缺失/未登记检查、全生命周期编号唯一、标题编号、状态/目录
  检查；冻结 archive 的 `done` 要求仍由 checker 固定。索引、链接与 DAG 新鲜度仍独立检查。
- CI workflow、指纹算法、路径白名单、物理证据身份及最终独立复核不改；checker/脚本修改仍
  是代码输入，base 改变或证据不可用仍保守重跑。main/manual 全物理门禁保持。
- 同步现行归档指令指向新库存；历史 Spec 与原始验收结论不改写。编译器 Architecture 无变化。

## 3. 非目标

不迁移 drafts/ADR，不放宽任意 `.py`/`.json` 路径，不伪造 reuse 或 success，不修改 required
checks 设置，不跨 PR 复用，不增加依赖，不改 Rust/Guide/ABI。此次本地不运行 Cargo、
故障注入、校准、sanitizer、reducer 或 aggregate drivers；普通 JSON/Markdown 拒绝输入和
临时 Git fixture 属于直接解析与证据合同测试。

## 4. 验收账本

原始日志与恢复材料保留仓库外；本表只记录简要输入、选择、结果与未运行项。

| 验收项 / 命令或精确选择 | 结果 | 输入 / 边界 |
|---|---|---|
| 原库存与新声明逐 ID 比较 | 未运行 | 以真实 base 的常量为迁移基线，新增本 Spec 单独列明 |
| `test_spec_inventory.py`：有效声明、错误 schema/fence/文件输入与生命周期正反例 | 未运行 | 普通临时文档 fixture，不调用故障驱动器 |
| `test_check_docs.py`：既有文档合同 | 未运行 | fixture 必须显式提供独立库存，不以入口失败遮蔽原断言 |
| `test_pr_ci_reuse.py`：真实 Git 归档/库存输入与拒绝边界 | 未运行 | 同 head/merge 有效树，checker/mode/base 改变不得复用 |
| `test_check_ci_results.py`：docs/尺寸/依赖必需与复用独立验证 | 未运行 | 普通 metadata/静态接线 fixture |
| `test_gen_spec_dag.py`、`python3 scripts/check_docs.py`、`git diff --check` | 未运行 | 当前状态、DAG、索引与链接 |
| 首次 PR 物理代码 CI | 待远端验收 | 必需双宿主与适用 preview jobs 实际成功，不以本地 Python 代替 |
| 同 PR 纯归档 head 复用 | 待远端验收 | source run/attempt/base/head/merge 指纹逐项核，docs 必須成功 |
| 最终 PR checks / merge / actual-main CI | 待远端验收 | 无 required pending；main 必需代码 jobs实际运行 |

## 5. 实施与交付

1. 批准合同与编号登记；先建立普通失败 fixture，再实施严格 parser 与等价库存迁移。
2. 运行表中窄测试与结构门禁，独立审查 diff，维护准确本地记录。
3. 经授权发布后，先取得物理成功 PR 证据，再单独归档本 Spec 并验收真实复用。
4. 归档不代表最终 head、merge 或 actual-main 已成功，各阶段分别记录。

本地提交按合同、失败测试、最小实现、验收记录分开；所有提交保持单一 Goal。
后续是否具备复用条件由原算法判定，不承诺固定省时或保证每次归档均复用。
