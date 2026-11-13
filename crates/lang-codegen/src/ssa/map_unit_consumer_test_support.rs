//! Original MemoryDocument contracts for the migrated Map control consumers.
//! Every original non-generic function is an independent Unit root, even if uncalled.
use crate::native_tests::boxed_enum_tests::unit_source::UnitSource;
use crate::test_support::diagnostics::diagnostic;
use lang_frontend::{
    name_resolution::{SourceUnitKey, SymbolKind},
    parser::Item,
};

use super::model::{
    EntityType, FunctionId, LoanKind, Operation, Origin, Program, SsaTypeKind, TerminatorKind,
};

struct RootProgram {
    name: String,
    program: Program,
    entry: FunctionId,
}

pub(super) fn assert_source(text: &str, roots: &[&str]) {
    let analysis = analyze(text);
    lower_roots(&analysis, "lowering.ko", text, roots);
}

pub(super) fn assert_return_controls(name: &str, text: &str, roots: &[&str]) {
    let analysis = analyze(text);
    for root in lower_roots(&analysis, "lowering.ko", text, roots) {
        assert_returned_owner_not_dropped(&root.program);
    }
    diagnostic!("CONTROL {name}: VERIFIED_LLVM");
}

pub(super) fn assert_outer_resource(name: &str, text: &str) {
    let analysis = analyze(text);
    let programs = lower_roots(&analysis, "lowering.ko", text, &["operand", "cleanup"]);
    for root in &programs {
        assert_returned_owner_not_dropped(&root.program);
    }
    let deinits = analysis
        .parsed()
        .ast()
        .items()
        .iter()
        .filter_map(|(_, node)| {
            matches!(node.payload(), Item::Deinit { .. }).then_some(node.span())
        })
        .collect::<Vec<_>>();
    let [deinit] = deinits.as_slice() else {
        panic!("one original Guard deinit");
    };
    assert_eq!((deinit.start(), deinit.end()), (32, 63));
    let cleanup = programs.iter().find(|root| root.name == "cleanup").unwrap();
    let definitions = cleanup
        .program
        .modules
        .iter()
        .flat_map(|module| &module.functions)
        .filter(|function| function.origin == Origin::Source(*deinit))
        .collect::<Vec<_>>();
    let [definition] = definitions.as_slice() else {
        panic!("cleanup must emit the real Guard deinit dependency");
    };
    let Some(EntityType::Loan {
        kind: LoanKind::Shared,
        target,
    }) = definition.receiver()
    else {
        panic!("Guard deinit must retain its exact shared owner receiver");
    };
    let module = cleanup.program.module(definition.id.module()).unwrap();
    assert!(matches!(
        module.type_kind(target),
        Some(SsaTypeKind::HeapOwner { .. })
    ));
    assert_eq!(module.deinit(target), Some(definition.id));
    let function = cleanup
        .program
        .module(cleanup.entry.module())
        .unwrap()
        .function(cleanup.entry)
        .unwrap();
    assert!(function.instructions.iter().any(|instruction| {
        matches!(instruction.operation, Operation::HeapAllocate { owner, .. } if owner == target)
            && analysis.sources().slice(instruction.origin.span()).unwrap() == "Guard(\"outer\")"
    }), "the emitted deinit belongs to the actual original outer Guard owner");
    diagnostic!("CONTROL {name}: VERIFIED_LLVM");
}

