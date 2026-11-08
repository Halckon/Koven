//! Ordinary call-result bindings keep control outside the published loan extent.
use std::collections::BTreeSet;

use super::{contains, local_if::binding_marker, ungroup};
use lang_frontend::{
    ast::StatementId,
    name_resolution::UnitSymbolId,
    ownership_checking::{
        BorrowBindingFact, BorrowBindingStorage, CompilationUnitOwnership, UnitDropPoint,
        UnitLoanTarget,
    },
    parser::{Expression, Item, ParsedFile, Statement},
    source::Span,
    type_checking::{CompilationUnitTypes, UnitExpressionId},
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

/// Only follow statement order through ordinary blocks. An end on a branch,
/// loop or control transfer cannot establish that a later statement sees no loan.
pub(in crate::ssa) fn disjoint_ifs(
    file: &ParsedFile,
    owned: &CompilationUnitOwnership,
    fact: &Binding,
) -> BTreeSet<usize> {
    let mut allowed = BTreeSet::new();
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
    collect_ifs(file, &elements[..binding_index], &mut allowed);

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
            collect_ifs(file, &elements[index + 1..], &mut allowed);
            true
        }
        _ => end == Some(scope),
    };

    let mut block = scope;
    while let Some((parent, siblings, index)) = parents[block.index()] {
        collect_ifs(file, &siblings[..index], &mut allowed);
        if ended_in_scope {
            collect_ifs(file, &siblings[index + 1..], &mut allowed);
        }
        block = parent;
    }
    allowed
}

fn collect_ifs(file: &ParsedFile, statements: &[StatementId], allowed: &mut BTreeSet<usize>) {
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
            _ => {}
        }
    }
}
