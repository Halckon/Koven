//! Atomic borrowed root vector at one actual control boundary.
//! This does not publish incoming coverage or authorize a nested transaction.
use super::*;

pub(super) struct ControlRootFrame<'a> {
    control: UnitStatementId,
    values: &'a [OwnedValue],
}
impl<'a> ControlRootFrame<'a> {
    /// Capture every root or none. Preserve the original vector order/correlation.
    pub(super) fn capture(control: UnitStatementId, state: &'a ValueState) -> Option<Self> {
        let values = boundary_values(state)?;
        for (index, value) in values.iter().enumerate() {
            let owner = value.owner.as_ref()?;
            if value.symbol.source_unit() != control.source_unit()
                || values[..index].iter().any(|previous| {
                    previous.symbol == value.symbol
                        || previous.owner.as_ref() == Some(owner)
                        || previous
                            .owner
                            .as_ref()
                            .is_none_or(|other| other.callable != owner.callable)
                })
            {
                return None;
            }
        }
        Some(Self { control, values })
    }
    /// A caller must present the same source-qualified control, not a nested sibling.
    pub(super) fn roots_for(&self, control: UnitStatementId) -> Option<&'a [OwnedValue]> {
        (self.control == control).then_some(self.values)
    }
    /// Current single-root products retain their exact admission restriction.
    pub(super) fn single_for(&self, control: UnitStatementId) -> Option<&'a OwnedValue> {
        let [value] = self.roots_for(control)? else {
            return None;
        };
        Some(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ownership_checking::compilation_unit::formed_tests::Fixture;
    use crate::parser::{FunctionBody, FunctionForm};

    // Structural model only: these bound IDs come from real analysis, but the tuple
    // is assembled for this helper test. It is never published as a planner snapshot.
    fn model() -> (UnitStatementId, UnitStatementId, ValueState) {
        let fixture = Fixture::new(&[(
            "frame.ko",
            r#"fun run() {
            val left = "left"
            val right = "right"
            val finalLeft = left
            val finalRight = right
        }"#,
        )]);
        let owned = fixture.shared();
        assert!(owned.diagnostics().is_empty());
        assert!(owned.deferred().is_empty());
        assert!(owned.clone().validate().is_ok());
        let observation = &owned.owner_observations()[0];
        assert_eq!(observation.status(), UnitOwnerObservationStatus::Complete);
        let ast = fixture.parsed[0].ast();
        let Item::Function { form, .. } = ast
            .items()
            .get(fixture.parsed[0].roots()[0])
            .unwrap()
            .payload()
        else {
            panic!("function");
        };
        let body = match form {
            FunctionForm::ImplicitUnitBlock(body)
            | FunctionForm::Explicit {
                body: FunctionBody::Block(body),
                ..
            } => *body,
            _ => panic!("block"),
        };
        let Statement::Block { elements } = ast.statements().get(body).unwrap().payload() else {
            panic!("block");
        };
        let values = observation
            .events()
            .iter()
            .filter_map(|event| match event {
                UnitOwnerTransportEvent::Binding {
                    symbol,
                    initializer,
                    owner,
                } => {
                    let span = ast
                        .expressions()
                        .get(initializer.expression())
                        .unwrap()
                        .span();
                    Some(OwnedValue {
                        symbol: *symbol,
                        owner: Some(owner.clone()),
                        origin: span,
                        declaration: span,
                        scope_depth: 1,
                    })
                }
                _ => None,
            })
            .take(2)
            .collect();
        (
            UnitStatementId::new(fixture.source(0), body),
            UnitStatementId::new(fixture.source(0), elements[0]),
            ValueState {
                values,
                ..ValueState::default()
            },
        )
    }

    #[test]
    fn root_frame_model_preserves_whole_vector_and_control_identity() {
        let (control, other, state) = model();
        let frame = ControlRootFrame::capture(control, &state).unwrap();
        assert_eq!(frame.roots_for(control), Some(state.values.as_slice()));
        assert_eq!(frame.roots_for(control).unwrap().len(), 2);
        assert!(frame.single_for(control).is_none());
        assert!(frame.roots_for(other).is_none());
    }

    #[test]
    fn ci_only_root_frame_rejects_partial_and_aliased_vectors() {
        let (control, _, state) = model();
        let mut missing = state.clone();
        missing.values[1].owner = None;
        assert!(ControlRootFrame::capture(control, &missing).is_none());
        let mut alias = state.clone();
        alias.values[1].owner = alias.values[0].owner.clone();
        assert!(ControlRootFrame::capture(control, &alias).is_none());
        let mut duplicate = state.clone();
        duplicate.values[1].symbol = duplicate.values[0].symbol;
        assert!(ControlRootFrame::capture(control, &duplicate).is_none());
    }
}
