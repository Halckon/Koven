//! SPEC-0360: direct-tail whole-borrow cleanup and the adjacent rejected route.
//! Keep the original terminal-argument negative route and the per-family fact guard.
use super::*;
use lang_frontend::ownership_checking::UnitDropPoint;

#[test]
fn string_when_with_bounded_tail_temporary_borrow_refines() {
    const WHEN: &str = "when (s) { \"a\" -> 1; else -> 0 }";
    // This is the original route-negative source, unchanged.
    const TERMINAL: &str = "class Node()\nfun outer(node: Node, own r: Int): Int = r\n\
        fun f(flag: Boolean): Int = outer(Node(), when (flag) { true -> return 1\n else -> 0 })";
    let green = format!("fun f(s: String): Int = {WHEN}");
    let mixed = format!(
        "class Node()\nfun outer(node: Node, own r: Int): Int = r\n\
         fun f(s: String, flag: Boolean): Int {{ val n = {WHEN}\n\
         return outer(Node(), when (flag) {{ true -> return n\n else -> n }}) }}"
    );
    let with_runtime_field = mixed
        .replace("class Node()", "class Node(val value: Int)")
        .replace("outer(Node(),", "outer(Node(0),");
    let direct_local = mixed
        .replace("return outer(Node(),", "val result = outer(Node(),")
        .replace("else -> n }) }", "else -> n }); return result }");
    assert_ne!(direct_local, mixed);
    let mut comparison_counts = Vec::new();
    for (source, expected_refine, has_string_when) in [
        (green.as_str(), true, true),
        (TERMINAL, false, false),
        (mixed.as_str(), true, true),
        (with_runtime_field.as_str(), false, true),
        (direct_local.as_str(), true, true),
    ] {
        let mut sources = SourceMap::new();
        let (id, file) = parsed(&mut sources, "p/main.ko", &format!("package p\n{source}"));
        let inputs = [SourceUnitInput::new("root", "p/main.ko", id, &file)];
        let (ne, te) = standard_environments();
        // All three sources must pass names, types and ownership before observing routing.
        let (names, typed, owned) = analyze(&sources, &inputs, &ne, &te);
        let comparisons = owned
            .ownership()
            .drops()
            .iter()
            .filter(|fact| matches!(fact.point(), UnitDropPoint::WhenComparisonCompleted { .. }))
            .count();
        if has_string_when {
            assert!(
                comparisons > 0,
                "must publish real candidate cleanup facts: {source}"
            );
            comparison_counts.push(comparisons);
        } else {
            assert_eq!(
                comparisons, 0,
                "original terminal control has no String comparison"
            );
        }
        let entry = declaration(&names, "p", "f");
        let (result, routed) = observe_routing(|| {
            lower_scalar_unit_with_entry(&sources, &inputs, &names, &te, &typed, &owned, entry)
        });
        let f_refined = routed.iter().any(|name| name.starts_with("koven.p.f.d"));
        assert_eq!(f_refined, expected_refine, "{source}: {routed:?}");
        if expected_refine {
            let (program, _) = result.expect("accepted String when lowers");
            crate::llvm::render_verified_program(&program).expect("accepted String when verifies");
        }
        // The original terminal fixture constrains routing, not fallback success.
    }
    assert_eq!(comparison_counts.len(), 4);
    assert!(
        comparison_counts
            .iter()
            .all(|count| *count == comparison_counts[0]),
        "all route controls retain the same comparison fact count"
    );
}

// Observe actual branch transport as well as the concrete native allocator.
#[test]
fn bounded_tail_temporary_borrow_ends_then_drops_on_both_paths() {
    let source = r#"package p
class Node()
fun outer(node: Node, own r: Int): Int = r + 9
fun f(s: String, flag: Boolean): Int {
    val n = when (s) { "a" -> 1; else -> 0 }
    return outer(Node(), when (flag) { true -> return n
        else -> n })
}
fun main(): Unit {
    if (f("a", true) != 1) { error("abandoned call dispatched") }
    if (f("a", false) != 10) { error("normal call did not dispatch") }
    if (f("b", true) != 0) { error("unmatched early path lost prefix") }
    if (f("b", false) != 9) { error("unmatched normal path lost result") }
    println("ok")
}
"#;
    audit_bounded_native(source, "tail", NativeUnitLayout::Rooted);
}

