//! Ordinary non-loop transport only, not an E2 loop fact or receipt.
use super::{
    borrow_memory_test_support::single, borrow_storage_contract_tests::unit, model::*,
    verify::verify_program,
};

fn producer(f: &Function, entity: EntityId) -> &Operation {
    let Definition::InstructionResult { instruction, index } = f.entity(entity).unwrap().definition
    else {
        panic!("real instruction result")
    };
    let inst = f.instruction(instruction).unwrap();
    assert_eq!(inst.results[index], entity);
    &inst.operation
}
fn owner(f: &Function, entity: EntityId) -> ValueId {
    let Operation::BorrowBegin {
        place,
        kind: LoanKind::Shared,
    } = producer(f, entity)
    else {
        panic!("shared call argument")
    };
    let Operation::RootPlace { owner } = producer(f, EntityId::Place(*place)) else {
        panic!("actual root owner")
    };
    *owner
}
fn prints(f: &Function, b: &Block) -> Vec<EntityId> {
    b.instructions
        .iter()
        .map(|id| {
            let Operation::PrintString { value } = f.instruction(*id).unwrap().operation else {
                panic!("only direct root prints")
            };
            EntityId::Loan(value)
        })
        .collect()
}

#[test]
fn shared_branch_transport_baseline() {
    for reversed in [false, true] {
        let parameters = if reversed {
            "right:String,left:String"
        } else {
            "left:String,right:String"
        };
        let args = if reversed { "b,a" } else { "a,b" };
        let source = format!(
            "fun probe({parameters},flag:Boolean):Unit {{if(flag){{println(left)}}else{{println(right)}};println(left);println(right)}}\nfun entry(){{val a=\"a\";val b=\"b\";probe({args},true);probe(a,a,false)}}"
        );
        for lower in [single, unit] {
            let program = lower(&source).unwrap();
            verify_program(&program).unwrap();
            crate::llvm::render_verified_program(&program).unwrap();
            let m = &program.modules[0];
            let f = m
                .functions
                .iter()
                .find(|f| f.name.contains("probe"))
                .unwrap();
            assert_eq!(f.blocks.len(), 4);
            let entry = &f.blocks[0];
            let TerminatorKind::Conditional {
                condition,
                when_true,
                when_false,
            } = &entry.terminator.as_ref().unwrap().kind
            else {
                panic!("real branch")
            };
            assert_ne!(when_true.target, when_false.target);
            assert!(
                matches!(producer(f, EntityId::Value(*condition)), Operation::Read { source: PlaceAccess::Loan(loan) } if EntityId::Loan(*loan) == entry.parameters[2])
            );
            let left = usize::from(reversed);
            let right = 1 - left;
            let mut join_id = None;
            for (edge, role) in [(when_true, left), (when_false, right)] {
                assert_eq!(edge.arguments, entry.parameters, "ordered input transport");
                let arm = f.block(edge.target).unwrap();
                assert_eq!(arm.parameters.len(), 3);
                assert_eq!(prints(f, arm), vec![arm.parameters[role]]);
                let TerminatorKind::Branch(next) = &arm.terminator.as_ref().unwrap().kind else {
                    panic!("real join edge")
                };
                assert_eq!(
                    next.arguments, arm.parameters,
                    "both roots survive each arm"
                );
                if let Some(id) = join_id {
                    assert_eq!(id, next.target);
                }
                join_id = Some(next.target);
            }
            let join = f.block(join_id.unwrap()).unwrap();
            assert_eq!(join.parameters.len(), 3);
            assert_eq!(
                prints(f, join),
                vec![join.parameters[left], join.parameters[right]]
            );
            assert!(matches!(
                join.terminator.as_ref().unwrap().kind,
                TerminatorKind::Return { .. }
            ));
            assert!(!f.instructions.iter().any(|i| matches!(
                i.operation,
                Operation::BorrowEnd { .. } | Operation::BorrowCall { .. }
            )));
            let caller = m
                .functions
                .iter()
                .find(|f| f.name.contains("entry"))
                .unwrap();
            let calls = caller
                .instructions
                .iter()
                .filter_map(|i| match &i.operation {
                    Operation::DirectCall {
                        callee, arguments, ..
                    } if *callee == f.id => Some(arguments),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(calls.len(), 2);
            for (arg, bytes) in [
                (calls[0][left], b"a"),
                (calls[0][right], b"b"),
                (calls[1][left], b"a"),
                (calls[1][right], b"a"),
            ] {
                assert!(
                    matches!(producer(caller, EntityId::Value(owner(caller, arg))), Operation::StringLiteral { bytes: actual, .. } if actual == bytes)
                );
            }
            assert_ne!(
                owner(caller, calls[0][left]),
                owner(caller, calls[0][right])
            );
            assert_eq!(
                owner(caller, calls[1][left]),
                owner(caller, calls[1][right])
            );
            // Equal owner is compatible with distinct shared loan capabilities.
            assert_ne!(calls[1][left], calls[1][right]);
        }
    }
}

#[test]
fn child_borrow_branch_remains_unsupported() {
    let source = "fun view(source:String):borrow String from source=source\nfun probe(left:String,right:String,flag:Boolean):Unit {if(flag){borrow val a=view(left);println(a)}else{borrow val b=view(right);println(b)};println(left);println(right)}\nfun entry(){val a=\"a\";val b=\"b\";probe(a,b,true);probe(a,a,false)}";
    for (lower, offset) in [
        (
            single as fn(&str) -> Result<Program, super::LoweringError>,
            0,
        ),
        (
            unit as fn(&str) -> Result<Program, super::LoweringError>,
            "package p\n".len(),
        ),
    ] {
        let error = lower(source).err().expect("unsupported child branch");
        assert!(matches!(
            error.kind,
            super::LoweringErrorKind::UnsupportedNode
        ));
        // Both wrappers complete strict frontend analysis before returning this error.
        let span = error.span.unwrap();
        let target =
            "if(flag){borrow val a=view(left);println(a)}else{borrow val b=view(right);println(b)}";
        let start = source.find(target).unwrap() + offset;
        assert_eq!((span.start(), span.end()), (start, start + target.len()));
    }
}
