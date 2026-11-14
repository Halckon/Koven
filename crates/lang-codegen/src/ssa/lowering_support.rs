//! 单文件与 compilation-unit adapter 共用的中立 lowering 支撑。

pub(in crate::ssa) mod callable_instances {
    //! Plan-local source identities refer to earlier records rather than recursively enclosing keys.

    use std::collections::BTreeMap;

    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
    pub(in crate::ssa) struct SourceToken(usize);

    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
    pub(in crate::ssa) struct CallableToken(usize);

    /// The existing source key keeps its own target/types/receiver plus ordered callback slots.
    pub(in crate::ssa) trait SourceIdentity: Clone + Ord {
        fn callable_arguments(&self) -> &[(usize, CallableToken)];
    }

    #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
    pub(in crate::ssa) enum CallableKey<E> {
        Lambda { owner: SourceToken, expression: E },
        KnownFunction { function: SourceToken },
    }

    #[derive(Debug, PartialEq, Eq)]
    enum Record<K, E> {
        Source(K),
        Callable(CallableKey<E>),
    }

    /// One append-only record order supplies creation ordinals to both kinds of token.
    #[derive(Debug, PartialEq, Eq)]
    pub(in crate::ssa) struct CallableArena<K, E> {
        sources: BTreeMap<K, SourceToken>,
        callables: BTreeMap<CallableKey<E>, CallableToken>,
        records: Vec<Record<K, E>>,
    }

    impl<K: SourceIdentity, E: Clone + Ord> Default for CallableArena<K, E> {
        fn default() -> Self {
            Self {
                sources: BTreeMap::new(),
                callables: BTreeMap::new(),
                records: Vec::new(),
            }
        }
    }

    impl<K: SourceIdentity, E: Clone + Ord> CallableArena<K, E> {
        pub(in crate::ssa) fn intern_source(&mut self, key: K) -> Option<SourceToken> {
            if let Some(token) = self.sources.get(&key) {
                return Some(*token);
            }
            let arguments = key.callable_arguments();
            if arguments.windows(2).any(|pair| pair[0].0 >= pair[1].0) {
                return None;
            }
            for &(_, token) in arguments {
                self.callable(token)?;
            }
            let token = SourceToken(self.records.len());
            self.records.push(Record::Source(key.clone()));
            self.sources.insert(key, token);
            Some(token)
        }

        pub(in crate::ssa) fn intern_callable(
            &mut self,
            key: CallableKey<E>,
        ) -> Option<CallableToken> {
            if let Some(token) = self.callables.get(&key) {
                return Some(*token);
            }
            let owner = match &key {
                CallableKey::Lambda { owner, .. } => *owner,
                CallableKey::KnownFunction { function } => *function,
            };
            self.source(owner)?;
            let token = CallableToken(self.records.len());
            self.records.push(Record::Callable(key.clone()));
            self.callables.insert(key, token);
            Some(token)
        }

        pub(in crate::ssa) fn source(&self, token: SourceToken) -> Option<&K> {
            match self.records.get(token.0)? {
                Record::Source(key) => Some(key),
                Record::Callable(_) => None,
            }
        }

