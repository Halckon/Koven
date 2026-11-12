//! Closure-specific linear ownership effects.

use std::collections::{BTreeMap, BTreeSet};

mod root_storage {
    use super::*;
    use crate::ssa::model::InstructionId;

    pub(super) fn take_sources(function: &Function) -> BTreeMap<InstructionId, ValueId> {
        let mut sources = BTreeMap::new();
        for block in &function.blocks {
            let mut writes = BTreeMap::new();
            for id in &block.instructions {
                let instruction = function.instruction(*id).expect("verified instruction");
                match instruction.operation {
                    Operation::Mutate { place, value } => {
                        writes.insert(place, value);
                    }
                    // The operation contract requires the direct RootPlace result, and
                    // linear live-ins forbid using that result across a block boundary.
                    Operation::RootPlaceTake { owner, place } => {
                        sources.insert(*id, writes.get(&place).copied().unwrap_or(owner));
                    }
                    _ => {}
                }
            }
        }
        sources
    }
}

use super::{
    AliasRoots, BlockState, ReborrowDependencies, consume_value, edges, error,
    has_exclusive_value_loan, is_move_only, require_value,
};
use crate::ssa::{
    model::{
        ClosureCaptureOperand, Edge, EntityId, Function, LoanId, Module, Operation, Origin, ValueId,
    },
    verify::{VerifyError, VerifyErrorKind, VerifyLocation},
};

pub(super) struct ClosureLoans {
    by_owner: BTreeMap<ValueId, BTreeSet<LoanId>>,
    take_sources: BTreeMap<crate::ssa::model::InstructionId, ValueId>,
    owners_by_loan: BTreeMap<LoanId, Vec<ValueId>>,
    borrowed_contents: BTreeMap<
        (crate::ssa::model::InstructionId, EntityId),
        crate::ssa::verify::closure_content::CallableContent,
    >,
}

impl ClosureLoans {
    pub(super) fn compute(
        module: &Module,
        function: &Function,
        capture_types: &crate::ssa::verify::TypeCaptures,
        aliases: &AliasRoots,
        trusted_view_fields: Option<&BTreeSet<usize>>,
    ) -> Self {
        let mut by_owner = BTreeMap::<ValueId, BTreeSet<LoanId>>::new();
        let mut owner_moves = Vec::new();
        let take_sources = root_storage::take_sources(function);
        for instruction in &function.instructions {
            let result = |index| match instruction.results[index] {
                EntityId::Value(value) => value,
                _ => unreachable!("verified operation returns an owner"),
            };
            match &instruction.operation {
                Operation::ClosureConstruct { captures, .. } => {
                    let owner = result(0);
                    for capture in captures {
                        match capture {
                            ClosureCaptureOperand::Shared(loan) => {
                                by_owner.entry(owner).or_default().insert(*loan);
                            }
                            ClosureCaptureOperand::Owned(value) => {
                                owner_moves.push((*value, owner))
                            }
                        }
                    }
                }
                Operation::RootPlaceTake { .. } => {
                    owner_moves.push((take_sources[&instruction.id], result(0)))
                }
                _ => {}
            }
        }
        loop {
            let mut changed = false;
            // Owned slots and root moves can receive CFG parameters, and their results
            // can in turn be passed along edges.
            for &(source, destination) in &owner_moves {
                if let Some(dependencies) = by_owner.get(&source).cloned() {
                    let entry = by_owner.entry(destination).or_default();
                    let before = entry.len();
                    entry.extend(dependencies);
                    changed |= entry.len() != before;
                }
            }
            for block in &function.blocks {
                let terminator = block.terminator.as_ref().expect("terminator must exist");
                for edge in edges(&terminator.kind) {
                    let target = function.block(edge.target).expect("target must exist");
                    let loan_rebindings = edge
                        .arguments
                        .iter()
                        .zip(&target.parameters)
                        .filter_map(|(argument, parameter)| match (argument, parameter) {
                            (EntityId::Loan(argument), EntityId::Loan(parameter)) => {
                                Some((*argument, *parameter))
                            }
                            _ => None,
                        })
                        .collect::<BTreeMap<_, _>>();
                    for (argument, parameter) in edge.arguments.iter().zip(&target.parameters) {
                        let (EntityId::Value(argument), EntityId::Value(parameter)) =
                            (argument, parameter)
                        else {
                            continue;
                        };
                        let dependencies = by_owner
                            .get(argument)
                            .into_iter()
                            .flatten()
                            .map(|loan| loan_rebindings.get(loan).copied().unwrap_or(*loan))
                            .collect::<BTreeSet<_>>();
                        if dependencies.is_empty() {
                            continue;
                        }
                        let entry = by_owner.entry(*parameter).or_default();
                        let before = entry.len();
                        entry.extend(dependencies);
                        changed |= entry.len() != before;
                    }
                }
            }
            if !changed {
                break;
            }
        }
        let mut owners_by_loan = BTreeMap::<_, Vec<_>>::new();
        for (&owner, loans) in &by_owner {
            for &loan in loans {
                owners_by_loan.entry(loan).or_default().push(owner);
            }
        }
        Self {
            by_owner,
            take_sources,
            owners_by_loan,
            borrowed_contents: crate::ssa::verify::closure_content::borrowed_callable_contents(
                module,
                function,
                capture_types,
                aliases,
                trusted_view_fields,
            ),
        }
    }

