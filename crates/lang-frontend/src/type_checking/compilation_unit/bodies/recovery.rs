//! 同一 producer 记录的诊断恢复事实；公开执行表不从此区取得执行授权。
use std::collections::{BTreeMap, BTreeSet};

use super::map::UnitMapPutDescriptor;
use super::{CompilationUnitTypeParts, CompilationUnitTypes};
use crate::name_resolution::UnitSymbolId;
use crate::source::Span;
use crate::type_checking::{
    RangeConstructionDescriptor, RangeSizeDescriptor, UnitExpressionId,
    UnitIntegerOperationDescriptor, UnitMapIterationDescriptor, UnitOwnershipPrimitiveDescriptor,
    UnitSequentialIterationDescriptor, UnitStatementId, UnitTypeId, UnitTypeKind,
};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct StorageRecoveryFacts {
    pub(super) rejected_origins: BTreeMap<UnitExpressionId, Span>,
    pub(super) pending_values: BTreeMap<UnitExpressionId, UnitTypeId>,
    // A pending iteration can be traversed for diagnostics without acquiring a provider.
    pub(super) pending_iterations: BTreeSet<UnitStatementId>,
    // Keep the declaration label for a pending field assignment without publishing a projection.
    pub(super) pending_fields: BTreeMap<UnitExpressionId, UnitSymbolId>,
    // Recovery keeps the write role without exporting an executable put.
    pub(super) unpublished_map_writes: BTreeMap<UnitExpressionId, UnitMapPutDescriptor>,
}

impl CompilationUnitTypes {
    pub(crate) fn unpublished_map_write(
        &self,
        target: UnitExpressionId,
    ) -> Option<&UnitMapPutDescriptor> {
        self.storage_recovery.unpublished_map_writes.get(&target)
    }
    pub(crate) fn rejected_storage_origin(&self, expression: UnitExpressionId) -> Option<Span> {
        self.storage_recovery
            .rejected_origins
            .get(&expression)
            .copied()
    }

    pub(crate) fn diagnostic_iteration_source_pending(&self, statement: UnitStatementId) -> bool {
        self.storage_recovery
            .pending_iterations
            .contains(&statement)
    }

