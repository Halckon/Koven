//! The new core capability does not authorize other loop-local borrowing shapes.
use lang_frontend::{
    lexer::lex,
    name_resolution::{
        SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names, resolve_names,
    },
    ownership_checking::{check_compilation_unit_ownership, check_ownership},
    parser::parse_file,
    source::SourceMap,
    type_checking::{check_compilation_unit_types, check_types, standard_environments},
};
fn observing<T>(on: bool, run: impl FnOnce() -> T) -> T {
    #[cfg(feature = "loan-observation-test-support")]
    {
        lang_frontend::ownership_checking::with_loan_observation(on, run)
    }
    #[cfg(not(feature = "loan-observation-test-support"))]
    {
        let _ = on;
        run()
    }
}
#[test]
fn ordinary_no_carry_rejects_unsealed_control_and_resource_boundaries() {
    for (name, source_mode, body) in [
        (
            "continue",
            "",
            "while(flag){borrow val item=view(source);observe(item);continue}",
        ),
        (
            "break",
            "",
            "while(flag){borrow val item=view(source);observe(item);break}",
        ),
        (
            "return",
            "",
            "while(flag){borrow val item=view(source);observe(item);return}",
        ),
        (
            "conditional",
            "",
            "while(flag){borrow val item=view(source);if(flag){observe(item)}}",
        ),
        (
            "empty-consumer",
            "",
            "while(flag){borrow val item=view(source)}",
        ),
        (
            "second-root",
            "",
            "while(flag){borrow val item=view(source);observe(other);observe(item)}",
        ),
        (
            "second-binding",
            "",
            "while(flag){borrow val item=view(source);borrow val sibling=view(source);observe(item);observe(sibling)}",
        ),
        (
            "own-source",
            "own ",
            "while(flag){borrow val item=view(source);observe(item)}",
        ),
        (
            "local-owner",
            "",
            "while(flag){val local=\"x\";borrow val item=view(local);observe(item)}",
        ),
        (
            "nested-body-block",
            "",
            "while(flag){{borrow val item=view(source);observe(item)}}",
        ),
        (
            "nested-loop",
            "",
            "while(flag){while(flag){borrow val item=view(source);observe(item)}}",
        ),
        (
            "outer-branch",
            "",
            "if(flag){while(flag){borrow val item=view(source);observe(item)}}",
        ),
        (
            "outer-block",
            "",
            "{while(flag){borrow val item=view(source);observe(item)}}",
        ),
        (
            "constant-condition",
            "",
            "while(true){borrow val item=view(source);observe(item)}",
        ),
    ] {
        let text = format!(
            "fun view(source:String):borrow String from source=source\nfun observe(item:String):Unit {{}}\nfun run({source_mode}source:String,other:String,flag:Boolean):Unit {{{body}}}"
        );
        let mut sources = SourceMap::new();
        let source = sources.add_source(name, &text).unwrap();
        let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
        assert!(parsed.diagnostics().is_empty(), "{name}");
        let (env, types) = standard_environments();
        let names = resolve_names(&sources, &parsed, &env).unwrap();
        assert!(names.diagnostics().is_empty(), "{name}");
        let typed = check_types(&sources, &parsed, &names, &types).unwrap();
        assert!(
            typed.diagnostics().is_empty(),
            "{name}: {:?}",
            typed.diagnostics()
        );
        let mut single = None;
        for on in [false, true] {
            let result = observing(on, || {
                check_ownership(&sources, &parsed, &names, &typed).unwrap()
            });
            assert!(
                result
                    .diagnostics()
                    .iter()
                    .any(|d| d.code().to_string() == "L0164"),
                "{name}: {:?}",
                result.diagnostics()
            );
            assert!(result.borrow_results().bindings().is_empty(), "{name}");
            if let Some(prior) = single.as_ref() {
                assert_eq!(
                    prior,
                    &(
                        result.diagnostics().to_vec(),
                        result.borrow_results().clone(),
                        result.loans().to_vec()
                    )
                );
            }
            single = Some((
                result.diagnostics().to_vec(),
                result.borrow_results().clone(),
                result.loans().to_vec(),
            ));
        }
        for rooted in [false, true] {
            let inputs = [if rooted {
                SourceUnitInput::new("root", "boundary.ko", source, &parsed)
            } else {
                SourceUnitInput::memory_document(source, &parsed)
            }];
            let index = index_compilation_unit(&sources, &inputs).unwrap();
            let names = resolve_compilation_unit_names(&sources, &inputs, &index, &env)
                .unwrap()
                .validate()
                .unwrap();
            let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
            assert!(
                typed.diagnostics().is_empty(),
                "{name}: {:?}",
                typed.diagnostics()
            );
            let typed = typed.validate().unwrap();
            let mut previous = None;
            for on in [false, true] {
                let result = observing(on, || {
                    check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
                        .unwrap()
                });
                assert!(
                    result
                        .diagnostics()
                        .iter()
                        .any(|d| d.code().to_string() == "L0164"),
                    "{name}/{rooted}: {:?}",
                    result.diagnostics()
                );
                assert!(
                    result.no_carry_loops().is_empty(),
                    "no partial or failed capability"
                );
                assert!(
                    result.borrow_results().bindings().is_empty(),
                    "atomic failed core result"
                );
                if let Some(prior) = previous.as_ref() {
                    assert_eq!(prior, &result);
                }
                previous = Some(result);
            }
        }
    }
}

