# SPEC-0368: 可信文本扩展与 UTF-8 字节往返

> **性质**：实现合同 · **状态**：in-progress · **读取时机**：实现或验收文本字节基础时 · **唯一真源**：本 Spec 的交付边界；语言语义以 Guide 为准

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P4-0368` |
| 所属 Phase | Phase 2、3、4、5 |
| 主里程碑 | CORE-V1-M1 / ECO-V1-M1，M1-S1 基础实现 |
| 语言规范 | [Guide v0.47 文本合同](../../guide/13-program-runtime-standard-library.md#文本字节与严格解析)、[受限标准扩展](../../guide/05-declarations-callables.md) |
| 前置 Spec | [0367 String 长度不变量](../../archive/specs/0367-string-length-invariant.md)、[0349 Result](../../archive/specs/0349-builtin-result-and-check.md) |
| 批准依据 | [生态持续实施授权](../../development/ecosystem/governance.md#立项与依赖)，现行 Guide 已启用本片表面 |
| 关联 ADR | [0018 String ABI](../../adr/accepted/0018-string-owner-runtime-abi.md) |
| 语言语义变更 | 否 |
| 阻塞项 | 无语义阻塞；实现与动态验收未完成 |

## 1. Goal 与输入身份

从 main `da5d93013125974fa8d736a541aee3ab727407c7` 建立
`feature/spec-0368-text-byte-foundation`。让单文件与 project 公开 CLI 通过显式导入使用
UTF-8 字节长度、独立字节副本、严格解码、按字节切片与可恢复连接，保持调用期借用和独立返回 owner。
不能以 helper、SSA 单测或仅一种入口代替真实公共调用的 build/run 验收。

## 2. 范围与实现边界

- 标准源码资产固定为 `crates/lang-std/koven/text/text.ko`，包为 `koven.text`；宿主按实际
  SourceId 授权，不从包名、文件名、路径或源码文本推断可信身份。
- 实现 `String.byteLength`、`String.toBytes`、`decodeUtf8`、`String.sliceBytes`、
  `String.checkedPlus` 及 Guide 定义的 TextError。签名、错误优先级和偏移口径以 Guide 为准。
- 扩展选择复用 package/import 可见性和 canonical callable 身份；String 扩展只接受可信
  标准源、Borrow receiver 和普通 owned 结果，不能借用 N1a 的 carrier/from 合同冒充。
- 标准源承载公共函数及可表达的范围检查、错误构造和长度策略；受控 runtime 只补存储读取、
  UTF-8 检验和复制所需边界。先核对并复用既有容器生成、String 连接／分配和 argv UTF-8 扫描。
- typed 产物明确发布调用及 receiver 事实；ownership、SSA/verifier、LLVM 消费这些事实，
  不从函数拼写或 AST 重新猜测语义。两前端入口与两 CLI 模式不得出现准入／清理分歧。
- 返回 String／Array 均独立持有数据；输入保持有效。保持 Int 长度不变量、OOM Abort、
  空值／空缓冲行为、UTF-8 与内嵌 NUL，不以 lossy decoding 或截断换取成功。

## 3. 非目标与后继

本片不交付 split／lines 的业务扫描、parseLong／格式化、IO、任意用户扩展或公共 FFI。
TextError 按现行合同保持完整分类；EmptySeparator 的实际产生路径随 split 后继交付，不能
用占位 split 函数伪装能力。本片归档后 M1-S1 仍须落实 split／lines，M1-S2 与 M1-S3 仍开放。
不改变 clone intrinsic、现有 Range Borrow 返回合同、用户同名声明语义或未启用语言能力。

## 4. 验收矩阵

| 主题 | 必须证明的结果 |
|---|---|
| 来源与导入 | 显式导入成功；未导入、伪造同包／同路径／同名源码不能取得标准能力；普通同名 callable 不被劫持 |
| 扩展签名 | 非标准源、错误 receiver／模式／结果、carrier/from 签名被拒绝；N1a 与 clone 回归不变 |
| 字节长度与复制 | 空串、ASCII、中文、多字节 scalar、NUL 长度与全部字节准确；副本不别名输入，销毁顺序独立 |
| 严格解码 | 合法边界及完整往返；孤立 continuation、非法 lead、overlong、surrogate、超 Unicode 范围、截断拒绝 |
| 错误偏移 | 有效前缀后的非法 continuation／截断报告序列起始字节；首错稳定，不按 Unicode 字符计数 |
| 字节切片 | 负数、反向、越界为 InvalidRange；有效范围的非 scalar 边界为 NotScalarBoundary；边界空切片有效 |
| 可恢复连接 | 两输入各求值一次、保持 owner；正常结果独立；超限仅返回 null；OOM 仍 Abort，不先复制超额输入 |
| 所有权与控制流 | 借用命名值／字段／临时值、结果早退、错误分支与容器持有结果均正确清理；输入后续可用 |
| 公共入口 | 单文件/project 的真实 import、check、build/run 与输出／退出码一致，不能只测手工注入标准源 |
| 回归与平台 | 双宿主官方 CI、既有 String／argv／Range／Result 行为、尺寸与文档门禁、独立审查 |

长度上限用合成长度和受控分配器探针覆盖，不真实分配 GB 缓冲；明确区分守卫证据与正常
小输入的真实编译运行。运行失败、被忽略或未运行项单独记录，不作通过计数。

## 5. 实施顺序

1. [ ] 盘点可信来源、扩展选择、ownership 与 native 接入点及直接调用者，确认既有能力和缺口。
2. [ ] 建立失败用例；接通标准源／TextError 和精确 String 扩展事实，验证防冒充及原有扩展回归。
3. [ ] 补充最小字节读取、严格 UTF-8 与复制边界；用 Koven 标准源组合公共接口，验证字节往返与错误偏移。
4. [ ] 验证切片和 checkedPlus 的边界、求值次数、OOM 与 owner 生命周期，补齐两种公开 CLI 入口。
5. [ ] 独立审查共享逻辑与最大剩余风险，修复并复核；同步实现事实、官方 CI 与验收账本。
6. [ ] 验收通过后归档、同步库存和里程碑；最终 head 全部适用检查结束后合并 PR。

## 6. 当前证据

已编写来源授权、byteLength／toBytes／checkedPlus 的两套前端签名绑定与 canonical
扩展候选选择，以及来源防冒充、签名拒绝、owned 结果、导入／alias／遮蔽用例。
unit 已为 sliceBytes 使用内建 Result 的 canonical 声明及授权文本源的 TextError enum 身份，
并补充未授权 Result／外部同名 TextError 的拒绝用例；旧单文件 checker 尚无同等 Result 身份接线。
此部分尚未编译或执行测试；标准源加载、公共函数、完整的双入口结果身份、
ownership 至 native 链路均未完成，不能据此宣称公开 API 可用。
独立静态审查覆盖授权身份、签名限制、候选选择、this 与构造点，未发现确认缺陷；
提出的跨文件覆盖缺口已补测试源码，仍待动态验证；复核发现顶层 inout 扩展先被 Parser 拒绝，
已从要求解析成功的类型测试数组移除该错误夹具。文档结构检查（821 篇）与 diff 检查通过。
前置 SPEC-0367 已由 PR #233 合入上述 main；其证据不能替代本片接口、来源或运行验收。
后续逐项记录精确输入提交、命令、实际结果和稳定 CI 来源，原始日志保留在仓库外。

首片提交 `2a749117cf4e5cf429f51d032baef6bb930eeacf` 的
[PR #235 CI](https://github.com/Halckon/Koven/actions/runs/40926038826) 在文档／CI 规则测试失败：
新增 `type_text_extension` 后精确选择数量应从126变为127，旧清单断言未同步。
464项规则测试中该项失败；Rust 编译与运行作业被跳过，不能视作前端通过。
已同步精确数量并增加新目标只出现一次的断言，等待修复提交的官方验证。
