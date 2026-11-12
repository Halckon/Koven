//! CI-only corruption of captured real products. Compile locally, never execute locally.
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
pub(super) fn record(plans: &Plans, function: &Function, receipt: &Receipt, span: Span) {
    SNAPSHOTS.with(|v| {
        if let Some(v) = v.borrow_mut().as_mut() {
            v.push(Snapshot {
                plans: plans.clone(),
                function: function.clone(),
                receipt: receipt.clone(),
                span,
            });
        }
    });
}
fn capture() -> Snapshot {
    let text = "fun view(source: String): borrow String from source = source\nfun wrap(source: String): borrow String from source = view(source)\nfun consume(item: String): Unit {}\nfun run(): Unit { val owner = \"kept\"; borrow val item = wrap(owner); consume(item) }";
    let mut sources = SourceMap::new();
    let (source, parsed) = unit_lower_test_support::parsed(&mut sources, "ci-forward.ko", text);
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
    let mut output = SNAPSHOTS.with(|v| v.borrow_mut().take().unwrap());
    assert_eq!(output.len(), 1);
    output.pop().unwrap()
}
fn current(s: &Snapshot) -> BTreeMap<Slot, EntityId> {
    let p = &s.plans.entries[0];
    [
        (p.source.unwrap(), EntityId::Loan(s.receipt.source)),
        (p.result.unwrap(), EntityId::Loan(s.receipt.result)),
    ]
    .into_iter()
    .collect()
}
fn call(s: &Snapshot) -> Result<Emitted, LoweringError> {
    let mut e = Emitted::new(&s.plans, &s.function, s.span)?;
    e.statement(
        &s.plans,
        Tag,
        &s.function,
        s.receipt.instruction,
        &current(s),
        s.span,
    )?;
    Ok(e)
}
#[test]
fn ci_only_forward_receipt_rejects_missing_duplicate_and_corrupt_roles() {
    let s = capture();
    let foreign = capture();
    assert!(Emitted::new(&s.plans, &foreign.function, s.span).is_err());
    let mut e = call(&s).unwrap();
    assert!(
        e.terminator(
            &s.plans,
            &s.function,
            foreign.receipt.block,
            &current(&s),
            s.span
        )
        .is_err()
    );
    let mut e = call(&s).unwrap();
    let mut foreign_source = current(&s);
    foreign_source.insert(
        s.plans.entries[0].source.unwrap(),
        EntityId::Loan(foreign.receipt.source),
    );
    assert!(
        e.terminator(
            &s.plans,
            &s.function,
            s.receipt.block,
            &foreign_source,
            s.span
        )
        .is_err()
    );
    assert!(
        Emitted::new(&s.plans, &s.function, s.span)
            .unwrap()
            .finish(&s.plans, &s.function, s.span)
            .is_err()
    );
    assert!(
        call(&s)
            .unwrap()
            .finish(&s.plans, &s.function, s.span)
            .is_err()
    );
    let mut e = call(&s).unwrap();
    assert!(
        e.statement(
            &s.plans,
            Tag,
            &s.function,
            s.receipt.instruction,
            &current(&s),
            s.span
        )
        .is_err()
    );
    let mut e = call(&s).unwrap();
    e.terminator(&s.plans, &s.function, s.receipt.block, &current(&s), s.span)
        .unwrap();
    assert!(
        e.terminator(&s.plans, &s.function, s.receipt.block, &current(&s), s.span)
            .is_err()
    );
    for fault in 0..5 {
        let mut bad = s.clone();
        match fault {
            0 => {
                bad.function.instructions[0].operation = Operation::BorrowEnd {
                    loan: s.receipt.source,
                }
            }
            1 => bad.function.instructions[0].results = vec![EntityId::Loan(s.receipt.source)],
            2 => bad.plans.entries[0].callee = bad.plans.entries[0].caller,
            3 => bad.plans.entries[0].stage = 2,
            _ => bad.plans.entries[0].returned = false,
        }
        assert!(call(&bad).is_err(), "fault {fault}");
    }
    let mut bad = s.clone();
    bad.function.blocks[0].terminator.as_mut().unwrap().kind = TerminatorKind::BorrowReturn {
        loan: s.receipt.source,
    };
    let mut e = call(&bad).unwrap();
    assert!(
        e.terminator(
            &bad.plans,
            &bad.function,
            bad.receipt.block,
            &current(&bad),
            bad.span
        )
        .is_err()
    );
    let mut e = call(&s).unwrap();
    let mut rebound = current(&s);
    rebound.insert(
        s.plans.entries[0].result.unwrap(),
        EntityId::Loan(s.receipt.source),
    );
    assert!(
        e.terminator(&s.plans, &s.function, s.receipt.block, &rebound, s.span)
            .is_err()
    );
    let mut e = call(&s).unwrap();
    e.terminator(&s.plans, &s.function, s.receipt.block, &current(&s), s.span)
        .unwrap();
    let mut bad = s.clone();
    bad.function
        .instructions
        .push(bad.function.instructions[0].clone());
    assert!(e.finish(&bad.plans, &bad.function, bad.span).is_err());
}
