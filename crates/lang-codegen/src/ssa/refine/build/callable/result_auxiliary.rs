//! Concrete callable ABI for the bounded borrowed-result auxiliary operand.
use lang_frontend::{
    ast::ExpressionId,
    ownership_checking::{LoanKind as FrontendLoanKind, UnitLoanTarget},
    parser::Expression,
    source::Span,
    type_checking::{BuiltinType, UnitExpressionId, UnitTypeId},
};

use super::{Builder, LentArgument, ROp};
use crate::ssa::{
    lowering_support::LoweringError,
    model::{ClosureCaptureMode, EntityType, LoanKind, SsaTypeKind},
};

impl Builder<'_, '_> {
    /// A view is never the type of a root place. Lend the actual owned closure
    /// and adapt its shared loan to the already-planned callable view ABI.
    pub(in crate::ssa::refine::build) fn borrowed_result_callable_auxiliary(
        &mut self,
        call: UnitExpressionId,
        argument: ExpressionId,
        parameter: UnitTypeId,
        span: Span,
    ) -> Result<Option<LentArgument>, LoweringError> {
        let Some(&view) = self.inputs.views.get(&self.c(parameter)) else {
            return Ok(None);
        };
        let (payload, argument_span) = self.node(argument)?;
        if !matches!(payload, Expression::Name) {
            return Ok(None);
        }
        let symbol = self.reference(argument_span)?;
        let Some(&owner) = self.bindings.get(&symbol) else {
            return Ok(None);
        };
        let EntityType::Value(closure) = self.body.slots[owner.0] else {
            return Ok(None);
        };
        let Some(SsaTypeKind::ConcreteClosure {
            signature,
            captures,
            ..
        }) = self.types.ssa_types.get(closure.index())
        else {
            return Ok(None);
        };
        let [capture] = captures.as_slice() else {
            return Ok(None);
        };
        let Some(int) = self.inputs.typed.types().builtin(BuiltinType::Int) else {
            return Ok(None);
        };
        if capture.mode != ClosureCaptureMode::Shared
            || !signature.parameters.is_empty()
            || signature.never_returns
            || signature.returns != [self.ssa_type(int, span)?]
            || !matches!(self.types.ssa_types.get(view.index()), Some(SsaTypeKind::CallableView { signature: abi }) if abi == signature)
        {
            return Ok(None);
        }
        let argument_id = self.unit(argument);
        let mut loans = self
            .inputs
            .owned
            .loans()
            .iter()
            .filter(|loan| loan.call() == call && loan.argument() == argument_id);
        let Some(fact) = loans.next() else {
            return Ok(None);
        };
        if loans.next().is_some()
            || fact.kind() != FrontendLoanKind::Shared
            || fact.begin_span() != argument_span
            || fact.end_span() != self.span(call.expression())
            || !matches!(fact.target(), UnitLoanTarget::Place(place) if place.is_root() && place.root() == symbol)
        {
            return Ok(None);
        }
        let parent = self.lend(owner, LoanKind::Shared, argument_span)?.loan;
        let child = self.value(
            ROp::CallableViewOf { source: parent },
            EntityType::Loan {
                kind: LoanKind::Shared,
                target: view,
            },
            span,
        )?;
        Ok(Some((child, vec![parent, child], Some(argument))))
    }
}
