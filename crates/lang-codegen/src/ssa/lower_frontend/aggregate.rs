//! Frontend aggregate projection, structural destructuring, and resource destructor declaration.

use lang_frontend::{
    ast::{ExpressionId, StatementId},
    type_checking::{AggregateProjectionReceiver, DestructuringMode, NominalKind, TypeKind},
};

use super::{
    ExpressionLowerer, LoweredValue, LoweringError, LoweringErrorKind, error, place, value,
};
use crate::ssa::model::{EntityType, Operation, PlaceAccess};

pub(in crate::ssa::lower_frontend) mod resource_deinit {
    use lang_frontend::{
        name_resolution::NameResolution,
        parser::{Expression, ParsedFile},
        source::Span,
        type_checking::{
            BuiltinType, DeinitDescriptor, IntrinsicTypeConstructor, NominalKind, ParameterMode,
            TypeId, TypeKind, TypedFile,
        },
    };

    use super::super::{LoweringError, LoweringErrorKind, error, nominal::NominalTypeMapper};
    use crate::ssa::model::{EntityType, FunctionId, LoanKind, Module, Origin};

    pub(in crate::ssa::lower_frontend) struct DeinitPlan {
        pub(in crate::ssa::lower_frontend) id: FunctionId,
        pub(in crate::ssa::lower_frontend) descriptor: DeinitDescriptor,
        pub(in crate::ssa::lower_frontend) return_type: TypeId,
        pub(in crate::ssa::lower_frontend) span: Span,
    }

