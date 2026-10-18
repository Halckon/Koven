//! A later provider may own the same binding epoch's final lexical Drop.
use super::*;
use crate::ssa::map_source::Formation;
use lang_frontend::ownership_checking::{
    CleanupOwnerValue, DropFact, DropTarget, IterationPhiBoundary,
};

/// Both full birth graphs are validated by the caller. Additionally require every
/// later incoming path to pass the current Exit, rather than only share a birth.
pub(super) fn terminal(
    frontend: (
        &ParsedFile,
        &NameResolution,
        &TypedFile,
        &OwnershipCheckedFile,
    ),
    flow: source_owner::Flow,
    fact: DropFact,
    formation: Option<&Formation<SymbolId>>,
    span: Span,
) -> Result<bool, LoweringError> {
    let (parsed, names, typed, owned) = frontend;
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let DropTarget::Named(root) = facts::source_target(flow.source) else {
        return Ok(false);
    };
    let conditions = owned.cleanup_conditions();
    let owner = fact.owner().ok_or_else(missing)?;
    let Some(CleanupOwnerValue::IterationPhi {
        statement,
        boundary: IterationPhiBoundary::Exit,
        symbol,
        origin,
    }) = conditions.owner_value(owner)
    else {
        return Ok(false);
    };
    if *statement == flow.statement || *symbol != root {
        return Ok(false);
    }
    let current = owned.map_iteration(flow.statement).ok_or_else(missing)?;
    let later = owned.map_iteration(*statement).ok_or_else(missing)?;
    let node = parsed
        .ast()
        .statements()
        .get(*statement)
        .map_err(|_| missing())?;
    if !matches!(node.payload(), Statement::For { .. })
        || node.span() != *origin
        || origin.source_id() != span.source_id()
        || origin.start() < span.end()
        || later.source_access() != MapIterationSourceAccess::Owned
        || later.source() != current.source()
        || !matches!(later.source(), Some(LoanTarget::Place(place)) if place.is_root() && place.root() == root)
        || later.descriptor().source_type() != current.descriptor().source_type()
        || typed.map_iteration(*statement) != Some(later.descriptor())
    {
        return Err(missing());
    }
    let declaration = names
        .symbols()
        .get(root.index())
        .ok_or_else(missing)?
        .span();
    let scope = resource::scope(parsed, flow.statement, declaration, span)?;
    if resource::scope(parsed, *statement, declaration, *origin)? != scope
        || !resource::normal_point(parsed, scope, *origin, fact.point())
    {
        return Err(missing());
    }
    if let Some(before) = formation {
        let after = formed::formation(
            parsed,
            names,
            typed,
            owned,
            root,
            later.descriptor().source_type(),
            *origin,
        )?;
        if before.birth != after.birth
            || before.binding != after.binding
            || before.origin != after.origin
            || before.declaration != after.declaration
            || before.roots != after.roots
            || before.updates != after.updates
            || !before
                .history
                .iter()
                .map(|event| (event.rhs, event.origin))
                .eq(after.history.iter().map(|event| (event.rhs, event.origin)))
        {
            return Err(missing());
        }
    }
    let exit = source_owner::phi(conditions, current, root, IterationPhiBoundary::Exit, span)?;
    let terminal = source_owner::phi(conditions, later, root, IterationPhiBoundary::Exit, *origin)?;
    let live = source_owner::provenance::from_exit(owned, root, owner, exit.owner(), span)?;
    if terminal.owner() != owner
        || fact.condition() != Some(live)
        || conditions.get(live) == Some(&CleanupCondition::Never)
    {
        return Err(missing());
    }
    let ledger = owned
        .cleanup_steps()
        .iter()
        .filter_map(|(point, action)| (*point == fact.point()).then_some(*action))
        .collect::<Vec<_>>();
    actions::validate_drop_ledger(owned, fact.point(), &ledger, span)?;
    if ledger
        .iter()
        .filter(|action| **action == IterationCleanupAction::Drop(fact))
        .count()
        != 1
    {
        return Err(missing());
    }
    Ok(true)
}
