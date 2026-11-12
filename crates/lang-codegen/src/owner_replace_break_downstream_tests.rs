//! Ordinary original B source through existing lowering, not a new carrier consumer.
mod owner_loan_receipt_sibling_tests {
//! Ordinary sibling callables only; no fault injection, native execution or counters.
use super::{OwnedProducts, SOURCE, UnitSource, assert_selected_owner};
use crate::ssa::{model::Operation, verify::verify_program};
use lang_frontend::ownership_checking::{
    UnitDropTarget, UnitIterationCleanupAction, UnitLoanAction, UnitLoanActionCoverage,
    UnitOwnerReplaceBreakConsumerActionV1,
};

#[test]
fn bounded_owner_and_protocol4_siblings_keep_receipts_and_drops_separate() {
    let text = format!(
        "{}\nfun view(source: String): borrow String from source = source\nfun consume(item: String): Unit {{}}\nfun loan(): Unit {{ val owner = \"kept\"; consume(view(owner)) }}",
        SOURCE.replace("run(false); run(true)", "run(false); loan(); run(true)")
    );
    for constant in [false, true] {
        let text = if constant {
            format!(
                "const val ON: Boolean = true\n{}",
                text.replace("run(true)", "run(ON)")
            )
        } else {
            text.clone()
        };
        let source = UnitSource::analyze("owner-loan-siblings.ko", &text).unwrap();
        source.assert_frontend_clean();
        assert_eq!(
            matches!(source.products, OwnedProducts::Constant(..)),
            constant
        );
        let owned = match &source.products {
            OwnedProducts::Basic(_, owned) => owned.ownership(),
            OwnedProducts::Constant(_, owned) => owned.ownership(),
        };
        let boundaries = owned
            .owner_observations()
            .iter()
            .filter_map(|o| o.replace_break_observation_v1())
            .collect::<Vec<_>>();
        let [boundary] = boundaries.as_slice() else {
            panic!("one B product")
        };
        let UnitOwnerReplaceBreakConsumerActionV1::Drop(point, retired) =
            boundary.suffix_consumer_v1().unwrap().actions()[2]
        else {
            panic!("B suffix Drop")
        };
        let owner_index = owned
            .drops()
            .iter()
            .position(|d| d.point() == point && d.target() == UnitDropTarget::Named(retired))
            .unwrap();
        let regions = owned
            .loan_action_observations()
            .iter()
            .flat_map(|o| {
                o.regions()
                    .iter()
                    .filter(|r| r.protocol_version() == 4)
                    .map(move |r| (o, r))
            })
            .collect::<Vec<_>>();
        let [(observation, region)] = regions.as_slice() else {
            panic!("one P4 product")
        };
        assert_eq!(region.coverage(), UnitLoanActionCoverage::Complete);
        let actions = region
            .action_ids()
            .map(|id| observation.action(&id).unwrap())
            .collect::<Vec<_>>();
        let drops = actions
            .iter()
            .filter_map(|a| match a {
                UnitLoanAction::Cleanup {
                    action: UnitIterationCleanupAction::Drop(drop),
                    ..
                } => Some(drop),
                _ => None,
            })
            .collect::<Vec<_>>();
        let [loan_drop] = drops.as_slice() else {
            panic!("one P4 cleanup")
        };
        let loan_index = owned
            .drops()
            .iter()
            .position(|d| d.point() == loan_drop.point() && d.target() == loan_drop.target())
            .unwrap();
        assert_ne!(owner_index, loan_index);
        assert_ne!(UnitDropTarget::Named(retired), loan_drop.target());
        let ((lowered, physical), receipts) = crate::ssa::observe_loan_receipts(|| {
            crate::ssa::observe_owner_drops(|| source.lower_entry(source.entry()))
        });
        let (program, entry) = lowered.unwrap();
        verify_program(&program).unwrap();
        crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
        let owner_receipts = physical
            .iter()
            .filter(|d| d.3.is_some())
            .collect::<Vec<_>>();
        let [owner_receipt] = owner_receipts.as_slice() else {
            panic!("one B receipt")
        };
        assert_eq!(
            (owner_receipt.0, owner_receipt.1),
            (UnitDropTarget::Named(retired), owner_index)
        );
        let (selected, bound, index) = owner_receipt.3.unwrap();
        assert_ne!(selected, bound);
        assert_eq!((bound, index), (owner_receipt.2, owner_index));
        let loan_events = physical
            .iter()
            .filter(|d| d.1 == loan_index)
            .collect::<Vec<_>>();
        let [loan_event] = loan_events.as_slice() else {
            panic!("one physical P4 Drop")
        };
        assert_eq!(loan_event.0, loan_drop.target());
        assert!(
            loan_event.3.is_none(),
            "P4 Drop does not borrow a B authorization"
        );
        let [receipt] = receipts.as_slice() else {
            panic!("one P4 receipt")
        };
        assert_eq!(Some(receipt.callable), region.callable());
        assert_eq!(receipt.call, region.call());
        assert_eq!(receipt.ids, region.action_ids().collect::<Vec<_>>());
        let functions = program
            .modules
            .iter()
            .flat_map(|m| &m.functions)
            .collect::<Vec<_>>();
        let b = functions
            .iter()
            .find(|f| {
                source
                    .sources
                    .slice(f.origin.span())
                    .is_ok_and(|s| s.starts_with("fun run("))
            })
            .unwrap();
        assert_selected_owner(&source, b);
        let loan = functions.iter().find(|f| f.id() == receipt.caller).unwrap();
        assert!(
            source
                .sources
                .slice(loan.origin.span())
                .unwrap()
                .starts_with("fun loan(")
        );
        assert_ne!(b.id(), loan.id());
        assert_eq!(receipt.instructions.len(), 7);
        assert_eq!(receipt.entities.len(), 4);
        let actual_drops = loan
            .instructions
            .iter()
            .filter(|i| matches!(i.operation, Operation::Drop { .. }))
            .collect::<Vec<_>>();
        let [actual_drop] = actual_drops.as_slice() else {
            panic!("one P4 SSA Drop")
        };
        assert_eq!(receipt.instructions.last(), Some(&actual_drop.id));
        assert_eq!(actual_drop.operation.entities(), [receipt.entities[0]]);
    }
}
}
use super::{OwnedProducts, UnitSource};
use crate::ssa::{
    model::{Edge, EntityId, Function, Operation, TerminatorKind, ValueId},
    verify::verify_program,
};
use lang_frontend::name_resolution::SymbolKind;

