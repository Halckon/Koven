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
    let non_tail = mixed
        .replace("return outer(Node(),", "val result = outer(Node(),")
        .replace("else -> n }) }", "else -> n }); return result }");
    assert_ne!(non_tail, mixed);
    let mut comparison_counts = Vec::new();
    for (source, expected_refine, has_string_when) in [
        (green.as_str(), true, true),
        (TERMINAL, false, false),
        (mixed.as_str(), true, true),
        (with_runtime_field.as_str(), false, true),
        (non_tail.as_str(), false, true),
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
    println("ok")
}
"#;
    let mut sources = SourceMap::new();
    let (id, file) = parsed(&mut sources, "p/main.ko", source);
    let inputs = [SourceUnitInput::new("root", "p/main.ko", id, &file)];
    let (ne, te) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &ne, &te);
    let entry = declaration(&names, "p", "main");
    let (result, routes) = observe_routing(|| {
        lower_scalar_unit_with_entry(&sources, &inputs, &names, &te, &typed, &owned, entry)
    });
    assert!(
        routes.iter().any(|n| n.starts_with("koven.p.f.d")),
        "{routes:?}"
    );
    let (program, entry) = result.expect("exact bounded call lowers");
    crate::ssa::verify::verify_program(&program).expect("valid transported ownership");
    let module = &program.modules[0];
    let f = module
        .functions
        .iter()
        .find(|f| f.name.starts_with("koven.p.f."))
        .unwrap();
    let outer = module
        .functions
        .iter()
        .find(|f| f.name.starts_with("koven.p.outer."))
        .unwrap();
    audit_tail_cleanup(f, outer.id, &sources);

    let llvm = crate::llvm::render_verified_program_with_entry(&program, entry)
        .expect("bounded call verifies in LLVM");
    let output =
        crate::native_tests::boxed_enum_tests::run_counted_allocations_in_order(&llvm, &[0, 1]);
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
                    outer if outer.starts_with("return outer(") => {
                        assert_eq!(state.dispatches, 1);
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
