# Active Specs

> **性质**：当前变更索引 · **状态**：current · **读取时机**：查找已批准或实施中的 Goal 时 · **唯一真源**：本目录 Spec

当前 0 份 active。

0308–0316及0307的当前归档、PR合并与main CI状态统一见[近期交付](../README.md#近期交付)；
本页不再维护重复的待验清单。原验收、失败与非目标仍保留在各历史Spec中。
本轮剩余范围与终验条件见[本轮 M2B 实用化收尾](../README.md#本轮-m2b-实用化收尾)。

[0306 混合 Value 参数的唯一来源借用返回](../../archive/specs/0306-mixed-value-borrow-result.md)已按PR84修复后完整物理CI及双宿主各15项正常/10项verifier测试有界归档；首失败保留，已由[PR84](https://github.com/Halckon/Koven/pull/84)合为actual-main `967bf98e`，其[main CI](https://github.com/Halckon/Koven/actions/runs/38368833991)成功；普通同步Borrow实参由0307关闭，其他M2B后继继续开放。

[0305 多Borrow参数的唯一来源直接返回](../../archive/specs/0305-multi-parameter-borrow-result.md)已按PR83首次物理CI及双宿主34项普通测试有界归档；最终归档head复用已验收并由[PR83](https://github.com/Halckon/Koven/pull/83)合入main `bfdb00bd`；[main CI](https://github.com/Halckon/Koven/actions/runs/38350665268)已成功，其余M2B后继开放。

[0304 Spec 归档库存声明化](../../archive/specs/0304-spec-archive-inventory.md)已按PR82首次物理CI 16/16 jobs成功归档；严格声明、docs每次门禁与原代码证据边界保持，已由[PR82](https://github.com/Halckon/Koven/pull/82)合入main `f7a4d15c`，[main CI](https://github.com/Halckon/Koven/actions/runs/38338171407)成功。

[0298 范围绑定与有界局部控制流](../../archive/specs/0298-range-binding-local-control-flow.md)已按PR81修复后实现head双宿主52/8/14/35逐目标证据及完整CI有界归档；只关闭Int ==/<与同根词法if首片，[广义后继](../drafts/v0.43/README.md#0298首片之外的开放后继)继续开放；已由[PR81](https://github.com/Halckon/Koven/pull/81)合入main `b63ec6a1`，[main CI](https://github.com/Halckon/Koven/actions/runs/38327500616)成功。

[0299 owned Map删除结果的非消费借用](../../archive/specs/0299-owned-map-result-borrow.md)已按PR80实现head双宿主各23项新增测试及完整CI有界归档，仅关闭冻结首片；嵌套调用等广义后继保持开放；已由[PR80](https://github.com/Halckon/Koven/pull/80)合入main `4eb38d74`，[main CI](https://github.com/Halckon/Koven/actions/runs/38275520368)成功。

[0300 single Map-for条件清理运输](../../archive/specs/0300-single-map-loop-owner-carry.md)已按PR79实现head双宿主各19项新增测试及完整CI有界归档；只覆盖正常耗尽；已由[PR79](https://github.com/Halckon/Koven/pull/79)合入main `08c6b8dc`，[main CI](https://github.com/Halckon/Koven/actions/runs/38259510177)成功。

[0297 Map key return清理](../../archive/specs/0297-map-key-return-cleanup.md)已按PR78精确实现head双宿主各42项新增测试及完整CI有界归档；已由[PR78](https://github.com/Halckon/Koven/pull/78)合入main `003f1299`，[main CI](https://github.com/Halckon/Koven/actions/runs/38228159089)成功。

[SPEC-0303](../../archive/specs/0303-development-document-ownership.md)已按PR77有界归档并合入main `289a8ad9`；合并后CI [38219409371](https://github.com/Halckon/Koven/actions/runs/38219409371)已成功。

[0302证据保留](../../archive/specs/0302-evidence-retention.md)已按[PR76](https://github.com/Halckon/Koven/pull/76)有界归档并合入main `8ef058f6`，[main CI](https://github.com/Halckon/Koven/actions/runs/38191208630)成功。

0296 owned nullable具名/临时结果cleanup已按[PR75](https://github.com/Halckon/Koven/pull/75)实现head
双宿主每host55项codegen与25项Map frontend有界验收归档，已合为main `8c6907c2`；
actual-main CI [38150175528](https://github.com/Halckon/Koven/actions/runs/38150175528)已成功。
[0296原账本](https://github.com/Halckon/Koven/blob/8c6907c238cc0e040bed4541203e450a5f53552b/docs/development/evidence/owned-null-cleanup-0296/delivery.json)保留当时检查点，
[最新整合记录](../../archive/migrations/pr72-74-followup-audit.md#基线与交付)不扩大single for或普通nullable工厂边界。

0295 两遍 argv Map 词频应用已按 [PR74](https://github.com/Halckon/Koven/pull/74) 实现 head
双宿主各 40 条公开命令及通用回归有界归档；已合为main `e168c485`，actual-main CI成功，见[最终核对](../../archive/migrations/pr72-74-followup-audit.md)。
[原交付账本](https://github.com/Halckon/Koven/blob/8c6907c238cc0e040bed4541203e450a5f53552b/docs/development/evidence/map-word-frequency-0295/delivery.json)保留当时检查点；只关闭 M3B K-H 应用 Goal。

0294 的 Map.put key/value return 有界修复已按 [PR73](https://github.com/Halckon/Koven/pull/73)
实现 head 双宿主 CI 归档；已合为main `4321e99e`，actual-main CI成功，见[最终核对](../../archive/migrations/pr72-74-followup-audit.md)。
[原交付账本](https://github.com/Halckon/Koven/blob/8c6907c238cc0e040bed4541203e450a5f53552b/docs/development/evidence/map-put-return-0294/delivery.json)保留当时检查点；不扩大一般 CFG 或 loop 退出。

0289 的 N1a 有界 Goal 已按 [PR72](https://github.com/Halckon/Koven/pull/72) 实现 head
双宿主直接证据验收归档；最终归档 head CI 已通过并合为 `3f976798`，actual main CI 已成功，见
[交付账本](https://github.com/Halckon/Koven/blob/8c6907c238cc0e040bed4541203e450a5f53552b/docs/development/evidence/range-carrier-0289/delivery.json)。归档不扩展既有 CFG
能力门，也不启用 consume、通用 Clone 或 N1b。

0290（PR69 交付回归修复）已按 [PR71](https://github.com/Halckon/Koven/pull/71) 实现 head 双宿主 CI 验收归档；最终归档 head、合并 `6667635` 与 actual main CI 已验证，见[交付账本](https://github.com/Halckon/Koven/blob/8c6907c238cc0e040bed4541203e450a5f53552b/docs/development/evidence/pr69-repair-0290/delivery.json)。0289 的实现验收与最终交付独立记录。

0293（PR 普通文档追加提交复用真实代码 CI）已按 [PR #70](https://github.com/Halckon/Koven/pull/70) 首轮真实完整 CI 与追加文档复用验收归档至 `docs/archive/specs/0293-pr-code-ci-reuse.md`；已合并为 `56af80b`，合并后 main CI 单独跟踪。

0288（Map 键值容器原生执行基础：SSA 原语、LLVM IR 代码生成与 Native Runtime 哈希表）已按双宿主 CI 与 Linux ASan/LSan Map 证据验收归档至 `docs/archive/specs/0288-map-native-execution.md` 并通过 PR #69 合入 main。

0287（M2B 通用借用合同冻结与 Map 键值容器类型系统基础）已按前端类型系统与规范验收归档至 `docs/archive/specs/0287-m2b-and-map-type-system.md`。

0286（编译单元局部解构 SSA Lowering 与原生执行）已按双宿主实现验收归档至 `docs/archive/specs/0286-unit-local-destructuring.md`。

0285（`MutableList.insertAt` 元素指定索引插入与向后平移扩容）已按双宿主实现验收归档至 `docs/archive/specs/0285-mutable-list-insert-at.md`。

0284（`MutableList.removeFirst` 头部元素快速移出与队列弹出）已按双宿主实现验收归档至 `docs/archive/specs/0284-mutable-list-remove-first.md`。

0283（`MutableList.removeLast` 尾部元素快速移出与生命周期交付）已按双宿主实现验收归档至 `docs/archive/specs/0283-mutable-list-remove-last.md` 并通过 PR #62 合入 main。

0282（`MutableList.removeAt` 索引元素移出与剩余元素前移压缩）已按双宿主实现验收归档至 `docs/archive/specs/0282-mutable-list-remove-at.md` 并通过 PR #61 合入 main。

0281（`MutableList.clear` 逆序元素清理与缓冲区复用）已按双宿主实现验收归档至 `docs/archive/specs/0281-mutable-list-clear.md` 并通过 PR #60 合入 main。

0280（`MutableList.add` 顺序追加与动态扩容）已按双宿主实现验收归档至 `docs/archive/specs/0280-mutable-list-add.md` 并通过 PR #59 合入 main。

0279按[PR57](https://github.com/Halckon/Koven/pull/57)双宿主实现验收与用户限定范围归档；最终归档head/merge/main待交付，见[账本](https://github.com/Halckon/Koven/blob/8c6907c238cc0e040bed4541203e450a5f53552b/docs/development/evidence/runtime-constructor-0279/delivery.json)。

0278按[PR56](https://github.com/Halckon/Koven/pull/56)双宿主实现验收归档；最终归档head、merge及actual main CI已闭环，见[交付账本](https://github.com/Halckon/Koven/blob/8c6907c238cc0e040bed4541203e450a5f53552b/docs/development/evidence/closure-escape-0278/delivery.json)。

0277按[PR55](https://github.com/Halckon/Koven/pull/55)双宿主实现验收归档，最终归档head及actual main已闭环，见[交付账本](https://github.com/Halckon/Koven/blob/8c6907c238cc0e040bed4541203e450a5f53552b/docs/development/evidence/p2-linux-0277-delivery.json)。

0269 的 M4b 有界资源程序首片已验收归档；实现审阅与双宿主原始证据见
[交付证据](https://github.com/Halckon/Koven/blob/8c6907c238cc0e040bed4541203e450a5f53552b/docs/development/evidence/generated-owners-0269-delivery.json)，
最终归档 head、merge 与 main CI 交付记录见 [PR48](https://github.com/Halckon/Koven/pull/48)。

0276已按[PR54](https://github.com/Halckon/Koven/pull/54)归档并合并为18b89e5；最终head必需14job成功、编辑器合法skip，actual main实际15/15成功，见[交付闭环账本](https://github.com/Halckon/Koven/blob/8c6907c238cc0e040bed4541203e450a5f53552b/docs/development/evidence/generic-body-0276-delivery.json)。

0275已由[PR53](https://github.com/Halckon/Koven/pull/53)合并为2be6406；最终归档CI37252410423必需14job成功、编辑器合法skip，main CI37253329605实际15/15成功。双宿主新增测试命中、完整词频/preview原始证据与source身份见[交付闭环账本](https://github.com/Halckon/Koven/blob/8c6907c238cc0e040bed4541203e450a5f53552b/docs/development/evidence/generic-containers-0275-delivery.json)。

0274已按[PR52](https://github.com/Halckon/Koven/pull/52)双宿主实现证据归档；最终归档head cdb9cba的CI37247799491已15/15成功，并合并为c5c4a8d。

0272已由[PR51](https://github.com/Halckon/Koven/pull/51)合并；0275从PR52最新main独立分支启动。main push CI37248672159已15/15成功，0275生产实施前置已确认。

0273 已按 PR50 最终归档双宿主 CI 通过并合并。

0271 已按双宿主最终归档 CI 通过，并经 [PR49](https://github.com/Halckon/Koven/pull/49) 合并。
0270单文件局部MoveOnly绑定交接已按双宿主实现证据归档，
交付见[PR47](https://github.com/Halckon/Koven/pull/47)。

0268已按双宿主实现证据归档，交付记录见[PR46](https://github.com/Halckon/Koven/pull/46)。
0263/0264/0265 已分别完成 M1A A2–A4 有界验收归档；
0266 检测接线与0267编辑器修复已分别通过PR44/PR45合并。0268补齐unit native for与完整应用实现验收。
0182、0255–0261按用户授权的本机恢复验收完成本地归档，
实际结果与未覆盖范围见[本机恢复账本](../../archive/development/recovery-local-delivery.md)。
[0262教程补齐](../../archive/specs/0262-current-tutorial-plan-coverage.md)已随PR38交付并具备精确head双宿主证据。
0266/PR44完成Linux ASan/LSan有界接线，不表示macOS动态检测或UBSan完成。
当时状态见[固定基线核对](../../archive/development/documentation-status-sync.md)；当前状态由本目录合同维护，旧Spec正文保留交付时快照。
原P2成本证据/预算接受仍延期，不能从归档或CI成功推定全部治理验收完成。

- [演进实施账本](../evolution-status.md)：语言能力与独立缺口
- [完成 Spec Archive](../../archive/specs/README.md)：只按需追溯原验收
