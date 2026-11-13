//! Shared finite straight-line syntax selection; typed and actual-state authority stays in adapters.
use super::Plan;
use crate::{
    ast::{ExpressionId, StatementId},
    parser::{Expression, Item, NameMarker, ParsedFile, Statement, VariableKind},
    source::Span,
};
pub(crate) enum Name {
    Declaration(NameMarker),
    Reference(Span),
}
pub(crate) struct Selection<B> {
    pub(crate) first: Plan<B>,
    pub(crate) second: Option<Plan<B>>,
    pub(crate) branch: Option<ExpressionId>,
}
pub(crate) fn selection<B: Copy + Eq>(
    parsed: &ParsedFile,
    loop_statement: StatementId,
    body: StatementId,
    resolve: impl Fn(Name) -> Option<B>,
    parameter: impl Fn(ExpressionId, bool) -> bool,
    call: impl Fn(ExpressionId, bool) -> bool,
) -> Option<Selection<B>> {
    if let Some(first) = plan(
        parsed,
        loop_statement,
        body,
        body,
        &resolve,
        &parameter,
        &call,
    ) {
        return Some(Selection {
            first,
            second: None,
            branch: None,
        });
    }
    let (Statement::Block { elements } | Statement::ControlBody { elements }) =
        parsed.ast().statements().get(body).ok()?.payload()
    else {
        return None;
    };
    let [statement] = elements.as_slice() else {
        return None;
    };
    let Statement::Expression { expression } =
        parsed.ast().statements().get(*statement).ok()?.payload()
    else {
        return None;
    };
    let Expression::If {
        condition,
        then_branch,
        else_branch: Some(else_branch),
        ..
    } = parsed.ast().expressions().get(*expression).ok()?.payload()
    else {
        return None;
    };
    if !parameter(*condition, true)
        || !matches!(
            parsed.ast().expressions().get(*condition).ok()?.payload(),
            Expression::Name
        )
        || then_branch == else_branch
    {
        return None;
    }
    let first = plan(
        parsed,
        loop_statement,
        body,
        *then_branch,
        &resolve,
        &parameter,
        &call,
    )?;
    let second = plan(
        parsed,
        loop_statement,
        body,
        *else_branch,
        &resolve,
        &parameter,
        &call,
    )?;
    if first.root != second.root
        || first.binding == second.binding
        || first.initializer == second.initializer
    {
        return None;
    }
    Some(Selection {
        first,
        second: Some(second),
        branch: Some(*expression),
    })
}
fn plan<B: Copy + Eq>(
    parsed: &ParsedFile,
    loop_statement: StatementId,
    loop_body: StatementId,
    body: StatementId,
    resolve: impl Fn(Name) -> Option<B>,
    parameter: impl Fn(ExpressionId, bool) -> bool,
    call: impl Fn(ExpressionId, bool) -> bool,
) -> Option<Plan<B>> {
    let loop_node = parsed.ast().statements().get(loop_statement).ok()?;
    let Statement::While {
        condition,
        body: actual,
        ..
    } = loop_node.payload()
    else {
        return None;
    };
    if *actual != loop_body
        || !parameter(*condition, true)
        || !matches!(
            parsed.ast().expressions().get(*condition).ok()?.payload(),
            Expression::Name
        )
    {
        return None;
    }
    let contains = |outer: Span, inner: Span| {
        outer.source_id() == inner.source_id()
            && outer.start() <= inner.start()
            && inner.end() <= outer.end()
    };
    if parsed.ast().statements().iter().any(|(id, n)| {
        id != loop_statement
            && contains(n.span(), loop_node.span())
            && matches!(
                n.payload(),
                Statement::While { .. } | Statement::For { .. } | Statement::Loop { .. }
            )
    }) || parsed.ast().expressions().iter().any(|(_, n)| {
        contains(n.span(), loop_node.span()) && matches!(n.payload(), Expression::Lambda { .. })
    }) {
        return None;
    }
    let callable = parsed
        .ast()
        .items()
        .iter()
        .filter(|(_, n)| {
            matches!(n.payload(), Item::Function { .. }) && contains(n.span(), loop_node.span())
        })
        .min_by_key(|(_, n)| n.span().end() - n.span().start())?
        .0;
    let Item::Function {
        form, parameters, ..
    } = parsed.ast().items().get(callable).ok()?.payload()
    else {
        return None;
    };
    let function_body = match form {
        crate::parser::FunctionForm::ImplicitUnitBlock(body) => *body,
        crate::parser::FunctionForm::Explicit {
            body: crate::parser::FunctionBody::Block(body),
            ..
        } => *body,
        _ => return None,
    };
    let Statement::Block { elements: siblings } =
        parsed.ast().statements().get(function_body).ok()?.payload()
    else {
        return None;
    };
    if !siblings.contains(&loop_statement) {
        return None;
    }
    let (Statement::Block { elements } | Statement::ControlBody { elements }) =
        parsed.ast().statements().get(body).ok()?.payload()
    else {
        return None;
    };
    let (first, rest) = elements.split_first()?;
    if rest.is_empty() {
        return None;
    }
    let Statement::LocalVariable { declaration } =
        parsed.ast().statements().get(*first).ok()?.payload()
    else {
        return None;
    };
    let Item::Variable {
        kind: VariableKind::BorrowVal(marker),
        name,
        initializer,
        ..
    } = parsed.ast().items().get(*declaration).ok()?.payload()
    else {
        return None;
    };
    let binding = resolve(Name::Declaration(*name))?;
    let source_call = *initializer;
    if !call(source_call, true) {
        return None;
    }
    let source_argument = one_argument(parsed, source_call)?;
    if !parameter(source_argument, false) {
        return None;
    }
    let root = resolve(Name::Reference(
        parsed.ast().expressions().get(source_argument).ok()?.span(),
    ))?;
    let condition_symbol = resolve(Name::Reference(
        parsed.ast().expressions().get(*condition).ok()?.span(),
    ))?;
    if !parameters
        .iter()
        .any(|p| resolve(Name::Declaration(p.name)) == Some(root))
        || !parameters
            .iter()
            .any(|p| resolve(Name::Declaration(p.name)) == Some(condition_symbol))
    {
        return None;
    }
    if root == binding {
        return None;
    }
    let mut consumers = Vec::new();
    for statement in rest {
        let Statement::Expression { expression } =
            parsed.ast().statements().get(*statement).ok()?.payload()
        else {
            return None;
        };
        if !call(*expression, false) {
            return None;
        }
        let argument = one_argument(parsed, *expression)?;
        if resolve(Name::Reference(
            parsed.ast().expressions().get(argument).ok()?.span(),
        )) != Some(binding)
        {
            return None;
        }
        consumers.push((*statement, *expression, argument));
    }
    Some(Plan {
        source: parsed.source_id(),
        loop_statement,
        body,
        callable,
        marker: *marker,
        binding,
        root,
        initializer: *initializer,
        source_call,
        source_argument,
        consumers,
    })
}
fn one_argument(parsed: &ParsedFile, id: ExpressionId) -> Option<ExpressionId> {
    let Expression::Call {
        callee, arguments, ..
    } = parsed.ast().expressions().get(id).ok()?.payload()
    else {
        return None;
    };
    if !matches!(
        parsed.ast().expressions().get(*callee).ok()?.payload(),
        Expression::Name
    ) {
        return None;
    }
    let [argument] = arguments.as_slice() else {
        return None;
    };
    matches!(
        parsed
            .ast()
            .expressions()
            .get(argument.value)
            .ok()?
            .payload(),
        Expression::Name
    )
    .then_some(argument.value)
}