const LOCAL_VAL_NATIVE: &str = r#"package p
class Node()
fun outer(node: Node, own r: Int): Int = r + 9
fun f(s: String, flag: Boolean): Int {
    val n = when (s) { "a" -> 1; else -> 0 }
    val result = outer(Node(), when (flag) { true -> return n
        else -> n })
    return result
}
fun main(): Unit {
    if (f("a", true) != 1) { error("matched early path lost prefix") }
    if (f("a", false) != 10) { error("matched normal path lost result") }
    if (f("b", true) != 0) { error("unmatched early path lost prefix") }
    if (f("b", false) != 9) { error("unmatched normal path lost result") }
    println("ok")
}
"#;

#[test]
fn direct_local_val_returns_exact_call_result_through_native_entry() {
    audit_local_val_in_both_unit_layouts(LOCAL_VAL_NATIVE, "val");
}

#[test]
fn grouped_local_val_returns_exact_call_result_through_native_entry() {
    audit_local_val_in_both_unit_layouts(&grouped_local_val_native_source(), "grouped val");
}

fn grouped_local_val_native_source() -> String {
    let source = LOCAL_VAL_NATIVE
        .replacen("val result = outer(", "val result = (outer(", 1)
        .replacen(
            "else -> n })\n    return result",
            "else -> n }))\n    return result",
            1,
        );
    assert!(source.contains("val result = (outer("));
    assert!(source.contains("else -> n }))\n    return result"));
    source
}

#[test]
fn explicit_prefix_block_preserves_grouped_local_val_native_cleanup() {
    let source = grouped_local_val_native_source()
        .replacen(
            "    val result =",
            "    var normal = 17\n    block { normal = n }\n    val result =",
            1,
        )
        .replacen("else -> n }))", "else -> normal }))", 1);
    assert!(source.contains("block { normal = n }"));
    assert!(source.contains("else -> normal }))"));
    // If the block's write is lost, the native normal branches see 17 rather
    // than the matched/unmatched value from the String when.
    audit_local_val_in_both_unit_layouts(&source, "explicit prefix block");
}

#[derive(Clone, Copy, Debug)]
enum NativeUnitLayout {
    MemoryDocument,
    Rooted,
}

fn audit_local_val_in_both_unit_layouts(source: &str, case: &str) {
    for layout in [NativeUnitLayout::MemoryDocument, NativeUnitLayout::Rooted] {
        audit_bounded_native(source, case, layout);
    }
}

fn audit_bounded_native(source: &str, case: &str, layout: NativeUnitLayout) {
    use lang_frontend::name_resolution::SourceUnitKey;
    let case = format!("{layout:?}/{case}");
    let mut sources = SourceMap::new();
    let (id, file) = parsed(&mut sources, "p/main.ko", source);
    assert_eq!(sources.source_text(id), Ok(source), "{case}");
    let inputs = [match layout {
        NativeUnitLayout::MemoryDocument => SourceUnitInput::memory_document(id, &file),
        NativeUnitLayout::Rooted => SourceUnitInput::new("root", "p/main.ko", id, &file),
    }];
    let (ne, te) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &ne, &te);
    let [document] = names.names().index().source_units() else {
        panic!("{case}: one unchanged source");
    };
    assert_eq!(document.source_id(), id, "{case}");
    match (layout, document.key()) {
        (NativeUnitLayout::MemoryDocument, SourceUnitKey::MemoryDocument) => {}
        (NativeUnitLayout::Rooted, SourceUnitKey::Rooted { root, logical_path }) => {
            assert_eq!(root.as_str(), "root", "{case}");
            assert_eq!(logical_path.as_str(), "p/main.ko", "{case}");
        }
        (_, key) => panic!("{case}: unexpected source key {key:?}"),
    }
    let entry = declaration(&names, "p", "main");
    let (result, routes) = observe_routing(|| {
        lower_scalar_unit_with_entry(&sources, &inputs, &names, &te, &typed, &owned, entry)
    });
    let (program, entry) = result.unwrap_or_else(|error| panic!("{case}: {error:?}"));
    crate::ssa::verify::verify_program(&program).expect("valid transported ownership");
    let module = &program.modules[0];
    let main = module.functions.iter().find(|f| f.id == entry).unwrap();
    assert!(main.name.starts_with("koven.p.main.d"));
    let called: Vec<_> = main
        .instructions
        .iter()
        .filter_map(|i| {
            let Operation::DirectCall { callee, .. } = i.operation else {
                return None;
            };
            module
                .functions
                .iter()
                .find(|f| f.id == callee && f.name.starts_with("koven.p.f.d"))
        })
        .collect();
    assert_eq!(
        called.len(),
        4,
        "{case}: all native cases must call the candidate"
    );
    let f = called[0];
    assert!(called.iter().all(|called| called.id == f.id));
    assert_eq!(
        routes.iter().filter(|name| **name == f.name).count(),
        1,
        "{case}: {routes:?}"
    );
    let outer = module
        .functions
        .iter()
        .find(|f| f.name.starts_with("koven.p.outer."))
        .unwrap();
    audit_tail_cleanup(f, outer.id, &sources);

    let llvm = crate::llvm::render_verified_program_with_entry(&program, entry)
        .expect("bounded call verifies in LLVM");
    let output = crate::native_tests::boxed_enum_tests::run_counted_allocations_in_order(
        &llvm,
        &[0, 1, 2, 3],
    );
    crate::native_tests::boxed_enum_tests::assert_success(&output, b"ok\n");
}

