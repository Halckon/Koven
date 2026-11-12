//! SPEC-0255: 先在原 helper 路径锁定字节、拒绝与错误优先级。

use lang_frontend::{
    ast::ExpressionId,
    parser::{Expression, ParsedFile, StringPart},
    source::{SourceMap, Span},
};

use super::{
    LoweringError, LoweringErrorKind,
    lowering_support::{parsed_by_source_unit, string_literal::decode_plain},
    unit_lower_test_support::{analyze, parsed},
};
use lang_frontend::{name_resolution::SourceUnitInput, type_checking::standard_environments};

fn expression(text: &str) -> (SourceMap, ParsedFile, ExpressionId) {
    let mut sources = SourceMap::new();
    let (_, file) = parsed(
        &mut sources,
        "support/literal.ko",
        &format!("fun entry(): String = {text}"),
    );
    let id = file.ast().expressions().iter().last().unwrap().0;
    (sources, file, id)
}

fn source_text(sources: &SourceMap, parsed: &ParsedFile) -> String {
    sources.source_text(parsed.source_id()).unwrap().to_owned()
}

fn text_span(file: &ParsedFile) -> Span {
    file.ast()
        .expressions()
        .iter()
        .find_map(|(_, node)| match node.payload() {
            Expression::String { parts } => parts.iter().find_map(|part| match part {
                StringPart::Text(span) => Some(*span),
                _ => None,
            }),
            _ => None,
        })
        .unwrap()
}

fn normalized_error(
    sources: &SourceMap,
    result: Result<Option<Vec<u8>>, LoweringError>,
) -> (LoweringErrorKind, Option<(String, usize, usize)>) {
    let error = result.expect_err("expected exact helper error");
    (
        error.kind,
        error.span.map(|span| {
            (
                sources.source_name(span.source_id()).unwrap().to_owned(),
                span.start(),
                span.end(),
            )
        }),
    )
}

fn assert_span_error(
    sources: &SourceMap,
    result: Result<Option<Vec<u8>>, LoweringError>,
    kind: LoweringErrorKind,
    span: Span,
) {
    assert_eq!(
        normalized_error(sources, result),
        (
            kind,
            Some(("support/literal.ko".to_owned(), span.start(), span.end()))
        )
    );
}

