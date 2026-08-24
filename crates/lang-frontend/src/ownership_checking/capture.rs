use std::collections::{BTreeMap, BTreeSet};

use crate::{
    ast::ExpressionId,
    name_resolution::{NameResolution, ReferenceTarget, ScopeId, ScopeKind, SymbolId, SymbolKind},
    parser::{Expression, ParsedFile},
    source::Span,
    type_checking::{
        BuiltinType, Capability, Copyability, DeferredReason, IntrinsicTypeConstructor, NominalId,
        NominalKind, TypeId, TypeKind, TypeParameterBound, TypedFile,
    },
};

use super::{
    ClosureCaptureDescriptor, ClosureCaptureEffect, ClosureCaptureMode, ClosureCaptureSource,
    ClosureDescriptor, OwnershipCheckingError, Transferability,
};

pub(super) struct Analysis {
    pub(super) captures: Vec<ClosureCaptureDescriptor>,
    pub(super) closures: Vec<ClosureDescriptor>,
    pub(super) transferabilities: Vec<Transferability>,
}

#[derive(Clone, Copy)]
struct Lambda {
    expression: ExpressionId,
    scope: ScopeId,
    span: Span,
    move_owned: bool,
}

#[derive(Clone, Copy)]
struct Candidate {
    source: ClosureCaptureSource,
    ty: TypeId,
    span: Span,
}

pub(super) fn analyze(
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
) -> Result<Analysis, OwnershipCheckingError> {
    let mut lambda_scopes = names
        .scopes()
        .iter()
        .filter(|scope| scope.kind() == ScopeKind::Lambda)
        .filter_map(|scope| {
            scope
                .span()
                .map(|span| ((span.start(), span.end()), scope.id()))
        })
        .collect::<BTreeMap<_, _>>();
    let mut lambdas = parsed
        .ast()
        .expressions()
        .iter()
        .filter_map(|(expression, node)| match node.payload() {
            Expression::Lambda { move_span, .. } => {
                Some((expression, node.span(), move_span.is_some()))
            }
            _ => None,
        })
        .filter_map(|(expression, span, move_owned)| {
            lambda_scopes
                .remove(&(span.start(), span.end()))
                .map(|scope| Lambda {
                    expression,
                    scope,
                    span,
                    move_owned,
                })
        })
        .collect::<Vec<_>>();
    lambdas.sort_by_key(|lambda| (lambda.span.start(), lambda.span.end()));

    let evaluator = TransferabilityEvaluator::new(typed);
    let transferabilities = evaluator.all();
    let mut captures = Vec::new();
    let mut closures = Vec::with_capacity(lambdas.len());
    for lambda in lambdas {
        let mut candidates = symbol_candidates(lambda, names, typed);
        candidates.extend(this_candidates(lambda, parsed, typed)?);
        candidates.sort_by_key(|candidate| {
            (
                candidate.span.start(),
                candidate.span.end(),
                candidate.source,
            )
        });
        let mut seen = BTreeSet::new();
        candidates.retain(|candidate| seen.insert(candidate.source));

        let mode = if lambda.move_owned {
            ClosureCaptureMode::Owned
        } else {
            ClosureCaptureMode::Shared
        };
        let first_capture = captures.len();
        for candidate in candidates {
            let effect = if mode == ClosureCaptureMode::Shared {
                ClosureCaptureEffect::Borrow
            } else {
                match typed.copyability(candidate.ty) {
                    Some(Copyability::Copyable) => ClosureCaptureEffect::Copy,
                    Some(Copyability::MoveOnly) => ClosureCaptureEffect::Move,
                    Some(Copyability::Unknown | Copyability::Error) | None => {
                        ClosureCaptureEffect::Unknown
                    }
                }
            };
            captures.push(ClosureCaptureDescriptor::new(
                lambda.expression,
                candidate.source,
                candidate.ty,
                mode,
                effect,
                candidate.span,
            ));
        }
        let lambda_captures = &captures[first_capture..];
        let transferability = if !lambda.move_owned && !lambda_captures.is_empty() {
            Transferability::NotTransferable
        } else {
            lambda_captures
                .iter()
                .fold(Transferability::Transferable, |state, capture| {
                    combine(state, evaluator.of(capture.ty()))
                })
        };
        closures.push(ClosureDescriptor::new(
            lambda.expression,
            lambda.move_owned,
            transferability,
        ));
    }

    Ok(Analysis {
        captures,
        closures,
        transferabilities,
    })
}

