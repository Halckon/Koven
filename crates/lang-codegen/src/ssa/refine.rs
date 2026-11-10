//! ADR-0032 ownership refinement layer: typed and ownership facts become a slot-based
//! CFG with explicit moves, drops and loans; SSA construction is generic.
//!
//! During migration `lower_owned_unit_with_entry` routes a function instance here only
//! when [`accepts`] proves its whole body lies inside the migrated subset. Once routed,
//! a failure is an error; there is no fallback to the per-family lowering.
mod body;
mod build;
#[cfg(test)]
pub(crate) use build::owner_consumer_test_support::{
    OwnerFault, observe_owner_actions, with_owner_fault,
};
mod emit;
pub(crate) mod loan_receipt;
pub(crate) mod receiver_receipt;
mod subset;

use std::collections::BTreeMap;

use lang_frontend::{
    ast::{ExpressionId, ItemId, StatementId},
    name_resolution::{DeclarationId, SourceUnitId, UnitSymbolId, ValidatedCompilationUnitNames},
    ownership_checking::{
        CompilationUnitOwnership, ConstEnabledOwnedUnit, LoanKind, UnitDropPoint, UnitDropTarget,
        UnitLoanTarget,
    },
    parser::{Expression, ParsedFile},
    source::{SourceMap, Span},
    type_checking::{
        CompilationUnitTypes, Copyability, ExpressionCategory, NominalKind, ParameterMode,
        UnitAggregateProjectionKind, UnitAggregateProjectionReceiver, UnitExpressionId, UnitTypeId,
        UnitTypeKind,
    },
};

use super::{
    lowering_support::{LoweringError, callable_instances::SourceToken},
    model::{BlockId, EntityId, Function, FunctionId, SsaTypeId},
    unit_plan::{UnitCallablePlan, UnitFunctionInstanceKey},
};

#[derive(Clone, Debug)]
pub(in crate::ssa) enum RefineBody {
    Expression(ExpressionId),
    Block(StatementId),
    /// A lambda thunk body (ADR-0034 §1).
    Lambda(RefineThunk),
}

/// One capture slot in a lambda thunk's borrowed environment (ADR-0034 §1).
#[derive(Clone, Copy, Debug)]
pub(in crate::ssa) struct RefineThunkCapture {
    pub(in crate::ssa) symbol: UnitSymbolId,
    pub(in crate::ssa) ty: UnitTypeId,
    pub(in crate::ssa) storage: SsaTypeId,
    pub(in crate::ssa) mode: lang_frontend::ownership_checking::ClosureCaptureMode,
    pub(in crate::ssa) span: Span,
}

/// A lambda thunk instance lowered by the refinement layer (ADR-0034 §1).
#[derive(Clone, Debug)]
pub(in crate::ssa) struct RefineThunk {
    pub(in crate::ssa) expression: lang_frontend::type_checking::UnitExpressionId,
    pub(in crate::ssa) body: StatementId,
    pub(in crate::ssa) result: Option<ExpressionId>,
    /// The closure's `.environment` aggregate type when `!captures.is_empty()`.
    pub(in crate::ssa) environment: Option<SsaTypeId>,
    pub(in crate::ssa) captures: Vec<RefineThunkCapture>,
}

/// The current receiver of a method instance; its entity precedes the parameters.
#[derive(Clone, Copy, Debug)]
pub(in crate::ssa) struct RefineReceiver {
    /// The classifier whose member this is; implicit-`this` projections name it.
    pub(in crate::ssa) owner: DeclarationId,
    pub(in crate::ssa) ty: UnitTypeId,
    /// Value, Borrow (a shared loan) or Inout (an exclusive loan).
    pub(in crate::ssa) mode: ParameterMode,
}

/// One capture of a lambda of this function instance (ADR-0034).
#[derive(Clone, Copy, Debug)]
pub(in crate::ssa) struct RefineCapture {
    pub(in crate::ssa) symbol: UnitSymbolId,
    /// The captured binding's SSA type.
    pub(in crate::ssa) ty: SsaTypeId,
    pub(in crate::ssa) mode: lang_frontend::ownership_checking::ClosureCaptureMode,
    pub(in crate::ssa) effect: lang_frontend::ownership_checking::ClosureCaptureEffect,
}

