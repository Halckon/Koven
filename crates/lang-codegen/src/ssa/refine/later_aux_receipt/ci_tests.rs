//! CI-only controls over genuine two-owner append events. Compile locally, never execute.
use super::*;
use crate::ssa::unit_lower_test_support;
use lang_frontend::{
    name_resolution::SourceUnitInput, ownership_checking::UnitLoanActionObservation,
    source::SourceMap, type_checking::standard_environments,
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
fn capture(second: bool) -> (Snapshot, UnitLoanActionObservation) {
    let (params, args) = if second {
        ("auxiliary: String, source: String", "auxiliary, owner")
    } else {
        ("source: String, auxiliary: String", "owner, auxiliary")
    };
    let text = format!(
        "fun view({params}): borrow String from source = source\nfun consume(item: String): Unit {{}}\nfun run(): Unit {{ val owner = \"kept\"; val auxiliary = \"other\"; borrow val item = view({args}); consume(item); consume(auxiliary) }}"
    );
    let mut sources = SourceMap::new();
    let (source, parsed) = unit_lower_test_support::parsed(&mut sources, "ci-later-aux.ko", &text);
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
    (
        snapshots.pop().unwrap(),
        owned.ownership().loan_action_observations()[0].clone(),
    )
}
fn current(s: &Snapshot) -> BTreeMap<Slot, EntityId> {
    slots(&s.plans.entries[0])
        .into_iter()
        .map(Option::unwrap)
        .zip(s.receipt.entities.iter().copied())
        .collect()
}
fn replay(s: &Snapshot, count: usize) -> Result<Emitted, LoweringError> {
    let mut emitted = Emitted::new(&s.plans, &s.function, s.span)?;
    let current = current(s);
    for (role, id) in s.receipt.instructions.iter().take(count).enumerate() {
        emitted.statement(
            &s.plans,
            Tag {
                plan: 0,
                role: role.try_into().unwrap(),
            },
            &s.function,
            *id,
            &current,
            s.span,
        )?;
    }
    Ok(emitted)
}
#[test]
fn ci_only_later_aux_requires_owned_catalog_and_each_authority() {
    let (s, catalog) = capture(false);
    let (_, foreign) = capture(false);
    let region = &catalog.later_auxiliary_binding_regions()[0];
    let ids = region.action_ids().collect::<Vec<_>>();
    super::super::admission::catalog_actions(&catalog, region, &ids, s.span).unwrap();
    assert!(super::super::admission::catalog_actions(&foreign, region, &ids, s.span).is_err());
    let foreign_ids = foreign.later_auxiliary_binding_regions()[0]
        .action_ids()
        .collect::<Vec<_>>();
    assert!(
        super::super::admission::catalog_actions(&catalog, region, &foreign_ids, s.span).is_err()
    );
    for fault in 0..18 {
        let mut p = s.plans.clone();
        let e = &mut p.entries[0];
        match fault {
            0 => e.activated = false,
            1 => e.source_authorized = false,
            2 => e.auxiliary_authorized = false,
            3 => e.committed = false,
            4 => e.consumer_authorized = false,
            5 => e.retired = false,
            6 => e.drop_authorized = false,
            7 => e.auxiliary_drop_authorized = false,
            8 => e.completed = false,
            9 => e.later_authorized = false,
            10 => e.auxiliary_owner = e.owner,
            11 => e.auxiliary_root = e.root,
            12 => e.auxiliary_drop_index = e.drop_index,
            13 => e.source_index = 2,
            14 => e.later_place = e.auxiliary_place,
            15 => e.later_loan = e.auxiliary,
            16 => e.later_loan = e.result,
            _ => e.stage = 14,
        }
        assert!(
            Emitted::new(&p, &s.function, s.span).is_err(),
            "fault {fault}"
        );
    }
}
#[test]
fn ci_only_later_aux_preserves_two_owner_and_reacquisition_identity() {
    for second in [false, true] {
        let (s, _) = capture(second);
        replay(&s, 15)
            .unwrap()
            .finish(&s.plans, &s.function, s.span)
            .unwrap();
        let p = &s.plans.entries[0];
        assert_eq!(p.source_index, usize::from(second));
        assert_eq!(s.receipt.entities.len(), 9);
        assert_ne!(s.receipt.entities[4], s.receipt.entities[7]);
        assert_ne!(s.receipt.entities[5], s.receipt.entities[8]);
        let first = replay(&s, 1).unwrap();
        let mut mapping = current(&s);
        let later_owner = if second { p.owner } else { p.auxiliary_owner };
        mapping.insert(
            later_owner.unwrap(),
            s.receipt.entities[usize::from(second)],
        );
        assert!(first.alias(&s.plans, &mapping, s.span).is_err());
        let before_later = replay(&s, 10).unwrap();
        let mut mapping = current(&s);
        mapping.insert(p.auxiliary_owner.unwrap(), s.receipt.entities[0]);
        assert!(before_later.alias(&s.plans, &mapping, s.span).is_err());
        for (slot, wrong) in [
            (p.later_place, s.receipt.entities[4]),
            (p.later_loan, s.receipt.entities[5]),
        ] {
            let fixed = replay(&s, 12).unwrap();
            let mut mapping = current(&s);
            mapping.insert(slot.unwrap(), wrong);
            assert!(fixed.alias(&s.plans, &mapping, s.span).is_err());
        }
        let mut reversed = s.clone();
        reversed.plans.entries[0].source_index = usize::from(!second);
        assert!(replay(&reversed, 1).is_err());
    }
}
#[test]
fn ci_only_later_aux_rejects_missing_reordered_and_foreign_operations() {
    let (s, _) = capture(false);
    for (prefix, role, selected) in [
        (9, 9, 14),
        (14, 14, 9),
        (10, 10, 0),
        (11, 11, 1),
        (12, 12, 6),
        (13, 13, 5),
    ] {
        let mut emitted = replay(&s, prefix).unwrap();
        assert!(
            emitted
                .statement(
                    &s.plans,
                    Tag { plan: 0, role },
                    &s.function,
                    s.receipt.instructions[selected],
                    &current(&s),
                    s.span
                )
                .is_err()
        );
    }
    for count in [6, 7, 9, 10, 12, 13, 14] {
        assert!(
            replay(&s, count)
                .unwrap()
                .finish(&s.plans, &s.function, s.span)
                .is_err()
        );
    }
    let mut emitted = replay(&s, 15).unwrap();
    emitted.receipts[0].instructions.swap(9, 14);
    assert!(emitted.finish(&s.plans, &s.function, s.span).is_err());
    let (foreign, _) = capture(false);
    let mut emitted = replay(&s, 15).unwrap();
    emitted.receipts[0].ids = foreign.receipt.ids;
    assert!(emitted.finish(&s.plans, &s.function, s.span).is_err());
    let mut emitted = replay(&s, 15).unwrap();
    emitted.receipts[0].entities.swap(5, 8);
    assert!(emitted.finish(&s.plans, &s.function, s.span).is_err());
}
#[test]
fn ci_only_later_aux_rolls_back_before_original_verifier_publication() {
    let (s, _) = capture(false);
    let (_, receipts) = observe(|| {
        let _guard = UnitPublication::begin();
        replay(&s, 15)
            .unwrap()
            .finish(&s.plans, &s.function, s.span)
            .unwrap();
        Err::<(), ()>(())
    });
    assert!(receipts.is_empty());
    let (_, receipts) = observe(|| {
        let guard = UnitPublication::begin();
        replay(&s, 15)
            .unwrap()
            .finish(&s.plans, &s.function, s.span)
            .unwrap();
        guard.commit();
    });
    assert_eq!(receipts.len(), 1);
    assert_eq!(receipts[0].instructions.len(), 15);
    assert_eq!(receipts[0].entities.len(), 9);
}
