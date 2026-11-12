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
    pub(in crate::ssa) use crate::ssa::rc_nullable_while_bounded_tests::memory_document::map_consume_nullable as nullable;

    use crate::ssa::lower_frontend::{nominal::NominalTypeMapper, *};
    use crate::ssa::model::{Definition, LoanKind, MapContainerKind, Module, SsaTypeKind};
    use lang_frontend::{
        ownership_checking::{BorrowSourceLoan, OwnershipBindingKind},
        parser::VariableKind,
        type_checking::IntrinsicTypeConstructor,
    };

    impl ExpressionLowerer<'_> {
        pub(in crate::ssa) fn lower_map_consume(
            &mut self,
            expression: ExpressionId,
        ) -> Result<LoweredValue, LoweringError> {
            let span = self.expression_span(expression)?;
            let descriptor = self
                .typed
                .map_consume(expression)
                .copied()
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let (root, storage) = descriptor
                .source()
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            if descriptor.expression() != expression
                || self.typed.expression_type(descriptor.receiver()) != Some(descriptor.receiver_type())
                || self.typed.expression_type(expression) != Some(descriptor.result_type())
                || self.typed.symbol_type(root) != Some(storage)
            {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            self.validate_map_consume_root(descriptor.receiver(), root, span)?;
            let receiver_type = self.resolve_type(descriptor.receiver_type(), span)?;
            let result_type = self.resolve_type(descriptor.result_type(), span)?;
            let key = self.resolve_type(descriptor.key_type(), span)?;
            let val = self.resolve_type(descriptor.value_type(), span)?;
            for (ty, constructor) in [
                (receiver_type, IntrinsicTypeConstructor::MutableMap),
                (result_type, IntrinsicTypeConstructor::Map),
            ] {
                if !matches!(self.typed.types().get(ty),
                    Some(TypeKind::Intrinsic { constructor: actual, arguments })
                        if *actual == constructor && arguments.as_slice() == [key, val])
                    || self.typed.copyability(ty) != Some(Copyability::MoveOnly)
                {
                    return Err(error(LoweringErrorKind::MissingFact, span));
                }
            }
            let source_type = self.expression_ssa_type(descriptor.receiver(), span)?;
            let map_type = self.expression_ssa_type(expression, span)?;
            let key = self
                .type_ids
                .get(&key)
                .copied()
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let val = self
                .type_ids
                .get(&val)
                .copied()
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            for (ty, kind) in [
                (source_type, MapContainerKind::MutableMap),
                (map_type, MapContainerKind::Map),
            ] {
                if self.ssa_types.get(ty.index())
                    != Some(&SsaTypeKind::MapContainer {
                        kind,
                        key,
                        value: val,
                    })
                {
                    return Err(error(LoweringErrorKind::MissingFact, span));
                }
            }
            if storage != descriptor.receiver_type() {
                return self.lower_nullable_map_consume(descriptor, source_type, map_type, span);
            }
            let Some(LoweredValue::Value(owner)) = self.bindings.get(&root).copied() else {
                return Err(error(LoweringErrorKind::MissingFact, span));
            };
            if self
                .function
                .entity(EntityId::Value(owner))
                .map(|entity| entity.ty)
                != Some(EntityType::Value(source_type))
            {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            self.ensure_map_consume_delivery(root, span)?;
            // The validated whole-file ownership input proves the source move. This entry has
            // no independent receiver-effect table. Preserve every Name/Group cleanup boundary.
            let delivered = self.require_value(descriptor.receiver())?;
            if delivered != owner || self.bindings.get(&root) != Some(&LoweredValue::Value(owner)) {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            self.ensure_map_consume_delivery(root, span)?;
            self.forget_delivered_owners(&[owner]);
            let (_, results) = self.append(
                Operation::MapConsume { owner, map_type },
                vec![EntityType::Value(map_type)],
                span,
            )?;
            self.emit_drops(DropPoint::CallReturn(expression))?;
            Ok(LoweredValue::Value(value(results[0])))
        }

        fn ensure_map_consume_delivery(&self, root: SymbolId, span: Span) -> Result<(), LoweringError> {
            self.reject_live_while_proof(root, span)?;
            if self.non_null_bindings.contains_key(&root) || self.borrow_bindings.contains_key(&root) {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
            // Preserve the original gate, including its Boolean-entry compatibility.
            match self.ensure_no_unresolved_while_proof_carriers(span) {
                Ok(()) => Ok(()),
                Err(previous) if previous.kind == LoweringErrorKind::UnsupportedNode => {
                    self.ensure_map_consume_entry_borrows(root, span)
                }
                Err(previous) => Err(previous),
            }
        }

        /// Only the nonnullable owned-local consume calls this, before and after its
        /// original receiver evaluation. No loan is ended, rebuilt or rebound here.
        fn ensure_map_consume_entry_borrows(
            &self,
            root: SymbolId,
            span: Span,
        ) -> Result<(), LoweringError> {
            let missing = || error(LoweringErrorKind::MissingFact, span);
            let unsupported = || error(LoweringErrorKind::UnsupportedNode, span);
            if !self.pending_call_loans.is_empty()
                || !self.capture_loans.is_empty()
                || !self.result_source_loans.is_empty()
                || !self.non_null_bindings.is_empty()
                || self.readonly_receiver.is_some()
                || self.thunk_expression.is_some()
                || !self.loops.is_empty()
                || self.function.receiver().is_some()
                || self.function.borrow_return.is_some()
                || self.function.carrier_return.is_some()
            {
                return Err(unsupported());
            }
            let source = self.names.symbols().get(root.index()).ok_or_else(missing)?;
            if source.kind() != SymbolKind::Variable {
                return Err(unsupported());
            }
            if source.id() != root || source.span().source_id() != self.parsed.source_id() {
                return Err(missing());
            }
            if source.is_synthetic() || source.name() == "_" {
                return Err(unsupported());
            }
            let mut declarations = 0;
            for (_, node) in self.parsed.ast().statements().iter() {
                let Statement::LocalVariable { declaration } = node.payload() else {
                    continue;
                };
                let (item, _) = orchestrate::unwrap_modified(self.parsed, *declaration)?;
                if let Item::Variable {
                    name: NameMarker::Present(name),
                    kind,
                    ..
                } = item
                    && name == source.span()
                {
                    if !matches!(kind, VariableKind::Val | VariableKind::Var) {
                        return Err(unsupported());
                    }
                    declarations += 1;
                }
            }
            // Formation is only historical identity, never current liveness/permission.
            // Current ownership is still checked by the original consume delivery path.
            if declarations != 1 || !self.owned.formed_owned_locals().contains(&root) {
                return Err(missing());
            }
            let Some(LoweredValue::Value(owner)) = self.bindings.get(&root).copied() else {
                return Err(missing());
            };
            if self
                .bindings
                .iter()
                .any(|(&symbol, &binding)| symbol != root && binding == LoweredValue::Value(owner))
                || self.temporaries.values().any(|&value| value == owner)
            {
                return Err(unsupported());
            }
            let mut instances = self
                .function_ids
                .iter()
                .filter(|(_, id)| **id == self.function.id());
            let (instance, _) = instances.next().ok_or_else(missing)?;
            if instances.next().is_some() {
                return Err(missing());
            }
            let mut callables = self
                .typed
                .callables()
                .iter()
                .filter(|callable| callable.symbol() == instance.symbol());
            let callable = callables.next().ok_or_else(missing)?;
            if callables.next().is_some() {
                return Err(missing());
            }
            // No hidden receiver/callable slot or specialized parameter ordering.
            if callable.owner().is_some()
                || callable.receiver().is_some()
                || callable.extension_receiver_symbol().is_some()
                || !callable.type_parameters().is_empty()
                || !instance.type_arguments().is_empty()
                || !instance.callable_arguments().is_empty()
            {
                return Err(unsupported());
            }
            let entry = self.function.entry_block().ok_or_else(missing)?;
            let parameters = &self.function.block(entry).ok_or_else(missing)?.parameters;
            if parameters.len() != callable.parameters().len()
                || callable.parameter_symbols().len() != callable.parameters().len()
            {
                return Err(missing());
            }
            if self.borrow_bindings.is_empty() {
                return Err(unsupported());
            }
            for (&symbol, &loan) in &self.borrow_bindings {
                let data = self
                    .names
                    .symbols()
                    .get(symbol.index())
                    .ok_or_else(missing)?;
                if symbol == root || data.kind() != SymbolKind::ValueParameter {
                    return Err(unsupported());
                }
                let mut slots = callable
                    .parameter_symbols()
                    .iter()
                    .enumerate()
                    .filter(|(_, actual)| **actual == Some(symbol));
                let (slot, _) = slots.next().ok_or_else(missing)?;
                if slots.next().is_some() {
                    return Err(missing());
                }
                let parameter = callable.parameters().get(slot).ok_or_else(missing)?;
                if parameter.mode != ParameterMode::Borrow {
                    return Err(unsupported());
                }
                if self.typed.parameter_mode(symbol) != Some(ParameterMode::Borrow)
                    || self.typed.symbol_type(symbol) != Some(parameter.ty)
                    || self.owned.binding_kind(symbol) != Some(OwnershipBindingKind::Shared)
                    || self.bindings.contains_key(&symbol)
                {
                    return Err(missing());
                }
                let ty = self.resolve_type(parameter.ty, span)?;
                // Ordinary parameter planning has already admitted this concrete
                // representation. Match its original Shared slot exactly, including
                // a Borrow Map with the same K/V as the independently owned local.
                let target = *self.type_ids.get(&ty).ok_or_else(missing)?;
                if self.ssa_types.get(target.index()).is_none() {
                    return Err(missing());
                }
                let entity = self
                    .function
                    .entity(EntityId::Loan(loan))
                    .ok_or_else(missing)?;
                let Definition::BlockParameter { block, index } = entity.definition else {
                    return Err(unsupported());
                };
                if block != entry {
                    return Err(unsupported());
                }
                if index != slot
                    || parameters.get(slot) != Some(&EntityId::Loan(loan))
                    || entity.ty
                        != (EntityType::Loan {
                            kind: LoanKind::Shared,
                            target,
                        })
                {
                    return Err(missing());
                }
            }
            Ok(())
        }

        fn validate_map_consume_root(
            &self,
            mut expression: ExpressionId,
            root: SymbolId,
            span: Span,
        ) -> Result<(), LoweringError> {
            loop {
                let node = self
                    .parsed
                    .ast()
                    .expressions()
                    .get(expression)
                    .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
                match node.payload() {
                    Expression::Group { expression: inner } => expression = *inner,
                    Expression::Name if self.references.get(&span_key(node.span())) == Some(&root) => {
                        return Ok(());
                    }
                    _ => return Err(error(LoweringErrorKind::MissingFact, span)),
                }
            }
        }

        pub(in crate::ssa) fn lower_map_with_value(
            &mut self,
            expression: ExpressionId,
        ) -> Result<LoweredValue, LoweringError> {
            let span = self.expression_span(expression)?;
            let d = *self
                .typed
                .map_with_value(expression)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let mut loans = Vec::new();
            for operand in [d.receiver(), d.key(), d.action()] {
                let operand_span = self.expression_span(operand)?;
                if self.runtime_operand_exits(operand, operand_span)? {
                    return Ok(LoweredValue::Diverged);
                }
                let (loan, new) = self.lower_borrow_argument(expression, operand, operand_span)?;
                loans.push(loan);
                self.pending_call_loans
                    .insert((expression.index(), operand.index()), new.then_some(loan));
            }
            let ty = self.expression_ssa_type(expression, span)?;
            let (_, results) = self.append(
                Operation::MapWithValue {
                    source: loans[0],
                    key: EntityId::Loan(loans[1]),
                    action: loans[2],
                },
                vec![EntityType::Value(ty)],
                span,
            )?;
            self.finish_borrowed_call(expression, span)?;
            Ok(LoweredValue::Value(value(results[0])))
        }

        pub(in crate::ssa) fn lower_map_require_result(
            &mut self,
            expression: ExpressionId,
            source: BorrowSourceLoan<ExpressionId>,
            span: Span,
        ) -> Result<(LoanId, Vec<LoanId>), LoweringError> {
            let span = self
                .parsed
                .ast()
                .expressions()
                .get(expression)
                .map_err(|_| error(LoweringErrorKind::MissingFact, span))?
                .span();
            let descriptor = *self
                .typed
                .map_require_value(expression)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            if source.call() != expression || source.argument() != descriptor.receiver() {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            let receiver_span = self.expression_span(descriptor.receiver())?;
            let (source_loan, created) =
                self.lower_borrow_argument(expression, descriptor.receiver(), receiver_span)?;
            let key_span = self.expression_span(descriptor.key())?;
            let (key_loan, key_created) =
                self.lower_borrow_argument(expression, descriptor.key(), key_span)?;
            let target = self.expression_ssa_type(expression, span)?;
            let (_, results) = self.append(
                Operation::MapRequireValue {
                    source: source_loan,
                    key: EntityId::Loan(key_loan),
                },
                vec![EntityType::Loan {
                    kind: LoanKind::Shared,
                    target,
                }],
                span,
            )?;
            let [EntityId::Loan(result)] = results.as_slice() else {
                return Err(error(LoweringErrorKind::InvalidModel, span));
            };
            if key_created {
                self.append(Operation::BorrowEnd { loan: key_loan }, Vec::new(), span)?;
            }
            self.emit_drops(DropPoint::CallReturn(expression))?;
            Ok((
                *result,
                if created {
                    vec![source_loan]
                } else {
                    Vec::new()
                },
            ))
        }
    }

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
                BuiltinType, DeinitDescriptor, IntrinsicTypeConstructor, NominalKind, ParameterMode,
                TypeId, TypeKind, TypedFile,
            },
        };

        use crate::ssa::lower_frontend::{LoweringError, LoweringErrorKind, error, nominal::NominalTypeMapper};
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

    impl ExpressionLowerer<'_> {
        pub(in crate::ssa) fn lower_aggregate_projection(
            &mut self,
            expression: ExpressionId,
        ) -> Result<LoweredValue, LoweringError> {
            use crate::ssa::model::PlaceAccess;
            use lang_frontend::type_checking::AggregateProjectionReceiver;
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
                .and_then(|node| self.references.get(&crate::ssa::lower_frontend::span_key(node.span())));
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
            use crate::ssa::model::PlaceAccess;
            let (_, root_results) = self.append(root, vec![EntityType::Place(aggregate)], span)?;
            let (_, field_results) = self.append(
                Operation::FieldPlace {
                    base: crate::ssa::lower_frontend::place(root_results[0]),
                    field,
                },
                vec![EntityType::Place(result_type)],
                span,
            )?;
            let (_, results) = self.append(
                Operation::Read {
                    source: PlaceAccess::Place(crate::ssa::lower_frontend::place(field_results[0])),
                },
                vec![EntityType::Value(result_type)],
                span,
            )?;
            Ok(LoweredValue::Value(value(results[0])))
        }

        pub(in crate::ssa) fn lower_destructuring(
            &mut self,
            statement: StatementId,
            initializer: ExpressionId,
            span: lang_frontend::source::Span,
        ) -> Result<LoweredValue, LoweringError> {
            use lang_frontend::type_checking::DestructuringMode;
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
}
