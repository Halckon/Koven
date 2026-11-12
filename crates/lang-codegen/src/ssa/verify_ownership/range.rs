//! Descriptor metadata and protecting source loan have a single explicit lifetime.
use super::*;

pub(super) mod pairs {
    use super::*;

    pub(super) fn compute(function: &Function) -> BTreeSet<(ValueId, LoanId)> {
        let seeds = function
            .instructions
            .iter()
            .filter_map(|instruction| {
                if !matches!(
                    instruction.operation,
                    Operation::RangeConstruct { .. } | Operation::RangeCall { .. }
                ) {
                    return None;
                }
                let [EntityId::Value(view), EntityId::Loan(loan)] = instruction.results.as_slice()
                else {
                    return None;
                };
                Some((*view, *loan))
            })
            .collect::<BTreeSet<_>>();
        if seeds.is_empty() {
            return seeds;
        }
        let seed_types = seeds
            .iter()
            .filter_map(|(view, loan)| {
                Some((
                    function.entity(EntityId::Value(*view))?.ty,
                    function.entity(EntityId::Loan(*loan))?.ty,
                ))
            })
            .collect::<Vec<_>>();
        let mut incoming = BTreeMap::<_, Vec<_>>::new();
        for block in &function.blocks {
            for edge in edges(&block.terminator.as_ref().expect("verified terminator").kind) {
                incoming.entry(edge.target).or_default().push(edge);
            }
        }
        let mut candidates = seeds.clone();
        let mut parameters = Vec::new();
        for block in &function.blocks {
            if !incoming.contains_key(&block.id) {
                continue;
            }
            for (vi, entity) in block.parameters.iter().enumerate() {
                let EntityId::Value(view) = entity else {
                    continue;
                };
                for (li, entity) in block.parameters.iter().enumerate() {
                    let EntityId::Loan(loan) = entity else {
                        continue;
                    };
                    if seed_types.iter().any(|(v, l)| {
                        function.entity(EntityId::Value(*view)).map(|d| d.ty) == Some(*v)
                            && function.entity(EntityId::Loan(*loan)).map(|d| d.ty) == Some(*l)
                    }) {
                        candidates.insert((*view, *loan));
                        parameters.push((block.id, vi, li, *view, *loan));
                    }
                }
            }
        }
        // Greatest fixed point keeps a loop only if every predecessor transports the same pair.
        loop {
            let invalid = parameters
                .iter()
                .filter_map(|(block, vi, li, view, loan)| {
                    let valid = incoming[block].iter().all(|edge| {
                        match (edge.arguments.get(*vi), edge.arguments.get(*li)) {
                            (Some(EntityId::Value(v)), Some(EntityId::Loan(l))) => {
                                candidates.contains(&(*v, *l))
                            }
                            _ => false,
                        }
                    });
                    (!valid).then_some((*view, *loan))
                })
                .collect::<Vec<_>>();
            let mut changed = false;
            for pair in invalid {
                changed |= candidates.remove(&pair);
            }
            if !changed {
                break;
            }
        }
        // Then require a path from a real construct/call; a self-supporting cycle is no proof.
        let mut reachable = seeds;
        loop {
            let mut changed = false;
            for (block, vi, li, view, loan) in &parameters {
                if !candidates.contains(&(*view, *loan)) {
                    continue;
                }
                if incoming[block].iter().any(|edge| matches!((edge.arguments.get(*vi),edge.arguments.get(*li)),(Some(EntityId::Value(v)),Some(EntityId::Loan(l))) if reachable.contains(&(*v,*l)))) {
                    changed|=reachable.insert((*view,*loan));
                }
            }
            if !changed {
                break;
            }
        }
        reachable
    }

    pub(in crate::ssa::verify_ownership) fn is_view(
        module: &Module,
        function: &Function,
        loan: LoanId,
    ) -> bool {
        function.entity(EntityId::Loan(loan)).is_some_and(|data| {
            matches!(
                module.type_kind(data.ty.semantic_type()),
                Some(crate::ssa::model::SsaTypeKind::RangeView { .. })
            )
        })
    }

