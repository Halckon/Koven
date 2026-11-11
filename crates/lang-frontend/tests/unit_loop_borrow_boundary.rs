//! Research boundary: iteration-local borrow bindings are typed, but not yet ownership-proven.
#![cfg(feature = "loan-observation-test-support")]
use lang_frontend::{
    lexer::lex,
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::{
        check_compilation_unit_ownership, legacy_ownership_eq, with_loan_observation,
    },
    parser::parse_file,
    source::SourceMap,
    type_checking::{check_compilation_unit_types, standard_environments},
};

#[test]
fn ordinary_loop_local_borrow_requires_iteration_instance_proof() {
    for (name, body) in [
        (
            "normal-backedge",
            "while (flag) { borrow val item = view(source); observe(item) }",
        ),
        (
            "continue",
            "while (flag) { borrow val item = view(source); observe(item); continue }",
        ),
        (
            "break",
            "while (flag) { borrow val item = view(source); observe(item); break }",
        ),
        (
            "body-owner",
            "while (flag) { val local = \"local\"; borrow val item = view(local); observe(item) }",
        ),
    ] {
        let text = format!(
            "fun view(source: String): borrow String from source = source\nfun observe(item: String): Unit {{}}\nfun run(source: String, flag: Boolean): Unit {{ {body} }}"
        );
        let mut sources = SourceMap::new();
        let source = sources.add_source("loop-boundary.ko", &text).unwrap();
        let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
        assert!(
            parsed.diagnostics().is_empty(),
            "{name}: {:?}",
            parsed.diagnostics()
        );
        let (environment, types) = standard_environments();
        for (layout, input) in [
            (
                "rooted",
                SourceUnitInput::new("root", "loop-boundary.ko", source, &parsed),
            ),
            ("memory", SourceUnitInput::memory_document(source, &parsed)),
        ] {
            let inputs = [input];
            let index = index_compilation_unit(&sources, &inputs).unwrap();
            let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
                .unwrap()
                .validate()
                .unwrap();
            let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
            assert!(
                typed.diagnostics().is_empty(),
                "{name}/{layout}: {:?}",
                typed.diagnostics()
            );
            let typed = typed.validate().unwrap();
            let on = with_loan_observation(true, || {
                check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap()
            });
            let off = with_loan_observation(false, || {
                check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap()
            });
            assert!(
                legacy_ownership_eq(&on, &off),
                "{name}/{layout}: legacy facts and diagnostics"
            );
            assert_eq!(on.diagnostics(), off.diagnostics());
            assert_eq!(on.owner_observations(), off.owner_observations());
            assert!(off.loan_action_observations().is_empty());
            let diagnostics = on.diagnostics();
            let rejected = diagnostics
                .iter()
                .filter(|d| d.code().to_string() == "L0164")
                .collect::<Vec<_>>();
            let [diagnostic] = rejected.as_slice() else {
                panic!("{name}/{layout}: expected one iteration-proof diagnostic: {diagnostics:?}");
            };
            let marker = diagnostic.primary_span();
            assert_eq!(marker.source_id(), source);
            assert_eq!(marker.start(), text.find("borrow val").unwrap());
            assert_eq!(&text[marker.start()..marker.end()], "borrow");
            assert!(
                on.clone().validate().is_err(),
                "not a validated backend input"
            );
            // A declaration-only error must not manufacture a live result transaction.
            assert!(on.borrow_results().bindings().is_empty());
            assert!(on.borrow_binding_transports().is_empty());
        }
    }
}
