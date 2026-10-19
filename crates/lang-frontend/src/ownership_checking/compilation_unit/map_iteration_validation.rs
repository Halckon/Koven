//! 真实 acquisition 模板与独立 Map 双分量 lineage 的封闭检查。
use super::{
    CompilationUnitOwnership, OwnershipBindingKind, UnitDropTarget, UnitIterationSourceAccess,
    UnitLoanTarget,
};
use crate::ownership_checking::{IterationSource, MapIterationComponent};

impl CompilationUnitOwnership {
    pub(super) fn map_iteration_headers_are_valid(&self) -> bool {
        if self.drops.iter().any(|drop| {
            matches!(drop.target(),
            UnitDropTarget::Named(symbol) if self.map_iteration_inputs.iter()
                .any(|descriptor| descriptor.bindings().contains(&symbol)))
        }) {
            return false;
        }
        if self.map_iterations.len() != self.map_iteration_templates.len()
            || !self
                .map_iteration_inputs
                .windows(2)
                .all(|pair| pair[0].statement() < pair[1].statement())
            || !self
                .map_iterations
                .windows(2)
                .all(|pair| pair[0].descriptor().statement() < pair[1].descriptor().statement())
            || !self
                .map_iteration_templates
                .windows(2)
                .all(|pair| pair[0].descriptor().statement() < pair[1].descriptor().statement())
        {
            return false;
        }
        for (index, plan) in self.map_iterations.iter().enumerate() {
            let descriptor = plan.descriptor();
            let statement = descriptor.statement();
            let Some(template) = self
                .map_iteration_templates
                .iter()
                .find(|template| template.descriptor().statement() == statement)
            else {
                return false;
            };
            if self.map_iteration_templates[..index]
                .iter()
                .any(|template| template.descriptor().statement() == statement)
                || !self.map_iteration_inputs.contains(descriptor)
                || self
                    .iterations
                    .iter()
                    .any(|other| other.descriptor().statement() == statement)
                || plan.descriptor() != template.descriptor()
                || plan.source_kind() != template.source_kind()
                || plan.source_access() != template.source_access()
                || plan.bindings() != template.bindings()
                || plan.loans() != template.loans()
            {
                return false;
            }
            let IterationSource::BorrowedSource(origin) = plan.source_kind() else {
                return false;
            };
            if matches!(origin, UnitLoanTarget::Temporary(_))
                != (plan.source_access() == UnitIterationSourceAccess::Temporary)
            {
                return false;
            }
            for ((binding, loan), (symbol, component)) in
                plan.bindings().iter().zip(plan.loans()).zip(
                    descriptor
                        .bindings()
                        .into_iter()
                        .zip([MapIterationComponent::Key, MapIterationComponent::Value]),
                )
            {
                if binding.symbol() != symbol
                    || binding.kind() != OwnershipBindingKind::Shared
                    || !self.bindings.contains(binding)
                    || loan.statement() != statement
                    || loan.source() != descriptor.source()
                    || loan.binding() != symbol
                    || loan.component() != component
                    || loan.origin() != origin
                {
                    return false;
                }
            }
        }
        true
    }
}
