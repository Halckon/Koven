//! Source-associated pending children must retain three separate actual CFG roles.
use super::*;
use crate::ssa::unit_lower::{
    UnitFunctionInstanceKey,
    nullable_proof::{
        NullableProof,
        guard_tests::{
            install_birth, origin_in_scope,
            support::{Fixture, assert_transport, edges, parameter, rebound, rejected, state},
        },
    },
};
use lang_frontend::{
    ownership_checking::UnitLoanTarget,
    parser::{Expression, Statement},
    type_checking::{UnitCallTarget, UnitExpressionId, UnitStatementId},
};

fn install_child(
    lowerer: &mut UnitExpressionLowerer<'_>,
    root: UnitSymbolId,
) -> (Span, PendingNullableChild, LoanId) {
    let call = lowerer.typed.calls().iter().find(|call| {
        matches!(call.target(), UnitCallTarget::Declaration(declaration)
            if lowerer.names.names().index().declarations()[declaration.index()].name() == "inspect")
    }).unwrap().expression();
    let Expression::Call { arguments, .. } = lowerer
        .parsed
        .ast()
        .expressions()
        .get(call.expression())
        .unwrap()
        .payload()
    else {
        panic!("ordinary call")
    };
    assert_eq!(arguments.len(), 2);
    let argument = UnitExpressionId::new(lowerer.source_unit, arguments[0].value);
    let span = lowerer
        .parsed
        .ast()
        .expressions()
        .get(arguments[1].value)
        .unwrap()
        .span();
    let fact = lowerer
        .owned
        .loans()
        .iter()
        .find(|fact| fact.call() == call && fact.argument() == argument)
        .unwrap();
    let UnitLoanTarget::Place(path) = fact.target() else {
        panic!("real field loan fact")
    };
    assert_eq!(path.root(), root);
    let [field_symbol] = path.fields() else {
        panic!("one source field")
    };
    let proof = lowerer.nullable_proofs[&root];
    let instruction = lowerer.function.instructions.iter().find(|instruction| {
        instruction.origin == Origin::Source(fact.begin_span())
            && matches!(instruction.operation, Operation::SharedHeapFieldLoan { base, .. } if base == proof.loan)
    }).unwrap();
    assert_eq!(instruction.block, lowerer.block);
    let Operation::SharedHeapFieldLoan { base, field } = instruction.operation else {
        unreachable!()
    };
    assert_eq!(base, proof.loan);
    let [EntityId::Loan(loan)] = instruction.results.as_slice() else {
        panic!("real child loan")
    };
    let loan = *loan;
    let target = lowerer
        .function
        .entity(EntityId::Loan(loan))
        .unwrap()
        .ty
        .semantic_type();
    let projection = lowerer.typed.aggregate_projection(argument).unwrap();
    assert_eq!(projection.field(), *field_symbol);
    assert_eq!(lowerer.type_ids[&projection.ty()], target);
    let child = PendingNullableChild {
        call,
        argument,
        root,
        parent_origin: proof.origin,
        parent_inner: proof.inner,
        field,
        target,
        slot: 0,
    };
    lowerer.pending_operands = vec![EntityId::Loan(loan)];
    lowerer.pending_call_frames = vec![PendingCallFrame {
        nullable_children: vec![child],
        call,
        receiver: false,
        loan_arguments: vec![(argument, vec![0])],
        loop_depth: lowerer.loops.len(),
        pending_start: 0,
        created_loans: vec![0],
        abi_slots: Vec::new(),
        field_replace_owner: None,
        shared_field_roots: vec![root],
        exclusive_root_owners: Vec::new(),
    }];
    lowerer
        .validate_pending_nullable_children(
            &lowerer.pending_call_frames,
            &lowerer.pending_operands,
            &lowerer.nullable_proofs,
            span,
        )
        .unwrap();
    (span, child, loan)
}

