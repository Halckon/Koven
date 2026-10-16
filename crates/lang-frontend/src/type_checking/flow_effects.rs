//! 只读取语法的潜在写入收集；稳定来源解析与流事实失效由各类型检查入口负责。

use crate::{
    ast::{AstError, ExpressionId, StatementId},
    parser::{
        Expression, Item, ParameterModeMarker, Statement, StringPart, SyntaxAst, WhenCondition,
    },
};

enum Pending {
    Expression(ExpressionId),
    Statement(StatementId),
}

pub(super) fn collect_expression_mutation_targets(
    ast: &SyntaxAst,
    expression: ExpressionId,
) -> Result<Vec<ExpressionId>, AstError> {
    collect_mutation_targets(ast, Pending::Expression(expression))
}

pub(super) fn collect_statement_mutation_targets(
    ast: &SyntaxAst,
    statement: StatementId,
) -> Result<Vec<ExpressionId>, AstError> {
    collect_mutation_targets(ast, Pending::Statement(statement))
}

fn collect_mutation_targets(ast: &SyntaxAst, root: Pending) -> Result<Vec<ExpressionId>, AstError> {
    let mut pending = vec![root];
    let mut targets = Vec::new();
    // 覆盖每条可能执行的分支，不推演可达性、callee 效果或实际写入顺序。
    while let Some(node) = pending.pop() {
        match node {
            Pending::Expression(expression) => match ast.expressions().get(expression)?.payload() {
                Expression::Error
                | Expression::Name
                | Expression::This
                | Expression::Literal(_)
                | Expression::Break { .. }
                | Expression::Continue { .. }
                | Expression::SuperMember { .. } => {}
                // 创建 callable 不执行其声明体；不能把捕获体中的写入算作当前求值效果。
                Expression::Lambda { .. } => {}
                Expression::Group { expression }
                | Expression::Cast { expression, .. }
                | Expression::TypeTest { expression, .. } => {
                    pending.push(Pending::Expression(*expression));
                }
                Expression::Prefix { operand, .. } | Expression::NonNullAssert { operand, .. } => {
                    pending.push(Pending::Expression(*operand));
                }
                Expression::Propagate { value, .. } => {
                    pending.push(Pending::Expression(*value));
                }
                Expression::String { parts } => {
                    for part in parts.iter().rev() {
                        match part {
                            StringPart::Interpolation { expression, .. } => {
                                pending.push(Pending::Expression(*expression));
                            }
                            StringPart::Text(_) | StringPart::Error(_) => {}
                        }
                    }
                }
                Expression::If {
                    condition,
                    then_branch,
                    else_branch,
                    ..
                } => {
                    if let Some(branch) = else_branch {
                        pending.push(Pending::Statement(*branch));
                    }
                    pending.push(Pending::Statement(*then_branch));
                    pending.push(Pending::Expression(*condition));
                }
                Expression::When {
                    subject, entries, ..
                } => {
                    for entry in entries.iter().rev() {
                        pending.push(Pending::Statement(entry.body));
                        for condition in entry.conditions.iter().rev() {
                            match condition {
                                WhenCondition::Expression(expression)
                                | WhenCondition::Contains { expression, .. } => {
                                    pending.push(Pending::Expression(*expression));
                                }
                                WhenCondition::TypeTest { .. } => {}
                            }
                        }
                    }
                    if let Some(subject) = subject {
                        pending.push(Pending::Expression(*subject));
                    }
                }
                Expression::Return { value, .. } => {
                    if let Some(value) = value {
                        pending.push(Pending::Expression(*value));
                    }
                }
                Expression::Binary { left, right, .. } => {
                    pending.push(Pending::Expression(*right));
                    pending.push(Pending::Expression(*left));
                }
                Expression::Assignment { target, value, .. } => {
                    targets.push(*target);
                    pending.push(Pending::Expression(*value));
                    pending.push(Pending::Expression(*target));
                }
                Expression::Member { receiver, .. } => {
                    pending.push(Pending::Expression(*receiver));
                }
                Expression::Call {
                    callee, arguments, ..
                } => {
                    for argument in arguments {
                        match argument.mode_marker {
                            Some(ParameterModeMarker::Inout(_)) => targets.push(argument.value),
                            Some(ParameterModeMarker::Own(_) | ParameterModeMarker::Borrow(_))
                            | None => {}
                        }
                    }
                    for argument in arguments.iter().rev() {
                        pending.push(Pending::Expression(argument.value));
                    }
                    pending.push(Pending::Expression(*callee));
                }
                Expression::Index { receiver, index } => {
                    pending.push(Pending::Expression(*index));
                    pending.push(Pending::Expression(*receiver));
                }
                Expression::CallableReference { receiver, .. } => {
                    if let Some(receiver) = receiver {
                        pending.push(Pending::Expression(*receiver));
                    }
                }
            },
            Pending::Statement(statement) => match ast.statements().get(statement)?.payload() {
                Statement::Error => {}
                Statement::Block { elements }
                | Statement::LambdaBody { elements }
                | Statement::ControlBody { elements } => {
                    for element in elements.iter().rev() {
                        pending.push(Pending::Statement(*element));
                    }
                }
                Statement::LocalVariable { declaration } => {
                    match ast.items().get(*declaration)?.payload() {
                        Item::Variable { initializer, .. } => {
                            pending.push(Pending::Expression(*initializer));
                        }
                        Item::Error
                        | Item::Modified { .. }
                        | Item::Constant { .. }
                        | Item::Function { .. }
                        | Item::Classifier(_)
                        | Item::Companion(_)
                        | Item::Deinit { .. } => {}
                    }
                }
                Statement::LocalDestructuring { initializer, .. } => {
                    pending.push(Pending::Expression(*initializer));
                }
                Statement::While {
                    condition, body, ..
                } => {
                    pending.push(Pending::Statement(*body));
                    pending.push(Pending::Expression(*condition));
                }
                Statement::For { source, body, .. } => {
                    pending.push(Pending::Statement(*body));
                    pending.push(Pending::Expression(*source));
                }
                Statement::Loop { body, .. } => {
                    pending.push(Pending::Statement(*body));
                }
                Statement::Expression { expression } => {
                    pending.push(Pending::Expression(*expression));
                }
            },
        }
    }
    Ok(targets)
}
