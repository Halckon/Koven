//! Research only: a lexical cleanup boundary is not an iteration-local End proof.
#![cfg(feature = "loan-observation-test-support")]

use lang_frontend::{
    lexer::lex,
    name_resolution::{
        SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names, resolve_names,
    },
    ownership_checking::{
        DropPoint, LoanEndPoint, UnitDropPoint, check_compilation_unit_ownership, check_ownership,
        legacy_ownership_eq, with_loan_observation,
    },
    parser::{Statement, parse_file},
    source::SourceMap,
    type_checking::{check_compilation_unit_types, check_types, standard_environments},
};

#[test]
fn ordinary_no_carry_scope_needs_real_end_in_both_checkers() {
    for count in [1, 3] {
        let consumers = vec!["observe(item)"; count].join("; ");
        let body = format!("borrow val item = view(source); {consumers}");
        for (name, statement, rejected) in [
            ("straight-scope", format!("{{ {body} }}"), false),
            ("iteration-body", format!("while (flag) {{ {body} }}"), true),
            (
                "iteration-inner-scope",
                format!("while (flag) {{ {{ {body} }} }}"),
                true,
            ),
        ] {
            let text = format!(
                "fun view(source: String): borrow String from source = source\nfun observe(item: String): Unit {{}}\nfun run(source: String, flag: Boolean): Unit {{ {statement} }}"
            );
            let mut sources = SourceMap::new();
            let source = sources.add_source("no-carry-evidence.ko", &text).unwrap();
            let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
            assert!(parsed.diagnostics().is_empty());
            let (environment, types) = standard_environments();
            let names = resolve_names(&sources, &parsed, &environment).unwrap();
            assert!(names.diagnostics().is_empty());
            let typed = check_types(&sources, &parsed, &names, &types).unwrap();
            assert!(
                typed.diagnostics().is_empty(),
                "{name}: {:?}",
                typed.diagnostics()
            );
            let single = check_ownership(&sources, &parsed, &names, &typed).unwrap();
            let marker = text.find("borrow val").unwrap();
            let last_call = text.rfind("observe(item)").unwrap();
            let single_unsupported = single
                .diagnostics()
                .iter()
                .filter(|diagnostic| diagnostic.code().to_string() == "L0164")
                .map(|diagnostic| diagnostic.primary_span())
                .collect::<Vec<_>>();
            if rejected {
                assert_eq!(
                    single_unsupported.len(),
                    1,
                    "{name}: {:?}",
                    single.diagnostics()
                );
                assert_eq!(single_unsupported[0].source_id(), source);
                assert_eq!(single_unsupported[0].start(), marker);
                assert_eq!(&text[marker..single_unsupported[0].end()], "borrow");
                assert!(single.borrow_results().bindings().is_empty());
                assert!(single.borrow_results().ends().is_empty());
            } else {
                assert!(
                    single.diagnostics().is_empty(),
                    "{:?}",
                    single.diagnostics()
                );
                let [binding] = single.borrow_results().bindings() else {
                    panic!("one real binding")
                };
                let [end] = single.borrow_results().ends() else {
                    panic!("one actual End, not scope restoration")
                };
                assert_eq!(end.binding(), binding.binding());
                let DropPoint::AfterStatement(id) = end.point() else {
                    panic!("normal last-use End")
                };
                let node = parsed.ast().statements().get(id).unwrap();
                assert!(matches!(node.payload(), Statement::Expression { .. }));
                assert_eq!(node.span().start(), last_call);
                let source_loan = binding.source_loan().unwrap();
                assert!(
                    single
                        .loans()
                        .iter()
                        .any(|loan| loan.call() == source_loan.call()
                            && loan.argument() == source_loan.argument()
                            && loan.target() == binding.origin())
                );
                assert!(
                    !single
                        .loan_ends()
                        .iter()
                        .any(|end| end.call() == source_loan.call()
                            && end.argument() == source_loan.argument()
                            && end.point() == LoanEndPoint::CallReturn(source_loan.call()))
                );
            }
            for rooted in [false, true] {
                let inputs = [if rooted {
                    SourceUnitInput::new("root", "no-carry-evidence.ko", source, &parsed)
                } else {
                    SourceUnitInput::memory_document(source, &parsed)
                }];
                let index = index_compilation_unit(&sources, &inputs).unwrap();
                let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
                    .unwrap()
                    .validate()
                    .unwrap();
                let typed =
                    check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
                assert!(typed.diagnostics().is_empty());
                let typed = typed.validate().unwrap();
                let on = with_loan_observation(true, || {
                    check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
                        .unwrap()
                });
                let off = with_loan_observation(false, || {
                    check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
                        .unwrap()
                });
                assert!(legacy_ownership_eq(&on, &off));
                assert_eq!(on.diagnostics(), off.diagnostics());
                assert!(off.loan_action_observations().is_empty());
                let unsupported = on
                    .diagnostics()
                    .iter()
                    .filter(|diagnostic| diagnostic.code().to_string() == "L0164")
                    .map(|diagnostic| diagnostic.primary_span())
                    .collect::<Vec<_>>();
                assert_eq!(
                    unsupported, single_unsupported,
                    "{name}/{count}/{rooted}: checker marker parity"
                );
                if rejected {
                    assert!(on.clone().validate().is_err());
                    assert!(on.borrow_results().bindings().is_empty());
                    assert!(on.borrow_results().ends().is_empty());
                    assert!(on.borrow_binding_transports().is_empty());
                } else {
                    assert!(on.diagnostics().is_empty(), "{:?}", on.diagnostics());
                    on.clone().validate().unwrap();
                    let [binding] = on.borrow_results().bindings() else {
                        panic!("one unit binding")
                    };
                    let [end] = on.borrow_results().ends() else {
                        panic!("one real unit End")
                    };
                    assert_eq!(end.binding(), binding.binding());
                    let UnitDropPoint::AfterStatement(id) = end.point() else {
                        panic!("unit last-use End")
                    };
                    assert_eq!(id.source_unit(), binding.binding().source_unit());
                    let node = parsed.ast().statements().get(id.statement()).unwrap();
                    assert!(matches!(node.payload(), Statement::Expression { .. }));
                    assert_eq!(node.span().start(), last_call);
                    let source_loan = binding.source_loan().unwrap();
                    assert!(
                        on.loans()
                            .iter()
                            .any(|loan| loan.call() == source_loan.call()
                                && loan.argument() == source_loan.argument()
                                && loan.target() == binding.origin())
                    );
                }
            }
        }
    }
}
