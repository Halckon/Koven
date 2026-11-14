//! Complete bounded caller coverage, not SSA emission or publication authority.
use super::*;
use crate::{
    name_resolution::{
        DeclarationId, SourceUnitInput, UnitReferenceTarget, ValidatedCompilationUnitNames,
    },
    parser::{Expression, FunctionBody, FunctionForm, Item, Statement},
    source::Span,
    type_checking::{BuiltinType, ParameterMode, UnitTypeKind, ValidatedCompilationUnitTypes},
};

/// Complete original function admitted by the narrow experimental shell subset.
/// Only four shared parameters and exactly initialization plus the closed loop.
#[derive(Clone, Debug)]
pub struct FunctionShell {
    callable: DeclarationId,
    body: Span,
    roles: [usize; 4],
    symbols: [UnitSymbolId; 4],
    controls: [UnitExpressionId; 4],
    counter: UnitSymbolId,
}
impl FunctionShell {
    /// Original declaration, not a synthetic helper declaration.
    pub fn callable(&self) -> DeclarationId {
        self.callable
    }
    /// Original complete function block.
    pub fn body(&self) -> Span {
        self.body
    }
    /// Absolute signature columns: arm A source, arm B source, rounds, choose.
    /// Together these cover every parameter; all have Borrow mode.
    pub fn roles(&self) -> [usize; 4] {
        self.roles
    }
    /// Resolved symbols in the same order as roles (not distinct runtime owners).
    pub fn symbols(&self) -> [UnitSymbolId; 4] {
        self.symbols
    }
    /// Original initializer, while condition, branch condition and decrement.
    pub fn controls(&self) -> [UnitExpressionId; 4] {
        self.controls
    }
    /// Original local counter symbol.
    pub fn counter(&self) -> UnitSymbolId {
        self.counter
    }
}

pub(super) fn collect(
    fact: &TwoSourceCounterFact,
    callable: DeclarationId,
    inputs: &[SourceUnitInput<'_>],
    names: &ValidatedCompilationUnitNames,
    typed: &ValidatedCompilationUnitTypes,
) -> Option<FunctionShell> {
    let declaration = names
        .names()
        .index()
        .declarations()
        .iter()
        .find(|d| d.id() == callable)?;
    let unit = declaration.source_unit();
    if fact.arms[0].callable().source_unit() != unit
        || fact.arms[0].callable().item() != declaration.root()
    {
        return None;
    }
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
    let signature = typed
        .types()
        .signatures()
        .declaration(callable)?
        .callable()?;
    if signature.receiver().is_some()
        || !signature.type_parameters().is_empty()
        || signature.borrow_return().is_some()
        || signature.parameters().len() != 4
        || typed.types().types().get(signature.return_type())
            != Some(&UnitTypeKind::Builtin(BuiltinType::Unit))
    {
        return None;
    }
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
    let node = parsed.ast().statements().get(body).ok()?;
    let Statement::Block { elements } = node.payload() else {
        return None;
    };
    let expected = [fact.initial_statement, fact.arms[0].loop_statement()];
    if elements.len() != 2
        || elements
            .iter()
            .zip(expected)
            .any(|(id, expected)| expected.source_unit() != unit || *id != expected.statement())
    {
        return None;
    }
    let Expression::If { condition, .. } = parsed
        .ast()
        .expressions()
        .get(fact.branch.expression())
        .ok()?
        .payload()
    else {
        return None;
    };
    let condition_node = parsed.ast().expressions().get(*condition).ok()?;
    if !matches!(condition_node.payload(), Expression::Name) {
        return None;
    }
    let mut references = names
        .names()
        .references()
        .iter()
        .filter(|r| r.source_unit() == unit && r.span() == condition_node.span());
    let UnitReferenceTarget::Symbol(choose) = references.next()?.target() else {
        return None;
    };
    if references.next().is_some() {
        return None;
    }
    let symbols = [
        fact.arms[0].source(),
        fact.arms[1].source(),
        fact.rounds,
        *choose,
    ];
    let types = [
        BuiltinType::String,
        BuiltinType::String,
        BuiltinType::Int,
        BuiltinType::Boolean,
    ];
    let mut roles = [0; 4];
    for (index, (symbol, ty)) in symbols.iter().zip(types).enumerate() {
        if symbols[..index].contains(symbol) {
            return None;
        }
        let mut matches = signature
            .parameters()
            .iter()
            .enumerate()
            .filter(|(_, p)| p.symbol() == Some(*symbol));
        let (position, parameter) = matches.next()?;
        if matches.next().is_some()
            || parameter.mode() != ParameterMode::Borrow
            || typed.types().types().get(parameter.ty()) != Some(&UnitTypeKind::Builtin(ty))
        {
            return None;
        }
        roles[index] = position;
    }
    Some(FunctionShell {
        callable,
        body: node.span(),
        roles,
        symbols,
        controls: [
            fact.initializer,
            fact.condition,
            UnitExpressionId::new(unit, *condition),
            fact.decrement,
        ],
        counter: fact.counter,
    })
}