fn audit_tail_cleanup(
    f: &crate::ssa::model::Function,
    callee_id: crate::ssa::model::FunctionId,
    sources: &SourceMap,
) {
    use crate::ssa::model::{EntityId, LoanKind, TerminatorKind};
    use std::collections::BTreeMap;

    #[derive(Clone, Default)]
    struct State {
        aliases: BTreeMap<EntityId, EntityId>,
        owner: Option<EntityId>,
        place: Option<EntityId>,
        loan: Option<EntityId>,
        ended: bool,
        dropped: bool,
        dispatches: usize,
        call_result: Option<EntityId>,
    }
    let mut pending = vec![(f.blocks[0].id, State::default())];
    let (mut visits, mut early, mut normal) = (0, 0, 0);
    while let Some((id, mut state)) = pending.pop() {
        visits += 1;
        assert!(visits <= 128, "fixture must be a bounded acyclic graph");
        let block = f.blocks.iter().find(|b| b.id == id).unwrap();
        for id in &block.instructions {
            let i = f.instructions.iter().find(|i| i.id == *id).unwrap();
            let canonical = |entity| state.aliases.get(&entity).copied().unwrap_or(entity);
            match &i.operation {
                Operation::HeapAllocate { .. } => {
                    assert_eq!(sources.slice(i.origin.span()).unwrap(), "Node()");
                    assert!(state.owner.replace(i.results[0]).is_none());
                }
                Operation::RootPlace { owner }
                    if Some(canonical(EntityId::Value(*owner))) == state.owner =>
                {
                    assert!(state.place.replace(i.results[0]).is_none());
                }
                Operation::BorrowBegin { place, kind }
                    if Some(canonical(EntityId::Place(*place))) == state.place =>
                {
                    assert_eq!(*kind, LoanKind::Shared);
                    assert!(state.loan.replace(i.results[0]).is_none());
                }
                Operation::BorrowEnd { loan }
                    if Some(canonical(EntityId::Loan(*loan))) == state.loan =>
                {
                    assert!(!state.ended && !state.dropped);
                    state.ended = true;
                }
                Operation::Drop { owner }
                    if Some(canonical(EntityId::Value(*owner))) == state.owner =>
                {
                    assert!(
                        state.ended && !state.dropped,
                        "same owner drops once after End"
                    );
                    state.dropped = true;
                }
                Operation::DirectCall {
                    callee, arguments, ..
                } if *callee == callee_id => {
                    assert_eq!(Some(canonical(arguments[0])), state.loan);
                    assert!(state.loan.is_some() && !state.ended && !state.dropped);
                    state.dispatches += 1;
                    let [result] = i.results[..] else {
                        panic!("Int call has one result")
                    };
                    assert!(state.call_result.replace(result).is_none());
                }
                _ => {}
            }
        }
        let term = block.terminator.as_ref().unwrap();
        let edges = match &term.kind {
            TerminatorKind::Return { values } => {
                assert_eq!(values.len(), 1);
                assert!(
                    state.owner.is_some() && state.loan.is_some() && state.ended && state.dropped
                );
                match sources.slice(term.origin.span()).unwrap().trim() {
                    "return n" => {
                        assert_eq!(state.dispatches, 0);
                        early += 1;
                    }
                    outer if outer == "return result" || outer.starts_with("return outer(") => {
                        assert_eq!(state.dispatches, 1);
                        let returned = EntityId::Value(values[0]);

                        let returned = state.aliases.get(&returned).copied().unwrap_or(returned);
                        assert_eq!(Some(returned), state.call_result);
                        normal += 1;
                    }
                    other => panic!("unexpected Return source: {other:?}"),
                }
                continue;
            }
            TerminatorKind::Branch(edge) => vec![edge],
            TerminatorKind::Conditional {
                when_true,
                when_false,
                ..
            } => vec![when_true, when_false],
            other => panic!("unexpected terminator: {other:?}"),
        };
        for edge in edges {
            let target = f.blocks.iter().find(|b| b.id == edge.target).unwrap();
            assert_eq!(target.parameters.len(), edge.arguments.len());
            let mut next = state.clone();
            for (&param, &arg) in target.parameters.iter().zip(&edge.arguments) {
                next.aliases
                    .insert(param, state.aliases.get(&arg).copied().unwrap_or(arg));
            }
            pending.push((edge.target, next));
        }
    }
    assert!(
        early > 0 && normal > 0,
        "both cleanup branches must be emitted"
    );
}

