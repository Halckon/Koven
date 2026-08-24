//! Closure-specific linear ownership effects.

use std::collections::BTreeSet;

use super::{
    AliasRoots, BlockState, consume_value, error, has_exclusive_value_loan, require_value,
};
use crate::ssa::{
    model::{ClosureCaptureOperand, Function, Module, Origin, ValueId},
    verify::{VerifyError, VerifyErrorKind, VerifyLocation},
};

#[allow(clippy::too_many_arguments)]
pub(super) fn apply_construct(
    module: &Module,
    function: &Function,
    captures: &[ClosureCaptureOperand],
    aliases: &AliasRoots,
    state: &mut BlockState,
    location: VerifyLocation,
    origin: &Origin,
    errors: &mut Vec<VerifyError>,
) {
    for capture in captures {
        match capture {
            ClosureCaptureOperand::Owned(value) => consume_value(
                module,
                function,
                *value,
                aliases,
                state,
                &BTreeSet::new(),
                &BTreeSet::new(),
                location.clone(),
                origin,
                errors,
            ),
            ClosureCaptureOperand::Shared(_) => {
                unreachable!("shared closure formation is rejected by the operation verifier")
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn apply_invoke(
    module: &Module,
    function: &Function,
    callable: ValueId,
    arguments: &[ValueId],
    aliases: &AliasRoots,
    state: &mut BlockState,
    location: VerifyLocation,
    origin: &Origin,
    errors: &mut Vec<VerifyError>,
) {
    if require_value(
        module,
        function,
        callable,
        state,
        location.clone(),
        origin,
        errors,
    ) && has_exclusive_value_loan(function, callable, aliases, state)
    {
        errors.push(error(
            VerifyErrorKind::OwnerLoanConflict { value: callable },
            location.clone(),
            origin,
        ));
    }
    for argument in arguments {
        consume_value(
            module,
            function,
            *argument,
            aliases,
            state,
            &BTreeSet::new(),
            &BTreeSet::new(),
            location.clone(),
            origin,
            errors,
        );
    }
}
