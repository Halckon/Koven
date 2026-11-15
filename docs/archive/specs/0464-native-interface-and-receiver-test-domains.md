# SPEC-0464：Native 接口/委托与接收者测试领域归位

> **性质**：实施 Spec · **状态**：done · **读取时机**：本批 native 测试根尺寸欠账时 · **唯一真源**：本批范围与验收，语言语义从属 Guide

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-0464` |
| 所属 Phase | Phase 4，native object/link/run测试组织 |
| 语言规范 | 现行 [Guide](../../guide/README.md)，无语义变更 |
| 批准依据 | 用户要求继续下一批；沿fix/spec-0456与PR #433，不另开PR |
| 前置 Spec | [0463](0463-ownership-operation-and-lowering-domains.md)，同批输入4df265b1 |
| 前置 ADR | [ADR-0010](../../adr/accepted/0010-first-native-object-and-linker-contract.md) |
| 影响范围 | native/unit_tests.rs及两个私有测试领域模块、对应policy |
| 语言语义变更 | 否 |
| 阻塞项 | 无 |

## 1. 两片完整测试领域

1. 14 项接口/继承/委托测试移入私有 `native/unit_interface_delegation_tests.rs`：interface default
   与 super static 调用、abstract requirement override、继承与无关默认实现、list 与
   parameter-independent/dependent inherited owner recipe、borrow-only 委托、same-requirement
   与 identity-changing 委托链、generic interface owner/callable 委托、parameter-independent 与
   nested generic nominal 委托、`Inout` 与 value interface default。
2. 10 项类接收者/inline 所有权测试移入私有 `native/unit_class_receiver_tests.rs`：object
   atomically replace、direct member receiver 顺序与唯一 drop、`Inout` 载荷变更、MoveOnly 载荷
   替换、stateless object 借用、inline `Inout` 只读/写回、MoveOnly inline take、inline 可复制
   字段变更与嵌套 drop glue。

原 `#[test]` 属性、正文、fixture 源码与断言逐字节保全；FQN 只增加 `interface_delegation_tests`
或 `class_receiver_tests` 一层。根保留全部 imports、`TestDirectory`、
`assert_no_sibling_temporary` 及子模块加载，子模块以私有 `#[path]` + `use super::*` 继承原绑定。

## 2. 非目标

不改生产 native/LLVM/SSA 代码与任何断言、stdout 或对象/链接身份；不新增 ignored 测试。根与
新文件均不超 1000，只删除 `native/unit_tests.rs` baseline/exception 两项，其他 policy 逐 JSON
相同。不导入未合 #427，不为凑数删除合理保留模块。

## 3. 验收

- [x] R1：24 项测试完整保全，旧完整 FQN 无副本，原根剩余 import 不动。
- [x] R2：逐片定向原领域GREEN→移动→GREEN（真实 native link/run），本地提交。
- [x] R3：与0463成组统一双配置组合、workspace check、双Clippy、fmt/docs/尺寸/policy。
- [x] R4：独立完整精确提交区间审查；Architecture/账本后一次push同PR。
- [x] R5：精确新头官方CI另验；保持active/Draft，不归档或合入main。

## 4. 真实证据

| 切片 | 真实执行 | 尺寸 |
|---|---|---|
| 接口/委托 14 项 | 移动后 **14/0/0**，逐 leaf 名与基线子集一一对应，唯一模块层 `interface_delegation_tests` | 1953→1232，新文件 741 |
| 类接收者 10 项 | 移动后 **10/0/0**，同样的 leaf 级核对，唯一模块层 `class_receiver_tests` | 1232→517，新文件 731 |
| 基线 | 移动前 24 项真实 native object/link/run **24/0/0**，stdout/stderr 与 drop 断言未改 | — |

机械核对：两个新文件的 `#[test]` 块与移动前逐字节相同，根剩余 import、`TestDirectory`、
失败用例 helper 与 `#[allow(unused_imports)]` 绑定未动；policy 只删除
`crates/lang-codegen/src/native/unit_tests.rs` 的 baseline 与 exception。

真实失败保留：组合回归首次 634/0/0 通过但两个新域过滤器写成 `native_tests::`（实际在
`native::unit_tests::` 下）导致零命中、runner 失败，不计完整基线；改为正确前缀后单次
**658/0/0**，未重复计入。rustfmt 首次 fmt 门禁失败（新文件尾部空行与 `mod` 声明顺序），
格式化后重跑默认组合仍 658/0/0 并通过 fmt/尺寸/空白。

24 项 FQN 各增加一层领域模块（`native::unit_tests::{interface_delegation_tests,class_receiver_tests}::*`），
CI/脚本无旧全名耦合；归档文档按治理不改写。

组末与0463同树：双配置各 **658/0/0**、workspace check/双Clippy/47项policy/docs961 通过；
历史超限 25→22。同批普通 merge main `eb67f330`（提交 `74dea8a6`），无冲突；主干新增
nullable writeback native 两项与 grouped-when/parser 改动由 0463 §5 记录实测。

独立审查未发现语义问题：arm/错误/location 行位置全部 +3 且文本相同，24 项断言位置一致，
无新增 ignore/allow；P1 fmt 与 P2 数字/`#[path]` 已按上节闭合。原始日志/快照/完整区间
审查留仓库外；精确新头 CI 仍待。

## 最终交付验收（PR #433）

用户授权先合入再从main开展后续。最终代码头 `914797c2c02dd3787ded8ff1255b53d4d3bc9e09`
普通整合main `fdf5766b2b2707fe29c7ab7f7b8445b98ef329ad` 后，双配置组合各 **663/0/0**；
workspace all-target check/双Clippy、fmt、policy47、docs962、尺寸/空白通过，历史超限
**37→21**（本PR各批次累计，不将此总数归因单一Spec）。

[精确代码PR CI 43699903034](https://github.com/Halckon/Koven/actions/runs/43699903034)
completed/success：双宿主Check/Clippy/Tests、Preview producer与独立consumer、最终汇总
实际成功；Editor skipped不计执行。此前c2328e14的PR为43692407513，43692403094是push，
两者不能替代本集成头。独立交付复核完整实际两个diff（分发2861行、主干增量555行），
无具体问题；历史0456–0464独立审查保持各自范围，不补成原188路径全量正文审计。

本页旧pending/未完成文字是当时检查点，以上最终记录闭合本次合同勾选；原失败、观察期限
及本地未跑项保留，不把CI或归档倒写成所有本地检查已跑。归档候选CI和实际合并另行核验，
不代表E3终态、清空34合理保留或扩大语言准入。
