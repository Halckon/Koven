//! Restricted actual leaf bodies; no generic signatures or arbitrary ownership product escape.
use super::*;
use crate::{
    name_resolution::{SourceUnitInput, UnitReferenceTarget, ValidatedCompilationUnitNames},
    ownership_checking::{
        CompilationUnitOwnership, LoanKind, UnitCallArgumentOwnershipKind, UnitDropPoint,
        UnitLoanFact, UnitLoanTarget,
    },
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
    /// One bound println intrinsic reuses the single input Shared capability.
    PrintSharedParameter,
}
/// Opaque, fully checked leaf body for one concrete instance in the experiment.
#[derive(Clone, Debug)]
pub struct CalleeLeaf {
    identity: Rc<()>,
    instance: UnitCallableInstanceKey,
    body: CalleeLeafBody,
    span: Span,
    print_loan: Option<UnitLoanFact>,
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
    /// Original checked argument loan; its frontend call end does not end the SSA input loan.
    pub fn print_loan(&self) -> Option<&UnitLoanFact> {
        self.print_loan.as_ref()
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
    owned: &CompilationUnitOwnership,
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
                    if (leaf.body == CalleeLeafBody::SharedIdentity)
                        != (expected == CalleeLeafBody::SharedIdentity)
                    {
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
                let mut body = expected;
                let mut print_loan = None;
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
                        let Statement::Block { elements } = node.payload() else {
                            return None;
                        };
                        if !elements.is_empty() {
                            let [statement] = elements.as_slice() else {
                                return None;
                            };
                            let Statement::Expression { expression } =
                                parsed.ast().statements().get(*statement).ok()?.payload()
                            else {
                                return None;
                            };
                            let call_id =
                                UnitExpressionId::new(declaration.source_unit(), *expression);
                            let call_node = parsed.ast().expressions().get(*expression).ok()?;
                            let call = typed.types().call(call_id)?;
                            if !call.prints_line()
                                || call.aborts()
                                || call.receiver().is_some()
                                || !call.instance().type_arguments().is_empty()
                                || call.return_type() != signature.return_type()
                                || !owned.call_site_was_entered(call_id)
                            {
                                return None;
                            }
                            let [mapping] = call.arguments() else {
                                return None;
                            };
                            let mut contracts = owned.call_argument_contracts_for(call_id);
                            let contract = contracts.next()?;
                            if contracts.next().is_some()
                                || !owned.call_argument_completed(contract.argument())
                                || mapping.argument_index() != 0
                                || mapping.parameter_index() != 0
                                || mapping.parameter_type() != parameter.ty()
                                || mapping.mode() != ParameterMode::Borrow
                                || contract.parameter_index() != 0
                                || contract.parameter_type() != parameter.ty()
                                || contract.kind() != UnitCallArgumentOwnershipKind::SharedLoan
                                || contract.crosses_thread()
                                || contract.call_span() != call_node.span()
                            {
                                return None;
                            }
                            let argument = parsed
                                .ast()
                                .expressions()
                                .get(contract.argument().expression())
                                .ok()?;
                            if !matches!(argument.payload(), Expression::Name) {
                                return None;
                            }
                            let symbol = parameter.symbol()?;
                            let mut references = names.names().references().iter().filter(|r| {
                                r.source_unit() == declaration.source_unit()
                                    && r.span() == argument.span()
                            });
                            if references.next()?.target() != &UnitReferenceTarget::Symbol(symbol)
                                || references.next().is_some()
                            {
                                return None;
                            }
                            let mut loans = owned.loans().iter().filter(|l| l.call() == call_id);
                            let loan = loans.next()?;
                            if loans.next().is_some()
                                || loan.argument() != contract.argument()
                                || loan.kind() != LoanKind::Shared
                                || loan.begin_span() != argument.span()
                                || loan.end_span() != call_node.span()
                                || !matches!(loan.target(), UnitLoanTarget::Place(p) if p.is_root() && p.root()==symbol)
                                || owned
                                    .direct_call_loan_ends()
                                    .iter()
                                    .any(|e| e.loan().call() == call_id)
                                || owned
                                    .drops()
                                    .iter()
                                    .any(|d| d.point() == UnitDropPoint::CallReturn(call_id))
                            {
                                return None;
                            }
                            body = CalleeLeafBody::PrintSharedParameter;
                            print_loan = Some(loan.clone());
                        }
                        node.span()
                    }
                    _ => return None,
                };
                leaves.push(CalleeLeaf {
                    identity: Rc::new(()),
                    instance: key.clone(),
                    body,
                    span,
                    print_loan,
                });
            }
        }
    }
    Some(leaves)
}