/// A lambda of this function instance: its callable value type, thunk and captures.
#[derive(Clone, Debug)]
pub(in crate::ssa) struct RefineCallable {
    /// Function pointer (no captures) or concrete closure type.
    pub(in crate::ssa) callable: SsaTypeId,
    pub(in crate::ssa) thunk: FunctionId,
    pub(in crate::ssa) captures: Vec<RefineCapture>,
}

/// Source-local lexical while facts, built at most once when an iteration route needs them.
#[derive(Default)]
pub(in crate::ssa) struct WhileScopes(
    std::cell::OnceCell<Option<std::collections::BTreeMap<(usize, usize), bool>>>,
);

/// Everything one function or lambda thunk instance needs; all facts come from the same unit.
pub(in crate::ssa) struct RefineInputs<'a> {
    pub(in crate::ssa) sources: &'a SourceMap,
    pub(in crate::ssa) parsed: &'a ParsedFile,
    pub(in crate::ssa) while_scopes: &'a WhileScopes,
    pub(in crate::ssa) source_unit: SourceUnitId,
    pub(in crate::ssa) names: &'a ValidatedCompilationUnitNames,
    pub(in crate::ssa) typed: &'a CompilationUnitTypes,
    pub(in crate::ssa) owned: &'a CompilationUnitOwnership,
    /// Present for the constant capability; const uses stay on the existing path.
    pub(in crate::ssa) constant_owned: Option<&'a ConstEnabledOwnedUnit>,
    pub(in crate::ssa) function_ids: &'a BTreeMap<UnitFunctionInstanceKey, FunctionId>,
    pub(in crate::ssa) source_plan: &'a UnitCallablePlan,
    pub(in crate::ssa) source_token: SourceToken,
    pub(in crate::ssa) references: &'a BTreeMap<(usize, usize), UnitSymbolId>,
    /// `Some` for a function instance, `None` for a lambda thunk.
    pub(in crate::ssa) function_item: Option<ItemId>,
    pub(in crate::ssa) body: RefineBody,
    pub(in crate::ssa) return_type: UnitTypeId,
    pub(in crate::ssa) receiver: Option<RefineReceiver>,
    /// Parameters in declaration order.
    pub(in crate::ssa) parameters: Vec<UnitSymbolId>,
    /// Per parameter: Value, Borrow (a shared loan) or Inout (an exclusive loan).
    pub(in crate::ssa) modes: Vec<ParameterMode>,
    pub(in crate::ssa) span: Span,
    /// The instance's type-parameter substitutions; empty for a non-generic function.
    pub(in crate::ssa) substitutions: &'a BTreeMap<UnitSymbolId, UnitTypeId>,
    /// The concrete `Self` of a static-self instance.
    pub(in crate::ssa) static_self: Option<UnitTypeId>,
    /// This instance's lambdas, by lambda expression (ADR-0034).
    pub(in crate::ssa) callables:
        &'a BTreeMap<lang_frontend::type_checking::UnitExpressionId, RefineCallable>,
    /// Callable view targets of Borrow function types, by concrete function type.
    pub(in crate::ssa) views: &'a BTreeMap<UnitTypeId, SsaTypeId>,
    /// Concrete callable return types of planned functions that return a callable.
    pub(in crate::ssa) callable_returns: &'a BTreeMap<FunctionId, SsaTypeId>,
    /// Sealed known-function targets mapped to their `(FunctionId, FunctionPointer SsaTypeId)`.
    pub(in crate::ssa) known_functions:
        &'a BTreeMap<lang_frontend::type_checking::UnitCallableTarget, (FunctionId, SsaTypeId)>,
    /// Source parameter index when the function returns a shared borrow.
    pub(in crate::ssa) borrow_return: Option<usize>,
    /// Source parameter/receiver index when the function returns a `RangeView` carrier.
    pub(in crate::ssa) carrier_return: Option<usize>,
}

