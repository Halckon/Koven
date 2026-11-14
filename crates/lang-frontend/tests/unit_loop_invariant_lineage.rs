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
fn ordinary_loop_invariant_lineage_uses_actual_boundaries() {
    for root in ["local", "borrow", "own"] {
        for count in [1, 2, 8] {
            for name in ["normal", "continue", "break", "return", "zero"] {
                let consumers = vec!["observe(item)"; count].join("; ");
                let body = match name {
                    "continue" | "break" | "return" => format!("{consumers}; {name}"),
                    _ => consumers,
                };
                let condition = if name == "zero" { "false" } else { "flag" };
                let (parameters, prefix) = match root {
                    "local" => ("flag: Boolean", "val source = \"source\"; "),
                    "borrow" => ("source: String, flag: Boolean", ""),
                    "own" => ("own source: String, flag: Boolean", ""),
                    _ => unreachable!(),
                };
                let text = format!(
                    "fun view(source: String): borrow String from source = source\nfun observe(item: String): Unit {{}}\nfun run({parameters}): Unit {{ {prefix}borrow val item = view(source); while ({condition}) {{ {body} }} }}"
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
                    let names =
                        resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
                            .unwrap()
                            .validate()
                            .unwrap();
                    let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
                        .unwrap()
                        .validate()
                        .unwrap();
                    let on = with_loan_observation(true, || {
                        check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
                            .unwrap()
                    });
                    let off = with_loan_observation(false, || {
                        check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
                            .unwrap()
                    });
                    assert!(legacy_ownership_eq(&on, &off));

                    assert_eq!(on.borrow_results().bindings().len(), 1);

                    let binding = on.borrow_results().bindings()[0].binding();
                    let ends = on.borrow_results().ends();
                    assert_eq!(ends.len(), if name == "return" { 2 } else { 1 });
                    let mut normal = 0;
                    let mut returned = 0;
                    for end in ends {
                        assert_eq!(end.binding(), binding);
                        match end.point() {
                            lang_frontend::ownership_checking::UnitDropPoint::AfterStatement(
                                id,
                            ) => {
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
                                assert_eq!(elements.len(), if root == "local" { 3 } else { 2 });
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
                                        .get(*elements.last().unwrap())
                                        .unwrap()
                                        .payload(),
                                    lang_frontend::parser::Statement::While { .. }
                                ));
                                normal += 1;
                            }
                            lang_frontend::ownership_checking::UnitDropPoint::ControlTransfer(
                                id,
                            ) => {
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
                    assert!(
                        off.loan_action_observations()
                            .iter()
                            .all(|catalog| catalog.loop_invariant_lineages().is_empty())
                    );
                    let catalogs = on.loan_action_observations();
                    let products = catalogs
                        .iter()
                        .flat_map(|c| c.loop_invariant_lineages())
                        .collect::<Vec<_>>();
                    if name == "normal" && root == "local" {
                        assert_eq!(products.len(), 1, "{root}/{name}/{count}");
                        let product = products[0];
                        assert_eq!(product.binding(), binding);
                        assert_eq!(
                            product.coverage(),
                            lang_frontend::ownership_checking::UnitLoanActionCoverage::Partial
                        );
                        let catalog = catalogs
                            .iter()
                            .find(|c| c.owns_loop_invariant_lineage(product))
                            .unwrap();
                        assert!(matches!(
                            catalog.action(&product.source_begin()),
                            Some(lang_frontend::ownership_checking::UnitLoanAction::Begin(_))
                        ));
                        assert!(matches!(catalog.action(&product.handoff()), Some(lang_frontend::ownership_checking::UnitLoanAction::BorrowHandoffCommitted(_))));
                        assert!(
                            matches!(catalog.action(&product.binding_end()), Some(lang_frontend::ownership_checking::UnitLoanAction::BorrowBindingEnd(end)) if end.binding()==binding)
                        );
                        let [entry, body_entry, body_return, exit] = product.boundary_cursors();
                        assert!(
                            entry <= body_entry && body_entry < body_return && body_return <= exit
                        );
                        assert_eq!(product.uses().len(), count);
                        let lang_frontend::parser::Statement::While {
                            condition, body, ..
                        } = parsed
                            .ast()
                            .statements()
                            .get(product.statement().statement())
                            .unwrap()
                            .payload()
                        else {
                            panic!("actual while")
                        };
                        assert_eq!(product.condition().expression(), *condition);
                        assert_eq!(product.body().statement(), *body);
                        let lang_frontend::parser::Statement::Block { elements } =
                            parsed.ast().statements().get(*body).unwrap().payload()
                        else {
                            panic!("actual body")
                        };
                        assert_eq!(elements.len(), count);
                        let mut previous = body_entry;
                        for (actual, statement) in product.uses().iter().zip(elements) {
                            let lang_frontend::parser::Statement::Expression { expression } =
                                parsed.ast().statements().get(*statement).unwrap().payload()
                            else {
                                panic!("actual call statement")
                            };
                            assert_eq!(actual.loan().call().expression(), *expression);
                            assert_eq!(actual.loan().call().source_unit(), binding.source_unit());
                            assert!(
                                matches!(catalog.action(&actual.begin()), Some(lang_frontend::ownership_checking::UnitLoanAction::Begin(loan)) if loan == actual.loan())
                            );
                            assert_eq!(
                                actual.return_point(),
                                lang_frontend::ownership_checking::UnitDropPoint::CallReturn(
                                    actual.loan().call()
                                )
                            );
                            assert!(
                                actual.return_cursor() > previous
                                    && actual.return_cursor() <= body_return
                            );
                            previous = actual.return_cursor();
                        }
                    } else {
                        assert!(products.is_empty(), "{name}: no new qualification");
                    }
                    off.validate().unwrap();
                    on.validate().unwrap();
                }
            }
        }
    }
}