#[test]
fn plain_literal_bytes_preserve_utf8_nul_all_escapes_and_nested_groups() {
    for (literal, expected) in [
        (r#""""#, b"".as_slice()),
        (r#""plain""#, b"plain".as_slice()),
        (r#""中文🙂""#, "中文🙂".as_bytes()),
        (r#""a\0b""#, b"a\0b".as_slice()),
        (r#""\\\'\"\n\r\t\0\$""#, b"\\'\"\n\r\t\0$".as_slice()),
        (r#"((("中文\0🙂")))"#, "中文\0🙂".as_bytes()),
    ] {
        let (sources, file, id) = expression(literal);
        assert_eq!(
            decode_plain(&file, &source_text(&sources, &file), id).unwrap(),
            Some(expected.to_vec()),
            "{literal}"
        );
    }
}

#[test]
fn nonliteral_and_interpolation_remain_absent_even_when_grouped() {
    for text in ["17", "value", "(value)", r#""${17}""#, r#"(("a${17}b"))"#] {
        let (sources, file, id) = expression(text);
        assert_eq!(
            decode_plain(&file, &source_text(&sources, &file), id),
            Ok(None)
        );
    }
}

#[test]
fn missing_ast_node_remains_missing_fact_without_span_before_text_access() {
    let (mut sources, file, id) = expression(r#"((("x")))"#);
    let (_, empty) = parsed(&mut sources, "support/empty.ko", "");
    assert!(id.index() >= empty.ast().expressions().len());
    assert_eq!(
        normalized_error(&sources, decode_plain(&empty, "", id)),
        (LoweringErrorKind::MissingFact, None)
    );
    assert!(decode_plain(&file, &source_text(&sources, &file), id).is_ok());
}

#[test]
fn invalid_text_range_and_utf8_boundary_keep_original_text_span() {
    let (sources, file, id) = expression(r#""x""#);
    let span = text_span(&file);
    assert_span_error(
        &sources,
        decode_plain(&file, "", id),
        LoweringErrorKind::MismatchedSource,
        span,
    );
    let mut text = " ".repeat(span.start() - 1);
    text.push('界');
    text.push_str("padding");
    assert_span_error(
        &sources,
        decode_plain(&file, &text, id),
        LoweringErrorKind::MismatchedSource,
        span,
    );
}

#[test]
fn invalid_escape_and_trailing_slash_keep_original_text_span() {
    let (sources, file, id) = expression(r#""\n""#);
    let span = text_span(&file);
    for replacement in ["\\q", "a\\"] {
        let mut text = source_text(&sources, &file);
        text.replace_range(span.start()..span.end(), replacement);
        assert_span_error(
            &sources,
            decode_plain(&file, &text, id),
            LoweringErrorKind::InvalidLiteral,
            span,
        );
    }
}

#[test]
fn text_before_interpolation_is_checked_but_interpolation_stops_later_text() {
    let (sources, file, id) = expression(r#""\n${17}""#);
    let span = text_span(&file);
    let mut text = source_text(&sources, &file);
    text.replace_range(span.start()..span.end(), "\\q");
    assert_span_error(
        &sources,
        decode_plain(&file, &text, id),
        LoweringErrorKind::InvalidLiteral,
        span,
    );
    assert_span_error(
        &sources,
        decode_plain(&file, "", id),
        LoweringErrorKind::MismatchedSource,
        span,
    );
    let (_, file, id) = expression(r#""${17}\n""#);
    assert_eq!(decode_plain(&file, "", id), Ok(None));
}

#[test]
fn canonical_lookup_preserves_borrowed_identity_first_match_and_missing_error() {
    let mut sources = SourceMap::default();
    let (a, first) = parsed(&mut sources, "a.ko", "fun a(): Unit {}");
    let (z, last) = parsed(&mut sources, "z.ko", "fun z(): Unit {}");
    let inputs = [
        SourceUnitInput::new("root", "z.ko", z, &last),
        SourceUnitInput::new("root", "a.ko", a, &first),
    ];
    let (names_env, types_env) = standard_environments();
    let (names, _, _) = analyze(&sources, &inputs, &names_env, &types_env);
    let result = parsed_by_source_unit(&inputs, &names).unwrap();
    assert!(std::ptr::eq(result[0], &first));
    assert!(std::ptr::eq(result[1], &last));
    let clone = lang_frontend::parser::parse_file(
        &sources,
        &lang_frontend::lexer::lex(&sources, a).unwrap(),
    )
    .unwrap();
    let duplicate = [
        inputs[1],
        SourceUnitInput::new("root", "a.ko", a, &clone),
        inputs[0],
    ];
    assert!(std::ptr::eq(
        parsed_by_source_unit(&duplicate, &names).unwrap()[0],
        &first
    ));
    let missing = parsed_by_source_unit(&inputs[..1], &names).unwrap_err();
    assert_eq!(missing.kind, LoweringErrorKind::MismatchedSource);
    assert_eq!(missing.span, None);
}

pub(in crate::ssa) mod numeric_iteration {
    //! Owned Copyable cursor state, independent of the shared container provider.
    use crate::ssa::lower_frontend::loop_control::*;
    use crate::ssa::lower_frontend::*;
    use crate::ssa::model::{BinaryOperator, ComparisonOperator, ScalarConstant};
    use lang_frontend::{
        ownership_checking::{IterationExitKind, IterationOwnershipPlan},
        type_checking::{BuiltinType, IterationProvider, SequentialIterationBinding},
    };

    pub(in crate::ssa) struct NumericLoopData {
        pub(in crate::ssa) plan: IterationOwnershipPlan,
        current: ValueId,
        last: ValueId,
        endpoint: SsaTypeId,
        exhaustion: BlockId,
        baseline: BTreeMap<SymbolId, LoweredValue>,
    }

    impl ExpressionLowerer<'_> {
        pub(in crate::ssa) fn lower_numeric_for(
            &mut self,
            statement: StatementId,
            source: ExpressionId,
            body: StatementId,
            span: Span,
            plan: IterationOwnershipPlan,
        ) -> Result<LoweredValue, LoweringError> {
            let inherited = self.qualify_numeric_while_entry(statement, span)?;
            let descriptor = plan.descriptor();
            if descriptor.statement() != statement {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            let endpoint = self.resolve_type(descriptor.element_type(), span)?;
            let endpoint = *self
                .type_ids
                .get(&endpoint)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let endpoint_builtin = match descriptor.provider() {
                IterationProvider::NumericRange(value @ (BuiltinType::Int | BuiltinType::Long)) => {
                    value
                }
                _ => return Err(error(LoweringErrorKind::MissingFact, span)),
            };
            if endpoint != self.ssa_builtin(endpoint_builtin, span)? {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            let range_type = self.resolve_type(descriptor.source_type(), span)?;
            let range_type = *self
                .type_ids
                .get(&range_type)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let boolean = self.ssa_builtin(BuiltinType::Boolean, span)?;
            let snapshot = match self.lower(source)? {
                LoweredValue::Value(value) => value,
                LoweredValue::Diverged => return Ok(LoweredValue::Diverged),
                LoweredValue::Unit => return Err(error(LoweringErrorKind::MissingFact, span)),
            };
            if self
                .function
                .entity(EntityId::Value(snapshot))
                .map(|entity| entity.ty)
                != Some(EntityType::Value(range_type))
            {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            self.validate_numeric_while_pair(inherited, span)?;
            let first = self.numeric_projection(snapshot, 0, endpoint, span)?;
            let last = self.numeric_projection(snapshot, 1, endpoint, span)?;
            let empty = self.numeric_projection(snapshot, 2, boolean, span)?;
            let baseline = self.bindings.clone();
            let mut carried = self.linear_binding_slots(&baseline, span)?;
            // All mutable Copyable bindings are loop-carried, just as for while/container loops.
            for (&symbol, &binding) in &baseline {
                if let LoweredValue::Value(value) = binding
                    && !carried.slots.iter().any(|slot| slot.symbol == Some(symbol))
                {
                    let source = EntityId::Value(value);
                    let ty = self
                        .function
                        .entity(source)
                        .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))?
                        .ty;
                    crate::ssa::carry::seed_slot(
                        &mut carried.slots,
                        LinearBindingSlot::new(Some(symbol), source, ty),
                    );
                }
            }
            let mut body_types = carried.slots.iter().map(|slot| slot.ty).collect::<Vec<_>>();
            body_types.push(EntityType::Value(endpoint));
            let header = self
                .function
                .add_block(body_types, Origin::Source(span))
                .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
            let exhaustion = self.add_linear_binding_block(&carried, span)?;
            let mut body_args = carried
                .slots
                .iter()
                .map(|slot| slot.source)
                .collect::<Vec<_>>();
            body_args.push(EntityId::Value(first));
            self.function
                .set_terminator(
                    self.block,
                    TerminatorKind::Conditional {
                        condition: empty,
                        when_true: loop_edge(exhaustion, &carried),
                        when_false: Edge {
                            target: header,
                            arguments: body_args,
                        },
                    },
                    Origin::Source(span),
                )
                .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
            self.block = header;
            self.bindings = self.rebind_linear_bindings(&baseline, header, &carried, span)?;
            self.validate_numeric_while_pair(inherited, span)?;
            let current = match self
                .function
                .block(header)
                .and_then(|block| block.parameters.last())
                .copied()
            {
                Some(EntityId::Value(value)) => value,
                _ => return Err(error(LoweringErrorKind::InvalidModel, span)),
            };
            match descriptor.binding() {
                SequentialIterationBinding::Name(symbol) => {
                    self.bindings.insert(*symbol, LoweredValue::Value(current));
                }
                SequentialIterationBinding::Discard => {}
                SequentialIterationBinding::Destructure(_) => {
                    return Err(error(LoweringErrorKind::MissingFact, span));
                }
            }
            self.loops.push(LoopContext {
                entry_views: self.non_null_bindings.clone(),
                outer_borrowed_while: None,
                map_loop: None,
                header,
                carried: carried.clone(),
                continues: Vec::new(),
                breaks: Vec::new(),
                for_loop: None,
                numeric_loop: Some(NumericLoopData {
                    plan: plan.clone(),
                    current,
                    last,
                    endpoint,
                    exhaustion,
                    baseline: baseline.clone(),
                }),
            });
            if self.lower_statement(body)? != LoweredValue::Diverged {
                self.validate_numeric_while_pair(inherited, span)?;
                let exit = plan
                    .exits()
                    .iter()
                    .find(|exit| matches!(exit.kind(), IterationExitKind::Fallthrough))
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                self.emit_iteration_exit_plan(exit, span)?;
                self.finish_numeric_iteration(span)?;
            }
            self.block = exhaustion;
            self.bindings = self.rebind_linear_bindings(&baseline, exhaustion, &carried, span)?;
            self.validate_numeric_while_pair(inherited, span)?;
            let exit = plan
                .exits()
                .iter()
                .find(|exit| matches!(exit.kind(), IterationExitKind::Exhaustion))
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            self.emit_iteration_exit_plan(exit, span)?;
            self.validate_numeric_while_pair(inherited, span)?;
            let context = self
                .loops
                .pop()
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let mut exits = context.breaks;
            exits.push(self.loop_exit());
            // Numeric snapshots are pure values; the normal merge still preserves outer loans/owners.
            let result = self.merge_exits(exits, &baseline, span)?;
            self.validate_numeric_while_pair(inherited, span)?;
            Ok(result)
        }

        fn numeric_projection(
            &mut self,
            aggregate: ValueId,
            field: usize,
            ty: SsaTypeId,
            span: Span,
        ) -> Result<ValueId, LoweringError> {
            self.numeric_value(Operation::AggregateProject { aggregate, field }, ty, span)
        }

        fn numeric_value(
            &mut self,
            operation: Operation,
            ty: SsaTypeId,
            span: Span,
        ) -> Result<ValueId, LoweringError> {
            let (_, results) = self.append(operation, vec![EntityType::Value(ty)], span)?;
            match results.as_slice() {
                [EntityId::Value(value)] => Ok(*value),
                _ => Err(error(LoweringErrorKind::InvalidModel, span)),
            }
        }

        pub(in crate::ssa) fn lower_numeric_break(
            &mut self,
            expression: ExpressionId,
            span: Span,
        ) -> Result<LoweredValue, LoweringError> {
            let plan = self
                .loops
                .last()
                .and_then(|context| context.numeric_loop.as_ref())
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?
                .plan
                .clone();
            let exit = plan
                .exits()
                .iter()
                .find(|exit| matches!(exit.kind(), IterationExitKind::Break(id) if id == expression))
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            self.emit_iteration_exit_plan(exit, span)?;
            self.lower_break(span)
        }

        pub(in crate::ssa) fn lower_numeric_continue(
            &mut self,
            expression: ExpressionId,
            span: Span,
        ) -> Result<LoweredValue, LoweringError> {
            let plan = self
                .loops
                .last()
                .and_then(|context| context.numeric_loop.as_ref())
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?
                .plan
                .clone();
            let inherited = self
                .loops
                .last()
                .and_then(|context| context.entry_views.first_key_value())
                .filter(|(_, proof)| proof.is_while_origin())
                .map(|(&root, &proof)| (root, proof));
            self.validate_numeric_while_pair(inherited, span)?;
            let exit = plan
                .exits()
                .iter()
                .find(|exit| matches!(exit.kind(), IterationExitKind::Continue(id) if id == expression))
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            self.emit_iteration_exit_plan(exit, span)?;
            self.finish_numeric_iteration(span)?;
            Ok(LoweredValue::Diverged)
        }

        fn finish_numeric_iteration(&mut self, span: Span) -> Result<(), LoweringError> {
            let context = self
                .loops
                .last()
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let inherited = context
                .entry_views
                .first_key_value()
                .filter(|(_, proof)| proof.is_while_origin())
                .map(|(&root, &proof)| (root, proof));
            let data = context
                .numeric_loop
                .as_ref()
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let (current, last, endpoint, exhaustion, baseline, header, carried) = (
                data.current,
                data.last,
                data.endpoint,
                data.exhaustion,
                data.baseline.clone(),
                context.header,
                context.carried.clone(),
            );
            self.validate_numeric_while_pair(inherited, span)?;
            let boolean = self.ssa_builtin(BuiltinType::Boolean, span)?;
            let terminal = self.numeric_value(
                Operation::Compare {
                    operator: ComparisonOperator::Equal,
                    left: current,
                    right: last,
                },
                boolean,
                span,
            )?;
            let increment = self.add_linear_binding_block(&carried, span)?;
            let state = self.loop_exit();
            let args = carried
                .slots
                .iter()
                .map(|slot| loop_slot_entity(self, slot, &state, span))
                .collect::<Result<Vec<_>, _>>()?;
            self.function
                .set_terminator(
                    self.block,
                    TerminatorKind::Conditional {
                        condition: terminal,
                        when_true: Edge {
                            target: exhaustion,
                            arguments: args.clone(),
                        },
                        when_false: Edge {
                            target: increment,
                            arguments: args,
                        },
                    },
                    Origin::Source(span),
                )
                .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
            self.block = increment;
            self.bindings = self.rebind_linear_bindings(&baseline, increment, &carried, span)?;
            self.validate_numeric_while_pair(inherited, span)?;
            // This block is reachable only when current != last; ascending current cannot overflow.
            let one = self.numeric_value(
                Operation::Constant(ScalarConstant::Integer(1)),
                endpoint,
                span,
            )?;
            let next = self.numeric_value(
                Operation::Binary {
                    operator: BinaryOperator::Add,
                    left: current,
                    right: one,
                },
                endpoint,
                span,
            )?;
            let state = self.loop_exit();
            let mut args = carried
                .slots
                .iter()
                .map(|slot| loop_slot_entity(self, slot, &state, span))
                .collect::<Result<Vec<_>, _>>()?;
            args.push(EntityId::Value(next));
            self.function
                .set_terminator(
                    self.block,
                    TerminatorKind::Branch(Edge {
                        target: header,
                        arguments: args,
                    }),
                    Origin::Source(span),
                )
                .map_err(|_| error(LoweringErrorKind::InvalidModel, span))
        }
    }

    pub(in crate::ssa) mod provider {
        //! Exact source/element births and the complete single-provider edge schedule.
        use crate::ssa::{
            lower_frontend::{
                nullable_borrowed_cleanup::{
                    Plan,
                    carrier::{
                        Birth, Columns, forward, incoming, instructions, parameter, result, transport,
                    },
                    unique,
                },
                *,
            },
            model::{BinaryOperator, Instruction, SsaTypeKind},
        };
        use lang_frontend::ownership_checking::IterationExitKind;

        pub(in crate::ssa) struct Provider {
            pub(in crate::ssa) header: Columns,
            pub(in crate::ssa) body: Columns,
            pub(in crate::ssa) exhausted: Columns,
            pub(in crate::ssa) source: LoanId,
            pub(in crate::ssa) length: ValueId,
            pub(in crate::ssa) cursor: ValueId,
            pub(in crate::ssa) body_cursor: ValueId,
            pub(in crate::ssa) step: ValueId,
        }

        pub(in crate::ssa) fn value(entity: EntityId) -> Option<ValueId> {
            if let EntityId::Value(value) = entity {
                Some(value)
            } else {
                None
            }
        }
        pub(in crate::ssa) fn loan(entity: EntityId) -> Option<LoanId> {
            if let EntityId::Loan(loan) = entity {
                Some(loan)
            } else {
                None
            }
        }
        fn only_result(instruction: &Instruction) -> Option<EntityId> {
            let [entity] = instruction.results.as_slice() else {
                return None;
            };
            Some(*entity)
        }

        pub(in crate::ssa) fn shape(
            lowerer: &ExpressionLowerer<'_>,
            plan: &Plan,
            birth: &Birth,
        ) -> Option<Provider> {
            let function = &*lowerer.function;
            let span = lowerer
                .parsed
                .ast()
                .statements()
                .get(plan.statement)
                .ok()?
                .span();
            let descriptor = lowerer.owned.iteration(plan.statement)?.descriptor();
            let source_type = *lowerer.type_ids.get(&descriptor.source_type())?;
            let element_type = *lowerer.type_ids.get(&descriptor.element_type())?;
            if lowerer.ssa_types.get(element_type.index())
                != Some(&SsaTypeKind::Integer {
                    bits: 32,
                    signed: true,
                })
                || !matches!(lowerer.ssa_types.get(source_type.index()), Some(SsaTypeKind::SequentialContainer {
                    kind: crate::ssa::model::SequentialContainerKind::List, element }) if *element == element_type)
            {
                return None;
            }
            let ops = instructions(function, birth.pair.block)?;
            let root = unique(ops.iter().copied().filter(|instruction| {
                matches!(instruction.operation,
                Operation::RootPlace { owner } if owner == birth.source_owner)
            }))?;
            let EntityId::Place(place) = only_result(root)? else {
                return None;
            };
            let begin = unique(
                ops.iter()
                    .copied()
                    .filter(|instruction| matches!(instruction.operation, Operation::BorrowBegin { .. })),
            )?;
            if begin.operation
                != (Operation::BorrowBegin {
                    place,
                    kind: LoanKind::Shared,
                })
                || root.origin != Origin::Source(span)
                || begin.origin != Origin::Source(span)
                || function.entity(EntityId::Place(place))?.ty != EntityType::Place(source_type)
                || ops.iter().position(|op| op.id == root.id)?
                    >= ops.iter().position(|op| op.id == begin.id)?
            {
                return None;
            }
            let source = loan(only_result(begin)?)?;
            if function.entity(EntityId::Loan(source))?.ty
                != (EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: source_type,
                })
            {
                return None;
            }
            let length = unique(ops.iter().copied().filter(|instruction| {
                instruction.operation
                    == (Operation::ContainerLength {
                        owner: EntityId::Loan(source),
                    })
            }))?;
            let zero = unique(ops.iter().copied().filter(|instruction| {
                instruction.operation == Operation::Constant(ScalarConstant::Integer(0))
            }))?;
            let one = unique(ops.iter().copied().filter(|instruction| {
                instruction.operation == Operation::Constant(ScalarConstant::Integer(1))
            }))?;
            if [length, zero, one]
                .iter()
                .any(|op| op.origin != Origin::Source(span))
            {
                return None;
            }
            let (length_value, zero_value, step) = (
                value(only_result(length)?)?,
                value(only_result(zero)?)?,
                value(only_result(one)?)?,
            );
            let term = function.block(birth.pair.block)?.terminator.as_ref()?;
            let TerminatorKind::Branch(entry) = &term.kind else {
                return None;
            };
            if term.origin != Origin::Source(span) || entry.target == birth.pair.block {
                return None;
            }
            let mut entry_entities = birth.pair.entities.clone();
            entry_entities.extend([EntityId::Value(birth.source_owner), EntityId::Loan(source)]);
            let header = forward(function, entry, &entry_entities)?;
            let numbers = forward(
                function,
                entry,
                &[EntityId::Value(length_value), EntityId::Value(zero_value)],
            )?;
            let (length, cursor) = (value(numbers.entities[0])?, value(numbers.entities[1])?);
            let term = function.block(header.block)?.terminator.as_ref()?;
            let TerminatorKind::Conditional {
                condition,
                when_true,
                when_false,
            } = &term.kind
            else {
                return None;
            };
            let compare = result(function, EntityId::Value(*condition))?;
            if term.origin != Origin::Source(span)
                || compare.block != header.block
                || compare.operation
                    != (Operation::Compare {
                        operator: ComparisonOperator::LessThan,
                        left: cursor,
                        right: length,
                    })
                || when_true.target == when_false.target
                || when_true.target == header.block
                || when_false.target == header.block
            {
                return None;
            }
            let mut body = forward(function, when_true, &header.entities)?;
            let exhausted = forward(function, when_false, &header.entities)?;
            if incoming(function, body.block).as_slice() != [(header.block, when_true)]
                || incoming(function, exhausted.block).as_slice() != [(header.block, when_false)]
            {
                return None;
            }
            let body_cursor =
                value(forward(function, when_true, &[EntityId::Value(cursor)])?.entities[0])?;
            let body_source = loan(body.entities[3])?;
            let ops = instructions(function, body.block)?;
            let element_place = unique(ops.iter().copied().filter(|instruction| {
                matches!(
                    instruction.operation,
                    Operation::ContainerElementPlace { .. }
                )
            }))?;
            let EntityId::Place(place) = only_result(element_place)? else {
                return None;
            };
            let begin = unique(
                ops.iter()
                    .copied()
                    .filter(|instruction| matches!(instruction.operation, Operation::BorrowBegin { .. })),
            )?;
            if element_place.operation
                != (Operation::ContainerElementPlace {
                    owner: EntityId::Loan(body_source),
                    index: body_cursor,
                })
                || begin.operation
                    != (Operation::BorrowBegin {
                        place,
                        kind: LoanKind::Shared,
                    })
                || element_place.origin != Origin::Source(span)
                || begin.origin != Origin::Source(span)
                || function.entity(EntityId::Place(place))?.ty != EntityType::Place(element_type)
                || ops.iter().position(|op| op.id == element_place.id)?
                    >= ops.iter().position(|op| op.id == begin.id)?
            {
                return None;
            }
            let element = loan(only_result(begin)?)?;
            if function.entity(EntityId::Loan(element))?.ty
                != (EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: element_type,
                })
            {
                return None;
            }
            body.entities.push(EntityId::Loan(element));
            for state in [&header, &body, &exhausted] {
                if function.block(state.block)?.origin != Origin::Source(span)
                    || function
                        .block(state.block)?
                        .parameters
                        .iter()
                        .any(|entity| {
                            matches!(entity, EntityId::Loan(_)) && !state.entities.contains(entity)
                        })
                {
                    return None;
                }
            }
            Some(Provider {
                header,
                body,
                exhausted,
                source,
                length,
                cursor,
                body_cursor,
                step,
            })
        }

        pub(in crate::ssa) fn backedges(
            lowerer: &ExpressionLowerer<'_>,
            plan: &Plan,
            birth: &Birth,
            provider: &Provider,
        ) -> Option<Vec<(LoanId, LoanId)>> {
            let function = &*lowerer.function;
            let span = lowerer
                .parsed
                .ast()
                .statements()
                .get(plan.statement)
                .ok()?
                .span();
            let iteration = lowerer.owned.iteration(plan.statement)?;
            let mut expected = Vec::new();
            for exit in iteration.exits() {
                match exit.kind() {
                    IterationExitKind::Continue(expression) => expected.push(
                        lowerer
                            .parsed
                            .ast()
                            .expressions()
                            .get(expression)
                            .ok()?
                            .span(),
                    ),
                    IterationExitKind::Fallthrough => expected.push(span),
                    IterationExitKind::Exhaustion => {}
                    _ => return None,
                }
            }
            let edges = incoming(function, provider.header.block);
            if edges.len() != expected.len() + 1 {
                return None;
            }
            let columns = provider
                .header
                .entities
                .iter()
                .map(|entity| parameter(function, *entity).map(|(_, index)| index))
                .collect::<Option<Vec<_>>>()?;
            let length_column = parameter(function, EntityId::Value(provider.length))?.1;
            let cursor_column = parameter(function, EntityId::Value(provider.cursor))?.1;
            let mut entry_count = 0;
            let mut ended = Vec::new();
            for (block, edge) in edges {
                if block == birth.pair.block {
                    entry_count += 1;
                    continue;
                }
                let term = function.block(block)?.terminator.as_ref()?;
                let TerminatorKind::Branch(actual_edge) = &term.kind else {
                    return None;
                };
                if actual_edge != edge {
                    return None;
                }
                let Origin::Source(origin) = &term.origin else {
                    return None;
                };
                let origin = *origin;
                let index = expected.iter().position(|span| *span == origin)?;
                expected.remove(index);
                let ops = instructions(function, block)?;
                let end =
                    unique(ops.iter().copied().filter(|instruction| {
                        matches!(instruction.operation, Operation::BorrowEnd { .. })
                    }))?;
                let Operation::BorrowEnd { loan: element } = end.operation else {
                    return None;
                };
                if end.origin != Origin::Source(origin) {
                    return None;
                }
                let mut entities = columns
                    .iter()
                    .map(|column| edge.arguments.get(*column).copied())
                    .collect::<Option<Vec<_>>>()?;
                if edge.arguments.contains(&EntityId::Loan(element)) {
                    return None;
                }
                ended.push((loan(entities[3])?, element));
                entities.push(EntityId::Loan(element));
                if !transport(
                    function,
                    Columns { block, entities },
                    &provider.body,
                    Some((block, element)),
                ) {
                    return None;
                }
                if edge.arguments.get(length_column) != Some(&EntityId::Value(provider.length)) {
                    return None;
                }
                let next = value(*edge.arguments.get(cursor_column)?)?;
                let next = result(function, EntityId::Value(next))?;
                if next.block != block
                    || next.origin != Origin::Source(origin)
                    || next.operation
                        != (Operation::Binary {
                            operator: BinaryOperator::Add,
                            left: provider.body_cursor,
                            right: provider.step,
                        })
                    || ops.iter().position(|op| op.id == end.id)?
                        >= ops.iter().position(|op| op.id == next.id)?
                {
                    return None;
                }
            }
            (entry_count == 1 && expected.is_empty()).then_some(ended)
        }

        pub(in crate::ssa) fn complete(
            lowerer: &ExpressionLowerer<'_>,
            plan: &Plan,
            birth: &Birth,
            provider: &Provider,
        ) -> Option<()> {
            backedges(lowerer, plan, birth, provider)?;
            let function = &*lowerer.function;
            let span = lowerer
                .parsed
                .ast()
                .statements()
                .get(plan.statement)
                .ok()?
                .span();
            if lowerer.block != provider.exhausted.block
                || lowerer.bindings.contains_key(&plan.source_root)
                || lowerer.borrow_bindings.contains_key(&plan.element)
            {
                return None;
            }
            let source = loan(provider.exhausted.entities[3])?;
            let owner = value(provider.exhausted.entities[2])?;
            let ops = instructions(function, provider.exhausted.block)?;
            let end =
                unique(ops.iter().enumerate().filter(|(_, instruction)| {
                    matches!(instruction.operation, Operation::BorrowEnd { .. })
                }))?;
            let drop = unique(
                ops.iter()
                    .enumerate()
                    .filter(|(_, instruction)| matches!(instruction.operation, Operation::Drop { .. })),
            )?;
            if end.0 >= drop.0
                || end.1.operation != (Operation::BorrowEnd { loan: source })
                || end.1.origin != Origin::Source(span)
                || drop.1.operation != (Operation::Drop { owner })
                || drop.1.origin != Origin::Source(plan.source_drop.value_origin())
                || ops.iter().any(|instruction| {
                    matches!(
                        instruction.operation,
                        Operation::BorrowBegin { .. } | Operation::ContainerElementPlace { .. }
                    )
                })
                || ops[end.0 + 1..].iter().any(|instruction| {
                    instruction
                        .operation
                        .entities()
                        .contains(&EntityId::Loan(source))
                })
                || ops[drop.0 + 1..].iter().any(|instruction| {
                    instruction
                        .operation
                        .entities()
                        .contains(&EntityId::Value(owner))
                })
            {
                return None;
            }
            Some(())
        }
    }
}
