//! Bounded actual entry BorrowCall consumption, deliberately not a binding-lifetime proof.
use super::{RefineInputs, body::Slot};
use crate::ssa::{
    model::{EntityId, Function, FunctionId, Instruction, InstructionId, Operation},
    unit_plan::UnitFunctionInstanceKey,
};
use lang_frontend::{
    ownership_checking::BorrowSourceLoan,
    parser::Expression,
    type_checking::{
        BorrowReturnOrigin, BuiltinType, ExpressionCategory, ParameterMode, UnitCallTarget,
        UnitCallableTarget, UnitExpressionId, UnitItemId, UnitTypeKind,
    },
};
use std::collections::BTreeMap;
#[derive(Clone, Debug)]
pub(super) struct Pending {
    pub(crate) call: UnitExpressionId,
    pub(crate) callable: UnitItemId,
    pub(crate) caller: FunctionId,
    pub(crate) callee: FunctionId,
    pub(crate) key: UnitFunctionInstanceKey,
    pub(crate) callee_key: UnitFunctionInstanceKey,
    pub(super) lifecycle: Option<super::entry_lifecycle::Pending>,
    arguments: Vec<Slot>,
    source: Slot,
    result: Option<Slot>,
}
impl Pending {
    pub(super) fn new(
        inputs: &RefineInputs<'_>,
        call: UnitExpressionId,
        source: BorrowSourceLoan<UnitExpressionId>,
        callee: FunctionId,
        arguments: &[Slot],
        source_slot: Slot,
    ) -> Option<Self> {
        let callable = UnitItemId::new(inputs.source_unit, inputs.function_item?);
        if inputs.owned.d10_entry_borrow_link(call) != Some(callable)
            || inputs.receiver.is_some()
            || inputs.static_self.is_some()
            || !inputs.substitutions.is_empty()
            || inputs.borrow_return.is_some()
            || inputs.carrier_return.is_some()
            || inputs.parameters.len() != 1
            || inputs.modes != [ParameterMode::Borrow]
            || source.call() != call
            || arguments.len() != 2
            || arguments[0] != source_slot
            || arguments[0] == arguments[1]
        {
            return None;
        }
        let builtin =
            |ty, expected| inputs.typed.types().get(ty) == Some(&UnitTypeKind::Builtin(expected));
        if !builtin(inputs.return_type, BuiltinType::Unit)
            || !builtin(
                inputs.typed.symbol_type(inputs.parameters[0])?,
                BuiltinType::Boolean,
            )
        {
            return None;
        }
        let descriptor = inputs.typed.call(call)?;
        let UnitCallTarget::Declaration(target) = descriptor.target() else {
            return None;
        };
        if descriptor.receiver().is_some()
            || descriptor.aborts()
            || !descriptor.instance().type_arguments().is_empty()
            || descriptor.borrow_return()?.origin() != BorrowReturnOrigin::Parameter(0)
            || !builtin(descriptor.return_type(), BuiltinType::String)
            || descriptor.arguments().len() != 2
        {
            return None;
        }
        for (index, argument) in descriptor.arguments().iter().enumerate() {
            if argument.mode() != ParameterMode::Borrow
                || argument.argument_index() != index
                || argument.parameter_index() != index
                || argument.category() != ExpressionCategory::Place
                || argument.crosses_thread()
                || !builtin(
                    argument.parameter_type(),
                    if index == 0 {
                        BuiltinType::String
                    } else {
                        BuiltinType::Boolean
                    },
                )
            {
                return None;
            }
        }
        let handoffs: Vec<_> = inputs
            .owned
            .borrow_call_handoffs()
            .iter()
            .filter(|h| h.source_loan().call() == call)
            .collect();
        let [handoff] = handoffs.as_slice() else {
            return None;
        };
        if handoff.source_loan().argument() != source.argument() {
            return None;
        }
        let call_node = inputs
            .parsed
            .ast()
            .expressions()
            .get(call.expression())
            .ok()?;
        let Expression::Call { arguments: raw, .. } = call_node.payload() else {
            return None;
        };
        let [owner_arg, flag_arg] = raw.as_slice() else {
            return None;
        };
        if source.argument() != UnitExpressionId::new(inputs.source_unit, owner_arg.value) {
            return None;
        }
        let flag_node = inputs.parsed.ast().expressions().get(flag_arg.value).ok()?;
        if !matches!(flag_node.payload(), Expression::Name)
            || inputs
                .references
                .get(&(flag_node.span().start(), flag_node.span().end()))
                != Some(&inputs.parameters[0])
        {
            return None;
        }
        let node = inputs
            .parsed
            .ast()
            .expressions()
            .get(source.argument().expression())
            .ok()?;
        if !matches!(node.payload(), Expression::Name) {
            return None;
        }
        let key = inputs.source_plan.source(inputs.source_token)?.clone();
        let UnitCallableTarget::Declaration(declaration) = key.target() else {
            return None;
        };
        let declared = inputs
            .names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|d| d.id() == declaration)?;
        if UnitItemId::new(declared.source_unit(), declared.root()) != callable
            || key != UnitFunctionInstanceKey::for_entry(declaration)
        {
            return None;
        }
        let site = inputs.source_plan.call_site(inputs.source_token, call)?;
        let callee_key = site.dispatch_key()?.clone();
        if !site.delegation().is_empty()
            || callee_key != UnitFunctionInstanceKey::for_entry(target)
            || inputs.function_ids.get(&callee_key) != Some(&callee)
        {
            return None;
        }
        Some(Self {
            call,
            callable,
            caller: *inputs.function_ids.get(&key)?,
            callee,
            key,
            callee_key,
            lifecycle: super::entry_lifecycle::Pending::new(inputs, call, callable),
            arguments: arguments.to_vec(),
            source: source_slot,
            result: None,
        })
    }
    pub(super) fn with_result(mut self, result: Slot) -> Self {
        self.result = Some(result);
        self
    }
    pub(super) fn emitted(
        &self,
        function: &Function,
        instruction: InstructionId,
        current: &BTreeMap<Slot, EntityId>,
    ) -> Option<Receipt> {
        let result = *current.get(&self.result?)?;
        let EntityId::Loan(source) = *current.get(&self.source)? else {
            return None;
        };
        let args = self
            .arguments
            .iter()
            .map(|slot| current.get(slot).copied())
            .collect::<Option<Vec<_>>>()?;
        let data = function.instruction(instruction)?;
        if function.id != self.caller
            || data.results != [result]
            || !matches!(result, EntityId::Loan(_))
            || data.operation
                != (Operation::BorrowCall {
                    callee: self.callee,
                    arguments: args,
                    source,
                })
        {
            return None;
        }
        Some(Receipt {
            pending: self.clone(),
            instruction: data.clone(),
            result,
            result_data: function.entity(result)?.clone(),
        })
    }
    pub(super) fn matches_result(&self, results: &[Slot]) -> bool {
        self.result.is_some_and(|r| results == [r])
    }
}
#[derive(Clone)]
pub(crate) struct Receipt {
    pub(super) pending: Pending,
    pub(crate) instruction: Instruction,
    result: EntityId,
    result_data: crate::ssa::model::EntityData,
}
impl Receipt {
    pub(crate) fn call(&self) -> UnitExpressionId {
        self.pending.call
    }
    pub(crate) fn callable(&self) -> UnitItemId {
        self.pending.callable
    }
    pub(crate) fn caller(&self) -> FunctionId {
        self.pending.caller
    }
    pub(crate) fn callee(&self) -> FunctionId {
        self.pending.callee
    }
    pub(crate) fn keys(&self) -> (&UnitFunctionInstanceKey, &UnitFunctionInstanceKey) {
        (&self.pending.key, &self.pending.callee_key)
    }
    pub(crate) fn matches_final(&self, function: &Function) -> bool {
        function.id == self.caller()
            && function.instruction(self.instruction.id) == Some(&self.instruction)
            && function.entity(self.result) == Some(&self.result_data)
            && function.block(self.instruction.block).is_some_and(|b| {
                b.instructions
                    .iter()
                    .filter(|id| **id == self.instruction.id)
                    .count()
                    == 1
            })
    }
}
