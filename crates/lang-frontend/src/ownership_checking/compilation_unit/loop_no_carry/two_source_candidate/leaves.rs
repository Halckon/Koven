//! Restricted actual leaf bodies; no generic signatures or arbitrary ownership product escape.
use super::*;
use crate::{
    name_resolution::{SourceUnitInput, UnitReferenceTarget, ValidatedCompilationUnitNames},
    parser::{Expression, FunctionBody, FunctionForm, Item, Statement},
    source::Span,
    type_checking::{
        BorrowReturnOrigin, BuiltinType, ParameterMode, UnitTypeKind, ValidatedCompilationUnitTypes,
    },
};

/// The complete actual body admitted by this experimental leaf subset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CalleeLeafBody {
    /// Return the single shared String parameter, with no other expression.
    SharedIdentity,
    /// An actual empty Unit block, not a placeholder for an unknown implementation.
    EmptyUnit,
}
/// Opaque, fully checked leaf body for one concrete instance in the experiment.
#[derive(Clone, Debug)]
pub struct CalleeLeaf {
    identity: Rc<()>,
    instance: UnitCallableInstanceKey,
    body: CalleeLeafBody,
    span: Span,
}
impl CalleeLeaf {
    /// Full concrete instance identity.
    pub fn instance(&self) -> &UnitCallableInstanceKey {
        &self.instance
    }
    /// Exact admitted body category.
    pub fn body(&self) -> CalleeLeafBody {
        self.body
    }
    /// Original body expression or block span.
    pub fn span(&self) -> Span {
        self.span
    }
    pub(super) fn same_identity(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.identity, &other.identity)
    }
}

pub(super) fn collect(
    facts: &[TwoSourceCounterFact],
    inputs: &[SourceUnitInput<'_>],
    names: &ValidatedCompilationUnitNames,
    typed: &ValidatedCompilationUnitTypes,
) -> Option<Vec<CalleeLeaf>> {
    let mut leaves: Vec<CalleeLeaf> = Vec::new();
    for fact in facts {
        for calls in &fact.calls {
            for (position, (_, key)) in calls.iter().enumerate() {
                let expected = if position == 0 {
                    CalleeLeafBody::SharedIdentity
                } else {
                    CalleeLeafBody::EmptyUnit
                };
                if let Some(leaf) = leaves.iter().find(|leaf| leaf.instance == *key) {
                    if leaf.body != expected {
                        return None;
                    }
                    continue;
                }
                if !key.type_arguments().is_empty() {
                    return None;
                }
                let UnitCallTarget::Declaration(id) = key.target() else {
                    return None;
                };
                let declaration = names
                    .names()
                    .index()
                    .declarations()
                    .iter()
                    .find(|d| d.id() == id)?;
                let source = names
                    .names()
                    .index()
                    .source_units()
                    .iter()
                    .find(|s| s.id() == declaration.source_unit())?;
                let parsed = inputs
                    .iter()
                    .find(|i| i.source_id() == source.source_id())?
                    .parsed();
                let signature = typed.types().signatures().declaration(id)?.callable()?;
                if signature.receiver().is_some() || !signature.type_parameters().is_empty() {
                    return None;
                }
                let [parameter] = signature.parameters() else {
                    return None;
                };
                if parameter.mode() != ParameterMode::Borrow
                    || typed.types().types().get(parameter.ty())
                        != Some(&UnitTypeKind::Builtin(BuiltinType::String))
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
                let span = match (expected, form) {
                    (
                        CalleeLeafBody::SharedIdentity,
                        FunctionForm::Explicit {
                            body: FunctionBody::Expression { expression, .. },
                            ..
                        },
                    ) => {
                        if signature.borrow_return()?.origin() != BorrowReturnOrigin::Parameter(0)
                            || signature.return_type() != parameter.ty()
                        {
                            return None;
                        }
                        let node = parsed.ast().expressions().get(*expression).ok()?;
                        if !matches!(node.payload(), Expression::Name) {
                            return None;
                        }
                        let symbol = parameter.symbol()?;
                        let mut references = names.names().references().iter().filter(|r| {
                            r.source_unit() == declaration.source_unit() && r.span() == node.span()
                        });
                        if references.next()?.target() != &UnitReferenceTarget::Symbol(symbol)
                            || references.next().is_some()
                        {
                            return None;
                        }
                        node.span()
                    }
                    (
                        CalleeLeafBody::EmptyUnit,
                        FunctionForm::Explicit {
                            body: FunctionBody::Block(block),
                            ..
                        }
                        | FunctionForm::ImplicitUnitBlock(block),
                    ) => {
                        if signature.borrow_return().is_some()
                            || typed.types().types().get(signature.return_type())
                                != Some(&UnitTypeKind::Builtin(BuiltinType::Unit))
                        {
                            return None;
                        }
                        let node = parsed.ast().statements().get(*block).ok()?;
                        if !matches!(node.payload(), Statement::Block { elements } if elements.is_empty())
                        {
                            return None;
                        }
                        node.span()
                    }
                    _ => return None,
                };
                leaves.push(CalleeLeaf {
                    identity: Rc::new(()),
                    instance: key.clone(),
                    body: expected,
                    span,
                });
            }
        }
    }
    Some(leaves)
}
