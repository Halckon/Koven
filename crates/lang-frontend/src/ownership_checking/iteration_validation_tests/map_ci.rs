//! CI-only single-dimension damage to actual Map facts. Never execute on developer hosts.
use crate::ownership_checking::{
    IterationCleanupAction, IterationExitKind, check_ownership, validate_iteration_facts,
};
use crate::{
    lexer::lex,
    name_resolution::resolve_names,
    parser::parse_file,
    source::SourceMap,
    type_checking::{check_types, complete_file_types, standard_environments},
};

const SOURCE: &str = "fun run(first: Map<Int, String>, second: Map<Int, String>): Unit { for ((key, value) in first) { println(value) }; for ((key, value) in second) { println(value) } }";

#[test]
fn map_private_fact_damage_rejects_at_original_provider() {
    let mut sources = SourceMap::new();
    let source = sources.add_source("map-ci.ko", SOURCE).unwrap();
    let lexed = lex(&sources, source).unwrap();
    assert!(lexed.diagnostics().is_empty());
    let parsed = parse_file(&sources, &lexed).unwrap();
    assert!(parsed.diagnostics().is_empty());
    let (ne, te) = standard_environments();
    let names = resolve_names(&sources, &parsed, &ne).unwrap();
    assert!(names.diagnostics().is_empty());
    let typed = check_types(&sources, &parsed, &names, &te).unwrap();
    assert!(typed.diagnostics().is_empty());
    let complete = complete_file_types(&sources, &parsed, &names, &te, &typed).unwrap();
    assert!(complete.is_same_analysis(&typed));
    let valid = check_ownership(&sources, &parsed, &names, complete.types()).unwrap();
    assert!(valid.diagnostics().is_empty() && valid.deferred().is_empty());
    assert_eq!(valid.map_iterations().len(), 2);
    validate_iteration_facts(&parsed, &names, &typed, &valid).unwrap();
    for fault in 0..8 {
        let mut damaged = valid.clone();
        let expected = match fault {
            0 => {
                damaged.map_iterations.remove(0);
                "missing or duplicate acquired Map ownership plan"
            }
            1 => {
                damaged.map_iterations[0].source = valid.map_iterations[1].source.clone();
                "Map provider differs from actual typed/source loan traversal"
            }
            2 => {
                damaged.map_iterations[0].loans[0] = valid.map_iterations[1].loans[0].clone();
                "Map provider differs from actual typed/source loan traversal"
            }
            3 => {
                damaged.map_iterations[0].descriptor.value_type =
                    valid.map_iterations[0].descriptor.key_type;
                "Map provider differs from actual typed/source loan traversal"
            }
            4 => {
                damaged.map_iterations[0].bindings.swap(0, 1);
                "Map provider differs from actual typed/source loan traversal"
            }
            5 => {
                damaged.map_iterations[0]
                    .exits
                    .retain(|e| e.kind() != IterationExitKind::Exhaustion);
                "orphan Map cleanup outside its actual exit points"
            }
            6 => {
                damaged.map_iterations[0]
                    .exits
                    .iter_mut()
                    .find(|e| e.kind() == IterationExitKind::Fallthrough)
                    .unwrap()
                    .actions
                    .swap(0, 1);
                "mixed-provider cleanup point is not one complete sequence"
            }
            7 => {
                let statement = damaged.map_iterations[0].descriptor.statement();
                damaged.map_iterations[0]
                    .exits
                    .iter_mut()
                    .find(|e| e.kind() == IterationExitKind::Fallthrough)
                    .unwrap()
                    .actions
                    .insert(0, IterationCleanupAction::EndSource(statement));
                "mixed-provider cleanup point is not one complete sequence"
            }
            _ => unreachable!(),
        };
        let before = format!("{damaged:?}");
        let error = validate_iteration_facts(&parsed, &names, &typed, &damaged)
            .expect_err("damaged Map fact must fail before lowering");
        assert_eq!(error.reason(), expected, "fault={fault}: {error:?}");
        assert_eq!(
            error.span(),
            Some(
                parsed
                    .ast()
                    .statements()
                    .get(valid.map_iterations[0].descriptor.statement())
                    .unwrap()
                    .span()
            ),
            "fault={fault}"
        );
        assert_eq!(
            before,
            format!("{damaged:?}"),
            "validator must not repair or publish damage"
        );
        println!(
            "SPEC0331 CI MAP FACT single fault={fault} reason={} original_span={:?}",
            error.reason(),
            error.span()
        );
    }
}
