//! 单文件与 compilation-unit adapter 共用的中立 lowering 支撑。

pub(in crate::ssa) const NULL_ASSERTION_MESSAGE: &str = "null assertion failed";

pub(in crate::ssa) const fn is_integer_builtin(
    ty: lang_frontend::type_checking::BuiltinType,
) -> bool {
    use lang_frontend::type_checking::BuiltinType::*;
    matches!(
        ty,
        Byte | Short | Int | Long | UByte | UShort | UInt | ULong
    )
}

pub(in crate::ssa) fn builtin_display_call(
    typed: &lang_frontend::type_checking::CompilationUnitTypes,
    call: &lang_frontend::type_checking::UnitCallDescriptor,
    concrete_receiver: Option<lang_frontend::type_checking::UnitTypeId>,
) -> Option<(
    lang_frontend::type_checking::UnitExpressionId,
    lang_frontend::type_checking::BuiltinType,
)> {
    use lang_frontend::type_checking::{
        BuiltinType, CallableResultSource, ParameterMode, UnitCallReceiverOrigin, UnitTypeKind,
    };
    let receiver = call.receiver()?;
    let UnitCallReceiverOrigin::Expression(operand) = receiver.origin() else {
        return None;
    };
    let UnitTypeKind::Builtin(builtin) = *typed.types().get(concrete_receiver?)? else {
        return None;
    };
    (call.is_display_requirement()
        && !receiver.is_qualified_super()
        && receiver.mode() == ParameterMode::Borrow
        && call.arguments().is_empty()
        && call.instance().type_arguments().is_empty()
        && call.result_source() == CallableResultSource::Owned
        && typed.types().get(call.return_type())
            == Some(&UnitTypeKind::Builtin(BuiltinType::String))
        && (matches!(
            builtin,
            BuiltinType::String | BuiltinType::Boolean | BuiltinType::Char
        ) || is_integer_builtin(builtin)
            || is_float_builtin(builtin)))
    .then_some((operand, builtin))
}

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
    ast::{ExpressionId, StatementId},
    name_resolution::{SourceUnitInput, ValidatedCompilationUnitNames},
    ownership_checking::UnitLoweringFacts,
    parser::{
        Expression, IntegerLiteralKind, LiteralKind, NameMarker, ParsedFile, WhenCondition,
        WhenEntry,
    },
    source::Span,
    type_checking::{
        BuiltinType, CompilationUnitTypes, ParameterMode, UnitCallDescriptor,
        UnitCallReceiverOrigin, UnitCallTarget, UnitExpressionId, UnitTypeId, UnitTypeKind,
    },
};

type BooleanWhenArms = ((StatementId, usize), (StatementId, usize));

/// The two distinct outcomes supported by ordinary Boolean when lowering.
pub(in crate::ssa) fn boolean_literal_when_arms(
    parsed: &ParsedFile,
    entries: &[WhenEntry],
) -> Result<Option<BooleanWhenArms>, Span> {
    let mut when_true = None;
    let mut when_false = None;
    for (index, entry) in entries.iter().enumerate() {
        if entry.else_span.is_some() {
            let arm = (entry.body, index);
            when_true.get_or_insert(arm);
            when_false.get_or_insert(arm);
            break;
        }
        for condition in &entry.conditions {
            let WhenCondition::Expression(condition) = condition else {
                return Ok(None);
            };
            let node = parsed
                .ast()
                .expressions()
                .get(*condition)
                .map_err(|_| entry.span)?;
            match node.payload() {
                Expression::Literal(LiteralKind::Boolean(true)) => {
                    when_true.get_or_insert((entry.body, index));
                }
                Expression::Literal(LiteralKind::Boolean(false)) => {
                    when_false.get_or_insert((entry.body, index));
                }
                _ => return Ok(None),
            }
        }
    }
    match (when_true, when_false) {
        (Some(t), Some(f)) if t != f => Ok(Some((t, f))),
        _ => Ok(None),
    }
}

/// Heap layouts supported by the ordinary nullable When lowering.
pub(in crate::ssa) fn ordinary_nullable_when_heap_types(
    typed: &CompilationUnitTypes,
    nullable: UnitTypeId,
    inner: UnitTypeId,
) -> bool {
    use lang_frontend::type_checking::Copyability;

    nullable_heap_owner_payload(typed, inner)
        && typed.copyability(nullable) == Copyability::MoveOnly
        && typed.copyability(inner) == Copyability::MoveOnly
}

pub(in crate::ssa) fn nullable_heap_owner_payload(
    typed: &CompilationUnitTypes,
    inner: UnitTypeId,
) -> bool {
    use lang_frontend::type_checking::{IntrinsicTypeConstructor, NominalKind};

    match typed.types().get(inner) {
        Some(UnitTypeKind::Nominal { declaration, .. }) => typed
            .signatures()
            .declaration(*declaration)
            .and_then(|signature| signature.nominal())
            .is_some_and(|nominal| nominal.kind() == NominalKind::Class),
        Some(UnitTypeKind::Intrinsic { constructor, .. }) => matches!(
            constructor,
            IntrinsicTypeConstructor::Box | IntrinsicTypeConstructor::Rc
        ),
        _ => false,
    }
}

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

    pub(in crate::ssa) fn decode_text(
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
    #[allow(
        dead_code,
        reason = "legacy lowering test diagnostics retain native error mapping"
    )]
    FrontendDiagnostics,
    #[allow(
        dead_code,
        reason = "legacy lowering test diagnostics retain native error mapping"
    )]
    BlockingDeferred,
    InstanceLimitExceeded,
    UnsupportedNode,
    UnsupportedCompanionCall,
    MissingFact,
    InvalidLiteral,
    InvalidModel,
    #[allow(
        dead_code,
        reason = "legacy lowering test diagnostics retain native error mapping"
    )]
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

