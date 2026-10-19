//! The two body gates consume actual assignment and builtin String descriptors.
use super::*;
use lang_frontend::parser::{AssignmentOperator, BinaryOperator};
fn assignment(
    parsed: &ParsedFile,
    id: lang_frontend::ast::ExpressionId,
) -> Result<
    (
        lang_frontend::ast::ExpressionId,
        lang_frontend::ast::ExpressionId,
        lang_frontend::ast::ExpressionId,
    ),
    Failure,
> {
    let Expression::Assignment {
        target,
        value,
        operator: AssignmentOperator::Assign,
        ..
    } = parsed.ast().expressions().get(id).unwrap().payload()
    else {
        return Err(failure("scope actual ordinary assignment", id));
    };
    let Expression::Binary {
        left,
        right,
        operator: BinaryOperator::Add,
        ..
    } = parsed.ast().expressions().get(*value).unwrap().payload()
    else {
        return Err(failure("scope actual assigned Add", value));
    };
    Ok((*target, *value, *left)).and_then(|v| {
        need(
            matches!(
                parsed.ast().expressions().get(*right).unwrap().payload(),
                Expression::Literal(_)
            ),
            "scope actual increment literal",
            right,
        )?;
        Ok(v)
    })
}
pub(super) fn single(
    input: Input<'_>,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
) -> Result<(), Failure> {
    let (c, _, parsed, i) = input;
    if let Some(id) = i.assignment {
        let (target, rhs, left) = assignment(parsed, id)?;
        let a = one(
            names
                .references()
                .iter()
                .filter(|r| r.span() == ast::span(parsed, target)),
            "scope assignment original target",
        )?;
        let b = one(
            names
                .references()
                .iter()
                .filter(|r| r.span() == ast::span(parsed, left)),
            "scope assignment current scalar read",
        )?;
        need(
            a.target() == b.target()
                && typed.expression_type(target) == typed.expression_type(rhs)
                && typed.expression_type(left) == typed.expression_type(rhs),
            "scope checked assigned scalar preserves actual type/root",
            (a, b),
        )?;
        let call = i.count_call.as_ref().unwrap();
        let d = typed
            .call(call.id)
            .ok_or_else(|| failure("scope actual checked result consumer", call))?;
        need(
            d.target() == CallableTarget::Source(symbol(names, i.count.as_ref().unwrap().name)?)
                && matches!(d.arguments(),[a]if a.mode()==ParameterMode::Borrow&&a.argument_index()==0&&a.parameter_index()==0),
            "scope exact count target and Borrow slot",
            d,
        )?;
        let r = one(
            names
                .references()
                .iter()
                .filter(|r| r.span() == ast::span(parsed, call.arguments[0])),
            "scope count actual current n",
        )?;
        need(
            r.target() == a.target(),
            "scope count observes same assigned scalar",
            r,
        )?;
    }
    if let Some(id) = i.clone {
        let d = typed
            .string_operation(id)
            .ok_or_else(|| failure("scope original typed StringClone", id))?;
        let effect = owned
            .string_effect(id)
            .ok_or_else(|| failure("scope original StringClone ownership", id))?;
        let root = symbol(names, i.cloned.unwrap())?;
        need(
            d.expression() == id
                && d.kind() == StringOperationKind::Clone
                && d.receiver_mode() == ParameterMode::Borrow
                && d.result_mode() == ParameterMode::Value
                && d.receiver_type() == d.result_type()
                && typed.expression_type(d.receiver()) == Some(d.receiver_type())
                && typed.expression_type(id) == Some(d.result_type())
                && typed.symbol_type(root) == Some(d.result_type())
                && effect.expression() == id
                && effect.receiver() == d.receiver()
                && effect.kind() == d.kind()
                && effect.result_type() == d.result_type()
                && owned.formed_owned_locals().contains(&root),
            "scope exact StringClone fresh current owner descriptor/effect",
            (d, effect),
        )?;
        if c.provider {
            let site = i.site.as_ref().unwrap();
            let reference = one(
                names
                    .references()
                    .iter()
                    .filter(|r| r.span() == ast::span(parsed, d.receiver())),
                "scope clone actual key reference",
            )?;
            need(
                reference.target() == &ReferenceTarget::Symbol(symbol(names, site.bindings[0])?),
                "scope clone borrows this provider key",
                reference,
            )?;
        }
        let print = i.clone_print.as_ref().unwrap();
        let drop = one(
            owned
                .drops()
                .iter()
                .filter(|d| d.target() == DropTarget::Named(root)),
            "scope unique independent cloned owner cleanup",
        )?;
        need(
            drop.point() == DropPoint::CallReturn(print.id)
                && drop.value_origin() == i.cloned.unwrap(),
            "scope cloned owner ends at actual last print CallReturn",
            drop,
        )?;
        let l = one(
            owned
                .loans()
                .iter()
                .filter(|l| l.call() == print.id && l.argument() == print.arguments[0]),
            "scope actual cloned print loan",
        )?;
        let end = one(
            owned
                .loan_ends()
                .iter()
                .filter(|l| l.call() == print.id && l.argument() == print.arguments[0]),
            "scope cloned print synchronous endpoint",
        )?;
        need(
            matches!(l.target(),LoanTarget::Place(p)if p.is_root()&&p.root()==root)
                && end.point() == LoanEndPoint::CallReturn(print.id),
            "scope print borrows exact fresh clone until return",
            (l, end),
        )?;
    }
    Ok(())
}
pub(super) fn unit(
    input: Input<'_>,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
) -> Result<(), Failure> {
    let (c, _, parsed, i) = input;
    let unit = one(
        names.names().source_units().iter(),
        "scope body original source unit",
    )?
    .source_unit();
    let e = |id| UnitExpressionId::new(unit, id);
    if let Some(id) = i.assignment {
        let (target, rhs, left) = assignment(parsed, id)?;
        let d = typed
            .assignment(e(id))
            .ok_or_else(|| failure("scope actual qualified assignment descriptor", id))?;
        need(
            d.expression() == e(id)
                && d.target() == e(target)
                && d.value() == e(rhs)
                && d.operator() == AssignmentOperator::Assign
                && d.falls_through()
                && typed.expression_type(e(target)) == Some(d.target_type())
                && typed.expression_type(e(left)) == Some(d.target_type())
                && typed.expression_type(e(rhs)) == Some(d.target_type()),
            "scope qualified exact current assignment typed descriptor",
            d,
        )?;
        let a = one(
            names
                .names()
                .references()
                .iter()
                .filter(|r| r.span() == ast::span(parsed, target)),
            "scope qualified assigned target",
        )?;
        let b = one(
            names
                .names()
                .references()
                .iter()
                .filter(|r| r.span() == ast::span(parsed, left)),
            "scope qualified assigned scalar read",
        )?;
        need(
            a.target() == b.target(),
            "scope qualified assigned scalar root",
            (a, b),
        )?;
        let call = i.count_call.as_ref().unwrap();
        let descriptor = typed
            .call(e(call.id))
            .ok_or_else(|| failure("scope qualified count call", call))?;
        let callee = one(
            names.names().index().declarations().iter().filter(|d| {
                d.name_span() == i.count.as_ref().unwrap().name && d.source_unit() == unit
            }),
            "scope qualified actual count target",
        )?;
        let arg = one(
            names
                .names()
                .references()
                .iter()
                .filter(|r| r.span() == ast::span(parsed, call.arguments[0])),
            "scope qualified current count reference",
        )?;
        need(
            descriptor.target() == UnitCallTarget::Declaration(callee.id())
                && matches!(descriptor.arguments(),[a]if a.mode()==ParameterMode::Borrow&&a.argument_index()==0&&a.parameter_index()==0)
                && arg.target() == a.target(),
            "scope qualified count uses latest assigned root and original Borrow callable",
            descriptor,
        )?;
    }
    if let Some(id) = i.clone {
        let d = typed
            .string_operation(e(id))
            .ok_or_else(|| failure("scope qualified original StringClone descriptor", id))?;
        let effect = owned
            .string_effect(e(id))
            .ok_or_else(|| failure("scope qualified actual StringClone effect", id))?;
        let root = unit_symbol(names, typed, i.cloned.unwrap())?;
        need(
            d.expression() == e(id)
                && d.kind() == StringOperationKind::Clone
                && d.receiver_mode() == ParameterMode::Borrow
                && d.result_mode() == ParameterMode::Value
                && d.receiver_type() == d.result_type()
                && typed.expression_type(d.receiver()) == Some(d.receiver_type())
                && typed.expression_type(e(id)) == Some(d.result_type())
                && typed.symbol_type(root) == Some(d.result_type())
                && effect.expression() == e(id)
                && effect.receiver() == d.receiver()
                && effect.kind() == d.kind()
                && effect.result_type() == d.result_type()
                && owned.formed_owned_locals().contains(&root),
            "scope qualified exact StringClone fresh original owner",
            (d, effect),
        )?;
        if c.provider {
            let site = i.site.as_ref().unwrap();
            let reference = one(
                names
                    .names()
                    .references()
                    .iter()
                    .filter(|r| r.span() == ast::span(parsed, d.receiver().expression())),
                "scope qualified clone actual key reference",
            )?;
            need(
                reference.target()
                    == &UnitReferenceTarget::Symbol(unit_symbol(names, typed, site.bindings[0])?),
                "scope qualified clone borrows original map key",
                reference,
            )?;
        }
        let print = i.clone_print.as_ref().unwrap();
        let drop = one(
            owned
                .drops()
                .iter()
                .filter(|d| d.target() == UnitDropTarget::Named(root)),
            "scope qualified unique cloned cleanup",
        )?;
        need(
            drop.point() == UnitDropPoint::CallReturn(e(print.id))
                && drop.value_origin() == i.cloned.unwrap(),
            "scope qualified independent clone CallReturn cleanup",
            drop,
        )?;
        let l = one(
            owned
                .loans()
                .iter()
                .filter(|l| l.call() == e(print.id) && l.argument() == e(print.arguments[0])),
            "scope qualified cloned print loan",
        )?;
        need(
            matches!(l.target(),UnitLoanTarget::Place(p)if p.is_root()&&p.root()==root)
                && l.end_span() == ast::span(parsed, print.id),
            "scope qualified print exact clone loan/return bound",
            l,
        )?;
    }
    Ok(())
}