pub(super) fn assert_trusted_range(text: &str) {
    let analysis =
        UnitSource::analyze_with_type_environment("trusted.ko", text, |sources, source, types| {
            types.authorize_range_source(sources, source).unwrap();
            types
                .authorize_range_extension_source(sources, source)
                .unwrap();
        })
        .unwrap_or_else(|error| panic!("original trusted range admission: {error:?}"));
    analysis.assert_frontend_clean();
    let programs = lower_roots(&analysis, "trusted.ko", text, &["main"]);
    let [root] = programs.as_slice() else {
        panic!("one original main root");
    };
    let templates = analysis
        .names()
        .names()
        .index()
        .declarations()
        .iter()
        .filter(|declaration| {
            declaration.kind() == SymbolKind::Function
                && analysis
                    .types()
                    .signatures()
                    .declaration(declaration.id())
                    .and_then(|signature| signature.callable())
                    .is_some_and(|signature| !signature.type_parameters().is_empty())
        })
        .collect::<Vec<_>>();
    assert_eq!(
        templates.len(),
        12,
        "the complete original generic library remains admitted"
    );
    let take = templates
        .iter()
        .filter(|declaration| {
            let span = analysis.declaration_span(declaration.id());
            declaration.name() == "take" && (span.start(), span.end()) == (93, 343)
        })
        .collect::<Vec<_>>();
    let [take] = take.as_slice() else {
        panic!("the real List<T> take declaration, not a synthetic generic entry");
    };
    let span = analysis.declaration_span(take.id());
    let instances = root
        .program
        .modules
        .iter()
        .flat_map(|module| &module.functions)
        .filter(|function| function.origin == Origin::Source(span))
        .collect::<Vec<_>>();
    let [instance] = instances.as_slice() else {
        panic!("main must emit its actual take instance");
    };
    let main = root
        .program
        .module(root.entry.module())
        .unwrap()
        .function(root.entry)
        .unwrap();
    assert!(main.instructions.iter().any(|instruction| {
        matches!(instruction.operation, Operation::RangeCall { callee, .. } if callee == instance.id)
    }));
    // The other eleven templates remain FE/ownership coverage, not codegen claims.
}

fn analyze(text: &str) -> UnitSource {
    let analysis = UnitSource::analyze("lowering.ko", text)
        .unwrap_or_else(|error| panic!("original Map source admission: {error:?}"));
    analysis.assert_frontend_clean();
    analysis
}

fn lower_roots(
    analysis: &UnitSource,
    path: &str,
    text: &str,
    expected_roots: &[&str],
) -> Vec<RootProgram> {
    let index = analysis.names().names().index();
    let [source] = index.source_units() else {
        panic!("one original MemoryDocument");
    };
    assert_eq!(source.key(), &SourceUnitKey::MemoryDocument);
    assert_eq!(source.source_id(), analysis.parsed().source_id());
    assert_eq!(
        analysis.sources().source_name(source.source_id()).unwrap(),
        path
    );
    assert_eq!(
        analysis.sources().source_text(source.source_id()).unwrap(),
        text
    );
    let mut declarations = index.declarations().iter().collect::<Vec<_>>();
    declarations.sort_by_key(|declaration| declaration.name_span().start());
    let mut roots = Vec::new();
    for declaration in declarations {
        assert_eq!(declaration.source_unit(), source.id());
        assert_eq!(declaration.package(), source.package());
        assert_eq!(declaration.name_span().source_id(), source.source_id());
        assert!(analysis.parsed().roots().contains(&declaration.root()));
        if declaration.kind() != SymbolKind::Function {
            continue;
        }
        let signature = analysis
            .types()
            .signatures()
            .declaration(declaration.id())
            .and_then(|signature| signature.callable())
            .expect("every original function has a real callable signature");
        if signature.type_parameters().is_empty() {
            roots.push((declaration, signature.parameters().len()));
        }
    }
    assert_eq!(
        roots
            .iter()
            .map(|(declaration, _)| declaration.name())
            .collect::<Vec<_>>(),
        expected_roots,
        "all original non-generic roots, in original source order"
    );
    roots
        .into_iter()
        .map(|(declaration, parameters)| {
            let name = declaration.name();
            let span = analysis.declaration_span(declaration.id());
            let (program, entry) = analysis
                .lower_entry(declaration.id())
                .unwrap_or_else(|error| panic!("{path} root {name}: {error:?}"));
            let function = program
                .module(entry.module())
                .unwrap()
                .function(entry)
                .unwrap();
            assert_eq!(function.origin, Origin::Source(span));
            super::verify::verify_program(&program)
                .unwrap_or_else(|error| panic!("{path} root {name} SSA: {error:?}"));
            let llvm = if name == "main" && parameters == 0 {
                crate::llvm::render_verified_program_with_entry(&program, entry)
            } else {
                crate::llvm::render_verified_program(&program)
            };
            llvm.unwrap_or_else(|error| panic!("{path} root {name} LLVM: {error:?}"));
            RootProgram {
                name: name.to_owned(),
                program,
                entry,
            }
        })
        .collect()
}

