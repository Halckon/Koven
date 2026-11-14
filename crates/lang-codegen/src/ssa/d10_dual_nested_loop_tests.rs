//! Exact two-operation nested observation handed to normal lowering and independently replayed.
use super::model::*;
use super::*;
use lang_frontend::ownership_checking::OwnedCompilationUnitView;
#[path = "d10_parent_after_child_tests.rs"]
mod parent_after_child;
mod path;
mod replay;
mod source;
const SOURCE: &str = "fun run(count: Int) { var a = \"first\"; var b = \"kept\"; var outer = count; while(outer > 0) { val parentOld = replace(&a, \"parent\"); var inner = count; while(inner > 0) { val old = replace(&a, \"next\"); inner -= 1 }; outer -= 1 }; println(b) }";
#[test]
fn actual_dual_nested_replacements_preserve_zero_and_backedge_generations() {
    run_cases(None, false);
}

#[test]
fn actual_postuse_paths_preserve_logical_generations_and_cleanup() {
    native_postuse_program(path::check_program);
}

#[test]
#[ignore = "CI-only cross-analysis nested replay identity mismatch"]
fn ci_only_dual_replay_rejects_foreign_owned_identity() {
    assert_eq!(std::env::var("GITHUB_ACTIONS").as_deref(), Ok("true"));
    run_cases(None, true);
}

/// Test adapter receives only the original, fully replayed dual-postuse program.
pub(crate) fn native_postuse_program(callback: fn(&Program, FunctionId)) {
    assert_eq!(
        run_cases(Some(callback), false),
        2,
        "Rooted and MemoryDocument must both reach the adapter"
    );
}

