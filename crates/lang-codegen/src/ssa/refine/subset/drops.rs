//! Every drop fact of the function must have a supported point and target in the subset.
use lang_frontend::{
    ownership_checking::{UnitDropPoint, UnitDropTarget},
    type_checking::{CallableResultSource, Copyability, UnitCallTarget, UnitExpressionId},
};

use super::Checker;
use crate::ssa::refine::RefineBody;

impl Checker<'_, '_> {
    /// Drop facts must name plain bindings or temporaries at points the builder emits.
    pub(super) fn drops_supported(&self) -> bool {
        self.inputs.owned.drops().iter().all(|fact| {
            let point_ok = match fact.point() {
                // This cleanup must run before the selected arm starts.
                UnitDropPoint::BranchEntry { .. } => false,
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
                UnitDropTarget::ReplacedCapture { .. } => false,
                UnitDropTarget::This(owner) => self.inputs.receiver.is_some_and(|receiver| {
                    receiver.owner == owner
                        && receiver.mode == lang_frontend::type_checking::ParameterMode::Value
                }),
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
            };
            !self.fact_in_function(fact.value_origin())
                || self.inside_lambda(fact.value_origin())
                || (point_ok && target_ok)
        }) && self
            .inputs
            .owned
            .conditional_receiver_drops()
            .iter()
            .all(|fact| {
                !self.fact_in_function(fact.value_origin())
                    || self.inside_lambda(fact.value_origin())
                    || (!match fact.point() {
                        UnitDropPoint::AfterExpression(expression)
                        | UnitDropPoint::AfterBinaryOperands(expression) => {
                            self.branch_form.contains(&expression)
                        }
                        _ => false,
                    } && self.inputs.receiver.is_some_and(|receiver| {
                        receiver.owner == fact.owner()
                            && receiver.mode == lang_frontend::type_checking::ParameterMode::Value
                            && receiver.ty == self.c(fact.receiver_type())
                    }))
            })
    }

    /// A pending argument a jump may abandon: a MoveOnly named owner or non-borrowed
    /// temporary moved into a Value argument. A borrowed temporary stays outside unless the
    /// legacy path can no longer lower its call: a function-value call (captured closures
    /// lower only here), a `RangeView` carrier call, or a borrow-result call proven not to
    /// dispatch. The ordinary exit ends those loans before dropping the temporary.
    fn pending_owner(&self, expression: UnitExpressionId) -> bool {
        if expression.source_unit() != self.inputs.source_unit {
            return false;
        }
        let Ok(node) = self
            .inputs
            .parsed
            .ast()
            .expressions()
            .get(expression.expression())
        else {
            return false;
        };
        let span = node.span();
        // The innermost call lending the temporary decides.
        let expressions = self.inputs.parsed.ast().expressions();
        let lender = self
            .inputs
            .owned
            .loans()
            .iter()
            .filter_map(|loan| {
                (loan.argument().source_unit() == self.inputs.source_unit)
                    .then(|| expressions.get(loan.argument().expression()).ok())
                    .flatten()
                    .filter(|arg| {
                        arg.span().start() <= span.start() && span.end() <= arg.span().end()
                    })
                    .map(|arg| (arg.span().end() - arg.span().start(), loan.call()))
            })
            .min_by_key(|(width, _)| *width);
        let ty = self.expression_type(expression.expression());
        let borrowed = lender.is_some_and(|(_, call)| {
            let abandoned_callback = ty.is_some_and(|ty| self.view_function(ty))
                && self.inputs.owned.call_may_dispatch(call) == Some(false);
            !abandoned_callback
                && !self.abandoned_borrow_result_call(call)
                && !self.inputs.typed.call(call).is_some_and(|call| {
                    call.target() == UnitCallTarget::FunctionValue
                        || matches!(call.result_source(), CallableResultSource::Carrier(_))
                })
        });
        !borrowed && ty.is_some_and(|ty| self.copyability_of(ty) == Copyability::MoveOnly)
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
