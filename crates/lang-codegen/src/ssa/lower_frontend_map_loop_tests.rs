//! SPEC-0300: real source owner facts and normal-exhaustion SSA transport.
use super::lower_map_unit;
use crate::ssa::map_unit_consumer_test_support::{assert_source, assert_trusted_range};
#[path = "lower_frontend_map_loop_unit_identity.rs"]
pub(in crate::ssa) mod unit_identity;
mod unit_iteration {
    //! Same-analysis Unit facts for the seven original conditional-exit consumers.
    //! The private Legacy phi graph is replaced by exact SSA incoming-column checks,
    //! not by fabricated compatibility facts or same-type owner equivalence.
    use crate::native_tests::boxed_enum_tests::unit_source::UnitSource;
    use lang_frontend::{
        name_resolution::{SourceUnitKey, SymbolKind, UnitReferenceTarget, UnitSymbolId},
        ownership_checking::{
            OwnershipBindingKind, UnitDropPoint, UnitDropTarget,
            UnitIterationCleanupAction as Action, UnitIterationExitKind as Exit,
            UnitIterationSourceAccess, UnitLoanTarget,
        },
        parser::{Expression, Statement},
        source::Span,
        type_checking::{
            IterationProvider, SequentialContainerKind, UnitExpressionId, UnitStatementId,
        },
    };

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(super) enum Control {
        Break,
        Continue,
        Return,
    }

    pub(super) struct Origins {
        pub(super) statement: Span,
        pub(super) control: Span,
        pub(super) source: Span,
    }

    fn expression_span(unit: &UnitSource, expression: UnitExpressionId) -> Span {
        let [source] = unit.names().names().index().source_units() else {
            panic!("one original memory document")
        };
        assert_eq!(expression.source_unit(), source.id());
        unit.parsed()
            .ast()
            .expressions()
            .get(expression.expression())
            .unwrap()
            .span()
    }

    fn reference_root(unit: &UnitSource, expression: UnitExpressionId) -> UnitSymbolId {
        let span = expression_span(unit, expression);
        assert!(matches!(
            unit.parsed()
                .ast()
                .expressions()
                .get(expression.expression())
                .unwrap()
                .payload(),
            Expression::Name
        ));
        let references = unit
            .names()
            .names()
            .references()
            .iter()
            .filter(|reference| {
                reference.source_unit() == expression.source_unit() && reference.span() == span
            })
            .collect::<Vec<_>>();
        let [reference] = references.as_slice() else {
            panic!("one actual Map receiver reference")
        };
        let UnitReferenceTarget::Symbol(root) = reference.target() else {
            panic!("Map receiver resolves to its original named owner")
        };
        *root
    }

