//! Validated association between an owning class layout and its readonly user destructor.
use super::{
    model::{EntityType, FunctionId, LoanKind, ModelError, Module, SsaTypeId, SsaTypeKind},
    verify::{VerifyError, VerifyErrorKind, VerifyLocation},
};

impl Module {
    /// Associate a compiler-only body; callers cannot turn this into an ordinary owned call.
    pub(crate) fn set_deinit(
        &mut self,
        owner: SsaTypeId,
        function: FunctionId,
    ) -> Result<(), ModelError> {
        if !valid_signature(self, owner, function) {
            return Err(ModelError::InvalidDeinit { owner, function });
        }
        if self.deinits.contains_key(&owner) {
            return Err(ModelError::DeinitAlreadyDefined { owner });
        }
        self.deinits.insert(owner, function);
        Ok(())
    }

    pub(crate) fn deinit(&self, owner: SsaTypeId) -> Option<FunctionId> {
        self.deinits.get(&owner).copied()
    }
}

fn valid_signature(module: &Module, owner: SsaTypeId, function: FunctionId) -> bool {
    matches!(module.type_kind(owner), Some(SsaTypeKind::HeapOwner { .. }))
        && module.function(function).is_some_and(|function| {
            function.receiver
                == Some(EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: owner,
                })
                && function.return_types.is_empty()
        })
}

pub(super) fn verify_deinits(module: &Module, errors: &mut Vec<VerifyError>) {
    for (&owner, &function) in &module.deinits {
        let valid = valid_signature(module, owner, function)
            && module.function(function).is_some_and(|function| {
                function.blocks.first().is_some_and(|entry| {
                    entry.parameters.len() == 1
                        && function
                            .entity(entry.parameters[0])
                            .is_some_and(|parameter| Some(parameter.ty) == function.receiver)
                })
            });
        if !valid {
            errors.push(VerifyError {
                kind: VerifyErrorKind::InvalidTypeDefinition {
                    reason: "deinit requires one exact shared heap-owner receiver and a Unit result",
                },
                location: VerifyLocation::Type(owner),
                origin: module.function(function).map(|function| function.origin.clone()),
            });
        }
    }
}
