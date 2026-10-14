//! Narrow single-file Borrow receiver admission and the shared readonly entry view.
use super::{EntityId, EntityType, LoanId, LoanKind, LoweringError, LoweringErrorKind, error};
use lang_frontend::{
    ast::ExpressionId,
    ownership_checking::LoanTarget,
    parser::Expression,
    source::Span,
    type_checking::{
        BuiltinType, CallDescriptor, CallReceiverOrigin, CallableDescriptor, CallableResultSource,
        CallableTarget, NominalId, NominalKind, ParameterMode, TypeId, TypeKind, TypedFile,
    },
};

pub(super) fn admitted(
    typed: &TypedFile,
    callable: &CallableDescriptor,
) -> Option<(NominalId, TypeId)> {
    let owner = callable.owner()?;
    let receiver = callable.receiver()?;
    let TypeKind::Nominal { nominal, arguments } = typed.types().get(receiver.ty())? else {
        return None;
    };
    let descriptor = typed
        .nominals()
        .iter()
        .find(|descriptor| descriptor.id() == owner)?;
    (receiver.mode() == ParameterMode::Borrow
        && *nominal == owner
        && arguments.is_empty()
        && descriptor.kind() == NominalKind::ValueClass
        && descriptor.type_parameters().is_empty()
        && descriptor.interfaces().is_empty()
        && callable.type_parameters().is_empty()
        && callable.result_source() == CallableResultSource::Owned
        && matches!(
            super::builtin_type(typed, callable.return_type()),
            Some(BuiltinType::Int | BuiltinType::Boolean)
        )
        && callable
            .parameters()
            .iter()
            .all(|parameter| parameter.mode != ParameterMode::Inout))
    .then_some((owner, receiver.ty()))
}

pub(super) struct Operand {
    argument: ExpressionId,
    loan: LoanId,
}

impl super::ExpressionLowerer<'_> {
    pub(super) fn lower_readonly_call_receiver(
        &mut self,
        expression: ExpressionId,
        callee: ExpressionId,
        descriptor: &CallDescriptor,
        span: Span,
    ) -> Result<Option<Operand>, LoweringError> {
        let Some(receiver) = descriptor.receiver() else {
            return Ok(None);
        };
        let CallableTarget::Source(symbol) = descriptor.target() else {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        };
        let callable = self
            .typed
            .callables()
            .iter()
            .find(|callable| callable.symbol() == symbol)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let (owner, ty) = admitted(self.typed, callable)
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
        if receiver.ty() != ty
            || receiver.mode() != ParameterMode::Borrow
            || !descriptor.instance().type_arguments().is_empty()
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let (argument, loan, created) = match receiver.origin() {
            CallReceiverOrigin::Expression(argument) => {
                let actual = self.field_replace_ungroup(argument, span)?;
                let node = self
                    .parsed
                    .ast()
                    .expressions()
                    .get(actual)
                    .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
                if self.typed.aggregate_projection(actual).is_some() {
                    return Err(error(LoweringErrorKind::UnsupportedNode, span));
                }
                if self
                    .references
                    .get(&super::span_key(node.span()))
                    .is_some_and(|symbol| {
                        self.owned
                            .borrow_results()
                            .bindings()
                            .iter()
                            .any(|binding| binding.binding() == *symbol)
                    })
                {
                    return Err(error(LoweringErrorKind::UnsupportedNode, span));
                }
                let (loan, created) = self.lower_borrow_argument(
                    expression,
                    argument,
                    self.expression_span(argument)?,
                )?;
                (argument, loan, created)
            }
            CallReceiverOrigin::ImplicitThis(actual) => {
                let fact = self
                    .owned
                    .loan_begin(callee)
                    .filter(|fact| fact.call() == expression)
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                let Some((current_owner, current_type, loan)) = self.readonly_receiver else {
                    return Err(error(LoweringErrorKind::UnsupportedNode, span));
                };
                if actual != owner
                    || current_owner != owner
                    || current_type != ty
                    || fact.target() != &LoanTarget::This(owner)
                    || fact.kind() != lang_frontend::ownership_checking::LoanKind::Shared
                {
                    return Err(error(LoweringErrorKind::MissingFact, span));
                }
                (callee, loan, false)
            }
        };
        self.pending_call_loans.insert(
            (expression.index(), argument.index()),
            created.then_some(loan),
        );
        Ok(Some(Operand { argument, loan }))
    }

    pub(super) fn current_readonly_call_receiver(
        &self,
        expression: ExpressionId,
        receiver: Option<Operand>,
        span: Span,
    ) -> Result<Option<EntityId>, LoweringError> {
        receiver
            .map(|receiver| {
                let current = self
                    .pending_call_loans
                    .get(&(expression.index(), receiver.argument.index()))
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                Ok(EntityId::Loan(current.unwrap_or(receiver.loan)))
            })
            .transpose()
    }
}

