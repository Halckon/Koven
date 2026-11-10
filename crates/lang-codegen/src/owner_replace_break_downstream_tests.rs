//! Ordinary original B source through existing lowering, not a new carrier consumer.
#[path = "owner_loan_receipt_sibling_tests.rs"]
mod owner_loan_receipt_sibling_tests;
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
    let text = SOURCE.replace("while(flag)", "while((flag))");
    let source =
        UnitSource::analyze("ordinary-grouped-condition.ko", &text).expect("legal noncandidate");
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