#[test]
fn ordinary_no_carry_siblings_keep_identities_and_global_failure_revokes_all() {
    // An earlier invalid sibling must not be forgotten when a later body has a valid shape.
    for (same_source, rejected_at) in [
        (false, None),
        (false, Some(0)),
        (false, Some(1)),
        (true, None),
        (true, Some(0)),
        (true, Some(1)),
    ] {
        let rejected = rejected_at.is_some();
        let first_tail = if rejected_at == Some(0) {
            ";continue"
        } else {
            ""
        };
        let second_tail = if rejected_at == Some(1) {
            ";continue"
        } else {
            ""
        };
        let second_source = if same_source { "source" } else { "other" };
        let text = format!(
            "fun view(source:String):borrow String from source=source\nfun observe(item:String):Unit {{}}\nfun run(source:String,other:String,flag:Boolean):Unit {{while(flag){{borrow val first=view(source);observe(first){first_tail}}};while(flag){{borrow val second=view({second_source});observe(second){second_tail}}}}}"
        );
        let mut sources = SourceMap::new();
        let source = sources.add_source("siblings.ko", &text).unwrap();
        let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
        assert!(parsed.diagnostics().is_empty());
        let (env, types) = standard_environments();
        let names = resolve_names(&sources, &parsed, &env).unwrap();
        let typed = check_types(&sources, &parsed, &names, &types).unwrap();
        assert!(typed.diagnostics().is_empty());
        let single = check_ownership(&sources, &parsed, &names, &typed).unwrap();
        assert_eq!(single.diagnostics().is_empty(), !rejected);
        assert_eq!(
            single.borrow_results().bindings().len(),
            if rejected { 0 } else { 2 }
        );
        for rooted in [false, true] {
            let inputs = [if rooted {
                SourceUnitInput::new("root", "siblings.ko", source, &parsed)
            } else {
                SourceUnitInput::memory_document(source, &parsed)
            }];
            let index = index_compilation_unit(&sources, &inputs).unwrap();
            let names = resolve_compilation_unit_names(&sources, &inputs, &index, &env)
                .unwrap()
                .validate()
                .unwrap();
            let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
                .unwrap()
                .validate()
                .unwrap();
            for on in [false, true] {
                let checked = observing(on, || {
                    check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
                        .unwrap()
                });
                assert_eq!(
                    checked.diagnostics().is_empty(),
                    !rejected,
                    "{:?}",
                    checked.diagnostics()
                );
                if rejected {
                    assert!(checked.no_carry_loops().is_empty());
                    assert!(checked.borrow_results().bindings().is_empty());
                } else {
                    let [left, right] = checked.no_carry_loops() else {
                        panic!("two separately sealed loop frames")
                    };
                    assert_ne!(left.loop_statement(), right.loop_statement());
                    assert_ne!(left.body(), right.body());
                    assert_ne!(left.binding(), right.binding());
                    assert_eq!(left.source() == right.source(), same_source);
                    assert_ne!(left.initializer(), right.initializer());
                    assert_ne!(left.end(), right.end());
                    assert_eq!(left.callable(), right.callable());
                    assert!(checked.owns_no_carry_loop(left) && checked.owns_no_carry_loop(right));
                    assert_eq!(left.consumers().len(), 1);
                    assert_eq!(right.consumers().len(), 1);
                }
            }
        }
    }
}
