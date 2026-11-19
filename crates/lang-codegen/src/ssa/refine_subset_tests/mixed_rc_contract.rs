//! Existing-capability regression based on b314's CI-verified nested Rc native control.
//! The actual f body refines with a real String comparison prefix; this does not fix the Node boundary.
use super::*;
use lang_frontend::ownership_checking::UnitDropPoint;

#[test]
fn mixed_string_when_rc_argument_return_refines_and_preserves_both_cleanup_edges() {
    let source = r#"package p
class Leaf(val name: String) { deinit() { println("leaf") } }
class Holder(val inner: Rc<Leaf>) { deinit() { println("holder") } }
fun show(borrow leaf: Leaf, ready: Boolean): Unit { if (ready) { println(leaf.name) } }
fun f(s: String, stop: Boolean): Unit {
    val ready = when (s) { "go" -> true; else -> false }
    show(Rc(Rc(Leaf("payload".clone()))).value.value,
        if (stop) { println("return"); return } else { ready })
    println("tail")
}
fun share(): Unit {
    val retained = Rc(Holder(Rc(Leaf("kept".clone())))).value.inner.share()
    println(retained.value.name)
    println("held")
}
fun entry(): Unit { f("go", true); f("go", false); share(); println("done") }
"#;
    let mut sources = SourceMap::new();
    let (id, file) = parsed(&mut sources, "p/main.ko", source);
    let inputs = [SourceUnitInput::new("root", "p/main.ko", id, &file)];
    let (ne, te) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &ne, &te);
    assert!(
        owned
            .ownership()
            .drops()
            .iter()
            .any(|fact| matches!(fact.point(), UnitDropPoint::WhenComparisonCompleted { .. })),
        "the added prefix must publish actual String comparison cleanup"
    );
    let entry = declaration(&names, "p", "entry");
    let (result, routed) = observe_routing(|| {
        lower_scalar_unit_with_entry(&sources, &inputs, &names, &te, &typed, &owned, entry)
    });
    let (program, entry) = result.expect("mixed source requires actual successful lowering");
    // A positive success record, not an inference from absence or another routed function.
    assert!(
        routed.iter().any(|name| name.starts_with("koven.p.f.d")),
        "actual f must successfully refine: {routed:?}"
    );
    crate::ssa::verify::verify_program(&program)
        .expect("all CFG edges must preserve loan/owner identities");
    let function = program.modules[0]
        .functions
        .iter()
        .find(|f| f.name.starts_with("koven.p.f."))
        .expect("actual f");
    audit_return_paths(function, &program.modules[0], &sources);
    let llvm = crate::llvm::render_verified_program_with_entry(&program, entry)
        .expect("mixed LLVM verifies");
    // Literal String selectors add no heap allocation. Keep the exact known-control
    // allocation/free identity sequence and stdout: early exit must not print payload/tail.
    let output = crate::native_tests::boxed_enum_tests::run_counted_allocations_in_order(
        &llvm,
        &[0, 1, 2, 3, 4, 5, 6, 7, 11, 12, 8, 9, 10],
    );
    crate::native_tests::boxed_enum_tests::assert_success(
        &output,
        b"return\nleaf\npayload\nleaf\ntail\nholder\nkept\nheld\nleaf\ndone\n",
    );
}

