//! Actual Call/entry births and typed owner-changing updates precede provider transport.
use super::*;
use crate::ssa::{map_source::Birth, model::Definition};

pub(in super::super) fn validate_formed_birth(
    lowerer: &UnitExpressionLowerer<'_>,
    root: UnitSymbolId,
    expected: Birth<UnitSymbolId>,
    current: EntityId,
    span: Span,
) -> Result<EntityId, LoweringError> {
    let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
    let storage = lowerer.typed.symbol_type(root).ok_or_else(bad)?;
    let formation = map_iteration::formed::formation(
        lowerer.parsed,
        lowerer.names,
        lowerer.typed,
        lowerer.owned,
        root,
        storage,
        span,
    )?;
    if formation.birth != expected
        || !lowerer.owned.formed_owned_locals().contains(&root)
        || lowerer.borrow_bindings.contains_key(&root)
        || !matches!(current, EntityId::Value(_))
    {
        return Err(bad());
    }
    let mut anchor = match expected {
        Birth::Call(expression) => {
            let expression = UnitExpressionId::new(root.source_unit(), expression);
            source::ordinary_birth(lowerer, expression, span)?
        }
        Birth::EntryParameter {
            root: parameter,
            reference,
            origin,
        } => {
            let (slot, marker) = map_iteration::formed::entry_parameter(
                lowerer.parsed,
                lowerer.names,
                lowerer.typed,
                lowerer.owned,
                parameter,
                storage,
                span,
            )?;
            let reference = lowerer
                .parsed
                .ast()
                .expressions()
                .get(reference)
                .map_err(|_| bad())?;
            let references = lowerer
                .names
                .names()
                .references()
                .iter()
                .filter(|r| {
                    r.source_unit() == parameter.source_unit() && r.span() == reference.span()
                })
                .collect::<Vec<_>>();
            if marker != origin
                || reference.payload() != &lang_frontend::parser::Expression::Name
                || !matches!(references.as_slice(), [r]
                    if r.target() == &lang_frontend::name_resolution::UnitReferenceTarget::Symbol(parameter))
            {
                return Err(bad());
            }
            let entry = lowerer.function.entry_block().ok_or_else(bad)?;
            let owner = transport::parameter(lowerer.function, entry, slot, span)?;
            if !matches!(owner, EntityId::Value(_)) {
                return Err(bad());
            }
            owner
        }
    };
    let storage = lowerer.iteration_type(storage, span)?;
    if lowerer.function.entity(anchor).map(|e| e.ty) != Some(EntityType::Value(storage)) {
        return Err(bad());
    }
    for update in &formation.updates {
        let expression = UnitExpressionId::new(root.source_unit(), update.expression);
        let receiver = UnitExpressionId::new(root.source_unit(), update.receiver);
        let node = lowerer
            .parsed
            .ast()
            .expressions()
            .get(update.expression)
            .map_err(|_| bad())?;
        let instructions = lowerer
            .function
            .instructions
            .iter()
            .filter(|i| {
                i.origin == Origin::Source(node.span())
                    && matches!(
                        i.operation,
                        Operation::MapPut { .. } | Operation::MapRemove { .. }
                    )
            })
            .collect::<Vec<_>>();
        let [instruction] = instructions.as_slice() else {
            return Err(bad());
        };
        let (input, result) = match (&instruction.operation, instruction.results.as_slice()) {
            (Operation::MapPut { owner, .. }, [EntityId::Value(result)]) => {
                let typed = lowerer.typed.map_put(expression).ok_or_else(bad)?;
                if typed.expression() != expression || typed.receiver() != receiver {
                    return Err(bad());
                }
                (*owner, *result)
            }
            (Operation::MapRemove { owner, .. }, [EntityId::Value(result), EntityId::Value(_)]) => {
                let typed = lowerer.typed.map_remove(expression).ok_or_else(bad)?;
                if typed.expression() != expression || typed.receiver() != receiver {
                    return Err(bad());
                }
                (*owner, *result)
            }
            _ => return Err(bad()),
        };
        if lowerer
            .function
            .entity(EntityId::Value(result))
            .map(|e| (e.ty, e.definition))
            != Some((
                EntityType::Value(storage),
                Definition::InstructionResult {
                    instruction: instruction.id,
                    index: 0,
                },
            ))
        {
            return Err(bad());
        }
        transport::from_anchor(lowerer.function, EntityId::Value(input), anchor, span)?;
        anchor = EntityId::Value(result);
    }
    transport::from_anchor(lowerer.function, current, anchor, span)?;
    if lowerer.bindings.iter().any(|(other, value)| *other != root
        && matches!(value, LoweredValue::Value(v) if EntityId::Value(*v) == current || EntityId::Value(*v) == anchor))
        || lowerer.temporaries.values().any(|v| EntityId::Value(*v) == current || EntityId::Value(*v) == anchor)
        || lowerer.pending_operands.iter().any(|v| *v == current || *v == anchor)
    { return Err(bad()); }
    Ok(anchor)
}
