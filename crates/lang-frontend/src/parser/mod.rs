//! Koven v1 的 Pratt 表达式 Parser 与具体索引式 AST。

mod engine;
mod error;
mod lambda_trial;
mod output;
mod syntax;
mod trial;

use std::thread;

use crate::{lexer::LexedFile, source::SourceMap};

pub use error::ParserInternalError;
pub use output::*;
pub use syntax::*;

/// 从一份确定性词法产物解析完整源码文件。
pub fn parse_file(
    sources: &SourceMap,
    lexed: &LexedFile,
) -> Result<ParsedFile, ParserInternalError> {
    thread::scope(|scope| {
        thread::Builder::new()
            .name("koven-file-parser".to_owned())
            .stack_size(PARSER_STACK_SIZE)
            .spawn_scoped(scope, || engine::parse_file(sources, lexed))
            .map_err(|error| ParserInternalError::ParserThread(error.kind()))?
            .join()
            .map_err(|_| ParserInternalError::ParserThreadPanicked)?
    })
}

/// 从一份确定性词法产物解析唯一独立表达式。
///
/// 用户语法错误保留在返回产物中；source identity、AST、诊断模型或实现资源边界失败才返回
/// 具体内部错误。资源边界是编译器实现保护，不是 Koven 语法拒绝规则。
pub fn parse_expression(
    sources: &SourceMap,
    lexed: &LexedFile,
) -> Result<ParsedExpression, ParserInternalError> {
    // Pratt、分组与 TypeRef 都天然递归。固定隔离栈避免调用线程的栈配置改变结果；统一
    // 递归预算则在耗尽该实现资源前受控返回错误，而不是让用户输入触发栈溢出。
    thread::scope(|scope| {
        thread::Builder::new()
            .name("koven-expression-parser".to_owned())
            .stack_size(PARSER_STACK_SIZE)
            .spawn_scoped(scope, || engine::parse(sources, lexed))
            .map_err(|error| ParserInternalError::ParserThread(error.kind()))?
            .join()
            .map_err(|_| ParserInternalError::ParserThreadPanicked)?
    })
}

/// 从一份确定性词法产物解析唯一独立声明。
///
/// 用户语法错误保留在返回产物中；source identity、AST、诊断模型或实现资源边界失败才返回
/// 具体内部错误。声明与表达式入口共享同一固定工作栈和递归预算。
pub fn parse_declaration(
    sources: &SourceMap,
    lexed: &LexedFile,
) -> Result<ParsedDeclaration, ParserInternalError> {
    thread::scope(|scope| {
        thread::Builder::new()
            .name("koven-declaration-parser".to_owned())
            .stack_size(PARSER_STACK_SIZE)
            .spawn_scoped(scope, || engine::parse_declaration(sources, lexed))
            .map_err(|error| ParserInternalError::ParserThread(error.kind()))?
            .join()
            .map_err(|_| ParserInternalError::ParserThreadPanicked)?
    })
}

/// 从一份确定性词法产物解析唯一独立 block。
///
/// 用户语法错误保留在返回产物中；source identity、AST、诊断模型或实现资源边界失败才返回
/// 具体内部错误。block、声明与表达式入口共享同一固定工作栈和递归预算。
pub fn parse_block(
    sources: &SourceMap,
    lexed: &LexedFile,
) -> Result<ParsedBlock, ParserInternalError> {
    thread::scope(|scope| {
        thread::Builder::new()
            .name("koven-block-parser".to_owned())
            .stack_size(PARSER_STACK_SIZE)
            .spawn_scoped(scope, || engine::parse_block(sources, lexed))
            .map_err(|error| ParserInternalError::ParserThread(error.kind()))?
            .join()
            .map_err(|_| ParserInternalError::ParserThreadPanicked)?
    })
}

const PARSER_STACK_SIZE: usize = 32 * 1024 * 1024;
// 该值是实现安全预算，不是 Koven 语法语义；超过预算由 ParserInternalError 明确报告。
pub(crate) const MAX_RECURSION_DEPTH: usize = 1024;