/// Enumerate every predecessor history; canonical identities come only from actual
/// edge operands, never source spelling, block numbers, or global instruction order.
fn audit_return_paths(
    function: &crate::ssa::model::Function,
    module: &crate::ssa::model::Module,
    sources: &SourceMap,
) {
    use crate::ssa::model::{BlockId, EntityId, TerminatorKind};
    use std::collections::{BTreeMap, BTreeSet};
    #[derive(Clone, Default)]
    struct State {
        aliases: BTreeMap<EntityId, EntityId>,
        allocations: BTreeSet<EntityId>,
        places: BTreeMap<EntityId, EntityId>,
        loans: BTreeMap<EntityId, (EntityId, Option<EntityId>)>,
        active: BTreeSet<EntityId>,
        ended: BTreeMap<EntityId, BlockId>,
        drops: BTreeMap<EntityId, usize>,
        dispatches: usize,
    }
    impl State {
        fn canonical(&self, entity: EntityId) -> EntityId {
            self.aliases.get(&entity).copied().unwrap_or(entity)
        }
    }
    let first = function.blocks.first().expect("entry block").id;
    let mut stack = vec![(first, Vec::new(), State::default())];
    let (mut early, mut normal, mut visited_paths) = (0, 0, 0);
    while let Some((id, mut path, mut state)) = stack.pop() {
        visited_paths += 1;
        assert!(
            visited_paths <= 256,
            "bounded acyclic fixture only; never truncate paths"
        );
        assert!(
            !path.contains(&id),
            "cycles require a separate traversal contract"
        );
        path.push(id);
        let block = function
            .blocks
            .iter()
            .find(|b| b.id == id)
            .expect("actual block");
        for instruction in &block.instructions {
            let i = function
                .instructions
                .iter()
                .find(|i| i.id == *instruction)
                .unwrap();
            match i.operation {
                Operation::SharedAllocate { .. } => {
                    state.allocations.insert(i.results[0]);
                }
                Operation::SharedPayloadPlace { owner } => {
                    let owner = state.canonical(owner);
                    assert!(
                        state.allocations.contains(&owner),
                        "payload belongs to an actual fresh shared allocation"
                    );
                    state.places.insert(i.results[0], owner);
                }
                Operation::BorrowBegin { place, .. } => {
                    let place = state.canonical(EntityId::Place(place));
                    if let Some(&owner) = state.places.get(&place) {
                        let loan = i.results[0];
                        assert!(state.active.insert(loan));
                        state.loans.insert(loan, (owner, None));
                    }
                }
                Operation::SharedPayloadLoan { source } => {
                    let parent = state.canonical(EntityId::Loan(source));
                    let &(owner, _) = state
                        .loans
                        .get(&parent)
                        .expect("derived payload uses tracked parent loan");
                    assert!(state.active.contains(&parent));
                    let child = i.results[0];
                    assert!(state.active.insert(child));
                    state.loans.insert(child, (owner, Some(parent)));
                }
                Operation::BorrowEnd { loan } => {
                    let loan = state.canonical(EntityId::Loan(loan));
                    if state.loans.contains_key(&loan) {
                        assert!(
                            !state
                                .loans
                                .iter()
                                .any(|(child, (_, parent))| *parent == Some(loan)
                                    && state.active.contains(child)),
                            "derived loan must end before its parent"
                        );
                        assert!(state.active.remove(&loan), "tracked loan ends exactly once");
                        assert!(state.ended.insert(loan, block.id).is_none());
                    }
                }
                Operation::Drop { owner } => {
                    let owner = state.canonical(EntityId::Value(owner));
                    let related = state
                        .loans
                        .iter()
                        .filter(|(_, (root, _))| *root == owner)
                        .map(|(loan, _)| *loan)
                        .collect::<Vec<_>>();
                    if !related.is_empty() {
                        assert_eq!(
                            related.len(),
                            2,
                            "both nested payload loans belong to the pending outer Rc"
                        );
                        for loan in related {
                            assert!(
                                !state.active.contains(&loan),
                                "same-owner active loan forbids Drop"
                            );
                            assert_eq!(
                                state.ended.get(&loan),
                                Some(&block.id),
                                "retain strict same-cleanup-block end-before-drop oracle"
                            );
                        }
                        let count = state.drops.entry(owner).or_default();
                        *count += 1;
                        assert_eq!(*count, 1, "same fresh owner drops once per full path");
                    }
                }
                Operation::DirectCall {
                    callee,
                    ref arguments,
                    ..
                }
                | Operation::BorrowCall {
                    callee,
                    ref arguments,
                    ..
                } => {
                    if module
                        .functions
                        .iter()
                        .any(|f| f.id == callee && f.name.starts_with("koven.p.show."))
                    {
                        state.dispatches += 1;
                        let borrowed = state.canonical(arguments[0]);
                        assert!(
                            state.active.contains(&borrowed),
                            "show receives the tracked live derived payload loan"
                        );
                        assert!(
                            state.loans[&borrowed].1.is_some(),
                            "show receives child loan, not parent handle"
                        );
                    }
                }
                _ => {}
            }
        }
        let term = block.terminator.as_ref().expect("terminated block");
        if let TerminatorKind::Return { values } = &term.kind {
            assert!(values.is_empty(), "actual Unit Return");
            assert!(
                state.active.is_empty(),
                "no pending Rc loan escapes either Return"
            );
            assert_eq!(
                state.drops.len(),
                1,
                "exactly one tracked fresh outer Rc owner on each complete path"
            );
            let actual = sources
                .slice(term.origin.span())
                .expect("actual Return source")
                .trim();
            if actual == "return" {
                early += 1;
                assert_eq!(
                    state.dispatches, 0,
                    "no show on any predecessor path to explicit Return"
                );
            } else {
                normal += 1;
                assert_eq!(state.dispatches, 1, "normal path dispatches exactly once");
            }
        } else {
            let edges = match &term.kind {
                TerminatorKind::Branch(edge) => vec![edge],
                TerminatorKind::Conditional {
                    when_true,
                    when_false,
                    ..
                } => vec![when_true, when_false],
                _ => panic!("outside this bounded fixture CFG contract"),
            };
            for edge in edges {
                let target = function
                    .blocks
                    .iter()
                    .find(|b| b.id == edge.target)
                    .unwrap();
                assert_eq!(target.parameters.len(), edge.arguments.len());
                let bindings = target
                    .parameters
                    .iter()
                    .copied()
                    .zip(edge.arguments.iter().map(|arg| state.canonical(*arg)))
                    .collect::<Vec<_>>();
                let mut next = state.clone();
                for (parameter, argument) in bindings {
                    next.aliases.insert(parameter, argument);
                }
                stack.push((edge.target, path.clone(), next));
            }
        }
    }
    assert!(
        early > 0 && normal > 0,
        "both actual Return kinds reachable"
    );
}