impl RefineInputs<'_> {
    /// The lexical span of this function or lambda thunk.
    pub(in crate::ssa) fn scope_span(&self) -> Option<Span> {
        match self.function_item {
            Some(item) => self
                .parsed
                .ast()
                .items()
                .get(item)
                .ok()
                .map(|node| node.span()),
            None => Some(self.span),
        }
    }

    /// The concrete identity of a type read from the (template) typed facts; the identity
    /// itself for a non-generic function.
    pub(in crate::ssa) fn concrete(&self, ty: UnitTypeId) -> Option<UnitTypeId> {
        super::unit_plan::resolve_concrete_type(
            self.typed,
            ty,
            self.substitutions,
            self.static_self,
            self.span,
        )
        .ok()
    }

    /// Qualifies the shared field loan and the complete temporary that the call must clean up.
    fn temporary_field_argument(
        &self,
        call: UnitExpressionId,
        operand: ExpressionId,
        parameter: UnitTypeId,
    ) -> Option<(ExpressionId, bool)> {
        if call.source_unit() != self.source_unit {
            return None;
        }
        let nodes = self.parsed.ast().expressions();
        let argument = UnitExpressionId::new(self.source_unit, operand);
        let mut field = operand;
        let parent = 'projection: loop {
            while let Expression::Group { expression } = nodes.get(field).ok()?.payload() {
                field = *expression;
            }
            let id = UnitExpressionId::new(self.source_unit, field);
            let projection = self.typed.aggregate_projection(id)?;
            let UnitAggregateProjectionReceiver::Expression(mut receiver) = projection.receiver()
            else {
                return None;
            };
            if projection.expression() != id
                || projection.kind() != UnitAggregateProjectionKind::Field
                || receiver.source_unit() != self.source_unit
            {
                return None;
            }
            let receiver_ty = self.concrete(self.typed.expression_type(receiver)?)?;
            let UnitTypeKind::Nominal { declaration, .. } = self.typed.types().get(receiver_ty)?
            else {
                return None;
            };
            let kind = self
                .typed
                .signatures()
                .declaration(*declaration)?
                .nominal()?
                .kind();
            if !matches!(kind, NominalKind::Class | NominalKind::ValueClass) {
                return None;
            }
            loop {
                if self.typed.expression_category(receiver) == Some(ExpressionCategory::Temporary) {
                    break 'projection receiver;
                }
                let Expression::Group { expression } =
                    nodes.get(receiver.expression()).ok()?.payload()
                else {
                    break;
                };
                receiver = UnitExpressionId::new(self.source_unit, *expression);
            }
            field = receiver.expression();
        };
        let loan = self.owned.loans().iter().find(|loan| {
            loan.call() == call && loan.argument() == argument && loan.kind() == LoanKind::Shared
        })?;
        let UnitLoanTarget::Temporary(target) = loan.target() else {
            return None;
        };
        let actual = self.concrete(self.typed.expression_type(argument)?)?;
        let copied = actual != parameter;
        if copied
            && (*target != argument
                || self.typed.copyability(actual) != Copyability::Copyable
                || !matches!(self.typed.types().get(parameter), Some(UnitTypeKind::Nullable(inner)) if *inner == actual))
        {
            return None;
        }
        if !copied && *target != parent {
            return None;
        }
        // A completed prefix still evaluates the parent. An Abort path has no drop fact.
        if self.owned.call_may_dispatch(call) == Some(false)
            && self.owned.call_argument_completed(argument)
        {
            return Some((parent.expression(), copied));
        }
        let call_span = nodes.get(call.expression()).ok()?.span();
        let operand_span = nodes.get(operand).ok()?.span();
        self.owned
            .drops()
            .iter()
            .any(|drop| {
                drop.target() == UnitDropTarget::Temporary(parent)
                    && match drop.point() {
                        UnitDropPoint::CallReturn(id) => id == call,
                        UnitDropPoint::ControlTransfer(id) => {
                            id.source_unit() == self.source_unit
                                && nodes.get(id.expression()).is_ok_and(|node| {
                                    let span = node.span();
                                    operand_span.end() <= span.start()
                                        && span.end() <= call_span.end()
                                })
                        }
                        _ => false,
                    }
            })
            .then_some((parent.expression(), copied))
    }
}

/// Deterministic subset precheck; reads typed facts and AST construct kinds only.
pub(in crate::ssa) fn accepts(inputs: &RefineInputs<'_>) -> bool {
    subset::accepts(inputs)
}

pub(in crate::ssa) enum CallableRoute {
    Refine,
    Compatibility,
    #[cfg(test)]
    ForcedPerFamily,
}

