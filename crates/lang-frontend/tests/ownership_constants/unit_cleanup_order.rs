//! Same-source constant String cleanup order contracts in both Unit layouts.
use lang_frontend::ownership_checking::{UnitDropPoint, UnitDropTarget};

pub(super) fn binary(text: &str) {
    for rooted in [false, true] {
        let (sources, _, owned) = super::unit_materialization::checked(text, rooted);
        assert_eq!(owned.ownership().drops().len(), 4);
        assert!(owned.ownership().drops().iter().all(|drop| {
            matches!(drop.target(), UnitDropTarget::Temporary(_))
                && matches!(drop.point(), UnitDropPoint::AfterBinaryOperands(_))
        }));
        let drops = owned.ownership().drops();
        assert_eq!(drops[0].point(), drops[1].point());
        assert!(drops[0].value_origin().start() > drops[1].value_origin().start());
        assert_eq!(sources.slice(drops[0].value_origin()).unwrap(), "TEXT");
        assert_eq!(sources.slice(drops[1].value_origin()).unwrap(), "TEXT");
    }
}

pub(super) fn nested(text: &str) {
    for rooted in [false, true] {
        let (sources, _, owned) = super::unit_materialization::checked(text, rooted);
        let transfers = owned
            .ownership()
            .drops()
            .iter()
            .filter(|drop| {
                matches!(drop.point(), UnitDropPoint::ControlTransfer(_))
                    && sources.slice(drop.value_origin()).unwrap() == "TEXT"
            })
            .collect::<Vec<_>>();
        assert_eq!(transfers.len(), 2);
        assert_eq!(transfers[0].point(), transfers[1].point());
        assert!(transfers[0].value_origin().start() > transfers[1].value_origin().start());
        for transfer in transfers {
            assert_eq!(
                owned
                    .ownership()
                    .drops()
                    .iter()
                    .filter(|drop| {
                        drop.target() == transfer.target()
                            && matches!(drop.point(), UnitDropPoint::AfterBinaryOperands(_))
                    })
                    .count(),
                1
            );
        }
    }
}

pub(super) fn interpolation(text: &str) {
    for rooted in [false, true] {
        let (sources, _, owned) = super::unit_materialization::checked(text, rooted);
        let plans = owned.materializations();
        assert_eq!(plans.len(), 2);
        assert_eq!(
            owned.ownership().drops().len(),
            2,
            "{:?}",
            owned.ownership().drops()
        );
        assert_eq!(
            owned.ownership().drops()[0].point(),
            owned.ownership().drops()[1].point()
        );
        assert!(
            owned.ownership().drops()[0].value_origin().start()
                > owned.ownership().drops()[1].value_origin().start()
        );
        for plan in plans {
            let owner = plan.descriptor().expression();
            let drops = owned
                .ownership()
                .drops()
                .iter()
                .filter(|drop| drop.target() == UnitDropTarget::Temporary(owner))
                .collect::<Vec<_>>();
            assert_eq!(drops.len(), 1);
            assert!(matches!(
                drops[0].point(),
                UnitDropPoint::AfterExpression(_)
            ));
            assert_eq!(sources.slice(drops[0].value_origin()).unwrap(), "TEXT");
        }
    }
}