        pub(in crate::ssa) fn callable(&self, token: CallableToken) -> Option<&CallableKey<E>> {
            match self.records.get(token.0)? {
                Record::Callable(key) => Some(key),
                Record::Source(_) => None,
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
        struct Key {
            function: usize,
            arguments: Vec<(usize, CallableToken)>,
        }
        impl SourceIdentity for Key {
            fn callable_arguments(&self) -> &[(usize, CallableToken)] {
                &self.arguments
            }
        }

        #[test]
        fn callable_arena_parameter_forwarding_reuses_identity_without_enclosing_keys() {
            let mut arena = CallableArena::default();
            let root = arena
                .intern_source(Key {
                    function: 0,
                    arguments: vec![],
                })
                .unwrap();
            let callback = arena
                .intern_callable(CallableKey::Lambda {
                    owner: root,
                    expression: 7,
                })
                .unwrap();
            let helper_key = Key {
                function: 1,
                arguments: vec![(3, callback)],
            };
            let helper = arena.intern_source(helper_key.clone()).unwrap();
            assert!(root.0 < callback.0 && callback.0 < helper.0);
            for _ in 0..4 {
                assert_eq!(arena.intern_source(helper_key.clone()), Some(helper));
                assert_eq!(
                    arena.intern_callable(CallableKey::Lambda {
                        owner: root,
                        expression: 7
                    }),
                    Some(callback)
                );
            }
            assert_eq!(arena.records.len(), 3);
            assert_eq!(arena.source(helper), Some(&helper_key));
        }

        #[test]
        fn callable_arena_new_owner_keeps_same_expression_distinct_with_flat_references() {
            let mut arena = CallableArena::default();
            let mut owner = arena
                .intern_source(Key {
                    function: 0,
                    arguments: vec![],
                })
                .unwrap();
            let mut callbacks = Vec::new();
            for _ in 0..4 {
                let callback = arena
                    .intern_callable(CallableKey::Lambda {
                        owner,
                        expression: 7,
                    })
                    .unwrap();
                let next = arena
                    .intern_source(Key {
                        function: 1,
                        arguments: vec![(0, callback)],
                    })
                    .unwrap();
                assert!(owner.0 < callback.0 && callback.0 < next.0);
                assert!(!callbacks.contains(&callback));
                assert_eq!(arena.source(next).unwrap().arguments, [(0, callback)]);
                callbacks.push(callback);
                owner = next;
            }
            assert_eq!(arena.records.len(), 9);
        }

        #[test]
        fn callable_arena_named_body_and_lambda_do_not_merge_equal_pointer_shapes() {
            let mut arena = CallableArena::default();
            let owner = arena
                .intern_source(Key {
                    function: 0,
                    arguments: vec![],
                })
                .unwrap();
            let named_key = CallableKey::KnownFunction { function: owner };
            let named = arena.intern_callable(named_key.clone()).unwrap();
            let lambda = arena
                .intern_callable(CallableKey::Lambda {
                    owner,
                    expression: 7,
                })
                .unwrap();
            assert_ne!(named, lambda);
            assert_eq!(arena.intern_callable(named_key.clone()), Some(named));
            assert_eq!(arena.callable(named), Some(&named_key));
            assert_eq!(arena.records.len(), 3);
        }
    }
}

use lang_frontend::{
    ast::ExpressionId,
    name_resolution::{SourceUnitInput, ValidatedCompilationUnitNames},
    parser::{Expression, IntegerLiteralKind, LiteralKind, NameMarker, ParsedFile},
    source::Span,
    type_checking::{CompilationUnitTypes, UnitCallReceiverOrigin, UnitExpressionId},
};

pub(in crate::ssa) mod string_literal {
    //! 编译器封闭 effect 使用的静态 String literal 解码。

    use lang_frontend::{
        ast::ExpressionId,
        parser::{Expression, ParsedFile, StringPart},
        source::Span,
    };

    use super::{LoweringError, LoweringErrorKind, error};

    pub(in crate::ssa) fn decode_plain(
        parsed: &ParsedFile,
        source_text: &str,
        expression: ExpressionId,
    ) -> Result<Option<Vec<u8>>, LoweringError> {
        let node = parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|_| LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?;
        match node.payload() {
            Expression::Group { expression } => decode_plain(parsed, source_text, *expression),
            Expression::String { parts } => {
                let mut bytes = Vec::new();
                for part in parts {
                    let StringPart::Text(span) = part else {
                        return Ok(None);
                    };
                    decode_text(source_text, *span, &mut bytes)?;
                }
                Ok(Some(bytes))
            }
            _ => Ok(None),
        }
    }