/// Qualify the current callable before choosing a lowering path. The compatibility path
/// supports mixed bodies whose validated facts do not fit the complete refine subset.
pub(in crate::ssa) fn route(inputs: &RefineInputs<'_>) -> CallableRoute {
    if !accepts(inputs) {
        return CallableRoute::Compatibility;
    }
    #[cfg(test)]
    if let PerFamilyOverride::Forced { except } = PER_FAMILY_OVERRIDE.with(std::cell::Cell::get)
        && except.is_none_or(|span| inputs.scope_span() != Some(span))
    {
        return CallableRoute::ForcedPerFamily;
    }
    CallableRoute::Refine
}

#[cfg(test)]
#[derive(Clone, Copy)]
enum PerFamilyOverride {
    Disabled,
    Forced { except: Option<Span> },
}

#[cfg(test)]
thread_local! {
    static PER_FAMILY_OVERRIDE: std::cell::Cell<PerFamilyOverride> =
        const { std::cell::Cell::new(PerFamilyOverride::Disabled) };
}

/// Test-only: pin the per-family path to observe or fault-inject its private guards.
#[cfg(test)]
pub(crate) fn with_per_family_lowering<T>(lower: impl FnOnce() -> T) -> T {
    with_per_family_lowering_except(None, lower)
}

/// A fixture producer may retain normal routing while its consumers are pinned.
#[cfg(test)]
pub(in crate::ssa) fn with_per_family_lowering_except<T>(
    except: Option<Span>,
    lower: impl FnOnce() -> T,
) -> T {
    struct Reset(PerFamilyOverride);
    impl Drop for Reset {
        fn drop(&mut self) {
            PER_FAMILY_OVERRIDE.with(|state| state.set(self.0));
        }
    }
    let previous =
        PER_FAMILY_OVERRIDE.with(|state| state.replace(PerFamilyOverride::Forced { except }));
    let _reset = Reset(previous);
    lower()
}

/// Frontend types an accepted body needs in the SSA type table, in first-use order.
pub(in crate::ssa) fn type_demands(inputs: &RefineInputs<'_>) -> Vec<UnitTypeId> {
    subset::type_demands(inputs)
}

/// The unit's interned type identities, filled in after [`type_demands`].
pub(in crate::ssa) struct RefineTypes<'t> {
    pub(in crate::ssa) type_ids: &'t BTreeMap<UnitTypeId, SsaTypeId>,
    /// Class owner type to its payload aggregate.
    pub(in crate::ssa) heap_payloads: &'t BTreeMap<SsaTypeId, SsaTypeId>,
    /// (owner type, field symbol) to payload field index; enum roots index case payloads.
    pub(in crate::ssa) field_indices: &'t BTreeMap<(UnitTypeId, UnitSymbolId), usize>,
    /// (tagged union type, case value or type symbol) to (variant, payload aggregate).
    pub(in crate::ssa) enum_payloads: &'t BTreeMap<(SsaTypeId, UnitSymbolId), (usize, SsaTypeId)>,
    /// Registered `$map.result.tX` aggregate types mapped to their payload SSA types.
    pub(in crate::ssa) map_results: &'t BTreeMap<SsaTypeId, SsaTypeId>,
    /// Registered nullable map handle types mapped to their inner `MapContainer` SSA types.
    pub(in crate::ssa) nullable_maps: &'t BTreeMap<SsaTypeId, SsaTypeId>,
    /// Frozen module SSA type table.
    pub(in crate::ssa) ssa_types: &'t [super::model::SsaTypeKind],
}

/// Lowers an accepted function body into `function`, whose `entry` block holds
/// `parameters` (the receiver entity, if any, then one per `inputs.parameters` symbol).
pub(in crate::ssa) fn lower(
    inputs: &RefineInputs<'_>,
    function: &mut Function,
    entry: BlockId,
    parameters: &[EntityId],
    types: &RefineTypes<'_>,
) -> Result<(), LoweringError> {
    let body = build::build(inputs, parameters, types)?;
    emit::emit(&body, function, entry, parameters, inputs.span)?;
    #[cfg(test)]
    ROUTED.with(|routed| {
        if let Some(routed) = routed.borrow_mut().as_mut() {
            routed.push(function.name.clone());
        }
    });
    Ok(())
}

#[cfg(test)]
thread_local! {
    static ROUTED: std::cell::RefCell<Option<Vec<String>>> = const { std::cell::RefCell::new(None) };
}

