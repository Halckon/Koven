//! Ordinary Range storage reaches its value ABI; independent nullable use stays bounded.
use super::{
    LoweringError, LoweringErrorKind,
    model::Program,
    render::render_program,
    unit_lower::lower_scalar_unit_with_entry,
    unit_lower_test_support::{declaration, parsed},
    verify::verify_program,
};
use crate::native_tests::boxed_enum_tests::unit_source::UnitSource;
use crate::test_support::diagnostics::diagnostic;
use lang_frontend::{
    name_resolution::{
        SourceUnitInput, SymbolKind, index_compilation_unit, resolve_compilation_unit_names,
    },
    ownership_checking::check_compilation_unit_ownership,
    source::SourceMap,
    type_checking::{
        check_compilation_unit_types, complete_compilation_unit_types, standard_environments,
    },
};
fn lower_source(text: &str, unit: bool) -> Result<Program, LoweringError> {
    let mut sources = SourceMap::new();
    let (environment, types) = standard_environments();
    let result = if unit {
        let text = format!("package p\n{text}");
        let (source, file) = parsed(&mut sources, "p/range.ko", &text);
        let inputs = [SourceUnitInput::new("root", "p/range.ko", source, &file)];
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
            .unwrap()
            .validate()
            .unwrap();
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
        complete_compilation_unit_types(&sources, &inputs, &names, &types, &typed).unwrap();
        let typed = typed.validate().unwrap();
        let owned = check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
            .unwrap()
            .validate()
            .unwrap();
        lower_scalar_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &types,
            &typed,
            &owned,
            declaration(&names, "p", "entry"),
        )
        .map(|(program, _)| program)
    } else {
        let analysis = UnitSource::analyze("range.ko", text)
            .expect("integer builtin source must pass strict Unit analysis");
        analysis.assert_frontend_clean();
        let entry = analysis.declaration("entry", SymbolKind::Function);
        analysis.lower_entry(entry).map(|(program, _)| program)
    };
    match &result {
        Ok(program) => diagnostic!(
            "MODE unit={unit} SOURCE {text:?} SSA {}",
            render_program(program)
        ),
        Err(error) => diagnostic!("MODE unit={unit} SOURCE {text:?} LOWER {error:?}"),
    }
    result
}

#[test]
fn range_type_uses_reach_value_abi_or_existing_nullable_boundary() {
    let mut results = Vec::new();
    for (integer, range) in [("Int", "IntRange"), ("Long", "LongRange")] {
        for text in [
            format!("fun entry(value: {range}): Unit {{}}"),
            format!("fun entry(value: {range}): {range} = value"),
            format!("fun entry(value: List<{range}>): Unit {{}}"),
            format!("fun entry(value: Map<Int, {range}>): Unit {{}}"),
            format!("fun entry(value: {range}?): Unit {{}}"),
            format!("fun entry(): Unit {{ val value: {range}? = null }}"),
            format!("class Holder(val value: {range})\nfun entry(value: Holder): Unit {{}}"),
            format!("fun entry(a: {integer}, b: {integer}): {range} = a..b"),
            format!("fun entry(a: {integer}, b: {integer}): List<{range}> = listOf(a..<b)"),
            format!("fun entry(a: {integer}, b: {integer}): Boolean = a in (a..b)"),
            format!("fun entry(a: {integer}, b: {integer}): Boolean = (a..b) == (a..<b)"),
        ] {
            for unit in [false, true] {
                diagnostic!("BEGIN unit={unit} SOURCE {text:?}");
                results.push((
                    text.contains(&format!("{range}?")),
                    lower_source(&text, unit),
                ));
            }
        }
    }
    let mut supported = 0;
    let mut nullable_rejected = 0;
    for (nullable, result) in results {
        if nullable {
            let error = result
                .err()
                .expect("independent nullable Range retains its existing ABI boundary");
            assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
            assert!(error.span.is_some());
            nullable_rejected += 1;
        } else {
            let program = result.expect("ordinary Range value storage is supported");
            verify_program(&program).unwrap();
            let llvm = crate::llvm::render_verified_program(&program).unwrap();
            diagnostic!("TYPE USE LLVM {llvm}");
            supported += 1;
        }
    }
    assert_eq!((supported, nullable_rejected), (36, 8));
}

#[test]
fn unused_range_environment_seeds_do_not_block_ordinary_lowering() {
    for unit in [false, true] {
        let program = lower_source("fun entry(value: Int): Int = value + 1", unit).unwrap();
        verify_program(&program).unwrap();
        let llvm = crate::llvm::render_verified_program(&program).unwrap();
        assert!(llvm.contains("define"));
    }
}

