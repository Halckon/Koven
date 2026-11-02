//! ADR-0032 ownership refinement layer: typed and ownership facts become a slot-based
//! CFG with explicit moves, drops and loans; SSA construction is generic.
//!
//! During migration `lower_owned_unit_with_entry` routes a function instance here only
//! when [`accepts`] proves its whole body lies inside the migrated subset. Once routed,
//! a failure is an error; there is no fallback to the per-family lowering.
mod body;
mod build;
mod emit;
mod subset;

use std::collections::BTreeMap;

use lang_frontend::{
    ast::{ExpressionId, ItemId, StatementId},
    name_resolution::{DeclarationId, SourceUnitId, UnitSymbolId, ValidatedCompilationUnitNames},
    ownership_checking::{CompilationUnitOwnership, ConstEnabledOwnedUnit},
    parser::ParsedFile,
    source::{SourceMap, Span},
    type_checking::{CompilationUnitTypes, ParameterMode, UnitTypeId},
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

/// Everything one function or lambda thunk instance needs; all facts come from the same unit.
pub(in crate::ssa) struct RefineInputs<'a> {
    pub(in crate::ssa) sources: &'a SourceMap,
    pub(in crate::ssa) parsed: &'a ParsedFile,
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
}

/// Deterministic subset precheck; reads typed facts and AST construct kinds only.
pub(in crate::ssa) fn accepts(inputs: &RefineInputs<'_>) -> bool {
    subset::accepts(inputs)
}

/// Whether the function instance is lowered here; [`accepts`] unless a test pinned the
/// per-family path.
pub(in crate::ssa) fn routes(inputs: &RefineInputs<'_>) -> bool {
    #[cfg(test)]
    if PER_FAMILY_ONLY.with(std::cell::Cell::get) {
        return false;
    }
    accepts(inputs)
}

#[cfg(test)]
thread_local! {
    static PER_FAMILY_ONLY: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Test-only: runs `lower` with every function on the per-family path, for tests that
/// inspect or fault-inject that path's private guards.
#[cfg(test)]
pub(crate) fn with_per_family_lowering<T>(lower: impl FnOnce() -> T) -> T {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            PER_FAMILY_ONLY.with(|flag| flag.set(false));
        }
    }
    PER_FAMILY_ONLY.with(|flag| flag.set(true));
    let _reset = Reset;
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
