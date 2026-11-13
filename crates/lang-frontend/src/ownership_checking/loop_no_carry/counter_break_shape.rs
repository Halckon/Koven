//! The only conditional exit admitted after two already closed counter arms.
use super::{BreakPlan, CounterBreakPlan, shape::Name};
use crate::{
    ast::{ExpressionId, StatementId},
    parser::{Expression, FunctionBody, FunctionForm, Item, ParsedFile, Statement},
};
pub(super) fn select<B: Copy>(
    parsed: &ParsedFile,
    target: StatementId,
    loop_body: StatementId,
    statement: StatementId,
    resolve: &impl Fn(Name) -> Option<B>,
    parameter: &impl Fn(ExpressionId, bool) -> bool,
) -> Option<CounterBreakPlan<B>> {
    let Statement::Expression {
        expression: control,
    } = parsed.ast().statements().get(statement).ok()?.payload()
    else {
        return None;
    };
    let Expression::If {
        condition,
        then_branch,
        else_branch: None,
        ..
    } = parsed.ast().expressions().get(*control).ok()?.payload()
    else {
        return None;
    };
    let name = parsed.ast().expressions().get(*condition).ok()?;
    if !matches!(name.payload(), Expression::Name) || !parameter(*condition, true) {
        return None;
    }
    let flag = resolve(Name::Reference(name.span()))?;
    let Statement::ControlBody { elements } =
        parsed.ast().statements().get(*then_branch).ok()?.payload()
    else {
        return None;
    };
    let [break_statement] = elements.as_slice() else {
        return None;
    };
    let Statement::Expression { expression } = parsed
        .ast()
        .statements()
        .get(*break_statement)
        .ok()?
        .payload()
    else {
        return None;
    };
    if !matches!(
        parsed.ast().expressions().get(*expression).ok()?.payload(),
        Expression::Break { .. }
    ) {
        return None;
    }
    // Do not combine this domain with the independently admitted prefix domain.
    let direct=parsed.ast().items().iter().any(|(_,node)| {
        let Item::Function {form,..}=node.payload() else{return false};
        let body=match form {FunctionForm::ImplicitUnitBlock(body)|FunctionForm::Explicit{body:FunctionBody::Block(body),..}=>*body,_=>return false};
        parsed.ast().statements().get(body).is_ok_and(|node|matches!(node.payload(),Statement::Block{elements} if elements.get(1)==Some(&target)))
    });
    direct.then_some(CounterBreakPlan {
        control: *control,
        condition: *condition,
        flag,
        body: *then_branch,
        exit: BreakPlan {
            loop_statement: target,
            loop_body,
            break_statement: *break_statement,
            break_expression: *expression,
        },
    })
}