#[test]
fn numeric_iteration_reaches_verified_value_backend() {
    let mut observed = Vec::new();
    for range in ["IntRange", "LongRange"] {
        for unit in [false, true] {
            let text = format!(
                "fun entry(value: {range}): Unit {{ for (item in value) {{ val copy = item }} }}"
            );
            let offset = if unit { "package p\n".len() } else { 0 };
            let start = text.find("for (").unwrap() + offset;
            let end = text.rfind(" }").unwrap() + offset;
            observed.push((lower_source(&text, unit), start, end));
        }
    }
    for (result, start, end) in observed {
        let program = result.expect("numeric provider reaches its value backend");
        verify_program(&program).unwrap();
        assert!(
            program
                .modules
                .iter()
                .flat_map(|module| &module.functions)
                .flat_map(|function| &function.instructions)
                .any(
                    |instruction| matches!(&instruction.origin, super::model::Origin::Source(span)
                if (span.start(), span.end()) == (start, end))
                )
        );
        crate::llvm::render_verified_program(&program).unwrap();
    }
}

pub(in crate::ssa) mod ownership_primitive {
    //! Owned whole-root exchange consumes typed identity and Phase 3 commit facts.
    use lang_frontend::{
        ast::ExpressionId,
        name_resolution::SymbolId,
        ownership_checking::{
            DropPoint, DropTarget, LoanKind as FrontendLoanKind, NonNullAssertionTransferKind,
            OwnershipPrimitiveValueTransfer,
        },
        parser::{BinaryOperator as Operator, Expression},
        source::Span,
        type_checking::{
            AssertionFailureEffect, ExpressionCategory, NullableWhenSubjectCategory,
            OwnershipPrimitiveKind,
        },
    };

    use crate::ssa::lower_frontend::*;