pub(in crate::ssa) const fn is_float_builtin(
    ty: lang_frontend::type_checking::BuiltinType,
) -> bool {
    use lang_frontend::type_checking::BuiltinType;
    matches!(ty, BuiltinType::Float | BuiltinType::Double)
}

pub(in crate::ssa) fn is_nullable_string(typed: &CompilationUnitTypes, ty: UnitTypeId) -> bool {
    matches!(typed.types().get(ty), Some(UnitTypeKind::Nullable(inner))
        if matches!(typed.types().get(*inner), Some(UnitTypeKind::Builtin(BuiltinType::String))))
}

/// For a mixed nullable string/null-only comparison, identify the string operand.
pub(in crate::ssa) fn nullable_string_presence_on_left(
    typed: &CompilationUnitTypes,
    left: UnitTypeId,
    right: UnitTypeId,
) -> Option<bool> {
    if is_nullable_string(typed, left) && is_null_only(typed, right) {
        Some(true)
    } else if is_null_only(typed, left) && is_nullable_string(typed, right) {
        Some(false)
    } else {
        None
    }
}

/// Common equality storage for same-width nullable floats and null-only pairs.
pub(in crate::ssa) fn nullable_equality_storage(
    typed: &CompilationUnitTypes,
    left: UnitTypeId,
    right: UnitTypeId,
) -> Option<UnitTypeId> {
    if left == right && is_null_only(typed, left) {
        return Some(left);
    }
    for (nullable, other) in [(left, right), (right, left)] {
        if let Some(UnitTypeKind::Nullable(inner)) = typed.types().get(nullable)
            && matches!(typed.types().get(*inner), Some(UnitTypeKind::Builtin(b)) if is_float_builtin(*b))
            && (other == nullable
                || other == *inner
                || is_null_only(typed, other)
                || (nullable == left
                    && matches!(
                        typed.types().get(right),
                        Some(UnitTypeKind::Builtin(BuiltinType::Nothing))
                    )))
        {
            return Some(nullable);
        }
    }
    None
}

pub(in crate::ssa) fn is_null_only(typed: &CompilationUnitTypes, ty: UnitTypeId) -> bool {
    matches!(typed.types().get(ty), Some(UnitTypeKind::Nullable(inner))
        if matches!(typed.types().get(*inner), Some(UnitTypeKind::Builtin(BuiltinType::Nothing))))
}

pub(in crate::ssa) fn parse_float_literal(
    sources: &lang_frontend::source::SourceMap,
    kind: lang_frontend::parser::FloatLiteralKind,
    span: Span,
) -> Result<super::model::ScalarConstant, LoweringError> {
    use super::model::ScalarConstant;
    use lang_frontend::parser::FloatLiteralKind;
    let text = sources
        .slice(span)
        .map_err(|_| error(LoweringErrorKind::MismatchedSource, span))?;
    let invalid = || error(LoweringErrorKind::InvalidLiteral, span);
    let digits =
        lang_frontend::type_checking::float_literal_core(text, kind).ok_or_else(invalid)?;
    match kind {
        FloatLiteralKind::Float => digits
            .parse::<f32>()
            .ok()
            .filter(|v| v.is_finite())
            .map(|value| ScalarConstant::Float32(value.to_bits())),
        FloatLiteralKind::Double => digits
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite())
            .map(|value| ScalarConstant::Float64(value.to_bits())),
    }
    .ok_or_else(invalid)
}

pub(in crate::ssa) fn parse_char_literal(
    sources: &lang_frontend::source::SourceMap,
    span: Span,
) -> Result<u32, LoweringError> {
    let text = sources
        .slice(span)
        .map_err(|_| error(LoweringErrorKind::MismatchedSource, span))?;
    lang_frontend::type_checking::decode_char_literal(text)
        .map(u32::from)
        .ok_or_else(|| error(LoweringErrorKind::InvalidLiteral, span))
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

/// An exact Unit value still runs its effects in a source declaration call
/// prefix. No argument slot is needed when the call cannot dispatch.
pub(in crate::ssa) fn unit_argument_in_nondispatching_call(
    typed: &CompilationUnitTypes,
    owned: &dyn UnitLoweringFacts,
    call: &UnitCallDescriptor,
    mode: ParameterMode,
    parameter: UnitTypeId,
    argument: Option<UnitTypeId>,
) -> bool {
    mode == ParameterMode::Value
        && argument == Some(parameter)
        && typed.types().get(parameter) == Some(&UnitTypeKind::Builtin(BuiltinType::Unit))
        && matches!(call.target(), UnitCallTarget::Declaration(_))
        && owned.call_may_dispatch(call.expression()) == Some(false)
}
