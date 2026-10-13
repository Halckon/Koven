# SPEC-0304: Spec 归档库存声明化

> **性质**：变更合同 · **状态**：in-progress · **读取时机**：实现或验收归档库存声明时 · **唯一真源**：本 Spec

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P6-0304` |
| 所属 Phase | Phase 6 交付门禁 |
| 语言规范 | 现行 [Guide](../../guide/README.md)，不改变语言行为 |
| 批准依据 | 2026-10-13 用户明确同意将归档编号抽为严格文档声明，并保持现有 CI 安全边界 |
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
| 原库存与新声明逐 ID 比较 | 通过：287 个 archived 逐 ID 相等，无增删；active 由 0 增加本 Spec 0304 | base `b63ec6a1`；draft/ADR 常量保持原值 |
| `test_spec_inventory.py`：声明、文件输入、生命周期、索引与 DAG 正反例 | 24/24 通过 | 普通临时文档 fixture；含 JSON 非法空白、目录子树及 FIFO 拒绝 |
| `test_check_docs.py`：既有文档合同 | 41/41 通过 | fixture 显式提供独立库存，保留缺失、done 冻结与原断言 |
| `test_pr_ci_reuse.py`：真实 Git 归档/库存输入与拒绝边界 | 26/26 通过 | 归档保 head/merge 指纹；checker、mode、扩展名、Guide、base、merge-only 代码变化不复用 |
| `test_check_ci_results.py`：docs/尺寸/依赖必需与复用独立验证 | 32/32 通过 | 无效声明→真实 docs error→已验证 reuse 仍因 docs failure 失败 |
| `test_gen_spec_dag.py`、`python3 scripts/check_docs.py`、`git diff --check` | 1/1、655 Markdown、完整 base diff 通过 | 两次重建 DAG 四产物字节不变，Python 编译检查通过 |
| 首次 PR 物理代码 CI | 待远端验收 | 必需双宿主与适用 preview jobs 实际成功，不以本地 Python 代替 |
| 同 PR 纯归档 head 复用 | 待远端验收 | source run/attempt/base/head/merge 指纹逐项核，docs 必须成功 |
| 最终 PR checks / merge / actual-main CI | 待远端验收 | 无 required pending；main 必需代码 jobs实际运行 |

## 5. 本地验证身份与边界

2026-10-13，在 Linux / Python 3.12.14 对本地代码输入
`24460e6f3a67f459fe00b13b39232edf62cb4bda`、tree
`861c79e508ef9b6e1452ffbfad585f0a2fb749c4` 执行上表五个精确普通文件，共124项通过。
各单测命令均为 `python3 -m unittest discover -s scripts/tests -p <表中文件名> -v`；
没有执行全 scripts 测试集合或其他驱动器。两次非作者只读审查已完成；独立复跑亦124/124、
docs655和diff通过。此后的验收记录追加只改本 Spec 文档。

- `5957539e` 的首次声明替代测试真实失败：旧 checker 仍按代码中的库存判定；实施后转绿。
- 审查识别的非 JSON 空白被 `splitlines()` 正常化、无效库存目录子文档提前读取两例，均先
  保留普通红测再修复。现按真实换行解析，并排除非法声明目录子树。
- 新增既有 docs fixture 初次缺四项 metadata，补齐后原41项通过。深层 JSON 初测对诊断文字
  断言过窄，改为单一结构错误断言；本机2000层由 schema 拒绝，不声称命中 decoder 递归上限。
- `pr_ci_reuse.py`、`check_ci_results.py`、workflow、DAG生成器相对base字节不变。
  本地真实 Git fixture 只证明输入指纹与门禁合同，不是远端 PR 物理复用验收。
- 本地未运行 Rust/Cargo、故障注入、校准、sanitizer、reducer 或 aggregate drivers；
  未执行 GitHub 推送、创建 PR、合并。远端物理 CI、归档复用与actual-main仍待单独验收。

## 6. 实施与交付

1. 批准合同与编号登记；先建立普通失败 fixture，再实施严格 parser 与等价库存迁移。
2. 运行表中窄测试与结构门禁，独立审查 diff，维护准确本地记录。
3. 经授权发布后，先取得物理成功 PR 证据，再单独归档本 Spec 并验收真实复用。
4. 归档不代表最终 head、merge 或 actual-main 已成功，各阶段分别记录。

本地提交按合同、失败测试、最小实现、验收记录分开；所有提交保持单一 Goal。
后续是否具备复用条件由原算法判定，不承诺固定省时或保证每次归档均复用。
