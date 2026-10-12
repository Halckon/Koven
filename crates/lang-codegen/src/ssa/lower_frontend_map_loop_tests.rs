//! SPEC-0300: real source owner facts and normal-exhaustion SSA transport.
use super::{analyze, lower_scalar_file};
use lang_frontend::ownership_checking::{
    CleanupOwnerValue, DropPoint, DropTarget, IterationPhiBoundary, IterationPhiIncomingKind,
};

const ORIGINAL: &str =
    include_str!("../../tests/fixtures/owned-null-cleanup-0296/single-loop-boundary.ko");
const NORMAL: &str = include_str!("../../tests/fixtures/map-loop-owner-0300/normal-exhaustion.ko");

#[test]
fn single_map_loop_original_remove_uses_published_header_owner() {
    let analysis = analyze(ORIGINAL);
    assert!(analysis.parsed.diagnostics().is_empty());
    assert!(analysis.names.diagnostics().is_empty());
    assert!(analysis.typed.diagnostics().is_empty());
    assert!(analysis.owned.diagnostics().is_empty());
    let guarded = analysis
        .owned
        .drops()
        .iter()
        .find(|fact| fact.condition().is_some())
        .expect("the original source publishes a guarded Map owner");
    let DropTarget::Named(symbol) = guarded.target() else {
        panic!("the Map is a named owner")
    };
    let Some(CleanupOwnerValue::IterationPhi {
        statement,
        symbol: owner_symbol,
        boundary,
        ..
    }) = analysis
        .owned
        .cleanup_conditions()
        .owner_value(guarded.owner().unwrap())
    else {
        panic!("the drop is tied to an iteration owner")
    };
    assert_eq!(*owner_symbol, symbol);
    assert_eq!(*boundary, IterationPhiBoundary::Header);
    assert_eq!(guarded.point(), DropPoint::LoopExit(*statement));
    let plan = analysis.owned.iteration(*statement).unwrap();
    for kind in [
        IterationPhiIncomingKind::Entry,
        IterationPhiIncomingKind::Fallthrough,
        IterationPhiIncomingKind::Exhaustion,
    ] {
        assert!(
            plan.closure_phi_incomings()
                .iter()
                .any(|edge| edge.kind() == kind)
        );
    }
    let program = lower_scalar_file(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
    )
    .unwrap_or_else(|error| {
        panic!(
            "normal-exhaustion Map carrier: {error:?}; drops={:?}; incoming={:?}",
            analysis.owned.drops(),
            plan.closure_phi_incomings()
        )
    });
    crate::llvm::render_verified_program(&program).unwrap();
    assert_map_drop_counts(&program, &[("main", 1)]);
}

#[test]
fn unit_map_loop_original_source_is_an_independent_control() {
    assert_unit_lowers(ORIGINAL);
}