pub(super) const SOURCE: &str = r#"fun run(flag: Boolean) { var held = "outer"; while(flag) { val previous = replace(&held, "next"); break }; val retired = held }
fun entry(): Unit { run(false); run(true) }"#;

#[test]
fn bounded_replace_break_original_source_reaches_existing_ssa_and_llvm() {
    let source = UnitSource::analyze("bounded-owner-b.ko", SOURCE).expect("ordinary B source");
    source.assert_frontend_clean();
    let OwnedProducts::Basic(_, owned) = &source.products else {
        panic!("basic Unit products")
    };
    let boundaries = owned
        .ownership()
        .owner_observations()
        .iter()
        .filter_map(|o| o.replace_break_observation_v1())
        .collect::<Vec<_>>();
    assert_eq!(boundaries.len(), 1);
    assert!(boundaries[0].suffix_consumer_v1().is_some());
    let (program, entry) = source
        .lower_entry(source.declaration("entry", SymbolKind::Function))
        .expect("existing Unit lowering reaches original B source");
    verify_program(&program).expect("original source SSA verifies");
    let functions = program
        .modules
        .iter()
        .flat_map(|module| &module.functions)
        .filter(|function| {
            source
                .sources
                .slice(function.origin.span())
                .is_ok_and(|text| text.starts_with("fun run("))
        })
        .collect::<Vec<_>>();
    let [function] = functions.as_slice() else {
        panic!("exact original callable")
    };
    assert_selected_owner(&source, function);
    let llvm = crate::llvm::render_verified_program_with_entry(&program, entry)
        .expect("original source LLVM verifies");
    assert!(llvm.contains("define i32 @main("));
}

fn value(entity: EntityId) -> ValueId {
    let EntityId::Value(value) = entity else {
        panic!("owned SSA value")
    };
    value
}

fn forward(function: &Function, edge: &Edge, owner: ValueId) -> ValueId {
    assert_eq!(edge.arguments, [EntityId::Value(owner)]);
    let parameters = &function.block(edge.target).unwrap().parameters;
    assert_eq!(parameters.len(), 1);
    value(parameters[0])
}

