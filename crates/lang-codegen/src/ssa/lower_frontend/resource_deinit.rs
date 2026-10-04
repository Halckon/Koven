//! Typed resource destructor descriptors to readonly hidden receiver callables.

use lang_frontend::{
    name_resolution::NameResolution,
    parser::{Expression, ParsedFile},
    source::Span,
    type_checking::{
        BuiltinType, DeinitDescriptor, IntrinsicTypeConstructor, NominalKind, ParameterMode,
        TypeId, TypeKind, TypedFile,
    },
};

use super::{LoweringError, LoweringErrorKind, error, nominal::NominalTypeMapper};
use crate::ssa::model::{EntityType, FunctionId, LoanKind, Module, Origin};

pub(super) struct DeinitPlan {
    pub(super) id: FunctionId,
    pub(super) descriptor: DeinitDescriptor,
    pub(super) return_type: TypeId,
    pub(super) span: Span,
}

pub(super) fn declare(
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
        if descriptor.owner() != nominal.id() || descriptor.receiver_mode() != ParameterMode::Borrow
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        if nominal.kind() != NominalKind::Class
            || !nominal.type_parameters().is_empty()
            || !nominal.interfaces().is_empty()
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        // A destructor closure would need separate receiver capture and reachability contracts.
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

/// Resource-bearing unsupported wrappers must never silently use plain memory glue.
pub(super) fn validate_type(
    typed: &TypedFile,
    ty: TypeId,
    span: Span,
) -> Result<(), LoweringError> {
    let classification = typed.is_resource_type(ty);
    if classification == Some(false) {
        return Ok(());
    }
    // A generic field template may remain unknown even when an actual argument carries
    // a resource. Do not let the existing nominal mapper silently enable that slice.
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

impl super::ExpressionLowerer<'_> {
    /// Resolve only the validated destructor receiver and projections rooted in it.
    /// Created loans are returned in outer-to-inner order for reverse cleanup.
    pub(super) fn deinit_view(
        &mut self,
        expression: lang_frontend::ast::ExpressionId,
    ) -> Result<Option<(super::LoanId, Vec<super::LoanId>)>, LoweringError> {
        use crate::ssa::model::{EntityId, Operation};
        use lang_frontend::type_checking::AggregateProjectionReceiver;
        let Some((owner, receiver_type, receiver)) = self.deinit_receiver else {
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
            return self.deinit_view(*expression);
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
                let Some((base, created)) = self.deinit_view(expression)? else {
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
        if nominal.kind() != NominalKind::Class || !arguments.is_empty() {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        let field = nominal
            .fields()
            .iter()
            .position(|field| *field == projection.field())
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let target = self.expression_ssa_type(expression, span)?;
        let (_, results) = self.append(
            Operation::SharedHeapFieldLoan { base, field },
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