    pub(super) fn activate_entry(&self, owner: ValueId, state: &mut BlockState) {
        state.loans.extend(self.dependencies(owner).iter().copied());
    }

    pub(super) fn release(&self, owner: ValueId, state: &mut BlockState) {
        for loan in self.dependencies(owner) {
            state.loans.remove(loan);
        }
    }

    pub(super) fn live_owner_holding(&self, loan: LoanId, state: &BlockState) -> Option<ValueId> {
        self.owners_by_loan
            .get(&loan)?
            .iter()
            .copied()
            .find(|owner| state.values.contains(owner))
    }

    pub(super) fn dependencies(&self, owner: ValueId) -> &BTreeSet<LoanId> {
        self.by_owner.get(&owner).unwrap_or(&EMPTY_LOANS)
    }

    pub(super) fn root_take_ready(
        &self,
        instruction: &crate::ssa::model::Instruction,
        owner: ValueId,
        state: &BlockState,
        errors: &mut Vec<VerifyError>,
    ) -> bool {
        let source = self.take_sources[&instruction.id];
        let captures = self.dependencies(source);
        let kind = if source != owner && !captures.is_disjoint(self.dependencies(owner)) {
            // Equal or overlapping unions cannot prove which loans the replaced
            // and currently stored values own on each incoming path.
            VerifyErrorKind::OperationContract {
                reason: "root take has ambiguous capture loans after a raw place write",
            }
        } else if let Some(&loan) = captures.iter().find(|loan| !state.loans.contains(loan)) {
            VerifyErrorKind::LoanInactive { loan }
        } else if source != owner
            && let Some(holder) = captures
                .iter()
                .find_map(|loan| self.live_owner_holding(*loan, state))
        {
            VerifyErrorKind::OwnerLoanConflict { value: holder }
        } else {
            return true;
        };
        errors.push(error(
            kind,
            VerifyLocation::Instruction(instruction.id),
            &instruction.origin,
        ));
        false
    }

    pub(super) fn edge_loans(
        &self,
        function: &Function,
        edge: &Edge,
        location: &VerifyLocation,
        origin: &Origin,
        errors: &mut Vec<VerifyError>,
    ) -> BTreeSet<LoanId> {
        let target = function.block(edge.target).expect("target must exist");
        let bindings = edge.arguments.iter().zip(&target.parameters);
        let loan_bindings =
            bindings
                .clone()
                .filter_map(|(source, target)| match (source, target) {
                    (EntityId::Loan(source), EntityId::Loan(target)) => Some((*target, *source)),
                    _ => None,
                });
        let mut transferring = loan_bindings
            .clone()
            .map(|(_, source)| source)
            .collect::<BTreeSet<_>>();
        if self.by_owner.is_empty() {
            return transferring;
        }
        let loan_sources = loan_bindings.collect::<BTreeMap<_, _>>();
        for (source, target) in bindings {
            let (EntityId::Value(source), EntityId::Value(target)) = (source, target) else {
                continue;
            };
            let captures = self.dependencies(*source);
            transferring.extend(captures);
            // A joined owner may hide different capture IDs on different paths. An explicit
            // loan parameter must agree with the actual owner on every incoming edge.
            if self.dependencies(*target).iter().any(|loan| {
                loan_sources
                    .get(loan)
                    .is_some_and(|source| !captures.contains(source))
            }) {
                errors.push(error(
                    VerifyErrorKind::OwnerLoanConflict { value: *source },
                    location.clone(),
                    origin,
                ));
            }
        }
        transferring
    }
}