    fn decode_text(
        source_text: &str,
        span: Span,
        output: &mut Vec<u8>,
    ) -> Result<(), LoweringError> {
        let text = source_text
            .get(span.start()..span.end())
            .ok_or_else(|| error(LoweringErrorKind::MismatchedSource, span))?;
        let mut chars = text.chars();
        while let Some(character) = chars.next() {
            if character != '\\' {
                let mut encoded = [0_u8; 4];
                output.extend_from_slice(character.encode_utf8(&mut encoded).as_bytes());
                continue;
            }
            let escaped = chars
                .next()
                .ok_or_else(|| error(LoweringErrorKind::InvalidLiteral, span))?;
            output.push(match escaped {
                '\\' => b'\\',
                '\'' => b'\'',
                '"' => b'"',
                'n' => b'\n',
                'r' => b'\r',
                't' => b'\t',
                '0' => b'\0',
                '$' => b'$',
                _ => return Err(error(LoweringErrorKind::InvalidLiteral, span)),
            });
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LoweringErrorKind {
    MismatchedSource,
    MismatchedAnalysis,
    FrontendDiagnostics,
    BlockingDeferred,
    InstanceLimitExceeded,
    UnsupportedNode,
    UnsupportedCompanionCall,
    MissingFact,
    InvalidLiteral,
    InvalidModel,
    InvalidSsa,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LoweringError {
    pub(crate) kind: LoweringErrorKind,
    pub(crate) span: Option<Span>,
}

pub(in crate::ssa) const fn error(kind: LoweringErrorKind, span: Span) -> LoweringError {
    LoweringError {
        kind,
        span: Some(span),
    }
}

pub(in crate::ssa) const fn present_name(marker: NameMarker) -> Option<Span> {
    match marker {
        NameMarker::Present(span) => Some(span),
        NameMarker::Missing(_) | NameMarker::Error(_) => None,
    }
}

pub(in crate::ssa) const fn span_key(span: Span) -> (usize, usize) {
    (span.start(), span.end())
}

pub(in crate::ssa) fn parse_integer_literal_text(
    text: &str,
    kind: IntegerLiteralKind,
    span: Span,
) -> Result<i128, LoweringError> {
    lang_frontend::type_checking::integer_literal_magnitude(text, kind)
        .and_then(|value| i128::try_from(value).ok())
        .ok_or_else(|| error(LoweringErrorKind::InvalidLiteral, span))
}

pub(in crate::ssa) fn is_null_literal(parsed: &ParsedFile, mut expression: ExpressionId) -> bool {
    while let Ok(node) = parsed.ast().expressions().get(expression) {
        match node.payload() {
            Expression::Literal(LiteralKind::Null) => return true,
            Expression::Group { expression: inner } => expression = *inner,
            _ => return false,
        }
    }
    false
}

pub(in crate::ssa) fn parsed_by_source_unit<'a>(
    inputs: &'a [SourceUnitInput<'a>],
    names: &ValidatedCompilationUnitNames,
) -> Result<Vec<&'a ParsedFile>, LoweringError> {
    names
        .names()
        .index()
        .source_units()
        .iter()
        .map(|source_unit| {
            inputs
                .iter()
                .copied()
                .find(|input| input.source_id() == source_unit.source_id())
                .map(SourceUnitInput::parsed)
                .ok_or(LoweringError {
                    kind: LoweringErrorKind::MismatchedSource,
                    span: None,
                })
        })
        .collect()
}

/// Return the typed receiver operand only when it cannot reach receiver delivery.
pub(in crate::ssa) fn non_completing_call_receiver(
    typed: &CompilationUnitTypes,
    call: UnitExpressionId,
) -> Option<UnitExpressionId> {
    let UnitCallReceiverOrigin::Expression(receiver) = typed.call(call)?.receiver()?.origin()
    else {
        return None;
    };
    (typed.expression_falls_through(receiver) == Some(false)).then_some(receiver)
}

#[cfg(test)]
pub(in crate::ssa) mod frontend_range {
    //! Same N1a inline descriptor and source continuation as compilation-unit lowering.
    use crate::ssa::lower_frontend::*;
    use crate::ssa::model::SsaTypeKind;
    use lang_frontend::{
        ownership_checking::BorrowBindingStorage,
        type_checking::{BorrowReturnOrigin, CallableResultSource},
    };

