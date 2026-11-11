//! Ordinary body CFG uses a plan-bound source identity and append-only exit evidence.
mod actions;
mod facts;
pub(super) mod retained;
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
