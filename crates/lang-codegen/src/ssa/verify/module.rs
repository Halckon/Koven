//! Module validation and shared proofs for function verification.

use super::*;

pub(super) fn verify_module(module: &Module, errors: &mut Vec<VerifyError>) {
    let module_before = errors.len();
    crate::ssa::verify_types::verify_types(module, errors);
    crate::ssa::deinit::verify_deinits(module, errors);
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