fn run_cases(native: Option<fn(&Program, FunctionId)>, foreign_owned: bool) -> usize {
    let mut deliveries = 0;
    use lang_frontend::{
        name_resolution::{
            SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names,
        },
        ownership_checking::{d10_catalog_experiment as experiment, owned_compilation_unit_view},
        source::SourceMap,
        type_checking::{check_compilation_unit_types, standard_environments},
    };
    // Callback delivery remains pinned to the original native matrix fixture, not identifier spelling.
    let native_fixture = SOURCE
        .replace("count: Int", "count: Int, childCount: Int")
        .replace("var inner = count", "var inner = childCount")
        .replace("println(b)", "println(a); println(b)");
    for (text, accepted) in [
        (native_fixture.replace("; ", ";\n  "), true),
        (
            native_fixture
                .replace("first", "初始")
                .replace("kept", "保留")
                .replace("parent\"", "父\"")
                .replace("next", "子"),
            true,
        ),
        (SOURCE.to_owned(), true),
        (SOURCE.replace("fun run", "fun renamed"), true),
        (renamed_source(SOURCE), true),
        (
            renamed_source(
                &SOURCE
                    .replace("count: Int", "count: Int, childCount: Int")
                    .replace("var inner = count", "var inner = childCount")
                    .replace("println(b)", "println(a); println(b)"),
            ),
            true,
        ),
        (
            SOURCE
                .replace("; ", ";\n  ")
                .replace("while(", "while (")
                .replace(" > ", " >  "),
            true,
        ),
        (SOURCE.replace("println(b)", "println(a); println(b)"), true),
        (
            SOURCE
                .replace("count: Int", "count: Int, childCount: Int")
                .replace("var inner = count", "var inner = childCount")
                .replace("println(b)", "println(a); println(b)"),
            true,
        ),
        (
            SOURCE
                .replace("count: Int", "count: Int, childCount: Int")
                .replace("var inner = count", "var inner = childCount"),
            true,
        ),
        (
            SOURCE
                .replace("first", "初始")
                .replace("kept", "保留")
                .replace("parent\"", "父\"")
                .replace("next", "子"),
            true,
        ),
        (SOURCE.replace("parent\"", "next\""), true),
        (
            SOURCE.replace("replace(&a, \"parent\")", "replace(&b, \"parent\")"),
            false,
        ),
        (
            SOURCE.replace("replace(&a, \"next\")", "replace(&b, \"next\")"),
            false,
        ),
        (
            SOURCE.replace("var inner = count", "var inner = outer"),
            false,
        ),
        (SOURCE.replace("inner -= 1", "inner -= 2"), false),
        (SOURCE.replace("println(b)", "println(a)"), false),
    ] {
        if native.is_some() && !(accepted && text == native_fixture) {
            continue;
        }
        let mut sources = SourceMap::new();
        let (source, ast) = super::unit_lower_test_support::parsed(&mut sources, "dual.ko", &text);
        let (environment, types) = standard_environments();
        for input in [
            SourceUnitInput::new("root", "dual.ko", source, &ast),
            SourceUnitInput::memory_document(source, &ast),
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
            let bound =
                experiment::analyze_dual_nested_loop(&sources, &inputs, &names, &types, &typed)
                    .unwrap();
            assert_eq!(bound.is_some(), accepted, "{text}");
            assert!(
                experiment::analyze_nested_loop(&sources, &inputs, &names, &types, &typed)
                    .unwrap()
                    .is_none()
            );
            let Some(bound) = bound else {
                continue;
            };
            let unit = owned_compilation_unit_view(
                &sources,
                &inputs,
                &names,
                &types,
                &typed,
                bound.ownership(),
            )
            .unwrap();
            assert!(bound.matches(&unit));
            let evidence = source::facts(&unit).expect("real initializer source identities");
            assert_eq!(bound.counter_parameters(), evidence.counter_parameters);
            assert_eq!(bound.counter_initializers(), evidence.counter_initializers);
            assert_eq!(
                bound.final_root_use().map(|u| (
                    Origin::Source(u.loan().begin_span()),
                    Origin::Source(u.loan().end_span())
                )),
                evidence.final_root_use
            );
            assert_ne!(bound.replacements()[0], bound.replacements()[1]);
            assert_ne!(bound.producer_sites()[1], bound.producer_sites()[2]);
            let callable = bound.controls().0;
            let entries = names
                .names()
                .index()
                .declarations()
                .iter()
                .filter(|d| {
                    d.source_unit() == callable.source_unit() && d.root() == callable.item()
                })
                .collect::<Vec<_>>();
            let [entry] = entries.as_slice() else {
                panic!("exact bound callable must resolve to one declaration");
            };
            let entry = entry.id();
            let (program, function) =
                unit_lower::lower_owned_unit_with_entry(&unit, entry).unwrap();
            replay::replay(&unit, &bound, &program, function)
                .expect("actual dual Replace CFG and cleanup replay");
            let publication_view = owned_compilation_unit_view(
                &sources,
                &inputs,
                &names,
                &types,
                &typed,
                bound.ownership(),
            )
            .unwrap();
            let publication = crate::d10_experiment::lower_before_child(publication_view, &bound);
            let [parent_parameter, child_parameter] = bound.counter_parameters();
            if parent_parameter != child_parameter && bound.final_root_use().is_some() {
                let publication = publication.expect("complete before-child publication");
                assert_eq!(publication.callable(), bound.controls().0);
                assert_eq!(publication.source_count(), 1);
                assert!(publication.function_name().is_some());
                let (actual, actual_entry) = publication.replay_input();
                replay::replay(&unit, &bound, actual, actual_entry)
                    .expect("independent old source/graph oracle after publication");
                path::check_before_child(actual, actual_entry, &evidence.literals);
                publication.verified_llvm_ir().unwrap();
            } else {
                let error = match publication {
                    Ok(_) => panic!("narrow publication must reject unproved source scope"),
                    Err(error) => error,
                };
                assert_eq!(error.stage(), crate::d10_experiment::Stage::Graph);
            }
            if foreign_owned {
                // An independent real planner run, not a forged or edited fact collection.
                let other =
                    experiment::analyze_dual_nested_loop(&sources, &inputs, &names, &types, &typed)
                        .unwrap()
                        .expect("same supported source, different ownership identity");
                let foreign = owned_compilation_unit_view(
                    &sources,
                    &inputs,
                    &names,
                    &types,
                    &typed,
                    other.ownership(),
                )
                .unwrap();
                assert!(!bound.matches(&foreign));
                assert!(replay::replay(&foreign, &bound, &program, function).is_none());
                let publication = crate::d10_experiment::lower_before_child(foreign, &bound);
                let error = match publication {
                    Ok(_) => panic!("foreign final-owned analysis must not publish"),
                    Err(error) => error,
                };
                assert_eq!(error.stage(), crate::d10_experiment::Stage::Identity);
                assert!(replay::replay(&unit, &other, &program, function).is_none());
            }
            if bound.final_root_use().is_some() {
                crate::llvm::render_verified_program(&program)
                    .expect("real dual post-use graph reaches the LLVM adapter");
            }
            if let Some(callback) = native {
                callback(&program, function);
                deliveries += 1;
            }
        }
    }
    deliveries
}

