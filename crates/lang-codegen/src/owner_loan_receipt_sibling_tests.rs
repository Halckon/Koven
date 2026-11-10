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
