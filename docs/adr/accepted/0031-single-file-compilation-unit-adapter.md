# ADR-0031: 单文件 compilation-unit driver 适配

> **性质**：长期架构决定 · **状态**：accepted · **读取时机**：实现单文件 source-set/root/标准源接线时 · **唯一真源**：本决定；语言语义见 Guide

## 状态

accepted

## 接受依据

2026-10-19 用户明确批准单文件 import、package 路径反推 root、可选显式 root、
标准环境一致与 v0.45 审后启用；实施与实际验收由 [SPEC-0341](../../specs/active/0341-shared-lowering-core.md)维护。
本决定局部扩展 [ADR-0005](0005-package-source-root-mapping.md) 的单文件 driver 边界，
不改写它的历史理由或多文件显式 roots 决定。

## 决定

- 单文件 driver 构造只含一个用户文件的显式 compilation unit；不调用递归 project
  discovery，不扫描兄弟文件。package/root 与 import 的完整规则只见
  [Guide 02](../../guide/02-names-files-packages.md#单文件输入与共同名称解析)。
- 单文件从调用者指定路径的逻辑父目录组件剥去匹配的 package 后缀取得 root；这是用户
  明确允许的特例。project 继续由 manifest 提供显式 roots，不反推或搜索 manifest。
- package 逻辑匹配和物理 root 身份等价分层；物理路径处理不反写声明或逻辑路径。
  不将 project 的 symlink-root 拒绝或递归发现政策移植为单文件的新拒绝。
  frontend 只接收已加载的稳定 root identity、规范逻辑路径和源码，不访问文件系统。
- 两模式使用同一个内建环境构造与受信任标准源 loader；loader 对真实加载的标准资产
  发布来源授权，不能由用户 package 名称或路径取得权限。加载标准包不自动导入名称，
  允许的 package 清单只由 Guide 02 维护。
- driver 保留单文件与 project 各自 entry 选择合同及操作失败身份；选中同一声明后的
  名称/类型/所有权/代码生成使用共同 compilation-unit 管线。不得以 fallback 保留第二套
  生产语义实现，也不把普通单文件 SymbolId 冒充 source-qualified DeclarationId。

## 边界与验证

本决定不引入 package manager、依赖扫描、自动 prelude 或新标准包；也不定义新的 filesystem
沙箱、realpath 重写或 symlink 拒绝政策。`..` 与 symlink 别名的物理 root 等价细节须在
入口适配实施前集中确认，不能用字符串折叠模拟宿主路径解析或改变现有单文件读取行为。
这项局部实施待项不阻塞与路径无关的 Unit 语义回退修复。

普通验收须覆盖 package 目录匹配/不匹配、无 package、显式 root 一致/不一致、同 package
只加载指定文件、未加载用户 package 的 L0148/`--project` 提示、真实标准 import、裸名
不自动导入，以及原 entry/argv 合同。切换还须满足 Guide 15 与 SPEC-0341 的能力保全、
零模式差异及零 InvalidSsa 门槛；accepted 不表示实现已完成。

## 关联

局部扩展 [ADR-0005](0005-package-source-root-mapping.md)；其余决定继续有效。
共同 unit 输入沿用 [ADR-0020](0020-multifile-compilation-unit.md)，project 发现沿用
[ADR-0022](0022-minimal-project-manifest-source-discovery.md)，标准源授权沿用
[ADR-0030](0030-range-carrier-source-facts.md)。