#[test]
fn direct_initializer_and_nested_initializer_keep_separate_pending_frames() {
    use lang_frontend::{
        name_resolution::UnitReferenceTarget,
        ownership_checking::{LoanKind, UnitDropTarget, UnitLoanTarget},
        parser::{Item, NameMarker, VariableKind},
        type_checking::{
            BuiltinType, CallableResultSource, UnitCallTarget, UnitExpressionId, UnitTypeKind,
        },
    };
    const HEADER: &str = r#"package p
class Node()
fun outer(node: Node, own r: Int): Int = r + 9
fun wrap(node: Node, own r: Int): Int = r + 17
fun f(s: String, flag: Boolean): Int {
    val n = when (s) { "a" -> 1; else -> 0 }
"#;
    const INNER: &str = "outer(Node(), when (flag) { true -> return n\n else -> n })";
    for (initializer, expected_pending) in
        [(INNER.to_owned(), 1), (format!("wrap(Node(), {INNER})"), 2)]
    {
        let text = format!("{HEADER}    val result = {initializer}\n    return result\n}}\n");
        let mut sources = SourceMap::new();
        let (id, file) = parsed(&mut sources, "p/main.ko", &text);
        let inputs = [SourceUnitInput::new("root", "p/main.ko", id, &file)];
        let (ne, te) = standard_environments();
        let (names, typed, owned) = analyze(&sources, &inputs, &ne, &te);

        // Only the direct initializer with one frame is admitted by production.
        let entry = declaration(&names, "p", "f");
        let (result, routes) = observe_routing(|| {
            lower_scalar_unit_with_entry(&sources, &inputs, &names, &te, &typed, &owned, entry)
        });
        let nested = expected_pending == 2;
        assert_eq!(
            routes.iter().any(|n| n.starts_with("koven.p.f.d")),
            !nested,
            "{routes:?}"
        );

        let span = |id: UnitExpressionId| {
            file.ast()
                .expressions()
                .get(id.expression())
                .unwrap()
                .span()
        };
        // Absence from the observer alone would also allow a failed Refine. Pin the
        // actual fallback failure at its String comparison, not at the local binding.
        if nested {
            let Err(error) = result else {
                panic!("nested initializer has no successful compatibility lowering");
            };
            assert_eq!(
                error.kind,
                crate::ssa::lowering_support::LoweringErrorKind::UnsupportedNode
            );
            assert_eq!(
                sources.slice(error.span.unwrap()).unwrap(),
                r#"when (s) { "a" -> 1; else -> 0 }"#,
            );
        } else {
            result.expect("direct local initializer must lower");
        }

        let function_span = file
            .ast()
            .items()
            .iter()
            .find(|(_, item)| sources.slice(item.span()).unwrap().starts_with("fun f("))
            .expect("fixture entry")
            .1
            .span();
        // Keep the public-route assertion above independent of this test-only
        // probe. Inspect the existing builder, not the admitted route or emitter.
        crate::ssa::refine::initializer_probe::run(function_span, nested, || {
            let _ =
                lower_scalar_unit_with_entry(&sources, &inputs, &names, &te, &typed, &owned, entry);
        });

        let source = |id| sources.slice(span(id)).unwrap();
        let (name, init) = file
            .ast()
            .items()
            .iter()
            .find_map(|(_, item)| match *item.payload() {
                Item::Variable {
                    kind: VariableKind::Val,
                    name: NameMarker::Present(name),
                    initializer,
                    ..
                } if sources.slice(name).unwrap() == "result" => Some((name, initializer)),
                _ => None,
            })
            .expect("actual immutable initializer");
        let unit = names.names().source_units()[0].source_unit();
        let init = UnitExpressionId::new(unit, init);
        assert_eq!(source(init), initializer);
        let typed = typed.types();
        let call = typed.call(init).expect("no initializer adapter");
        let callee = if expected_pending == 1 {
            "outer"
        } else {
            "wrap"
        };
        assert_eq!(
            call.target(),
            UnitCallTarget::Declaration(declaration(&names, "p", callee))
        );
        assert!(call.receiver().is_none());
        assert_eq!(call.result_source(), CallableResultSource::Owned);
        assert_eq!(
            typed.types().get(call.return_type()),
            Some(&UnitTypeKind::Builtin(BuiltinType::Int)),
        );
        let distinct_expression = |text| {
            let found: Vec<_> = typed
                .expression_types()
                .keys()
                .copied()
                .filter(|&id| source(id) == text)
                .collect();
            let [id] = found[..] else {
                panic!("one typed expression {text:?}: {found:?}")
            };
            id
        };
        assert_eq!(typed.expression_falls_through(init), Some(true));
        assert_eq!(
            typed.expression_falls_through(distinct_expression("return n")),
            Some(false),
        );
        let use_id = distinct_expression("result");
        let use_span = span(use_id);
        let reference = names
            .names()
            .references()
            .iter()
            .find(|refer| refer.source_unit() == unit && refer.span() == use_span)
            .expect("exact return name reference");
        let UnitReferenceTarget::Symbol(symbol) = *reference.target() else {
            panic!("return must reference a local symbol");
        };
        assert_eq!(symbol.source_unit(), unit);
        assert_eq!(
            names.names().source_units()[0].resolution().symbols()[symbol.symbol().index()].span(),
            name
        );
        assert_eq!(typed.symbol_type(symbol), Some(call.return_type()));
        assert_eq!(typed.expression_type(use_id), Some(call.return_type()));

        let facts = owned.ownership();
        let loans: Vec<_> = facts
            .loans()
            .iter()
            .filter(|loan| {
                matches!(
                    source(loan.call()).split('(').next(),
                    Some("outer" | "wrap")
                )
            })
            .collect();
        assert_eq!(loans.len(), expected_pending);
        let early: Vec<_> = facts.direct_call_loan_ends().iter().filter(|end| {
            matches!(end.point(), UnitDropPoint::ControlTransfer(at) if source(at) == "return n")
        }).collect();
        assert_eq!(early.len(), expected_pending);
        let inner = loans
            .iter()
            .copied()
            .find(|loan| source(loan.call()) == INNER)
            .unwrap();
        for &loan in &loans {
            let UnitLoanTarget::Temporary(owner) = *loan.target() else {
                panic!("expected an actual whole temporary loan");
            };
            assert_eq!(loan.kind(), LoanKind::Shared);
            assert_eq!(owner, loan.argument());
            assert_eq!(source(owner), "Node()");
            let ends: Vec<_> = early
                .iter()
                .copied()
                .filter(|end| end.loan() == loan)
                .collect();
            let [end] = ends.as_slice() else {
                panic!("one exact early End per loan");
            };
            let normal: Vec<_> = facts
                .direct_call_loan_ends()
                .iter()
                .filter(|other| {
                    other.point() == UnitDropPoint::CallReturn(loan.call()) && other.loan() == loan
                })
                .collect();
            let [normal] = normal.as_slice() else {
                panic!("one exact normal End per loan");
            };
            assert_eq!(
                facts
                    .drops()
                    .iter()
                    .filter(|drop| { drop.target() == UnitDropTarget::Temporary(owner) })
                    .count(),
                2,
                "normal outer Return must not also retire the temporary"
            );
            for end in [*end, *normal] {
                let positions: Vec<_> = facts
                    .drops()
                    .iter()
                    .filter(|drop| drop.point() == end.point())
                    .enumerate()
                    .filter(|(_, drop)| drop.target() == UnitDropTarget::Temporary(owner))
                    .collect();
                let [(index, drop)] = positions.as_slice() else {
                    panic!("one exact Drop per owner");
                };
                assert_eq!(drop.value_origin(), span(owner));
                assert!(!drop.is_conditional_temporary());
                assert!(end.preceding_drops() <= *index);
            }
            if loan != inner {
                assert!(
                    !facts.drops().iter().any(|drop| {
                        drop.point() == UnitDropPoint::CallReturn(inner.call())
                            && drop.target() == UnitDropTarget::Temporary(owner)
                    }),
                    "normal inner return cannot drop the wrapper's earlier temporary",
                );
            }
        }
    }
}

#[test]
fn grouped_terminal_initializer_keeps_exact_inner_call_cleanup() {
    use lang_frontend::{
        name_resolution::UnitReferenceTarget,
        ownership_checking::{LoanKind, UnitDropTarget, UnitLoanTarget},
        parser::{Expression, Item, NameMarker, VariableKind},
        type_checking::{BuiltinType, CallableResultSource, UnitExpressionId, UnitTypeKind},
    };
    const PREFIX: &str = r#"package p
class Node()
fun outer(node: Node, own r: Int): Int = r + 9
fun f(s: String, flag: Boolean): Int {
    val n = when (s) { "a" -> 1; else -> 0 }
"#;
    const CALL: &str = "outer(Node(), when (flag) { true -> return n\n else -> n })";
    for (initializer, grouped) in [(CALL.to_owned(), false), (format!("({CALL})"), true)] {
        let text = format!("{PREFIX}    val result = {initializer}\n    return result\n}}\n");
        let mut sources = SourceMap::new();
        let (id, file) = parsed(&mut sources, "p/main.ko", &text);
        let inputs = [SourceUnitInput::new("root", "p/main.ko", id, &file)];
        let (ne, te) = standard_environments();
        let (names, typed, owned) = analyze(&sources, &inputs, &ne, &te);
        let unit = names.names().source_units()[0].source_unit();
        let (binder, initializer_id) = file
            .ast()
            .items()
            .iter()
            .find_map(|(_, item)| match *item.payload() {
                Item::Variable {
                    kind: VariableKind::Val,
                    name: NameMarker::Present(name),
                    initializer,
                    ..
                } if sources.slice(name).unwrap() == "result" => Some((name, initializer)),
                _ => None,
            })
            .expect("exact immutable binder");
        let expressions = file.ast().expressions();
        let inner = match expressions.get(initializer_id).unwrap().payload() {
            Expression::Group { expression } if grouped => *expression,
            Expression::Call { .. } if !grouped => initializer_id,
            other => panic!("unexpected initializer: {other:?}"),
        };
        assert!(matches!(
            expressions.get(inner).unwrap().payload(),
            Expression::Call { .. }
        ));
        let span = |id: UnitExpressionId| expressions.get(id.expression()).unwrap().span();
        let source = |id| sources.slice(span(id)).unwrap();
        let init = UnitExpressionId::new(unit, initializer_id);
        let inner = UnitExpressionId::new(unit, inner);
        assert_eq!(source(init), initializer);
        assert_eq!(source(inner), CALL);
        let types = typed.types();
        let descriptor = types.call(inner).expect("direct inner call");
        assert_eq!(descriptor.result_source(), CallableResultSource::Owned);
        assert_eq!(
            types.types().get(descriptor.return_type()),
            Some(&UnitTypeKind::Builtin(BuiltinType::Int))
        );
        if grouped {
            assert!(types.call(init).is_none(), "Group must not own call facts");
        }
        for id in [init, inner] {
            assert_eq!(types.expression_type(id), Some(descriptor.return_type()));
            assert_eq!(types.expression_falls_through(id), Some(true));
        }
        let named = |text| {
            let ids: Vec<_> = types
                .expression_types()
                .keys()
                .copied()
                .filter(|&id| source(id) == text)
                .collect();
            let [id] = ids[..] else {
                panic!("one exact expression {text:?}: {ids:?}")
            };
            id
        };
        let returned = named("result");
        let reference = names
            .names()
            .references()
            .iter()
            .find(|r| r.source_unit() == unit && r.span() == span(returned))
            .expect("exact return reference");
        let UnitReferenceTarget::Symbol(symbol) = *reference.target() else {
            panic!("return must read a symbol");
        };
        assert_eq!(symbol.source_unit(), unit);
        assert_eq!(
            names.names().source_units()[0].resolution().symbols()[symbol.symbol().index()].span(),
            binder
        );
        assert_eq!(types.symbol_type(symbol), Some(descriptor.return_type()));
        assert_eq!(
            types.expression_type(returned),
            Some(descriptor.return_type())
        );

        let facts = owned.ownership();
        let owner = named("Node()");
        let transfer = named("return n");
        assert_eq!(types.expression_falls_through(transfer), Some(false));
        let loans: Vec<_> = facts
            .loans()
            .iter()
            .filter(|loan| loan.call() == inner)
            .collect();
        let [loan] = loans[..] else {
            panic!("one inner loan: {loans:?}")
        };
        assert_eq!(loan.argument(), owner);
        assert_eq!(loan.kind(), LoanKind::Shared);
        assert_eq!(loan.target(), &UnitLoanTarget::Temporary(owner));

        let points = [
            UnitDropPoint::CallReturn(inner),
            UnitDropPoint::ControlTransfer(transfer),
        ];
        for point in points {
            let ends: Vec<_> = facts
                .direct_call_loan_ends()
                .iter()
                .filter(|end| end.point() == point)
                .collect();
            let drops: Vec<_> = facts
                .drops()
                .iter()
                .filter(|drop| drop.point() == point)
                .collect();
            let ([end], [drop]) = (ends.as_slice(), drops.as_slice()) else {
                panic!("one cleanup pair at {point:?}: {ends:?} / {drops:?}");
            };
            assert_eq!(end.loan(), loan);
            assert_eq!(end.preceding_call_loan_ends(), 0);
            assert_eq!(end.preceding_drops(), 0, "end precedes drop");
            assert_eq!(drop.target(), UnitDropTarget::Temporary(owner));
            assert_eq!(drop.value_origin(), span(owner));
            assert!(!drop.is_conditional_temporary());
        }
        assert!(
            facts
                .drops()
                .iter()
                .filter(|drop| drop.target() == UnitDropTarget::Temporary(owner))
                .all(|drop| points.contains(&drop.point()))
        );

        let entry = declaration(&names, "p", "f");
        let (result, routes) = observe_routing(|| {
            lower_scalar_unit_with_entry(&sources, &inputs, &names, &te, &typed, &owned, entry)
        });
        assert!(
            routes.iter().any(|name| name.starts_with("koven.p.f.d")),
            "{initializer}: {routes:?}",
        );
        let (program, _) = result.expect("both admitted initializers must lower");
        crate::ssa::verify::verify_program(&program).expect("initializer must verify");
        crate::llvm::render_verified_program(&program).expect("initializer LLVM");
    }
}

#[test]
fn terminal_call_group_position_matrix_keeps_exact_cleanup_and_route_boundary() {
    use lang_frontend::{
        name_resolution::SourceUnitKey,
        ownership_checking::{LoanKind, UnitDropTarget, UnitLoanTarget},
        parser::Expression,
        type_checking::{BuiltinType, CallableResultSource, UnitExpressionId, UnitTypeKind},
    };
    const PREFIX: &str = r#"package p
class Node()
fun outer(node: Node, own r: Int): Int = r + 9
fun f(s: String, flag: Boolean): Int {
    val n = when (s) { "a" -> 1; else -> 0 }
"#;
    const CALL: &str = "outer(Node(), when (flag) { true -> return n\n else -> n })";
    for ((shape, body, refined), rooted) in [
        ("bare tail", format!("return {CALL}"), true),
        ("grouped tail", format!("return ({CALL})"), false),
        (
            "bare initializer",
            format!("val result = {CALL}\n    return result"),
            true,
        ),
        (
            "single-group initializer",
            format!("val result = ({CALL})\n    return result"),
            true,
        ),
        (
            "grouped pair inside explicit block",
            format!("block {{ val result = ({CALL})\n        return result }}"),
            false,
        ),
        (
            "double-group initializer",
            format!("val result = (({CALL}))\n    return result"),
            false,
        ),
        (
            "grouped binder use",
            format!("val result = ({CALL})\n    return (result)"),
            false,
        ),
        (
            "mutable",
            format!("var result = {CALL}\n    return result"),
            false,
        ),
        (
            "different symbol",
            format!("val result = {CALL}\n    return n"),
            false,
        ),
        (
            "intervening declaration",
            format!("val result = {CALL}\n    val extra = 3\n    return result"),
            false,
        ),
    ]
    .into_iter()
    .flat_map(|shape| {
        [false, true]
            .into_iter()
            .map(move |rooted| (shape.clone(), rooted))
    }) {
        let layout = if rooted { "Rooted" } else { "MemoryDocument" };
        let shape = format!("{layout}/{shape}");
        let text = format!("{PREFIX}    {body}\n}}\n");
        let mut sources = SourceMap::new();
        let (id, file) = parsed(&mut sources, "p/main.ko", &text);
        assert_eq!(sources.source_text(id), Ok(text.as_str()));
        let inputs = [if rooted {
            SourceUnitInput::new("root", "p/main.ko", id, &file)
        } else {
            SourceUnitInput::memory_document(id, &file)
        }];
        let (ne, te) = standard_environments();
        let (names, typed, owned) = analyze(&sources, &inputs, &ne, &te);
        let [document] = names.names().index().source_units() else {
            panic!("{shape}: expected one unchanged source");
        };
        assert_eq!(document.source_id(), id, "{shape}");
        assert_eq!(
            matches!(document.key(), SourceUnitKey::MemoryDocument),
            !rooted,
            "{shape}"
        );
        let types = typed.types();
        let span = |expr: UnitExpressionId| {
            file.ast()
                .expressions()
                .get(expr.expression())
                .unwrap()
                .span()
        };
        let distinct = |source: &str, inside: Option<UnitExpressionId>| -> UnitExpressionId {
            let mut matches = types.expression_types().keys().copied().filter(|&expr| {
                let expr_span = span(expr);
                sources.slice(expr_span).unwrap() == source
                    && inside.is_none_or(|parent| {
                        let container = span(parent);
                        container.start() <= expr_span.start() && expr_span.end() <= container.end()
                    })
            });
            let expression = matches.next().expect("original typed source");
            assert!(matches.next().is_none(), "{shape}: ambiguous {source}");
            expression
        };
        let lender = distinct(CALL, None);
        let transfer = distinct("return n", Some(lender));
        let owner = distinct("Node()", Some(lender));
        let descriptor = types.call(lender).expect("original call");
        assert_eq!(descriptor.result_source(), CallableResultSource::Owned);
        let result_type = descriptor.return_type();
        assert_eq!(
            types.types().get(result_type),
            Some(&UnitTypeKind::Builtin(BuiltinType::Int))
        );
        assert_eq!(types.expression_falls_through(lender), Some(true));
        assert_eq!(types.expression_falls_through(transfer), Some(false));

        for group_source in [
            format!("({CALL})"),
            format!("(({CALL}))"),
            "(result)".to_owned(),
        ] {
            if !body.contains(&group_source) {
                continue;
            }
            let group = distinct(group_source.as_str(), None);
            assert!(matches!(
                file.ast()
                    .expressions()
                    .get(group.expression())
                    .unwrap()
                    .payload(),
                Expression::Group { .. }
            ));
            assert_eq!(types.expression_type(group), Some(result_type), "{shape}");
            assert_eq!(types.expression_falls_through(group), Some(true), "{shape}");
            assert!(types.call(group).is_none(), "{shape}: no wrapper call fact");
        }

        let facts = owned.ownership();
        let mut loans = facts.loans().iter().filter(|loan| loan.call() == lender);
        let loan = loans.next().expect("original shared loan");
        assert!(loans.next().is_none());
        assert_eq!(loan.argument(), owner);
        assert_eq!(loan.kind(), LoanKind::Shared);
        assert_eq!(loan.target(), &UnitLoanTarget::Temporary(owner));
        let normal = UnitDropPoint::CallReturn(lender);
        let early = UnitDropPoint::ControlTransfer(transfer);
        let ends = facts
            .direct_call_loan_ends()
            .iter()
            .filter(|end| end.loan() == loan)
            .collect::<Vec<_>>();
        let [first, second] = ends.as_slice() else {
            panic!("{shape}: {ends:?}")
        };
        assert_ne!(first.point(), second.point(), "{shape}");
        for end in ends {
            assert!([normal, early].contains(&end.point()), "{shape}");
            assert_eq!(end.preceding_call_loan_ends(), 0, "{shape}");
            assert_eq!(end.preceding_drops(), 0, "{shape}");
        }
        let drops = facts
            .drops()
            .iter()
            .filter(|drop| drop.target() == UnitDropTarget::Temporary(owner))
            .collect::<Vec<_>>();
        let [first, second] = drops.as_slice() else {
            panic!("{shape}: {drops:?}")
        };
        assert_ne!(first.point(), second.point(), "{shape}");
        for drop in drops {
            assert!([normal, early].contains(&drop.point()), "{shape}");
            assert_eq!(drop.value_origin(), span(owner), "{shape}");
            assert!(!drop.is_conditional_temporary(), "{shape}");
        }
        let entry = declaration(&names, "p", "f");
        let (result, routes) = observe_routing(|| {
            lower_scalar_unit_with_entry(&sources, &inputs, &names, &te, &typed, &owned, entry)
        });
        assert_eq!(
            routes.iter().any(|n| n.starts_with("koven.p.f.d")),
            refined,
            "{shape}: {routes:?}"
        );
        if refined {
            let (program, _) = result.unwrap_or_else(|error| panic!("{shape}: {error:?}"));
            crate::ssa::verify::verify_program(&program).expect("admitted SSA verifies");
            crate::llvm::render_verified_program(&program).expect("admitted LLVM verifies");
            continue;
        }
        let Err(error) = result else {
            panic!("{shape}: boundary must not successfully lower")
        };
        assert_eq!(
            error.kind,
            crate::ssa::lowering_support::LoweringErrorKind::UnsupportedNode,
            "{shape}"
        );
        assert_eq!(
            sources.slice(error.span.unwrap()).unwrap(),
            r#"when (s) { "a" -> 1; else -> 0 }"#,
            "{shape}: actual rejection remains at the shared fallback boundary",
        );
    }
}
