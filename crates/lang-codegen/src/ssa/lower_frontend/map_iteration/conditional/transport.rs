//! Read the current Map roles; ordinary CFG slots own branch/merge transportation.
use super::*;

pub(in super::super) fn roles(
    lowerer: &ExpressionLowerer<'_>,
    state: &Context,
    span: Span,
) -> Result<Roles, LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, span);
    if state.conditional.is_none()
        || state.cleaned
        || state.frame.finished
        || state.body != lowerer.block
    {
        return Err(missing());
    }
    let [(key_symbol, Some(key)), (value_symbol, Some(value))] = state.frame.components else {
        return Err(missing());
    };
    if lowerer.borrow_bindings.get(&key_symbol) != Some(&key)
        || lowerer.borrow_bindings.get(&value_symbol) != Some(&value)
        || state
            .owner
            .is_some_and(|(_, source, owner)| source.current_owner(lowerer) != Some(owner))
    {
        return Err(missing());
    }
    Ok(Roles {
        source: state.frame.source.ok_or_else(missing)?,
        cursor: state.cursor,
        key,
        value,
        owner: state.owner.map(|(_, _, owner)| owner),
    })
}
