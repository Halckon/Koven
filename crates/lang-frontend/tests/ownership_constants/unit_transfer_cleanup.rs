//! Original constant String prefix cleanup contracts across control transfers.
use lang_frontend::ownership_checking::{UnitDropPoint, UnitDropTarget};

pub(super) fn binary(source: &str, left: &str, transfers: usize, normal: usize) {
    for rooted in [false, true] {
        let (sources, _, owned) = super::unit_materialization::checked(source, rooted);
        let drops = owned
            .ownership()
            .drops()
            .iter()
            .filter(|drop| {
                matches!(drop.target(), UnitDropTarget::Temporary(_))
                    && sources.slice(drop.value_origin()).unwrap() == left
            })
            .collect::<Vec<_>>();
        assert_eq!(
            drops
                .iter()
                .filter(|drop| matches!(drop.point(), UnitDropPoint::ControlTransfer(_)))
                .count(),
            transfers,
            "{source}: {drops:?}"
        );
        assert_eq!(
            drops
                .iter()
                .filter(|drop| matches!(drop.point(), UnitDropPoint::AfterBinaryOperands(_)))
                .count(),
            normal,
            "{source}: {drops:?}"
        );
        assert_eq!(
            drops.len(),
            transfers + normal,
            "{source}: no extra cleanup"
        );
    }
}

pub(super) fn interpolation(source: &str, transfers: usize, normal: usize) {
    for rooted in [false, true] {
        let (sources, _, owned) = super::unit_materialization::checked(source, rooted);
        let drops = owned
            .ownership()
            .drops()
            .iter()
            .filter(|drop| sources.slice(drop.value_origin()).unwrap() == "TEXT")
            .collect::<Vec<_>>();
        assert_eq!(
            drops
                .iter()
                .filter(|drop| matches!(drop.point(), UnitDropPoint::ControlTransfer(_)))
                .count(),
            transfers,
            "{source}: {drops:?}"
        );
        assert_eq!(
            drops
                .iter()
                .filter(|drop| matches!(drop.point(), UnitDropPoint::AfterExpression(_)))
                .count(),
            normal,
            "{source}: {drops:?}"
        );
        assert_eq!(drops.len(), transfers + normal);
    }
}
