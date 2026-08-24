//! Koven 源码前端的公共 crate 边界。

/// 保留源码范围的索引式 AST 存储骨架。
pub mod ast;

/// 结构化诊断模型、错误码目录与稳定排序。
pub mod diagnostic;

/// 保留 token、注释与换行边界的源码格式化。
pub mod formatting;

/// 确定性 Koven 词法分析与逐字节源码范围。
pub mod lexer;

/// 单文件声明收集、词法作用域与名称引用。
pub mod name_resolution;

/// Phase 3 的变量所有权状态与 use-after-move 检查。
pub mod ownership_checking;

/// Pratt 表达式、类型引用与具体索引式 AST。
pub mod parser;

/// 源码身份、字节范围与展示位置。
pub mod source;

/// v0.22 基础类型检查、局部推导与 typed 产物。
pub mod type_checking;
