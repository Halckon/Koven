# Map 双分量借用遍历当前边界

> **性质**：当前实现事实 · **状态**：current · **读取时机**：选择 Map 借用迭代来源和退出组合时 · **唯一真源**：代码与测试；验收见 [SPEC-0331](../specs/active/0331-map-borrowed-iteration.md)

[SPEC-0331](../specs/active/0331-map-borrowed-iteration.md)仍在实施；以下为固定快照
`60512fda0677b07abb77f10d88242f7a16edd1fb` 的有限支持，不改写 Guide 的来源资格。
`for ((key, value) in source)`沿插入链取得两个独立 Shared loan，不构造 Pair 或 entry owner。
source 求值、分量与 source 清理由已验证事实交付，SSA verifier 后再生成 LLVM。

| 已验来源与类型 | 控制流范围 | 普通证据 |
|---|---|---|
| own Map / Borrow MutableMap 参数；String/String | 正常耗尽、无条件 continue/break/return、有界条件退出 | `map-provider-0331`、`map-provider-exits-0331`、`map-provider-conditional-0331` |
| Borrow Map / MutableMap；String/String | 同根两层共享遍历，以及固定内层无条件 continue/break/return | `map-provider-nested-0331`、`map-provider-nested-exits-0331` |
| 零参 Owned factory temporary Map / MutableMap；String/String | 正常耗尽、固定尾 continue/break/return，hidden owner 保活至 provider 结束 | `map-provider-temporary-0331`、`map-provider-temporary-exits-0331` |
| factory 初始化的 owned local Map / MutableMap，val/var；String/String | 正常耗尽；固定 A/B 形状中退出即清理，或保活至后续 size 查询 | `map-provider-local-sources-0331` |
| Borrow Map / MutableMap 参数；Int/String | single/basic/const 正常耗尽及 whole-set 资格 | `map_iteration_stage_gate_tests`，只有 SSA/LLVM，不计新增 native |

上述 fixture 目录均位于 `crates/lang-codegen/tests/fixtures/`；普通 single/basic native
在该固定快照的本机复验为16项测试、56个 case×entry 产物，每个硬核 exit0、完整 golden
及空 stderr。另15项普通 pipeline/身份测试保留独立 FE/SSA 查询并通过 LLVM verifier。
具体命令及输入身份见0331本机账本；stdout 不证明精确动态分配/释放次数或无泄漏。
local A/B 的独立全实体/全 incoming 深查询仍待，不用 collector 取代。

Group 仍由真实源码阶段门精确拒绝；Field/Inout、direct Borrow-return continuation、
更一般 source/CFG、nullable/Resource K/V 组合不由本表开放。body-local Guard 的析构
正例不等于 Map 的 Resource 分量支持。Map Borrow 的直接退出扩展及 Unit 闭包赋值修复
属于尚未纳入该快照的后续源码；本表不据云端摘要宣称支持。私有损坏事实与故障对照仅在
GitHub Actions 执行，本次本机未运行；macOS sanitizer 合同仍为 partial。

