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
/// Opaque same-analysis evidence for exactly two scalar vals and one direct call.
#[derive(Clone, Debug)]
pub struct EntryDescriptor {
    identity: Rc<()>,
    entry: DeclarationId,
    body: Span,
    instance: UnitCallableInstanceKey,
    initializers: [UnitExpressionId; 2],
    contracts: [UnitCallArgumentOwnershipContract; 4],
    ends: [UnitDirectCallLoanEndFact; 4],
    drops: [UnitDropFact; 2],
    strings: [Vec<u8>; 2],
    rounds: i32,
    choose: bool,
    initializer_spans: [Span; 2],
}
impl EntryDescriptor {
    /// Original analyzed plain String payloads, excluding escapes/interpolation.
    pub fn strings(&self) -> &[Vec<u8>; 2] {
        &self.strings
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
    /// Actual temporary owner drops in traversal order.
    pub fn drops(&self) -> &[UnitDropFact; 2] {
        &self.drops
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
            let [first, second, last] = elements.as_slice() else {
                return None;
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
                let mut bindings = owned.formed_owned_locals().iter().filter(|symbol| {
                    symbol.source_unit() == unit
                        && names
                            .names()
                            .source_units()
                            .get(unit.index())
                            .and_then(|source| {
                                source.resolution().symbols().get(symbol.symbol().index())
                            })
                            .is_some_and(|resolved| resolved.span() == *span)
                });
                let binding = *bindings.next()?;
                if bindings.next().is_some() {
                    return None;
                }
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
                if i < 2 {
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
                {
                    return None;
                }
                match (i, end.loan().target()) {
                    (0 | 1, UnitLoanTarget::Temporary(owner)) if *owner == c.argument() => {}
                    (2 | 3, UnitLoanTarget::Place(place))
                        if place.is_root() && place.root() == locals[i - 2].0 => {}
                    _ => return None,
                }
            }
            let drops: [UnitDropFact; 2] = owned
                .drops()
                .iter()
                .filter(|d| d.point() == UnitDropPoint::CallReturn(call_id))
                .copied()
                .collect::<Vec<_>>()
                .try_into()
                .ok()?;
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
            let strings: [Vec<u8>; 2] = contracts[..2]
                .iter()
                .map(|c| {
                    let Expression::String { parts } = parsed
                        .ast()
                        .expressions()
                        .get(c.argument().expression())
                        .ok()?
                        .payload()
                    else {
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
                })
                .collect::<Option<Vec<_>>>()?
                .try_into()
                .ok()?;
            Some(EntryDescriptor {
                identity: Rc::new(()),
                strings,
                rounds,
                choose: *choose,
                initializer_spans,
                entry: declaration.id(),
                body: body_node.span(),
                instance: call.instance().clone(),
                initializers: [locals[0].1, locals[1].1],
                contracts,
                ends,
                drops,
            })
        })
        .collect()
}