/// Test-only: runs `lower` and returns the names of the functions this layer lowered,
/// in lowering order.
#[cfg(test)]
pub(in crate::ssa) fn observe_routing<T>(lower: impl FnOnce() -> T) -> (T, Vec<String>) {
    ROUTED.with(|routed| *routed.borrow_mut() = Some(Vec::new()));
    let result = lower();
    let names = ROUTED.with(|routed| routed.borrow_mut().take().unwrap_or_default());
    (result, names)
}

#[cfg(test)]
type OwnerDropWitness = (UnitDropTarget, usize, usize, Option<(usize, usize, usize)>);
#[cfg(test)]
thread_local! {
    static OWNER_DROPS: std::cell::RefCell<Option<Vec<OwnerDropWitness>>> = const { std::cell::RefCell::new(None) };
}

/// Observe only Drop operations already emitted by the production emitter.
#[cfg(test)]
pub(crate) fn observe_owner_drops<T>(lower: impl FnOnce() -> T) -> (T, Vec<OwnerDropWitness>) {
    OWNER_DROPS.with(|events| *events.borrow_mut() = Some(Vec::new()));
    let result = lower();
    let events = OWNER_DROPS.with(|events| events.borrow_mut().take().unwrap_or_default());
    (result, events)
}

#[cfg(test)]
fn record_owner_drop(
    target: UnitDropTarget,
    index: usize,
    slot: usize,
    receipt: Option<(usize, usize, usize)>,
) {
    OWNER_DROPS.with(|events| {
        if let Some(events) = events.borrow_mut().as_mut() {
            events.push((target, index, slot, receipt));
        }
    });
}

#[cfg(test)]
type OwnerCallWitness = (
    lang_frontend::type_checking::UnitExpressionId,
    crate::ssa::model::FunctionId,
    Vec<usize>,
    Option<usize>,
);
#[cfg(test)]
thread_local! {
    static OWNER_CALLS: std::cell::RefCell<Option<Vec<OwnerCallWitness>>> = const { std::cell::RefCell::new(None) };
}
#[cfg(test)]
pub(crate) fn observe_owner_calls<T>(lower: impl FnOnce() -> T) -> (T, Vec<OwnerCallWitness>) {
    OWNER_CALLS.with(|events| *events.borrow_mut() = Some(Vec::new()));
    let result = lower();
    let events = OWNER_CALLS.with(|events| events.borrow_mut().take().unwrap_or_default());
    (result, events)
}
#[cfg(test)]
fn record_owner_call(
    call: lang_frontend::type_checking::UnitExpressionId,
    callee: crate::ssa::model::FunctionId,
    arguments: &[body::Slot],
    receipt: Option<usize>,
) {
    OWNER_CALLS.with(|events| {
        if let Some(events) = events.borrow_mut().as_mut() {
            events.push((
                call,
                callee,
                arguments.iter().map(|s| s.0).collect(),
                receipt,
            ));
        }
    });
}

#[cfg(test)]
type OwnerReturnWitness = (
    lang_frontend::type_checking::UnitExpressionId,
    Option<usize>,
    Option<usize>,
);
#[cfg(test)]
thread_local! {
    static OWNER_RETURNS: std::cell::RefCell<Option<Vec<OwnerReturnWitness>>> = const { std::cell::RefCell::new(None) };
}
#[cfg(test)]
pub(crate) fn observe_owner_returns<T>(lower: impl FnOnce() -> T) -> (T, Vec<OwnerReturnWitness>) {
    OWNER_RETURNS.with(|events| *events.borrow_mut() = Some(Vec::new()));
    let result = lower();
    let events = OWNER_RETURNS.with(|events| events.borrow_mut().take().unwrap_or_default());
    (result, events)
}
#[cfg(test)]
fn record_owner_return(
    expression: lang_frontend::type_checking::UnitExpressionId,
    payload: Option<usize>,
    receipt: Option<usize>,
) {
    OWNER_RETURNS.with(|events| {
        if let Some(events) = events.borrow_mut().as_mut() {
            events.push((expression, payload, receipt));
        }
    });
}

#[cfg(test)]
pub(crate) mod named_receipt {
    pub(crate) fn observe<T>(lower: impl FnOnce() -> T) -> (T, Vec<String>) {
        (lower(), Vec::new())
    }
}
