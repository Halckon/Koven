//! Stable Borrow bindings keep their real declaration, scope end and reborrow birth.
use super::*;
use crate::ssa::model::{Definition, EntityId, EntityType, Function, LoanKind, Operation, Origin};
use lang_frontend::parser::{NameMarker, VariableKind};

/// The published end is the enclosing ordinary callable body, not the provider exit.
pub(in crate::ssa) fn declaration(
    parsed: &ParsedFile,
    marker: Span,
    borrow_marker: Span,
    initializer: ExpressionId,
    scope: StatementId,
    provider: StatementId,
) -> Result<Span, LoweringError> {
    let bad = || error(LoweringErrorKind::MissingFact, marker);
    let unsupported = || error(LoweringErrorKind::UnsupportedNode, marker);
    let node = parsed.ast().statements().get(scope).map_err(|_| bad())?;
    let Statement::Block { elements } = node.payload() else {
        return Err(unsupported());
    };
    if !parsed.ast().items().iter().any(|(_, node)| {
        matches!(node.payload(),
        Item::Function { form: FunctionForm::ImplicitUnitBlock(body) | FunctionForm::Explicit {
            body: FunctionBody::Block(body), .. }, .. } if *body == scope)
    }) {
        return Err(unsupported());
    }
    let provider_index = elements
        .iter()
        .position(|id| *id == provider)
        .ok_or_else(unsupported)?;
    let mut declarations = Vec::new();
    for (index, &statement) in elements.iter().enumerate() {
        let node = parsed
            .ast()
            .statements()
            .get(statement)
            .map_err(|_| bad())?;
        let Statement::LocalVariable { declaration } = node.payload() else {
            continue;
        };
        let mut declaration = *declaration;
        loop {
            let item = parsed.ast().items().get(declaration).map_err(|_| bad())?;
            match item.payload() {
                Item::Modified {
                    declaration: inner, ..
                } => declaration = *inner,
                Item::Variable {
                    kind: VariableKind::BorrowVal(actual),
                    name: NameMarker::Present(name),
                    initializer: expression,
                    ..
                } if *name == marker && *expression == initializer && *actual == borrow_marker => {
                    declarations.push((index, node.span()));
                    break;
                }
                _ => break,
            }
        }
    }
    let [(index, declaration)] = declarations.as_slice() else {
        return Err(bad());
    };
    if *index >= provider_index {
        return Err(unsupported());
    }
    Ok(*declaration)
}

/// Match the original alias instruction, then prove every current incoming column.
/// No operand is evaluated and no unrelated same-type loan can serve as the seed.
pub(in crate::ssa) fn seed(
    function: &Function,
    current: EntityId,
    parent: EntityId,
    declaration: Span,
    span: Span,
    transport: impl Fn(&Function, EntityId, EntityId, Span) -> Result<(), LoweringError>,
) -> Result<EntityId, LoweringError> {
    let bad = || error(LoweringErrorKind::MissingFact, span);
    let Some(EntityType::Loan {
        kind: LoanKind::Shared,
        ..
    }) = function.entity(parent).map(|e| e.ty)
    else {
        return Err(bad());
    };
    let entry = function.entry_block().ok_or_else(bad)?;
    if !matches!(function.entity(parent).map(|e| e.definition),
        Some(Definition::BlockParameter { block, .. }) if block == entry)
    {
        return Err(bad());
    }
    let births = function
        .instructions
        .iter()
        .filter(|i| {
            i.origin == Origin::Source(declaration)
                && matches!(i.operation, Operation::SharedReborrow { .. })
        })
        .collect::<Vec<_>>();
    let [birth] = births.as_slice() else {
        return Err(bad());
    };
    let Operation::SharedReborrow { source } = birth.operation else {
        return Err(bad());
    };
    let [anchor @ EntityId::Loan(_)] = birth.results.as_slice() else {
        return Err(bad());
    };
    if function.entity(*anchor).map(|e| (e.ty, e.definition))
        != function.entity(parent).map(|e| {
            (
                e.ty,
                Definition::InstructionResult {
                    instruction: birth.id,
                    index: 0,
                },
            )
        })
    {
        return Err(bad());
    }
    transport(function, EntityId::Loan(source), parent, span)?;
    transport(function, current, *anchor, span)?;
    Ok(*anchor)
}
