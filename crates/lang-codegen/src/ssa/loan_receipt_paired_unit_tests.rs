//! Ordinary paired receiver admission must keep equal local call IDs in different sources separate.
use crate::ssa::loan_region_ssa_tests::*;
use lang_frontend::ownership_checking::{
    CompilationUnitOwnership, UnitDropPoint, UnitDropTarget, UnitReceiverOwnershipTarget,
};
use lang_frontend::type_checking::UnitExpressionId;

#[test]
fn paired_receiver_facts_stay_source_qualified_in_a_multi_source_unit() {
    const BODY: &str = "
class Node { fun touch(): Int = 1 }
fun run(own node: Node): Unit { node.touch(); node.touch() }
fun launch(): Unit { run(Node()) }";
    check_receiver_sequence::<2>(BODY);
}

#[test]
fn three_shared_receiver_calls_keep_three_loans_and_one_terminal_drop() {
    const BODY: &str = "
class Node { fun touch(): Int = 1; fun other(): Int = 2 }
fun run(own node: Node): Unit { node.touch(); node.other(); node.touch() }
fun launch(): Unit { run(Node()) }";
    check_receiver_sequence::<3>(BODY);
}

#[test]
fn grouped_shared_receivers_preserve_the_exact_source_qualified_owner() {
    const BODY: &str = "
class Node { fun touch(): Int = 1; fun other(): Int = 2 }
fun run(own node: Node): Unit { (node).touch(); ((node)).other(); node.touch() }
fun launch(): Unit { run(Node()) }";
    check_receiver_sequence::<3>(BODY);
}

fn check_receiver_sequence<const N: usize>(body: &str) {
    let mut sources = SourceMap::new();
    let (left, left_file) =
        unit_lower_test_support::parsed(&mut sources, "p/run.ko", &format!("package p{body}"));
    let (right, right_file) =
        unit_lower_test_support::parsed(&mut sources, "q/run.ko", &format!("package q{body}"));
    let (entry_source, entry_file) = unit_lower_test_support::parsed(
        &mut sources,
        "r/entry.ko",
        "package r\nimport p.launch as first\nimport q.launch as second\nfun entry(): Unit { first(); second() }",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/run.ko", left, &left_file),
        SourceUnitInput::new("root", "q/run.ko", right, &right_file),
        SourceUnitInput::new("root", "r/entry.ko", entry_source, &entry_file),
    ];
    let (environment, types) = standard_environments();
    let mut first_output = None;
    for inputs in [inputs, [inputs[2], inputs[1], inputs[0]]] {
        let (names, typed, owned) =
            unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
        let entry = unit_lower_test_support::declaration(&names, "r", "entry");
        let facts = owned.ownership();
        let left_calls = receiver_calls::<N>(facts, left);
        let right_calls = receiver_calls::<N>(facts, right);
        for (a, b) in left_calls.into_iter().zip(right_calls) {
            assert_eq!(
                a.expression(),
                b.expression(),
                "fixtures must collide locally"
            );
            assert_ne!(a.source_unit(), b.source_unit());
        }
        let ((lowered, routes), receipts) = crate::ssa::refine::receiver_receipt::observe(|| {
            crate::ssa::refine::observe_routing(|| {
                crate::ssa::unit_lower::lower_scalar_unit_with_entry(
                    &sources, &inputs, &names, &types, &typed, &owned, entry,
                )
            })
        });
        let (program, entry) = lowered.expect("ordinary multi-source Unit lowering");
        let [module] = program.modules.as_slice() else {
            panic!("one Unit module");
        };
        for (source, calls) in [(left, left_calls), (right, right_calls)] {
            let mut callers = module.functions.iter().filter(|f| {
                let span = f.origin.span();
                span.source_id() == source && sources.slice(span).unwrap().starts_with("fun run(")
            });
            let caller = callers.next().expect("original run emitted");
            assert!(callers.next().is_none());
            assert!(routes.contains(&caller.name), "actual route: {routes:?}");
            assert!(receipts.iter().all(|receipt| receipt.caller != caller.id()));
            assert_lifecycle(&sources, facts, calls, caller, module);
        }
        verify_program(&program).unwrap();
        let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
        let rendered = crate::ssa::render_program(&program);
        if let Some((first_ssa, first_llvm)) = &first_output {
            assert_eq!(
                first_ssa, &rendered,
                "input order must not change the emitted SSA"
            );
            assert_eq!(
                first_llvm, &llvm,
                "input order must not change the emitted LLVM"
            );
        } else {
            first_output = Some((rendered, llvm));
        }
    }
    let (_, llvm) = first_output.unwrap();
    let output =
        crate::native_tests::boxed_enum_tests::run_counted_allocations_in_order(&llvm, &[0, 1]);
    crate::native_tests::boxed_enum_tests::assert_success(&output, b"");
}

