//! AST owner 决定的 value / statement expression context。

use crate::{
    ast::{ExpressionId, StatementId},
    parser::{Expression, ParsedFile, Statement},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExpressionUse {
    Value,
    Statement,
}

pub(crate) fn collect_expression_uses(parsed: &ParsedFile) -> Vec<ExpressionUse> {
    let mut uses = vec![ExpressionUse::Value; parsed.ast().expressions().len()];
    for (_, node) in parsed.ast().statements().iter() {
        let elements = match node.payload() {
            Statement::Block { elements } => Some((elements.as_slice(), false)),
            Statement::LambdaBody { elements } | Statement::ControlBody { elements } => {
                Some((elements.as_slice(), true))
            }
            _ => None,
        };
        let Some((elements, tail_is_value)) = elements else {
            continue;
        };
        for (index, &statement) in elements.iter().enumerate() {
            if tail_is_value && index + 1 == elements.len() {
                continue;
            }
            if let Ok(statement) = parsed.ast().statements().get(statement)
                && let Statement::Expression { expression } = statement.payload()
            {
                mark_statement_expression(parsed, *expression, &mut uses);
            }
        }
    }
    uses
}

fn mark_statement_expression(parsed: &ParsedFile, id: ExpressionId, uses: &mut [ExpressionUse]) {
    uses[id.index()] = ExpressionUse::Statement;
    let Ok(node) = parsed.ast().expressions().get(id) else {
        return;
    };
    match node.payload() {
        Expression::Group { expression } => mark_statement_expression(parsed, *expression, uses),
        Expression::If {
            then_branch,
            else_branch,
            ..
        } => {
            mark_control_tail(parsed, *then_branch, uses);
            if let Some(else_branch) = else_branch {
                mark_control_tail(parsed, *else_branch, uses);
            }
        }
        Expression::When { entries, .. } => {
            for entry in entries {
                mark_control_tail(parsed, entry.body, uses);
            }
        }
        _ => {}
    }
}

fn mark_control_tail(parsed: &ParsedFile, statement: StatementId, uses: &mut [ExpressionUse]) {
    let Ok(node) = parsed.ast().statements().get(statement) else {
        return;
    };
    let Statement::ControlBody { elements } = node.payload() else {
        return;
    };
    let Some(last) = elements.last() else {
        return;
    };
    if let Ok(statement) = parsed.ast().statements().get(*last)
        && let Statement::Expression { expression } = statement.payload()
    {
        mark_statement_expression(parsed, *expression, uses);
    }
}
