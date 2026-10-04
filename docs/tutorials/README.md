# 当前教程

> **性质**：教程入口 · **状态**：current · **读取时机**：学习可执行的 Koven 示例时 · **唯一真源**：本目录

[当前 Koven tour](koven-tour.md)依据 [Guide v0.40](../guide/README.md)。源码只保存在 Markdown；
`examples.json` 保存完整输出合同，`scripts/check_tutorial.py` 使用真实 CLI 验收。
跨文件合同按路径引用本页 fence ID，自动生成现行 project manifest；源码不在 JSON 重复。
定向验收可重复传入 `--example ID`；CI 默认选择十二个正例和两个负例；parameter-report的四组argv共用一份三文件源码，
每组分别执行build、artifact和run，总计十七组执行合同。新增例由SPEC-0268验收中，
planned不执行；接线与本机编排测试不代表实际CLI已经通过。
历史教程保留在 archive，不代表当前实现。planned 示例不计入通过。