// Alpha-renaming changes source spelling only; all roles must still be recovered from AST/IDs.
fn renamed_source(source: &str) -> String {
    source
        .replace("fun run", "fun inspect")
        .replace("parentOld", "detachedParent")
        .replace("val old", "val detachedChild")
        .replace("childCount", "innerLimit")
        .replace("count", "outerLimit")
        .replace("outer", "major")
        .replace("inner", "minor")
        .replace("var a ", "var primary ")
        .replace("&a", "&primary")
        .replace("println(a)", "println(primary)")
        .replace("var b ", "var retained ")
        .replace("println(b)", "println(retained)")
}

#[test]
#[ignore = "CI-only before-child source/graph negative controls"]
fn ci_only_before_child_graph_rejects_locally_valid_source_mismatch() {
    assert_eq!(std::env::var("GITHUB_ACTIONS").as_deref(), Ok("true"));
    use lang_frontend::{
        name_resolution::SourceUnitInput,
        ownership_checking::{d10_catalog_experiment as experiment, owned_compilation_unit_view},
        source::SourceMap,
        type_checking::standard_environments,
    };
    let text = SOURCE
        .replace("count: Int", "count: Int, childCount: Int")
        .replace("var inner = count", "var inner = childCount")
        .replace("println(b)", "println(a); println(b)");
    let mut sources = SourceMap::new();
    let (source_id, parsed) = unit_lower_test_support::parsed(&mut sources, "before-ci.ko", &text);
    let inputs = [SourceUnitInput::memory_document(source_id, &parsed)];
    let (environment, types) = standard_environments();
    let (names, typed, _) =
        unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
    let bound = experiment::analyze_dual_nested_loop(&sources, &inputs, &names, &types, &typed)
        .unwrap()
        .unwrap();
    let unit =
        owned_compilation_unit_view(&sources, &inputs, &names, &types, &typed, bound.ownership())
            .unwrap();
    let callable = bound.controls().0;
    let entry = names
        .names()
        .index()
        .declarations()
        .iter()
        .find(|d| d.source_unit() == callable.source_unit() && d.root() == callable.item())
        .unwrap()
        .id();
    for fault in 0..3 {
        let (mut program, function) =
            unit_lower::lower_owned_unit_with_entry(&unit, entry).unwrap();
        let f = program
            .module_mut(function.module())
            .unwrap()
            .function_mut(function)
            .unwrap();
        let instruction = f
            .instructions
            .iter_mut()
            .find(|i| match fault {
                0 => matches!(i.operation, Operation::StringLiteral { .. }),
                1 => matches!(i.operation, Operation::Constant(ScalarConstant::Integer(1))),
                _ => matches!(i.operation, Operation::Read { .. }),
            })
            .unwrap();
        match (&mut instruction.operation, fault) {
            (Operation::StringLiteral { bytes, .. }, 0) => bytes.push(b'!'),
            (Operation::Constant(ScalarConstant::Integer(value)), 1) => *value = 2,
            (_, 2) => instruction.origin = f.origin.clone(),
            _ => unreachable!("selected CI-only fault"),
        }
        verify::verify_program(&program).unwrap();
        assert!(d10_before_child_graph::check(&unit, &bound, &program, function).is_err());
    }
}
