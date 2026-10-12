# 验收证据保留索引

> **性质**：历史证据导航 · **状态**：current · **读取时机**：复核旧验收或决定新产物是否入库时 · **唯一真源**：保留规则见测试文档；旧事实见固定快照

当前目录只保留短索引。规则见[证据保留](../testing.md#证据保留)，本次范围见
[SPEC-0302](../../specs/active/0302-evidence-retention.md)。测试输入迁至所属 crate，
生成日志、完整 JSON 投影、重复源码/CI 下载包与一次性冻结工具退出当前树。

全部旧材料原样保留在[清理前提交 `8c6907c238`](https://github.com/Halckon/Koven/tree/8c6907c238cc0e040bed4541203e450a5f53552b/docs/development/evidence/)。
这是 checkout 与后续 diff 精简，不是 Git 历史或已有 pack 瘦身。原验收数字、失败、
P2 预算待接受和平台限制不因清理改变；旧 CI artifact 可能到期，Git 快照仍可恢复。

## 按证据族复核

- [借用访问与 v0.41 来源快照](borrow-access-0288/README.md)
- [借用闭包 escape 的红绿、冻结源码与双宿主交付](closure-escape-0278/README.md)
- [固定主干文档状态核对](documentation-status-sync-20261004/README.md)
- [资源程序最终静态审阅](generated-owners-0269-final-review-20261005/README.md)
- [资源程序实现与原始 CI 审阅](generated-owners-0269-review-20261005/README.md)
- [资源程序普通修复验证](generated-owners-0269-safe-fix-20261005/README.md)
- [普通 unit 泛型体归一化与 initial/final/main CI](generic-body-0276/README.md)
- [普通 unit 泛型容器签名与双宿主 CI](generic-containers-0275/README.md)
- [编辑器能力与接线核对](m0-editor-20261004/README.md)
- [Map.put 控制流 return 的有界交付](map-put-return-0294/README.md)
- [两遍 argv Map 词频应用与已知边界](map-word-frequency-0295/README.md)
- [owned nullable 最后判空、临时结果与边界](owned-null-cleanup-0296/README.md)
- [Linux P2 测量移交与后续 CI 对齐](p2-linux-20261005/README.md)
- [macOS LSP 测量协议、样本与 raw](p2-lsp-mac-20261004/README.md)
- [macOS multifile ownership 成本测量](p2-multifile-ownership-mac-20261004/README.md)
- [macOS multifile ownership 成本复测](p2-multifile-ownership-retest-mac-20261004/README.md)
- [PR37 CI 静态复核](pr37-ci/README.md)
- [PR69 回归修复与交付](pr69-repair-0290/README.md)
- [N1a range carrier、receiver 与计数续验](range-carrier-0289/README.md)
- [运行时长度容器、helper/callable 与源码 ABI](runtime-constructor-0279/README.md)
- [SPEC-0271 macOS 本地验收](spec-0271-mac-20261004/README.md)
- [教程门禁验收快照](tutorial-0262/README.md)
- [argv 词频原始字节合同验收](word-frequency-0274/README.md)

## 一次性迁移与交付侧车

下列工具及其输入以固定快照整组保留，不作为当前源码的滚动测试。
复核时使用相应历史工作树；不得以旧脚本失效或旧通过记录推断当前结果。

- [check_iteration_production_migration.py](https://github.com/Halckon/Koven/blob/8c6907c238cc0e040bed4541203e450a5f53552b/docs/development/evidence/check_iteration_production_migration.py)
- [const-owned-unit-handoff-measurement.json](https://github.com/Halckon/Koven/blob/8c6907c238cc0e040bed4541203e450a5f53552b/docs/development/evidence/const-owned-unit-handoff-measurement.json)
- [drop-iteration-test-migration.json](https://github.com/Halckon/Koven/blob/8c6907c238cc0e040bed4541203e450a5f53552b/docs/development/evidence/drop-iteration-test-migration.json)
- [generated-owners-0269-delivery.json](https://github.com/Halckon/Koven/blob/8c6907c238cc0e040bed4541203e450a5f53552b/docs/development/evidence/generated-owners-0269-delivery.json)
- [generic-body-0276-delivery.json](https://github.com/Halckon/Koven/blob/8c6907c238cc0e040bed4541203e450a5f53552b/docs/development/evidence/generic-body-0276-delivery.json)
- [generic-containers-0275-delivery.json](https://github.com/Halckon/Koven/blob/8c6907c238cc0e040bed4541203e450a5f53552b/docs/development/evidence/generic-containers-0275-delivery.json)
- [generic-containers-0275-public.json](https://github.com/Halckon/Koven/blob/8c6907c238cc0e040bed4541203e450a5f53552b/docs/development/evidence/generic-containers-0275-public.json)
- [iteration-production-migration.json](https://github.com/Halckon/Koven/blob/8c6907c238cc0e040bed4541203e450a5f53552b/docs/development/evidence/iteration-production-migration.json)
- [multifile-ownership-test-migration.json](https://github.com/Halckon/Koven/blob/8c6907c238cc0e040bed4541203e450a5f53552b/docs/development/evidence/multifile-ownership-test-migration.json)
- [multifile-type-test-migration.json](https://github.com/Halckon/Koven/blob/8c6907c238cc0e040bed4541203e450a5f53552b/docs/development/evidence/multifile-type-test-migration.json)
- [owned-unit-handoff-initial-ci.json](https://github.com/Halckon/Koven/blob/8c6907c238cc0e040bed4541203e450a5f53552b/docs/development/evidence/owned-unit-handoff-initial-ci.json)
- [owned-unit-handoff-measurement.json](https://github.com/Halckon/Koven/blob/8c6907c238cc0e040bed4541203e450a5f53552b/docs/development/evidence/owned-unit-handoff-measurement.json)
- [ownership-iteration-test-migration.json](https://github.com/Halckon/Koven/blob/8c6907c238cc0e040bed4541203e450a5f53552b/docs/development/evidence/ownership-iteration-test-migration.json)
- [ownership-planning-milestone.json](https://github.com/Halckon/Koven/blob/8c6907c238cc0e040bed4541203e450a5f53552b/docs/development/evidence/ownership-planning-milestone.json)
- [p2-linux-0277-delivery.json](https://github.com/Halckon/Koven/blob/8c6907c238cc0e040bed4541203e450a5f53552b/docs/development/evidence/p2-linux-0277-delivery.json)
- [pr37-and-p2-static-review.json](https://github.com/Halckon/Koven/blob/8c6907c238cc0e040bed4541203e450a5f53552b/docs/development/evidence/pr37-and-p2-static-review.json)
- [pr37-and-p2-static-review.py](https://github.com/Halckon/Koven/blob/8c6907c238cc0e040bed4541203e450a5f53552b/docs/development/evidence/pr37-and-p2-static-review.py)
- [preview-candidate-0272-ci.json](https://github.com/Halckon/Koven/blob/8c6907c238cc0e040bed4541203e450a5f53552b/docs/development/evidence/preview-candidate-0272-ci.json)
- [publication-redaction.json](https://github.com/Halckon/Koven/blob/8c6907c238cc0e040bed4541203e450a5f53552b/docs/development/evidence/publication-redaction.json)
- [publication-relationship.json](https://github.com/Halckon/Koven/blob/8c6907c238cc0e040bed4541203e450a5f53552b/docs/development/evidence/publication-relationship.json)
- [unit-planner-responsibility-equivalence.json](https://github.com/Halckon/Koven/blob/8c6907c238cc0e040bed4541203e450a5f53552b/docs/development/evidence/unit-planner-responsibility-equivalence.json)
- [unit-planner-responsibility-equivalence.py](https://github.com/Halckon/Koven/blob/8c6907c238cc0e040bed4541203e450a5f53552b/docs/development/evidence/unit-planner-responsibility-equivalence.py)
- [unit-runtime-layout-migration.json](https://github.com/Halckon/Koven/blob/8c6907c238cc0e040bed4541203e450a5f53552b/docs/development/evidence/unit-runtime-layout-migration.json)
- [verify_drop_iteration_migration.py](https://github.com/Halckon/Koven/blob/8c6907c238cc0e040bed4541203e450a5f53552b/docs/development/evidence/verify_drop_iteration_migration.py)
- [verify_multifile_ownership_migration.py](https://github.com/Halckon/Koven/blob/8c6907c238cc0e040bed4541203e450a5f53552b/docs/development/evidence/verify_multifile_ownership_migration.py)
- [verify_multifile_type_migration.py](https://github.com/Halckon/Koven/blob/8c6907c238cc0e040bed4541203e450a5f53552b/docs/development/evidence/verify_multifile_type_migration.py)
- [verify_ownership_iteration_migration.py](https://github.com/Halckon/Koven/blob/8c6907c238cc0e040bed4541203e450a5f53552b/docs/development/evidence/verify_ownership_iteration_migration.py)
- [word-frequency-0274-delivery.json](https://github.com/Halckon/Koven/blob/8c6907c238cc0e040bed4541203e450a5f53552b/docs/development/evidence/word-frequency-0274-delivery.json)

## 最小保留信息

每次验收只记录目标/输入提交、命令或选择、实际结果、未测/失败/平台边界及来源链接。
完整输出留在对应 CI artifact；需要长期保全时先确定存储与保留期限。测试真正读取的
fixture/golden 作为版本化输入保留，不得当作一次性生成日志删除。