    pub(in crate::ssa::verify_ownership) fn root(
        function: &Function,
        loan: LoanId,
    ) -> Option<LoanId> {
        let pairs = compute(function);
        let flows = LoanFlowAliases::compute(function);
        resolve(function, loan, &pairs, &flows, &mut BTreeSet::new(), true)
    }

    /// A child's capability remains live even after the original caller loan and parent metadata end.
    pub(in crate::ssa::verify_ownership) fn capability(
        function: &Function,
        loan: LoanId,
    ) -> Option<LoanId> {
        let pairs = compute(function);
        let flows = LoanFlowAliases::compute(function);
        resolve(function, loan, &pairs, &flows, &mut BTreeSet::new(), false)
    }

    fn resolve(
        function: &Function,
        loan: LoanId,
        pairs: &BTreeSet<(ValueId, LoanId)>,
        flows: &LoanFlowAliases,
        visiting: &mut BTreeSet<LoanId>,
        flatten: bool,
    ) -> Option<LoanId> {
        if !visiting.insert(loan) {
            return None;
        }
        let result = (|| match function.entity(EntityId::Loan(loan))?.definition {
            Definition::BlockParameter { block, .. } if block.index() == 0 => Some(loan),
            Definition::BlockParameter { .. } => {
                let mut roots = BTreeSet::new();
                for origin in flows.origins.get(&loan)? {
                    roots.insert(resolve(function, *origin, pairs, flows, visiting, flatten)?);
                }
                (roots.len() == 1).then(|| *roots.first().expect("one root"))
            }
            Definition::InstructionResult { instruction, .. } => {
                match function.instruction(instruction)?.operation {
                    Operation::SharedReborrow { source } | Operation::BorrowCall { source, .. } => {
                        resolve(function, source, pairs, flows, visiting, flatten)
                    }
                    Operation::RangeConstruct { source, .. }
                    | Operation::RangeCall { source, .. } => {
                        if flatten {
                            resolve(function, source, pairs, flows, visiting, flatten)
                        } else {
                            Some(loan)
                        }
                    }
                    Operation::BorrowBegin { place, .. }
                        if pairs.iter().any(|(view, _)| {
                            function
                                .entity(EntityId::Value(*view))
                                .map(|data| data.ty.semantic_type())
                                == function
                                    .entity(EntityId::Loan(loan))
                                    .map(|data| data.ty.semantic_type())
                        }) =>
                    {
                        let Definition::InstructionResult { instruction, .. } =
                            function.entity(EntityId::Place(place))?.definition
                        else {
                            return None;
                        };
                        let Operation::RootPlace { owner } =
                            function.instruction(instruction)?.operation
                        else {
                            return None;
                        };
                        let mut roots = BTreeSet::new();
                        for (_, source) in pairs.iter().filter(|(view, _)| *view == owner) {
                            roots.insert(if flatten {
                                resolve(function, *source, pairs, flows, visiting, flatten)?
                            } else {
                                *source
                            });
                        }
                        (roots.len() == 1).then(|| *roots.first().expect("one root"))
                    }
                    Operation::BorrowBegin { .. } => Some(loan),
                    _ => None,
                }
            }
        })();
        visiting.remove(&loan);
        result
    }
}
pub(super) use pairs as source;
fn pair(function: &Function, view: ValueId) -> Option<LoanId> {
    pairs::compute(function)
        .into_iter()
        .find_map(|(value, loan)| (value == view).then_some(loan))
}

pub(super) fn live_descriptor(
    function: &Function,
    source: LoanId,
    state: &BlockState,
) -> Option<ValueId> {
    state
        .values
        .iter()
        .copied()
        .find(|view| pair(function, *view) == Some(source))
}

