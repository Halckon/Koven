# 示范程序与跨仓库实施前置矩阵

> **性质**：实现前置盘点 · **状态**：current（有界快照） · **读取时机**：为 M0-S5／M0-S6 和三个示范程序选择下一切片时 · **唯一真源**：本页的依赖映射；语义、实现与验收分别以链接来源为准

## 输入与判定边界

2026-10-23 核查主干 `29923839726734b9d7c0804cf370e5edb969fc81`，现行 Guide v0.45。
本次读取源码、Guide、Spec 及既有运行记录，没有重新执行整套 native 或浏览器测试。
“已有有界证据”仅表示所列入口和场景；不得外推到所有泛型、所有权模式或新 API。
候选合同已获用户批准，但 [文本／IO 候选](../../proposals/text-input-and-io.md) 尚非现行 Guide。
v0.46 由另一实施工作负责，本矩阵不启用、不实现或重写其合同。

## M0-S5：能力、入口与后继

| 能力 | 当前状态与直接来源 | 需要先闭合的事项／使用方 |
|---|---|---|
| argv 与进程入口 | Guide13 规定 `main(): Unit` 或 `main(args: Array<String>): Unit`；[0274](../../archive/specs/0274-argv-word-frequency.md)有 argv 应用证据 | 可复用参数输入；不能据此提供文件读取或 Result main。三个工具均需入口与错误退出合同 |
| String 核心 | [Guide13](../../guide/13-program-runtime-standard-library.md)只发布拼接、clone、相等与 println 等封闭操作 | byteLength、解码、切片、分行、split、数字解析及格式化须先进入 Guide，再定普通 single/project 正反例；日志与 CSV 均依赖这些能力 |
| Result 数据值 | [prelude](../../../crates/lang-std/koven/prelude.ko)有 enum 与 bootstrapPairResult；[bootstrap 测试](../../../crates/lang-cli/src/bootstrap_tests.rs)覆盖其有界入口 | 声明或 bootstrap 成功不证明自动传播、must-use、Display 或 Result main。恢复错误／退出须与 v0.46 责任方协调，不另造退出函数 |
| Map 聚合 | [0295](../../archive/specs/0295-map-argv-word-frequency.md)记录 argv 词频的双入口有界证据；[0291](../../specs/active/0291-readonly-map-construction.md)、[0333](../../specs/active/0333-static-map-callers.md)仍有独立范围 | 复用已验收调用形状，不能将 active 合同或存在编号当完成；日志聚合还依赖文本、IO、溢出与输出合同 |
| 文件、路径与目录 | [Guide13](../../guide/13-program-runtime-standard-library.md)限定同步阻塞模型；标准库源码资产目前为 prelude 与 ranges | 模型不是可调用 API。按已选候选明确路径编码、打开／读写／枚举、close、flush 与失败清理后再立实现 Spec；三个工具均受此阻塞 |
| 用户测试工具 | [教程 runner](../../../scripts/check_tutorial.py)已有源码／命令／stdout／stderr／退出状态 oracle；不是用户项目测试发现器 | M1-S4 必须另定发现、筛选、断言、失败位置、机器结果和零测试行为；复用现有执行合同，不追认已有 runner 为完整用户测试工具 |
| 普通编译入口 | [0341](../../specs/active/0341-shared-lowering-core.md)仍为 in-progress | 新能力必须核实际 single/project 路径；helper 或局部测试通过不等于共用管线整体完成 |
| 安装与版本切换 | [0345](../../archive/specs/0345-preview-version-manager.md)及 PR128 完成双宿主真实 parameter-report 消费 | 可复用安装管理；包内示例不是日志、目录或 CSV 工具，三工具最终安装验收仍待 M1-S5–S7 |

三个场景的成功、失败和资源要求继续由 [API 使用草案](api-use-cases.md)维护。
日志先闭合文本／解析与 IO，再聚合；目录先闭合路径／枚举和稳定排序；CSV 先闭合文本／IO、
已批准的 CSV 子集与行列错误。三者均依赖用户测试和可恢复失败退出，不通过降低失败验收绕开前置。

## M0-S6：不同身份不能混成兼容结论

| 输入／链路 | 已验证范围 | 尚不能证明 |
|---|---|---|
| 站点 `70a6bc8631bbdfa1479e590b584ba034b3857e3e`，内容 `acfacca31a5f43d9999c018bcb1f29d3bd786c6f` | [站点 SPEC-0002](https://github.com/koven-lang/koven-site/blob/70a6bc8631bbdfa1479e590b584ba034b3857e3e/docs/archive/specs/0002.md)核配置、checkout、声明身份 | 固定设计快照不是发布工具链，不能证明该快照全部语言能力可执行 |
| main CI → 通知 → 站点 build/deploy | [0344](../../archive/specs/0344-site-content-sync.md)保存 source SHA、通知及实际部署证据 | 通知 SHA 与站点配置的内容 SHA 可以不同；重建成功不表示消费了 main 或通过工具链兼容测试 |
| preview 源码／target／宿主 | [0345](../../archive/specs/0345-preview-version-manager.md)记录精确输入及双平台消费 | 不自动对应网站 v0.46 设计快照或插件输入；不能拼接不同提交测试结果声称共同兼容 |

后继联合验收需先选真实工具链及其 Guide、网站与插件固定输入，逐场景记录命令与结果。
网站保留现有快照；只有既有授权条件满足后才能切换。编辑器输入和能力需在实施时重新读取，
不把 [冻结基线](baseline.md)中的旧 lock 当作当前验收组合。

## 下一步与完成边界

本矩阵完成“缺口对应哪份已有合同、哪些前置不能跳过”的盘点；M0 尚不 completed。
优先协调已批准文本／IO 的 Guide 迁移；迁移前可细化有界测试合同或补充事实证据，
不能增加候选 public API。Guide 迁移交付后，按 M1-S1→S2→S3 与 M1-S4 的依赖组织实现。
跨仓库兼容组合须独立真实运行，本页不以文档门禁替代该证据。
