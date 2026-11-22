use super::{
    EntityId, EntityType, LoanKind, Operation, SourceMap, SourceUnitInput, SourceUnitKey,
    SsaTypeKind, TerminatorKind, analyze, complete_compilation_unit_types,
    lower_scalar_unit_with_entry, parsed, standard_environments,
};
use crate::native_tests::boxed_enum_tests::{assert_success, run_counted_allocations_in_order};

#[test]
fn original_field_borrow_projects_and_ends_before_the_same_record_is_freed() {
    let text = include_str!(
        "../../../../lang-frontend/tests/ownership_nested_borrow_arguments/fixtures/field-transport.ko"
    );
    let path = "nested_borrow.ko";
    let mut sources = SourceMap::new();
    let (source, file) = parsed(&mut sources, path, text);
    let (ne, te) = standard_environments();
    for rooted in [false, true] {
        let inputs = [if rooted {
            SourceUnitInput::new("root", path, source, &file)
        } else {
            SourceUnitInput::memory_document(source, &file)
        }];
        let (names, typed, owned) = analyze(&sources, &inputs, &ne, &te);
        complete_compilation_unit_types(&sources, &inputs, &names, &te, typed.types()).unwrap();
        let index = names.names().index();
        let [input] = index.source_units() else {
            panic!("one original input")
        };
        assert_eq!(input.source_id(), source);
        assert_eq!(sources.source_text(source), Ok(text));
        assert_eq!(
            matches!(input.key(), SourceUnitKey::MemoryDocument),
            !rooted
        );
        let declaration = index
            .declarations()
            .iter()
            .find(|d| d.name() == "run")
            .unwrap()
            .id();
        let (program, entry) = lower_scalar_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &te,
            &typed,
            &owned,
            declaration,
        )
        .expect("the original field source reaches real Unit SSA");
        crate::ssa::verify::verify_program(&program).unwrap();
        let module = program.module(entry.module()).unwrap();
        let caller = module.function(entry).unwrap();
        let [block] = caller.blocks.as_slice() else {
            panic!("linear caller")
        };
        let [
            literal,
            construct,
            alloc,
            root,
            begin,
            field_call,
            observe,
            end_child,
            end_root,
            drop,
        ] = caller.instructions.as_slice()
        else {
            panic!("exact constructor, two loan ends, and one owner release")
        };
        let [EntityId::Value(string)] = literal.results.as_slice() else {
            panic!("original literal")
        };
        assert!(matches!(literal.operation, Operation::StringLiteral { .. }));
        let EntityType::Value(string_type) = caller.entity(EntityId::Value(*string)).unwrap().ty
        else {
            panic!("literal owner type")
        };
        assert_eq!(
            module.type_kind(string_type),
            Some(&SsaTypeKind::StringOwner)
        );
        let Operation::AggregateConstruct { fields, .. } = &construct.operation else {
            panic!("original Record payload")
        };
        assert_eq!(fields.as_slice(), [*string]);
        let [EntityId::Value(payload)] = construct.results.as_slice() else {
            panic!("constructed Record payload")
        };
        let Operation::HeapAllocate {
            owner: record_type,
            payload: allocated,
        } = alloc.operation
        else {
            panic!("real Record heap owner")
        };
        assert_eq!(allocated, *payload);
        assert_ne!(record_type, string_type);
        assert_eq!(sources.slice(alloc.origin.span()), Ok("Record(\"kept\")"));
        let [EntityId::Value(owner)] = alloc.results.as_slice() else {
            panic!("new Record owner")
        };
        assert_eq!(root.operation, Operation::RootPlace { owner: *owner });
        let [EntityId::Place(place)] = root.results.as_slice() else {
            panic!("place of the allocated Record")
        };
        assert_eq!(
            begin.operation,
            Operation::BorrowBegin {
                place: *place,
                kind: LoanKind::Shared
            }
        );
        let [EntityId::Loan(lender)] = begin.results.as_slice() else {
            panic!("Record lender")
        };
        assert_eq!(
            caller.entity(EntityId::Loan(*lender)).unwrap().ty,
            EntityType::Loan {
                kind: LoanKind::Shared,
                target: record_type
            }
        );
        let Operation::BorrowCall {
            callee,
            arguments,
            source: witness,
        } = &field_call.operation
        else {
            panic!("actual field borrow call")
        };
        assert_eq!(*witness, *lender);
        assert_eq!(arguments.as_slice(), [EntityId::Loan(*lender)]);
        assert_eq!(sources.slice(field_call.origin.span()), Ok("field(source)"));
        let field = module.function(*callee).unwrap();
        assert!(
            sources
                .slice(field.origin.span())
                .unwrap()
                .starts_with("fun field(")
        );
        let [field_entry] = field.blocks.as_slice() else {
            panic!("linear projecting callee")
        };
        let [EntityId::Loan(record_argument)] = field_entry.parameters.as_slice() else {
            panic!("real borrowed Record ABI")
        };
        assert_eq!(
            field.entity(EntityId::Loan(*record_argument)).unwrap().ty,
            EntityType::Loan {
                kind: LoanKind::Shared,
                target: record_type
            }
        );
        let [projection] = field.instructions.as_slice() else {
            panic!("returned field is projected, not copied or reallocated")
        };
        assert_eq!(
            projection.operation,
            Operation::SharedHeapFieldLoan {
                base: *record_argument,
                field: 0
            }
        );
        let [EntityId::Loan(projected)] = projection.results.as_slice() else {
            panic!("distinct interior String loan")
        };
        let expected_string_loan = EntityType::Loan {
            kind: LoanKind::Shared,
            target: string_type,
        };
        assert_ne!(projected, record_argument);
        assert_eq!(
            field.entity(EntityId::Loan(*projected)).unwrap().ty,
            expected_string_loan
        );
        assert!(matches!(field_entry.terminator.as_ref().unwrap().kind,
            TerminatorKind::BorrowReturn { loan } if loan == *projected));
        let [EntityId::Loan(child)] = field_call.results.as_slice() else {
            panic!("actual returned String loan")
        };
        assert_ne!(child, lender);
        assert_eq!(
            caller.entity(EntityId::Loan(*child)).unwrap().ty,
            expected_string_loan
        );
        let Operation::DirectCall {
            callee: observer,
            arguments,
            receiver: None,
        } = &observe.operation
        else {
            panic!("actual synchronous observer")
        };
        assert_eq!(arguments.as_slice(), [EntityId::Loan(*child)]);
        assert!(
            sources
                .slice(module.function(*observer).unwrap().origin.span())
                .unwrap()
                .starts_with("fun observe(")
        );
        assert_eq!(
            sources.slice(observe.origin.span()),
            Ok("observe(field(source))")
        );
        assert_eq!(end_child.origin, observe.origin);
        assert_eq!(end_root.origin, observe.origin);
        assert_eq!(end_child.operation, Operation::BorrowEnd { loan: *child });
        assert_eq!(end_root.operation, Operation::BorrowEnd { loan: *lender });
        assert_eq!(drop.operation, Operation::Drop { owner: *owner });
        assert!(matches!(&block.terminator.as_ref().unwrap().kind,
            TerminatorKind::Return { values } if values.is_empty()));
        let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
        let run = run_counted_allocations_in_order(&llvm, &[0]);
        assert_success(&run, b"kept\n");
    }
}
