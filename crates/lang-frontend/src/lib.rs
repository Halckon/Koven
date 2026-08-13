//! Koven 源码前端的公共 crate 边界。

/// 保留源码范围的索引式 AST 存储骨架。
pub mod ast;

/// 结构化诊断模型、错误码目录与稳定排序。
pub mod diagnostic;

/// 确定性 Koven 词法分析与逐字节源码范围。
pub mod lexer;

/// Pratt 表达式、类型引用与具体索引式 AST。
pub mod parser;

/// 源码身份、字节范围与展示位置。
pub mod source;
