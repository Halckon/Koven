# 示范程序与跨仓库实施前置矩阵

> **性质**：实现前置盘点 · **状态**：current（有界快照） · **读取时机**：为 M0-S5／M0-S6 和三个示范程序选择下一切片时 · **唯一真源**：本页的依赖映射；语义、实现与验收分别以链接来源为准

## 输入与判定边界

2026-10-29 重新核查主干 `8a0b336cf6d8f29b0a7c8bbf74a36520017679af`，现行 Guide v0.46。
本次读取源码、Guide、Spec、在途 PR 与实际 CI 状态，没有重新执行 native 或浏览器测试。
该精确主干的 [push CI](https://github.com/Halckon/Koven/actions/runs/40865015271) 为 success；
它不替代下列尚未实现能力的直接验收。此前主干 `e764ada0` 的
[CI39957053961](https://github.com/Halckon/Koven/actions/runs/39957053961) 最终 cancelled，不记为通过。
2026-10-23 的 v0.45 前置判断已由本次核查取代；原冻结观察仍见 [baseline](baseline.md)。
“已有有界证据”仅表示所列入口和场景；不得外推到所有泛型、所有权模式或新 API。
用户已批准 [文本／IO 合同](../../archive/migrations/text-io-approved-contract.md#eco-v1-m0-使用合同建议2026-10-22未启用)
及后续 Guide 启用；[SPEC-0366](../../specs/active/0366-text-io-guide.md)在 v0.47 写入文本／IO 与用户补选错误规则。
上列 v0.46 输入身份保留为核查基线；规范迁移不改变下列生产实现观察。

## M0-S5：能力、入口与后继

| 能力 | 当前状态与直接来源 | 需要先闭合的事项／使用方 |
|---|---|---|
| argv 与进程入口 | Guide13 已允许 Result process entry；当前 [CLI 选择器](../../../crates/lang-cli/src/unit_build.rs) 的 `process_entry_shape` 仍只接受 Unit 返回；[0274](../../archive/specs/0274-argv-word-frequency.md)有 argv 应用证据 | 可复用参数输入；不能据此提供文件读取或宣称 Result main 已实现。三个工具仍需生产方错误退出交付 |
| String 核心 | [Guide13](../../guide/13-program-runtime-standard-library.md)已定义核心 String 与 v0.47 文本表面；现有实现证据仍限各自已交付 Spec | byteLength、解码、切片、分行、split、数字解析已由 v0.47 定义，生产实现待交付；格式化沿用 Display 合同，仍需普通 single/project 实现验收；日志与 CSV 均依赖这些能力 |
| Result 数据值 | [0349](../../archive/specs/0349-builtin-result-and-check.md)已交付内建 Result/Ok/Err、subject 推断及 check；[标准源加载](../../../crates/lang-cli/src/standard_sources.rs)按 SourceId 授权 result.ko | 声明或 bootstrap 成功不证明自动传播、must-use、Display 或 Result main。恢复错误／退出沿用已启用 Guide09/13，补实现与直接验收，不另造退出函数 |
| Map 聚合 | [0295](../../archive/specs/0295-map-argv-word-frequency.md)记录 argv 词频的双入口有界证据；[0291](../../specs/active/0291-readonly-map-construction.md)、[0333](../../specs/active/0333-static-map-callers.md)仍有独立范围 | 复用已验收调用形状，不能将 active 合同或存在编号当完成；日志聚合还依赖文本、IO、溢出与输出合同 |
| 文件、路径与目录 | [Guide13](../../guide/13-program-runtime-standard-library.md)限定同步阻塞模型；标准库源码资产为 prelude、ranges 与 core/result | 模型不是可调用 API。路径编码、读写／枚举、close、flush 与失败清理已迁入 v0.47；后继实施仍须正式 Spec；三个工具均受此阻塞 |
| 用户测试工具 | [教程 runner](../../../scripts/check_tutorial.py)已有源码／命令／stdout／stderr／退出状态 oracle；不是用户项目测试发现器 | [0354](../../archive/specs/0354-conformance-corpus.md)的固定种子矩阵也不是用户测试发现器。M1-S4 必须另定发现、筛选、断言、失败位置、机器结果和零测试行为；复用现有执行合同，不追认已有 runner 为完整用户测试工具 |
| 普通编译入口 | [0341](../../specs/active/0341-shared-lowering-core.md)仍为 in-progress | PR220已交付有界 iteration Unit witnesses，PR225/162仍在处理迁移待项；新能力须核实际 single/project 路径，不能从局部退役测试推定整条管线完成 |
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
规范迁移由 SPEC-0366 承接；后继依次落实文本与 IO 实现，也不把尚未获得本生态直接验收的
Display 或仍被入口选择器拒绝的 Result main 当可用。规范迁移与实现分开验收。
Guide 迁移交付后，按 M1-S1→S2→S3 与 M1-S4 的依赖组织实现。
[编译器路线图](../../roadmap/README.md)的 L5/L6 已对齐已批准的字节偏移、显式输入上限及 Result 表面；
未启用的其他 v1 目标继续保留，不能混成第二套标准库 API。
跨仓库兼容组合须独立真实运行，本页不以文档门禁替代该证据。