    /// A frontend-authorized extension is a static range function whose first ABI slot is this.
    /// It is deliberately separate from nominal instance dispatch and ordinary borrow returns.
    pub(in crate::ssa) fn receiver_parameter(
        module: &mut crate::ssa::model::Module,
        names: &NameResolution,
        typed: &TypedFile,
        callable: &lang_frontend::type_checking::CallableDescriptor,
        substitutions: &BTreeMap<SymbolId, TypeId>,
        mapper: &mut crate::ssa::lower_frontend::nominal::NominalTypeMapper,
    ) -> Result<Option<(SymbolId, EntityType)>, LoweringError> {
        let Some(binding) = callable.range_extension() else {
            return Ok(None);
        };
        let span = binding.receiver_span();
        let symbol = callable
            .extension_receiver_symbol()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let concrete =
            instances::resolve_concrete_type(typed, binding.receiver_type(), substitutions, span)?;
        let target = mapper.intern(module, names, typed, concrete, span)?;
        Ok(Some((
            symbol,
            EntityType::Loan {
                kind: LoanKind::Shared,
                target,
            },
        )))
    }

    pub(in crate::ssa) fn carrier_source_index(
        origin: BorrowReturnOrigin,
        extension: bool,
        span: Span,
    ) -> Result<usize, LoweringError> {
        match origin {
            BorrowReturnOrigin::Parameter(index) => Ok(index),
            BorrowReturnOrigin::Receiver if extension => Ok(0),
            _ => Err(error(LoweringErrorKind::UnsupportedNode, span)),
        }
    }

