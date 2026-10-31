//! CI-only unit Map facts; preserve original qualified identities and reject before view publication.
use super::super::*;

#[test]
fn unit_map_private_fact_damage_cannot_publish_owned_capability() {
    let valid = super::super::constants_tests::analyze(
        "package a\nfun run(first:Map<Int,String>, second:Map<Int,String>) {for((key,value) in first){println(value)};for((key,value) in second){println(value)}}",
    );
    assert!(valid.diagnostics().is_empty() && valid.deferred().is_empty());
    assert_eq!(valid.map_iterations().len(), 2);
    assert!(valid.map_iteration_headers_are_valid());
    assert!(valid.clone().validate().is_ok());
    for fault in 0..8 {
        let mut damaged = valid.clone();
        match fault {
            0 => {
                damaged.map_iterations.remove(0);
            }
            1 => damaged.map_iterations[0].source = valid.map_iterations[1].source.clone(),
            2 => damaged.map_iterations[0].loans[0] = valid.map_iterations[1].loans[0].clone(),
            3 => {
                damaged.map_iterations[0].descriptor.value_type =
                    valid.map_iterations[0].descriptor.key_type
            }
            4 => damaged.map_iterations[0].bindings.swap(0, 1),
            5 => damaged.map_iterations[0]
                .exits
                .retain(|e| e.kind() != UnitIterationExitKind::Exhaustion),
            6 => damaged.map_iterations[0]
                .exits
                .iter_mut()
                .find(|e| e.kind() == UnitIterationExitKind::Fallthrough)
                .unwrap()
                .actions
                .swap(0, 1),
            7 => {
                let statement = damaged.map_iterations[0].descriptor.statement();
                damaged.map_iterations[0]
                    .exits
                    .iter_mut()
                    .find(|e| e.kind() == UnitIterationExitKind::Fallthrough)
                    .unwrap()
                    .actions
                    .insert(0, UnitIterationCleanupAction::EndSource(statement));
            }
            _ => unreachable!(),
        }
        if fault < 5 {
            assert!(
                !damaged.map_iteration_headers_are_valid(),
                "fault={fault}: header guard"
            );
        } else {
            assert!(damaged.map_iteration_headers_are_valid());
            assert!(
                !damaged.iterations_are_valid(),
                "fault={fault}: complete cleanup guard"
            );
        }
        let before = format!("{damaged:?}");
        let recovery = damaged
            .validate()
            .expect_err("bad Map cannot publish owned unit");
        assert_eq!(
            before,
            format!("{:?}", *recovery),
            "reject preserves original recovery"
        );
        println!("SPEC0331 CI MAP FACT unit fault={fault} capability=rejected");
    }
}

// Stage A: original Single source and filename, without retiring its reason/span oracle.
#[test]
fn original_map_private_damage_preserves_rooted_and_memory_recovery() {
    use crate::{
        lexer::lex,
        name_resolution::{
            SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names,
        },
        parser::parse_file,
        source::SourceMap,
        type_checking::{
            check_compilation_unit_types, complete_compilation_unit_types, standard_environments,
        },
    };
    const SOURCE: &str = "fun run(first: Map<Int, String>, second: Map<Int, String>): Unit { for ((key, value) in first) { println(value) }; for ((key, value) in second) { println(value) } }";
    let mut sources = SourceMap::new();
    let source = sources.add_source("map-ci.ko", SOURCE).unwrap();
    let lexed = lex(&sources, source).unwrap();
    assert!(lexed.diagnostics().is_empty());
    let parsed = parse_file(&sources, &lexed).unwrap();
    assert!(parsed.diagnostics().is_empty());
    for input in [
        SourceUnitInput::new("root", "map-ci.ko", source, &parsed),
        SourceUnitInput::memory_document(source, &parsed),
    ] {
        let inputs = [input];
        let (ne, te) = standard_environments();
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &ne).unwrap();
        assert!(names.diagnostics().is_empty());
        let names = names.validate().unwrap();
        let source_unit = names.names().index().source_units()[0].id();
        let raw = check_compilation_unit_types(&sources, &inputs, &names, &te).unwrap();
        assert!(raw.diagnostics().is_empty());
        let complete =
            complete_compilation_unit_types(&sources, &inputs, &names, &te, &raw).unwrap();
        assert!(complete.is_same_analysis(&raw));
        let typed = raw.validate().unwrap();
        let valid =
            check_compilation_unit_ownership(&sources, &inputs, &names, &te, &typed).unwrap();
        assert!(valid.diagnostics().is_empty() && valid.deferred().is_empty());
        assert!(valid.is_compatible_with(&typed));
        assert!(valid.clone().validate().is_ok());
        assert_eq!(valid.map_iterations().len(), 2);
        assert!(valid.map_iteration_headers_are_valid());
        assert!(valid.iterations_are_valid());
        for plan in valid.map_iterations() {
            assert_eq!(plan.descriptor.statement().source_unit(), source_unit);
        }
        for fault in 0..8 {
            let mut damaged = valid.clone();
            match fault {
                0 => {
                    damaged.map_iterations.remove(0);
                }
                1 => damaged.map_iterations[0].source = valid.map_iterations[1].source.clone(),
                2 => damaged.map_iterations[0].loans[0] = valid.map_iterations[1].loans[0].clone(),
                3 => {
                    damaged.map_iterations[0].descriptor.value_type =
                        valid.map_iterations[0].descriptor.key_type
                }
                4 => damaged.map_iterations[0].bindings.swap(0, 1),
                5 => damaged.map_iterations[0]
                    .exits
                    .retain(|e| e.kind() != UnitIterationExitKind::Exhaustion),
                6 => damaged.map_iterations[0]
                    .exits
                    .iter_mut()
                    .find(|e| e.kind() == UnitIterationExitKind::Fallthrough)
                    .unwrap()
                    .actions
                    .swap(0, 1),
                7 => {
                    let statement = damaged.map_iterations[0].descriptor.statement();
                    damaged.map_iterations[0]
                        .exits
                        .iter_mut()
                        .find(|e| e.kind() == UnitIterationExitKind::Fallthrough)
                        .unwrap()
                        .actions
                        .insert(0, UnitIterationCleanupAction::EndSource(statement));
                }
                _ => unreachable!(),
            }
            assert_ne!(
                damaged, valid,
                "fault={fault}: independent nonempty mutation"
            );
            assert!(damaged.diagnostics().is_empty() && damaged.deferred().is_empty());
            assert!(damaged.is_compatible_with(&typed));
            if fault < 5 {
                assert!(!damaged.map_iteration_headers_are_valid(), "fault={fault}");
            } else {
                assert!(damaged.map_iteration_headers_are_valid(), "fault={fault}");
                assert!(!damaged.iterations_are_valid(), "fault={fault}");
            }
            let expected_recovery = damaged.clone();
            let recovery = damaged
                .validate()
                .expect_err("private Map damage must not publish");
            assert!(recovery.is_same_analysis(&expected_recovery));
            assert!(recovery.is_compatible_with(&typed));
            assert_eq!(
                *recovery, expected_recovery,
                "fault={fault}: unchanged whole recovery"
            );
            assert!(valid.clone().validate().is_ok());
        }
    }
}
