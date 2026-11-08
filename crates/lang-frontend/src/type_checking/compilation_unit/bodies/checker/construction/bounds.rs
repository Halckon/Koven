//! Type argument bounds for source nominal and enum construction.

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    diagnostic::codes,
    name_resolution::UnitSymbolId,
    source::Span,
    type_checking::{
        Capability, CompilationUnitTypeError, Copyability, UnitTypeId, UnitTypeParameterBound,
    },
};

use super::{super::copyability::UnitTransferability, BodyChecker};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum BoundCheck {
    Complete,
    Pending,
    Rejected,
}

impl BodyChecker<'_> {
    pub(super) fn check_recovered_operand_bounds(
        &mut self,
        operands: Vec<(UnitTypeId, UnitTypeId, Span)>,
        parameters: &BTreeSet<UnitSymbolId>,
        known: &BTreeMap<UnitSymbolId, UnitTypeId>,
        known_origins: &BTreeMap<UnitSymbolId, Span>,
        fallback: Span,
    ) -> Result<(), CompilationUnitTypeError> {
        let known_parameters = known.keys().copied().collect::<Vec<_>>();
        if self.check_construction_bounds(
            &known_parameters,
            known,
            known_origins,
            fallback,
            true,
        )? == BoundCheck::Rejected
        {
            return Ok(());
        }
        // Partial operands provide diagnostic evidence only. Never use them
        // as the inferred instance or replace a type fixed by another root.
        for (template, actual, span) in operands {
            let mut substitutions = BTreeMap::new();
            let mut origins = BTreeMap::new();
            if self
                .infer_unit_type_arguments(
                    template,
                    actual,
                    parameters,
                    &mut substitutions,
                    &mut origins,
                    span,
                )
                .is_ok()
            {
                let inferred = substitutions
                    .keys()
                    .copied()
                    .filter(|parameter| !known.contains_key(parameter))
                    .collect::<Vec<_>>();
                if self.check_construction_bounds(
                    &inferred,
                    &substitutions,
                    &origins,
                    span,
                    true,
                )? == BoundCheck::Rejected
                {
                    break;
                }
            }
        }
        Ok(())
    }

    pub(super) fn check_construction_bounds(
        &mut self,
        parameters: &[UnitSymbolId],
        substitutions: &BTreeMap<UnitSymbolId, UnitTypeId>,
        origins: &BTreeMap<UnitSymbolId, Span>,
        fallback: Span,
        recovering_type_arguments: bool,
    ) -> Result<BoundCheck, CompilationUnitTypeError> {
        let mut result = BoundCheck::Complete;
        for &parameter in parameters {
            let actual = substitutions[&parameter];
            let bound = self
                .signatures
                .type_parameter(parameter)
                .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?
                .bound();
            // Explicit poisoned types still permit structural capability proofs.
            // Other bounds retain the original type-argument recovery path.
            if recovering_type_arguments
                && !matches!(
                    bound,
                    UnitTypeParameterBound::Capability(
                        Capability::Copyable | Capability::Transferable
                    )
                )
            {
                continue;
            }
            let failure = match bound {
                UnitTypeParameterBound::Any | UnitTypeParameterBound::Error => None,
                UnitTypeParameterBound::Interface(interface) => {
                    let expected = self.substitute_type(interface, substitutions)?;
                    (!self.satisfies_interface(actual, expected)?).then_some((
                        codes::TYPE_ARGUMENT_BOUND,
                        "constructor type argument does not satisfy its interface bound",
                    ))
                }
                UnitTypeParameterBound::Capability(Capability::Copyable) => {
                    match self.copyability_of(actual) {
                        Copyability::Copyable => None,
                        Copyability::Unknown => {
                            result = BoundCheck::Pending;
                            None
                        }
                        Copyability::Error if recovering_type_arguments => None,
                        Copyability::Error | Copyability::MoveOnly => Some((
                            codes::COPYABLE_TYPE_ARGUMENT_BOUND,
                            "constructor type argument does not satisfy its Copyable bound",
                        )),
                    }
                }
                UnitTypeParameterBound::Capability(Capability::Transferable) => {
                    match self.transferability_of(actual) {
                        UnitTransferability::Transferable => None,
                        UnitTransferability::Unknown => {
                            result = BoundCheck::Pending;
                            None
                        }
                        UnitTransferability::Error if recovering_type_arguments => None,
                        UnitTransferability::Error | UnitTransferability::NotTransferable => {
                            Some((
                                codes::TRANSFERABLE_TYPE_ARGUMENT_BOUND,
                                "constructor type argument does not satisfy its Transferable bound",
                            ))
                        }
                    }
                }
                UnitTypeParameterBound::Capability(Capability::Hashable) => {
                    (!self.is_hashable_type(actual)).then_some((
                        codes::HASHABLE_TYPE_ARGUMENT_BOUND,
                        "constructor type argument does not satisfy its Hashable bound",
                    ))
                }
            };
            if let Some((code, message)) = failure {
                self.emit_maybe_label(
                    code,
                    message,
                    origins.get(&parameter).copied().unwrap_or(fallback),
                    Some(self.unit_symbol_span(parameter)?),
                    "type parameter bound declared here",
                )?;
                return Ok(BoundCheck::Rejected);
            }
        }
        Ok(result)
    }
}
