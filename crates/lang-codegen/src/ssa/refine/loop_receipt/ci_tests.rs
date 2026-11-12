//! CI-only controls over genuine single literal-owner loop append events. Compile locally, never execute.
use super::*;
use crate::ssa::unit_lower_test_support;
use lang_frontend::{
    name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
};
use std::cell::RefCell;
#[derive(Clone)]
struct Snapshot {
    plans: Plans,
    function: Function,
    receipt: Receipt,
    span: Span,
}
thread_local! { static SNAPSHOTS: RefCell<Option<Vec<Snapshot>>> = const { RefCell::new(None) }; }
pub(super) fn record(plans: &Plans, function: &Function, receipts: &[Receipt], span: Span) {
    SNAPSHOTS.with(|v| {
        if let Some(v) = v.borrow_mut().as_mut() {
            for receipt in receipts {
                v.push(Snapshot {
                    plans: plans.clone(),
                    function: function.clone(),
                    receipt: receipt.clone(),
                    span,
                });
            }
        }
    });
}
fn capture() -> Snapshot {
    let text = "fun view(source: String): borrow String from source = source\nfun consume(item: String): Unit {}\nfun run(flag: Boolean): Unit { val source=\"value\"; borrow val item=view(source); while(flag){consume(item);consume(item);} }";
    let mut sources = SourceMap::new();
    let (source, parsed) = unit_lower_test_support::parsed(&mut sources, "ci-loop.ko", text);
    let inputs = [SourceUnitInput::memory_document(source, &parsed)];
    let (environment, types) = standard_environments();
    let (names, typed, owned) =
        unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
    let entry = names
        .names()
        .index()
        .declarations()
        .iter()
        .find(|d| d.name() == "run")
        .unwrap()
        .id();
    SNAPSHOTS.with(|v| *v.borrow_mut() = Some(Vec::new()));
    crate::ssa::unit_lower::lower_scalar_unit_with_entry(
        &sources, &inputs, &names, &types, &typed, &owned, entry,
    )
    .unwrap();
    let mut snapshots = SNAPSHOTS.with(|v| v.borrow_mut().take().unwrap());
    assert_eq!(snapshots.len(), 1);
    snapshots.pop().unwrap()
}

fn replay(s: &Snapshot, corrupt: bool) -> Result<Emitted, LoweringError> {
    let mut e = Emitted::new(&s.plans, &s.function, s.span)?;
    let parameters = s
        .receipt
        .blocks
        .iter()
        .map(|(b, id)| (*b, s.receipt.parameters[id].clone()))
        .collect();
    let entities = s
        .receipt
        .blocks
        .iter()
        .map(|(b, id)| (*b, s.receipt.entities[id].clone()))
        .collect();
    e.layout(
        &s.plans,
        &s.function,
        &s.receipt.blocks,
        &parameters,
        &entities,
        s.span,
    )?;
    let p = s.plans.plan.as_ref().unwrap();
    let mut currents = e.expected.clone();
    for (tag, id) in &s.receipt.instructions {
        let d = s.function.instruction(*id).unwrap();
        let current = currents.get_mut(&d.block).unwrap();
        let slot = match tag {
            Tag::Owner => p.literal,
            Tag::Place => p.place,
            Tag::Source => p.source,
            Tag::Result => p.result,
            Tag::Condition => p.condition_value,
            _ => None,
        };
        if let Some(slot) = slot {
            current.insert(slot, d.results[0]);
        }
        if corrupt && *tag == Tag::Place {
            current.insert(p.owner.unwrap(), d.results[0]);
        }
        e.statement(&s.plans, *tag, &s.function, *id, current, s.span)?;
        if *tag == Tag::Owner {
            let (source, result) = p.owner_alias.ok_or_else(|| invalid(s.span))?;
            let value = *current.get(&source).ok_or_else(|| invalid(s.span))?;
            current.insert(result, value);
            e.alias(&s.plans, source, result, d.block, current, s.span)?;
        }
    }
    for (block, current) in &currents {
        e.terminator(&s.plans, &s.function, *block, current, s.span)?;
    }
    Ok(e)
}
#[test]
#[ignore = "CI-only fault injection; compile locally"]
fn loop_frozen_current_and_missing_foreign_events() {
    let s = capture();
    assert!(
        replay(&s, false)
            .unwrap()
            .finish(&s.plans, &s.function, s.span)
            .is_ok()
    );
    assert!(replay(&s, true).is_err());
    let mut missing = s.clone();
    missing.receipt.instructions.pop();
    assert!(
        replay(&missing, false)
            .and_then(|e| e.finish(&missing.plans, &missing.function, missing.span))
            .is_err()
    );
    let foreign = capture();
    let mut mixed = s.clone();
    mixed.receipt.instructions[0].1 = foreign.receipt.instructions[0].1;
    assert!(
        mixed
            .function
            .instruction(mixed.receipt.instructions[0].1)
            .is_none()
    );
}
#[test]
#[ignore = "CI-only fault injection; compile locally"]
fn loop_wrong_fold_and_entry_flag_rejected() {
    let s = capture();
    let mut bad = s.clone();
    let p = bad.plans.plan.as_mut().unwrap();
    let b = p.blocks.unwrap();
    p.folds.push((b.header, b.body));
    assert!(Emitted::new(&bad.plans, &bad.function, bad.span).is_err());
    let mut bad = s.clone();
    bad.plans.plan.as_mut().unwrap().flag_parameter = usize::MAX;
    assert!(replay(&bad, false).is_err());
}
#[test]
#[ignore = "CI-only fault injection; compile locally"]
fn loop_failure_rolls_back_pending() {
    let s = capture();
    let (_, receipts) = observe(|| {
        let _guard = UnitPublication::begin();
        replay(&s, false)
            .unwrap()
            .finish(&s.plans, &s.function, s.span)
            .unwrap();
    });
    assert!(receipts.is_empty());
}
#[test]
#[ignore = "CI-only fault injection; compile locally"]
fn loop_parameter_transport_substitution_rejected() {
    let s = capture();
    let mut bad = s.clone();
    let block = *bad
        .receipt
        .entities
        .iter()
        .find(|(_, v)| v.len() > 1)
        .unwrap()
        .0;
    bad.receipt.entities.get_mut(&block).unwrap().swap(0, 1);
    assert!(replay(&bad, false).is_err());
}
