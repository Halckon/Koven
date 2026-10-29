# 文本与 IO 后继候选

> **性质**：非规范后继候选 · **状态**：current（已批准表面已迁入 v0.47） · **读取时机**：为文本／IO 后继能力立项时 · **唯一真源**：本页的未启用范围；已启用语义见 Guide

2026-10-22 已批准的字节文本、解析、便利文件、目录路径与标准流合同已迁入
[Guide13](../guide/13-program-runtime-standard-library.md#文本字节与严格解析)。
2026-10-29 用户补选的三个错误 oracle 一并启用；详见
[启用账本](../archive/migrations/v0.47-enablement.md)与[原批准材料](../archive/migrations/text-io-approved-contract.md)。
规范启用不证明任何生产 API 已实现；实现入口见 [SPEC-0366](../specs/active/0366-text-io-guide.md)。

## 仍待后继合同

- 显式流 owner、close 幂等性、drop 尽力清理与不可观测错误；不能从便利函数或 RAII 推导 close 必成功。
- Char 迭代、任意索引、trim、StringBuilder、未列出的文本 API；不能把旧路线图摘要当公共签名。
- exitProcess、time/sleep/math、网络接口及其他 v1 目标分别立项，不因三个程序暂不需要而删除。
- normalization、locale/case folding、正则、异步 IO、Str、公开 C FFI 均未由文本／IO 合同授权。

三个示范程序的格式、产品容量、成功与失败 oracle 保留在
[原批准材料](../archive/migrations/text-io-approved-contract.md#三个程序的确定性-oracle-建议)，
由 [生态工作项](../development/ecosystem/M1.md)和各自实现 Spec 承接，不当成语言规则。
