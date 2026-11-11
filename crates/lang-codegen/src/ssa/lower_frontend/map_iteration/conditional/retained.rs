//! A nested source may retain the exact owner whose lifetime belongs to an acquired ancestor.
use super::*;
use crate::ssa::map_source::Formation;
use lang_frontend::ownership_checking::{
    CleanupOwnerValue, DropFact, DropTarget, IterationPhiBoundary,
};

pub(super) fn ancestor_fact(
    frontend: (
        &ParsedFile,
        &NameResolution,
        &TypedFile,
        &OwnershipCheckedFile,
    ),
    flow: source_owner::Flow,
    fact: DropFact,
    span: Span,
) -> Result<bool, LoweringError> {
    let (parsed, names, typed, owned) = frontend;
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let current = owned.map_iteration(flow.statement).ok_or_else(missing)?;
    let child = parsed
        .ast()
        .statements()
        .get(flow.statement)
        .map_err(|_| missing())?
        .span();
    let DropTarget::Named(root) = facts::source_target(flow.source) else {
        return Ok(false);
    };
    if fact.target() != DropTarget::Named(root) {
        return Err(missing());
    }
    for ancestor in owned.map_iterations() {
        let statement = ancestor.descriptor().statement();
        if statement == flow.statement
            || ancestor.source_access() != MapIterationSourceAccess::Owned
            || ancestor.source() != current.source()
        {
            continue;
        }
        let node = parsed
            .ast()
            .statements()
            .get(statement)
            .map_err(|_| missing())?;
        let Statement::For { body, .. } = node.payload() else {
            return Err(missing());
        };
        let body = parsed
            .ast()
            .statements()
            .get(*body)
            .map_err(|_| missing())?
            .span();
        if body.source_id() != child.source_id()
            || body.start() > child.start()
            || child.end() > body.end()
        {
            continue;
        }
        if ancestor.descriptor().source_type() != current.descriptor().source_type() {
            return Err(missing());
        }
        // A terminal Drop belongs to the ancestor's actual EndSource segment,
        // even when this same point also retires the nested provider.
        let mut exits = ancestor
            .exits()
            .iter()
            .filter(|exit| exit.point() == fact.point());
        let exit = exits.next();
        if exits.next().is_some() {
            return Err(missing());
        }
        if let Some(exit) = exit
            && actions::source_drop(owned, ancestor, flow.source, exit, span)? == Some(fact)
        {
            return Ok(true);
        }
        if typed.is_resource_type(ancestor.descriptor().source_type()) != Some(true) {
            continue;
        }
        let Some(CleanupOwnerValue::IterationPhi {
            statement: owner_statement,
            boundary: IterationPhiBoundary::Exit,
            symbol,
            origin,
        }) = fact
            .owner()
            .and_then(|owner| owned.cleanup_conditions().owner_value(owner))
        else {
            continue;
        };
        if *owner_statement != statement {
            continue;
        }
        if *symbol != root || *origin != node.span() {
            return Err(missing());
        }
        // Lexical Resource cleanup follows this ancestor's live Exit owner,
        // not the nested provider's different Exit phi or a same-typed owner.
        let phi = source_owner::phi(
            owned.cleanup_conditions(),
            ancestor,
            root,
            IterationPhiBoundary::Exit,
            node.span(),
        )?;
        let origin = names
            .symbols()
            .get(root.index())
            .ok_or_else(missing)?
            .span();
        let scope = resource::scope(parsed, statement, origin, node.span())?;
        let live = source_owner::flow_condition(
            parsed,
            names,
            owned,
            source_owner::Flow {
                statement,
                source: flow.source,
                span: node.span(),
                late: None,
            },
            phi.owner(),
            span,
        )?;
        if fact.owner() != Some(phi.owner())
            || fact.condition() != Some(live)
            || !resource::normal_point(parsed, scope, node.span(), fact.point())
        {
            return Err(missing());
        }
        let cleanup = owned
            .cleanup_steps()
            .iter()
            .filter_map(|(point, action)| (*point == fact.point()).then_some(*action))
            .collect::<Vec<_>>();
        actions::validate_drop_ledger(owned, fact.point(), &cleanup, span)?;
        if cleanup
            .iter()
            .filter(|action| **action == IterationCleanupAction::Drop(fact))
            .count()
            != 1
        {
            return Err(missing());
        }
        return Ok(true);
    }
    Ok(false)
}

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