fn branch(function: &Function, block: crate::ssa::model::BlockId) -> &Edge {
    let TerminatorKind::Branch(edge) = &function
        .block(block)
        .unwrap()
        .terminator
        .as_ref()
        .unwrap()
        .kind
    else {
        panic!("unconditional exit edge")
    };
    edge
}

fn assert_selected_owner(source: &UnitSource, function: &Function) {
    let literal = |bytes: &[u8]| {
        let found = function.instructions.iter().filter(|instruction|
            matches!(&instruction.operation, Operation::StringLiteral { bytes: actual, .. } if actual == bytes)
        ).collect::<Vec<_>>();
        assert_eq!(found.len(), 1);
        found[0]
    };
    let initial = literal(b"outer");
    let replacement = literal(b"next");
    assert_eq!(source.sources.slice(initial.origin.span()), Ok("\"outer\""));
    assert_eq!(
        source.sources.slice(replacement.origin.span()),
        Ok("\"next\"")
    );
    let conditionals = function
        .blocks
        .iter()
        .filter_map(|block| match &block.terminator.as_ref()?.kind {
            TerminatorKind::Conditional {
                when_true,
                when_false,
                ..
            } => Some((when_true, when_false)),
            _ => None,
        })
        .collect::<Vec<_>>();
    let [(when_true, when_false)] = conditionals.as_slice() else {
        panic!("one actual Boolean condition")
    };
    let body_owner = forward(function, when_true, value(initial.results[0]));
    let false_owner = forward(function, when_false, value(initial.results[0]));
    let exchanges = function
        .instructions
        .iter()
        .filter(|instruction| matches!(instruction.operation, Operation::RootReplace { .. }))
        .collect::<Vec<_>>();
    let [exchange] = exchanges.as_slice() else {
        panic!("one actual Replace")
    };
    assert_eq!(
        source.sources.slice(exchange.origin.span()),
        Ok("replace(&held, \"next\")")
    );
    let Operation::RootReplace {
        owner,
        replacement: rhs,
        ..
    } = exchange.operation
    else {
        unreachable!()
    };
    assert_eq!(owner, body_owner);
    assert_eq!(rhs, value(replacement.results[0]));
    assert_eq!(exchange.block, when_true.target);
    assert_eq!(exchange.results.len(), 2);
    let break_edge = branch(function, exchange.block);
    let false_edge = branch(function, when_false.target);
    assert_eq!(break_edge.target, false_edge.target);
    let selected = forward(function, break_edge, value(exchange.results[0]));
    assert_eq!(forward(function, false_edge, false_owner), selected);
    assert_ne!(break_edge.arguments, false_edge.arguments);
    let drops = function
        .instructions
        .iter()
        .filter_map(|instruction| {
            if let Operation::Drop { owner } = instruction.operation {
                Some((instruction, owner))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(
        drops.len(),
        2,
        "one detached old value and one selected exit value"
    );
    let old = drops
        .iter()
        .find(|(_, owner)| *owner == value(exchange.results[1]))
        .unwrap()
        .0;
    let final_drop = drops
        .iter()
        .find(|(_, owner)| *owner == selected)
        .unwrap()
        .0;
    assert_eq!(old.block, exchange.block);
    assert_eq!(final_drop.block, break_edge.target);
    assert_eq!(source.sources.slice(old.origin.span()), Ok("previous"));
    assert_eq!(
        source.sources.slice(final_drop.origin.span()),
        Ok("retired")
    );
    let body = function.block(exchange.block).unwrap();
    assert!(
        body.instructions
            .iter()
            .position(|id| *id == exchange.id)
            .unwrap()
            < body
                .instructions
                .iter()
                .position(|id| *id == old.id)
                .unwrap()
    );
    assert!(matches!(
        function
            .block(break_edge.target)
            .unwrap()
            .terminator
            .as_ref()
            .unwrap()
            .kind,
        TerminatorKind::Return { .. }
    ));
}

#[test]
fn bounded_replace_break_physical_drop_has_product_authorization() {
    use lang_frontend::ownership_checking::{
        UnitDropTarget, UnitOwnerReplaceBreakConsumerActionV1 as A,
    };
    let source =
        UnitSource::analyze("bounded-owner-consumer.ko", SOURCE).expect("ordinary B source");
    source.assert_frontend_clean();
    let OwnedProducts::Basic(_, owned) = &source.products else {
        panic!("basic Unit")
    };
    let boundary = owned
        .ownership()
        .owner_observations()
        .iter()
        .find_map(|o| o.replace_break_observation_v1())
        .expect("B boundary");
    let A::Drop(point, retired) = boundary.suffix_consumer_v1().unwrap().actions()[2] else {
        panic!("Drop")
    };
    let index = owned
        .ownership()
        .drops()
        .iter()
        .position(|d| d.point() == point && d.target() == UnitDropTarget::Named(retired))
        .unwrap();
    let ((result, events), actions) = crate::ssa::observe_owner_actions(|| {
        crate::ssa::observe_owner_drops(|| {
            source.lower_entry(source.declaration("entry", SymbolKind::Function))
        })
    });
    let (program, _) = result.expect("ordinary B lowering");
    verify_program(&program).expect("selected owner SSA");
    let selected = events
        .iter()
        .filter(|e| e.0 == UnitDropTarget::Named(retired))
        .collect::<Vec<_>>();
    assert_eq!(
        selected.len(),
        1,
        "one physical suffix Drop, not one per incoming"
    );
    assert_eq!(selected[0].1, index, "exact legacy cleanup obligation");
    let (taken, bound, authorized_index) = selected[0]
        .3
        .expect("physical Drop must consume a completed product Take/Bind authorization");
    assert_eq!(
        bound, selected[0].2,
        "authorized selected binding reaches emitter"
    );
    assert_eq!(authorized_index, index);
    assert_eq!(
        actions,
        [
            ("root", taken),
            ("take", taken),
            ("bind", bound),
            ("drop", bound)
        ]
    );
    assert_ne!(taken, bound);
}

#[test]
fn bounded_replace_break_dual_units_same_literal_and_reanalysis_keep_receipts_local() {
    for constant in [false, true] {
        for same_literal in [false, true] {
            let mut text = if same_literal {
                SOURCE.replace("next", "outer")
            } else {
                SOURCE.to_owned()
            };
            if constant {
                text = format!(
                    "const val ON: Boolean = true\n{}",
                    text.replace("run(true)", "run(ON)")
                );
            }
            let mut previous_owner = None;
            for name in ["first-owner-source.ko", "second-owner-source.ko"] {
                let source = UnitSource::analyze(name, &text).expect("ordinary source");
                source.assert_frontend_clean();
                assert_eq!(
                    matches!(source.products, OwnedProducts::Constant(..)),
                    constant
                );
                let owned = match &source.products {
                    OwnedProducts::Basic(_, owned) => owned.ownership(),
                    OwnedProducts::Constant(_, owned) => owned.ownership(),
                };
                let boundary = owned
                    .owner_observations()
                    .iter()
                    .find_map(|o| o.replace_break_observation_v1())
                    .unwrap();
                let p = boundary.condition_false_exit().owner();
                let q = boundary.break_boundary().owner();
                assert_ne!(p, q);
                assert!(boundary.input_producers().definition(q.clone()).is_none());
                assert!(boundary.body_producers().definition(p.clone()).is_none());
                if let Some(previous) = previous_owner {
                    assert_ne!(p, previous);
                }
                previous_owner = Some(p);
                let (result, drops) =
                    crate::ssa::observe_owner_drops(|| source.lower_entry(source.entry()));
                let (program, entry) = result.expect("both Unit capabilities lower");
                verify_program(&program).unwrap();
                crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
                assert_eq!(
                    drops.len(),
                    2,
                    "detached P and selected owner, no extra Drop"
                );
                assert_eq!(drops.iter().filter(|d| d.3.is_some()).count(), 1);
                let authorized = drops.iter().find(|d| d.3.is_some()).unwrap();
                let (selected, bound, index) = authorized.3.unwrap();
                assert_ne!(selected, bound, "Take precedes the actual Alias binding");
                assert_eq!((bound, index), (authorized.2, authorized.1));
            }
        }
    }
}

#[test]
fn bounded_replace_break_absent_optional_product_keeps_ordinary_lowering() {
    let text = SOURCE.replace("while(flag)", "while((flag == true) && flag)");
    let source =
        UnitSource::analyze("ordinary-logical-condition.ko", &text).expect("legal noncandidate");
    source.assert_frontend_clean();
    let OwnedProducts::Basic(_, owned) = &source.products else {
        panic!("basic")
    };
    assert!(owned.ownership().owner_observations().iter().all(|o| {
        o.replace_break_observation_v1()
            .and_then(|b| b.suffix_consumer_v1())
            .is_none()
    }));
    let (result, drops) = crate::ssa::observe_owner_drops(|| source.lower_entry(source.entry()));
    let (program, entry) = result.expect("optional B must not reject legal noncandidate");
    verify_program(&program).unwrap();
    crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    assert_eq!(drops.len(), 2);
    assert!(drops.iter().all(|d| d.3.is_none()));
}

#[test]
fn bounded_replace_break_both_runtime_edges_reach_native_in_both_units() {
    let text = SOURCE.replace(
        "run(false); run(true)",
        "println(\"before\"); run(false); println(\"middle\"); run(true); println(\"after\")",
    );
    for constant in [false, true] {
        let text = if constant {
            format!(
                "const val ON: Boolean = true\n{}",
                text.replace("run(true)", "run(ON)")
            )
        } else {
            text.clone()
        };
        let source = UnitSource::analyze("bounded-owner-native.ko", &text)
            .expect("ordinary B native source");
        source.assert_frontend_clean();
        assert_eq!(
            matches!(source.products, OwnedProducts::Constant(..)),
            constant
        );
        super::assert_native_output(&source, b"before\nmiddle\nafter\n");
    }
}

#[test]
fn ci_only_bounded_replace_break_rejects_replayed_actions_and_corrupt_receipts() {
    use crate::ssa::OwnerFault::*;
    for fault in [
        DuplicateTake,
        DuplicateBind,
        DuplicateDrop,
        WrongSlot,
        WrongIndex,
        MissingFinish,
    ] {
        let source = UnitSource::analyze("ci-only-owner-receipt.ko", SOURCE).unwrap();
        let result = crate::ssa::with_owner_fault(fault, || source.lower_entry(source.entry()));
        assert!(
            result.is_err(),
            "selected B never falls back after a broken action or ledger"
        );
    }
}

#[test]
fn bounded_replace_break_same_analysis_callables_keep_products_and_receipts_local() {
    use lang_frontend::ownership_checking::{
        UnitDropTarget, UnitOwnerReplaceBreakConsumerActionV1 as A,
    };
    let run = SOURCE.split("\nfun entry").next().unwrap();
    let text = format!(
        "{run}\n{}\nfun entry(): Unit {{ run(false); other(true); run(true); other(false); println(\"done\") }}",
        run.replace("fun run(", "fun other(")
    );
    for constant in [false, true] {
        let text = if constant {
            format!(
                "const val ON: Boolean = true\n{}",
                text.replace("(true)", "(ON)")
            )
        } else {
            text.clone()
        };
        let source = UnitSource::analyze("two-b-callables.ko", &text).unwrap();
        source.assert_frontend_clean();
        assert_eq!(
            matches!(source.products, OwnedProducts::Constant(..)),
            constant
        );
        let owned = match &source.products {
            OwnedProducts::Basic(_, owned) => owned.ownership(),
            OwnedProducts::Constant(_, owned) => owned.ownership(),
        };
        let boundaries = owned
            .owner_observations()
            .iter()
            .filter_map(|o| o.replace_break_observation_v1())
            .collect::<Vec<_>>();
        assert_eq!(boundaries.len(), 2);
        for (i, boundary) in boundaries.iter().enumerate() {
            let other = boundaries[1 - i];
            let p = boundary.condition_false_exit().owner();
            let q = boundary.break_boundary().owner();
            assert!(boundary.input_producers().definition(p.clone()).is_some());
            assert!(boundary.body_producers().definition(q.clone()).is_some());
            assert_ne!(p, other.condition_false_exit().owner());
            assert_ne!(q, other.break_boundary().owner());
        }
        let (result, drops) =
            crate::ssa::observe_owner_drops(|| source.lower_entry(source.entry()));
        let (program, entry) = result.unwrap();
        verify_program(&program).unwrap();
        crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
        let authorized = drops.iter().filter(|d| d.3.is_some()).collect::<Vec<_>>();
        assert_eq!(
            authorized.len(),
            2,
            "one receipt per callable, not per call site"
        );
        let mut indices = Vec::new();
        for boundary in boundaries {
            let A::Drop(point, retired) = boundary.suffix_consumer_v1().unwrap().actions()[2]
            else {
                panic!("Drop action")
            };
            let index = owned
                .drops()
                .iter()
                .position(|d| d.point() == point && d.target() == UnitDropTarget::Named(retired))
                .unwrap();
            let matches = authorized
                .iter()
                .filter(|d| d.0 == UnitDropTarget::Named(retired) && d.1 == index)
                .collect::<Vec<_>>();
            assert_eq!(matches.len(), 1);
            let event = matches[0];
            let (selected, bound, receipt_index) = event.3.unwrap();
            assert_ne!(selected, bound);
            assert_eq!((bound, receipt_index), (event.2, index));
            indices.push(index);
        }
        assert_ne!(indices[0], indices[1]);
        let functions = program
            .modules
            .iter()
            .flat_map(|m| &m.functions)
            .filter(|f| {
                source
                    .sources
                    .slice(f.origin.span())
                    .is_ok_and(|s| s.starts_with("fun run(") || s.starts_with("fun other("))
            })
            .collect::<Vec<_>>();
        assert_eq!(functions.len(), 2);
        for function in functions {
            assert_selected_owner(&source, function);
        }
        super::assert_native_output(&source, b"done\n");
    }
}

pub(crate) mod owner_boolean_comparison_tests {
//! Finite real Boolean comparisons; no injected facts, foreign lookups or resource counters.
use super::super::{OwnedProducts, UnitSource};
const SOURCE: &str = "fun run(flag: Boolean): String { var held = \"outer\"; while(flag == true) { val previous = replace(&held, \"next\"); break }; return held }\nfun entry(): Unit { val first = run(false); val second = run(true) }";
// Whether the complete source condition is true when its parameter is false.
const CASES: &[(&str, bool)] = &[
    ("flag == true", false),
    ("flag == false", true),
    ("flag != true", true),
    ("flag != false", false),
    ("true == flag", false),
    ("false == flag", true),
    ("true != flag", true),
    ("false != flag", false),
    ("!((!flag) == true)", false),
    ("(true != (!flag))", false),
];
#[test]
fn boolean_comparison_actual_return_requires_owner_receipt() {
    let source = UnitSource::analyze("comparison-owner.ko", SOURCE).unwrap();
    source.assert_frontend_clean();
    let (lowered, events) =
        crate::ssa::observe_owner_returns(|| source.lower_entry(source.entry()));
    let (program, entry) = lowered.unwrap();
    crate::ssa::verify::verify_program(&program).unwrap();
    crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    let [event] = events.as_slice() else {
        panic!("one actual Return")
    };
    assert!(event.1.is_some());
    assert_eq!(
        event.2, event.1,
        "actual comparison Return needs P/Q authorization"
    );
}
#[test]
fn boolean_comparison_keeps_actual_owner_edges_in_both_units() {
    for (condition, _) in CASES {
        super::super::owner_return_tests::assert_return_transport(
            &SOURCE.replace("flag == true", condition),
        );
    }
}
#[test]
fn boolean_comparison_native_preserves_all_truth_tables() {
    let body = SOURCE.split("\nfun entry").next().unwrap();
    let mut text = String::new();
    let mut entry = String::from("fun entry(): Unit {");
    let mut expected = Vec::new();
    for (index, (condition, false_result)) in CASES.iter().enumerate() {
        text.push_str(
            &body
                .replace("fun run(", &format!("fun run{index}("))
                .replace("flag == true", condition),
        );
        text.push('\n');
        entry.push_str(&format!("val first{index} = run{index}(false); println(first{index}); val second{index} = run{index}(true); println(second{index});"));
        expected.extend_from_slice(if *false_result {
            b"next\nouter\n"
        } else {
            b"outer\nnext\n"
        });
    }
    entry.push('}');
    text.push_str(&entry);
    for constant in [false, true] {
        let text = if constant {
            format!(
                "const val ON: Boolean = true\n{}",
                text.replace("(true)", "(ON)")
            )
        } else {
            text.clone()
        };
        let source = UnitSource::analyze("comparison-owner-native.ko", &text).unwrap();
        source.assert_frontend_clean();
        assert_eq!(
            matches!(source.products, OwnedProducts::Constant(..)),
            constant
        );
        super::super::assert_native_output(&source, &expected);
    }
}
}

pub(crate) mod owner_pair_call_tests {
//! Finite normal source tests for selected owner and Copy Value operand mapping.
use super::super::UnitSource;
use crate::ssa::{model::Operation, verify::verify_program};
fn cases() -> [(&'static str, &'static str, usize); 4] {
    [
        ("own item: String, own count: Int", "held, 7", 0),
        ("own count: Int, own item: String", "7, held", 1),
        (
            "own item: String, own count: Int",
            "count = 7, item = held",
            0,
        ),
        (
            "own count: Int, own item: String",
            "item = held, count = 7",
            1,
        ),
    ]
}
fn source(parameters: &str, arguments: &str, output: bool) -> String {
    let body = if output {
        r#"println(item); if (count == 7) { println("7") } else { println("wrong") }"#
    } else {
        ""
    };
    format!(
        r#"fun sink({parameters}): Unit {{ {body} }}
fun run(flag: Boolean) {{ var held = "outer"; while(flag) {{ val previous = replace(&held, "next"); break }}; sink({arguments}) }}
fun entry(): Unit {{ run(false); run(true) }}"#
    )
}
#[test]
fn pair_call_both_units_preserve_selected_slot_and_cleanup() {
    use super::super::OwnedProducts;
    use crate::ssa::model::{EntityId, Function, TerminatorKind};
    fn edge(function: &Function, block: crate::ssa::model::BlockId) -> &crate::ssa::model::Edge {
        let TerminatorKind::Branch(edge) = &function
            .block(block)
            .unwrap()
            .terminator
            .as_ref()
            .unwrap()
            .kind
        else {
            panic!("exit branch")
        };
        edge
    }
    for (parameters, arguments, selected_index) in cases() {
        let source_text = source(parameters, arguments, false);
        for constant in [false, true] {
            let text = if constant {
                format!(
                    "const val ON: Boolean = true\n{}",
                    source_text.replace("run(true)", "run(ON)")
                )
            } else {
                source_text.clone()
            };
            let source = UnitSource::analyze("owner-own-call-dual.ko", &text).unwrap();
            source.assert_frontend_clean();
            assert_eq!(
                matches!(source.products, OwnedProducts::Constant(..)),
                constant
            );
            let owned = match &source.products {
                OwnedProducts::Basic(_, owned) => owned.ownership(),
                OwnedProducts::Constant(_, owned) => owned.ownership(),
            };
            let boundaries = owned
                .owner_observations()
                .iter()
                .filter_map(|o| o.replace_break_observation_v1())
                .collect::<Vec<_>>();
            let [boundary] = boundaries.as_slice() else {
                panic!("one selected-owner boundary")
            };
            assert!(boundary.suffix_consumer_v1().is_none());
            let consumer = boundary.suffix_call_consumer_v1().unwrap();
            assert_ne!(
                consumer.incoming()[0].owner(),
                consumer.incoming()[1].owner()
            );
            let ((lowered, calls), actions) = crate::ssa::observe_owner_actions(|| {
                crate::ssa::observe_owner_calls(|| source.lower_entry(source.entry()))
            });
            let (program, entry) = lowered.unwrap();
            verify_program(&program).unwrap();
            crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
            let functions = program
                .modules
                .iter()
                .flat_map(|m| &m.functions)
                .collect::<Vec<_>>();
            let named = |prefix: &str| {
                *functions
                    .iter()
                    .find(|f| {
                        source
                            .sources
                            .slice(f.origin.span())
                            .is_ok_and(|s| s.starts_with(prefix))
                    })
                    .unwrap()
            };
            let run = named("fun run(");
            let sink = named("fun sink(");
            let selected_calls = calls
                .iter()
                .filter(|c| c.0 == consumer.call())
                .collect::<Vec<_>>();
            let [receipt] = selected_calls.as_slice() else {
                panic!("one matching call receipt")
            };
            assert_eq!(receipt.1, sink.id());
            let selected_slot = receipt.3.unwrap();
            assert_eq!(receipt.2.len(), 2);
            assert_eq!(receipt.2[selected_index], selected_slot);
            assert_eq!(actions, [("root", selected_slot), ("take", selected_slot)]);
            let initial = run.instructions.iter().find(|i| matches!(&i.operation, Operation::StringLiteral { bytes, .. } if bytes == b"outer")).unwrap();
            let replacement = run.instructions.iter().find(|i| matches!(&i.operation, Operation::StringLiteral { bytes, .. } if bytes == b"next")).unwrap();
            let branches = run
                .blocks
                .iter()
                .filter_map(|b| match &b.terminator.as_ref()?.kind {
                    TerminatorKind::Conditional {
                        when_true,
                        when_false,
                        ..
                    } => Some((when_true, when_false)),
                    _ => None,
                })
                .collect::<Vec<_>>();
            let [(body, no_body)] = branches.as_slice() else {
                panic!("two exact runtime alternatives")
            };
            assert_eq!(body.arguments, initial.results);
            assert_eq!(no_body.arguments, initial.results);
            let exchange = run
                .instructions
                .iter()
                .find(|i| matches!(i.operation, Operation::RootReplace { .. }))
                .unwrap();
            let Operation::RootReplace {
                owner,
                replacement: rhs,
                ..
            } = exchange.operation
            else {
                unreachable!()
            };
            assert_eq!(
                EntityId::Value(owner),
                run.block(body.target).unwrap().parameters[0]
            );
            assert_eq!(EntityId::Value(rhs), replacement.results[0]);
            let from_break = edge(run, exchange.block);
            let from_false = edge(run, no_body.target);
            assert_eq!(from_break.target, from_false.target);
            assert_eq!(from_break.arguments, [exchange.results[0]]);
            assert_eq!(
                from_false.arguments,
                run.block(no_body.target).unwrap().parameters
            );
            let exit = run.block(from_break.target).unwrap();
            assert_eq!(exit.parameters.len(), 1);
            let call = run.instructions.iter().find(|i| matches!(i.operation, Operation::DirectCall { callee, .. } if callee == sink.id())).unwrap();
            let Operation::DirectCall { arguments, .. } = &call.operation else {
                unreachable!()
            };
            assert_eq!(arguments.len(), 2);
            assert_eq!(arguments[selected_index], exit.parameters[0]);
            assert_ne!(arguments[1 - selected_index], exit.parameters[0]);
            assert_eq!(call.block, exit.id);
            let caller_drops = run
                .instructions
                .iter()
                .filter_map(|i| match i.operation {
                    Operation::Drop { owner } => Some((i, owner)),
                    _ => None,
                })
                .collect::<Vec<_>>();
            let [(old_drop, old_owner)] = caller_drops.as_slice() else {
                panic!("only detached P is dropped by caller")
            };
            assert_eq!(EntityId::Value(*old_owner), exchange.results[1]);
            assert_eq!(old_drop.block, exchange.block);
            let callee_drops = sink
                .instructions
                .iter()
                .filter_map(|i| match i.operation {
                    Operation::Drop { owner } => Some(owner),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(callee_drops.len(), 1);
            assert_eq!(
                EntityId::Value(callee_drops[0]),
                sink.block(sink.entry_block().unwrap()).unwrap().parameters[selected_index]
            );
        }
    }
}

#[test]
fn pair_call_native_preserves_values_and_parameter_order() {
    for (parameters, arguments, _) in cases() {
        for constant in [false, true] {
            let mut text = source(parameters, arguments, true);
            if constant {
                text = format!(
                    "const val ON: Boolean = true\n{}",
                    text.replace("run(true)", "run(ON)")
                );
            }
            let source = UnitSource::analyze("pair-native.ko", &text).unwrap();
            source.assert_frontend_clean();
            super::super::assert_native_output(&source, b"outer\n7\nnext\n7\n");
        }
    }
}

// Deliberate mutations are compiled locally; execution belongs to official CI only.
#[test]
fn ci_only_pair_call_rejects_wrong_selected_slot_and_replayed_call() {
    use crate::ssa::OwnerFault::{DuplicateOwnCall, MissingOwnCall, WrongOwnCallSlot};
    for (parameters, arguments, _) in cases() {
        for fault in [DuplicateOwnCall, MissingOwnCall, WrongOwnCallSlot] {
            let text = source(parameters, arguments, false);
            let source = UnitSource::analyze("pair-ci.ko", &text).unwrap();
            source.assert_frontend_clean();
            assert!(
                crate::ssa::with_owner_fault(fault, || source.lower_entry(source.entry())).is_err()
            );
        }
    }
}
}
