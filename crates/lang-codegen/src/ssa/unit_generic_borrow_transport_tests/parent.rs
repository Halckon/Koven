use super::{
    EntityId, EntityType, LoanKind, Operation, SourceMap, SourceUnitInput, SsaTypeKind,
    TerminatorKind, analyze, complete_compilation_unit_types, lower_scalar_unit_with_entry, parsed,
    standard_environments,
};

#[test]
fn original_parent_borrow_survives_child_and_ends_before_consuming_same_owner() {
    let text = include_str!(
        "../../../../lang-frontend/tests/ownership_nested_borrow_arguments/fixtures/parent-transport.ko"
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
        let entry = names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|d| d.name() == "run")
            .unwrap()
            .id();
        let (program, entry) =
            lower_scalar_unit_with_entry(&sources, &inputs, &names, &te, &typed, &owned, entry)
                .expect("exact parent source reaches Unit SSA");
        crate::ssa::verify::verify_program(&program).unwrap();
        let module = program.module(entry.module()).unwrap();
        let caller = module.function(entry).unwrap();
        let [block] = caller.blocks.as_slice() else {
            panic!("linear caller")
        };
        let [
            literal,
            root,
            begin,
            parent_call,
            child_call,
            observe,
            end_child,
            print,
            end_parent,
            end_root,
            consume,
        ] = caller.instructions.as_slice()
        else {
            panic!("expected children, later parent use, then delivery of the owner");
        };
        let [EntityId::Value(owner)] = literal.results.as_slice() else {
            panic!("original owner")
        };
        assert!(matches!(literal.operation, Operation::StringLiteral { .. }));
        let EntityType::Value(owner_type) = caller.entity(EntityId::Value(*owner)).unwrap().ty
        else {
            panic!("String owner type")
        };
        assert_eq!(
            module.type_kind(owner_type),
            Some(&SsaTypeKind::StringOwner)
        );
        assert_eq!(root.operation, Operation::RootPlace { owner: *owner });
        let [EntityId::Place(place)] = root.results.as_slice() else {
            panic!("same owner place")
        };
        assert_eq!(
            begin.operation,
            Operation::BorrowBegin {
                place: *place,
                kind: LoanKind::Shared
            }
        );
        let [EntityId::Loan(lender)] = begin.results.as_slice() else {
            panic!("original root lender")
        };
        let [EntityId::Loan(parent)] = parent_call.results.as_slice() else {
            panic!("persistent parent")
        };
        let [EntityId::Loan(child)] = child_call.results.as_slice() else {
            panic!("ephemeral child")
        };
        assert_ne!(parent, lender);
        assert_ne!(child, parent);
        assert_ne!(child, lender);
        for (call, loan, output, source_text) in [
            (parent_call, lender, parent, "view(source)"),
            (child_call, parent, child, "view(parent)"),
        ] {
            let Operation::BorrowCall {
                callee,
                arguments,
                source: source_loan,
            } = &call.operation
            else {
                panic!("actual borrowing call, not a copy or fresh owner");
            };
            assert_eq!(sources.slice(call.origin.span()), Ok(source_text));
            assert_eq!(*source_loan, *loan);
            assert_eq!(arguments.as_slice(), [EntityId::Loan(*loan)]);
            assert_eq!(
                caller.entity(EntityId::Loan(*output)).unwrap().ty,
                EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: owner_type
                }
            );
            let callee = module.function(*callee).unwrap();
            assert!(
                sources
                    .slice(callee.origin.span())
                    .unwrap()
                    .starts_with("fun view(")
            );
            let [entry] = callee.blocks.as_slice() else {
                panic!("simple view")
            };
            let [EntityId::Loan(arg)] = entry.parameters.as_slice() else {
                panic!("borrowed view ABI")
            };
            assert!(callee.instructions.is_empty());
            assert!(matches!(entry.terminator.as_ref().unwrap().kind,
                TerminatorKind::BorrowReturn { loan } if loan == *arg));
        }
        let Operation::DirectCall {
            callee: observer,
            arguments,
            receiver: None,
        } = &observe.operation
        else {
            panic!("actual observer");
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
            Ok("observe(view(parent))")
        );
        assert_eq!(end_child.operation, Operation::BorrowEnd { loan: *child });
        assert_eq!(end_child.origin, observe.origin);
        assert_eq!(print.operation, Operation::PrintString { value: *parent });
        assert_eq!(sources.slice(print.origin.span()), Ok("println(parent)"));
        assert_eq!(end_parent.operation, Operation::BorrowEnd { loan: *parent });
        assert_eq!(end_root.operation, Operation::BorrowEnd { loan: *lender });
        let Operation::DirectCall {
            callee: consumer,
            arguments,
            receiver: None,
        } = &consume.operation
        else {
            panic!("subsequent own delivery");
        };
        assert_eq!(sources.slice(consume.origin.span()), Ok("consume(source)"));
        assert_eq!(arguments.as_slice(), [EntityId::Value(*owner)]);
        let consumer = module.function(*consumer).unwrap();
        assert!(
            sources
                .slice(consumer.origin.span())
                .unwrap()
                .starts_with("fun consume(")
        );
        let [consumer_entry] = consumer.blocks.as_slice() else {
            panic!("simple consume")
        };
        let [EntityId::Value(received)] = consumer_entry.parameters.as_slice() else {
            panic!("actual own ABI")
        };
        let [drop] = consumer.instructions.as_slice() else {
            panic!("one Drop in the consumer only")
        };
        assert_eq!(drop.operation, Operation::Drop { owner: *received });
        assert!(matches!(&block.terminator.as_ref().unwrap().kind,
            TerminatorKind::Return { values } if values.is_empty()));
        let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
        super::super::unit_field_replace_tests::link_and_run(&llvm, "", b"kept\nkept\n");
    }
}