fn require_source(
    source: LoanId,
    state: &BlockState,
    location: VerifyLocation,
    origin: &super::super::model::Origin,
    errors: &mut Vec<VerifyError>,
) -> bool {
    if state.loans.contains(&source) {
        true
    } else {
        errors.push(error(
            VerifyErrorKind::LoanInactive { loan: source },
            location,
            origin,
        ));
        false
    }
}

fn require_root(
    function: &Function,
    loan: LoanId,
    state: &BlockState,
    location: VerifyLocation,
    origin: &super::super::model::Origin,
    errors: &mut Vec<VerifyError>,
) {
    let flows = LoanFlowAliases::compute(function);
    if pairs::capability(function, loan).is_none_or(|root| {
        !state
            .loans
            .iter()
            .any(|active| flows.equivalent(*active, root))
    }) {
        errors.push(error(
            VerifyErrorKind::OperationContract {
                reason: "range source has no live exact root capability",
            },
            location,
            origin,
        ));
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn apply(
    module: &Module,
    function: &Function,
    instruction: &super::super::model::Instruction,
    aliases: &AliasRoots,
    closure_loans: &closure::ClosureLoans,
    dependencies: &ReborrowDependencies,
    state: &mut BlockState,
    errors: &mut Vec<VerifyError>,
) {
    let location = VerifyLocation::Instruction(instruction.id);
    let origin = &instruction.origin;
    match &instruction.operation {
        Operation::RangeElementPlace { view, index } => {
            require_source(*view, state, location.clone(), origin, errors);
            require_value(module, function, *index, state, location, origin, errors);
        }
        Operation::RangeConstruct { source, .. } => {
            require_source(*source, state, location.clone(), origin, errors);
            require_root(function, *source, state, location, origin, errors);
        }
        Operation::RangeCall {
            source, arguments, ..
        } => {
            require_source(*source, state, location.clone(), origin, errors);
            require_root(function, *source, state, location.clone(), origin, errors);
            for argument in arguments {
                match argument {
                    EntityId::Loan(loan) => {
                        require_source(*loan, state, location.clone(), origin, errors);
                        closure::check_borrowed_contents(
                            *loan,
                            closure_loans,
                            state,
                            instruction.id,
                            origin,
                            errors,
                            "range call Borrow requires proved current callable capture contents",
                        );
                    }
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
                    EntityId::Place(place) => {
                        require_place(*place, state, location.clone(), origin, errors);
                    }
                }
            }
        }
        Operation::RangeLength {
            view: EntityId::Value(view),
        } => {
            require_value(
                module,
                function,
                *view,
                state,
                location.clone(),
                origin,
                errors,
            );
            if let Some(source) = pair(function, *view) {
                require_source(source, state, location, origin, errors);
            } else {
                errors.push(error(
                    VerifyErrorKind::ReturnType { index: 0 },
                    location,
                    origin,
                ));
            }
        }
        Operation::RangeLength {
            view: EntityId::Loan(loan),
        } => {
            require_source(*loan, state, location, origin, errors);
        }
        Operation::RangeEnd { view, source } => {
            if pair(function, *view) != Some(*source) {
                errors.push(error(
                    VerifyErrorKind::ReturnType { index: 0 },
                    location,
                    origin,
                ));
                return;
            }
            if consume_value(
                module,
                function,
                *view,
                aliases,
                state,
                &BTreeSet::new(),
                &BTreeSet::new(),
                location.clone(),
                origin,
                errors,
            ) {
                end_source(
                    function,
                    *source,
                    aliases,
                    closure_loans,
                    dependencies,
                    state,
                    location,
                    origin,
                    errors,
                );
            }
        }
        _ => {}
    }
}

#[allow(clippy::too_many_arguments)]
fn end_source(
    function: &Function,
    source: LoanId,
    aliases: &AliasRoots,
    closure_loans: &closure::ClosureLoans,
    dependencies: &ReborrowDependencies,
    state: &mut BlockState,
    location: VerifyLocation,
    origin: &super::super::model::Origin,
    errors: &mut Vec<VerifyError>,
) {
    if let Some(owner) = live_descriptor(function, source, state)
        .or_else(|| closure_loans.live_owner_holding(source, state))
    {
        errors.push(error(
            VerifyErrorKind::OwnerLoanConflict { value: owner },
            location,
            origin,
        ));
    } else if let Some(dependent) = dependencies.active_descendant(source, aliases, state) {
        errors.push(error(
            VerifyErrorKind::LoanDependencyActive {
                parent: source,
                dependent,
            },
            location,
            origin,
        ));
    } else if !state.loans.remove(&source) {
        errors.push(error(
            VerifyErrorKind::LoanInactive { loan: source },
            location,
            origin,
        ));
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn verify_return(
    module: &Module,
    function: &Function,
    view: ValueId,
    source: LoanId,
    aliases: &AliasRoots,
    dependencies: &ReborrowDependencies,
    state: &mut BlockState,
    location: VerifyLocation,
    origin: &super::super::model::Origin,
    errors: &mut Vec<VerifyError>,
) {
    let Some((parameter, _, _)) = super::super::verify_operation::range_parameter(module, function)
    else {
        return;
    };
    if pair(function, view) != Some(source)
        || aliases.roots.get(&EntityId::Loan(source))
            != Some(&BTreeSet::from([EntityId::Loan(parameter)]))
    {
        errors.push(error(
            VerifyErrorKind::ReturnType { index: 0 },
            location,
            origin,
        ));
        return;
    }
    if !require_source(source, state, location.clone(), origin, errors) {
        return;
    }
    if dependencies
        .active_descendant(source, aliases, state)
        .is_some()
    {
        errors.push(error(
            VerifyErrorKind::ReturnType { index: 0 },
            location,
            origin,
        ));
        return;
    }
    if !consume_value(
        module,
        function,
        view,
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
    let mut current = source;
    let mut visited = BTreeSet::new();
    while visited.insert(current) {
        state.loans.remove(&current);
        if dependencies.flows.equivalent(current, parameter) {
            return;
        }
        let Some(parent) = dependencies.parent_by_child.get(&current).copied() else {
            break;
        };
        current = parent;
    }
    errors.push(error(VerifyErrorKind::ReturnType { index: 0 }, location, origin));
}

pub(in crate::ssa::verify_ownership) mod borrow_result {
    use super::super::*;
    use crate::ssa::model::{Instruction, Origin};

    #[allow(clippy::too_many_arguments)]
    pub(in crate::ssa::verify_ownership) fn verify_return(
        function: &Function,
        loan: LoanId,
        aliases: &AliasRoots,
        dependencies: &ReborrowDependencies,
        state: &mut BlockState,
        location: VerifyLocation,
        origin: &Origin,
        errors: &mut Vec<VerifyError>,
    ) {
        let Some((parameter, _)) = crate::ssa::verify_borrow_result::parameter(function) else {
            return;
        };
        let expected = BTreeSet::from([EntityId::Loan(parameter)]);
        if aliases.roots.get(&EntityId::Loan(loan)) != Some(&expected) {
            errors.push(error(VerifyErrorKind::ReturnType { index: 0 }, location.clone(), origin));
            return;
        }
        if !state.loans.contains(&loan) {
            errors.push(error(VerifyErrorKind::LoanInactive { loan }, location.clone(), origin));
            return;
        }
        if let Some(dependent) = dependencies.active_descendant(loan, aliases, state) {
            errors.push(error(
                VerifyErrorKind::LoanDependencyActive { parent: loan, dependent },
                location.clone(),
                origin,
            ));
            return;
        }
        let mut current = loan;
        let mut visited = BTreeSet::new();
        while visited.insert(current) {
            state.loans.remove(&current);
            if current == parameter {
                return;
            }
            let Some(parent) = dependencies.parent_by_child.get(&current).copied() else {
                break;
            };
            current = parent;
        }
        errors.push(error(VerifyErrorKind::ReturnType { index: 0 }, location, origin));
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::ssa::verify_ownership) fn verify_call(
        module: &Module,
        function: &Function,
        instruction: &Instruction,
        aliases: &AliasRoots,
        closure_loans: &closure::ClosureLoans,
        reborrows: &ReborrowDependencies,
        state: &mut BlockState,
        errors: &mut Vec<VerifyError>,
    ) {
        let Operation::BorrowCall { arguments, source, .. } = &instruction.operation else {
            return;
        };
        verify_call_arguments(
            module, function, instruction, arguments.iter().copied(),
            aliases, closure_loans, reborrows, state, errors,
        );
        if !arguments.contains(&EntityId::Loan(*source)) && !state.loans.contains(source) {
            errors.push(error(
                VerifyErrorKind::LoanInactive { loan: *source },
                VerifyLocation::Instruction(instruction.id),
                &instruction.origin,
            ));
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::ssa::verify_ownership) fn verify_call_arguments(
        module: &Module,
        function: &Function,
        instruction: &Instruction,
        arguments: impl IntoIterator<Item = EntityId>,
        aliases: &AliasRoots,
        closure_loans: &closure::ClosureLoans,
        reborrows: &ReborrowDependencies,
        state: &mut BlockState,
        errors: &mut Vec<VerifyError>,
    ) {
        let location = VerifyLocation::Instruction(instruction.id);
        let origin = &instruction.origin;
        for argument in arguments {
            match argument {
                EntityId::Value(value) => {
                    consume_value(
                        module, function, value, aliases, state,
                        &BTreeSet::new(), &BTreeSet::new(), location.clone(), origin, errors,
                    );
                }
                EntityId::Loan(loan) => {
                    closure::check_borrowed_contents(
                        loan, closure_loans, state, instruction.id, origin, errors,
                        "helper Borrow requires proved current callable capture contents",
                    );
                    if !state.loans.contains(&loan) {
                        errors.push(error(VerifyErrorKind::LoanInactive { loan }, location.clone(), origin));
                    } else if loan_kind(function, loan) == LoanKind::Exclusive
                        && let Some(dependent) = reborrows.active_descendant(loan, aliases, state)
                    {
                        errors.push(error(
                            VerifyErrorKind::LoanDependencyActive { parent: loan, dependent },
                            location.clone(),
                            origin,
                        ));
                    }
                }
                EntityId::Place(place) => {
                    require_place(place, state, location.clone(), origin, errors);
                }
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::ssa::{model::{Origin, Program, SsaTypeKind}, verify::verify_program};
        use lang_frontend::source::SourceMap;

        fn fixture(copyable: bool, drop_parameters: bool) -> Program {
            fixture_with_prefix(copyable, drop_parameters, |_| {})
        }

        fn fixture_with_prefix(
            copyable: bool,
            drop_parameters: bool,
            prefix: impl FnOnce(&mut Function),
        ) -> Program {
            let mut sources = SourceMap::new();
            let source = sources.add_source("call-contract.ko", "view").unwrap();
            let origin = Origin::Source(sources.span(source, 0, 4).unwrap());
            let mut program = Program::default();
            let module_id = program.add_module("call-contract");
            let module = program.module_mut(module_id).unwrap();
            let string = module.intern_type(SsaTypeKind::StringOwner);
            let value = if copyable {
                module.intern_type(SsaTypeKind::Integer { bits: 32, signed: true })
            } else {
                string
            };
            let shared = EntityType::Loan { kind: LoanKind::Shared, target: string };
            let parameters = vec![EntityType::Value(value), EntityType::Value(value), shared, shared];
            let callee = module.add_function("view", vec![string], origin.clone()).unwrap();
            let function = module.function_mut(callee).unwrap();
            function.borrow_return = Some(2);
            let entry = function.add_block(parameters.clone(), origin.clone()).unwrap();
            if !copyable && drop_parameters {
                for index in 0..2 {
                    let EntityId::Value(owner) = function.blocks[0].parameters[index] else {
                        unreachable!()
                    };
                    function.append_instruction(entry, Operation::Drop { owner }, vec![], origin.clone()).unwrap();
                }
            }
            let EntityId::Loan(loan) = function.blocks[0].parameters[2] else {
                unreachable!()
            };
            function.set_terminator(entry, TerminatorKind::BorrowReturn { loan }, origin.clone()).unwrap();
            let caller = module.add_function("caller", vec![], origin.clone()).unwrap();
            let function = module.function_mut(caller).unwrap();
            let entry = function.add_block(parameters, origin.clone()).unwrap();
            let arguments = function.blocks[0].parameters.clone();
            let EntityId::Loan(source) = arguments[2] else {
                unreachable!()
            };
            let (_, results) = function
                .append_instruction(
                    entry,
                    Operation::BorrowCall { callee, arguments, source },
                    vec![shared],
                    origin.clone(),
                )
                .unwrap();
            let EntityId::Loan(loan) = results[0] else {
                unreachable!()
            };
            function.append_instruction(entry, Operation::BorrowEnd { loan }, vec![], origin.clone()).unwrap();
            prefix(function);
            function.set_terminator(entry, TerminatorKind::Return { values: vec![] }, origin).unwrap();
            program
        }

        fn caller(program: &mut Program) -> &mut Function {
            &mut program.modules[0].functions[1]
        }

        fn arguments(function: &mut Function) -> &mut Vec<EntityId> {
            let Operation::BorrowCall { arguments, .. } = &mut function.instructions[0].operation else {
                unreachable!()
            };
            arguments
        }

        fn owner(function: &Function) -> ValueId {
            let EntityId::Value(owner) = function.blocks[0].parameters[0] else {
                unreachable!()
            };
            owner
        }

        fn before_call(
            function: &mut Function,
            operation: Operation,
            results: Vec<EntityType>,
        ) -> Vec<EntityId> {
            let block = function.blocks[0].id;
            let call = function.instructions[0].id;
            let (instruction, results) = function
                .append_instruction(block, operation, results, function.origin.clone())
                .unwrap();
            let instructions = &mut function.blocks[0].instructions;
            assert_eq!(instructions.pop(), Some(instruction));
            let index = instructions.iter().position(|id| *id == call).unwrap();
            instructions.insert(index, instruction);
            results
        }

        fn errors(program: &Program) -> Vec<VerifyError> {
            verify_program(program).expect_err("invalid call contract").errors
        }

        #[test]
        fn verify_mixed_call_ci_only_repeated_copyable_value_is_allowed() {
            let mut program = fixture(true, true);
            let arguments = arguments(caller(&mut program));
            arguments[1] = arguments[0];
            verify_program(&program).unwrap();
        }

        #[test]
        fn verify_mixed_call_ci_only_repeated_move_only_value_is_rejected() {
            let mut program = fixture(false, true);
            let function = caller(&mut program);
            let owner = owner(function);
            let arguments = arguments(function);
            arguments[1] = arguments[0];
            assert!(errors(&program).iter().any(|error| {
                error.kind == VerifyErrorKind::ValueUnavailable { value: owner }
            }));
        }

        #[test]
        fn verify_mixed_call_ci_only_consumed_value_is_rejected() {
            let mut program = fixture_with_prefix(false, true, |function| {
                let owner = owner(function);
                before_call(function, Operation::Drop { owner }, vec![]);
            });
            let function = caller(&mut program);
            let owner = owner(function);
            let call = function.instructions[0].id;
            assert!(errors(&program).iter().any(|error| {
                error.kind == VerifyErrorKind::ValueUnavailable { value: owner }
                    && error.location == VerifyLocation::Instruction(call)
            }));
        }

        #[test]
        fn verify_mixed_call_ci_only_inactive_auxiliary_loan_is_rejected() {
            let mut program = fixture_with_prefix(false, true, |function| {
                let EntityId::Loan(loan) = function.blocks[0].parameters[3] else {
                    unreachable!()
                };
                before_call(function, Operation::BorrowEnd { loan }, vec![]);
            });
            let function = caller(&mut program);
            let EntityId::Loan(loan) = function.blocks[0].parameters[3] else {
                unreachable!()
            };
            let call = function.instructions[0].id;
            assert!(errors(&program).iter().any(|error| {
                error.kind == VerifyErrorKind::LoanInactive { loan }
                    && error.location == VerifyLocation::Instruction(call)
            }));
        }

        #[test]
        fn verify_mixed_call_ci_only_borrowed_owner_cannot_be_consumed() {
            let mut program = fixture_with_prefix(false, true, |function| {
                let owner = owner(function);
                let target = function.entity(EntityId::Value(owner)).unwrap().ty.semantic_type();
                let places = before_call(function, Operation::RootPlace { owner }, vec![EntityType::Place(target)]);
                let EntityId::Place(place) = places[0] else {
                    unreachable!()
                };
                before_call(
                    function,
                    Operation::BorrowBegin { place, kind: LoanKind::Shared },
                    vec![EntityType::Loan { kind: LoanKind::Shared, target }],
                );
            });
            let function = caller(&mut program);
            let owner = owner(function);
            let call = function.instructions[0].id;
            assert!(errors(&program).iter().any(|error| {
                error.kind == VerifyErrorKind::OwnerLoanConflict { value: owner }
                    && error.location == VerifyLocation::Instruction(call)
            }));
        }

        #[test]
        fn verify_mixed_call_ci_only_callee_owned_exit_is_still_required() {
            let program = fixture(false, false);
            let function = &program.modules[0].functions[0];
            let missing = owner(function);
            assert!(errors(&program).iter().any(|error| {
                error.kind == VerifyErrorKind::MissingOwnedExit { value: missing }
            }));
        }

        #[test]
        fn verify_mixed_call_ci_only_raw_place_argument_is_rejected() {
            let mut program = fixture_with_prefix(false, true, |function| {
                let owner = owner(function);
                let target = function.entity(EntityId::Value(owner)).unwrap().ty.semantic_type();
                let places = before_call(function, Operation::RootPlace { owner }, vec![EntityType::Place(target)]);
                arguments(function)[0] = places[0];
            });
            let call = caller(&mut program).instructions[0].id;
            assert!(errors(&program).iter().any(|error| {
                matches!(error.kind, VerifyErrorKind::OperationContract { .. })
                    && error.location == VerifyLocation::Instruction(call)
            }));
        }

        #[test]
        fn verify_mixed_call_ci_only_exclusive_auxiliary_parameter_is_rejected() {
            let mut program = fixture(false, true);
            for function in &mut program.modules[0].functions {
                let EntityType::Loan { kind, .. } = &mut function.loans[1].ty else {
                    unreachable!()
                };
                *kind = LoanKind::Exclusive;
            }
            assert!(errors(&program).iter().any(|error| {
                matches!(error.kind, VerifyErrorKind::OperationContract { .. })
            }));
        }

        #[test]
        fn verify_mixed_call_ci_only_owned_source_parameter_is_rejected() {
            let mut program = fixture(false, true);
            program.modules[0].functions[0].borrow_return = Some(0);
            assert!(errors(&program).iter().any(|error| {
                matches!(error.kind, VerifyErrorKind::OperationContract { .. })
            }));
        }

        #[test]
        fn verify_mixed_call_ci_only_non_abi_value_parameter_is_rejected() {
            let mut program = fixture(true, true);
            program.modules[0].types[1] = SsaTypeKind::Unit;
            assert!(errors(&program).iter().any(|error| {
                matches!(error.kind, VerifyErrorKind::OperationContract { .. })
            }));
        }
    }
}
