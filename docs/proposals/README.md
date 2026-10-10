# Koven Proposals

> **性质**：非规范候选索引 · **状态**：未启用 · **读取时机**：任务明确要求评审未来语义时 · **唯一真源**：各 proposal 正文

Proposal 本身不修改现行 v0.42、不批准 Spec，也不代表实现优先级。已选 String.clone 部分
已移入 guide，剩余候选仍未启用；普通开发任务不要读取本目录。

- [通用借用访问结果（M2B）](general-borrow-access-results.md)
- [借用结果声明与类型合同比较](borrow-access-contract-comparison.md)
- [Map / MutableMap 所有权候选](map-ownership.md)
- [v2 interface 值与动态分发](v2-interface-values-and-dynamic-dispatch.md)
- [String Copyable 候选取舍](string-copyability.md)
- [显式 clone() 候选设计](explicit-clone.md)
- [受限扩展函数候选设计](restricted-extension-functions.md)
- [集合算法所有权受审修订（r3.1）](collection-algorithm-ownership.md)

## 候选依赖与推进顺序

2026-10-08 的 collection r3.1 小修保留核心规划方向；候选正文不启用新能力。
以下是设计/实施依赖，不以候选表面或已有语法授权代替 Guide 明确启用：

```text
闭合已批准 M2B/Map 欠项 --> N1a 范围 carrier 生命周期 --> 最小范围视图矩阵
显式 consume/构造原语   ----------------------------> 立即消费路径
N1b 自有索引与交付合同  ----------------------------> 借用 filter（后续）
复制能力独立评审        ----------------------------> 显式 materialize
受限扩展声明            ----------------------------> 标准库算法表面

最小矩阵闭合 --> 其他算法；可选 owned 元素返回另以 Option 为条件依赖
String Copyable、Map 转换与 v2 interface 值不由此获得启用
```

| 候选 | 前置 | 当前状态 |
|---|---|---|
| 受限扩展函数 | 独立 grammar/静态查找审查；需新 Guide 启用 | 已记录放宽语法授权，仍未启用；见候选 §5.1 |
| 显式 clone() | 通用能力需独立设计 | String.clone 已启用；不代表所有 MoveOnly 可 clone |
| String Copyable | 无 | 候选结论：保持 `MoveOnly` |
| 集合算法所有权 | consume 原语、N1a carrier、复制能力与声明表面 | r3.1 区分首片必需项与 N1b 后续验收；正文保持候选 |
| N1 非逃逸 carrier | N1a 新描述符交付、绑定/参数/返回与真实来源 loan；N1b 自有存储另审 | 独立验收；不能直接替换多 capture borrowed closure |
| 通用借用访问结果（M2B） | 实际缺口与正式 Spec | v0.42 已启用受限普通结果与 scoped Map；其余候选及实现欠项保持独立 |
| Map / MutableMap 所有权 | 新增借用结果依赖 M2B；其它查询按所选合同 | 历史候选；新决定须先重基 |
| v2 interface 值与动态分发 | 无 | 独立候选 |

建议先闭合当前已批准欠项，再分别评审 consume 基础与复制能力，冻结 N1a carrier 合同、
对齐受限扩展声明，形成最小范围视图/消费矩阵；N1b/filter 后续验收。各独立设计可以先评审；
语义实施前仍须明确启用，资源原语不能反向依赖尚未实现的 filter。
完整验收维度见[collection §11](collection-algorithm-ownership.md#11-phase-边界)，
实施排序见[后续里程碑](../development/post-governance-milestones.md)。

对应阻塞 Spec 从 [Specs](../specs/README.md) 进入；长期架构候选仍遵循 ADR 生命周期。

## 从候选 guide 到可实施 Spec

版本号不表示累积继承已经成立；任何未来候选都须明确对现行版本的继承与取代关系。
启用前按以下顺序收口，每一步保留可审查证据：

1. 逐条对照候选规则与现行 v0.42，列出保留、补充、取代的规则及目标章节；对冲突请求决定。
2. 明确新版本是否包含其他候选版本。未纳入的候选保持未启用，不能凭版本号隐式合并。
3. 准备完整的新 guide 与引用更新，并由用户明确启用；在此之前 current 入口仍为 v0.42。
4. 将对应 Spec 重基到启用后的 guide，核对前置 Spec/ADR 与批准依据，再迁移依赖完备的切片到 active。
5. 按各版本 Spec 索引推进 Phase 产物，逐项记录定向验收；规范启用不代表实现已经完成。

验收命令和测试并行仅由[测试与分层验收](../development/testing.md)定义；候选正文不复制工程门禁。

## 历史

v0.35、v0.36 与 v0.37 已启用；原候选与重基记录仅用于追溯：
[v0.35](../archive/guides/v0.35-candidate.md)、[v0.36](../archive/guides/v0.36-candidate.md)、[v0.37](../archive/guides/v0.37-candidate.md)。
