use super::{
    EntityId, EntityType, LoanKind, Operation, SourceMap, SourceUnitInput, SsaTypeKind,
    TerminatorKind, analyze, complete_compilation_unit_types, lower_scalar_unit_with_entry, parsed,
    standard_environments,
};
use crate::ssa::model::ScalarConstant;

#[test]
fn original_scalar_borrow_returns_one_then_ends_before_consuming_same_owner() {
    let text = include_str!(
        "../../../../lang-frontend/tests/ownership_nested_borrow_arguments/fixtures/scalar-transport.ko"
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
        .expect("exact scalar source reaches Unit SSA");
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
            view_call,
            measure_call,
            end_child,
            end_lender,
            consume,
        ] = caller.instructions.as_slice()
        else {
            panic!("original owner, two borrow ends, then own delivery");
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
            panic!("actual root lender")
        };
        let Operation::BorrowCall {
            callee: view,
            arguments,
            source: witness,
        } = &view_call.operation
        else {
            panic!("actual borrowing call")
        };
        assert_eq!(sources.slice(view_call.origin.span()), Ok("view(source)"));
        assert_eq!(*witness, *lender);
        assert_eq!(arguments.as_slice(), [EntityId::Loan(*lender)]);
        let view = module.function(*view).unwrap();
        assert!(
            sources
                .slice(view.origin.span())
                .unwrap()
                .starts_with("fun view(")
        );
        let [view_entry] = view.blocks.as_slice() else {
            panic!("linear view")
        };
        let [EntityId::Loan(view_arg)] = view_entry.parameters.as_slice() else {
            panic!("real view ABI")
        };
        assert!(view.instructions.is_empty());
        assert!(matches!(view_entry.terminator.as_ref().unwrap().kind,
            TerminatorKind::BorrowReturn { loan } if loan == *view_arg));
        let [EntityId::Loan(child)] = view_call.results.as_slice() else {
            panic!("actual child")
        };
        assert_ne!(child, lender);
        let shared = EntityType::Loan {
            kind: LoanKind::Shared,
            target: owner_type,
        };
        for id in [*lender, *child] {
            assert_eq!(caller.entity(EntityId::Loan(id)).unwrap().ty, shared);
        }
        let Operation::DirectCall {
            callee: measure,
            arguments,
            receiver: None,
        } = &measure_call.operation
        else {
            panic!("actual scalar call, not absent because count is unused");
        };
        assert_eq!(arguments.as_slice(), [EntityId::Loan(*child)]);
        assert_eq!(
            sources.slice(measure_call.origin.span()),
            Ok("measure(view(source))")
        );
        let measure = module.function(*measure).unwrap();
        assert!(
            sources
                .slice(measure.origin.span())
                .unwrap()
                .starts_with("fun measure(")
        );
        let [EntityId::Value(count)] = measure_call.results.as_slice() else {
            panic!("returned scalar")
        };
        let EntityType::Value(int_type) = caller.entity(EntityId::Value(*count)).unwrap().ty else {
            panic!("real caller scalar type")
        };
        assert_eq!(
            module.type_kind(int_type),
            Some(&SsaTypeKind::Integer {
                bits: 32,
                signed: true
            })
        );
        let [measure_entry] = measure.blocks.as_slice() else {
            panic!("linear measure")
        };
        let [EntityId::Loan(measure_arg)] = measure_entry.parameters.as_slice() else {
            panic!("real measure ABI")
        };
        assert_eq!(
            measure.entity(EntityId::Loan(*measure_arg)).unwrap().ty,
            shared
        );
        let [one] = measure.instructions.as_slice() else {
            panic!("exact body of measure")
        };
        assert_eq!(
            one.operation,
            Operation::Constant(ScalarConstant::Integer(1))
        );
        let [EntityId::Value(returned)] = one.results.as_slice() else {
            panic!("actual returned 1")
        };
        assert_eq!(
            measure.entity(EntityId::Value(*returned)).unwrap().ty,
            EntityType::Value(int_type)
        );
        assert!(matches!(&measure_entry.terminator.as_ref().unwrap().kind,
            TerminatorKind::Return { values } if values.as_slice() == [*returned]));
        assert_eq!(end_child.operation, Operation::BorrowEnd { loan: *child });
        assert_eq!(end_lender.operation, Operation::BorrowEnd { loan: *lender });
        assert_eq!(end_child.origin, measure_call.origin);
        assert_eq!(end_lender.origin, measure_call.origin);
        let Operation::DirectCall {
            callee: consumer,
            arguments,
            receiver: None,
        } = &consume.operation
        else {
            panic!("subsequent own delivery")
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
            panic!("linear consume")
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
        // The unchanged program is silent. Separately observe the existing emitted call result
        // without editing the Koven fixture or replacing/introducing a call to measure.
        super::super::unit_field_replace_tests::link_and_run(&llvm, "", b"");
        let native_symbol = format!("f{}.{}", measure.id.index(), measure.name);
        let observed = watch_i32_result(&llvm, &native_symbol);
        super::super::unit_field_replace_tests::link_and_run(
            &observed,
            r#"
#include <assert.h>
#include <stdint.h>
#include <stdio.h>
void koven_test_watch_measure(int32_t value) {
    assert(value == 1);
    puts("measure=1");
}
"#,
            b"measure=1\n",
        );
    }
}

fn watch_i32_result(llvm: &str, callee: &str) -> String {
    let marker = format!("call i32 @{callee}(");
    let mut observed = String::with_capacity(llvm.len() + 100);
    let mut sites = 0;
    for line in llvm.lines() {
        observed.push_str(line);
        observed.push('\n');
        if let Some((destination, call)) = line.trim().split_once(" = ")
            && call.starts_with(&marker)
        {
            assert!(destination.starts_with('%'));
            observed.push_str(&format!(
                "  call void @koven_test_watch_measure(i32 {destination})\n"
            ));
            sites += 1;
        }
    }
    assert_eq!(
        sites, 1,
        "find the actual existing measure call, not its definition"
    );
    observed.push_str("\ndeclare void @koven_test_watch_measure(i32)\n");
    observed
}