#[test]
fn single_map_loop_normal_exhaustion_preserves_dead_and_live_owners() {
    let analysis = analyze(NORMAL);
    assert!(
        analysis.typed.diagnostics().is_empty(),
        "{:?}",
        analysis.typed.diagnostics()
    );
    assert!(
        analysis.owned.diagnostics().is_empty(),
        "{:?}",
        analysis.owned.diagnostics()
    );
    let mut header_drops = 0;
    let mut exit_drops = 0;
    for fact in analysis
        .owned
        .drops()
        .iter()
        .filter(|fact| fact.condition().is_some())
    {
        let Some(CleanupOwnerValue::IterationPhi {
            statement,
            boundary,
            symbol,
            ..
        }) = analysis
            .owned
            .cleanup_conditions()
            .owner_value(fact.owner().unwrap())
        else {
            panic!("Map phi owner")
        };
        assert_eq!(
            analysis
                .typed
                .is_resource_type(analysis.typed.symbol_type(*symbol).unwrap()),
            Some(false)
        );
        let plan = analysis.owned.iteration(*statement).unwrap();
        let exit = plan
            .closure_phis()
            .iter()
            .find(|phi| phi.symbol() == *symbol && phi.boundary() == IterationPhiBoundary::Exit)
            .unwrap();
        let incoming = plan
            .closure_phi_incomings()
            .iter()
            .find(|edge| edge.kind() == IterationPhiIncomingKind::Exhaustion)
            .unwrap()
            .bindings()
            .iter()
            .find(|binding| binding.target() == exit.owner())
            .unwrap();
        assert_eq!(
            incoming.availability_selector(),
            exit.availability_selector()
        );
        match boundary {
            IterationPhiBoundary::Header => {
                header_drops += 1;
                assert_eq!(fact.point(), DropPoint::LoopExit(*statement));
                assert!(
                    incoming.values().is_empty(),
                    "cleaned Header must not reappear in Exit"
                );
                assert!(matches!(
                    analysis
                        .owned
                        .cleanup_conditions()
                        .get(incoming.available_when()),
                    Some(lang_frontend::ownership_checking::CleanupCondition::Never)
                ));
            }
            IterationPhiBoundary::Exit => {
                exit_drops += 1;
                assert_ne!(fact.point(), DropPoint::LoopExit(*statement));
                assert_eq!(
                    incoming.values().len(),
                    1,
                    "a live Exit receives one Header owner"
                );
                let header = plan
                    .closure_phis()
                    .iter()
                    .find(|phi| {
                        phi.symbol() == *symbol && phi.boundary() == IterationPhiBoundary::Header
                    })
                    .unwrap();
                assert_eq!(incoming.values()[0].source(), header.owner());
                assert_eq!(incoming.available_when(), header.availability_condition());
                assert_eq!(fact.condition(), Some(exit.availability_condition()));
            }
        }
    }
    assert_eq!((header_drops, exit_drops), (2, 2));
    let program = lower_scalar_file(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
    )
    .unwrap_or_else(|error| {
        panic!(
            "{error:?}; guarded={:?}; plans={:?}",
            analysis
                .owned
                .drops()
                .iter()
                .filter(|fact| fact.condition().is_some())
                .collect::<Vec<_>>(),
            analysis.owned.iterations()
        )
    });
    crate::llvm::render_verified_program(&program).unwrap();
    assert_map_drop_counts(
        &program,
        &[
            ("removeDead", 1),
            ("removeLive", 1),
            ("putDead", 1),
            ("putLive", 1),
        ],
    );
}

#[test]
fn unit_map_loop_normal_source_is_an_independent_control() {
    assert_unit_lowers(NORMAL);
}

#[test]
fn single_map_loop_abort_guard_combination_remains_explicit() {
    let source = include_str!("../../tests/fixtures/map-loop-owner-0300/guarded-abort-boundary.ko");
    assert_guarded_boundary(source);
}

#[test]
fn single_map_loop_control_exits_remain_explicit() {
    for exit in ["break", "continue", "return"] {
        let source = format!(
            "fun main(): Unit {{ var cache = mutableMapOf<Int, String>(); for (key in arrayOf(1, 2)) {{ cache.put(key, \"value\".clone()); if (key == 1) {{ {exit} }} }}; cache.contains(1) }}"
        );
        assert_guarded_boundary(&source);
    }
}

#[test]
fn single_map_loop_replaced_seed_remains_explicit() {
    let source = "fun main(): Unit { var cache = mutableMapOf<Int, String>(); for (key in arrayOf(1, 2)) { cache = mutableMapOf<Int, String>(); cache.put(key, \"value\".clone()) }; cache.contains(1) }";
    assert_guarded_boundary(source);
}

fn assert_guarded_boundary(source: &str) {
    let analysis = analyze(source);
    assert!(
        analysis.typed.diagnostics().is_empty(),
        "{:?}",
        analysis.typed.diagnostics()
    );
    assert!(
        analysis.owned.diagnostics().is_empty(),
        "{:?}",
        analysis.owned.diagnostics()
    );
    let error = lower_scalar_file(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
    )
    .err()
    .expect("unproved owner selection must remain explicit");
    assert_eq!(error.kind, super::LoweringErrorKind::UnsupportedNode);
    assert!(
        analysis
            .owned
            .drops()
            .iter()
            .any(|fact| fact.condition().is_some() && Some(fact.value_origin()) == error.span)
    );
}

