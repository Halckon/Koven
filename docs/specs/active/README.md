# Active Specs

> **性质**：当前变更索引 · **状态**：current · **读取时机**：查找已批准或实施中的 Goal 时 · **唯一真源**：本目录 Spec

当前 0 份 active。

0288（Map 键值容器原生执行基础：SSA 原语、LLVM IR 代码生成与 Native Runtime 哈希表）已按双宿主 CI 与 Linux ASan/LSan Map 证据验收归档至 `docs/archive/specs/0288-map-native-execution.md` 并通过 PR #69 合入 main。

0287（M2B 通用借用合同冻结与 Map 键值容器类型系统基础）已按前端类型系统与规范验收归档至 `docs/archive/specs/0287-m2b-and-map-type-system.md`。

0286（编译单元局部解构 SSA Lowering 与原生执行）已按双宿主实现验收归档至 `docs/archive/specs/0286-unit-local-destructuring.md`。

0285（`MutableList.insertAt` 元素指定索引插入与向后平移扩容）已按双宿主实现验收归档至 `docs/archive/specs/0285-mutable-list-insert-at.md`。

0284（`MutableList.removeFirst` 头部元素快速移出与队列弹出）已按双宿主实现验收归档至 `docs/archive/specs/0284-mutable-list-remove-first.md`。

0283（`MutableList.removeLast` 尾部元素快速移出与生命周期交付）已按双宿主实现验收归档至 `docs/archive/specs/0283-mutable-list-remove-last.md` 并通过 PR #62 合入 main。

0282（`MutableList.removeAt` 索引元素移出与剩余元素前移压缩）已按双宿主实现验收归档至 `docs/archive/specs/0282-mutable-list-remove-at.md` 并通过 PR #61 合入 main。

0281（`MutableList.clear` 逆序元素清理与缓冲区复用）已按双宿主实现验收归档至 `docs/archive/specs/0281-mutable-list-clear.md` 并通过 PR #60 合入 main。

0280（`MutableList.add` 顺序追加与动态扩容）已按双宿主实现验收归档至 `docs/archive/specs/0280-mutable-list-add.md` 并通过 PR #59 合入 main。

0279按[PR57](https://github.com/Halckon/Koven/pull/57)双宿主实现验收与用户限定范围归档；最终归档head/merge/main待交付，见[账本](../../development/evidence/runtime-constructor-0279/delivery.json)。

0278按[PR56](https://github.com/Halckon/Koven/pull/56)双宿主实现验收归档；最终归档head、merge及actual main CI已闭环，见[交付账本](../../development/evidence/closure-escape-0278/delivery.json)。

0277按[PR55](https://github.com/Halckon/Koven/pull/55)双宿主实现验收归档，最终归档head及actual main已闭环，见[交付账本](../../development/evidence/p2-linux-0277-delivery.json)。

0269 的 M4b 有界资源程序首片已验收归档；实现审阅与双宿主原始证据见
[交付证据](../../development/evidence/generated-owners-0269-delivery.json)，
最终归档 head、merge 与 main CI 交付记录见 [PR48](https://github.com/Halckon/Koven/pull/48)。

0276已按[PR54](https://github.com/Halckon/Koven/pull/54)归档并合并为18b89e5；最终head必需14job成功、编辑器合法skip，actual main实际15/15成功，见[交付闭环账本](../../development/evidence/generic-body-0276-delivery.json)。

0275已由[PR53](https://github.com/Halckon/Koven/pull/53)合并为2be6406；最终归档CI37252410423必需14job成功、编辑器合法skip，main CI37253329605实际15/15成功。双宿主新增测试命中、完整词频/preview原始证据与source身份见[交付闭环账本](../../development/evidence/generic-containers-0275-delivery.json)。

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
实际结果与未覆盖范围见[本机恢复账本](../../development/recovery-local-delivery.md)。
[0262教程补齐](../../archive/specs/0262-current-tutorial-plan-coverage.md)已随PR38交付并具备精确head双宿主证据。
0266/PR44完成Linux ASan/LSan有界接线，不表示macOS动态检测或UBSan完成。
当前状态以[本次固定基线核对](../../development/documentation-status-sync.md)为准；旧Spec正文保留交付时快照。
原P2成本证据/预算接受仍延期，不能从归档或CI成功推定全部治理验收完成。

- [演进实施账本](../evolution-status.md)：语言能力与独立缺口
- [完成 Spec Archive](../../archive/specs/README.md)：只按需追溯原验收
