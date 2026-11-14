//! Quarantined syntax exploration only: no core sessions, facts, End events or authority.
use super::*;
use crate::{lexer::lex, parser::parse_file, source::SourceMap};

fn probe(source: &str) -> (bool, bool) {
    let mut sources = SourceMap::new();
    let id = sources.add_source("two-source-shape.ko", source).unwrap();
    let lexed = lex(&sources, id).unwrap();
    assert!(lexed.diagnostics().is_empty());
    let parsed = parse_file(&sources, &lexed).unwrap();
    assert!(parsed.diagnostics().is_empty());
    let (loop_id, body) = parsed
        .ast()
        .statements()
        .iter()
        .find_map(|(id, node)| {
            if let Statement::While { body, .. } = node.payload() {
                Some((id, *body))
            } else {
                None
            }
        })
        .unwrap();
    // These syntax-only callbacks deliberately confer no typed/ownership authority.
    // Fixture names are unique; actual checker adapters must use resolved symbol identities.
    let resolve = |name| {
        let span = match name {
            Name::Reference(span) | Name::Declaration(NameMarker::Present(span)) => span,
            _ => return None,
        };
        sources.slice(span).ok()
    };
    let text = |id| {
        sources
            .slice(parsed.ast().expressions().get(id).unwrap().span())
            .unwrap()
    };
    let parameter = |id, boolean| {
        if boolean {
            text(id) == "choose"
        } else {
            matches!(text(id), "left" | "right")
        }
    };
    let call = |id, initializer| {
        let Expression::Call { callee, .. } = parsed.ast().expressions().get(id).unwrap().payload()
        else {
            return false;
        };
        text(*callee) == if initializer { "view" } else { "consume" }
    };
    let checks = (
        |id| matches!(text(id), "remaining" | "rounds" | "0" | "1" | "remaining-1"),
        |id| text(id) == "rounds",
        |id, expected: &str| text(id) == expected,
    );
    let old = selection(&parsed, loop_id, body, resolve, parameter, call, checks).is_some();
    let candidate = select::<_, true>(&parsed, loop_id, body, resolve, parameter, call, checks);
    if let Some(selected) = &candidate {
        assert_eq!(selected.first.root, "left");
        assert_eq!(selected.second.as_ref().unwrap().root, "right");
        assert_ne!(selected.first.body, selected.second.as_ref().unwrap().body);
        assert!(selected.counter.is_some());
        assert!(selected.exit.is_none() && selected.conditional.is_none());
    }
    (old, candidate.is_some())
}

const SOURCE: &str = "fun view(source:String):borrow String from source=source\nfun consume(item:String):Unit {}\nfun run(left:String,right:String,rounds:Int,choose:Boolean):Unit {var remaining=rounds;while(remaining>0){if(choose){borrow val a=view(left);consume(a)}else{borrow val b=view(right);consume(b);consume(b)};remaining=remaining-1}}";

#[test]
fn distinct_source_counter_syntax_stays_outside_production_selection() {
    assert_eq!(probe(SOURCE), (false, true));
    // Genuine frontend rejection remains the public result, despite syntax exploration.
    use crate::{
        name_resolution::resolve_names,
        ownership_checking::check_ownership,
        type_checking::{check_types, standard_environments},
    };
    for (source, accepted) in [
        (SOURCE.to_owned(), false),
        (SOURCE.replace("view(right)", "view(left)"), true),
    ] {
        let mut sources = SourceMap::new();
        let id = sources
            .add_source("production-boundary.ko", source)
            .unwrap();
        let parsed = parse_file(&sources, &lex(&sources, id).unwrap()).unwrap();
        let (environment, types) = standard_environments();
        let names = resolve_names(&sources, &parsed, &environment).unwrap();
        assert!(names.diagnostics().is_empty());
        let typed = check_types(&sources, &parsed, &names, &types).unwrap();
        assert!(typed.diagnostics().is_empty());
        let owned = check_ownership(&sources, &parsed, &names, &typed).unwrap();
        assert_eq!(owned.diagnostics().is_empty(), accepted);
        if !accepted {
            assert!(
                owned
                    .diagnostics()
                    .iter()
                    .any(|d| d.code().to_string() == "L0164")
            );
        }
    }
    assert_eq!(
        probe(&SOURCE.replace("view(right)", "view(left)")),
        (true, false)
    );
    assert_eq!(
        probe(&SOURCE.replace("remaining=remaining-1", "remaining=remaining-2")),
        (false, false)
    );
    assert_eq!(
        probe(&SOURCE.replace("remaining=remaining-1", "break")),
        (false, false)
    );
    assert_eq!(
        probe(
            &SOURCE
                .replace("remaining>0", "choose")
                .replace(";remaining=remaining-1", "")
        ),
        (false, false)
    );
    assert_eq!(
        probe(&SOURCE.replace("consume(b);consume(b)", "consume(b);return")),
        (false, false)
    );
}

#[cfg(feature = "loan-observation-test-support")]
#[test]
fn actual_unit_rejection_cannot_publish_candidate_authority() {
    use crate::{
        name_resolution::{
            SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names,
        },
        ownership_checking::{
            check_compilation_unit_ownership, legacy_ownership_eq, with_loan_observation,
        },
        type_checking::{check_compilation_unit_types, standard_environments},
    };
    for (source, accepted) in [
        (SOURCE.to_owned(), false),
        (SOURCE.replace("view(right)", "view(left)"), true),
    ] {
        let mut sources = SourceMap::new();
        let id = sources.add_source("unit-candidate.ko", source).unwrap();
        let lexed = lex(&sources, id).unwrap();
        let parsed = parse_file(&sources, &lexed).unwrap();
        assert!(lexed.diagnostics().is_empty() && parsed.diagnostics().is_empty());
        for input in [
            SourceUnitInput::new("root", "unit-candidate.ko", id, &parsed),
            SourceUnitInput::memory_document(id, &parsed),
        ] {
            let inputs = [input];
            let (environment, types) = standard_environments();
            let index = index_compilation_unit(&sources, &inputs).unwrap();
            let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
                .unwrap()
                .validate()
                .unwrap();
            let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
                .unwrap()
                .validate()
                .unwrap();
            let analyze = |enabled| {
                with_loan_observation(enabled, || {
                    check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
                        .unwrap()
                })
            };
            let observed = analyze(true);
            let unobserved = analyze(false);
            assert!(legacy_ownership_eq(&observed, &unobserved));
            for owned in [observed, unobserved] {
                assert_eq!(owned.diagnostics().is_empty(), accepted);
                assert!(owned.no_carry_loops().is_empty());
                assert!(owned.no_carry_branches().is_empty());
                assert_eq!(
                    owned.no_carry_counter_branches().len(),
                    usize::from(accepted)
                );
                if accepted {
                    let fact = &owned.no_carry_counter_branches()[0];
                    assert!(owned.owns_no_carry_counter_branch(fact));
                    assert!(!owned.owns_no_carry_branch(fact.pair()));
                    assert_eq!(
                        fact.pair().arms()[0].source(),
                        fact.pair().arms()[1].source()
                    );
                    assert_ne!(
                        fact.pair().arms()[0].binding(),
                        fact.pair().arms()[1].binding()
                    );
                } else {
                    assert!(
                        owned
                            .diagnostics()
                            .iter()
                            .any(|d| d.code().to_string() == "L0164")
                    );
                    assert!(owned.borrow_results().bindings().is_empty());
                }
                assert_eq!(owned.validate().is_ok(), accepted);
            }
        }
    }
}
