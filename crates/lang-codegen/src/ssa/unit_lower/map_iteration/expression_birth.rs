//! An ordinary If owns its selected tail result, not either arm's Call identity.
use super::*;

#[derive(Clone, Copy)]
pub(in super::super) struct Proof {
    pub(in super::super) condition: UnitExpressionId,
    pub(in super::super) arms: [StatementId; 2],
    pub(in super::super) results: [UnitExpressionId; 2],
}

pub(in super::super) fn qualify(
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    expression: UnitExpressionId,
    storage: UnitTypeId,
    span: Span,
) -> Result<Proof, LoweringError> {
    let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
    let unsupported = || lowering_error(LoweringErrorKind::UnsupportedNode, span);
    let node = parsed
        .ast()
        .expressions()
        .get(expression.expression())
        .map_err(|_| bad())?;
    let Expression::If {
        condition,
        then_branch,
        else_branch: Some(else_branch),
        ..
    } = node.payload()
    else {
        return Err(unsupported());
    };
    let condition = UnitExpressionId::new(expression.source_unit(), *condition);
    if typed.expression_type(expression) != Some(storage)
        || typed.expression_category(expression) != Some(ExpressionCategory::Temporary)
        || typed.copyability(storage) != Copyability::MoveOnly
        || typed
            .expression_type(condition)
            .and_then(|ty| builtin_type(typed, ty))
            != Some(BuiltinType::Boolean)
    {
        return Err(bad());
    }
    let arms = [*then_branch, *else_branch];
    let mut results = Vec::new();
    for arm in arms {
        let branch = parsed.ast().statements().get(arm).map_err(|_| bad())?;
        let tail = match branch.payload() {
            Statement::Expression { expression } => *expression,
            Statement::ControlBody { elements } => {
                let last = elements.last().ok_or_else(bad)?;
                match parsed
                    .ast()
                    .statements()
                    .get(*last)
                    .map_err(|_| bad())?
                    .payload()
                {
                    Statement::Expression { expression } => *expression,
                    _ => return Err(unsupported()),
                }
            }
            _ => return Err(unsupported()),
        };
        let tail = crate::ssa::map_source::transparent_expression(parsed, tail, |id| {
            let id = UnitExpressionId::new(expression.source_unit(), id);
            typed.expression_type(id).zip(typed.expression_category(id))
        })?;
        let tail = UnitExpressionId::new(expression.source_unit(), tail);
        local::factory(parsed, names, typed, owned, tail, storage, span)?;
        results.push(tail);
    }
    let [left, right] = results.as_slice() else {
        return Err(bad());
    };
    if left == right || arms[0] == arms[1] {
        return Err(bad());
    }
    Ok(Proof {
        condition,
        arms,
        results: [*left, *right],
    })
}