fn symbol_candidates(lambda: Lambda, names: &NameResolution, typed: &TypedFile) -> Vec<Candidate> {
    names
        .references()
        .iter()
        .filter(|reference| is_descendant(reference.scope(), lambda.scope, names))
        .filter_map(|reference| {
            let ReferenceTarget::Symbol(symbol) = reference.target() else {
                return None;
            };
            let declaration = names.symbols().get(symbol.index())?;
            if is_descendant(declaration.scope(), lambda.scope, names) {
                return None;
            }
            match declaration.kind() {
                SymbolKind::Variable
                | SymbolKind::ValueParameter
                | SymbolKind::LambdaParameter
                | SymbolKind::ForBinding
                | SymbolKind::DestructuringBinding => Some(Candidate {
                    source: ClosureCaptureSource::Symbol(*symbol),
                    ty: typed.symbol_type(*symbol)?,
                    span: reference.span(),
                }),
                SymbolKind::Field => Some(Candidate {
                    source: ClosureCaptureSource::This,
                    ty: classifier_type(declaration.scope(), names, typed)?,
                    span: reference.span(),
                }),
                SymbolKind::Classifier
                | SymbolKind::ObjectValue
                | SymbolKind::TypeParameter
                | SymbolKind::Constant
                | SymbolKind::Function
                | SymbolKind::EnumVariant
                | SymbolKind::EnumCaseType => None,
            }
        })
        .collect()
}

fn this_candidates(
    lambda: Lambda,
    parsed: &ParsedFile,
    typed: &TypedFile,
) -> Result<Vec<Candidate>, OwnershipCheckingError> {
    let mut candidates = Vec::new();
    for (expression, node) in parsed.ast().expressions().iter() {
        if matches!(node.payload(), Expression::This)
            && contains(lambda.span, node.span())
            && let Some(ty) = typed.expression_type(expression)
        {
            candidates.push(Candidate {
                source: ClosureCaptureSource::This,
                ty,
                span: node.span(),
            });
        }
    }
    Ok(candidates)
}

fn classifier_type(scope: ScopeId, names: &NameResolution, typed: &TypedFile) -> Option<TypeId> {
    let classifier_scope = ancestors(scope, names)
        .find(|scope| names.scopes()[scope.index()].kind() == ScopeKind::Classifier)?;
    let classifier_span = names.scopes()[classifier_scope.index()].span()?;
    typed.nominals().iter().find_map(|descriptor| {
        let symbol = names.symbols().get(descriptor.id().symbol().index())?;
        contains(classifier_span, symbol.span()).then(|| typed.symbol_type(symbol.id()))?
    })
}

fn ancestors(scope: ScopeId, names: &NameResolution) -> impl Iterator<Item = ScopeId> + '_ {
    std::iter::successors(Some(scope), |scope| {
        names
            .scopes()
            .get(scope.index())
            .and_then(|scope| scope.parent())
    })
}

fn is_descendant(scope: ScopeId, ancestor: ScopeId, names: &NameResolution) -> bool {
    ancestors(scope, names).any(|scope| scope == ancestor)
}

fn contains(outer: Span, inner: Span) -> bool {
    outer.source_id() == inner.source_id()
        && outer.start() <= inner.start()
        && inner.end() <= outer.end()
}

struct TransferabilityEvaluator<'a> {
    typed: &'a TypedFile,
}

impl<'a> TransferabilityEvaluator<'a> {
    const fn new(typed: &'a TypedFile) -> Self {
        Self { typed }
    }

    fn all(&self) -> Vec<Transferability> {
        (0..self.typed.types().len())
            .map(|index| self.of(TypeId::new(index)))
            .collect()
    }

    fn of(&self, ty: TypeId) -> Transferability {
        self.with(ty, &BTreeMap::new(), &mut BTreeSet::new())
    }

