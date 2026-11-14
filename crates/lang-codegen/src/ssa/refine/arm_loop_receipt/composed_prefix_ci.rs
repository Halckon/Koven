//! CI-only mutations of independent scalar-prefix/counter/exit composition receipts.
use super::publication::{Fault, Trace, require_ci, with_fault};
use super::*;
use crate::ssa::{
    model::{Program, TerminatorKind},
    unit_lower_test_support,
};
use lang_frontend::{
    name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
};
fn lower_shape(
    fault: Option<Fault>,
    shared: bool,
) -> (Result<Program, LoweringError>, Vec<Receipt>, Trace) {
    let body = "(source:String,rounds:Int,choose:Boolean,stop:Boolean,other:String,before:Boolean):Unit {if(before){tick(1)}else{tick(2)};var remaining=rounds;while(remaining>0){if(choose){borrow val a=view(source);consume(a)}else{borrow val b=view(source);consume(b);consume(b)};if(stop){break};remaining=remaining-1};consume(source);consume(other)}";
    let text = format!(
        "fun tick(own n:Int):Unit {{}}\nfun view(source:String):borrow String from source=source\nfun consume(item:String):Unit {{}}\nfun first{body}\nfun second{body}\nfun run(source:String,rounds:Int,choose:Boolean,stop:Boolean,before:Boolean):Unit {{first(source,rounds,choose,stop,source,before);second(source,rounds,choose,stop,source,before)}}"
    );
    let text = if shared {
        text.replace("if(before){tick", "if(stop){tick")
            .replace(",before:Boolean", "")
            .replace(",source,before)", ",source)")
    } else {
        text
    };
    let mut sources = SourceMap::new();
    let (source, parsed) = unit_lower_test_support::parsed(&mut sources, "break-ci.ko", &text);
    let inputs = [SourceUnitInput::memory_document(source, &parsed)];
    let (environment, types) = standard_environments();
    let (names, typed, owned) =
        unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
    assert_eq!(owned.ownership().no_carry_prefix_counter_breaks().len(), 2);
    let entry = names
        .names()
        .index()
        .declarations()
        .iter()
        .find(|d| d.name() == "run")
        .unwrap()
        .id();
    let execute = || {
        crate::ssa::unit_lower::lower_scalar_unit_with_entry(
            &sources, &inputs, &names, &types, &typed, &owned, entry,
        )
        .map(|(p, _)| p)
    };
    match fault {
        Some(f) => with_fault(f, execute),
        None => observe(execute),
    }
}
fn lower(fault: Option<Fault>) -> (Result<Program, LoweringError>, Vec<Receipt>, Trace) {
    lower_shape(fault, false)
}
fn capture_shape(shared: bool) -> (Function, Receipt) {
    let (p, receipts, trace) = lower_shape(None, shared);
    let p = p.unwrap();
    assert_eq!(receipts.len(), 2);
    assert_eq!(trace.recorded, 2);
    assert_eq!(trace.before_verifier, 2);
    assert_eq!(trace.pending_after, 0);
    assert!(!trace.rolled_back);
    for r in &receipts {
        assert!(r.plan.counter_break.is_some());
        assert!(r.plan.prefix.as_ref().unwrap().is_counter_composition());
        assert_eq!(r.evidence.composed_prefix_selected.len(), 1);
        let prefix_fact::PrefixFact::Counter(fact) = &r.plan.prefix.as_ref().unwrap().fact else {
            panic!("composition")
        };
        assert!(r.evidence.composed_prefix_selected[0].same_identity(fact));
        r.validate(
            p.modules
                .iter()
                .flat_map(|m| &m.functions)
                .find(|f| f.id == r.caller)
                .unwrap(),
        )
        .unwrap();
    }
    let r = receipts[0].clone();
    let f = p
        .modules
        .iter()
        .flat_map(|m| &m.functions)
        .find(|f| f.id == r.caller)
        .unwrap()
        .clone();
    (f, r)
}
#[test]
fn ordinary_composed_prefix_receipts_publish_and_replay() {
    let _ = capture_shape(false);
    let _ = capture_shape(true);
}
fn capture() -> (Function, Receipt) {
    capture_shape(false)
}

