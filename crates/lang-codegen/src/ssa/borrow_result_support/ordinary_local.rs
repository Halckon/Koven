//! Ordinary call-result bindings keep control outside the published loan extent.
use std::collections::{BTreeMap, BTreeSet};

use super::{contains, local_if::binding_marker, ungroup};
use lang_frontend::{
    ast::{ExpressionId, StatementId},
    name_resolution::{SourceUnitId, UnitSymbolId},
    ownership_checking::{
        BorrowBindingFact, BorrowBindingStorage, CompilationUnitOwnership, UnitDropPoint,
        UnitLoanTarget,
    },
    parser::{Expression, Item, ParsedFile, Statement, VariableKind},
    source::Span,
    type_checking::{CompilationUnitTypes, Copyability, ParameterMode, UnitExpressionId},
};

type Binding = BorrowBindingFact<UnitExpressionId, UnitSymbolId, UnitLoanTarget>;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::ssa) enum Policy {
    Scoped,
    WithOwnershipPrimitive,
}

pub(in crate::ssa) fn policy(
    file: &ParsedFile,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    fact: &Binding,
) -> Option<Policy> {
    let source = fact.source_loan()?;
    if fact.storage() != BorrowBindingStorage::BorrowedStorage
        || !matches!(fact.origin(), UnitLoanTarget::Place(place) if place.element().is_none())
        || typed.map_require_value(source.call()).is_some()
        || typed.call(source.call())?.borrow_return().is_none()
    {
        return None;
    }

    let source_unit = fact.binding().source_unit();
    if source.call().source_unit() != source_unit
        || source.argument().source_unit() != source_unit
        || ungroup(file, fact.initializer().expression(), fact.marker_span()).ok()?
            != source.call().expression()
    {
        return None;
    }
    let argument = ungroup(file, source.argument().expression(), fact.marker_span()).ok()?;
    if typed.non_null_use(source.argument()).is_some()
        || typed
            .non_null_use(UnitExpressionId::new(source_unit, argument))
            .is_some()
    {
        return None;
    }

    let owner = callable(file, fact.marker_span())?;
    let has_primitive = typed
        .ownership_primitives()
        .iter()
        .map(|descriptor| descriptor.expression())
        .chain(
            owned
                .field_replacements()
                .iter()
                .map(|plan| plan.descriptor().expression()),
        )
        .filter(|expression| expression.source_unit() == fact.binding().source_unit())
        .any(|expression| {
            file.ast()
                .expressions()
                .get(expression.expression())
                .is_ok_and(|node| {
                    contains(owner, node.span()) && callable(file, node.span()) == Some(owner)
                })
        });
    Some(if has_primitive {
        Policy::WithOwnershipPrimitive
    } else {
        Policy::Scoped
    })
}

fn callable(file: &ParsedFile, span: Span) -> Option<Span> {
    file.ast()
        .items()
        .iter()
        .filter(|(_, node)| matches!(node.payload(), Item::Function { .. }))
        .map(|(_, node)| node.span())
        .chain(
            file.ast()
                .expressions()
                .iter()
                .filter(|(_, node)| matches!(node.payload(), Expression::Lambda { .. }))
                .map(|(_, node)| node.span()),
        )
        .filter(|candidate| contains(*candidate, span))
        .min_by_key(|candidate| candidate.end() - candidate.start())
}

/// Admit exact argument returns proved by the source loan's unwind.
/// For disjoint lexical controls, only follow statement order through ordinary blocks.
pub(in crate::ssa) fn allowed_controls(
    file: &ParsedFile,
    owned: &CompilationUnitOwnership,
    fact: &Binding,
    argument_controls: &BTreeMap<UnitExpressionId, BTreeSet<usize>>,
) -> BTreeSet<usize> {
    let mut allowed = argument_controls
        .get(&fact.initializer())
        .cloned()
        .unwrap_or_default();
    let collect = |statements: &[StatementId], allowed: &mut BTreeSet<usize>| {
        collect_disjoint_controls(
            file,
            statements,
            fact.binding().source_unit(),
            argument_controls,
            allowed,
        );
    };
    let mut parents = vec![None; file.ast().statements().len()];
    let mut acquisition = None;
    for (block, node) in file.ast().statements().iter() {
        let Statement::Block { elements } = node.payload() else {
            continue;
        };
        for (index, &statement) in elements.iter().enumerate() {
            if binding_marker(file, statement) == Some(fact.marker_span()) {
                acquisition = Some((block, elements.as_slice(), index));
            }
            parents[statement.index()] = Some((block, elements.as_slice(), index));
        }
    }
    let Some((scope, elements, binding_index)) = acquisition else {
        return allowed;
    };
    collect(&elements[..binding_index], &mut allowed);

    let mut ends = owned
        .borrow_results()
        .ends()
        .iter()
        .filter(|end| end.binding() == fact.binding());
    let end = match (ends.next().map(|end| end.point()), ends.next()) {
        (Some(UnitDropPoint::AfterStatement(end)), None)
            if end.source_unit() == fact.binding().source_unit() =>
        {
            Some(end.statement())
        }
        _ => None,
    };
    let ended_in_scope = match end.and_then(|end| parents.get(end.index()).copied().flatten()) {
        Some((block, _, index)) if block == scope && index >= binding_index => {
            collect(&elements[index + 1..], &mut allowed);
            true
        }
        _ => end == Some(scope),
    };

    let mut block = scope;
    while let Some((parent, siblings, index)) = parents[block.index()] {
        collect(&siblings[..index], &mut allowed);
        if ended_in_scope {
            collect(&siblings[index + 1..], &mut allowed);
        }
        block = parent;
    }
    allowed
}

