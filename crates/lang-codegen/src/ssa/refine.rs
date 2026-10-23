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
    /// Value-mode parameters in declaration order.
    pub(in crate::ssa) parameters: Vec<UnitSymbolId>,
    pub(in crate::ssa) span: Span,
}

/// Deterministic subset precheck; reads typed facts and AST construct kinds only.
pub(in crate::ssa) fn accepts(inputs: &RefineInputs<'_>) -> bool {
    subset::accepts(inputs)
}

/// Frontend types an accepted body needs in the SSA type table, in first-use order.
pub(in crate::ssa) fn type_demands(inputs: &RefineInputs<'_>) -> Vec<UnitTypeId> {
    subset::type_demands(inputs)
}

/// Lowers an accepted function body into `function`, whose `entry` block holds
/// `parameters` (one entity per `inputs.parameters` symbol).
pub(in crate::ssa) fn lower(
    inputs: &RefineInputs<'_>,
    function: &mut Function,
    entry: BlockId,
    parameters: &[EntityId],
    type_ids: &BTreeMap<UnitTypeId, SsaTypeId>,
) -> Result<(), LoweringError> {
    let body = build::build(inputs, parameters, type_ids)?;
    emit::emit(&body, function, entry, parameters, inputs.span)
}
