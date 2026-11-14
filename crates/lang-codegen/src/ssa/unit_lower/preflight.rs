//! One whole-unit preflight implementation; input queries do not grant lowering authority.
use super::*;
use lang_frontend::ownership_checking::UnitPreflightFacts;

pub(in crate::ssa) fn validate(
    parsed_by_source: &[&ParsedFile],
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &dyn UnitPreflightFacts,
) -> Result<(), LoweringError> {
    // Callers must have checked the complete frontend identity chain before this query-only gate.
    // Declarations are validated unconditionally for every unit.
    super::borrow_result::validate_declarations(parsed_by_source, names, typed, owned)?;
    // Map providers are validated at their reached lowering route. Keep the older
    // borrow-binding CFG frontier for bindings without dedicated lowering checks.
    let unit_has_refine_constructs =
        !owned.field_replacements().is_empty() || !typed.ownership_primitives().is_empty();
    super::borrow_result::validate_bindings(
        parsed_by_source,
        names,
        typed,
        owned,
        !unit_has_refine_constructs,
    )?;
    // Runtime global initialization is not lowered yet. Check every source root,
    // including unused declarations, before reachability can discard its initializer.
    for parsed in parsed_by_source {
        for root in parsed.roots() {
            let node = parsed.ast().items().get(*root).map_err(|_| LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?;
            let (_, item, _) = unwrap_modified(parsed, *root)?;
            if matches!(item, Item::Variable { .. }) {
                return Err(lowering_error(
                    LoweringErrorKind::UnsupportedNode,
                    node.span(),
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lang_frontend::{
        lexer::lex,
        name_resolution::{index_compilation_unit, resolve_compilation_unit_names},
        ownership_checking::{check_compilation_unit_ownership, owned_compilation_unit_view},
        parser::parse_file,
        type_checking::{check_compilation_unit_types, standard_environments},
    };

    #[test]
    fn ordinary_preflight_preserves_frontend_global_rejection() {
        for global in [false, true] {
            let text = if global {
                "val unused=1\nfun main():Unit {}"
            } else {
                "fun unused():Int=1\nfun main():Unit {}"
            };
            let mut sources = SourceMap::new();
            let id = sources.add_source("preflight.ko", text).unwrap();
            let parsed = parse_file(&sources, &lex(&sources, id).unwrap()).unwrap();
            let inputs = [SourceUnitInput::memory_document(id, &parsed)];
            let (env, types) = standard_environments();
            let index = index_compilation_unit(&sources, &inputs).unwrap();
            let names = resolve_compilation_unit_names(&sources, &inputs, &index, &env)
                .unwrap()
                .validate()
                .unwrap();
            let checked_types =
                check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
            if global {
                assert!(
                    checked_types
                        .diagnostics()
                        .iter()
                        .any(|d| d.code().to_string() == "L0167")
                );
                assert!(checked_types.validate().is_err());
                continue; // Never manufacture a capability to bypass the earlier frontend gate.
            }
            let typed = checked_types.validate().unwrap();
            let owned = check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
                .unwrap()
                .validate()
                .unwrap();
            let view =
                owned_compilation_unit_view(&sources, &inputs, &names, &types, &typed, &owned)
                    .unwrap();
            let files =
                crate::ssa::lowering_support::parsed_by_source_unit(view.inputs(), view.names())
                    .unwrap();
            let checked = validate(&files, view.names(), view.types(), view.ownership());
            let main = names
                .names()
                .index()
                .declarations()
                .iter()
                .find(|d| d.name() == "main")
                .unwrap();
            let lowered = super::super::lower_owned_unit_with_entry(&view, main.id());
            checked.unwrap();
            lowered.unwrap();
        }
    }
}
