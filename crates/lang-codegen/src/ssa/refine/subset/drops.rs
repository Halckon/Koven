//! Every drop fact of the function must have a supported point and target in the subset.
use lang_frontend::{
    ownership_checking::{UnitDropPoint, UnitDropTarget},
    parser::Expression,
    type_checking::{Copyability, UnitExpressionId},
};

use super::Checker;
use crate::ssa::refine::RefineBody;

impl Checker<'_, '_> {
    /// Drop facts must name plain bindings or temporaries at points the builder emits.
    pub(super) fn drops_supported(&self) -> bool {
        self.inputs.owned.drops().iter().all(|fact| {
            let point_ok = match fact.point() {
                UnitDropPoint::AfterExpression(expression)
                | UnitDropPoint::AfterBinaryOperands(expression) => {
                    !self.branch_form.contains(&expression)
                }
                UnitDropPoint::BeforeReplacement(assignment) => {
                    self.replacements.contains(&assignment)
                        && matches!(fact.target(), UnitDropTarget::ReplacedField { .. })
                }
                UnitDropPoint::LambdaEntry(lambda) => {
                    matches!(&self.inputs.body, RefineBody::Lambda(thunk) if thunk.expression == lambda)
                }
                UnitDropPoint::AfterReplacement(assignment) => {
                    self.element_replacements.contains(&assignment)
                        && fact.target() == UnitDropTarget::ReplacedElement(assignment)
                }
                // A jump out of a call argument drops the earlier MoveOnly arguments it
                // abandons; temporaries holding loans stay outside.
                UnitDropPoint::ControlTransfer(_) => match fact.target() {
                    UnitDropTarget::Temporary(expression) => self.pending_owner(expression),
                    _ => true,
                },
                _ => true,
            };
            let target_ok = match fact.target() {
                UnitDropTarget::Named(_)
                | UnitDropTarget::Temporary(_)
                | UnitDropTarget::ReplacedField { .. } => true,
                UnitDropTarget::ReplacedElement(assignment) => {
                    self.element_replacements.contains(&assignment)
                }
                // Released by the drop glue of a closure formed in this function.
                UnitDropTarget::Captured { closure, .. } => {
                    self.inputs.callables.contains_key(&closure)
                        || matches!(
                            &self.inputs.body,
                            RefineBody::Lambda(thunk) if thunk.expression == closure
                        )
                }
                _ => false,
            };
            !self.fact_in_function(fact.value_origin())
                || self.inside_lambda(fact.value_origin())
                || (point_ok && target_ok)
        }) && self
            .inputs
            .owned
            .conditional_receiver_drops()
            .iter()
            .all(|fact| !self.fact_in_function(fact.value_origin()))
    }

    /// A pending argument a jump may abandon: a MoveOnly named owner moved into a Value
    /// argument (borrowed temporaries stay outside: their loan would be abandoned too).
    fn pending_owner(&self, expression: UnitExpressionId) -> bool {
        expression.source_unit() == self.inputs.source_unit
            && self
                .inputs
                .parsed
                .ast()
                .expressions()
                .get(expression.expression())
                .is_ok_and(|node| matches!(node.payload(), Expression::Name))
            && self
                .expression_type(expression.expression())
                .is_some_and(|ty| self.copyability_of(ty) == Copyability::MoveOnly)
    }

    fn fact_in_function(&self, origin: lang_frontend::source::Span) -> bool {
        self.inputs.scope_span().is_some_and(|item| {
            origin.source_id() == item.source_id()
                && item.start() <= origin.start()
                && origin.end() <= item.end()
                && (!matches!(self.inputs.body, RefineBody::Lambda(_)) || origin != item)
        })
    }
}
