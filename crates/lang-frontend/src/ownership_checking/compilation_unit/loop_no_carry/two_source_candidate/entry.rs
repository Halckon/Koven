//! Private-construction source entry evidence; not normal ownership or backend authority.
use super::*;
use crate::{
    name_resolution::{DeclarationId, SourceUnitInput, ValidatedCompilationUnitNames},
    ownership_checking::{
        CompilationUnitOwnership, UnitCallArgumentOwnershipContract, UnitCallArgumentOwnershipKind,
        UnitDirectCallLoanEndFact, UnitDropFact, UnitDropPoint, UnitDropTarget, UnitLoanTarget,
    },
    parser::{
        Expression, FunctionBody, FunctionForm, Item, LiteralKind, NameMarker, Statement,
        StringPart, VariableKind,
    },
    source::Span,
    type_checking::{BuiltinType, UnitTypeKind, ValidatedCompilationUnitTypes},
};
/// Opaque evidence for two scalar vals and one call, optionally preceded by one shared String owner.
#[derive(Clone, Debug)]
pub struct EntryDescriptor {
    identity: Rc<()>,
    entry: DeclarationId,
    body: Span,
    instance: UnitCallableInstanceKey,
    initializers: [UnitExpressionId; 2],
    contracts: [UnitCallArgumentOwnershipContract; 4],
    ends: [UnitDirectCallLoanEndFact; 4],
    storage: StringStorage,
    rounds: i32,
    choose: bool,
    initializer_spans: [Span; 2],
}
#[derive(Clone, Debug)]
enum StringStorage {
    Temporary {
        strings: [Vec<u8>; 2],
        drops: [UnitDropFact; 2],
    },
    Named {
        root: UnitSymbolId,
        initializer: UnitExpressionId,
        span: Span,
        bytes: Vec<u8>,
        drop: UnitDropFact,
    },
}
impl EntryDescriptor {
    /// Temporary-only original String payloads; named-owner entries return None.
    pub fn strings(&self) -> Option<&[Vec<u8>; 2]> {
        match &self.storage {
            StringStorage::Temporary { strings, .. } => Some(strings),
            _ => None,
        }
    }
    /// A single real named owner shared by the two distinct String argument loans.
    pub fn named_string(&self) -> Option<(UnitSymbolId, UnitExpressionId, Span, &[u8])> {
        match &self.storage {
            StringStorage::Named {
                root,
                initializer,
                span,
                bytes,
                ..
            } => Some((*root, *initializer, *span, bytes)),
            _ => None,
        }
    }
    /// Original analyzed nonnegative decimal Int.
    pub fn rounds(&self) -> i32 {
        self.rounds
    }
    /// Original analyzed Boolean.
    pub fn choose(&self) -> bool {
        self.choose
    }
    /// Original initializer spans.
    pub fn initializer_spans(&self) -> [Span; 2] {
        self.initializer_spans
    }