    pub(in crate::ssa::lower_frontend) fn declare(
        module: &mut Module,
        mapper: &mut NominalTypeMapper,
        parsed: &ParsedFile,
        names: &NameResolution,
        typed: &TypedFile,
    ) -> Result<Vec<DeinitPlan>, LoweringError> {
        let mut plans = Vec::new();
        for nominal in typed.nominals() {
            let Some(descriptor) = nominal.deinit() else {
                continue;
            };
            let span = parsed
                .ast()
                .items()
                .get(descriptor.item())
                .map_err(|_| LoweringError {
                    kind: LoweringErrorKind::MissingFact,
                    span: None,
                })?
                .span();
            if descriptor.owner() != nominal.id()
                || descriptor.receiver_mode() != ParameterMode::Borrow
            {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            if nominal.kind() != NominalKind::Class
                || !nominal.type_parameters().is_empty()
                || !nominal.interfaces().is_empty()
            {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
            if parsed.ast().expressions().iter().any(|(_, node)| {
                span.start() <= node.span().start()
                    && node.span().end() <= span.end()
                    && matches!(node.payload(), Expression::Lambda { .. })
            }) {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
            let owner = mapper.intern(module, names, typed, descriptor.receiver_type(), span)?;
            let receiver = EntityType::Loan {
                kind: LoanKind::Shared,
                target: owner,
            };
            let id = module
                .add_instance_function(
                    format!("__deinit.t{}", descriptor.receiver_type().index()),
                    receiver,
                    Vec::new(),
                    Origin::Source(span),
                )
                .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
            module
                .function_mut(id)
                .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))?
                .add_block(vec![receiver], Origin::Source(span))
                .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
            module
                .set_deinit(owner, id)
                .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
            let return_type = typed
                .types()
                .builtin(BuiltinType::Unit)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            plans.push(DeinitPlan {
                id,
                descriptor,
                return_type,
                span,
            });
        }
        Ok(plans)
    }

    pub(in crate::ssa::lower_frontend) fn validate_type(
        typed: &TypedFile,
        ty: TypeId,
        span: Span,
    ) -> Result<(), LoweringError> {
        let classification = typed.is_resource_type(ty);
        if classification == Some(false) {
            return Ok(());
        }
        if classification.is_none() {
            if let Some(TypeKind::Nominal { arguments, .. }) = typed.types().get(ty)
                && arguments
                    .iter()
                    .any(|argument| typed.is_resource_type(*argument) != Some(false))
            {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
            return Ok(());
        }
        if let Some(TypeKind::Intrinsic {
            constructor,
            arguments,
        }) = typed.types().get(ty)
            && matches!(
                constructor,
                IntrinsicTypeConstructor::Map | IntrinsicTypeConstructor::MutableMap
            )
            && let [key, value] = arguments.as_slice()
        {
            validate_type(typed, *key, span)?;
            return validate_map_value_type(typed, *value, span);
        }
        if let Some(TypeKind::Intrinsic {
            constructor,
            arguments,
        }) = typed.types().get(ty)
            && matches!(
                constructor,
                IntrinsicTypeConstructor::Array
                    | IntrinsicTypeConstructor::List
                    | IntrinsicTypeConstructor::MutableList
            )
            && let [element] = arguments.as_slice()
        {
            return validate_type(typed, *element, span);
        }
        let Some(TypeKind::Nominal { nominal, arguments }) = typed.types().get(ty) else {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        };
        let descriptor = typed
            .nominals()
            .iter()
            .find(|value| value.id() == *nominal)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if !matches!(
            descriptor.kind(),
            NominalKind::Class | NominalKind::ValueClass
        ) || !arguments.is_empty()
            || !descriptor.type_parameters().is_empty()
            || !descriptor.interfaces().is_empty()
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        if descriptor.kind() == NominalKind::ValueClass {
            for field in descriptor.fields() {
                let field_type = typed
                    .symbol_type(*field)
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                validate_type(typed, field_type, span)?;
            }
        }
        Ok(())
    }

    pub(in crate::ssa::lower_frontend) fn validate_map_value_type(
        typed: &TypedFile,
        ty: TypeId,
        span: Span,
    ) -> Result<(), LoweringError> {
        if let Some(TypeKind::Nullable(inner)) = typed.types().get(ty)
            && let Some(TypeKind::Nominal { nominal, .. }) = typed.types().get(*inner)
            && typed.nominals().iter().any(|descriptor| {
                descriptor.id() == *nominal && descriptor.kind() == NominalKind::Class
            })
        {
            return validate_type(typed, *inner, span);
        }
        validate_type(typed, ty, span)
    }
}

impl ExpressionLowerer<'_> {
    pub(super) fn lower_aggregate_projection(
        &mut self,
        expression: ExpressionId,
    ) -> Result<LoweredValue, LoweringError> {
        let projection = self
            .typed
            .aggregate_projection(expression)
            .ok_or(LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?;
        let span = self
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|_| LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?
            .span();
        if let Some(result) = self.read_while_proof_int_field(expression, projection, span)? {
            return Ok(result);
        }
        if let Some((loan, created)) = self.readonly_view(expression)? {
            if self.typed.copyability(projection.ty())
                != Some(lang_frontend::type_checking::Copyability::Copyable)
            {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
            let result_type = self.expression_ssa_type(expression, span)?;
            let (_, results) = self.append(
                Operation::Read {
                    source: PlaceAccess::Loan(loan),
                },
                vec![EntityType::Value(result_type)],
                span,
            )?;
            for loan in created.into_iter().rev() {
                self.append(Operation::BorrowEnd { loan }, Vec::new(), span)?;
            }
            return Ok(LoweredValue::Value(value(results[0])));
        }
        let AggregateProjectionReceiver::Expression(receiver_expression) = projection.receiver()
        else {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        };
        let receiver_root = self
            .field_replace_ungroup(receiver_expression, span)
            .ok()
            .and_then(|receiver| self.parsed.ast().expressions().get(receiver).ok())
            .and_then(|node| self.references.get(&super::span_key(node.span())));
        if receiver_root.is_some_and(|root| self.owned.loans().iter().any(|active| {
            self.pending_call_loans.contains_key(&(active.call().index(), active.argument().index()))
                && active.kind() == lang_frontend::ownership_checking::LoanKind::Exclusive
                && matches!(active.target(), lang_frontend::ownership_checking::LoanTarget::Place(path)
                    if !path.is_root() && path.root() == *root)
        })) {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        let receiver_type = self
            .typed
            .expression_type(receiver_expression)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let borrowed_root =
            receiver_root.is_some_and(|symbol| self.borrow_bindings.contains_key(symbol));
        if borrowed_root
            && matches!(
                self.typed.types().get(receiver_type),
                Some(TypeKind::Nominal { .. })
            )
        {
            if self.typed.copyability(projection.ty())
                != Some(lang_frontend::type_checking::Copyability::Copyable)
            {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
            let mut created = Vec::new();
            let loan = self.clone_field_loan(expression, &mut created)?;
            let result_type = self.expression_ssa_type(expression, span)?;
            let (_, results) = self.append(
                Operation::Read {
                    source: PlaceAccess::Loan(loan),
                },
                vec![EntityType::Value(result_type)],
                span,
            )?;
            for loan in created.into_iter().rev() {
                self.append(Operation::BorrowEnd { loan }, Vec::new(), span)?;
            }
            return Ok(LoweredValue::Value(value(results[0])));
        }
        let receiver = self.require_value(receiver_expression)?;
        let result_type = self.expression_ssa_type(expression, span)?;
        match self.typed.types().get(receiver_type) {
            Some(TypeKind::Nominal { nominal, .. }) => {
                let descriptor = self
                    .typed
                    .nominals()
                    .iter()
                    .find(|descriptor| descriptor.id() == *nominal)
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                let field = descriptor
                    .fields()
                    .iter()
                    .position(|field| *field == projection.field())
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                if descriptor.kind() == NominalKind::ValueClass {
                    let (_, results) = self.append(
                        Operation::AggregateProject {
                            aggregate: receiver,
                            field,
                        },
                        vec![EntityType::Value(result_type)],
                        span,
                    )?;
                    return Ok(LoweredValue::Value(value(results[0])));
                }
                if descriptor.kind() != NominalKind::Class {
                    return Err(error(LoweringErrorKind::UnsupportedNode, span));
                }
                let receiver_ssa = self.expression_ssa_type(receiver_expression, span)?;
                let payload = self
                    .heap_payloads
                    .get(&receiver_ssa)
                    .copied()
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                self.read_field(
                    Operation::HeapPayloadPlace { owner: receiver },
                    payload,
                    field,
                    result_type,
                    span,
                )
            }
            Some(TypeKind::EnumCase { case, root }) => {
                let root = self
                    .type_ids
                    .get(root)
                    .copied()
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                let (variant, payload) = self
                    .enum_payloads
                    .get(&(root, *case))
                    .copied()
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                let case = self
                    .typed
                    .enum_cases()
                    .iter()
                    .find(|descriptor| descriptor.id() == *case)
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                let field = case
                    .payloads()
                    .iter()
                    .position(|(field, _)| *field == projection.field())
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                self.read_field(
                    Operation::TaggedPayloadPlace {
                        owner: receiver,
                        variant,
                    },
                    payload,
                    field,
                    result_type,
                    span,
                )
            }
            _ => Err(error(LoweringErrorKind::MissingFact, span)),
        }
    }

    fn read_field(
        &mut self,
        root: Operation,
        aggregate: crate::ssa::model::SsaTypeId,
        field: usize,
        result_type: crate::ssa::model::SsaTypeId,
        span: lang_frontend::source::Span,
    ) -> Result<LoweredValue, LoweringError> {
        let (_, root_results) = self.append(root, vec![EntityType::Place(aggregate)], span)?;
        let (_, field_results) = self.append(
            Operation::FieldPlace {
                base: place(root_results[0]),
                field,
            },
            vec![EntityType::Place(result_type)],
            span,
        )?;
        let (_, results) = self.append(
            Operation::Read {
                source: PlaceAccess::Place(place(field_results[0])),
            },
            vec![EntityType::Value(result_type)],
            span,
        )?;
        Ok(LoweredValue::Value(value(results[0])))
    }

    pub(super) fn lower_destructuring(
        &mut self,
        statement: StatementId,
        initializer: ExpressionId,
        span: lang_frontend::source::Span,
    ) -> Result<LoweredValue, LoweringError> {
        let descriptor = self
            .typed
            .destructuring(statement)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let source = self.require_value(initializer)?;
        let result_types = descriptor
            .components()
            .iter()
            .map(|component| {
                self.type_ids
                    .get(&component.ty())
                    .copied()
                    .map(EntityType::Value)
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let operation = match descriptor.mode() {
            DestructuringMode::Copy => Operation::AggregateCopyExplode { aggregate: source },
            DestructuringMode::Consume => Operation::AggregateExplode { aggregate: source },
        };
        let (_, results) = self.append(operation, result_types, span)?;
        for (component, entity) in descriptor.components().iter().zip(results) {
            self.bindings
                .insert(component.symbol(), LoweredValue::Value(value(entity)));
        }
        Ok(LoweredValue::Unit)
    }
}
