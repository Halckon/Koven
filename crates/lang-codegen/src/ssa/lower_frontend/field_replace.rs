//! Trusted direct class-field exchange, separate from whole-root owner replacement.
use lang_frontend::{
    ownership_checking::{LoanKind as FrontendLoanKind, OwnershipPrimitiveValueTransfer},
    parser::CallArgument,
    type_checking::{AggregateProjectionKind, AggregateProjectionReceiver, ExpressionCategory},
};

use super::*;

impl ExpressionLowerer<'_> {
    pub(super) fn lower_field_replace(
        &mut self,
        call: ExpressionId,
        first: &CallArgument,
        second: &CallArgument,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let descriptor = self
            .typed
            .ownership_primitive(call)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let ty = self.resolve_type(descriptor.value_type(), span)?;
        let target = *self
            .type_ids
            .get(&ty)
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
        let (symbol, field, owner_type) =
            self.begin_field_replace(call, first.value, target, first.span)?;
        let mut replacement = match self.lower(second.value)? {
            LoweredValue::Diverged => return Ok(LoweredValue::Diverged),
            LoweredValue::Value(value) => value,
            LoweredValue::Unit => {
                if builtin_type(self.typed, ty) != Some(BuiltinType::Unit) {
                    return Err(error(LoweringErrorKind::MissingFact, second.span));
                }
                let (_, results) = self.append(
                    Operation::Constant(ScalarConstant::Unit),
                    vec![EntityType::Value(target)],
                    second.span,
                )?;
                value(results[0])
            }
        };
        // A terminating prefix has no commit fact. Require the trusted plan only here.
        let plan = self
            .owned
            .field_replacement(call)
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
        let fact = self
            .owned
            .loan_begin(first.value)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if plan.descriptor() != &descriptor
            || plan.owner_type() != owner_type
            || !matches!(fact.target(), LoanTarget::Place(place) if place == plan.place())
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let copyable = self.typed.copyability(ty) == Some(Copyability::Copyable);
        let expected = match (self.typed.expression_category(second.value), copyable) {
            (Some(ExpressionCategory::Temporary), _) => OwnershipPrimitiveValueTransfer::Temporary,
            (Some(ExpressionCategory::Place), true) => OwnershipPrimitiveValueTransfer::Copy,
            (Some(ExpressionCategory::Place), false) => OwnershipPrimitiveValueTransfer::Move,
            _ => return Err(error(LoweringErrorKind::MissingFact, second.span)),
        };
        if plan.new_value_transfer() != expected {
            return Err(error(LoweringErrorKind::MissingFact, second.span));
        }
        if copyable {
            let (_, results) = self.append(
                Operation::Copy {
                    source: replacement,
                },
                vec![EntityType::Value(target)],
                second.span,
            )?;
            replacement = value(results[0]);
        } else {
            self.forget_delivered_owners(&[replacement]);
            if self
                .function
                .entity(EntityId::Value(replacement))
                .map(|entity| entity.ty)
                != Some(EntityType::Value(target))
            {
                let Some(TypeKind::Nullable(inner)) = self.typed.types().get(ty) else {
                    return Err(error(LoweringErrorKind::MissingFact, second.span));
                };
                if self.typed.expression_type(second.value) != Some(*inner) {
                    return Err(error(LoweringErrorKind::MissingFact, second.span));
                }
                let (_, results) = self.append(
                    Operation::NullableWrap {
                        nullable: target,
                        owner: replacement,
                    },
                    vec![EntityType::Value(target)],
                    second.span,
                )?;
                replacement = value(results[0]);
            }
        }
        // Both identities may have crossed CFG while the replacement was evaluated.
        let Some(LoweredValue::Value(owner)) = self.bindings.get(&symbol).copied() else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        let loan = self
            .pending_call_loans
            .get(&(call.index(), first.value.index()))
            .copied()
            .flatten()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let (_, results) = self.append(
            Operation::HeapFieldExchange {
                owner,
                field,
                loan,
                replacement,
            },
            vec![EntityType::Value(target)],
            span,
        )?;
        self.pending_call_loans
            .remove(&(call.index(), first.value.index()));
        self.emit_drops(DropPoint::CallReturn(call))?;
        Ok(if builtin_type(self.typed, ty) == Some(BuiltinType::Unit) {
            LoweredValue::Unit
        } else {
            LoweredValue::Value(value(results[0]))
        })
    }

    fn begin_field_replace(
        &mut self,
        call: ExpressionId,
        operand: ExpressionId,
        target: SsaTypeId,
        span: Span,
    ) -> Result<(SymbolId, usize, TypeId), LoweringError> {
        let fact = self
            .owned
            .loan_begin(operand)
            .filter(|fact| fact.call() == call)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let LoanTarget::Place(path) = fact.target() else {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        };
        let [field_symbol] = path.fields() else {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        };
        if !path.elements().is_empty() || fact.kind() != FrontendLoanKind::Exclusive {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        let symbol = path.root();
        // The SSA alias model still treats sibling fields as a shared parent root.
        // Keep that backend boundary explicit rather than generating invalid SSA for
        // otherwise legal simultaneous disjoint source loans.
        if self.owned.loans().iter().any(|active| {
            self.pending_call_loans
                .contains_key(&(active.call().index(), active.argument().index()))
                && matches!(active.target(), LoanTarget::Place(place) if place.root() == symbol)
        }) {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        let begin = fact.begin_span();
        let projected = self.field_replace_ungroup(operand, span)?;
        let projection = self
            .typed
            .aggregate_projection(projected)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let AggregateProjectionReceiver::Expression(receiver) = projection.receiver() else {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        };
        let receiver = self.field_replace_ungroup(receiver, span)?;
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(receiver)
            .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
        if !matches!(node.payload(), Expression::Name)
            || self.references.get(&span_key(node.span())) != Some(&symbol)
            || projection.field() != *field_symbol
            || projection.kind() != AggregateProjectionKind::Field
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        let owner_type = self
            .typed
            .symbol_type(symbol)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let Some(TypeKind::Nominal { nominal, arguments }) = self.typed.types().get(owner_type)
        else {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        };
        let nominal = self
            .typed
            .nominals()
            .iter()
            .find(|declaration| declaration.id() == *nominal)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if nominal.kind() != NominalKind::Class
            || !arguments.is_empty()
            || self.typed.parameter_mode(symbol).is_some()
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        let field = nominal
            .fields()
            .iter()
            .position(|candidate| candidate == field_symbol)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let field_type = self.resolve_type(projection.ty(), span)?;
        if self.type_ids.get(&field_type) != Some(&target) {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let Some(LoweredValue::Value(owner)) = self.bindings.get(&symbol).copied() else {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        };
        let owner_ssa = self
            .type_ids
            .get(&owner_type)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let payload = self
            .heap_payloads
            .get(owner_ssa)
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let (_, roots) = self.append(
            Operation::HeapPayloadPlace { owner },
            vec![EntityType::Place(payload)],
            span,
        )?;
        let (_, fields) = self.append(
            Operation::FieldPlace {
                base: place(roots[0]),
                field,
            },
            vec![EntityType::Place(target)],
            span,
        )?;
        let (_, loans) = self.append(
            Operation::BorrowBegin {
                place: place(fields[0]),
                kind: LoanKind::Exclusive,
            },
            vec![EntityType::Loan {
                kind: LoanKind::Exclusive,
                target,
            }],
            begin,
        )?;
        let EntityId::Loan(loan) = loans[0] else {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        };
        self.pending_call_loans
            .insert((call.index(), operand.index()), Some(loan));
        Ok((symbol, field, owner_type))
    }

    pub(super) fn field_replace_ungroup(
        &self,
        mut expression: ExpressionId,
        span: Span,
    ) -> Result<ExpressionId, LoweringError> {
        loop {
            let node = self
                .parsed
                .ast()
                .expressions()
                .get(expression)
                .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
            let Expression::Group { expression: inner } = node.payload() else {
                return Ok(expression);
            };
            expression = *inner;
        }
    }
}

pub(in crate::ssa::lower_frontend) mod readonly_receiver {
    //! Narrow single-file Borrow receiver admission and the shared readonly entry view.
    use super::super::{
        EntityId, EntityType, LoanId, LoanKind, LoweringError, LoweringErrorKind, error,
    };
    use lang_frontend::{
        ast::ExpressionId,
        ownership_checking::LoanTarget,
        parser::Expression,
        source::Span,
        type_checking::{
            BuiltinType, CallDescriptor, CallReceiverOrigin, CallableDescriptor,
            CallableResultSource, CallableTarget, NominalId, NominalKind, ParameterMode, TypeId,
            TypeKind, TypedFile,
        },
    };

    pub(in crate::ssa::lower_frontend) fn admitted(
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
                super::super::builtin_type(typed, callable.return_type()),
                Some(BuiltinType::Int | BuiltinType::Boolean)
            )
            && callable
                .parameters()
                .iter()
                .all(|parameter| parameter.mode != ParameterMode::Inout))
        .then_some((owner, receiver.ty()))
    }

    pub(in crate::ssa::lower_frontend) struct Operand {
        argument: ExpressionId,
        loan: LoanId,
    }

    impl super::super::ExpressionLowerer<'_> {
        pub(in crate::ssa::lower_frontend) fn lower_readonly_call_receiver(
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
                    let mut actual = argument;
                    loop {
                        if self.typed.non_null_use(actual).is_some() {
                            return Err(error(LoweringErrorKind::UnsupportedNode, span));
                        }
                        let node = self
                            .parsed
                            .ast()
                            .expressions()
                            .get(actual)
                            .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
                        let Expression::Group { expression } = node.payload() else {
                            break;
                        };
                        actual = *expression;
                    }
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
                        .get(&super::super::span_key(node.span()))
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
            if !created {
                let entity = self
                    .function
                    .entity(EntityId::Loan(loan))
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                if !matches!(entity.definition,
                    crate::ssa::model::Definition::BlockParameter { block, .. }
                        if Some(block) == self.function.entry_block())
                {
                    return Err(error(LoweringErrorKind::UnsupportedNode, span));
                }
            }
            self.pending_call_loans.insert(
                (expression.index(), argument.index()),
                created.then_some(loan),
            );
            Ok(Some(Operand { argument, loan }))
        }

        pub(in crate::ssa::lower_frontend) fn current_readonly_call_receiver(
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

        /// Resolve only the validated readonly entry receiver and projections rooted in it.
        /// Created loans are returned in outer-to-inner order for reverse cleanup.
        pub(in crate::ssa::lower_frontend) fn readonly_view(
            &mut self,
            expression: lang_frontend::ast::ExpressionId,
        ) -> Result<Option<(super::super::LoanId, Vec<super::super::LoanId>)>, LoweringError>
        {
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
                NominalKind::Class => nominal.kind() == NominalKind::Class,
                NominalKind::ValueClass => {
                    nominal.id() == owner
                        && created.is_empty()
                        && matches!(
                            super::super::builtin_type(self.typed, projection.ty()),
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
}

#[cfg(test)]
mod map_consume_nullable_guard_tests {
    //! CI-only qualification/live/branch checkpoints from the unchanged C1/C2/C3 sources.
    use crate::ssa::lower_frontend::map_consume::nullable::*;
    use crate::ssa::lower_frontend::*;
    use crate::ssa::lower_frontend::orchestrate::nullable_map_guard_support::{
        Fixture, Point, Snapshot,
    };
    use lang_frontend::ownership_checking::{IterationCleanupAction, LoanEndPoint};

    #[derive(Clone, Copy, Debug)]
    enum Target {
        Qualification,
        Live,
        Branch,
    }

    #[derive(Clone, Copy, Debug)]
    enum Mutation {
        NoSource,
        WrongSource,
        MissingFunction,
        MissingType,
        MissingRegistration,
        WrongRegistration,
        MissingProof,
        MissingBinding,
        HeaderBinding,
        WrongLoanType,
        WrongLoanDefinition,
        NamedAlias,
        Temporary,
        Borrow,
        Pending,
        ReplayedTake,
        WrongBirthOrigin,
        WrongNonNullTarget,
        WrongNullArgument,
        WrongOwnerColumn,
    }

    impl Mutation {
        fn target(self) -> Target {
            match self {
                Self::NoSource
                | Self::WrongSource
                | Self::MissingFunction
                | Self::MissingType
                | Self::MissingRegistration
                | Self::WrongRegistration
                | Self::MissingProof => Target::Qualification,
                Self::WrongLoanDefinition
                | Self::WrongBirthOrigin
                | Self::WrongNonNullTarget
                | Self::WrongNullArgument
                | Self::WrongOwnerColumn
                | Self::ReplayedTake => Target::Branch,
                _ => Target::Live,
            }
        }
        fn kind(self) -> LoweringErrorKind {
            match self {
                Self::NoSource
                | Self::MissingProof
                | Self::NamedAlias
                | Self::Temporary
                | Self::Borrow
                | Self::Pending
                | Self::ReplayedTake => LoweringErrorKind::UnsupportedNode,
                _ => LoweringErrorKind::MissingFact,
            }
        }
    }

    fn target(
        lowerer: &ExpressionLowerer<'_>,
        point: Point,
        target: Target,
    ) -> Result<(), LoweringError> {
        let descriptor = *lowerer.typed.map_consume(point.expression).unwrap();
        let source = lowerer.nullable_consume_source(descriptor, point.proof.inner, point.span)?;
        match target {
            Target::Qualification => Ok(()),
            Target::Live => lowerer.validate_nullable_consume_live(&source, point.proof, point.span),
            Target::Branch => {
                lowerer.validate_nullable_consume_branch(&source, point.proof, point.span)
            }
        }
    }

    fn direct_baseline(fixture: &Fixture, which: Target) {
        let mut state = fixture.consume_checkpoint();
        let point = state.point;
        fixture.with_lowerer(&mut state, |lowerer| {
            let before = Snapshot::take(lowerer);
            target(lowerer, point, which).unwrap();
            assert_eq!(Snapshot::take(lowerer), before);
        });
    }

    fn reject(fixture: &Fixture, mutation: Mutation) {
        direct_baseline(fixture, mutation.target());
        entry_baseline(fixture);
        let mut state = fixture.consume_checkpoint();
        let point = state.point;
        match mutation {
            Mutation::NoSource => state.source = None,
            Mutation::WrongSource => {
                state.source = Some(
                    fixture
                        .other_source(state.source.unwrap())
                        .expect("C3 has another actual planned callable"),
                )
            }
            Mutation::MissingFunction => state.function_ids.retain(|_, id| *id != state.function.id),
            Mutation::MissingType => {
                state.type_ids.remove(&point.storage);
            }
            Mutation::MissingRegistration => {
                state.nullable_maps.remove(&point.nullable);
            }
            Mutation::WrongRegistration => {
                let result = fixture
                    .typed
                    .map_consume(point.expression)
                    .unwrap()
                    .result_type();
                state
                    .nullable_maps
                    .insert(point.nullable, state.type_ids[&result]);
            }
            _ => {}
        }
        let alias = fixture.symbol(match fixture.id {
            "C1" => "result",
            "C2" => "seed",
            "C3" => "input",
            _ => unreachable!(),
        });
        fixture.with_lowerer(&mut state, |lowerer| {
            match mutation {
                Mutation::MissingProof => {
                    lowerer.non_null_bindings.remove(&point.root);
                }
                Mutation::MissingBinding => {
                    lowerer.bindings.remove(&point.root);
                }
                Mutation::HeaderBinding => {
                    lowerer
                        .bindings
                        .insert(point.root, LoweredValue::Value(point.header_owner));
                }
                Mutation::WrongLoanType => {
                    lowerer.function.loans[point.proof.loan.index()].ty = EntityType::Loan {
                        kind: LoanKind::Exclusive,
                        target: point.proof.inner,
                    };
                }
                Mutation::WrongLoanDefinition => {
                    let definition = lowerer
                        .function
                        .entity(EntityId::Value(point.proof.owner))
                        .unwrap()
                        .definition;
                    lowerer.function.loans[point.proof.loan.index()].definition = definition;
                }
                Mutation::NamedAlias => {
                    lowerer
                        .bindings
                        .insert(alias, LoweredValue::Value(point.proof.owner));
                }
                Mutation::Temporary => {
                    lowerer
                        .temporaries
                        .insert(point.expression.index(), point.proof.owner);
                }
                Mutation::Borrow => {
                    lowerer.borrow_bindings.insert(point.root, point.proof.loan);
                }
                Mutation::Pending => {
                    let receiver = lowerer
                        .typed
                        .map_consume(point.expression)
                        .unwrap()
                        .receiver();
                    lowerer
                        .pending_call_loans
                        .insert((point.expression.index(), receiver.index()), None);
                }
                Mutation::ReplayedTake => {
                    lowerer.function.blocks[point.body.index()]
                        .instructions
                        .push(point.instruction);
                }
                Mutation::WrongBirthOrigin => {
                    lowerer.function.blocks[point.header.index()]
                        .terminator
                        .as_mut()
                        .unwrap()
                        .origin = Origin::Source(point.span);
                }
                Mutation::WrongNonNullTarget => {
                    let TerminatorKind::NullableBranch { when_non_null, .. } =
                        &mut lowerer.function.blocks[point.header.index()]
                            .terminator
                            .as_mut()
                            .unwrap()
                            .kind
                    else {
                        unreachable!()
                    };
                    when_non_null.target = point.null_block;
                }
                Mutation::WrongNullArgument => {
                    let TerminatorKind::NullableBranch { when_null, .. } = &mut lowerer.function.blocks
                        [point.header.index()]
                    .terminator
                    .as_mut()
                    .unwrap()
                    .kind
                    else {
                        unreachable!()
                    };
                    when_null.arguments[0] = EntityId::Value(point.proof.owner);
                }
                Mutation::WrongOwnerColumn => {
                    let definition = lowerer
                        .function
                        .entity(EntityId::Loan(point.proof.loan))
                        .unwrap()
                        .definition;
                    lowerer.function.values[point.proof.owner.index()].definition = definition;
                }
                _ => {}
            }
            let before = Snapshot::take(lowerer);
            fixture.reject(
                target(lowerer, point, mutation.target()),
                mutation.kind(),
                point.span,
            );
            assert_eq!(
                Snapshot::take(lowerer),
                before,
                "{mutation:?}: direct guard is read-only"
            );
            fixture.reject(
                lowerer.lower_map_consume(point.expression),
                mutation.kind(),
                point.span,
            );
            assert_eq!(
                Snapshot::take(lowerer),
                before,
                "{mutation:?}: no new Take/Consume/End/Drop or partial state"
            );
        });
    }

    fn cleanup_and_entry_baselines(fixture: &Fixture) {
        let mut state = fixture.consume_checkpoint();
        let point = state.point;
        fixture.with_lowerer(&mut state, |lowerer| {
            let descriptor = *lowerer.typed.map_consume(point.expression).unwrap();
            let source = lowerer
                .nullable_consume_source(descriptor, point.proof.inner, point.span)
                .unwrap();
            let mut expected = Vec::new();
            let mut operand = descriptor.receiver();
            loop {
                expected.push(operand);
                match lowerer
                    .parsed
                    .ast()
                    .expressions()
                    .get(operand)
                    .unwrap()
                    .payload()
                {
                    Expression::Group { expression } => operand = *expression,
                    Expression::Name => break,
                    _ => panic!("frozen Name/Group receiver"),
                }
            }
            expected.reverse();
            assert_eq!(
                source.operands, expected,
                "actual leaf-to-outer cleanup order"
            );
            assert_eq!(expected.len(), if fixture.id == "C2" { 3 } else { 1 });
            let points = expected
                .iter()
                .copied()
                .map(DropPoint::AfterExpression)
                .chain(std::iter::once(DropPoint::CallReturn(point.expression)))
                .collect::<Vec<_>>();
            for at in points {
                let facts = lowerer
                    .owned
                    .drops()
                    .iter()
                    .copied()
                    .filter(|fact| fact.point() == at)
                    .collect::<Vec<_>>();
                let steps = lowerer
                    .owned
                    .cleanup_steps()
                    .iter()
                    .filter(|(point, _)| *point == at)
                    .map(|(_, action)| match action {
                        IterationCleanupAction::Drop(fact) => *fact,
                        _ => panic!("frozen source has no other cleanup action at these points"),
                    })
                    .collect::<Vec<_>>();
                assert_eq!(steps, facts, "actual per-boundary cleanup order");
                // Deliberately empty in these frozen sources. This is not nonempty-fact fault coverage.
                assert!(
                    facts.is_empty(),
                    "new cleanup facts require a separately reviewed checkpoint"
                );
                assert!(
                    !lowerer
                        .owned
                        .borrow_results()
                        .ends()
                        .iter()
                        .any(|fact| fact.point() == at)
                );
            }
            assert!(
                !lowerer.owned.loans().iter().any(
                    |loan| loan.call() == source.call || source.operands.contains(&loan.argument())
                )
            );
            assert!(
                !lowerer
                    .owned
                    .loan_ends()
                    .iter()
                    .any(|end| end.point() == LoanEndPoint::CallReturn(source.call))
            );
            let before = Snapshot::take(lowerer);
            lowerer
                .preflight_nullable_consume_cleanup(&source, point.proof.owner, point.span)
                .unwrap();
            assert_eq!(Snapshot::take(lowerer), before);
        });
    }

    fn entry_baseline(fixture: &Fixture) {
        // Every mutating entry refusal has its own independent fresh successful entry.
        let mut state = fixture.consume_checkpoint();
        let point = state.point;
        fixture.with_lowerer(&mut state, |lowerer| {
            let start = lowerer.function.instructions.len();
            let result = lowerer.lower_map_consume(point.expression).unwrap();
            assert!(matches!(result, LoweredValue::Value(_)));
            assert_eq!(lowerer.function.instructions.len(), start + 2);
            assert_eq!(
                lowerer.function.instructions[start].operation,
                Operation::NullableTake {
                    owner: point.proof.owner,
                    proof: point.proof.loan
                }
            );
            assert!(matches!(
                lowerer.function.instructions[start + 1].operation,
                Operation::MapConsume { .. }
            ));
            assert!(!lowerer.bindings.contains_key(&point.root));
            assert!(!lowerer.non_null_bindings.contains_key(&point.root));
        });
    }

    #[test]
    fn single_nullable_map_consume_guards() {
        for id in ["C1", "C2", "C3"] {
            let fixture = Fixture::analyze(id);
            cleanup_and_entry_baselines(&fixture);
            for mutation in [
                Mutation::NoSource,
                Mutation::MissingFunction,
                Mutation::MissingType,
                Mutation::MissingRegistration,
                Mutation::WrongRegistration,
                Mutation::MissingProof,
                Mutation::MissingBinding,
                Mutation::HeaderBinding,
                Mutation::WrongLoanType,
                Mutation::WrongLoanDefinition,
                Mutation::NamedAlias,
                Mutation::Temporary,
                Mutation::Borrow,
                Mutation::Pending,
                Mutation::ReplayedTake,
                Mutation::WrongBirthOrigin,
                Mutation::WrongNonNullTarget,
                Mutation::WrongNullArgument,
                Mutation::WrongOwnerColumn,
            ] {
                reject(&fixture, mutation);
            }
            if id == "C3" {
                reject(&fixture, Mutation::WrongSource);
            }
        }
        // No real While/When/assertion scope exists in these inputs: do not invent one.
        // FE private descriptor/formation/cleanup mutation remains explicitly outside this exact.
    }
}