    impl ExpressionLowerer<'_> {
        pub(in crate::ssa) fn lower_ownership_primitive(
            &mut self,
            expression: ExpressionId,
            arguments: &[lang_frontend::parser::CallArgument],
            span: Span,
        ) -> Result<LoweredValue, LoweringError> {
            let descriptor = self
                .typed
                .ownership_primitive(expression)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let [first, second] = arguments else {
                return Err(error(LoweringErrorKind::MissingFact, span));
            };
            if descriptor.expression() != expression
                || descriptor.operands() != [first.value, second.value]
            {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            if descriptor.kind() == OwnershipPrimitiveKind::Replace
                && self.owned.loan_begin(first.value).is_some_and(
                    |fact| matches!(fact.target(), LoanTarget::Place(place) if !place.is_root()),
                )
            {
                return self.lower_field_replace(expression, first, second, span);
            }
            let ty = self.resolve_type(descriptor.value_type(), span)?;
            let target = *self
                .type_ids
                .get(&ty)
                .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
            let first_root =
                self.lower_primitive_root(expression, first.value, target, first.span)?;
            let second_root;
            let replacement;
            match descriptor.kind() {
                OwnershipPrimitiveKind::Replace => {
                    replacement = match self.lower(second.value)? {
                        LoweredValue::Diverged => return Ok(LoweredValue::Diverged),
                        LoweredValue::Unit => {
                            if builtin_type(self.typed, ty) != Some(BuiltinType::Unit) {
                                return Err(error(LoweringErrorKind::MissingFact, second.span));
                            }
                            let (_, result) = self.append(
                                Operation::Constant(ScalarConstant::Unit),
                                vec![EntityType::Value(target)],
                                second.span,
                            )?;
                            value(result[0])
                        }
                        LoweredValue::Value(owner) => owner,
                    };
                    second_root = None;
                }
                OwnershipPrimitiveKind::Swap => {
                    second_root = Some(self.lower_primitive_root(
                        expression,
                        second.value,
                        target,
                        second.span,
                    )?);
                    replacement = self.primitive_root_value(first_root, first.value, span)?;
                }
            }
            // A completely terminating prefix has no executable commit plan. Only require
            // the plan once evaluation actually reaches this normal continuation.
            let plan = self
                .owned
                .ownership_primitive(expression)
                .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
            if plan.descriptor() != &descriptor
                || plan.places().first().map(|place| place.root()) != Some(first_root)
            {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            let first_owner = self.primitive_root_value(first_root, first.value, span)?;
            let first_loan = self.primitive_pending_loan(expression, first.value, span)?;
            let result = match descriptor.kind() {
                OwnershipPrimitiveKind::Replace => {
                    if plan.places().len() != 1 {
                        return Err(error(LoweringErrorKind::MissingFact, span));
                    }
                    let copyable = self.typed.copyability(ty) == Some(Copyability::Copyable);
                    let expected = match (self.typed.expression_category(second.value), copyable) {
                        (Some(ExpressionCategory::Place), true) => {
                            OwnershipPrimitiveValueTransfer::Copy
                        }
                        (Some(ExpressionCategory::Temporary), _) => {
                            OwnershipPrimitiveValueTransfer::Temporary
                        }
                        (Some(ExpressionCategory::Place), false) => {
                            OwnershipPrimitiveValueTransfer::Move
                        }
                        _ => return Err(error(LoweringErrorKind::MissingFact, second.span)),
                    };
                    if plan.new_value_transfer() != Some(expected) {
                        return Err(error(LoweringErrorKind::MissingFact, second.span));
                    }
                    let mut replacement = replacement;
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
                    let (_, results) = self.append(
                        Operation::RootReplace {
                            owner: first_owner,
                            loan: first_loan,
                            replacement,
                        },
                        vec![EntityType::Value(target); 2],
                        span,
                    )?;
                    self.bindings
                        .insert(first_root, LoweredValue::Value(value(results[0])));
                    if builtin_type(self.typed, ty) == Some(BuiltinType::Unit) {
                        self.bindings.insert(first_root, LoweredValue::Unit);
                        LoweredValue::Unit
                    } else {
                        LoweredValue::Value(value(results[1]))
                    }
                }
                OwnershipPrimitiveKind::Swap => {
                    let second_root =
                        second_root.ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                    if plan.places().len() != 2
                        || plan.places()[1].root() != second_root
                        || first_root == second_root
                        || plan.new_value_transfer().is_some()
                    {
                        return Err(error(LoweringErrorKind::MissingFact, span));
                    }
                    let second_owner =
                        self.primitive_root_value(second_root, second.value, span)?;
                    let second_loan =
                        self.primitive_pending_loan(expression, second.value, span)?;
                    let (_, results) = self.append(
                        Operation::RootSwap {
                            owners: [first_owner, second_owner],
                            loans: [first_loan, second_loan],
                        },
                        vec![EntityType::Value(target); 2],
                        span,
                    )?;
                    self.bindings
                        .insert(first_root, LoweredValue::Value(value(results[0])));
                    self.bindings
                        .insert(second_root, LoweredValue::Value(value(results[1])));
                    if builtin_type(self.typed, ty) == Some(BuiltinType::Unit) {
                        self.bindings.insert(first_root, LoweredValue::Unit);
                        self.bindings.insert(second_root, LoweredValue::Unit);
                    }
                    LoweredValue::Unit
                }
            };
            self.release_primitive_unit_operand(expression.index(), first.value.index());
            self.pending_call_loans
                .remove(&(expression.index(), first.value.index()));
            if second_root.is_some() {
                self.release_primitive_unit_operand(expression.index(), second.value.index());
                self.pending_call_loans
                    .remove(&(expression.index(), second.value.index()));
            }
            self.emit_drops(DropPoint::CallReturn(expression))?;
            Ok(result)
        }

        pub(in crate::ssa) fn release_primitive_unit_operand(
            &mut self,
            call: usize,
            operand: usize,
        ) {
            if self.typed.ownership_primitives().iter().any(|primitive| {
                primitive.expression().index() == call
                    && builtin_type(self.typed, primitive.value_type()) == Some(BuiltinType::Unit)
                    && (primitive.operands()[0].index() == operand
                        || (primitive.kind() == OwnershipPrimitiveKind::Swap
                            && primitive.operands()[1].index() == operand))
            }) {
                self.temporaries.remove(&operand);
            }
        }

        fn primitive_pending_loan(
            &self,
            call: ExpressionId,
            operand: ExpressionId,
            span: Span,
        ) -> Result<LoanId, LoweringError> {
            self.pending_call_loans
                .get(&(call.index(), operand.index()))
                .copied()
                .flatten()
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))
        }

