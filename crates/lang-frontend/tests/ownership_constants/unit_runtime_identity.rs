//! Constant declarations do not become runtime loan or capture identities.
use lang_frontend::ownership_checking::{UnitDropPoint, UnitDropTarget, UnitLoanTarget};

pub(super) fn repeated(text: &str) {
    for rooted in [false, true] {
        let (_, _, owned) = super::unit_materialization::checked(text, rooted);
        // Dependency reads have no runtime effects; each executed read has its own owner.
        assert_eq!(owned.ownership().loans().len(), 3);
        let mut owners = Vec::new();
        for loan in owned.ownership().loans() {
            let UnitLoanTarget::Temporary(owner) = loan.target() else {
                panic!("constant declaration must not be borrowed: {loan:?}");
            };
            if !owners.contains(owner) {
                owners.push(*owner);
            }
            assert_eq!(
                owned
                    .ownership()
                    .drops()
                    .iter()
                    .filter(|drop| {
                        drop.target() == UnitDropTarget::Temporary(*owner)
                            && drop.point() == UnitDropPoint::CallReturn(loan.call())
                    })
                    .count(),
                1
            );
        }
        assert_eq!(owners.len(), 3);
        assert_eq!(owned.ownership().drops().len(), 3);
        assert!(owned.ownership().captures().is_empty());
    }
}

pub(super) fn delivery(text: &str) {
    for rooted in [false, true] {
        let (_, _, owned) = super::unit_materialization::checked(text, rooted);
        assert!(owned.ownership().loans().is_empty());
        assert!(owned.ownership().drops().is_empty());
        assert!(owned.ownership().captures().is_empty());
    }
}

pub(super) fn closure(text: &str) {
    for rooted in [false, true] {
        let (_, _, owned) = super::unit_materialization::checked(text, rooted);
        assert!(owned.ownership().captures().is_empty());
        assert!(owned.ownership().loans().is_empty());
    }
}
