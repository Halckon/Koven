# PR72–74 有界交付与后继核对

> **性质**：交付与后继边界摘要 · **状态**：frozen（2026-10-11检查点） · **读取时机**：区分已验收范围和后继合同 · **唯一真源**：固定基线源码、对应Spec、精确CI记录及下列有界复核；本页不启用语义

## 基线与交付

静态审计基线为main `e168c485a5379764cf927433452f8b7059f7b334`；精简文档分支从
PR75实际main `8c6907c238cc0e040bed4541203e450a5f53552b` 起建，tree为
`d1dee1478a37e85acc2858dad3fd918a7b975c03`。以下状态来自已核验交付记录，不是本页重跑CI。

| 有界合同 | merge/main | actual-main CI |
|---|---|---|
| 0289 / PR72 | `3f97679839a175088ba33163851e56dccc1dd860` | [38115027930](https://github.com/Halckon/Koven/actions/runs/38115027930)，success |
| 0294 / PR73 | `4321e99e401a3d8b9099f087c577840822a64512` | [38121438850](https://github.com/Halckon/Koven/actions/runs/38121438850)，success |
| 0295 / PR74 | `e168c485a5379764cf927433452f8b7059f7b334` | [38133685795](https://github.com/Halckon/Koven/actions/runs/38133685795)，success |
| 0296 / PR75 | `8c6907c238cc0e040bed4541203e450a5f53552b` | [38150175528](https://github.com/Halckon/Koven/actions/runs/38150175528)，success（attempt1） |

0296保持archive；本分支不改其实现或验收收据。首次接收main CI时为pending，随后
已核验attempt1于2026-10-11 15:24:21Z成功；main required jobs通过，PR-only Code CI
Evidence合法skip，不算再次执行。旧收据的pending保留原检查点，不倒填历史。
用户2026-10-11确认实施顺序0297→0300→0299/0298；四片在本分支仍draft，激活由各自
独立实现分支维护，不因本次文档合并自动启用额外Guide语义或范围外退出能力。

## 已核实的范围区别

以下源码行号固定于原静态基线e168c485，不随新main漂移；合同入口见[后继索引](../../specs/drafts/v0.43/README.md)。

- 0298：`ssa/borrow_result_support.rs`50–120按binding所属最小函数Span扫描，非活跃loan
  区间。`n==2`、`n<3`、短路和String拼接即使在binding前后也可能拒绝；NewRangeDescriptor
  只额外放行真实Int五种算术及同root range-call前缀if/return。0289原合同明确保留更广CFG，
  不能把真实组合缺口说成其有界验收失败，也不能直接删除gate。
- 0297：single `lower_frontend/map.rs`142–182、285–320和unit `unit_lower/map.rs`
  107–186、290–348缺完整key Diverged传播；single owned receiver还缺put已有的精确
  pending cleanup identity（`lower_frontend/call_lifetimes.rs`40–59）。mixed有正常Value
  出口，不能一概判红；普通源码结果见下节。0294明确只修put。
- 0299：`lower_frontend/borrow_argument.rs`219–251、`unit_lower/borrow.rs`250–295的
  Copyable门保护Map result复制桥，不是所有Borrow的语言限制；single已有view转发。
  get(MoveOnly)原本应拒绝，owned remove/`!!`已有通用回归；窄化非消费Borrow与0296清理独立。
- 0300：普通single Map-for的Named texts为IterationPhi，非Resource且非iteration-source，
  命中`lower_frontend/orchestrate.rs`193–199 guarded-drop门；无closure/snapshot动作，
  也无borrow-result binding。0296保留的正常源码测试已定位此门，不能归为range gate。

### 冻结CLI的Map key return矩阵

0296生产实现 `ae65b2ce1e779b2d2b54e0f793d03207671192b9` 的冻结CLI已实际运行以下矩阵。
其SHA256为`bdba08202abe42add92b130c116cc05458f398fdd38211fd5a31a5c4015c9d20`；
它不是e168c485二进制，CLI不支持`--version`，不能仅用workspace版本0.1.0标识。

| get/contains/remove各一格 | single | unit project |
|---|---|---|
| `if (flag) { return } else { 8 }` | 3格exit2 / MissingFact | 3格build/native exit0 |
| `if (flag) { 8 } else { return }` | 3格exit2 / MissingFact | 3格build/native exit0 |
| `if (flag) { return } else { return }` | 3格exit2 / MissingFact | 3格exit1 / MissingFact |
| 无return对照 `if (flag) { 8 } else { 8 }` | 3格build/native exit0 | 3格build/native exit0 |

程序创建owned named `MutableMap<Int, Int>`并put(8,80)，操作前后打印before/after，
分别以false/true调用，最后打印done。两种unit mixed准确输出分别为
`before\nafter\nbefore\ndone\n`及`before\nbefore\nafter\ndone\n`。
运行先source toolchains/env.sh；single命令为`kovenc build main.ko -o program`，
unit为`kovenc build --project project.toml --entry app.main -o program`，成功后`./program`。
18个return格6成功/12拒绝，6个对照全成功；本精简分支没有重跑这些程序。

CLI未给Span或内部facts，不能据MissingFact单独确认内部调用点；普通输出不证明逐owner
释放次数或完整内存安全。直接return、String/Resource前缀及其他receiver未在此矩阵执行。
正式源码由0297实现测试owner维护，本页不虚构尚未合入main的fixture路径。

## 记录保留与后继候选

旧原始24格源码、命令和输出保留于未发布的原本地链`5833424b91b2f340b976901682adcb561d18e862`
及对应scratch；原JSON SHA256为`916de3f0d394edd9007edd400ec49d11f2e4a8bdb3daff1da59cc000038610a3`。
本发布链仅保留上述可核对摘要，不复制全文JSON/日志，也不把本地commit包装为远端链接。
四份合同原起草提交分别为`a8dde068`、`34affb54`、`6abb8487`、`cb8081ed`；
后续正文纠正保留于上述原本地链，新链只调整状态/引用，不改变有界Goal与非目标。
曾短暂起草的0301已撤回并由0296第二阶段承接；本链不新增0301或重复归档计数。

普通nullable Resource factory另列候选，不分配新Spec：0296测试
`single_nullable_factory_planning_boundary_precedes_any_comparison`用无Map、无判空源码
确认single在返回类型登记时UnsupportedNode，unit同源通过verifier；本页未重跑该测试。
原基线`lower_frontend/orchestrate.rs`536–550→`nominal.rs`166–175→
`resource_deinit.rs`146–147将Resource?经过Nominal门，拒绝早于body。
后继须单独验收nullable返回映射/ABI及恰一次清理，不并入Map结果借用或判空修复。

Guide两处现行段落的旧版本标签只改为“现行版本”；evolution/roadmap的历史检查点保留。
本分支仅静态文档验收；不改生产Rust，不运行Cargo、注入或聚合驱动。

精简链静态复验：摘要613篇、四draft依次614/615/616/617篇Markdown通过；
四片各37个checker测试通过，diff检查通过。草案内初稿检查数字保留原本地时点。
