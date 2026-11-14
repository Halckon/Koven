//! Exact bounded scalar syntax only; actual arm closure and update are checked by adapters.
use super::{CounterPlan, shape::Name};
use crate::{
    ast::{ExpressionId, StatementId},
    parser::{
        AssignmentOperator, BinaryOperator, Expression, FunctionBody, FunctionForm,
        IntegerLiteralKind, Item, LiteralKind, ParsedFile, Statement, VariableKind,
    },
};

pub(super) fn select<B: Copy + Eq>(
    parsed: &ParsedFile,
    loop_statement: StatementId,
    body: StatementId,
    update: StatementId,
    resolve: impl Fn(Name) -> Option<B>,
    (integer, value_parameter, literal): (
        impl Fn(ExpressionId) -> bool,
        impl Fn(ExpressionId) -> bool,
        impl Fn(ExpressionId, &str) -> bool,
    ),
) -> Option<CounterPlan<B>> {
    let loop_node = parsed.ast().statements().get(loop_statement).ok()?;
    let Statement::While {
        condition,
        body: actual,
        ..
    } = loop_node.payload()
    else {
        return None;
    };
    if *actual != body {
        return None;
    }
    let Expression::Binary {
        left: header_read,
        operator: BinaryOperator::Greater,
        right: zero,
        ..
    } = parsed.ast().expressions().get(*condition).ok()?.payload()
    else {
        return None;
    };
    let Statement::Expression {
        expression: assignment,
    } = parsed.ast().statements().get(update).ok()?.payload()
    else {
        return None;
    };
    let Expression::Assignment {
        target,
        operator: AssignmentOperator::Assign,
        value: decrement,
        ..
    } = parsed.ast().expressions().get(*assignment).ok()?.payload()
    else {
        return None;
    };
    let Expression::Binary {
        left: decrement_operand,
        operator: BinaryOperator::Subtract,
        right: one,
        ..
    } = parsed.ast().expressions().get(*decrement).ok()?.payload()
    else {
        return None;
    };
    let name = |id| {
        let node = parsed.ast().expressions().get(id).ok()?;
        matches!(node.payload(), Expression::Name).then(|| resolve(Name::Reference(node.span())))?
    };
    let counter = name(*header_read)?;
    if name(*target) != Some(counter)
        || name(*decrement_operand) != Some(counter)
        || [
            *header_read,
            *target,
            *decrement_operand,
            *decrement,
            *zero,
            *one,
        ]
        .into_iter()
        .any(|id| !integer(id))
    {
        return None;
    }
    for (id, text) in [(*zero, "0"), (*one, "1")] {
        if !matches!(
            parsed.ast().expressions().get(id).ok()?.payload(),
            Expression::Literal(LiteralKind::Integer(IntegerLiteralKind::Unsuffixed))
        ) || !literal(id, text)
        {
            return None;
        }
    }
    let allowed = [*header_read, *target, *decrement_operand];
    if allowed[0] == allowed[1]
        || allowed[0] == allowed[2]
        || allowed[1] == allowed[2]
        || parsed.ast().expressions().iter().any(|(id, node)| {
            matches!(node.payload(), Expression::Name)
                && resolve(Name::Reference(node.span())) == Some(counter)
                && !allowed.contains(&id)
        })
    {
        return None;
    }
    let contains = |span: crate::source::Span| {
        span.source_id() == loop_node.span().source_id()
            && span.start() <= loop_node.span().start()
            && loop_node.span().end() <= span.end()
    };
    let (_, function) = parsed
        .ast()
        .items()
        .iter()
        .filter(|(_, node)| {
            matches!(node.payload(), Item::Function { .. }) && contains(node.span())
        })
        .min_by_key(|(_, node)| node.span().end() - node.span().start())?;
    let Item::Function {
        form, parameters, ..
    } = function.payload()
    else {
        return None;
    };
    let function_body = match form {
        FunctionForm::ImplicitUnitBlock(body) => *body,
        FunctionForm::Explicit {
            body: FunctionBody::Block(body),
            ..
        } => *body,
        _ => return None,
    };
    let Statement::Block { elements } =
        parsed.ast().statements().get(function_body).ok()?.payload()
    else {
        return None;
    };
    let position = elements.iter().position(|id| *id == loop_statement)?;
    let initial_statement = *elements.get(position.checked_sub(1)?)?;
    let Statement::LocalVariable { declaration } = parsed
        .ast()
        .statements()
        .get(initial_statement)
        .ok()?
        .payload()
    else {
        return None;
    };
    let Item::Variable {
        kind: VariableKind::Var,
        name: counter_name,
        initializer,
        ..
    } = parsed.ast().items().get(*declaration).ok()?.payload()
    else {
        return None;
    };
    let rounds = name(*initializer)?;
    if resolve(Name::Declaration(*counter_name)) != Some(counter)
        || rounds == counter
        || !integer(*initializer)
        || !value_parameter(*initializer)
        || !parameters
            .iter()
            .any(|p| resolve(Name::Declaration(p.name)) == Some(rounds))
    {
        return None;
    }
    Some(CounterPlan {
        counter,
        rounds,
        initializer: *initializer,
        initial_statement,
        condition: *condition,
        update,
        decrement: *decrement,
        decrement_operand: *decrement_operand,
    })
}