impl super::ExpressionLowerer<'_> {
    /// Resolve only the validated readonly entry receiver and projections rooted in it.
    /// Created loans are returned in outer-to-inner order for reverse cleanup.
    pub(super) fn readonly_view(
        &mut self,
        expression: lang_frontend::ast::ExpressionId,
    ) -> Result<Option<(super::LoanId, Vec<super::LoanId>)>, LoweringError> {
        use crate::ssa::model::{EntityId, Operation};
        use lang_frontend::type_checking::AggregateProjectionReceiver;
        let Some((owner, receiver_type, receiver)) = self.readonly_receiver else {
            return Ok(None);
        };
        let span = self.expression_span(expression)?;
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
        if let Expression::Group { expression } = node.payload() {
            return self.readonly_view(*expression);
        }
        if matches!(node.payload(), Expression::This) {
            if self.typed.expression_type(expression) != Some(receiver_type) {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            return Ok(Some((receiver, Vec::new())));
        }
        let Some(projection) = self.typed.aggregate_projection(expression) else {
            return Ok(None);
        };
        let (base, mut created, base_type) = match projection.receiver() {
            AggregateProjectionReceiver::This(nominal) if nominal == owner => {
                (receiver, Vec::new(), receiver_type)
            }
            AggregateProjectionReceiver::This(_) => {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            AggregateProjectionReceiver::Expression(expression) => {
                let Some((base, created)) = self.readonly_view(expression)? else {
                    return Ok(None);
                };
                let ty = self
                    .typed
                    .expression_type(expression)
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                (base, created, ty)
            }
        };
        let Some(TypeKind::Nominal { nominal, arguments }) = self.typed.types().get(base_type)
        else {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        };
        let nominal = self
            .typed
            .nominals()
            .iter()
            .find(|value| value.id() == *nominal)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let entry_kind = self
            .typed
            .nominals()
            .iter()
            .find(|value| value.id() == owner)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?
            .kind();
        let supported = match entry_kind {
            // Keep the pre-existing destructor projection boundary unchanged.
            NominalKind::Class => nominal.kind() == NominalKind::Class,
            NominalKind::ValueClass => {
                nominal.id() == owner
                    && created.is_empty()
                    && matches!(
                        super::builtin_type(self.typed, projection.ty()),
                        Some(BuiltinType::Int | BuiltinType::Boolean)
                    )
            }
            _ => false,
        };
        if !arguments.is_empty() || !supported {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        let field = nominal
            .fields()
            .iter()
            .position(|field| *field == projection.field())
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let target = self.expression_ssa_type(expression, span)?;
        let operation = match nominal.kind() {
            NominalKind::Class => Operation::SharedHeapFieldLoan { base, field },
            NominalKind::ValueClass => Operation::SharedFieldLoan { base, field },
            _ => return Err(error(LoweringErrorKind::UnsupportedNode, span)),
        };
        let (_, results) = self.append(
            operation,
            vec![EntityType::Loan {
                kind: LoanKind::Shared,
                target,
            }],
            span,
        )?;
        let EntityId::Loan(loan) = results[0] else {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        };
        created.push(loan);
        Ok(Some((loan, created)))
    }
}