        fn primitive_root_value(
            &self,
            symbol: SymbolId,
            operand: ExpressionId,
            span: Span,
        ) -> Result<ValueId, LoweringError> {
            match self.bindings.get(&symbol) {
                Some(LoweredValue::Value(value)) => Ok(*value),
                Some(LoweredValue::Unit) => self
                    .temporaries
                    .get(&operand.index())
                    .copied()
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span)),
                _ => Err(error(LoweringErrorKind::UnsupportedNode, span)),
            }
        }

        fn lower_primitive_root(
            &mut self,
            call: ExpressionId,
            operand: ExpressionId,
            target: SsaTypeId,
            span: Span,
        ) -> Result<SymbolId, LoweringError> {
            let fact = self
                .owned
                .loan_begin(operand)
                .filter(|fact| fact.call() == call)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let LoanTarget::Place(root) = fact.target() else {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            };
            if !root.is_root() {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
            let symbol = root.root();
            if fact.kind() != FrontendLoanKind::Exclusive {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            let begin = fact.begin_span();
            let mut node = operand;
            loop {
                let expression = self
                    .parsed
                    .ast()
                    .expressions()
                    .get(node)
                    .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
                match expression.payload() {
                    Expression::Group { expression } => node = *expression,
                    Expression::Name
                        if self.references.get(&span_key(expression.span())) == Some(&symbol) =>
                    {
                        break;
                    }
                    _ => return Err(error(LoweringErrorKind::UnsupportedNode, span)),
                }
            }
            let mut owner = match self.bindings.get(&symbol).copied() {
                Some(LoweredValue::Value(owner)) => owner,
                Some(LoweredValue::Unit) => {
                    let (_, results) = self.append(
                        Operation::Constant(ScalarConstant::Unit),
                        vec![EntityType::Value(target)],
                        span,
                    )?;
                    value(results[0])
                }
                _ => return Err(error(LoweringErrorKind::UnsupportedNode, span)),
            };
            let ty = self
                .typed
                .symbol_type(symbol)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            if self.typed.copyability(ty) == Some(Copyability::Copyable) {
                let (_, results) = self.append(
                    Operation::Copy { source: owner },
                    vec![EntityType::Value(target)],
                    span,
                )?;
                owner = value(results[0]);
            }
            if builtin_type(self.typed, ty) == Some(BuiltinType::Unit) {
                // The semantic binding stays Unit on every exit. Only the call prefix
                // transports its physical zero-sized root together with the loan.
                self.temporaries.insert(operand.index(), owner);
            } else {
                self.bindings.insert(symbol, LoweredValue::Value(owner));
            }
            let (_, result) = self.append(
                Operation::RootPlace { owner },
                vec![EntityType::Place(target)],
                span,
            )?;
            let EntityId::Place(place) = result[0] else {
                return Err(error(LoweringErrorKind::InvalidModel, span));
            };
            let (_, result) = self.append(
                Operation::BorrowBegin {
                    place,
                    kind: LoanKind::Exclusive,
                },
                vec![EntityType::Loan {
                    kind: LoanKind::Exclusive,
                    target,
                }],
                begin,
            )?;
            let EntityId::Loan(loan) = result[0] else {
                return Err(error(LoweringErrorKind::InvalidModel, span));
            };
            self.pending_call_loans
                .insert((call.index(), operand.index()), Some(loan));
            Ok(symbol)
        }

        pub(in crate::ssa) fn lower_non_null_assertion(
            &mut self,
            expression: ExpressionId,
            span: Span,
        ) -> Result<LoweredValue, LoweringError> {
            let descriptor = self
                .typed
                .non_null_assertion(expression)
                .copied()
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let plan = self
                .owned
                .non_null_assertion(expression)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            if plan.descriptor() != &descriptor || descriptor.expression() != expression {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            let nullable = self.resolve_type(descriptor.nullable_type(), span)?;
            if let Some(&result) = self.type_ids.get(&nullable)
                && self.map_results.get(&result).copied().is_some()
                && plan.non_null_transfer() == NonNullAssertionTransferKind::Copy
            {
                if plan.non_null_transfer() != NonNullAssertionTransferKind::Copy
                    || descriptor.copyability()
                        != lang_frontend::type_checking::Copyability::Copyable
                {
                    return Err(error(LoweringErrorKind::MissingFact, span));
                }
                let operand = self.require_value(descriptor.operand())?;
                return self.unwrap_map_result(operand, descriptor.operator_span());
            }
            if plan.non_null_transfer() != NonNullAssertionTransferKind::Consume
                || !matches!(
                    descriptor.source_category(),
                    NullableWhenSubjectCategory::OwnedRoot | NullableWhenSubjectCategory::Temporary
                )
            {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
            let AssertionFailureEffect::Abort = plan.null_effect();
            let inner = self.expression_ssa_type(expression, span)?;
            let mut operands = vec![descriptor.operand()];
            let mut evaluated = descriptor.operand();
            loop {
                let node = self
                    .parsed
                    .ast()
                    .expressions()
                    .get(evaluated)
                    .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
                let Expression::Group { expression } = node.payload() else {
                    break;
                };
                evaluated = *expression;
                operands.push(evaluated);
            }
            let owner = match self.lower_expression(evaluated)? {
                LoweredValue::Value(owner) => owner,
                LoweredValue::Diverged => return Ok(LoweredValue::Diverged),
                LoweredValue::Unit => return Err(error(LoweringErrorKind::MissingFact, span)),
            };
            if descriptor.source_category() == NullableWhenSubjectCategory::Temporary {
                self.temporaries.insert(descriptor.operand().index(), owner);
            }
            let baseline = self.bindings.clone();
            let carried = self.linear_binding_slots(&baseline, span)?;
            let owner_slot = carried
                .slots
                .iter()
                .position(|slot| slot.source == EntityId::Value(owner))
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let types = carried.slots.iter().map(|slot| slot.ty).collect::<Vec<_>>();
            let null = self
                .function
                .add_block(types.clone(), Origin::Source(span))
                .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
            let mut proven_types = types;
            proven_types.push(EntityType::Loan {
                kind: LoanKind::Shared,
                target: inner,
            });
            let non_null = self
                .function
                .add_block(proven_types, Origin::Source(span))
                .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
            let EntityId::Loan(proof) = self
                .function
                .block(non_null)
                .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))?
                .parameters[carried.slots.len()]
            else {
                return Err(error(LoweringErrorKind::InvalidModel, span));
            };
            let edge = |target| Edge {
                target,
                arguments: carried.slots.iter().map(|slot| slot.source).collect(),
            };
            self.function
                .set_terminator(
                    self.block,
                    TerminatorKind::NullableBranch {
                        owner,
                        when_null: edge(null),
                        when_non_null: edge(non_null),
                        view: proof,
                    },
                    Origin::Source(span),
                )
                .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
            self.function
                .set_terminator(
                    null,
                    TerminatorKind::Abort,
                    Origin::Source(descriptor.operator_span()),
                )
                .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
            self.block = non_null;
            self.bindings = self.rebind_linear_bindings(&baseline, non_null, &carried, span)?;
            let owner = value(
                self.function
                    .block(non_null)
                    .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))?
                    .parameters[owner_slot],
            );
            self.validate_non_null_proof_types(
                crate::ssa::lower_frontend::NonNullProof::introduced(
                    proof,
                    owner,
                    inner,
                    crate::ssa::lower_frontend::non_null_proof::ProofScope::Assertion(expression),
                ),
                span,
            )?;
            let consumed_symbols = self
                .bindings
                .iter()
                .filter_map(|(&symbol, binding)| {
                    matches!(binding, LoweredValue::Value(value) if *value == owner)
                        .then_some(symbol)
                })
                .collect::<Vec<_>>();
            for &symbol in &consumed_symbols {
                self.reject_live_while_proof(symbol, span)?;
                if let Some(&existing) = self.non_null_bindings.get(&symbol) {
                    self.validate_non_null_proof(symbol, existing, &self.bindings, span)?;
                }
            }
            let (_, result) = self.append(
                Operation::NullableTake { owner, proof },
                vec![EntityType::Value(inner)],
                span,
            )?;
            for symbol in consumed_symbols {
                self.bindings.remove(&symbol);
                self.non_null_bindings.remove(&symbol);
            }
            self.temporaries.retain(|_, value| *value != owner);
            for operand in operands {
                self.emit_drops(DropPoint::AfterExpression(operand))?;
            }
            Ok(LoweredValue::Value(value(result[0])))
        }

        pub(in crate::ssa) fn lower_nullable_comparison(
            &mut self,
            expression: ExpressionId,
            span: Span,
        ) -> Result<Option<LoweredValue>, LoweringError> {
            let Some(d) = self.typed.null_comparison(expression) else {
                return self.lower_temporary_nullable_comparison(expression, span);
            };
            let ty = self.resolve_type(d.nullable_type(), span)?;
            let target = self
                .type_ids
                .get(&ty)
                .copied()
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let aggregate_presence = self.map_results.contains_key(&target);
            let operation = if let Some(&source) = self.borrow_bindings.get(&d.symbol()) {
                if self.nullable_maps.contains_key(&target) {
                    return Err(error(LoweringErrorKind::UnsupportedNode, span));
                }
                if aggregate_presence {
                    return Ok(None);
                }
                Operation::NullableLoanIsNull { source }
            } else if let Some(LoweredValue::Value(owner)) = self.bindings.get(&d.symbol()).copied()
            {
                if aggregate_presence {
                    Operation::AggregateProject {
                        aggregate: owner,
                        field: 0,
                    }
                } else {
                    Operation::NullableIsNull { owner }
                }
            } else {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            };
            let boolean = self.expression_ssa_type(expression, span)?;
            let (_, results) = self.append(operation, vec![EntityType::Value(boolean)], span)?;
            let mut result = value(results[0]);
            if d.non_null_when_true() != aggregate_presence {
                let (_, results) = self.append(
                    Operation::BooleanNot { operand: result },
                    vec![EntityType::Value(boolean)],
                    span,
                )?;
                result = value(results[0]);
            }
            let node = self
                .parsed
                .ast()
                .expressions()
                .get(expression)
                .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
            let Expression::Binary { left, right, .. } = *node.payload() else {
                return Err(error(LoweringErrorKind::MissingFact, span));
            };
            self.emit_null_condition_drops(left)?;
            self.emit_null_condition_drops(right)?;
            self.emit_drops(DropPoint::AfterBinaryOperands(expression))?;
            Ok(Some(LoweredValue::Value(result)))
        }

        pub(in crate::ssa) fn emit_null_condition_drops(
            &mut self,
            expression: ExpressionId,
        ) -> Result<(), LoweringError> {
            let node = self
                .parsed
                .ast()
                .expressions()
                .get(expression)
                .map_err(|_| LoweringError {
                    kind: LoweringErrorKind::MissingFact,
                    span: None,
                })?;
            match node.payload().clone() {
                Expression::Binary { left, right, .. } => {
                    self.emit_null_condition_drops(left)?;
                    self.emit_null_condition_drops(right)?;
                }
                Expression::Group { expression } => self.emit_null_condition_drops(expression)?,
                _ => {}
            }
            self.emit_drops(DropPoint::AfterExpression(expression))
        }

        fn lower_temporary_nullable_comparison(
            &mut self,
            expression: ExpressionId,
            span: Span,
        ) -> Result<Option<LoweredValue>, LoweringError> {
            let expressions = self.parsed.ast().expressions();
            let node = expressions
                .get(expression)
                .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
            let Expression::Binary {
                left,
                operator,
                right,
                ..
            } = *node.payload()
            else {
                return Ok(None);
            };
            if !matches!(operator, Operator::Equal | Operator::NotEqual) {
                return Ok(None);
            }
            let operand = if self.is_null_literal(right) {
                left
            } else if self.is_null_literal(left) {
                right
            } else {
                return Ok(None);
            };
            let mut leaf = operand;
            let origin = loop {
                let node = expressions
                    .get(leaf)
                    .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
                if let Expression::Group { expression } = *node.payload() {
                    leaf = expression;
                } else {
                    break node.span();
                }
            };
            let point = DropPoint::AfterBinaryOperands(expression);
            let target = DropTarget::Temporary(leaf);
            let mut facts = self
                .owned
                .drops()
                .iter()
                .filter(|fact| fact.point() == point && fact.target() == target);
            let Some(fact) = facts.next() else {
                return Ok(None);
            };
            if facts.next().is_some() || fact.value_origin() != origin {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            let target = self.expression_ssa_type(leaf, origin)?;
            let aggregate_presence = self.map_results.contains_key(&target);
            if !aggregate_presence
                && !self.nullable_maps.contains_key(&target)
                && !matches!(
                    self.ssa_types.get(target.index()),
                    Some(crate::ssa::model::SsaTypeKind::NullableHandle { .. })
                )
            {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
            let owner = self.require_value(operand)?;
            let operation = if aggregate_presence {
                Operation::AggregateProject {
                    aggregate: owner,
                    field: 0,
                }
            } else {
                Operation::NullableIsNull { owner }
            };
            let boolean = self.expression_ssa_type(expression, span)?;
            let (_, results) = self.append(operation, vec![EntityType::Value(boolean)], span)?;
            let mut result = value(results[0]);
            if (operator == Operator::NotEqual) != aggregate_presence {
                let (_, results) = self.append(
                    Operation::BooleanNot { operand: result },
                    vec![EntityType::Value(boolean)],
                    span,
                )?;
                result = value(results[0]);
            }
            self.emit_drops(point)?;
            Ok(Some(LoweredValue::Value(result)))
        }
    }
}