    pub(super) fn assert_facts(unit: &UnitSource, control: Control, live: bool) -> Origins {
        unit.assert_frontend_clean();
        let [input] = unit.names().names().index().source_units() else {
            panic!("one original memory document")
        };
        assert_eq!(input.key(), &SourceUnitKey::MemoryDocument);
        assert_eq!(input.source_id(), unit.parsed().source_id());
        let [plan] = unit.ownership().iterations() else {
            panic!("one actual sequential provider")
        };
        let descriptor = plan.descriptor();
        let statement = descriptor.statement();
        assert_eq!(statement.source_unit(), input.id());
        assert_eq!(descriptor.source().source_unit(), input.id());
        assert_eq!(
            unit.types().sequential_iteration(statement),
            Some(descriptor)
        );
        assert_eq!(
            descriptor.provider(),
            IterationProvider::Sequential(SequentialContainerKind::Array)
        );
        assert_eq!(plan.source_access(), UnitIterationSourceAccess::Temporary);
        assert_eq!(
            plan.source(),
            Some(&UnitLoanTarget::Temporary(descriptor.source()))
        );
        let node = unit
            .parsed()
            .ast()
            .statements()
            .get(statement.statement())
            .unwrap();
        let Statement::For { source, body, .. } = node.payload() else {
            panic!("the published descriptor names the original for")
        };
        assert_eq!(descriptor.source().expression(), *source);
        let source_span = expression_span(unit, descriptor.source());
        let main_span = unit.declaration_span(unit.declaration("main", SymbolKind::Function));
        assert_eq!(node.span().source_id(), main_span.source_id());
        assert!(main_span.start() <= node.span().start() && node.span().end() <= main_span.end());
        let [construction] = unit.types().container_constructions() else {
            panic!("one original array source construction")
        };
        assert_eq!(construction.expression(), descriptor.source());
        assert_eq!(construction.container_type(), descriptor.source_type());
        assert_eq!(
            plan.bindings()
                .iter()
                .map(|binding| binding.symbol())
                .collect::<Vec<_>>(),
            descriptor.binding().symbols().collect::<Vec<_>>()
        );
        assert!(plan.bindings().iter().all(|binding| {
            binding.kind() == OwnershipBindingKind::Shared
                && binding.symbol().source_unit() == input.id()
        }));

        let controls = unit
            .parsed()
            .ast()
            .expressions()
            .iter()
            .filter_map(|(id, node)| {
                let matches = matches!(
                    (control, node.payload()),
                    (Control::Break, Expression::Break { .. })
                        | (Control::Continue, Expression::Continue { .. })
                        | (Control::Return, Expression::Return { value: None, .. })
                );
                matches.then_some(UnitExpressionId::new(input.id(), id))
            })
            .collect::<Vec<_>>();
        let [expression] = controls.as_slice() else {
            panic!("one original explicit control transfer")
        };
        let explicit = match control {
            Control::Break => Exit::Break(*expression),
            Control::Continue => Exit::Continue(*expression),
            Control::Return => Exit::Return(*expression),
        };
        let control_span = expression_span(unit, *expression);
        assert!(
            node.span().start() < control_span.start() && control_span.end() < node.span().end()
        );
        assert_eq!(plan.exits().len(), 3);
        let body = UnitStatementId::new(input.id(), *body);
        for (kind, point) in [
            (explicit, UnitDropPoint::ControlTransfer(*expression)),
            (Exit::Fallthrough, UnitDropPoint::AfterStatement(body)),
            (Exit::Exhaustion, UnitDropPoint::LoopExit(statement)),
        ] {
            let exits = plan
                .exits()
                .iter()
                .filter(|exit| exit.kind() == kind)
                .collect::<Vec<_>>();
            let [exit] = exits.as_slice() else {
                panic!("one exact explicit, fallthrough, and exhaustion plan")
            };
            assert_eq!(exit.point(), point);
            assert_eq!(
                unit.ownership().iteration_cleanup_at(point),
                Some(exit.actions())
            );
            let finishes =
                kind == Exit::Exhaustion || matches!(kind, Exit::Break(_) | Exit::Return(_));
            let mut expected = Vec::new();
            if kind != Exit::Exhaustion {
                expected.extend(
                    plan.bindings()
                        .iter()
                        .rev()
                        .map(|binding| Action::EndBinding {
                            statement,
                            symbol: binding.symbol(),
                        }),
                );
                expected.push(Action::EndElement(statement));
            }
            if finishes {
                expected.extend([
                    Action::FinishProvider(statement),
                    Action::EndSource(statement),
                ]);
            }
            let actions = exit
                .actions()
                .iter()
                .filter(|action| !matches!(action, Action::Drop(_)))
                .cloned()
                .collect::<Vec<_>>();
            assert_eq!(actions, expected);
            let source_drops = exit
                .actions()
                .iter()
                .filter_map(|action| match action {
                    Action::Drop(fact)
                        if fact.target() == UnitDropTarget::Temporary(descriptor.source()) =>
                    {
                        Some(*fact)
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(source_drops.len(), usize::from(finishes));
            if finishes {
                let fact = source_drops[0];
                assert_eq!(fact.point(), point);
                assert_eq!(fact.value_origin(), source_span);
                assert!(unit.ownership().drops().contains(&fact));
                let source_end = exit
                    .actions()
                    .iter()
                    .position(|action| *action == Action::EndSource(statement))
                    .unwrap();
                let source_drop = exit
                    .actions()
                    .iter()
                    .position(|action| *action == Action::Drop(fact))
                    .unwrap();
                assert!(source_end < source_drop);
            }
        }
        let source_drops = unit
            .ownership()
            .drops()
            .iter()
            .filter(|fact| fact.target() == UnitDropTarget::Temporary(descriptor.source()))
            .collect::<Vec<_>>();
        assert_eq!(
            source_drops.len(),
            if control == Control::Continue { 1 } else { 2 }
        );

        // The receiver's real name-resolution identity, not the Map type or spelling,
        // binds every published terminal Drop to the owner whose SSA columns follow.
        let puts = unit.types().map_puts();
        assert_eq!(puts.len(), if control == Control::Break { 1 } else { 2 });
        let map_root = reference_root(unit, puts[0].receiver());
        assert!(
            puts.iter()
                .all(|put| reference_root(unit, put.receiver()) == map_root)
        );
        let map_drops = unit
            .ownership()
            .drops()
            .iter()
            .filter(|fact| fact.target() == UnitDropTarget::Named(map_root))
            .collect::<Vec<_>>();
        let mut expected_points = Vec::new();
        if control == Control::Return || (control == Control::Break && !live) {
            expected_points.push(UnitDropPoint::ControlTransfer(*expression));
        }
        if live {
            let [query] = unit.types().map_contains_calls() else {
                panic!("one original post-loop use of the same Map")
            };
            assert_eq!(reference_root(unit, query.receiver()), map_root);
            assert!(expression_span(unit, query.expression()).start() > node.span().end());
            expected_points.push(UnitDropPoint::CallReturn(query.expression()));
        } else {
            assert!(unit.types().map_contains_calls().is_empty());
            expected_points.push(UnitDropPoint::LoopExit(statement));
        }
        assert_eq!(map_drops.len(), expected_points.len());
        for point in expected_points {
            assert_eq!(
                map_drops
                    .iter()
                    .filter(|fact| fact.point() == point)
                    .count(),
                1
            );
        }
        Origins {
            statement: node.span(),
            control: control_span,
            source: source_span,
        }
    }
}

const ORIGINAL: &str =
    include_str!("../../tests/fixtures/owned-null-cleanup-0296/single-loop-boundary.ko");
const NORMAL: &str = include_str!("../../tests/fixtures/map-loop-owner-0300/normal-exhaustion.ko");

#[test]
fn single_map_loop_original_remove_uses_published_header_owner() {
    unit_identity::check_original(ORIGINAL);
}

#[test]
fn unit_map_loop_original_source_is_an_independent_control() {
    assert_unit_lowers(ORIGINAL);
}

#[test]
fn single_map_loop_normal_exhaustion_preserves_dead_and_live_owners() {
    unit_identity::check_normal(NORMAL);
}

#[test]
fn unit_map_loop_normal_source_is_an_independent_control() {
    assert_unit_lowers(NORMAL);
}

#[test]
fn single_map_loop_abort_guard_combination_remains_explicit() {
    let source = include_str!("../../tests/fixtures/map-loop-owner-0300/guarded-abort-boundary.ko");
    assert_source(
        source,
        &["removeDead", "removeLive", "putDead", "putLive", "main"],
    );
}

fn assert_control_exit_boundary(exit: &str) {
    let source = format!(
        "fun main(): Unit {{ var cache = mutableMapOf<Int, String>(); for (key in arrayOf(1, 2)) {{ cache.put(key, \"value\".clone()); if (key == 1) {{ {exit} }} }}; cache.contains(1) }}"
    );
    assert_source(&source, &["main"]);
}

#[test]
fn single_map_continue_with_break_remains_explicit() {
    assert_control_exit_boundary("if (key == 2) { break }; continue");
}

#[test]
fn single_map_continue_unconditional_and_multiple_remain_explicit() {
    for exit in [
        "continue",
        "if (key == 1) { continue }; if (key == 2) { continue }",
    ] {
        let source = format!(
            "fun main(): Unit {{ var cache = mutableMapOf<Int, String>(); for (key in arrayOf(1, 2)) {{ cache.put(key, \"value\".clone()); {exit} }}; cache.contains(1) }}"
        );
        assert_source(&source, &["main"]);
    }
}

#[test]
fn single_map_continue_return_replacement_and_abort_remain_explicit() {
    assert_control_exit_boundary("if (key == 2) { return }; continue");
    for prefix in [
        "cache = mutableMapOf<Int, String>();",
        "if (key == 2) { error(\"stop\") };",
    ] {
        let source = format!(
            "fun main(): Unit {{ var cache = mutableMapOf<Int, String>(); for (key in arrayOf(1, 2)) {{ {prefix} cache.put(key, \"value\".clone()); if (key == 1) {{ continue }} }}; cache.contains(1) }}"
        );
        assert_source(&source, &["main"]);
    }
}

#[test]
fn single_map_loop_unconditional_return_remains_explicit() {
    assert_source(
        "fun main(): Unit { var cache = mutableMapOf<Int, String>(); for (key in arrayOf(1, 2)) { cache.put(key, \"value\".clone()); return }; cache.contains(1) }",
        &["main"],
    );
}

#[test]
fn single_map_loop_replaced_seed_remains_explicit() {
    let source = "fun main(): Unit { var cache = mutableMapOf<Int, String>(); for (key in arrayOf(1, 2)) { cache = mutableMapOf<Int, String>(); cache.put(key, \"value\".clone()) }; cache.contains(1) }";
    assert_source(source, &["main"]);
}

#[test]
fn single_map_loop_returned_map_seed_remains_explicit() {
    assert_source(
        "fun make(): MutableMap<Int, String> = mutableMapOf<Int, String>()\nfun main(): Unit { var cache = make(); for (key in arrayOf(1)) { cache.put(key, \"value\".clone()) }; cache.contains(1) }",
        &["make", "main"],
    );
}

#[test]
fn single_map_loop_range_provider_remains_explicit() {
    let text = format!(
        "{}\nfun main(): Unit {{ var cache = mutableMapOf<Int, String>(); val keys = listOf(1, 2); for (key in take(keys, 1)) {{ cache.put(key, \"value\".clone()) }}; cache.contains(1) }}",
        include_str!("../../../lang-std/koven/algorithms/ranges.ko")
            .strip_prefix("package koven.algorithms\n")
            .unwrap()
    );
    assert_trusted_range(&text);
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
    let (program, analysis) = lower_map_unit(source, "main");
    assert_unit_map_drop_counts(
        &program,
        &analysis,
        &[("parameterOwner", 1), ("separateOwners", 2)],
    );
    assert_unit_lowers(source);
}

#[test]
fn single_map_loop_resource_payload_keeps_existing_classification() {
    let source = "class Resource { deinit() { println(\"drop\") } }\nfun main(): Unit { var resources = mutableMapOf<Int, Resource>(); for (key in arrayOf(1, 2)) { resources.put(key, Resource()) }; println(\"after\") }";
    unit_identity::check_resource(source);
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
        assert_source(&source, &["main"]);
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
            "{name}: expected static Map drops for the selected exits"
        );
    }
}

fn assert_unit_map_drop_counts(
    program: &crate::ssa::model::Program,
    analysis: &crate::native_tests::boxed_enum_tests::unit_source::UnitSource,
    expected: &[(&str, usize)],
) {
    use crate::ssa::model::Origin;
    use lang_frontend::name_resolution::SymbolKind;

    let expected = expected
        .iter()
        .map(|(name, count)| {
            let declaration = analysis.declaration(name, SymbolKind::Function);
            let span = analysis.declaration_span(declaration);
            let functions = program
                .modules
                .iter()
                .flat_map(|module| &module.functions)
                .filter(|function| function.origin == Origin::Source(span))
                .collect::<Vec<_>>();
            let [function] = functions.as_slice() else {
                panic!("one original source function for {name}");
            };
            (function.name.as_str(), *count)
        })
        .collect::<Vec<_>>();
    assert_map_drop_counts(program, &expected);
}

#[path = "lower_frontend_map_break_tests.rs"]
pub(in crate::ssa) mod conditional_break;

#[path = "lower_frontend_map_continue_tests.rs"]
pub(in crate::ssa) mod conditional_continue;

#[path = "lower_frontend_map_return_tests.rs"]
pub(in crate::ssa) mod conditional_return;