    pub(crate) fn is_pending_storage_value(&self, expression: UnitExpressionId) -> bool {
        self.storage_recovery
            .pending_values
            .get(&expression)
            .is_some_and(|&ty| {
                self.expression_type(expression) == Some(ty) && self.types().get(ty).is_some()
            })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct OwnershipRecoveryFacts {
    primitives: Vec<UnitOwnershipPrimitiveDescriptor>,
    integers: Vec<UnitIntegerOperationDescriptor>,
    range_sizes: Vec<RangeSizeDescriptor<UnitExpressionId, UnitTypeId>>,
    range_constructions: Vec<RangeConstructionDescriptor<UnitExpressionId, UnitTypeId>>,
    iterations: Vec<UnitSequentialIterationDescriptor>,
    map_iterations: Vec<UnitMapIterationDescriptor>,
    rejected_iteration_headers: Vec<UnitStatementId>,
}

impl CompilationUnitTypeParts {
    /// 只在最终 trial 回滚与排序完成后移动；不保留另一份可执行表。
    pub(super) fn retain_ownership_recovery(&mut self) {
        self.recovery_ownership = Some(OwnershipRecoveryFacts {
            primitives: std::mem::take(&mut self.ownership_primitives),
            integers: std::mem::take(&mut self.integer_operations),
            range_sizes: std::mem::take(&mut self.range_sizes),
            range_constructions: self
                .calls
                .iter_mut()
                .filter_map(|call| call.range_construction.take())
                .collect(),
            iterations: std::mem::take(&mut self.iterations),
            map_iterations: std::mem::take(&mut self.map_descriptors.iterations),
            rejected_iteration_headers: std::mem::take(&mut self.rejected_iteration_headers),
        });
    }
}

impl CompilationUnitTypes {
    pub(crate) fn diagnostic_iteration_header_rejected(&self, statement: UnitStatementId) -> bool {
        self.recovery_ownership
            .as_ref()
            .is_some_and(|facts| facts.rejected_iteration_headers.contains(&statement))
    }

    pub(crate) fn diagnostic_ownership_primitive(
        &self,
        expression: UnitExpressionId,
    ) -> Option<UnitOwnershipPrimitiveDescriptor> {
        match &self.recovery_ownership {
            Some(facts) => facts
                .primitives
                .iter()
                .copied()
                .find(|p| p.expression() == expression),
            None => self.ownership_primitive(expression),
        }
    }

    pub(crate) fn diagnostic_integer_operation(
        &self,
        expression: UnitExpressionId,
    ) -> Option<UnitIntegerOperationDescriptor> {
        match &self.recovery_ownership {
            Some(facts) => facts
                .integers
                .iter()
                .copied()
                .find(|p| p.expression() == expression),
            None => self.integer_operation(expression),
        }
    }

    pub(crate) fn diagnostic_range_size(
        &self,
        expression: UnitExpressionId,
    ) -> Option<&RangeSizeDescriptor<UnitExpressionId, UnitTypeId>> {
        match &self.recovery_ownership {
            Some(facts) => facts
                .range_sizes
                .iter()
                .find(|p| p.expression() == expression),
            None => self.range_size(expression),
        }
    }

    pub(crate) fn diagnostic_range_construction(
        &self,
        expression: UnitExpressionId,
    ) -> Option<&RangeConstructionDescriptor<UnitExpressionId, UnitTypeId>> {
        match &self.recovery_ownership {
            Some(facts) => facts
                .range_constructions
                .iter()
                .find(|p| p.expression() == expression),
            None => self
                .calls()
                .iter()
                .find(|call| call.expression() == expression)
                .and_then(|call| call.range_construction()),
        }
    }

    pub(crate) fn diagnostic_sequential_iterations(&self) -> &[UnitSequentialIterationDescriptor] {
        match &self.recovery_ownership {
            Some(facts) => &facts.iterations,
            None => self.sequential_iterations(),
        }
    }

    pub(crate) fn diagnostic_sequential_iteration(
        &self,
        statement: UnitStatementId,
    ) -> Option<&UnitSequentialIterationDescriptor> {
        self.diagnostic_sequential_iterations()
            .binary_search_by_key(&statement, |plan| plan.statement())
            .ok()
            .map(|index| &self.diagnostic_sequential_iterations()[index])
    }

    pub(crate) fn diagnostic_map_iterations(&self) -> &[UnitMapIterationDescriptor] {
        match &self.recovery_ownership {
            Some(facts) => &facts.map_iterations,
            None => self.map_iterations(),
        }
    }

    pub(crate) fn diagnostic_map_iteration(
        &self,
        statement: UnitStatementId,
    ) -> Option<&UnitMapIterationDescriptor> {
        self.diagnostic_map_iterations()
            .iter()
            .find(|plan| plan.statement() == statement)
    }

    /// 仅明确的 Error 类型可抑制同根级联；Unknown、Deferred、无类型均不算。
    pub(crate) fn is_diagnostic_error_type(&self, ty: UnitTypeId) -> bool {
        matches!(self.types().get(ty), Some(UnitTypeKind::Error))
    }

    /// Only the existing copyability producer may prove an Error entirely caused
    /// by actual Error nodes. This is not a capability or execution-fact witness.
    pub(crate) fn copyability_error_is_diagnostic(&self, ty: UnitTypeId) -> bool {
        super::checker::copyability::unit_copyability_error_is_diagnostic(self.signatures(), ty)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        lexer::lex,
        name_resolution::{
            SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names,
        },
        parser::parse_file,
        source::SourceMap,
        type_checking::{
            FinalTypeAdmissionError, FinalTypeAdmissionInternalError, check_compilation_unit_types,
            complete_compilation_unit_types, standard_environments,
        },
    };

    #[test]
    fn incomplete_annotation_does_not_mask_missing_initializer_fact() {
        let mut sources = SourceMap::new();
        let source = sources
            .add_source(
                "storage.ko",
                "fun entry(): Int { val value: List<Nothing> = 1; return 0 }",
            )
            .unwrap();
        let file = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
        let inputs = [SourceUnitInput::memory_document(source, &file)];
        let (names, types) = standard_environments();
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &names)
            .unwrap()
            .validate()
            .unwrap();
        let mut typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
        assert_eq!(typed.diagnostics()[0].code().to_string(), "L0125");
        let (initializer, node) = file
            .ast()
            .expressions()
            .iter()
            .find(|(_, node)| sources.slice(node.span()) == Ok("1"))
            .unwrap();
        let unit = index.source_units()[0].id();
        assert!(
            typed
                .expression_types
                .remove(&UnitExpressionId::new(unit, initializer))
                .is_some()
        );

        let result = complete_compilation_unit_types(&sources, &inputs, &names, &types, &typed);
        assert!(
            matches!(
                result,
                Err(FinalTypeAdmissionError::Internal(FinalTypeAdmissionInternalError::MissingRequiredFact {
                    span, kind: "expression type",
                })) if span == node.span()
            ),
            "{result:?}"
        );
    }
}
