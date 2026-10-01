# Koven Proposals

> **性质**：非规范候选索引 · **状态**：未启用 · **读取时机**：任务明确要求评审未来语义时 · **唯一真源**：各 proposal 正文

Proposal 本身不修改现行 v0.40、不批准 Spec，也不代表实现优先级。已选 String.clone 部分
已移入 guide，剩余候选仍未启用；普通开发任务不要读取本目录。

- [Map / MutableMap 所有权候选](map-ownership.md)
- [v2 interface 值与动态分发](v2-interface-values-and-dynamic-dispatch.md)
- [String Copyable 候选取舍](string-copyability.md)
- [显式 clone() 候选设计](explicit-clone.md)
- [受限扩展函数候选设计](restricted-extension-functions.md)
- [集合算法所有权候选设计](collection-algorithm-ownership.md)

## 候选依赖与推进顺序

六份候选不是彼此独立的，其中"集合算法"依赖一条前置链：

```text
受限扩展函数  -->  View<T> intrinsic 与构造  -->  集合算法所有权
  （声明语法）          （类型 + 视图构造）           （filter / consume / 物化）
                                                       ^
                                              显式 clone()（物化时调用）

String Copyable  -->  若采纳，则集合算法物化的 clone 成本消失（非前置）

Map 所有权、v2 interface 值  -->  独立，不依赖上述链
```

| 候选 | 前置 | 当前状态 |
|---|---|---|
| 受限扩展函数 | 无；需放宽 15 页 | 授权已获得，实施时生效 |
| 显式 clone() | 无 | 仅 String 切片已进入 v0.40；通用 clone 等仍为候选 |
| String Copyable | 无 | 候选结论：保持 `MoveOnly` |
| 集合算法所有权 | 受限扩展函数、`View<T>` intrinsic、clone() | 候选设计已成形 |
| Map / MutableMap 所有权 | 无 | 独立候选 |
| v2 interface 值与动态分发 | 无 | 独立候选 |

推荐推进顺序：受限扩展函数 → `View<T>` intrinsic 与视图构造 → 集合算法表面 → expected type
物化。String.clone 已有规范结论，不代表集合算法物化或 String Copyable 获得启用；其余候选
仍按各自前置与授权评审。

对应阻塞 Spec 从 [Specs](../specs/README.md) 进入；长期架构候选仍遵循 ADR 生命周期。

## 从候选 guide 到可实施 Spec

版本号不表示累积继承已经成立；任何未来候选都须明确对现行版本的继承与取代关系。
启用前按以下顺序收口，每一步保留可审查证据：

1. 逐条对照候选规则与现行 v0.40，列出保留、补充、取代的规则及目标章节；对冲突请求决定。
2. 明确新版本是否包含其他候选版本。未纳入的候选保持未启用，不能凭版本号隐式合并。
3. 准备完整的新 guide 与引用更新，并由用户明确启用；在此之前 current 入口仍为 v0.40。
4. 将对应 Spec 重基到启用后的 guide，核对前置 Spec/ADR 与批准依据，再迁移依赖完备的切片到 active。
5. 按各版本 Spec 索引推进 Phase 产物，逐项记录定向验收；规范启用不代表实现已经完成。

验收命令和测试并行仅由[测试与分层验收](../development/testing.md)定义；候选正文不复制工程门禁。

## 历史

v0.35、v0.36 与 v0.37 已启用；原候选与重基记录仅用于追溯：
[v0.35](../archive/guides/v0.35-candidate.md)、[v0.36](../archive/guides/v0.36-candidate.md)、[v0.37](../archive/guides/v0.37-candidate.md)。
