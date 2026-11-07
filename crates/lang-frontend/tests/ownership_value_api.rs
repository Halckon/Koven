//! Compile-time external-consumer witness for the additive V1 projection API.
use lang_frontend::ownership_checking::{
    UnitOwnerChoiceIdV1, UnitOwnerChoiceInputV1, UnitOwnerChoiceV1, UnitOwnerId,
    UnitOwnerObservation, UnitOwnerProducerCatalogV1, UnitOwnerValueEventV1,
    UnitOwnerValueObservationV1, UnitOwnerValueV1,
};

#[test]
fn value_api_is_nameable_and_matchable_without_private_modules() {
    fn require_eq<T: Eq>() {}
    require_eq::<UnitOwnerObservation>();
    let project: fn(&UnitOwnerObservation) -> Option<UnitOwnerValueObservationV1> =
        UnitOwnerObservation::value_observation_v1;
    let catalog: fn(&UnitOwnerValueObservationV1) -> &UnitOwnerProducerCatalogV1 =
        UnitOwnerValueObservationV1::producers;
    let select: fn(&UnitOwnerValueObservationV1) -> Option<&UnitOwnerChoiceV1> =
        UnitOwnerValueObservationV1::choice;
    let inputs: fn(&UnitOwnerChoiceV1) -> &[UnitOwnerChoiceInputV1; 2] = UnitOwnerChoiceV1::inputs;
    let identity: fn(&UnitOwnerChoiceV1) -> &UnitOwnerChoiceIdV1 = UnitOwnerChoiceV1::id;
    let owner: fn(&UnitOwnerChoiceInputV1) -> UnitOwnerId = UnitOwnerChoiceInputV1::owner;
    fn value_kind(value: &UnitOwnerValueV1) -> bool {
        match value {
            UnitOwnerValueV1::Definition(_) => false,
            UnitOwnerValueV1::Choice(_) => true,
        }
    }
    fn event_kind(event: &UnitOwnerValueEventV1) -> bool {
        match event {
            UnitOwnerValueEventV1::Create(..) => false,
            UnitOwnerValueEventV1::Forward(_, _, value)
            | UnitOwnerValueEventV1::Move(_, _, value)
            | UnitOwnerValueEventV1::Bind(_, _, value)
            | UnitOwnerValueEventV1::Pending(_, _, value)
            | UnitOwnerValueEventV1::Drop(_, _, value) => value_kind(value),
            UnitOwnerValueEventV1::Swap {
                left_value,
                right_value,
                ..
            } => value_kind(left_value) || value_kind(right_value),
            UnitOwnerValueEventV1::Replace {
                previous, incoming, ..
            } => value_kind(previous) || value_kind(incoming),
        }
    }
    fn selection_equal(left: &UnitOwnerChoiceIdV1, right: &UnitOwnerChoiceIdV1) -> bool {
        left == right
    }
    let _ = (
        project,
        catalog,
        select,
        inputs,
        identity,
        owner,
        event_kind,
        selection_equal,
    );
}