    impl ExpressionLowerer<'_> {
        pub(in crate::ssa) fn lower_short_range(
            &mut self,
            expression: ExpressionId,
            ty: SsaTypeId,
            span: Span,
        ) -> Result<LoanId, LoweringError> {
            let fact = self
                .owned
                .borrow_results()
                .range_uses()
                .iter()
                .find(|fact| fact.expression() == expression)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let source = fact.source_loan();
            if self
                .owned
                .loans()
                .iter()
                .find(|loan| loan.call() == source.call() && loan.argument() == source.argument())
                .is_none_or(|loan| {
                    loan.target() != fact.origin()
                        || loan.kind() != lang_frontend::ownership_checking::LoanKind::Shared
                })
            {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            let (view, protecting, created) = self.lower_range_call(expression, span)?;
            if created.len() > 1 {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
            self.temporaries.insert(expression.index(), view);
            self.pending_call_loans
                .insert((expression.index(), expression.index()), Some(protecting));
            if let Some(loan) = created.first() {
                self.pending_call_loans.insert(
                    (source.call().index(), source.argument().index()),
                    Some(*loan),
                );
            }
            let (_, places) = self.append(
                Operation::RootPlace { owner: view },
                vec![EntityType::Place(ty)],
                span,
            )?;
            let EntityId::Place(place) = places[0] else {
                return Err(error(LoweringErrorKind::InvalidModel, span));
            };
            let (_, loans) = self.append(
                Operation::BorrowBegin {
                    place,
                    kind: LoanKind::Shared,
                },
                vec![EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: ty,
                }],
                span,
            )?;
            let EntityId::Loan(loan) = loans[0] else {
                return Err(error(LoweringErrorKind::InvalidModel, span));
            };
            Ok(loan)
        }
        pub(in crate::ssa) fn finish_short_range(
            &mut self,
            expression: ExpressionId,
            span: Span,
        ) -> Result<(), LoweringError> {
            let source = self
                .owned
                .borrow_results()
                .range_uses()
                .iter()
                .find(|fact| fact.expression() == expression)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?
                .source_loan();
            let view = self
                .temporaries
                .remove(&expression.index())
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let loan = self
                .pending_call_loans
                .remove(&(expression.index(), expression.index()))
                .flatten()
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            self.append(Operation::RangeEnd { view, source: loan }, vec![], span)?;
            if let Some(Some(loan)) = self
                .pending_call_loans
                .remove(&(source.call().index(), source.argument().index()))
            {
                self.append(Operation::BorrowEnd { loan }, vec![], span)?;
            }
            Ok(())
        }
        pub(in crate::ssa) fn finish_short_call_ranges(
            &mut self,
            call: ExpressionId,
            span: Span,
        ) -> Result<(), LoweringError> {
            let results = self
                .owned
                .borrow_results()
                .range_uses()
                .iter()
                .filter(|fact| {
                    fact.site() == lang_frontend::ownership_checking::RangeUseSite::Call(call)
                })
                .map(|fact| fact.expression())
                .collect::<Vec<_>>();
            for result in results.into_iter().rev() {
                self.finish_short_range(result, span)?;
            }
            Ok(())
        }
        pub(in crate::ssa) fn range_iteration_loan(
            &mut self,
            source: ExpressionId,
            ty: SsaTypeId,
            span: Span,
        ) -> Result<LoanId, LoweringError> {
            let node = self
                .parsed
                .ast()
                .expressions()
                .get(source)
                .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
            if self.owned.borrow_results().range_uses().iter().any(|fact| {
                fact.expression() == source
                    && fact.site() == lang_frontend::ownership_checking::RangeUseSite::Iteration
            }) {
                return self.lower_short_range(source, ty, span);
            }
            if !matches!(node.payload(), Expression::Name) {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
            let symbol = self
                .references
                .get(&span_key(node.span()))
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            if let Some(LoweredValue::Value(owner)) = self.bindings.get(symbol).copied() {
                let (_, results) = self.append(
                    Operation::RootPlace { owner },
                    vec![EntityType::Place(ty)],
                    span,
                )?;
                let EntityId::Place(place) = results[0] else {
                    return Err(error(LoweringErrorKind::InvalidModel, span));
                };
                let (_, results) = self.append(
                    Operation::BorrowBegin {
                        place,
                        kind: LoanKind::Shared,
                    },
                    vec![EntityType::Loan {
                        kind: LoanKind::Shared,
                        target: ty,
                    }],
                    span,
                )?;
                let EntityId::Loan(loan) = results[0] else {
                    return Err(error(LoweringErrorKind::InvalidModel, span));
                };
                Ok(loan)
            } else {
                self.borrow_bindings
                    .get(symbol)
                    .copied()
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))
            }
        }
        pub(in crate::ssa) fn lower_range_metadata_alias(
            &mut self,
            initializer: ExpressionId,
            parent: Option<SymbolId>,
            span: Span,
        ) -> Result<Option<LoanId>, LoweringError> {
            let node = self
                .parsed
                .ast()
                .expressions()
                .get(initializer)
                .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
            if !matches!(node.payload(), Expression::Name) {
                return Ok(None);
            }
            let symbol = self
                .references
                .get(&span_key(node.span()))
                .copied()
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let Some(LoweredValue::Value(view)) = self.bindings.get(&symbol).copied() else {
                return Ok(None);
            };
            if parent != Some(symbol) {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            let target = self.expression_ssa_type(initializer, span)?;
            if !matches!(
                self.ssa_types.get(target.index()),
                Some(SsaTypeKind::RangeView { .. })
            ) {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            let (_, results) = self.append(
                Operation::RootPlace { owner: view },
                vec![EntityType::Place(target)],
                span,
            )?;
            let place = place(results[0]);
            let (_, results) = self.append(
                Operation::BorrowBegin {
                    place,
                    kind: LoanKind::Shared,
                },
                vec![EntityType::Loan {
                    kind: LoanKind::Shared,
                    target,
                }],
                span,
            )?;
            let [EntityId::Loan(loan)] = results.as_slice() else {
                return Err(error(LoweringErrorKind::InvalidModel, span));
            };
            let loan = *loan;
            Ok(Some(loan))
        }

        pub(in crate::ssa) fn lower_range_binding(
            &mut self,
            symbol: SymbolId,
            initializer: ExpressionId,
            span: Span,
        ) -> Result<LoweredValue, LoweringError> {
            let fact = self
                .owned
                .borrow_results()
                .bindings()
                .iter()
                .find(|f| f.binding() == symbol && f.initializer() == initializer)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let source = fact
                .source_loan()
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            if fact.storage() != BorrowBindingStorage::NewRangeDescriptor
                || self.owned.loan_begin(source.argument()).is_none_or(|loan| {
                    loan.call() != source.call() || loan.target() != fact.origin()
                })
            {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            let (view, loan, created) = self.lower_range_call(initializer, span)?;
            self.bindings.insert(symbol, LoweredValue::Value(view));
            self.borrow_bindings.insert(symbol, loan);
            self.result_source_loans.insert(symbol, created);
            Ok(LoweredValue::Unit)
        }

        fn lower_range_call(
            &mut self,
            expression: ExpressionId,
            span: Span,
        ) -> Result<(ValueId, LoanId, Vec<LoanId>), LoweringError> {
            let node = self
                .parsed
                .ast()
                .expressions()
                .get(expression)
                .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
            let Expression::Call { arguments, .. } = node.payload().clone() else {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            };
            let descriptor = self
                .typed
                .call(expression)
                .cloned()
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let CallableResultSource::Carrier(contract) = descriptor.result_source() else {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            };
            let (index, receiver) = match contract.origin() {
                BorrowReturnOrigin::Parameter(index) => (index, None),
                BorrowReturnOrigin::Receiver => {
                    let receiver = descriptor
                        .receiver()
                        .filter(|receiver| receiver.mode() == ParameterMode::Borrow)
                        .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                    let lang_frontend::type_checking::CallReceiverOrigin::Expression(receiver) =
                        receiver.origin()
                    else {
                        return Err(error(LoweringErrorKind::UnsupportedNode, span));
                    };
                    (0, Some(receiver))
                }
            };
            let view = self.expression_ssa_type(expression, span)?;
            let Some(SsaTypeKind::RangeView { source: source_ty }) =
                self.ssa_types.get(view.index())
            else {
                return Err(error(LoweringErrorKind::MissingFact, span));
            };
            let source_ty = *source_ty;
            let offset = usize::from(receiver.is_some());
            let mut ordered = vec![None; arguments.len() + offset];
            let mut borrowed = Vec::new();
            if let Some(receiver) = receiver {
                let (loan, new) = self.lower_borrow_argument(expression, receiver, span)?;
                ordered[0] = Some(EntityId::Loan(loan));
                self.pending_call_loans
                    .insert((expression.index(), receiver.index()), new.then_some(loan));
                borrowed.push((0, receiver));
            }
            for (actual, argument) in arguments.iter().enumerate() {
                let mapping = descriptor
                    .arguments()
                    .iter()
                    .find(|m| m.argument_index() == actual)
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                let entity = match mapping.mode() {
                    ParameterMode::Borrow => {
                        let (loan, new) =
                            self.lower_borrow_argument(expression, argument.value, argument.span)?;
                        self.pending_call_loans.insert(
                            (expression.index(), argument.value.index()),
                            new.then_some(loan),
                        );
                        borrowed.push((mapping.parameter_index() + offset, argument.value));
                        EntityId::Loan(loan)
                    }
                    ParameterMode::Value => {
                        if self
                            .typed
                            .expression_type(argument.value)
                            .and_then(|ty| self.typed.copyability(ty))
                            != Some(Copyability::Copyable)
                        {
                            return Err(error(LoweringErrorKind::UnsupportedNode, span));
                        }
                        EntityId::Value(self.require_value(argument.value)?)
                    }
                    ParameterMode::Inout => {
                        return Err(error(LoweringErrorKind::UnsupportedNode, span));
                    }
                };
                let slot = ordered
                    .get_mut(mapping.parameter_index() + offset)
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                if slot.replace(entity).is_some() {
                    return Err(error(LoweringErrorKind::MissingFact, span));
                }
            }
            // Later operands can branch or return. Recover only the current block's
            // frontend-authorized loan identities, and leave early-exit cleanup to the same map.
            let mut created = Vec::new();
            for (slot, argument) in borrowed {
                let pending = self
                    .pending_call_loans
                    .remove(&(expression.index(), argument.index()))
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                if let Some(loan) = pending {
                    ordered[slot] = Some(EntityId::Loan(loan));
                    created.push(loan);
                } else {
                    let actual =
                        crate::ssa::borrow_result_support::ungroup(self.parsed, argument, span)?;
                    let node = self
                        .parsed
                        .ast()
                        .expressions()
                        .get(actual)
                        .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
                    if let Some(loan) = self
                        .references
                        .get(&span_key(node.span()))
                        .and_then(|symbol| self.borrow_bindings.get(symbol))
                    {
                        ordered[slot] = Some(EntityId::Loan(*loan));
                    }
                }
            }
            let arguments = ordered
                .into_iter()
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let Some(EntityId::Loan(source)) = arguments.get(index).copied() else {
                return Err(error(LoweringErrorKind::MissingFact, span));
            };
            let operation = if descriptor.range_construction().is_some() {
                let [_, begin, end] = arguments.as_slice() else {
                    return Err(error(LoweringErrorKind::MissingFact, span));
                };
                let begin = self.range_index_operand(*begin, span)?;
                let end = self.range_index_operand(*end, span)?;
                Operation::RangeConstruct {
                    view,
                    source,
                    begin,
                    end,
                }
            } else {
                let token = self
                    .source_token
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                let key = self
                    .instance_plan
                    .call_site(token, expression)
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                let callee = *self
                    .function_ids
                    .get(key)
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                Operation::RangeCall {
                    callee,
                    arguments,
                    source,
                }
            };
            let (_, results) = self.append(
                operation,
                vec![
                    EntityType::Value(view),
                    EntityType::Loan {
                        kind: LoanKind::Shared,
                        target: source_ty,
                    },
                ],
                node.span(),
            )?;
            let [EntityId::Value(view), EntityId::Loan(loan)] = results.as_slice() else {
                return Err(error(LoweringErrorKind::InvalidModel, span));
            };
            let (view, loan) = (*view, *loan);
            let mut continued = Vec::new();
            for created in created.into_iter().rev() {
                if created == source
                    && self
                        .function
                        .entity(EntityId::Loan(source))
                        .is_some_and(|data| data.ty.semantic_type() == source_ty)
                {
                    continued.push(created);
                } else {
                    self.append(Operation::BorrowEnd { loan: created }, vec![], span)?;
                }
            }
            self.finish_short_call_ranges(expression, span)?;
            Ok((view, loan, continued))
        }

        fn range_index_operand(
            &mut self,
            entity: EntityId,
            span: Span,
        ) -> Result<ValueId, LoweringError> {
            if let EntityId::Value(value) = entity {
                return Ok(value);
            }
            let EntityId::Loan(loan) = entity else {
                return Err(error(LoweringErrorKind::MissingFact, span));
            };
            let ty = self
                .function
                .entity(entity)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?
                .ty
                .semantic_type();
            let (_, results) = self.append(
                Operation::Read {
                    source: crate::ssa::model::PlaceAccess::Loan(loan),
                },
                vec![EntityType::Value(ty)],
                span,
            )?;
            Ok(value(results[0]))
        }

        pub(in crate::ssa) fn lower_range_return(
            &mut self,
            expression: ExpressionId,
            span: Span,
        ) -> Result<LoweredValue, LoweringError> {
            if !self
                .owned
                .borrow_results()
                .range_return_origins()
                .iter()
                .any(|fact| fact.expression() == expression)
            {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            let (view, source, created) = self.lower_range_call(expression, span)?;
            if !created.is_empty() {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
            self.emit_drops(DropPoint::ControlTransfer(expression))?;
            self.function
                .set_terminator(
                    self.block,
                    TerminatorKind::RangeReturn { view, source },
                    Origin::Source(span),
                )
                .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
            Ok(LoweredValue::Diverged)
        }

        pub(in crate::ssa) fn lower_range_size(
            &mut self,
            expression: ExpressionId,
        ) -> Result<LoweredValue, LoweringError> {
            let descriptor = self.typed.range_size(expression).ok_or(LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?;
            let span = descriptor.span();
            let node = self
                .parsed
                .ast()
                .expressions()
                .get(crate::ssa::borrow_result_support::ungroup(
                    self.parsed,
                    descriptor.receiver(),
                    span,
                )?)
                .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
            if !matches!(node.payload(), Expression::Name | Expression::This) {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
            let symbol = self
                .references
                .get(&span_key(node.span()))
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let view = if let Some(LoweredValue::Value(view)) = self.bindings.get(symbol) {
                EntityId::Value(*view)
            } else {
                EntityId::Loan(
                    *self
                        .borrow_bindings
                        .get(symbol)
                        .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?,
                )
            };
            let result = self.expression_ssa_type(expression, span)?;
            let (_, results) = self.append(
                Operation::RangeLength { view },
                vec![EntityType::Value(result)],
                span,
            )?;
            Ok(LoweredValue::Value(value(results[0])))
        }
    }
}
