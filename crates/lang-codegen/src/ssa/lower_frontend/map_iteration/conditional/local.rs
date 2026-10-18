//! A Flow local consumes its current SSA owner, rooted at the original declaration producer.
use super::*;
use lang_frontend::ownership_checking::{DropFact, DropTarget};

pub(in super::super) fn validate(
    frontend: (
        &ParsedFile,
        &NameResolution,
        &TypedFile,
        &OwnershipCheckedFile,
    ),
    function: &Function,
    fact: DropFact,
    owner: ValueId,
    span: Span,
) -> Result<(), LoweringError> {
    let (parsed, names, typed, owned) = frontend;
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let DropTarget::Named(root) = fact.target() else {
        return Err(error(LoweringErrorKind::UnsupportedNode, span));
    };
    let symbol = names.symbols().get(root.index()).ok_or_else(missing)?;
    if fact.value_origin() != symbol.span()
        || !owned.drops().contains(&fact)
        || fact.capture_slot().is_some()
        || fact.instance_address().is_some()
    {
        return Err(missing());
    }
    actions::binding_condition(
        parsed,
        names,
        typed,
        owned,
        root,
        fact.owner().ok_or_else(missing)?,
        span,
    )?;
    let (expression, origin) = actions::binding_birth(parsed, names, root, span)?;
    if expression.is_none() {
        return Err(error(LoweringErrorKind::UnsupportedNode, span));
    }
    let ty = function
        .entity(EntityId::Value(owner))
        .ok_or_else(missing)?
        .ty;
    let anchor = unique(function.instructions.iter().filter_map(|instruction| {
        let [EntityId::Value(value)] = instruction.results.as_slice() else {
            return None;
        };
        (instruction.origin == Origin::Source(origin)
            && function.entity(EntityId::Value(*value)).map(|data| data.ty) == Some(ty))
        .then_some(*value)
    }))
    .ok_or_else(missing)?;
    nested::transport(
        function,
        EntityId::Value(owner),
        EntityId::Value(anchor),
        span,
    )
}
