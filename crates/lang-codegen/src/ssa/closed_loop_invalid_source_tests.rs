//! Closed result bindings do not restore a source consumed before a later while.
use super::*;
use lang_frontend::name_resolution::{index_compilation_unit, resolve_compilation_unit_names};
use lang_frontend::ownership_checking::{
    check_compilation_unit_ownership, legacy_ownership_eq, with_loan_observation,
};
use lang_frontend::type_checking::check_compilation_unit_types;

#[test]
fn ordinary_closed_binding_does_not_restore_moved_loop_source() {
    let text = "fun view(source: String): borrow String from source = source\nfun observe(item: String): Unit {}\nfun take(own item: String): Unit {}\nfun run(flag: Boolean): Unit { val source = \"owned\"; { borrow val item = view(source); observe(item) }; take(source); while (flag) { observe(source); break } }";
    let mut sources = SourceMap::new();
    let (source, parsed) = unit_lower_test_support::parsed(&mut sources, "closed-moved.ko", text);
    let (environment, types) = standard_environments();
    for input in [
        SourceUnitInput::new("root", "closed-moved.ko", source, &parsed),
        SourceUnitInput::memory_document(source, &parsed),
    ] {
        let inputs = [input];
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
            .unwrap()
            .validate()
            .unwrap();
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
            .unwrap()
            .validate()
            .unwrap();
        let on = with_loan_observation(true, || {
            check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap()
        });
        let off = with_loan_observation(false, || {
            check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap()
        });
        assert!(legacy_ownership_eq(&on, &off));
        assert!(!on.diagnostics().is_empty());
        // Do not pass invalid ownership to codegen, or assume a diagnostic code.
        assert!(on.validate().is_err());
        assert!(off.validate().is_err());
    }
}
