//! Module validation and shared proofs for function verification.

use super::*;

pub(super) fn verify_module(module: &Module, errors: &mut Vec<VerifyError>) {
    let module_before = errors.len();
    crate::ssa::verify_types::verify_types(module, errors);
    verify_deinits(module, errors);
    let capture_types =
        (errors.len() == module_before).then(|| closure_escape::TypeCaptures::compute(module));

    let trusted_view_fields = closure_content::trusted_view_fields(module);

    for (function_index, function) in module.functions.iter().enumerate() {
        if function.id.module() != module.id || function.id.index() != function_index {
            errors.push(VerifyError {
                kind: VerifyErrorKind::InvalidFunctionId,
                location: VerifyLocation::Function(function.id),
                origin: Some(function.origin.clone()),
            });
            continue;
        }
        let before = errors.len();
        verify_function_structure(module, function, errors);
        if errors.len() == before {
            verify_cfg_types(module, function, errors);
            verify_dominance(function, errors);
            // Ownership content proofs require a validated module type/deinit graph.
            if errors.len() == before
                && let Some(types) = &capture_types
            {
                crate::ssa::verify_ownership::verify_ownership(
                    module,
                    function,
                    types,
                    trusted_view_fields.get(&function.id),
                    errors,
                );
                if errors.len() == before {
                    closure_escape::verify(function, types, errors);
                }
            }
        }
    }
}

fn verify_deinits(module: &Module, errors: &mut Vec<VerifyError>) {
    for (&owner, &function) in &module.deinits {
        let valid = crate::ssa::types::valid_deinit_signature(module, owner, function)
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
