//! The local normal slice is a bounded sequential family, not arbitrary surrounding CFG.
use super::*;

pub(super) struct Family<'a> {
    pub(super) elements: &'a [StatementId],
    pub(super) source: SourceUnitId,
    pub(super) span: Span,
}

pub(super) fn qualify(
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    family: Family<'_>,
) -> Result<(), LoweringError> {
    let Family {
        elements,
        source,
        span,
    } = family;
    let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
    let unsupported = || lowering_error(LoweringErrorKind::UnsupportedNode, span);
    let mut index = 0;
    let mut locals = 0;
    while index < elements.len() {
        let node = parsed
            .ast()
            .statements()
            .get(elements[index])
            .map_err(|_| bad())?;
        let Statement::LocalVariable { declaration } = node.payload() else {
            // Uncovered expressions use the existing straight-expression capability.
            // Additional local owners, loops and control flow are outside this family.
            let Statement::Expression { expression } = node.payload() else {
                return Err(unsupported());
            };
            straight_expression(parsed, *expression, span)?;
            for (id, expression_node) in parsed.ast().expressions().iter() {
                if !contains(node.span(), expression_node.span())
                    || !matches!(expression_node.payload(), Expression::Call { .. })
                {
                    continue;
                }
                let call_id = UnitExpressionId::new(source, id);
                let call = typed.call(call_id).ok_or_else(bad)?;
                let ty = typed.expression_type(call_id).ok_or_else(bad)?;
                if ty != call.return_type() {
                    return Err(bad());
                }
                if call.aborts() || builtin_type(typed, ty) == Some(BuiltinType::Nothing) {
                    return Err(unsupported());
                }
                match typed.is_resource_type(ty) {
                    Some(false) => {}
                    Some(true) => return Err(unsupported()),
                    None => return Err(bad()),
                }
            }
            index += 1;
            continue;
        };
        locals += 1;
        if locals > 2 {
            return Err(unsupported());
        }
        let (_, item, _) = unwrap_modified(parsed, *declaration)?;
        let Item::Variable {
            kind: VariableKind::Val | VariableKind::Var,
            name: NameMarker::Present(marker),
            ..
        } = item
        else {
            return Err(unsupported());
        };
        let next = *elements.get(index + 1).ok_or_else(unsupported)?;
        let for_node = parsed.ast().statements().get(next).map_err(|_| bad())?;
        if !matches!(for_node.payload(), Statement::For { .. }) {
            return Err(unsupported());
        }
        let statement = UnitStatementId::new(source, next);
        let descriptor = typed.map_iteration(statement).ok_or_else(unsupported)?;
        // No AcquireProvider remains the original legal Unsupported boundary.
        let plan = owned.map_iteration(statement).ok_or_else(unsupported)?;
        if plan.descriptor() != descriptor {
            return Err(bad());
        }
        let IterationSource::BorrowedSource(UnitLoanTarget::Place(place)) = plan.source_kind()
        else {
            return Err(unsupported());
        };
        if !place.is_root() || place.root().source_unit() != source {
            return Err(unsupported());
        }
        let symbol = names
            .names()
            .source_units()
            .get(source.index())
            .and_then(|source| {
                source
                    .resolution()
                    .symbols()
                    .get(place.root().symbol().index())
            })
            .ok_or_else(bad)?;
        if symbol.kind() != SymbolKind::Variable || symbol.span() != marker {
            return Err(unsupported());
        }
        index += 2;
        let Some(next) = elements.get(index) else {
            continue;
        };
        let next_node = parsed.ast().statements().get(*next).map_err(|_| bad())?;
        let Statement::Expression { expression } = next_node.payload() else {
            continue;
        };
        let node = parsed
            .ast()
            .expressions()
            .get(*expression)
            .map_err(|_| bad())?;
        let Expression::If { condition, .. } = node.payload() else {
            continue;
        };
        let condition = parsed
            .ast()
            .expressions()
            .get(*condition)
            .map_err(|_| bad())?;
        let Expression::Binary {
            left,
            operator: BinaryOperator::Equal,
            ..
        } = condition.payload()
        else {
            return Err(unsupported());
        };
        let size = typed
            .map_size(UnitExpressionId::new(source, *left))
            .ok_or_else(unsupported)?;
        let receiver = parsed
            .ast()
            .expressions()
            .get(size.receiver().expression())
            .map_err(|_| bad())?;
        if size.receiver().source_unit() != source {
            return Err(bad());
        }
        if receiver.payload() != &Expression::Name {
            return Err(unsupported());
        }
        let references = names
            .names()
            .references()
            .iter()
            .filter(|reference| {
                reference.source_unit() == source
                    && reference.namespace() == Some(Namespace::Value)
                    && reference.span() == receiver.span()
            })
            .collect::<Vec<_>>();
        let [reference] = references.as_slice() else {
            return Err(bad());
        };
        if reference.target() != &UnitReferenceTarget::Symbol(place.root()) {
            return Err(unsupported());
        }
        qualify_size_successor(parsed, typed, *next, size.expression(), span)?;
        index += 1;
    }
    if locals == 0 {
        return Err(unsupported());
    }
    Ok(())
}