fn assert_returned_owner_not_dropped(program: &Program) {
    for function in program.modules.iter().flat_map(|module| &module.functions) {
        for block in &function.blocks {
            if let TerminatorKind::Return { values } = &block.terminator.as_ref().unwrap().kind {
                for value in values {
                    assert!(
                        !function.instructions.iter().any(|instruction| {
                            matches!(instruction.operation, Operation::Drop { owner } if owner == *value)
                        }),
                        "returned owner is never also dropped"
                    );
                }
            }
        }
    }
}

pub(in crate::ssa) mod string_clone {
    //! Validated String clone / field-loan lowering and Copyable Map result comparison/unwrap helpers.
    use lang_frontend::{
        ast::ExpressionId,
        ownership_checking::DropPoint,
        parser::BinaryOperator,
        source::Span,
        type_checking::{ParameterMode, StringOperationKind},
    };

    use crate::ssa::lower_frontend::{
        ComparisonOperator, EntityId, EntityType, ExpressionLowerer, LoanKind, LoweredValue,
        LoweringError, LoweringErrorKind, Operation, ScalarConstant, SsaTypeId, ValueId, error,
        value,
    };

    impl ExpressionLowerer<'_> {
        pub(in crate::ssa) fn map_result_expression_type(
            &self,
            expression: ExpressionId,
            span: Span,
        ) -> Result<Option<SsaTypeId>, LoweringError> {
            let ty = self
                .typed
                .expression_type(expression)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let ty = self.resolve_type(ty, span)?;
            Ok(self
                .type_ids
                .get(&ty)
                .copied()
                .filter(|ty| self.map_results.contains_key(ty)))
        }

        pub(in crate::ssa) fn is_map_result_value(&self, source: ValueId) -> bool {
            self.function
                .entity(EntityId::Value(source))
                .is_some_and(|data| self.map_results.contains_key(&data.ty.semantic_type()))
        }

        pub(in crate::ssa) fn unwrap_map_result(
            &mut self,
            result: ValueId,
            span: Span,
        ) -> Result<LoweredValue, LoweringError> {
            let ty = self
                .function
                .entity(EntityId::Value(result))
                .and_then(|data| self.map_results.get(&data.ty.semantic_type()).copied())
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let (_, output) = self.append(
                Operation::MapResultUnwrap { result },
                vec![EntityType::Value(ty)],
                span,
            )?;
            Ok(LoweredValue::Value(value(output[0])))
        }

        pub(in crate::ssa) fn wrap_map_result(
            &mut self,
            payload: ValueId,
            result: SsaTypeId,
            span: Span,
        ) -> Result<ValueId, LoweringError> {
            let present = self
                .typed
                .types()
                .builtin(lang_frontend::type_checking::BuiltinType::Boolean)
                .and_then(|ty| self.type_ids.get(&ty).copied())
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let (_, flag) = self.append(
                Operation::Constant(ScalarConstant::Boolean(true)),
                vec![EntityType::Value(present)],
                span,
            )?;
            let (_, wrapped) = self.append(
                Operation::AggregateConstruct {
                    aggregate: result,
                    fields: vec![value(flag[0]), payload],
                },
                vec![EntityType::Value(result)],
                span,
            )?;
            Ok(value(wrapped[0]))
        }