fn assert_unit_lowers(source: &str) {
    use crate::ssa::{
        unit_lower::lower_scalar_unit_with_entry,
        unit_lower_test_support::{analyze, declaration, parsed},
    };
    use lang_frontend::{
        name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
    };
    let mut sources = SourceMap::new();
    let (source, file) = parsed(&mut sources, "p/main.ko", &format!("package p\n{source}"));
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &file)];
    let (environment, types) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &environment, &types);
    let (program, entry) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &types,
        &typed,
        &owned,
        declaration(&names, "p", "main"),
    )
    .unwrap();
    crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
}

#[test]
fn single_map_loop_parameter_seed_and_same_typed_owners_stay_distinct() {
    let source = include_str!("../../tests/fixtures/map-loop-owner-0300/distinct-owners.ko");
    let analysis = analyze(source);
    assert!(analysis.typed.diagnostics().is_empty());
    assert!(analysis.owned.diagnostics().is_empty());
    let program = lower_scalar_file(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
    )
    .unwrap();
    crate::llvm::render_verified_program(&program).unwrap();
    assert_map_drop_counts(&program, &[("parameterOwner", 1), ("separateOwners", 2)]);
    assert_unit_lowers(source);
}

#[test]
fn single_map_loop_resource_payload_keeps_existing_classification() {
    let source = "class Resource { deinit() { println(\"drop\") } }\nfun main(): Unit { var resources = mutableMapOf<Int, Resource>(); for (key in arrayOf(1, 2)) { resources.put(key, Resource()) }; println(\"after\") }";
    let analysis = analyze(source);
    assert!(analysis.typed.diagnostics().is_empty());
    assert!(analysis.owned.diagnostics().is_empty());
    let fact = analysis
        .owned
        .drops()
        .iter()
        .find(|fact| fact.condition().is_some())
        .unwrap();
    let DropTarget::Named(symbol) = fact.target() else {
        panic!("resource Map owner")
    };
    assert_eq!(
        analysis
            .typed
            .is_resource_type(analysis.typed.symbol_type(symbol).unwrap()),
        Some(true)
    );
    assert!(
        !matches!(fact.point(), DropPoint::LoopExit(_)),
        "Resource remains lexical"
    );
    let program = lower_scalar_file(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
    )
    .unwrap();
    crate::llvm::render_verified_program(&program).unwrap();
}

#[test]
fn single_map_loop_multiple_iteration_phi_seeds_remain_explicit() {
    for loops in [
        "for (key in arrayOf(1)) { cache.put(key, \"first\".clone()) }; for (key in arrayOf(2)) { cache.put(key, \"second\".clone()) }",
        "for (key in arrayOf(1)) { for (other in arrayOf(2)) { cache.put(other, \"nested\".clone()) }; cache.put(key, \"outer\".clone()) }",
    ] {
        let source = format!(
            "fun main(): Unit {{ var cache = mutableMapOf<Int, String>(); {loops}; cache.contains(1) }}"
        );
        assert_guarded_boundary(&source);
    }
}

fn assert_map_drop_counts(program: &crate::ssa::model::Program, expected: &[(&str, usize)]) {
    use crate::ssa::model::{EntityId, EntityType, Operation, SsaTypeKind};
    for (name, count) in expected {
        let (module, function) = program
            .modules
            .iter()
            .find_map(|module| {
                module
                    .functions
                    .iter()
                    .find(|function| function.name == *name)
                    .map(|function| (module, function))
            })
            .expect(name);
        let drops = function
            .instructions
            .iter()
            .filter(|instruction| match instruction.operation {
                Operation::Drop { owner } => {
                    match function.entity(EntityId::Value(owner)).unwrap().ty {
                        EntityType::Value(ty) => {
                            matches!(module.types[ty.index()], SsaTypeKind::MapContainer { .. })
                        }
                        _ => false,
                    }
                }
                _ => false,
            })
            .count();
        assert_eq!(
            drops, *count,
            "{name}: one terminal drop per actual Map owner"
        );
    }
}