    pub(super) fn same_identity(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.identity, &other.identity)
    }
    /// Original zero-parameter Unit declaration.
    pub fn entry(&self) -> DeclarationId {
        self.entry
    }
    /// Complete source body.
    pub fn body(&self) -> Span {
        self.body
    }
    /// Exact called instance.
    pub fn instance(&self) -> &UnitCallableInstanceKey {
        &self.instance
    }
    /// Original Int and Boolean local initializers.
    pub fn initializers(&self) -> &[UnitExpressionId; 2] {
        &self.initializers
    }
    /// Actual argument contracts in source and parameter order.
    pub fn contracts(&self) -> &[UnitCallArgumentOwnershipContract; 4] {
        &self.contracts
    }
    /// Actual End events in traversal order, not inferred from descriptors.
    pub fn ends(&self) -> &[UnitDirectCallLoanEndFact; 4] {
        &self.ends
    }
    /// Actual owner drops in traversal order (two temporaries or one named owner).
    pub fn drops(&self) -> &[UnitDropFact] {
        match &self.storage {
            StringStorage::Temporary { drops, .. } => drops,
            StringStorage::Named { drop, .. } => std::slice::from_ref(drop),
        }
    }
}
pub(super) fn collect(
    sources: &crate::source::SourceMap,
    shells: &[Option<FunctionShell>],
    inputs: &[SourceUnitInput<'_>],
    names: &ValidatedCompilationUnitNames,
    typed: &ValidatedCompilationUnitTypes,
    owned: &CompilationUnitOwnership,
) -> Vec<EntryDescriptor> {
    names
        .names()
        .index()
        .declarations()
        .iter()
        .filter_map(|declaration| {
            let signature = typed
                .types()
                .signatures()
                .declaration(declaration.id())?
                .callable()?;
            if signature.receiver().is_some()
                || !signature.parameters().is_empty()
                || !signature.type_parameters().is_empty()
                || signature.borrow_return().is_some()
                || typed.types().types().get(signature.return_type())
                    != Some(&UnitTypeKind::Builtin(BuiltinType::Unit))
            {
                return None;
            }
            let unit = declaration.source_unit();
            let source = names
                .names()
                .index()
                .source_units()
                .iter()
                .find(|s| s.id() == unit)?;
            let parsed = inputs
                .iter()
                .find(|i| i.source_id() == source.source_id())?
                .parsed();
            let Item::Function {
                form,
                extension_receiver: None,
                ..
            } = parsed.ast().items().get(declaration.root()).ok()?.payload()
            else {
                return None;
            };
            let body = match form {
                FunctionForm::ImplicitUnitBlock(body)
                | FunctionForm::Explicit {
                    body: FunctionBody::Block(body),
                    ..
                } => *body,
                _ => return None,
            };
            let body_node = parsed.ast().statements().get(body).ok()?;
            let Statement::Block { elements } = body_node.payload() else {
                return None;
            };
            let (named_statement, first, second, last) = match elements.as_slice() {
                [first, second, last] => (None, first, second, last),
                [named, first, second, last] => (Some(named), first, second, last),
                _ => return None,
            };
            let formed_local = |span: Span| {
                let mut bindings = owned.formed_owned_locals().iter().filter(|symbol| {
                    symbol.source_unit() == unit
                        && names
                            .names()
                            .source_units()
                            .get(unit.index())
                            .and_then(|s| s.resolution().symbols().get(symbol.symbol().index()))
                            .is_some_and(|resolved| resolved.span() == span)
                });
                let binding = *bindings.next()?;
                bindings.next().is_none().then_some(binding)
            };
            let named = if let Some(statement) = named_statement {
                let Statement::LocalVariable { declaration } =
                    parsed.ast().statements().get(*statement).ok()?.payload()
                else {
                    return None;
                };
                let Item::Variable {
                    kind: VariableKind::Val,
                    name: NameMarker::Present(name),
                    initializer,
                    ..
                } = parsed.ast().items().get(*declaration).ok()?.payload()
                else {
                    return None;
                };
                let root = formed_local(*name)?;
                let initializer = UnitExpressionId::new(unit, *initializer);
                if typed
                    .types()
                    .symbol_type(root)
                    .and_then(|t| typed.types().types().get(t))
                    != Some(&UnitTypeKind::Builtin(BuiltinType::String))
                    || typed
                        .types()
                        .expression_type(initializer)
                        .and_then(|t| typed.types().types().get(t))
                        != Some(&UnitTypeKind::Builtin(BuiltinType::String))
                {
                    return None;
                }
                let node = parsed
                    .ast()
                    .expressions()
                    .get(initializer.expression())
                    .ok()?;
                let bytes = plain_string(sources, node.payload())?;
                Some((root, initializer, node.span(), bytes))
            } else {
                None
            };
            let mut locals = Vec::new();
            for (position, statement) in [first, second].into_iter().enumerate() {
                let Statement::LocalVariable { declaration } =
                    parsed.ast().statements().get(*statement).ok()?.payload()
                else {
                    return None;
                };
                let Item::Variable {
                    kind: VariableKind::Val,
                    name: NameMarker::Present(span),
                    initializer,
                    ..
                } = parsed.ast().items().get(*declaration).ok()?.payload()
                else {
                    return None;
                };
                let expression = parsed.ast().expressions().get(*initializer).ok()?;
                if !matches!(
                    (position, expression.payload()),
                    (0, Expression::Literal(LiteralKind::Integer(_)))
                        | (1, Expression::Literal(LiteralKind::Boolean(_)))
                ) {
                    return None;
                }
                let binding = formed_local(*span)?;
                let initializer = UnitExpressionId::new(unit, *initializer);
                let expected = UnitTypeKind::Builtin(if position == 0 {
                    BuiltinType::Int
                } else {
                    BuiltinType::Boolean
                });
                if typed
                    .types()
                    .expression_type(initializer)
                    .and_then(|ty| typed.types().types().get(ty))
                    != Some(&expected)
                    || typed
                        .types()
                        .symbol_type(binding)
                        .and_then(|ty| typed.types().types().get(ty))
                        != Some(&expected)
                {
                    return None;
                }
                locals.push((binding, initializer));
            }
            let Statement::Expression { expression } =
                parsed.ast().statements().get(*last).ok()?.payload()
            else {
                return None;
            };
            let call_id = UnitExpressionId::new(unit, *expression);
            let call = typed.types().call(call_id)?;
            if call.receiver().is_some() || !call.instance().type_arguments().is_empty() {
                return None;
            }
            let UnitCallTarget::Declaration(target) = call.target() else {
                return None;
            };
            if !shells
                .iter()
                .flatten()
                .any(|s| s.callable() == target && s.roles() == [0, 1, 2, 3])
            {
                return None;
            }
            let contracts: [UnitCallArgumentOwnershipContract; 4] = owned
                .call_argument_contracts_for(call_id)
                .copied()
                .collect::<Vec<_>>()
                .try_into()
                .ok()?;
            if call.arguments().len() != 4 {
                return None;
            }
            for (i, c) in contracts.iter().enumerate() {
                if c.parameter_index() != i
                    || contracts
                        .iter()
                        .filter(|other| other.argument() == c.argument())
                        .count()
                        != 1
                    || c.kind() != UnitCallArgumentOwnershipKind::SharedLoan
                    || c.crosses_thread()
                    || call.arguments()[i].argument_index() != i
                    || call.arguments()[i].parameter_index() != i
                    || call.arguments()[i].parameter_type() != c.parameter_type()
                {
                    return None;
                }
                let node = parsed
                    .ast()
                    .expressions()
                    .get(c.argument().expression())
                    .ok()?;
                if i < 2 && named.is_some() {
                    if !matches!(node.payload(), Expression::Name) {
                        return None;
                    }
                } else if i < 2 {
                    let Expression::String { parts } = node.payload() else {
                        return None;
                    };
                    if parts
                        .iter()
                        .any(|part| !matches!(part, StringPart::Text(_)))
                    {
                        return None;
                    }
                } else if !matches!(node.payload(), Expression::Name) {
                    return None;
                }
            }
            let ends: [UnitDirectCallLoanEndFact; 4] = owned
                .direct_call_loan_ends()
                .iter()
                .filter(|e| e.loan().call() == call_id)
                .cloned()
                .collect::<Vec<_>>()
                .try_into()
                .ok()?;
            for (i, c) in contracts.iter().enumerate() {
                let mut matching = ends.iter().filter(|e| e.loan().argument() == c.argument());
                let end = matching.next()?;
                if matching.next().is_some()
                    || end.point() != UnitDropPoint::CallReturn(call_id)
                    || end.preceding_drops() != 0
                    || end.loan().kind() != crate::ownership_checking::LoanKind::Shared
                {
                    return None;
                }
                match (i, end.loan().target()) {
                    (0 | 1, UnitLoanTarget::Temporary(owner))
                        if named.is_none() && *owner == c.argument() => {}
                    (0 | 1, UnitLoanTarget::Place(place))
                        if named
                            .as_ref()
                            .is_some_and(|n| place.is_root() && place.root() == n.0) => {}
                    (2 | 3, UnitLoanTarget::Place(place))
                        if place.is_root() && place.root() == locals[i - 2].0 => {}
                    _ => return None,
                }
            }
            for (position, end) in ends.iter().enumerate() {
                if end.preceding_call_loan_ends() != position {
                    return None;
                }
            }
            let drops: Vec<_> = owned
                .drops()
                .iter()
                .filter(|d| d.point() == UnitDropPoint::CallReturn(call_id))
                .copied()
                .collect();
            let storage = if let Some((root, initializer, span, bytes)) = named {
                let [drop] = drops.as_slice() else {
                    return None;
                };
                if drop.target() != UnitDropTarget::Named(root)
                    || owned
                        .drops()
                        .iter()
                        .filter(|d| d.target() == UnitDropTarget::Named(root))
                        .count()
                        != 1
                {
                    return None;
                }
                StringStorage::Named {
                    root,
                    initializer,
                    span,
                    bytes,
                    drop: *drop,
                }
            } else {
                let drops: [UnitDropFact; 2] = drops.try_into().ok()?;
                for c in &contracts[..2] {
                    if drops
                        .iter()
                        .filter(|d| d.target() == UnitDropTarget::Temporary(c.argument()))
                        .count()
                        != 1
                    {
                        return None;
                    }
                }
                let strings: [Vec<u8>; 2] = contracts[..2]
                    .iter()
                    .map(|c| {
                        plain_string(
                            sources,
                            parsed
                                .ast()
                                .expressions()
                                .get(c.argument().expression())
                                .ok()?
                                .payload(),
                        )
                    })
                    .collect::<Option<Vec<_>>>()?
                    .try_into()
                    .ok()?;
                StringStorage::Temporary { strings, drops }
            };
            let initializer_spans = [
                parsed
                    .ast()
                    .expressions()
                    .get(locals[0].1.expression())
                    .ok()?
                    .span(),
                parsed
                    .ast()
                    .expressions()
                    .get(locals[1].1.expression())
                    .ok()?
                    .span(),
            ];
            let text = sources.slice(initializer_spans[0]).ok()?;
            if text.is_empty() || !text.bytes().all(|c| c.is_ascii_digit()) {
                return None;
            }
            let rounds = text.parse::<i32>().ok()?;
            let Expression::Literal(LiteralKind::Boolean(choose)) = parsed
                .ast()
                .expressions()
                .get(locals[1].1.expression())
                .ok()?
                .payload()
            else {
                return None;
            };
            Some(EntryDescriptor {
                identity: Rc::new(()),
                storage,
                rounds,
                choose: *choose,
                initializer_spans,
                entry: declaration.id(),
                body: body_node.span(),
                instance: call.instance().clone(),
                initializers: [locals[0].1, locals[1].1],
                contracts,
                ends,
            })
        })
        .collect()
}

fn plain_string(sources: &crate::source::SourceMap, expression: &Expression) -> Option<Vec<u8>> {
    let Expression::String { parts } = expression else {
        return None;
    };
    let mut bytes = Vec::new();
    for part in parts {
        let StringPart::Text(span) = part else {
            return None;
        };
        let text = sources.slice(*span).ok()?;
        if text.contains('\\') {
            return None;
        }
        bytes.extend_from_slice(text.as_bytes());
    }
    Some(bytes)
}
