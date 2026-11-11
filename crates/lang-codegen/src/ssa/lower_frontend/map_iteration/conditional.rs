//! Ordinary body CFG uses a plan-bound source identity and append-only exit evidence.
mod actions;
mod facts;
pub(super) use retained as successor;
mod verify;

use super::*;
use lang_frontend::{
    ownership_checking::{DropFact, DropTarget},
    type_checking::MapIterationDescriptor,
};

pub(super) use actions::validate_drop_ledger;
pub(super) use facts::{qualify, source_drop};
pub(super) use verify::validate;
pub(super) use self::{local_validate as local, transport_roles as transport};

#[derive(Clone, Copy)]
pub(super) struct Shape {
    pub(super) statement: StatementId,
    pub(super) body: StatementId,
    pub(super) source: SourceIdentity,
}

pub(super) struct State {
    pub(super) shape: Shape,
    pub(super) exits: Vec<Exit>,
}

impl State {
    pub(super) fn new(shape: Shape) -> Self {
        Self {
            shape,
            exits: Vec::new(),
        }
    }
}

/// These are the actual live entities immediately before this path's cleanup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Roles {
    pub(super) source: LoanId,
    pub(super) cursor: ValueId,
    pub(super) key: LoanId,
    pub(super) value: LoanId,
    pub(super) owner: Option<ValueId>,
}

#[derive(Clone, Copy)]
pub(super) struct CleanupFrame {
    pub(super) statement: StatementId,
    pub(super) roles: Roles,
    pub(super) owner: Option<(source_owner::Proof, SourceIdentity, ValueId)>,
}

pub(super) struct Exit {
    pub(super) kind: IterationExitKind,
    pub(super) block: BlockId,
    pub(super) roles: Roles,
    pub(super) frames: Vec<CleanupFrame>,
    pub(super) cleanup_start: usize,
    pub(super) cleanup_end: usize,
    pub(super) next: Option<ValueId>,
    pub(super) returned: Option<ValueId>,
}

/// Conditional local drops are admitted only when the complete source-qualified
/// plan has proved their original expression owner and ordering.
pub(super) fn supported_local_drop(
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
    fact: DropFact,
) -> bool {
    typed.map_iterations().iter().any(|descriptor| {
        super::qualify::qualify(parsed, names, typed, owned, *descriptor)
            .ok()
            .is_some_and(|qualified| {
                qualified.conditional.is_some()
                    && qualified
                        .plan
                        .exits()
                        .iter()
                        .any(|exit| exit.actions().contains(&IterationCleanupAction::Drop(fact)))
            })
    })
}

pub(super) fn shape(
    parsed: &ParsedFile,
    _names: &NameResolution,
    _typed: &TypedFile,
    descriptor: MapIterationDescriptor,
    body: StatementId,
    source: SourceIdentity,
    span: Span,
) -> Result<Option<Shape>, LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let node = parsed
        .ast()
        .statements()
        .get(descriptor.statement())
        .map_err(|_| missing())?;
    if !matches!(node.payload(), Statement::For { source, body: actual, .. }
        if *source == descriptor.source() && *actual == body)
    {
        return Err(missing());
    }
    // Every acquired provider uses the same full plan/body proof. A normal body
    // does not require a different lifetime or transport representation.
    Ok(Some(Shape {
        statement: descriptor.statement(),
        body,
        source,
    }))
}

pub(super) mod transport_roles {
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
}

pub(super) mod local_validate {
    use super::*;

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
}

pub(super) mod retained {
    //! A nested source may retain the exact owner whose lifetime belongs to an acquired ancestor.
    use super::*;
    use crate::ssa::map_source::Formation;
    use lang_frontend::ownership_checking::{
        CleanupOwnerValue, DropFact, DropTarget, IterationPhiBoundary,
    };

    pub(in super::super) fn ancestor_fact(
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
    pub(in super::super) fn terminal(
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
        let terminal =
            source_owner::phi(conditions, later, root, IterationPhiBoundary::Exit, *origin)?;
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
}