    fn with(
        &self,
        ty: TypeId,
        substitutions: &BTreeMap<SymbolId, TypeId>,
        active: &mut BTreeSet<NominalId>,
    ) -> Transferability {
        let Some(kind) = self.typed.types().get(ty) else {
            return Transferability::Error;
        };
        match kind {
            TypeKind::Builtin(
                BuiltinType::Byte
                | BuiltinType::Short
                | BuiltinType::Int
                | BuiltinType::Long
                | BuiltinType::UByte
                | BuiltinType::UShort
                | BuiltinType::UInt
                | BuiltinType::ULong
                | BuiltinType::Float
                | BuiltinType::Double
                | BuiltinType::Boolean
                | BuiltinType::Char
                | BuiltinType::String
                | BuiltinType::Unit
                | BuiltinType::Nothing,
            )
            | TypeKind::IntegerLiteral(_) => Transferability::Transferable,
            TypeKind::Builtin(BuiltinType::Any) | TypeKind::Function { .. } => {
                Transferability::NotTransferable
            }
            TypeKind::Nullable(inner) => self.with(*inner, substitutions, active),
            TypeKind::Nominal { nominal, arguments } => {
                self.nominal(*nominal, arguments, substitutions, active)
            }
            TypeKind::Intrinsic {
                constructor: IntrinsicTypeConstructor::Rc,
                ..
            } => Transferability::NotTransferable,
            TypeKind::Intrinsic {
                constructor:
                    IntrinsicTypeConstructor::Box
                    | IntrinsicTypeConstructor::Array
                    | IntrinsicTypeConstructor::List
                    | IntrinsicTypeConstructor::MutableList,
                arguments,
            } => arguments
                .iter()
                .fold(Transferability::Transferable, |state, argument| {
                    combine(state, self.with(*argument, substitutions, active))
                }),
            TypeKind::EnumCase { root, .. } => self.with(*root, substitutions, active),
            TypeKind::TypeParameter(symbol) => {
                if let Some(actual) = substitutions.get(symbol).copied()
                    && actual != ty
                {
                    return self.with(actual, substitutions, active);
                }
                match self
                    .typed
                    .type_parameters()
                    .iter()
                    .find(|descriptor| descriptor.symbol() == *symbol)
                    .map(|descriptor| descriptor.bound())
                {
                    Some(TypeParameterBound::Capability(Capability::Transferable)) => {
                        Transferability::Transferable
                    }
                    Some(TypeParameterBound::Error) => Transferability::Error,
                    Some(
                        TypeParameterBound::Any
                        | TypeParameterBound::Interface(_)
                        | TypeParameterBound::Capability(Capability::Copyable),
                    )
                    | None => Transferability::NotTransferable,
                }
            }
            TypeKind::Deferred(DeferredReason::AnyValueRepresentation) => {
                Transferability::NotTransferable
            }
            TypeKind::Deferred(_) => Transferability::Unknown,
            TypeKind::Error | TypeKind::StaticSelf(_) | TypeKind::Capability(_) => {
                Transferability::Error
            }
        }
    }

    fn nominal(
        &self,
        nominal: NominalId,
        arguments: &[TypeId],
        outer: &BTreeMap<SymbolId, TypeId>,
        active: &mut BTreeSet<NominalId>,
    ) -> Transferability {
        if !active.insert(nominal) {
            // Reference-semantics recursive graphs are evaluated coinductively.
            return Transferability::Transferable;
        }
        let Some(descriptor) = self
            .typed
            .nominals()
            .iter()
            .find(|descriptor| descriptor.id() == nominal)
        else {
            active.remove(&nominal);
            return Transferability::Error;
        };
        let result = match descriptor.kind() {
            NominalKind::Object => Transferability::NotTransferable,
            NominalKind::Interface => Transferability::Error,
            NominalKind::ValueClass | NominalKind::Class | NominalKind::EnumClass => {
                let mut substitutions = outer.clone();
                substitutions.extend(
                    descriptor
                        .type_parameters()
                        .iter()
                        .copied()
                        .zip(arguments.iter().copied()),
                );
                let components = if descriptor.kind() == NominalKind::EnumClass {
                    self.typed
                        .enum_cases()
                        .iter()
                        .filter(|case| case.root() == nominal)
                        .flat_map(|case| case.payloads().iter().map(|(_, ty)| *ty))
                        .collect::<Vec<_>>()
                } else {
                    let Some(fields) = descriptor
                        .fields()
                        .iter()
                        .map(|field| self.typed.symbol_type(*field))
                        .collect::<Option<Vec<_>>>()
                    else {
                        active.remove(&nominal);
                        return Transferability::Error;
                    };
                    fields
                };
                components
                    .into_iter()
                    .fold(Transferability::Transferable, |state, component| {
                        combine(state, self.with(component, &substitutions, active))
                    })
            }
        };
        active.remove(&nominal);
        result
    }
}

fn combine(left: Transferability, right: Transferability) -> Transferability {
    match (left, right) {
        (Transferability::Error, _) | (_, Transferability::Error) => Transferability::Error,
        (Transferability::Unknown, _) | (_, Transferability::Unknown) => Transferability::Unknown,
        (Transferability::NotTransferable, _) | (_, Transferability::NotTransferable) => {
            Transferability::NotTransferable
        }
        (Transferability::Transferable, Transferability::Transferable) => {
            Transferability::Transferable
        }
    }
}