#[test]
#[ignore = "CI-only prefix capture loss; local compile/list only"]
fn composed_prefix_capture_loss_revokes_and_recovers() {
    require_ci();
    reject_restore(Fault::PrefixCaptureMissing);
}
#[test]
#[ignore = "CI-only composition selection loss; local compile/list only"]
fn composed_prefix_selection_loss_revokes_and_recovers() {
    require_ci();
    reject_restore(Fault::ComposedPrefixReceiptMissing);
}
fn reject_restore(fault: Fault) {
    let (result, receipts, trace) = lower(Some(fault));
    assert!(result.is_err());
    assert!(receipts.is_empty());
    assert!(trace.rolled_back);
    assert_eq!(trace.pending_after, 0);
    assert_eq!(trace.before_verifier, 0);
    let _ = capture();
}
#[test]
#[ignore = "CI-only prefix arm bypasses initialization; local compile/list only"]
fn composed_prefix_join_bypass_rejects() {
    require_ci();
    let (f, r) = capture();
    for arm in &r.plan.prefix.as_ref().unwrap().split.unwrap()[1..] {
        let mut bad = f.clone();
        let id = r.evidence.blocks[arm];
        let target = r.evidence.blocks[&r.plan.blocks.unwrap()[1]];
        let TerminatorKind::Branch(edge) =
            &mut bad.blocks[id.index()].terminator.as_mut().unwrap().kind
        else {
            panic!("prefix edge")
        };
        assert_ne!(edge.target, target);
        edge.target = target;
        assert!(r.validate(&bad).is_err());
    }
}
#[test]
#[ignore = "CI-only remove actual initialization read; local compile/list only"]
fn composed_prefix_initializer_missing_rejects() {
    require_ci();
    let (mut f, r) = capture();
    let join = r.evidence.blocks[&r.plan.prefix.as_ref().unwrap().join.unwrap()];
    let id = f.blocks[join.index()].instructions.remove(0);
    assert!(matches!(
        f.instruction(id).unwrap().operation,
        crate::ssa::model::Operation::Read { .. }
    ));
    assert!(r.validate(&f).is_err());
}
#[test]
#[ignore = "CI-only active stop replaced by another Boolean root; local compile/list only"]
fn composed_prefix_stop_identity_rejects() {
    require_ci();
    let (mut f, r) = capture_shape(true);
    let gate = r.plan.counter_break.as_ref().unwrap().split.unwrap()[0];
    let id = r.evidence.blocks[&gate];
    let EntityId::Loan(other) = r
        .evidence
        .entity(gate, r.parameters[r.plan.roles[2]], r.span)
        .unwrap()
    else {
        panic!("choose loan")
    };
    let iid = f.blocks[id.index()].instructions[0];
    let inst = f.instructions.iter_mut().find(|i| i.id == iid).unwrap();
    let crate::ssa::model::Operation::Read {
        source: crate::ssa::model::PlaceAccess::Loan(source),
    } = &mut inst.operation
    else {
        panic!("stop Read")
    };
    assert_ne!(*source, other);
    *source = other;
    assert!(r.validate(&f).is_err());
}
#[test]
#[ignore = "CI-only resource root lost during scalar trim; local compile/list only"]
fn composed_prefix_resource_transport_rejects() {
    require_ci();
    let (f, r) = capture();
    let prefix = r.plan.prefix.as_ref().unwrap();
    let join = prefix.join.unwrap();
    let source = r.parameters[r.plan.roles[0]];
    let position = r.evidence.parameters[&join]
        .iter()
        .position(|s| *s == source)
        .unwrap();
    for arm in &prefix.split.unwrap()[1..] {
        let mut bad = f.clone();
        let id = r.evidence.blocks[arm];
        let TerminatorKind::Branch(edge) =
            &mut bad.blocks[id.index()].terminator.as_mut().unwrap().kind
        else {
            panic!("edge")
        };
        let removed = edge.arguments.remove(position);
        assert!(matches!(removed, EntityId::Loan(_)));
        assert!(r.validate(&bad).is_err());
    }
}
#[test]
#[ignore = "CI-only actual exit and continue target substitution; local compile/list only"]
fn composed_prefix_dual_exit_rejects() {
    require_ci();
    let (f, r) = capture();
    let bs = r.plan.blocks.unwrap();
    let cb = r.plan.counter_break.as_ref().unwrap();
    let success = r.plan.counter.as_ref().unwrap().checked.unwrap().0[1];
    for (from, to) in [(cb.split.unwrap()[1], bs[1]), (success, bs[4])] {
        let mut bad = f.clone();
        let id = r.evidence.blocks[&from];
        let target = r.evidence.blocks[&to];
        let TerminatorKind::Branch(edge) =
            &mut bad.blocks[id.index()].terminator.as_mut().unwrap().kind
        else {
            panic!("edge")
        };
        assert_ne!(edge.target, target);
        edge.target = target;
        assert!(r.validate(&bad).is_err());
    }
}
#[cfg(feature = "loan-observation-test-support")]
#[test]
#[ignore = "CI-only final verifier revokes compositions; local compile/list only"]
fn composed_prefix_final_verifier_revokes_and_recovers() {
    require_ci();
    let ((result, receipts, trace), verified) =
        crate::ssa::unit_lower::observe_final_verifier(|| lower(Some(Fault::FinalVerifier)));
    assert_eq!(verified.len(), 1);
    assert!(verified[0].contains("MissingTerminator"));
    assert!(result.is_err());
    assert!(receipts.is_empty());
    assert_eq!(trace.recorded, 2);
    assert_eq!(trace.before_verifier, 2);
    assert_eq!(trace.pending_after, 0);
    assert!(trace.rolled_back);
    let _ = capture();
}
