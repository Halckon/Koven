//! Independent source-role and final-owned evidence reconciled with AfterChild only.
//! Test-only: this is neither a general admission route nor a dynamic generation certificate.
use lang_frontend::{
    ownership_checking::{
        LoanKind, OwnedCompilationUnitView, UnitDropPoint, UnitDropTarget, UnitLoanTarget,
        d10_catalog_experiment::{BoundAfterChildNestedLoop, ReplacementOrder},
    },
    parser::{Expression, Item, NameMarker, ParameterModeMarker, Statement},
    type_checking::{
        BuiltinType, OwnershipPrimitiveKind, UnitExpressionId, UnitItemId, UnitStatementId,
        UnitTypeKind,
    },
};
#[path = "d10_after_child_source_roles.rs"]
mod roles;

/// Independently exercise the closed AST envelope on a normal owned unit, before sidecar analysis.
/// This predicate alone does not confer the final-owned/primitive proof checked below.
pub(super) fn has_closed_roles(unit: &OwnedCompilationUnitView<'_, '_>) -> bool {
    roles::discover(unit).is_some()
}

pub(super) fn matches_bound(
    unit: &OwnedCompilationUnitView<'_, '_>,
    bound: &BoundAfterChildNestedLoop,
) -> Option<()> {
    let discovered = roles::discover(unit)?;
    let [input] = unit.inputs() else {
        return None;
    };
    let [names] = unit.names().names().source_units() else {
        return None;
    };
    let source = names.source_unit();
    let ast = input.parsed().ast();
    let expr = |id| UnitExpressionId::new(source, id);
    let [a, b, outer, parent_old, inner, child_old] = discovered.bindings;
    let controls = discovered
        .controls
        .map(|id| UnitStatementId::new(source, id));
    let calls = [expr(child_old.initializer), expr(parent_old.initializer)];
    let sites = [
        expr(a.initializer),
        expr(discovered.incoming[1]),
        expr(discovered.incoming[0]),
    ];
    let final_call = expr(discovered.final_use?);
    let suffix_call = expr(discovered.suffix);
    // All expected IDs above came from AST declarations/references, not from the bound.
    let plans = unit.ownership().ownership_primitives();
    if plans.len() != 2 {
        return None;
    }
    for ((call, incoming), plan) in calls.into_iter().zip(sites[1..].iter().copied()).zip(plans) {
        if plan.descriptor().expression() != call
            || plan.descriptor().kind() != OwnershipPrimitiveKind::Replace
            || plan.places().len() != 1
            || !plan.places()[0].is_root()
            || plan.places()[0].root() != a.symbol
            || plan.descriptor().operands().get(1) != Some(&incoming)
        {
            return None;
        }
    }
    let [child_drop, parent_drop, final_drop, suffix_drop] = unit.ownership().drops() else {
        return None;
    };
    for (drop, local, call) in [
        (child_drop, child_old, calls[0]),
        (parent_drop, parent_old, calls[1]),
    ] {
        let UnitDropPoint::AfterStatement(statement) = drop.point() else {
            return None;
        };
        if statement.source_unit() != source {
            return None;
        }
        let Statement::LocalVariable { declaration } =
            ast.statements().get(statement.statement()).ok()?.payload()
        else {
            return None;
        };
        let Item::Variable {
            initializer,
            name: NameMarker::Present(span),
            ..
        } = ast.items().get(*declaration).ok()?.payload()
        else {
            return None;
        };
        if *declaration != local.item
            || expr(*initializer) != call
            || *span != local.span
            || drop.target() != UnitDropTarget::Named(local.symbol)
            || drop.value_origin() != *span
        {
            return None;
        }
    }
    for (drop, local, call) in [(final_drop, a, final_call), (suffix_drop, b, suffix_call)] {
        if drop.target() != UnitDropTarget::Named(local.symbol)
            || drop.value_origin() != local.span
            || drop.point() != UnitDropPoint::CallReturn(call)
        {
            return None;
        }
    }
    let [child_loan, parent_loan, final_loan, suffix_loan] = unit.ownership().loans() else {
        return None;
    };
    for (loan, call, root, kind) in [
        (child_loan, calls[0], a.symbol, LoanKind::Exclusive),
        (parent_loan, calls[1], a.symbol, LoanKind::Exclusive),
        (final_loan, final_call, a.symbol, LoanKind::Shared),
        (suffix_loan, suffix_call, b.symbol, LoanKind::Shared),
    ] {
        let node = ast.expressions().get(call.expression()).ok()?;
        let Expression::Call { arguments, .. } = node.payload() else {
            return None;
        };
        let arg = arguments.first()?;
        let begin = match (kind, arg.mode_marker) {
            (LoanKind::Exclusive, Some(ParameterModeMarker::Inout(span))) => span,
            (LoanKind::Shared, None) => ast.expressions().get(arg.value).ok()?.span(),
            _ => return None,
        };
        if loan.call() != call
            || loan.argument() != expr(arg.value)
            || loan.kind() != kind
            || loan.begin_span() != begin
            || loan.end_span() != node.span()
            || !matches!(loan.target(),UnitLoanTarget::Place(place) if place.is_root() && place.root()==root)
        {
            return None;
        }
    }
    // Reconcile every public identity field only after the independent owned/source checks.
    if !bound.matches(unit)
        || bound.order() != ReplacementOrder::ChildThenParent
        || bound.controls()
            != (
                UnitItemId::new(source, discovered.function),
                controls[0],
                controls[1],
            )
        || bound.roots()
            != (
                a.symbol,
                outer.symbol,
                inner.symbol,
                discovered.parameters[0],
            )
        || bound.counter_parameters() != discovered.parameters
        || bound.counter_initializers() != [expr(outer.initializer), expr(inner.initializer)]
        || bound.counter_expressions()
            != [
                (expr(discovered.conditions[0]), expr(discovered.steps[0])),
                (expr(discovered.conditions[1]), expr(discovered.steps[1])),
            ]
        || bound.replacements() != calls
        || bound.producer_sites() != sites
        || bound.cleanup() != &[*child_drop, *parent_drop, *final_drop]
        || bound.loans() != &[child_loan.clone(), parent_loan.clone()]
    {
        return None;
    }
    let final_use = bound.final_root_use();
    let kept = bound.retained();
    if final_use.source() != (a.symbol, sites[0])
        || final_use.call() != final_call
        || final_use.loan() != final_loan
        || final_use.cleanup() != *final_drop
        || kept.source() != (b.symbol, expr(b.initializer))
        || kept.call() != suffix_call
        || kept.loan() != suffix_loan
        || kept.cleanup() != *suffix_drop
    {
        return None;
    }
    Some(())
}
