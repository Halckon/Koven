//! A nested source may retain the exact owner whose lifetime belongs to an acquired ancestor.
use super::*;
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