        pub(in crate::ssa) fn is_null_literal(&self, expression: ExpressionId) -> bool {
            crate::ssa::lowering_support::is_null_literal(self.parsed, expression)
        }

        pub(in crate::ssa) fn lower_map_result_binary(
            &mut self,
            left: ExpressionId,
            operator: BinaryOperator,
            right: ExpressionId,
            expression: ExpressionId,
            span: Span,
        ) -> Result<Option<LoweredValue>, LoweringError> {
            if !matches!(operator, BinaryOperator::Equal | BinaryOperator::NotEqual) {
                return Ok(None);
            }
            let left_type = self.map_result_expression_type(left, span)?;
            let right_type = self.map_result_expression_type(right, span)?;
            if left_type.is_none() && right_type.is_none() {
                return Ok(None);
            }
            let boolean = self.expression_ssa_type(expression, span)?;
            let nullable = if self.is_null_literal(right) && left_type.is_some() {
                Some(left)
            } else if self.is_null_literal(left) && right_type.is_some() {
                Some(right)
            } else {
                None
            };
            if let Some(nullable) = nullable {
                let aggregate = self.require_value(nullable)?;
                let (_, present) = self.append(
                    Operation::AggregateProject {
                        aggregate,
                        field: 0,
                    },
                    vec![EntityType::Value(boolean)],
                    span,
                )?;
                let present = value(present[0]);
                if operator == BinaryOperator::NotEqual {
                    return Ok(Some(LoweredValue::Value(present)));
                }
                let (_, absent) = self.append(
                    Operation::BooleanNot { operand: present },
                    vec![EntityType::Value(boolean)],
                    span,
                )?;
                return Ok(Some(LoweredValue::Value(value(absent[0]))));
            }
            let mut left = self.require_value(left)?;
            let mut right = self.require_value(right)?;
            if let Some(ty) = left_type
                && right_type.is_none()
            {
                right = self.wrap_map_result(right, ty, span)?;
            }
            if let Some(ty) = right_type
                && left_type.is_none()
            {
                left = self.wrap_map_result(left, ty, span)?;
            }
            let comparison = if operator == BinaryOperator::Equal {
                ComparisonOperator::Equal
            } else {
                ComparisonOperator::NotEqual
            };
            let (_, result) = self.append(
                Operation::Compare {
                    operator: comparison,
                    left,
                    right,
                },
                vec![EntityType::Value(boolean)],
                span,
            )?;
            Ok(Some(LoweredValue::Value(value(result[0]))))
        }