#[test]
fn unit_pending_child_guards_preserve_actual_columns() {
    let fixture = Fixture::new(&crate::ssa::while_flow_tests::proof_cases::pending_child_source());
    let root = fixture.symbol("current");
    for case in 0..20 {
        let mut function = fixture.function().clone();
        fixture.with_lowerer(&mut function, |lowerer| {
            let birth = install_birth(lowerer, root, &[root]);
            let context = lowerer.guard_source_loop_context(birth.header, &[root], Vec::new(), Some(birth.scope), birth.span);
            lowerer.loops.push(context);
            let (span, child, child_loan) = install_child(lowerer, root);
            assert_eq!(lowerer.pending_call_frames[0].loop_depth, 1);
            let proof = lowerer.nullable_proofs[&root];
            let mut bindings = lowerer.carried_bindings(&lowerer.bindings, span).unwrap();
            let mut loans = lowerer.carried_loans(&lowerer.borrow_bindings, span).unwrap();
            lowerer.carry_pending_operands(&mut bindings, &mut loans, span).unwrap();
            let TerminatorKind::Conditional { when_true, when_false, .. } =
                lowerer.function.block(lowerer.block).unwrap().terminator.as_ref().unwrap().kind.clone()
                else { panic!("the second source argument begins with actual if edges") };
            assert_eq!(carried_control_edge(when_true.target, &bindings, &loans), when_true);
            assert_eq!(when_true.arguments, when_false.arguments);
            let snapshot = state(lowerer);
            lowerer.validate_pending_child_edge_columns(&bindings, &loans, &snapshot, &when_true.arguments, span).unwrap();
            let child_slot = loans.iter().position(|slot| slot.source == EntityId::Loan(child_loan)).unwrap();
            let role = loans[child_slot].nullable_children[0].clone();
            let owner_col = role.owner_column;
            let parent_col = bindings.len() + role.parent_column;
            let child_col = bindings.len() + child_slot;
            assert_ne!(owner_col, parent_col);
            assert_ne!(parent_col, child_col);
            assert_eq!(when_true.arguments[owner_col], EntityId::Value(proof.owner));
            assert_eq!(when_true.arguments[parent_col], EntityId::Loan(proof.loan));
            assert_eq!(when_true.arguments[child_col], EntityId::Loan(child_loan));
            if case < 12 {
                let mut arguments = when_true.arguments.clone();
                let readonly = lowerer.parsed.ast().statements().iter().find_map(|(id, node)| {
                    let scope = UnitStatementId::new(lowerer.source_unit, id);
                    (scope != birth.scope && matches!(node.payload(), Statement::While { .. })).then_some(scope)
                }).unwrap();
                match case {
                    0 => {
                        let original = lowerer.function.block(lowerer.function.entry_block().unwrap()).unwrap().parameters[0];
                        assert_eq!(lowerer.function.entity(original).unwrap().ty,
                            lowerer.function.entity(arguments[owner_col]).unwrap().ty);
                        arguments[owner_col] = original;
                    }
                    1 | 2 => {
                        let column = if case == 1 { parent_col } else { child_col };
                        let old = arguments[column];
                        let replacement = rebound(lowerer.function, old, when_true.target);
                        assert_ne!(old, replacement);
                        assert_eq!(lowerer.function.entity(old).unwrap().ty, lowerer.function.entity(replacement).unwrap().ty);
                        arguments[column] = replacement;
                    }
                    3 => loans[child_slot].nullable_children[0].child.parent_origin = origin_in_scope(proof, readonly),
                    4 => loans[child_slot].nullable_children[0].child.slot = 1,
                    5 => loans[child_slot].nullable_children.clear(),
                    6 => loans[child_slot].nullable_children.push(role),
                    7 => loans[child_slot].pending.clear(),
                    8 => loans[child_slot].nullable_children[0].owner_column = bindings.len(),
                    9 => loans[child_slot].nullable_children[0].parent_column = child_slot,
                    10 => loans[child_slot].nullable_children[0].frames[0].loop_depth += 1,
                    11 => { arguments.swap(parent_col, child_col); }
                    _ => unreachable!(),
                }
                rejected(lowerer.validate_pending_child_edge_columns(&bindings, &loans, &snapshot, &arguments, span),
                    LoweringErrorKind::MissingFact, span);
                return;
            }
            // The real inspect call identifies the actual merge child, after both
            // source arms and the readonly nested while have rebound all roles.
            let UnitCallTarget::Declaration(declaration) = lowerer.typed.call(child.call).unwrap().target() else { unreachable!() };
            let callee = lowerer.function_ids[&UnitFunctionInstanceKey::for_entry(declaration)];
            let call = lowerer.function.instructions.iter().find(|instruction| {
                matches!(instruction.operation, Operation::DirectCall { callee: actual, .. } if actual == callee)
            }).unwrap();
            let merge = call.block;
            let Operation::DirectCall { arguments: call_arguments, .. } = &call.operation else { unreachable!() };
            let merged_child = rebound(lowerer.function, EntityId::Loan(child_loan), merge);
            assert_eq!(call_arguments[0], merged_child, "the existing call consumes the current child");
            let merged_owner = rebound(lowerer.function, EntityId::Value(proof.owner), merge);
            let merged_parent = rebound(lowerer.function, EntityId::Loan(proof.loan), merge);
            assert_transport(lowerer.function, merge, [merged_owner, merged_parent, merged_child],
                birth.body, [EntityId::Value(proof.owner), EntityId::Loan(proof.loan), EntityId::Loan(child_loan)]);
            let (_, owner_column) = parameter(lowerer.function, merged_owner);
            let (_, parent_column) = parameter(lowerer.function, merged_parent);
            let (_, child_column) = parameter(lowerer.function, merged_child);
            let predecessors = lowerer.function.blocks.iter().flat_map(|block| {
                block.terminator.iter().flat_map(|term| edges(&term.kind))
                    .filter(move |edge| edge.target == merge).map(move |edge| (block.id, edge.clone()))
            }).collect::<Vec<_>>();
            assert_eq!(predecessors.len(), 2);
            let mut exits = Vec::new();
            let mut arguments = Vec::new();
            for (block, edge) in predecessors {
                let mut exit = state(lowerer);
                exit.block = block;
                let EntityId::Value(owner) = edge.arguments[owner_column] else { panic!() };
                let EntityId::Loan(parent) = edge.arguments[parent_column] else { panic!() };
                let EntityId::Value(result) = edge.arguments[0] else { panic!("if expression result") };
                exit.result = LoweredValue::Value(result);
                exit.bindings.insert(root, LoweredValue::Value(owner));
                exit.nullable_proofs.insert(root, NullableProof { owner, loan: parent, ..proof });
                exit.pending_operands[0] = edge.arguments[child_column];
                for loan in exit.borrow_bindings.values_mut() {
                    let merged = rebound(lowerer.function, EntityId::Loan(*loan), merge);
                    let (_, column) = parameter(lowerer.function, merged);
                    let EntityId::Loan(current) = edge.arguments[column] else { panic!() };
                    *loan = current;
                }
                exits.push(exit);
                arguments.push(edge.arguments);
            }
            let mut pending_columns = vec![child_column];
            let mut proof_columns = vec![parent_column];
            lowerer.validate_pending_child_exits(&exits, span).unwrap();
            lowerer.validate_pending_child_merge_columns(&exits, &arguments, &[root], owner_column,
                &pending_columns, &proof_columns, span).unwrap();
            if case == 19 {
                exits[1].pending_call_frames[0].loop_depth += 1;
                rejected(lowerer.validate_pending_child_exits(&exits, span), LoweringErrorKind::UnsupportedNode, span);
                return;
            }
            match case {
                12 => arguments[0][owner_column] = EntityId::Value(proof.owner),
                13 => arguments[0][parent_column] = EntityId::Loan(proof.loan),
                14 => arguments[0][child_column] = EntityId::Loan(child_loan),
                15 => pending_columns.clear(),
                16 => proof_columns.clear(),
                17 => pending_columns[0] = parent_column,
                18 => proof_columns[0] = child_column,
                _ => unreachable!(),
            }
            rejected(lowerer.validate_pending_child_merge_columns(&exits, &arguments, &[root], owner_column,
                &pending_columns, &proof_columns, span), LoweringErrorKind::MissingFact, span);
        });
    }
}
