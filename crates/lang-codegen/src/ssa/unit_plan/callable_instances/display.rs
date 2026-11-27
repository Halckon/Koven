//! Static dispatch for the implicit calls in interpolation, keyed separately from source calls.
use super::{super::interpolation::DisplayRead, *};

impl UnitCallablePlan {
    pub(in crate::ssa) fn display_site(
        &self,
        caller: SourceToken,
        operand: UnitExpressionId,
    ) -> Option<&PlannedUnitCallSite> {
        self.display_sites.get(&(caller, operand))
    }
}

impl CallablePlanner<'_> {
    pub(in crate::ssa::unit_plan) fn freeze_display_calls(
        &mut self,
        caller: SourceToken,
        reads: &[DisplayRead],
        substitutions: &BTreeMap<UnitSymbolId, UnitTypeId>,
        static_self: Option<UnitTypeId>,
        pending: &mut BTreeSet<UnitFunctionInstanceKey>,
        runtime_demands: &mut BTreeMap<UnitTypeId, UnitRuntimeTypeDemand>,
    ) -> Result<(), LoweringError> {
        for &DisplayRead {
            operand,
            requirement,
            span,
        } in reads
        {
            let ty = self
                .typed
                .expression_type(operand)
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            // A case refinement denotes the same runtime receiver and methods as the enum.
            let layout = match self.typed.types().get(ty) {
                Some(UnitTypeKind::EnumCase { root, .. }) => *root,
                _ => ty,
            };
            let receiver =
                resolve_concrete_type(self.typed, layout, substitutions, static_self, span)?;
            if matches!(
                self.typed.types().get(receiver),
                Some(UnitTypeKind::Builtin(_))
            ) {
                // A generic source receiver may specialize to an intrinsic conversion.
                continue;
            }
            let signature = unit_callable_signature(self.typed, requirement)
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            if signature.receiver().map(|receiver| receiver.mode()) != Some(ParameterMode::Borrow)
                || !signature.parameters().is_empty()
                || signature.result_source()
                    != lang_frontend::type_checking::CallableResultSource::Owned
                || !matches!(
                    self.typed.types().get(signature.return_type()),
                    Some(UnitTypeKind::Builtin(
                        lang_frontend::type_checking::BuiltinType::String
                    ))
                )
            {
                return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
            }
            let route = resolve_unit_call_instance(
                self.typed,
                self.owned,
                requirement,
                Vec::new(),
                Some(receiver),
                span,
            )?;
            for dependent in route.dependent_owner_types {
                runtime_demands
                    .entry(dependent)
                    .or_insert(UnitRuntimeTypeDemand::InstanceKeyOnly);
            }
            pending.insert(route.key.clone());
            self.plan.display_sites.insert(
                (caller, operand),
                PlannedUnitCallSite {
                    key: route.key,
                    delegation: route.delegation,
                    dispatches: true,
                },
            );
        }
        Ok(())
    }
}