        pub(in crate::ssa) fn lower_string_clone(
            &mut self,
            expression: ExpressionId,
        ) -> Result<LoweredValue, LoweringError> {
            let span = self.expression_span(expression)?;
            let descriptor = self
                .typed
                .string_operation(expression)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let effect = self
                .owned
                .string_effect(expression)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            if descriptor.kind() != StringOperationKind::Clone
                || descriptor.receiver_mode() != ParameterMode::Borrow
                || descriptor.result_mode() != ParameterMode::Value
                || effect.receiver() != descriptor.receiver()
                || effect.receiver_type() != descriptor.receiver_type()
                || effect.result_type() != descriptor.result_type()
                || effect.kind() != descriptor.kind()
            {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            let receiver = descriptor.receiver();
            let receiver_span = self.expression_span(receiver)?;
            let (source, created) = if self.clone_field_projection(receiver)?.is_some() {
                let mut created = Vec::new();
                let source = self.clone_field_loan(receiver, &mut created)?;
                (source, created)
            } else {
                let (source, created) =
                    self.lower_borrow_argument(expression, receiver, receiver_span)?;
                (source, if created { vec![source] } else { Vec::new() })
            };
            let result_type = self.expression_ssa_type(expression, span)?;
            let (_, results) = self.append(
                Operation::StringClone { source },
                vec![EntityType::Value(result_type)],
                span,
            )?;
            for loan in created.into_iter().rev() {
                self.append(Operation::BorrowEnd { loan }, Vec::new(), span)?;
            }
            self.emit_drops(DropPoint::AfterExpression(receiver))?;
            self.emit_drops(DropPoint::CallReturn(expression))?;
            Ok(LoweredValue::Value(value(results[0])))
        }

        fn clone_field_projection(
            &self,
            expression: ExpressionId,
        ) -> Result<
            Option<lang_frontend::type_checking::AggregateProjectionDescriptor>,
            LoweringError,
        > {
            let node = self
                .parsed
                .ast()
                .expressions()
                .get(expression)
                .map_err(|_| LoweringError {
                    kind: LoweringErrorKind::MissingFact,
                    span: None,
                })?;
            if let lang_frontend::parser::Expression::Group { expression } = node.payload() {
                return self.clone_field_projection(*expression);
            }
            Ok(self.typed.aggregate_projection(expression))
        }

        pub(in crate::ssa) fn clone_field_loan(
            &mut self,
            expression: ExpressionId,
            created: &mut Vec<crate::ssa::lower_frontend::LoanId>,
        ) -> Result<crate::ssa::lower_frontend::LoanId, LoweringError> {
            use lang_frontend::type_checking::{
                AggregateProjectionReceiver, NominalKind, TypeKind,
            };
            let span = self.expression_span(expression)?;
            let target = self.expression_ssa_type(expression, span)?;
            if let Some((loan, loans)) = self.readonly_view(expression)? {
                created.extend(loans);
                return Ok(loan);
            }

            if let Some(projection) = self.clone_field_projection(expression)? {
                let AggregateProjectionReceiver::Expression(receiver) = projection.receiver()
                else {
                    return Err(error(LoweringErrorKind::UnsupportedNode, span));
                };
                let receiver_type = self
                    .typed
                    .expression_type(receiver)
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                let receiver_type = self.resolve_type(receiver_type, span)?;
                let Some(TypeKind::Nominal { nominal, .. }) = self.typed.types().get(receiver_type)
                else {
                    return Err(error(LoweringErrorKind::UnsupportedNode, span));
                };
                let nominal = self
                    .typed
                    .nominals()
                    .iter()
                    .find(|value| value.id() == *nominal)
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                let field = nominal
                    .fields()
                    .iter()
                    .position(|field| *field == projection.field())
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                let kind = nominal.kind();
                let base = self.clone_field_loan(receiver, created)?;
                let operation = match kind {
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
                return Ok(loan);
            }
            let node = self
                .parsed
                .ast()
                .expressions()
                .get(expression)
                .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
            if let lang_frontend::parser::Expression::Group { expression } = node.payload() {
                return self.clone_field_loan(*expression, created);
            }
            if matches!(node.payload(), lang_frontend::parser::Expression::Name)
                && let Some(symbol) = self
                    .references
                    .get(&crate::ssa::lower_frontend::span_key(node.span()))
                && let Some(loan) = self.borrow_bindings.get(symbol).copied()
            {
                return Ok(loan);
            }
            if self.references.get(&crate::ssa::lower_frontend::span_key(node.span())).is_some_and(|symbol| {
                self.owned.loans().iter().any(|active| {
                    self.pending_call_loans.contains_key(&(active.call().index(), active.argument().index()))
                        && active.kind() == lang_frontend::ownership_checking::LoanKind::Exclusive
                        && matches!(active.target(), lang_frontend::ownership_checking::LoanTarget::Place(path)
                            if !path.is_root() && path.root() == *symbol)
                })
            }) {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
            let owner = self.require_value(expression)?;
            let (_, root) = self.append(
                Operation::RootPlace { owner },
                vec![EntityType::Place(target)],
                span,
            )?;
            let (_, result) = self.append(
                Operation::BorrowBegin {
                    place: crate::ssa::lower_frontend::place(root[0]),
                    kind: LoanKind::Shared,
                },
                vec![EntityType::Loan {
                    kind: LoanKind::Shared,
                    target,
                }],
                span,
            )?;
            let EntityId::Loan(loan) = result[0] else {
                return Err(error(LoweringErrorKind::InvalidModel, span));
            };
            created.push(loan);
            Ok(loan)
        }
    }

    pub(in crate::ssa) mod declarations {
        //! Source function declaration collection, statement dispatch, constant materialization, and local binding delivery.
        use crate::ssa::lower_frontend::{
            AssignmentOperator, Copyability, EntityId, EntityType, ExpressionLowerer, LoweredValue,
            LoweringError, LoweringErrorKind, Operation, TypeId, TypeKind, ValueId, error,
            orchestrate::{self, unwrap_modified},
            present_name, readonly_receiver, value,
        };
        use crate::ssa::{constant_materialization, statement_core::StatementLowering};
        use lang_frontend::{
            ast::{ExpressionId, ItemId, StatementId},
            name_resolution::{NameResolution, SymbolId, SymbolKind},
            ownership_checking::DropPoint,
            parser::{ClassifierKind, Item, ParsedFile},
            source::Span,
            type_checking::{CallableDescriptor, TypedFile},
        };

        impl StatementLowering for ExpressionLowerer<'_> {
            fn lower_inner(
                &mut self,
                statement: StatementId,
            ) -> Result<LoweredValue, LoweringError> {
                self.lower_statement_inner(statement)
            }

            fn emit_after_statement(
                &mut self,
                statement: StatementId,
            ) -> Result<(), LoweringError> {
                self.emit_drops(DropPoint::AfterStatement(statement))
            }
        }

        impl ExpressionLowerer<'_> {
            /// Consume the original owner before changing its SSA identity for approved T -> T?.
            pub(in crate::ssa) fn adapt_owned_value_to_expected(
                &mut self,
                expression: ExpressionId,
                owner: ValueId,
                expected: TypeId,
                span: Span,
            ) -> Result<ValueId, LoweringError> {
                let actual = self
                    .typed
                    .expression_type(expression)
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                let actual = self.resolve_type(actual, span)?;
                let expected = self.resolve_type(expected, span)?;
                if actual == expected {
                    return Ok(owner);
                }
                if !matches!(self.typed.types().get(expected), Some(TypeKind::Nullable(inner)) if *inner == actual)
                {
                    return Err(error(LoweringErrorKind::MissingFact, span));
                }
                let nullable = self
                    .type_ids
                    .get(&expected)
                    .copied()
                    .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
                if self.map_results.contains_key(&nullable)
                    && self.typed.copyability(actual) == Some(Copyability::Copyable)
                {
                    return self.wrap_map_result(owner, nullable, span);
                }
                self.forget_delivered_owners(&[owner]);
                let (_, results) = self.append(
                    Operation::NullableWrap { nullable, owner },
                    vec![EntityType::Value(nullable)],
                    span,
                )?;
                Ok(value(results[0]))
            }

            /// Adapt only the already-mapped owned source-call boundary, preserving its
            /// existing argument slot so later operands carry the wrapped owner.
            pub(in crate::ssa) fn adapt_pending_source_value_argument(
                &mut self,
                expression: ExpressionId,
                owner: ValueId,
                expected: TypeId,
                span: Span,
            ) -> Result<ValueId, LoweringError> {
                use crate::ssa::model::SsaTypeKind;
                let qualified = (|| {
                    let actual = self
                        .resolve_type(self.typed.expression_type(expression)?, span)
                        .ok()?;
                    let expected = self.resolve_type(expected, span).ok()?;
                    if self.typed.copyability(actual) != Some(Copyability::MoveOnly)
                        || !matches!(self.typed.types().get(expected), Some(TypeKind::Nullable(inner)) if *inner == actual)
                        || self.temporaries.get(&expression.index()).copied() != Some(owner)
                    {
                        return None;
                    }
                    let actual_ssa = *self.type_ids.get(&actual)?;
                    let nullable = *self.type_ids.get(&expected)?;
                    if !matches!(
                        self.ssa_types.get(actual_ssa.index()),
                        Some(SsaTypeKind::HeapOwner { .. })
                    ) || !matches!(self.ssa_types.get(nullable.index()), Some(SsaTypeKind::NullableHandle { inner }) if *inner == actual_ssa)
                        || self
                            .function
                            .entity(EntityId::Value(owner))
                            .map(|entity| entity.ty)
                            != Some(EntityType::Value(actual_ssa))
                    {
                        return None;
                    }
                    Some(expected)
                })();
                let Some(expected) = qualified else {
                    return Ok(owner);
                };
                let wrapped =
                    self.adapt_owned_value_to_expected(expression, owner, expected, span)?;
                self.temporaries.insert(expression.index(), wrapped);
                Ok(wrapped)
            }

            pub(in crate::ssa) fn lower_local_variable(
                &mut self,
                declaration: ItemId,
                span: Span,
            ) -> Result<LoweredValue, LoweringError> {
                let (item, _) = orchestrate::unwrap_modified(self.parsed, declaration)?;
                let Item::Variable {
                    name, initializer, ..
                } = item
                else {
                    return Err(error(LoweringErrorKind::MissingFact, span));
                };
                if let Some(fact) = self
                    .owned
                    .borrow_results()
                    .bindings()
                    .iter()
                    .find(|fact| fact.initializer() == initializer)
                {
                    return self.lower_result_binding(fact.binding(), initializer, span);
                }
                let mut lowered = self.lower(initializer)?;
                if matches!(lowered, LoweredValue::Diverged) {
                    return Ok(lowered);
                }
                let name_span = present_name(name)
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                if self.source_slice(name_span)? == "_" {
                    return Ok(LoweredValue::Unit);
                }
                let symbol = self.declaration_symbol(name_span, SymbolKind::Variable)?;
                let declared = self
                    .typed
                    .symbol_type(symbol)
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                if let LoweredValue::Value(owner) = lowered {
                    let concrete = self.resolve_type(declared, span)?;
                    if self.typed.copyability(concrete) == Some(Copyability::MoveOnly) {
                        self.forget_delivered_owners(&[owner]);
                    } else {
                        self.temporaries.retain(|_, temporary| *temporary != owner);
                    }
                }
                if let LoweredValue::Value(owner) = lowered {
                    lowered = LoweredValue::Value(self.adapt_owned_value_to_expected(
                        initializer,
                        owner,
                        declared,
                        span,
                    )?);
                }
                self.bindings.insert(symbol, lowered);
                Ok(LoweredValue::Unit)
            }

            pub(in crate::ssa) fn deliver_assignment_rhs(
                &mut self,
                operator: AssignmentOperator,
                expression: ExpressionId,
                assigned: LoweredValue,
                span: Span,
            ) {
                if operator == AssignmentOperator::Assign
                    && let LoweredValue::Value(owner) = assigned
                    && let Some(template) = self.typed.expression_type(expression)
                    && let Ok(concrete) = self.resolve_type(template, span)
                    && self.typed.copyability(concrete) == Some(Copyability::MoveOnly)
                {
                    self.forget_delivered_owners(&[owner]);
                }
            }

            pub(in crate::ssa) fn lower_constant(
                &mut self,
                expression: ExpressionId,
            ) -> Result<Option<LoweredValue>, LoweringError> {
                let Some(descriptor) = self
                    .typed
                    .constants()
                    .and_then(|facts| facts.use_at(expression))
                else {
                    return Ok(None);
                };
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
                let plan = self
                    .owned
                    .constant_materializations()
                    .and_then(|facts| facts.plan_at(expression))
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                let planned = plan.descriptor();
                if planned.expression() != descriptor.expression()
                    || planned.target() != descriptor.target()
                    || planned.ty() != descriptor.ty()
                    || planned.value() != descriptor.value()
                {
                    return Err(error(LoweringErrorKind::MismatchedAnalysis, span));
                }
                let ty = self.expression_ssa_type(expression, span)?;
                let operation =
                    constant_materialization::operation(plan.kind(), descriptor.value(), ty)
                        .map_err(|kind| error(kind, span))?;
                let (_, results) = self.append(operation, vec![EntityType::Value(ty)], span)?;
                Ok(Some(LoweredValue::Value(value(results[0]))))
            }
        }

        pub(in crate::ssa) struct FunctionDeclaration {
            pub(in crate::ssa) item_id: ItemId,
            pub(in crate::ssa) item: Item,
            pub(in crate::ssa) symbol: SymbolId,
            pub(in crate::ssa) callable: CallableDescriptor,
            pub(in crate::ssa) span: Span,
        }

        pub(in crate::ssa) fn collect_functions(
            parsed: &ParsedFile,
            names: &NameResolution,
            typed: &TypedFile,
        ) -> Result<Vec<FunctionDeclaration>, LoweringError> {
            let mut functions = Vec::new();
            for root in parsed.roots() {
                let node = parsed.ast().items().get(*root).map_err(|_| LoweringError {
                    kind: LoweringErrorKind::MissingFact,
                    span: None,
                })?;
                let (item, span) = unwrap_modified(parsed, *root)?;
                let mut item_id = *root;
                while let Item::Modified { declaration, .. } = parsed
                    .ast()
                    .items()
                    .get(item_id)
                    .map_err(|_| error(LoweringErrorKind::MissingFact, span))?
                    .payload()
                {
                    item_id = *declaration;
                }
                let name = match &item {
                    Item::Function { name, .. } => name,
                    Item::Constant { .. } => continue,
                    Item::Classifier(declaration)
                        if matches!(
                            declaration.kind,
                            ClassifierKind::Object { .. }
                                | ClassifierKind::Class { .. }
                                | ClassifierKind::ValueClass { .. }
                                | ClassifierKind::Interface { .. }
                                | ClassifierKind::EnumClass { .. }
                        ) =>
                    {
                        continue;
                    }
                    _ => return Err(error(LoweringErrorKind::UnsupportedNode, node.span())),
                };
                let name_span = present_name(*name)
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                let symbol = names
                    .symbols()
                    .iter()
                    .find(|symbol| {
                        symbol.span() == name_span && symbol.kind() == SymbolKind::Function
                    })
                    .map(|symbol| symbol.id())
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, name_span))?;
                let callable = typed
                    .callables()
                    .iter()
                    .find(|callable| callable.symbol() == symbol && callable.owner().is_none())
                    .cloned()
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, name_span))?;
                functions.push(FunctionDeclaration {
                    item_id,
                    item,
                    symbol,
                    callable,
                    span,
                });
            }
            for callable in typed.callables() {
                if readonly_receiver::admitted(typed, callable).is_none() {
                    continue;
                }
                let symbol =
                    names
                        .symbols()
                        .get(callable.symbol().index())
                        .ok_or(LoweringError {
                            kind: LoweringErrorKind::MissingFact,
                            span: None,
                        })?;
                let (item_id, node) = parsed
                    .ast()
                    .items()
                    .iter()
                    .find(|(_, node)| {
                        matches!(node.payload(), Item::Function { name, .. }
                        if present_name(*name) == Some(symbol.span()))
                    })
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, symbol.span()))?;
                functions.push(FunctionDeclaration {
                    item_id,
                    item: node.payload().clone(),
                    symbol: callable.symbol(),
                    callable: callable.clone(),
                    span: node.span(),
                });
            }
            functions.sort_by_key(|declaration| declaration.span.start());
            Ok(functions)
        }
    }
}