// The binding does not exist yet on a Return from an argument. Require the
// actual end of this call's source token there, independently of its normal path.
pub(in crate::ssa) fn argument_returns(
    file: &ParsedFile,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    fact: &Binding,
) -> Option<BTreeSet<usize>> {
    let source = fact.source_loan()?;
    let descriptor = typed.call(source.call())?;
    let call = file
        .ast()
        .expressions()
        .get(source.call().expression())
        .ok()?;
    let Expression::Call { arguments, .. } = call.payload() else {
        return None;
    };
    let mut allowed = BTreeSet::new();
    let unit = |id| UnitExpressionId::new(source.call().source_unit(), id);
    for mapping in descriptor.arguments() {
        if mapping.mode() != ParameterMode::Value
            || typed.copyability(mapping.parameter_type()) != Copyability::Copyable
        {
            continue;
        }
        let Some(argument) = arguments.get(mapping.argument_index()) else {
            continue;
        };
        let Ok(control) = ungroup(file, argument.value, argument.span) else {
            continue;
        };
        let Ok(node) = file.ast().expressions().get(control) else {
            continue;
        };
        let Expression::If {
            then_branch,
            else_branch: Some(else_branch),
            ..
        } = node.payload()
        else {
            continue;
        };
        if typed.expression_falls_through(unit(control)) != Some(true) {
            continue;
        }
        let Some(returned) =
            bare_return(file, *then_branch).or_else(|| bare_return(file, *else_branch))
        else {
            continue;
        };
        if owned.borrow_call_prefix_ends().iter().any(|end| {
            end.point() == UnitDropPoint::ControlTransfer(unit(returned))
                && end.loan().call() == source.call()
                && end.loan().argument() == source.argument()
                && end.loan().target() == fact.origin()
        }) {
            allowed.insert(control.index());
            allowed.insert(returned.index());
        }
    }
    (!allowed.is_empty()).then_some(allowed)
}

fn bare_return(file: &ParsedFile, statement: StatementId) -> Option<ExpressionId> {
    let statements = file.ast().statements();
    let node = statements.get(statement).ok()?;
    let tail = match node.payload() {
        Statement::Block { elements } | Statement::ControlBody { elements } => {
            statements.get(*elements.last()?).ok()?
        }
        _ => node,
    };
    let Statement::Expression { expression } = tail.payload() else {
        return None;
    };
    let expression = ungroup(file, *expression, tail.span()).ok()?;
    matches!(
        file.ast().expressions().get(expression).ok()?.payload(),
        Expression::Return { value: None, .. }
    )
    .then_some(expression)
}

fn collect_disjoint_controls(
    file: &ParsedFile,
    statements: &[StatementId],
    source_unit: SourceUnitId,
    argument_controls: &BTreeMap<UnitExpressionId, BTreeSet<usize>>,
    allowed: &mut BTreeSet<usize>,
) {
    let mut pending = statements.to_vec();
    while let Some(id) = pending.pop() {
        let Ok(node) = file.ast().statements().get(id) else {
            continue;
        };
        match node.payload() {
            Statement::Block { elements } => pending.extend_from_slice(elements),
            Statement::Expression { expression } => {
                if let Ok(expression) = ungroup(file, *expression, node.span())
                    && file
                        .ast()
                        .expressions()
                        .get(expression)
                        .is_ok_and(|node| matches!(node.payload(), Expression::If { .. }))
                {
                    allowed.insert(expression.index());
                }
            }
            Statement::LocalVariable { declaration } => {
                if let Ok(node) = file.ast().items().get(*declaration)
                    && let Item::Variable {
                        kind: VariableKind::BorrowVal(_),
                        initializer,
                        ..
                    } = node.payload()
                    && let Some(controls) =
                        argument_controls.get(&UnitExpressionId::new(source_unit, *initializer))
                {
                    allowed.extend(controls);
                }
            }
            _ => {}
        }
    }
}
