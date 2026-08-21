# ADR-0005: package 与 source root 映射

## 状态

accepted

## 接受依据

用户在当前持续 Goal 中要求继续分阶段实施 Specs，并授权简化重复验收与后续完整决策文档的
确认；本 ADR 只决定现行 guide 明确留给 package ADR 的映射，不改变语言语法。

## 背景

Koven 文件已经使用绝对、点分的 `package` / `import` 语法，但单文件前端刻意不从文件名或
当前工作目录推导 package。SPEC-0025 需要把多份源码组成确定的 package 图；若 source root、
目录映射和多 root 合并规则仍由调用方临时猜测，同一源码会因启动目录、枚举顺序或工具不同
获得不同身份，也会迫使 `lang-frontend` 读取文件系统。

本决策需要保持现有 `SourceMap` 的 map-local `SourceId`、frontend 的纯分析边界和未来
`project.toml` 的配置空间，同时不提前固定 import 冲突、visibility 或 package manager schema。

## 决策

- 一次多文件编译显式接收一个或多个 **source root**。root 是调用方提供的源码树边界，不能
  从进程当前目录、首个输入文件或 `package` directive 反向猜测。未来 `project.toml` 可以
  声明这些 root；在 manifest 实现前，测试或 CLI 编排层必须显式构造同一输入。
- root 下源码的规范逻辑路径使用 UTF-8、`/` 分隔的相对路径；禁止绝对路径、空段、`.` 与
  `..`。物理路径规范化、目录遍历和 IO 错误属于驱动层；`lang-frontend` 只接收已经加载的
  `(root identity, logical path, source text)`，不读取目录、不解析 symlink，也不依赖宿主路径
  大小写规则。
- 只把扩展名为 `.ko` 的普通源码纳入自动发现。驱动层递归发现时不跟随 symlink，并按规范
  逻辑路径排序后加载；虚拟文件、编辑器 buffer 和测试可以直接提供满足同一不变量的逻辑
  路径，无需伪造物理文件。
- 文件所在目录相对 source root 的每个路径段一一对应一个 package Identifier segment；位于
  root 直属目录的文件属于默认 package。文件名和 stem 不进入 package identity，也不声明
  类型或值名称。
- 文件声明的 `package` 必须与上述目录映射完全一致；省略 directive 只对 root 直属文件合法。
  不匹配是 package/source-set 构建诊断，不能通过重写声明、移动逻辑路径或回退到默认 package
  自动修复。
- 多个 root 可以贡献同一个逻辑 package，package 按有序 Identifier segment 统一合并；root
  顺序不产生遮蔽或优先级。两个输入具有相同 `(root identity, logical path)`，或同一物理
  source 被重复加入一次编译，均在进入名称收集前拒绝。合并后的声明冲突由 SPEC-0025 使用
  普通符号规则诊断。
- package identity 是结构化的 Identifier segment 序列，不是宿主路径字符串；源码的稳定
  排序键是 root 的调用方稳定 identity 加规范逻辑路径。`SourceId` 仍只是 `SourceMap` 内部
  身份，不能充当 package/file 持久键。
- SPEC-0025 在 frontend 内建立显式多文件 source-set/package 层，再复用现有单文件声明、
  类型/值双命名空间和确定性诊断模型。exact/wildcard import 绑定、alias、visibility 与名称
  冲突规则由适用 guide 和 SPEC-0025 定义；本 ADR 不让物理目录参与这些语义。

## 替代方案

### 只以 `package` directive 为身份，不校验目录

拒绝。它允许任意物理布局，但文件发现仍没有边界，CLI、LSP 和构建系统容易各自形成不同的
输入集合，也无法及早发现源码放错 root。

### 目录路径权威，忽略或自动修正 `package`

拒绝。源码文本与工具展示会对 package identity 产生不同结论，重命名目录还可能静默改变
程序语义。

### 每个 package 在 manifest 中逐项映射任意目录

暂不采用。它能支持更自由的布局，但会在最小 package manager 之前引入额外 schema、重叠
映射和优先级规则；v1 的一一目录映射足以建立确定基线。

### 由 `lang-frontend` 直接遍历文件系统

拒绝。它会把 IO、symlink、宿主大小写和工作目录状态带入可复用的分析 crate，破坏 CLI、
LSP、测试与虚拟源码共享同一前端入口的边界。

## 后果

收益：

- 同一显式 source set 在 CLI、测试与 LSP 中获得相同 package identity 和稳定顺序；
- package 声明与目录布局漂移会被明确拒绝，而不是产生隐式导入差异；
- frontend 保持文件系统无关，未来 manifest 和编辑器只需适配同一逻辑 source-set API；
- 多 source root 的同 package 合并不依赖 root 优先级，名称冲突继续由统一语义层处理。

代价与风险：

- 源码目录必须与 package 段一致，迁移现有任意布局时需要移动文件或修改声明；
- 驱动层必须生成稳定 root identity、规范逻辑路径并负责安全的文件发现；
- 不跟随 symlink 会限制用链接拼装源码树的工作流，需要由构建系统显式提供真实 root；
- import 冲突、visibility 和 manifest schema 仍需各自的 guide/Spec，不能从本 ADR 推断。

## 关联

- 相关 Spec：SPEC-0025、SPEC-0052、SPEC-0055
- 相关 ADR：[ADR-0004](./0004-source-span-position-model.md)
- 取代的 ADR：无
- 被以下 ADR 取代：无
