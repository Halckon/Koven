use super::{
    EntityId, EntityType, LoanKind, Operation, SourceMap, SourceUnitInput, SsaTypeKind,
    TerminatorKind, analyze, complete_compilation_unit_types, lower_scalar_unit_with_entry, parsed,
    standard_environments,
};

#[test]
fn exact_grouped_named_pair_evaluates_source_order_and_passes_parameter_order() {
    let text = include_str!(
        "../../../../lang-frontend/tests/ownership_nested_borrow_arguments/fixtures/grouped-transport.ko"
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
        let declaration = names
            .names()
            .index()
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
        .expect("original grouped named source reaches actual Unit SSA");
        crate::ssa::verify::verify_program(&program).unwrap();
        let module = program.module(entry.module()).unwrap();
        let caller = module.function(entry).unwrap();
        let [block] = caller.blocks.as_slice() else {
            panic!("linear caller")
        };
        let [
            literal,
            second_root,
            second_begin,
            second_call,
            first_root,
            first_begin,
            first_call,
            pair,
            end_first_child,
            end_first_lender,
            end_second_child,
            end_second_lender,
            drop,
        ] = caller.instructions.as_slice()
        else {
            panic!("exactly two complete chains in source order, then endings and root cleanup");
        };
        let [EntityId::Value(owner)] = literal.results.as_slice() else {
            panic!("original owner")
        };
        assert!(matches!(literal.operation, Operation::StringLiteral { .. }));
        let EntityType::Value(owner_type) = caller.entity(EntityId::Value(*owner)).unwrap().ty
        else {
            panic!("String Value")
        };
        assert_eq!(
            module.type_kind(owner_type),
            Some(&SsaTypeKind::StringOwner)
        );
        let shared = EntityType::Loan {
            kind: LoanKind::Shared,
            target: owner_type,
        };
        let chain = |root: &crate::ssa::model::Instruction,
                     begin: &crate::ssa::model::Instruction,
                     call: &crate::ssa::model::Instruction,
                     spelling| {
            assert_eq!(root.operation, Operation::RootPlace { owner: *owner });
            let [EntityId::Place(place)] = root.results.as_slice() else {
                panic!("actual owned storage")
            };
            assert_eq!(
                begin.operation,
                Operation::BorrowBegin {
                    place: *place,
                    kind: LoanKind::Shared
                }
            );
            let [EntityId::Loan(lender)] = begin.results.as_slice() else {
                panic!("actual lender")
            };
            let Operation::BorrowCall {
                callee,
                arguments,
                source,
            } = &call.operation
            else {
                panic!("actual view")
            };
            assert_eq!(*source, *lender);
            assert_eq!(arguments.as_slice(), [EntityId::Loan(*lender)]);
            assert_eq!(sources.slice(call.origin.span()), Ok(spelling));
            let [EntityId::Loan(child)] = call.results.as_slice() else {
                panic!("actual child")
            };
            for id in [*lender, *child] {
                assert_eq!(caller.entity(EntityId::Loan(id)).unwrap().ty, shared);
            }
            (*place, *lender, *child, *callee)
        };
        let (second_place, second_lender, second_child, view) =
            chain(second_root, second_begin, second_call, "view((source))");
        let (first_place, first_lender, first_child, same_view) =
            chain(first_root, first_begin, first_call, "view(source)");
        assert_ne!(second_place, first_place);
        assert_eq!(view, same_view);
        assert_eq!(
            std::collections::BTreeSet::from([
                second_lender,
                second_child,
                first_lender,
                first_child
            ])
            .len(),
            4,
        );
        let view = module.function(view).unwrap();
        let [view_entry] = view.blocks.as_slice() else {
            panic!("linear view")
        };
        let [EntityId::Loan(view_param)] = view_entry.parameters.as_slice() else {
            panic!("shared view ABI")
        };
        assert_eq!(view.entity(EntityId::Loan(*view_param)).unwrap().ty, shared);
        assert!(view.instructions.is_empty());
        assert!(matches!(view_entry.terminator.as_ref().unwrap().kind,
            TerminatorKind::BorrowReturn { loan } if loan == *view_param));
        let Operation::DirectCall {
            callee,
            arguments,
            receiver: None,
        } = &pair.operation
        else {
            panic!("actual pair")
        };
        assert_eq!(
            arguments.as_slice(),
            [EntityId::Loan(first_child), EntityId::Loan(second_child)]
        );
        assert_eq!(
            sources.slice(pair.origin.span()),
            Ok("pair(second = ((view((source)))), first = view(source))")
        );
        let pair_fn = module.function(*callee).unwrap();
        assert_eq!(
            sources.slice(pair_fn.origin.span()),
            Ok("fun pair(first: String, second: String) {}")
        );
        let [pair_entry] = pair_fn.blocks.as_slice() else {
            panic!("linear empty pair")
        };
        let [EntityId::Loan(first), EntityId::Loan(second)] = pair_entry.parameters.as_slice()
        else {
            panic!("two actual shared ABI parameters");
        };
        assert_ne!(first, second);
        for parameter in [*first, *second] {
            assert_eq!(
                pair_fn.entity(EntityId::Loan(parameter)).unwrap().ty,
                shared
            );
        }
        assert!(pair_fn.instructions.is_empty());
        for (end, loan) in [
            (end_first_child, first_child),
            (end_first_lender, first_lender),
            (end_second_child, second_child),
            (end_second_lender, second_lender),
        ] {
            assert_eq!(end.origin, pair.origin);
            assert_eq!(end.operation, Operation::BorrowEnd { loan });
        }
        assert_eq!(drop.operation, Operation::Drop { owner: *owner });
        assert!(matches!(&block.terminator.as_ref().unwrap().kind,
            TerminatorKind::Return { values } if values.is_empty()));
        let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
        let view_symbol = format!("call ptr @f{}.{}(", view.id.index(), view.name);
        let pair_symbol = format!("call void @f{}.{}(", pair_fn.id.index(), pair_fn.name);
        let evaluated: Vec<_> = llvm
            .lines()
            .filter_map(|line| line.trim().split_once(" = "))
            .filter_map(|(result, call)| call.starts_with(&view_symbol).then_some(result))
            .collect();
        let [second_native, first_native] = evaluated.as_slice() else {
            panic!("two actual emitted view calls")
        };
        assert_ne!(second_native, first_native);
        let passed: Vec<_> = llvm
            .lines()
            .filter_map(|line| line.trim().strip_prefix(&pair_symbol))
            .collect();
        assert_eq!(
            passed.as_slice(),
            [format!("ptr {first_native}, ptr {second_native})")]
        );
        // This native smoke only proves the original empty pair links and runs. The
        // source→parameter permutation comes from SSA and the distinct emitted operands,
        // not equality or inequality of the runtime values of two loans of one owner.
        super::super::unit_field_replace_tests::link_and_run(&llvm, "", b"");
    }
}
