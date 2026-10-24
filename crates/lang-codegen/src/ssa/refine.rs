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
    name_resolution::{SourceUnitId, UnitSymbolId, ValidatedCompilationUnitNames},
    ownership_checking::{CompilationUnitOwnership, ConstEnabledOwnedUnit},
    parser::ParsedFile,
    source::{SourceMap, Span},
    type_checking::{CompilationUnitTypes, UnitTypeId},
};

use super::{
    lowering_support::{LoweringError, callable_instances::SourceToken},
    model::{BlockId, EntityId, Function, FunctionId, SsaTypeId},
    unit_plan::{UnitCallablePlan, UnitFunctionInstanceKey},
};

#[derive(Clone, Copy, Debug)]
pub(in crate::ssa) enum RefineBody {
    Expression(ExpressionId),
    Block(StatementId),
}

/// Everything one function instance needs; all facts come from the same validated unit.
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
    pub(in crate::ssa) function_item: ItemId,
    pub(in crate::ssa) body: RefineBody,
    pub(in crate::ssa) return_type: UnitTypeId,
    /// Parameters in declaration order.
    pub(in crate::ssa) parameters: Vec<UnitSymbolId>,
    /// Per parameter: whether the caller passes a shared loan (Borrow mode).
    pub(in crate::ssa) borrowed: Vec<bool>,
    pub(in crate::ssa) span: Span,
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
pub(in crate::ssa) fn with_per_family_lowering<T>(lower: impl FnOnce() -> T) -> T {
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
    /// (owner type, field symbol) to payload field index.
    pub(in crate::ssa) field_indices: &'t BTreeMap<(UnitTypeId, UnitSymbolId), usize>,
}

/// Lowers an accepted function body into `function`, whose `entry` block holds
/// `parameters` (one entity per `inputs.parameters` symbol).
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
