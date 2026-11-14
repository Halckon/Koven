//! Outer bindings retain one lexical lease across loop uses; return adds its own exit end.
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
fn ordinary_outer_loop_binding_keeps_lexical_lease() {
    for (name, body) in [
        ("normal", "observe(item)"),
        ("continue", "observe(item); continue"),
        ("break", "observe(item); break"),
        ("return", "observe(item); return"),
        ("zero", "observe(item)"),
    ] {
        let condition = if name == "zero" { "false" } else { "flag" };
        let text = format!(
            "fun view(source: String): borrow String from source = source\nfun observe(item: String): Unit {{}}\nfun run(source: String, flag: Boolean): Unit {{ borrow val item = view(source); while ({condition}) {{ {body} }} }}"
        );
        let mut sources = SourceMap::new();
        let source = sources.add_source("outer-loop.ko", &text).unwrap();
        let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
        assert!(parsed.diagnostics().is_empty());
        let (environment, types) = standard_environments();
        for input in [
            SourceUnitInput::new("root", "outer-loop.ko", source, &parsed),
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
            off.validate().unwrap();
            assert_eq!(on.borrow_results().bindings().len(), 1);

            let binding = on.borrow_results().bindings()[0].binding();
            let ends = on.borrow_results().ends();
            assert_eq!(ends.len(), if name == "return" { 2 } else { 1 });
            let mut normal = 0;
            let mut returned = 0;
            for end in ends {
                assert_eq!(end.binding(), binding);
                match end.point() {
                    lang_frontend::ownership_checking::UnitDropPoint::AfterStatement(id) => {
                        assert_eq!(id.source_unit(), binding.source_unit());
                        let lang_frontend::parser::Statement::Block { elements } = parsed
                            .ast()
                            .statements()
                            .get(id.statement())
                            .unwrap()
                            .payload()
                        else {
                            panic!("end must belong to outer lexical block")
                        };
                        assert_eq!(elements.len(), 2);
                        assert!(matches!(
                            parsed
                                .ast()
                                .statements()
                                .get(elements[0])
                                .unwrap()
                                .payload(),
                            lang_frontend::parser::Statement::LocalVariable { .. }
                        ));
                        assert!(matches!(
                            parsed
                                .ast()
                                .statements()
                                .get(elements[1])
                                .unwrap()
                                .payload(),
                            lang_frontend::parser::Statement::While { .. }
                        ));
                        normal += 1;
                    }
                    lang_frontend::ownership_checking::UnitDropPoint::ControlTransfer(id) => {
                        assert_eq!(id.source_unit(), binding.source_unit());
                        assert!(matches!(
                            parsed
                                .ast()
                                .expressions()
                                .get(id.expression())
                                .unwrap()
                                .payload(),
                            lang_frontend::parser::Expression::Return { value: None, .. }
                        ));
                        returned += 1;
                    }
                    _ => panic!("no per-iteration or jump end is justified"),
                }
            }
            assert_eq!(normal, 1);
            assert_eq!(returned, usize::from(name == "return"));
            on.validate().unwrap();
        }
    }
}
