//! Exact identity borrow-return certificate from original inputs and successful emission.
//! This is an observation, not permission to enter any lowering route.
use super::{RefineBody, RefineInputs, RefineTypes};
use crate::ssa::{
    lowering_support::callable_instances::SourceToken,
    model::{
        Definition, EntityId, EntityType, Function, FunctionId, LoanId, LoanKind, Origin, Program,
        SsaTypeId, SsaTypeKind, TerminatorKind,
    },
    unit_plan::UnitFunctionInstanceKey,
};
use lang_frontend::{
    ownership_checking::UnitLoanTarget,
    parser::Expression,
    type_checking::{
        BorrowReturnOrigin, BuiltinType, ParameterMode, UnitCallableTarget, UnitExpressionId,
        UnitItemId, UnitTypeKind,
    },
};

#[derive(Clone, Debug)]
pub(crate) struct IdentityReturnReceipt {
    callable: UnitItemId,
    key: UnitFunctionInstanceKey,
    source_token: SourceToken,
    expression: UnitExpressionId,
    loan: LoanId,
    target: SsaTypeId,
    snapshot: Function,
}
impl IdentityReturnReceipt {
    pub(crate) fn callable(&self) -> UnitItemId {
        self.callable
    }
    pub(crate) fn key(&self) -> &UnitFunctionInstanceKey {
        &self.key
    }
    pub(crate) fn source_token(&self) -> SourceToken {
        self.source_token
    }
    pub(crate) fn expression(&self) -> UnitExpressionId {
        self.expression
    }
    pub(crate) fn loan(&self) -> LoanId {
        self.loan
    }
    pub(crate) fn function(&self) -> FunctionId {
        self.snapshot.id
    }
    /// Revalidate the complete final definition and its type table, not just the exit.
    pub(crate) fn matches_final(&self, program: &Program) -> bool {
        let functions: Vec<_> = program
            .modules
            .iter()
            .flat_map(|m| &m.functions)
            .filter(|f| f.id == self.snapshot.id)
            .collect();
        let modules: Vec<_> = program
            .modules
            .iter()
            .filter(|m| m.id == self.target.module)
            .collect();
        matches!(functions.as_slice(), [f] if **f == self.snapshot)
            && matches!(modules.as_slice(), [m] if m.types.get(self.target.index) == Some(&SsaTypeKind::StringOwner))
    }
}
/// Called only after the original emitter succeeds. No reconstructed AST lowering or loans.
pub(crate) fn certify(
    inputs: &RefineInputs<'_>,
    function: &Function,
    types: &RefineTypes<'_>,
) -> Option<IdentityReturnReceipt> {
    let RefineBody::Expression(raw) = inputs.body else {
        return None;
    };
    let expression = UnitExpressionId::new(inputs.source_unit, raw);
    let node = inputs.parsed.ast().expressions().get(raw).ok()?;
    let [root] = inputs.parameters.as_slice() else {
        return None;
    };
    if !matches!(node.payload(), Expression::Name)
        || inputs
            .references
            .get(&(node.span().start(), node.span().end()))
            != Some(root)
        || inputs.modes != [ParameterMode::Borrow]
        || inputs.receiver.is_some()
        || inputs.static_self.is_some()
        || !inputs.substitutions.is_empty()
        || !inputs.callables.is_empty()
        || inputs.borrow_return != Some(0)
        || inputs.carrier_return.is_some()
        || inputs.constant_owned.is_some()
        || inputs.typed.symbol_type(*root) != Some(inputs.return_type)
        || inputs.typed.expression_type(expression) != Some(inputs.return_type)
        || inputs.typed.types().get(inputs.return_type)
            != Some(&UnitTypeKind::Builtin(BuiltinType::String))
    {
        return None;
    }
    let callable = UnitItemId::new(inputs.source_unit, inputs.function_item?);
    let key = inputs.source_plan.source(inputs.source_token)?;
    let UnitCallableTarget::Declaration(declaration) = key.target() else {
        return None;
    };
    if *key != UnitFunctionInstanceKey::for_entry(declaration)
        || inputs.function_ids.get(key) != Some(&function.id)
    {
        return None;
    }
    let declaration = inputs
        .names
        .names()
        .index()
        .declarations()
        .iter()
        .find(|d| d.id() == declaration)?;
    if UnitItemId::new(declaration.source_unit(), declaration.root()) != callable {
        return None;
    }
    let signature = inputs
        .typed
        .signatures()
        .declaration(declaration.id())?
        .callable()?;
    let [parameter] = signature.parameters() else {
        return None;
    };
    let contract = signature.borrow_return()?;
    if !signature.type_parameters().is_empty()
        || signature.receiver().is_some()
        || parameter.symbol() != Some(*root)
        || parameter.mode() != ParameterMode::Borrow
        || parameter.ty() != inputs.return_type
        || signature.return_type() != inputs.return_type
        || contract.origin() != BorrowReturnOrigin::Parameter(0)
    {
        return None;
    }
    let origins: Vec<_> = inputs
        .owned
        .borrow_return_origins()
        .iter()
        .filter(|fact| fact.expression() == expression)
        .collect();
    let [origin] = origins.as_slice() else {
        return None;
    };
    if origin.declaration_span() != contract.marker_span()
        || !matches!(origin.origin(), UnitLoanTarget::Place(p) if p.is_root() && p.root() == *root)
    {
        return None;
    }
    let target = *types.type_ids.get(&inputs.return_type)?;
    if types.ssa_types.get(target.index) != Some(&SsaTypeKind::StringOwner)
        || target.module != function.id.module
        || function.origin != Origin::Source(inputs.span)
        || function.borrow_return != Some(0)
        || function.carrier_return.is_some()
        || function.receiver.is_some()
        || function.never_returns
        || function.return_types != [target]
        || !function.instructions.is_empty()
        || !function.values.is_empty()
        || !function.places.is_empty()
        || function.loans.len() != 1
    {
        return None;
    }
    let [block] = function.blocks.as_slice() else {
        return None;
    };
    let [EntityId::Loan(loan)] = block.parameters.as_slice() else {
        return None;
    };
    if function.entry_block() != Some(block.id)
        || function.block(block.id) != Some(block)
        || !block.instructions.is_empty()
        || block.origin != Origin::Source(inputs.span)
    {
        return None;
    }
    let entity = function.entity(EntityId::Loan(*loan))?;
    if entity.ty
        != (EntityType::Loan {
            kind: LoanKind::Shared,
            target,
        })
        || entity.definition
            != (Definition::BlockParameter {
                block: block.id,
                index: 0,
            })
        || entity.origin != Origin::Source(inputs.span)
    {
        return None;
    }
    let terminator = block.terminator.as_ref()?;
    if terminator.abort.is_some()
        || terminator.kind != (TerminatorKind::BorrowReturn { loan: *loan })
        || terminator.origin != Origin::Source(inputs.span)
    {
        return None;
    }
    Some(IdentityReturnReceipt {
        callable,
        key: key.clone(),
        source_token: inputs.source_token,
        expression,
        loan: *loan,
        target,
        snapshot: function.clone(),
    })
}

#[cfg(test)]
thread_local! { static OBSERVED: std::cell::RefCell<Option<Vec<IdentityReturnReceipt>>> = const { std::cell::RefCell::new(None) }; }
#[cfg(test)]
pub(super) fn test_record(inputs: &RefineInputs<'_>, function: &Function, types: &RefineTypes<'_>) {
    OBSERVED.with(|slot| {
        if let Some(receipts) = slot.borrow_mut().as_mut()
            && let Some(receipt) = certify(inputs, function, types)
        {
            receipts.push(receipt);
        }
    });
}
#[cfg(test)]
mod tests;