fn receiver_calls<const N: usize>(
    owned: &CompilationUnitOwnership,
    source: SourceId,
) -> [UnitExpressionId; N] {
    let mut facts = owned
        .receiver_facts()
        .iter()
        .filter(|fact| fact.begin_span().source_id() == source)
        .collect::<Vec<_>>();
    facts.sort_by_key(|fact| fact.begin_span().start());
    let facts: [&_; N] = facts
        .try_into()
        .unwrap_or_else(|facts| panic!("expected {N} receiver facts in {source:?}, got {facts:?}"));
    let first = facts[0];
    let UnitReceiverOwnershipTarget::Place(place) = first.target() else {
        panic!("receiver sequence must use a place");
    };
    assert!(place.is_root());
    assert_eq!(place.root().source_unit(), first.call().source_unit());
    for fact in facts {
        assert_eq!(first.target(), fact.target());
        assert_eq!(first.call().source_unit(), fact.call().source_unit());
        assert_eq!(owned.receiver_fact(fact.call()), Some(fact));
        assert_eq!(
            owned
                .receiver_call_return_ends()
                .iter()
                .filter(|end| end.receiver() == fact
                    && end.point() == UnitDropPoint::CallReturn(fact.call()))
                .count(),
            1,
        );
    }
    let drops = owned
        .drops()
        .iter()
        .filter(|drop| drop.target() == UnitDropTarget::Named(place.root()))
        .collect::<Vec<_>>();
    let [drop] = drops.as_slice() else {
        panic!("one original, source-qualified owner drop");
    };
    assert_eq!(drop.point(), UnitDropPoint::CallReturn(facts[N - 1].call()));
    facts.map(|fact| fact.call())
}

fn assert_lifecycle<const N: usize>(
    sources: &SourceMap,
    owned: &CompilationUnitOwnership,
    calls: [UnitExpressionId; N],
    caller: &Function,
    module: &Module,
) {
    let [block] = caller.blocks.as_slice() else {
        panic!("one ordinary caller block");
    };
    let [EntityId::Value(owner)] = block.parameters.as_slice() else {
        panic!("one original owner");
    };
    let operations = block
        .instructions
        .iter()
        .map(|id| caller.instruction(*id).unwrap());
    let mut loans = Vec::new();
    let mut root = None;
    let mut next = 0;
    let mut closed = 0;
    let mut drops = 0;
    for instruction in operations {
        match instruction.operation {
            Operation::RootPlace { owner: actual } => {
                assert_eq!(actual, *owner);
                let [EntityId::Place(place)] = instruction.results.as_slice() else {
                    panic!("one owner place");
                };
                root = Some(*place);
            }
            Operation::BorrowBegin {
                place,
                kind: LoanKind::Shared,
            } => {
                assert_eq!(root, Some(place));
                assert_eq!(next, closed);
                assert_eq!(loans.len(), next);
                assert_eq!(
                    instruction.origin.span(),
                    owned.receiver_fact(calls[next]).unwrap().begin_span(),
                    "the loan begins at the actual receiver occurrence, including any groups"
                );
                let [EntityId::Loan(loan)] = instruction.results.as_slice() else {
                    panic!("one receiver loan");
                };
                assert!(!loans.contains(loan));
                loans.push(*loan);
            }
            Operation::DirectCall {
                callee,
                receiver: Some(EntityId::Loan(loan)),
                ref arguments,
            } => {
                assert_eq!(loans.get(next), Some(&loan));
                assert!(arguments.is_empty());
                let fact = owned
                    .receiver_fact(calls[next])
                    .expect("this caller's real fact");
                assert_eq!(instruction.origin.span(), fact.end_span());
                let dispatched = module.function(callee).unwrap().origin.span();
                assert_eq!(dispatched.source_id(), fact.end_span().source_id());
                let member = sources
                    .slice(fact.end_span())
                    .unwrap()
                    .rsplit_once('.')
                    .unwrap()
                    .1
                    .strip_suffix("()")
                    .unwrap();
                assert!(
                    sources
                        .slice(dispatched)
                        .unwrap()
                        .starts_with(&format!("fun {member}("))
                );
                next += 1;
            }
            Operation::BorrowEnd { loan } => {
                assert_eq!(next, closed + 1);
                assert_eq!(loans.get(closed), Some(&loan));
                closed += 1;
            }
            Operation::Drop { owner: actual } => {
                assert_eq!(actual, *owner);
                assert_eq!(closed, calls.len());
                drops += 1;
            }
            _ => {}
        }
    }
    assert_eq!((loans.len(), next, closed, drops), (N, N, N, 1));
}