static EMPTY_LOANS: BTreeSet<LoanId> = BTreeSet::new();

#[allow(clippy::too_many_arguments)]
pub(super) fn apply_construct(
    module: &Module,
    function: &Function,
    captures: &[ClosureCaptureOperand],
    aliases: &AliasRoots,
    closure_loans: &ClosureLoans,
    state: &mut BlockState,
    location: VerifyLocation,
    origin: &Origin,
    errors: &mut Vec<VerifyError>,
) {
    for capture in captures {
        match capture {
            ClosureCaptureOperand::Owned(value) => {
                consume_value(
                    module,
                    function,
                    *value,
                    aliases,
                    state,
                    &BTreeSet::new(),
                    &BTreeSet::new(),
                    location.clone(),
                    origin,
                    errors,
                );
            }
            ClosureCaptureOperand::Shared(loan) => {
                let VerifyLocation::Instruction(instruction) = location else {
                    unreachable!("closure formation is an instruction");
                };
                check_borrowed_contents(
                    *loan,
                    closure_loans,
                    state,
                    instruction,
                    origin,
                    errors,
                    "capturing a callable view requires proved current capture contents",
                );
                if !state.loans.contains(loan) {
                    errors.push(error(
                        VerifyErrorKind::LoanInactive { loan: *loan },
                        location.clone(),
                        origin,
                    ));
                } else if let Some(owner) = closure_loans.live_owner_holding(*loan, state) {
                    errors.push(error(
                        VerifyErrorKind::OwnerLoanConflict { value: owner },
                        location.clone(),
                        origin,
                    ));
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn apply_drop(
    module: &Module,
    function: &Function,
    owner: ValueId,
    aliases: &AliasRoots,
    closure_loans: &ClosureLoans,
    reborrows: &ReborrowDependencies,
    state: &mut BlockState,
    location: VerifyLocation,
    origin: &Origin,
    errors: &mut Vec<VerifyError>,
) {
    if !is_move_only(module, function, owner) {
        errors.push(error(
            VerifyErrorKind::DropCopyable { value: owner },
            location,
            origin,
        ));
        return;
    }
    if !consume_value(
        module,
        function,
        owner,
        aliases,
        state,
        &BTreeSet::new(),
        &BTreeSet::new(),
        location.clone(),
        origin,
        errors,
    ) {
        return;
    }
    let captures = closure_loans.dependencies(owner);
    // The environment ends its entire capture set. Reborrows surviving that
    // cleanup must still have their parent; CFG transfers do not end any loan.
    let surviving = state
        .loans
        .iter()
        .copied()
        .filter(|loan| !captures.contains(loan));
    for &parent in captures {
        // An ambiguous join can make two live owners claim the same loan even
        // when the original constructions used independent loans.
        if let Some(holder) = closure_loans.live_owner_holding(parent, state) {
            errors.push(error(
                VerifyErrorKind::OwnerLoanConflict { value: holder },
                location,
                origin,
            ));
            return;
        }
        if let Some(dependent) = reborrows.active_descendant_among(parent, surviving.clone()) {
            errors.push(error(
                VerifyErrorKind::LoanDependencyActive { parent, dependent },
                location,
                origin,
            ));
            return;
        }
    }
    closure_loans.release(owner, state);
}

#[allow(clippy::too_many_arguments)]
pub(super) fn apply_invoke(
    module: &Module,
    function: &Function,
    callable: ValueId,
    arguments: &[EntityId],
    aliases: &AliasRoots,
    closure_loans: &ClosureLoans,
    state: &mut BlockState,
    location: VerifyLocation,
    origin: &Origin,
    errors: &mut Vec<VerifyError>,
) {
    if require_value(
        module,
        function,
        callable,
        state,
        location.clone(),
        origin,
        errors,
    ) && has_exclusive_value_loan(function, callable, aliases, state)
    {
        errors.push(error(
            VerifyErrorKind::OwnerLoanConflict { value: callable },
            location.clone(),
            origin,
        ));
    }
    let VerifyLocation::Instruction(instruction) = location else {
        unreachable!("callable invocation is an instruction");
    };
    check_callable_contents(
        EntityId::Value(callable),
        closure_loans,
        state,
        instruction,
        origin,
        errors,
        None,
    );
    invoke_arguments(
        module,
        function,
        arguments,
        aliases,
        closure_loans,
        state,
        location,
        origin,
        errors,
    );
}

#[allow(clippy::too_many_arguments)]
fn invoke_arguments(
    module: &Module,
    function: &Function,
    arguments: &[EntityId],
    aliases: &AliasRoots,
    closure_loans: &ClosureLoans,
    state: &mut BlockState,
    location: VerifyLocation,
    origin: &Origin,
    errors: &mut Vec<VerifyError>,
) {
    for argument in arguments {
        match argument {
            EntityId::Value(value) => {
                consume_value(
                    module,
                    function,
                    *value,
                    aliases,
                    state,
                    &BTreeSet::new(),
                    &BTreeSet::new(),
                    location.clone(),
                    origin,
                    errors,
                );
            }
            EntityId::Loan(loan) => {
                if !state.loans.contains(loan) {
                    errors.push(error(
                        VerifyErrorKind::LoanInactive { loan: *loan },
                        location.clone(),
                        origin,
                    ));
                }
                let VerifyLocation::Instruction(instruction) = location else {
                    unreachable!("callable invocation is an instruction");
                };
                check_borrowed_contents(
                    *loan,
                    closure_loans,
                    state,
                    instruction,
                    origin,
                    errors,
                    "helper Borrow requires proved current callable capture contents",
                );
            }
            EntityId::Place(place) => {
                super::require_place(*place, state, location.clone(), origin, errors);
            }
        }
    }
}

/// ADR-0034: a call through a callable view reads the borrowed callable; the view loan and
/// every loan argument must be active, and value arguments are consumed.
#[allow(clippy::too_many_arguments)]
pub(super) fn apply_invoke_view(
    module: &Module,
    function: &Function,
    view: LoanId,
    arguments: &[EntityId],
    aliases: &AliasRoots,
    closure_loans: &ClosureLoans,
    state: &mut BlockState,
    location: VerifyLocation,
    origin: &Origin,
    errors: &mut Vec<VerifyError>,
) {
    if !state.loans.contains(&view) {
        errors.push(error(
            VerifyErrorKind::LoanInactive { loan: view },
            location.clone(),
            origin,
        ));
    }
    let VerifyLocation::Instruction(instruction) = location else {
        unreachable!("view invocation is an instruction");
    };
    check_borrowed_contents(
        view,
        closure_loans,
        state,
        instruction,
        origin,
        errors,
        "callable view requires proved current capture contents",
    );
    invoke_arguments(
        module,
        function,
        arguments,
        aliases,
        closure_loans,
        state,
        location,
        origin,
        errors,
    );
}

/// Borrowed generation reads the callable storage and retains every capture dependency.
#[allow(clippy::too_many_arguments)]
pub(super) fn apply_borrowed_generate(
    initializer: LoanId,
    closure_loans: &ClosureLoans,
    state: &BlockState,
    location: VerifyLocation,
    origin: &Origin,
    errors: &mut Vec<VerifyError>,
) {
    if !state.loans.contains(&initializer) {
        errors.push(error(
            VerifyErrorKind::LoanInactive { loan: initializer },
            location.clone(),
            origin,
        ));
    }
    let VerifyLocation::Instruction(instruction) = location else {
        unreachable!("borrowed generation is an instruction");
    };
    check_borrowed_contents(
        initializer,
        closure_loans,
        state,
        instruction,
        origin,
        errors,
        "borrowed generation requires proved current callable capture contents",
    );
}

/// Direct-call callable Borrow operands establish the contract trusted by the callee entry.
pub(super) fn check_borrowed_contents(
    initializer: LoanId,
    closure_loans: &ClosureLoans,
    state: &BlockState,
    instruction: crate::ssa::model::InstructionId,
    origin: &Origin,
    errors: &mut Vec<VerifyError>,
    reason: &'static str,
) {
    check_callable_contents(
        EntityId::Loan(initializer),
        closure_loans,
        state,
        instruction,
        origin,
        errors,
        Some(reason),
    );
}

fn check_callable_contents(
    callable: EntityId,
    closure_loans: &ClosureLoans,
    state: &BlockState,
    instruction: crate::ssa::model::InstructionId,
    origin: &Origin,
    errors: &mut Vec<VerifyError>,
    unknown_reason: Option<&'static str>,
) {
    let Some(content) = closure_loans
        .borrowed_contents
        .get(&(instruction, callable))
    else {
        return;
    };
    if !content.known
        && let Some(reason) = unknown_reason
    {
        errors.push(error(
            VerifyErrorKind::OperationContract { reason },
            VerifyLocation::Instruction(instruction),
            origin,
        ));
    }
    for dependency in &content.loans {
        if !state.loans.contains(dependency) {
            errors.push(error(
                VerifyErrorKind::LoanInactive { loan: *dependency },
                VerifyLocation::Instruction(instruction),
                origin,
            ));
        }
    }
}

#[cfg(test)]
mod map_require_tests {
    //! 正常源码生成的实际 Map slot loan 及纯合同查询，不改写 IR。
    use crate::ssa::{
        borrow_memory_test_support::single as memory_unit, borrow_storage_contract_tests::unit,
        model::*, verify::verify_program,
    };

    #[test]
    fn map_require_memory_and_rooted_verify_real_slot_and_source_end_order() {
        for (_, text, _) in crate::native_map_require_cases::cases() {
            for program in [memory_unit(text).unwrap(), unit(text).unwrap()] {
                verify_program(&program).unwrap();
                let module = &program.modules[0];
                let llvm = crate::llvm::render_verified_program(&program).unwrap();
                assert!(llvm.contains("required.missing"));
                assert!(llvm.contains("required.found"));
                let slot_pointers = llvm
                    .lines()
                    .filter(|line| {
                        line.contains(".required")
                            && line.contains(" = getelementptr")
                            && line.ends_with(", i32 0, i32 2")
                    })
                    .count();
                let queries = module
                    .functions
                    .iter()
                    .flat_map(|function| &function.instructions)
                    .filter(|i| matches!(i.operation, Operation::MapRequireValue { .. }))
                    .count();
                assert!(queries > 0);
                assert_eq!(
                    slot_pointers, queries,
                    "every query must return actual V field storage"
                );
                for function in &module.functions {
                    for (index, instruction) in function.instructions.iter().enumerate() {
                        let Operation::MapRequireValue { source, key } = instruction.operation else {
                            continue;
                        };
                        let [EntityId::Loan(result)] = instruction.results.as_slice() else {
                            unreachable!()
                        };
                        let result_type = function.entity(EntityId::Loan(*result)).unwrap().ty;
                        assert!(super::verify_map_operation(
                            module,
                            function,
                            &instruction.operation,
                            &[result_type]
                        ));
                        assert!(!super::verify_map_operation(
                            module,
                            function,
                            &instruction.operation,
                            &[EntityType::Value(result_type.semantic_type())]
                        ));
                        assert!(!super::verify_map_operation(
                            module,
                            function,
                            &Operation::MapRequireValue {
                                source: *result,
                                key
                            },
                            &[result_type]
                        ));
                        let result_end = function
                            .instructions
                            .iter()
                            .position(|i| {
                                matches!(i.operation,
             Operation::BorrowEnd { loan } if loan == *result)
                            })
                            .unwrap();
                        let source_end = function
                            .instructions
                            .iter()
                            .position(|i| {
                                matches!(i.operation,
             Operation::BorrowEnd { loan } if loan == source)
                            })
                            .unwrap();
                        assert!(index < result_end && result_end < source_end);
                        let source_type = function
                            .entity(EntityId::Loan(source))
                            .unwrap()
                            .ty
                            .semantic_type();
                        assert_eq!(
                            module.map_container(source_type).unwrap().2,
                            result_type.semantic_type()
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn map_require_readonly_source_can_return_its_slot_through_ordinary_wrapper() {
        let text = "fun view(source: Map<String, String>): borrow String from source = source.requireValue(\"key\")\nfun entry() {}";
        // The legacy no-entry witness checks emission of an unreachable wrapper.
        let program = crate::ssa::borrow_result_contract_tests::single(text).unwrap();
        verify_program(&program).unwrap();
        let function = program.modules[0]
            .functions
            .iter()
            .find(|f| f.borrow_return.is_some())
            .unwrap();
        assert!(
            function
                .instructions
                .iter()
                .any(|i| matches!(i.operation, Operation::MapRequireValue { .. }))
        );
        assert!(!function.instructions.iter().any(|i| matches!(
            i.operation,
            Operation::Copy { .. } | Operation::SharedRetain { .. }
        )));
    }
}
