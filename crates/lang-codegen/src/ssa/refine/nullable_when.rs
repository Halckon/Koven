//! Shared read-only provenance checks for nullable When admission and construction.
//! This borrows the analysis owner's plan; it does not create frontend facts or owners.
use lang_frontend::{
    name_resolution::SymbolKind,
    ownership_checking::{
        OwnershipBindingKind, UnitDropPoint, UnitDropTarget, UnitNullableWhenOwnershipPlan,
    },
    parser::{Expression, LiteralKind, WhenCondition},
    type_checking::{
        ExpressionCategory, NullableWhenSubjectCategory, UnitExpressionId, UnitNullableWhenDomain,
        UnitStatementId,
    },
};

use super::RefineInputs;
use crate::ssa::lowering_support::{LoweringError, LoweringErrorKind, error};

pub(super) struct Source<'a> {
    pub(super) plan: &'a UnitNullableWhenOwnershipPlan,
    /// Subject and transparent Groups in source order; drop points run leaf first.
    pub(super) operands: Vec<UnitExpressionId>,
}

/// Preserve the exact descriptor/ownership/AST identity before either consumer uses it.
/// Concrete pointer-like types and active binding/view state are checked by the caller.
pub(super) fn source<'i, 'a>(
    inputs: &'i RefineInputs<'a>,
    control: UnitExpressionId,
) -> Result<Source<'i>, LoweringError> {
    use UnitNullableWhenDomain::{Empty, NonNull, Null, Nullable};
    let missing = || error(LoweringErrorKind::MissingFact, inputs.span);
    let unsupported = || error(LoweringErrorKind::UnsupportedNode, inputs.span);
    // Preserve the original enclosing-callable boundary, even if another
    // admission check currently rejects the same combination earlier.
    if inputs.borrow_return.is_some() || inputs.carrier_return.is_some() {
        return Err(unsupported());
    }
    let descriptor = inputs.typed.nullable_when(control).ok_or_else(missing)?;
    let plan = inputs.owned.nullable_when(control).ok_or_else(missing)?;
    if control.source_unit() != inputs.source_unit || plan.descriptor() != descriptor {
        return Err(missing());
    }
    let node = inputs
        .parsed
        .ast()
        .expressions()
        .get(control.expression())
        .map_err(|_| missing())?;
    let Expression::When {
        subject: Some(subject),
        entries,
        ..
    } = node.payload()
    else {
        return Err(missing());
    };
    if descriptor.expression() != control
        || descriptor.subject() != UnitExpressionId::new(inputs.source_unit, *subject)
        || inputs.typed.expression_type(descriptor.subject()) != Some(descriptor.nullable_type())
        || entries.len() != 2
        || descriptor.entries().len() != 2
        || plan.entries().len() != 2
        || descriptor.falls_through() != plan.entries().iter().any(|entry| entry.falls_through())
    {
        return Err(missing());
    }
    for (index, ((actual, entry), owned)) in entries
        .iter()
        .zip(descriptor.entries())
        .zip(plan.entries())
        .enumerate()
    {
        if entry.index() != index
            || owned.index() != index
            || entry.body() != UnitStatementId::new(inputs.source_unit, actual.body)
            || owned.body() != entry.body()
            || entry.span() != actual.span
            || entry.is_else() != actual.else_span.is_some()
            || entry.is_else() != (index == 1)
            || entry.falls_through() != owned.falls_through()
            || entry.input_domain() != if index == 0 { Nullable } else { NonNull }
            || entry.body_domain() != if index == 0 { Null } else { NonNull }
            || entry.remaining_domain() != if index == 0 { NonNull } else { Empty }
        {
            return Err(missing());
        }
    }
    let [WhenCondition::Expression(condition)] = entries[0].conditions.as_slice() else {
        return Err(missing());
    };
    let [alternative] = descriptor.entries()[0].alternatives() else {
        return Err(missing());
    };
    let condition_node = inputs
        .parsed
        .ast()
        .expressions()
        .get(*condition)
        .map_err(|_| missing())?;
    if !matches!(
        condition_node.payload(),
        Expression::Literal(LiteralKind::Null)
    ) || alternative.expression() != UnitExpressionId::new(inputs.source_unit, *condition)
        || alternative.span() != condition_node.span()
        || alternative.input_domain() != Nullable
        || alternative.matched_domain() != Null
        || alternative.remaining_domain() != NonNull
        || !entries[1].conditions.is_empty()
        || !descriptor.entries()[1].alternatives().is_empty()
    {
        return Err(missing());
    }
    let mut operands = Vec::new();
    let mut current = descriptor.subject();
    loop {
        if current.source_unit() != inputs.source_unit
            || inputs.typed.expression_type(current) != Some(descriptor.nullable_type())
        {
            return Err(missing());
        }
        operands.push(current);
        let node = inputs
            .parsed
            .ast()
            .expressions()
            .get(current.expression())
            .map_err(|_| missing())?;
        if let Expression::Group { expression } = node.payload() {
            current = UnitExpressionId::new(inputs.source_unit, *expression);
        } else {
            break;
        }
    }
    match descriptor.source_category() {
        NullableWhenSubjectCategory::OwnedRoot => {
            let root = descriptor.stable_symbol().ok_or_else(unsupported)?;
            let leaf = inputs.parsed.ast().expressions().get(current.expression())
                .map_err(|_| missing())?;
            let span = leaf.span();
            let symbol = inputs.names.names().source_units().get(root.source_unit().index())
                .and_then(|unit| unit.resolution().symbols().get(root.symbol().index()))
                .ok_or_else(missing)?;
            if !matches!(symbol.kind(), SymbolKind::Variable | SymbolKind::ValueParameter)
                || symbol.is_synthetic()
            {
                return Err(unsupported());
            }
            let owns = match inputs.owned.binding_kind(root) {
                Some(OwnershipBindingKind::Owned) => true,
                Some(OwnershipBindingKind::Shared | OwnershipBindingKind::Exclusive) => false,
                None => symbol.kind() == SymbolKind::Variable
                    && inputs.owned.formed_owned_locals().contains(&root),
            };
            if !owns {
                return Err(missing());
            }
            if !matches!(leaf.payload(), Expression::Name)
                || descriptor.subject_reference() != Some(current)
                || descriptor.source_generation().is_none_or(|generation| generation == u64::MAX)
                || plan.source_place().is_none_or(|place| !place.is_root() || place.root() != root)
                || inputs.references.get(&(span.start(), span.end())).copied() != Some(root)
                || inputs.typed.symbol_type(root) != Some(descriptor.nullable_type())
                || operands.iter().any(|&operand| {
                    inputs.typed.expression_category(operand) != Some(ExpressionCategory::Place)
                })
            {
                return Err(missing());
            }
        }
        NullableWhenSubjectCategory::Temporary => {
            if descriptor.stable_symbol().is_some() || descriptor.subject_reference().is_some()
                || descriptor.source_generation().is_some() || plan.source_place().is_some()
                || !plan.uses().is_empty()
                || inputs.typed.nullable_when_uses().iter().any(|use_| use_.control() == control)
                || operands.iter().any(|&operand| {
                    inputs.typed.expression_category(operand) != Some(ExpressionCategory::Temporary)
                })
                || inputs.owned.drops().iter().any(|fact| {
                    matches!(fact.point(), UnitDropPoint::AfterExpression(point) if operands.contains(&point))
                        && matches!(fact.target(), UnitDropTarget::Temporary(target) if operands.contains(&target))
                })
                || inputs.owned.drops().iter().filter(|fact| {
                    fact.point() == UnitDropPoint::AfterExpression(control)
                        && fact.target() == UnitDropTarget::Temporary(descriptor.subject())
                }).count() != usize::from(descriptor.falls_through())
            {
                return Err(missing());
            }
        }
        _ => return Err(unsupported()),
    }
    Ok(Source { plan, operands })
}
