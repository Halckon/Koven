//! Ordinary original-source evidence; no fact rewriting or forced routing.
use super::*;
#[test]
fn original_binding_lifecycle_records_real_actions_and_ssa() {
    let text = "fun view(source: String): borrow String from source = source\nfun consume(item: String): Unit {}\nfun run(): Unit { val owner = \"kept\"; borrow val item = view(owner); consume(item) }";
    let mut sources = SourceMap::new();
    let (source, parsed) =
        unit_lower_test_support::parsed(&mut sources, "binding-lifecycle.ko", text);
    let (environment, types) = standard_environments();
    let inputs = [SourceUnitInput::memory_document(source, &parsed)];
    let (names, typed, owned) =
        unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
    println!(
        "actions={:#?}",
        owned.ownership().loan_action_observations()
    );
    println!("handoffs={:#?}", owned.ownership().borrow_call_handoffs());
    println!(
        "transports={:#?}",
        owned.ownership().borrow_binding_transports()
    );
    println!(
        "bindings={:#?}",
        owned.ownership().borrow_results().bindings()
    );
    println!(
        "binding_ends={:#?}",
        owned.ownership().borrow_results().ends()
    );
    let run = names
        .names()
        .index()
        .declarations()
        .iter()
        .find(|d| d.name() == "run")
        .unwrap();
    let (lowered, routes) = crate::ssa::refine::observe_routing(|| {
        crate::ssa::unit_lower::lower_scalar_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &types,
            &typed,
            &owned,
            run.id(),
        )
    });
    let (program, entry) = lowered.expect("original ordinary binding source lowers");
    verify_program(&program).unwrap();
    crate::llvm::render_verified_program(&program).unwrap();
    println!("routes={routes:?}; entry={entry:?}");
    println!("{}", crate::ssa::render_program(&program));
}

pub(in crate::ssa) mod map_consume {
    //! The checked owned-local consume path; ordinary Map borrowing stays separate.

    use crate::ssa::lower_frontend::{nominal::NominalTypeMapper, *};
    use crate::ssa::model::Module;

    impl NominalTypeMapper {
        /// Register only the consume descriptors inside this reachable concrete instance.
        pub(in crate::ssa) fn intern_map_consume_types(
            &mut self,
            module: &mut Module,
            names: &NameResolution,
            typed: &TypedFile,
            parsed: &ParsedFile,
            substitutions: &BTreeMap<SymbolId, TypeId>,
            span: Span,
        ) -> Result<(), LoweringError> {
            for descriptor in typed.map_consumes() {
                let consume_span = parsed
                    .ast()
                    .expressions()
                    .get(descriptor.expression())
                    .map_err(|_| error(LoweringErrorKind::MissingFact, span))?
                    .span();
                if consume_span.source_id() != span.source_id()
                    || consume_span.start() < span.start()
                    || consume_span.end() > span.end()
                {
                    continue;
                }
                for ty in [
                    descriptor.receiver_type(),
                    descriptor.result_type(),
                    descriptor.key_type(),
                    descriptor.value_type(),
                ] {
                    let concrete =
                        instances::resolve_concrete_type(typed, ty, substitutions, consume_span)?;
                    self.intern(module, names, typed, concrete, consume_span)?;
                }
            }
            Ok(())
        }
    }

    pub(in crate::ssa) mod resource_deinit {
        use lang_frontend::{
            name_resolution::NameResolution,
            parser::{Expression, ParsedFile},
            source::Span,
            type_checking::{
                BuiltinType, DeinitDescriptor, IntrinsicTypeConstructor, NominalKind,
                ParameterMode, TypeId, TypeKind, TypedFile,
            },
        };

        use crate::ssa::lower_frontend::{
            LoweringError, LoweringErrorKind, error, nominal::NominalTypeMapper,
        };
        use crate::ssa::model::{EntityType, FunctionId, LoanKind, Module, Origin};

        pub(in crate::ssa) struct DeinitPlan {
            pub(in crate::ssa) id: FunctionId,
            pub(in crate::ssa) descriptor: DeinitDescriptor,
            pub(in crate::ssa) return_type: TypeId,
            pub(in crate::ssa) span: Span,
        }

        pub(in crate::ssa) fn declare(
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
                let owner =
                    mapper.intern(module, names, typed, descriptor.receiver_type(), span)?;
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

        pub(in crate::ssa) fn validate_type(
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

        pub(in crate::ssa) fn validate_map_value_type(
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
}
