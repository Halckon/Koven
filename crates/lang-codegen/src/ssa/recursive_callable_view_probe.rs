//! The concrete callable auxiliary is a protected BorrowCall operand, never a place of view type.
use crate::ssa::{
    model::{
        ClosureCaptureOperand, EntityId, EntityType, LoanKind, Operation, PlaceAccess, SsaTypeKind,
    },
    unit_lower_test_support,
};
use lang_frontend::{
    name_resolution::SourceUnitInput,
    source::SourceMap,
    type_checking::{complete_compilation_unit_types, standard_environments},
};

// Exact original frontend regression source; no wrapper edits or newline drift.
const SOURCE: &str = r#"class Boxed(val value: Int)
fun view(source: Boxed, aux: () -> Int): borrow Int from source = source.value
fun observe(value: Int?): Unit {}
fun run() { val boxed = Boxed(7); val aux: () -> Int = { boxed.value }; observe(view(boxed, aux)) }"#;

#[test]
fn recursive_callable_auxiliary_preserves_exact_shared_lifetimes_and_native_release() {
    assert_eq!(SOURCE.len(), 240);
    assert_eq!(&SOURCE[233..236], "aux");
    let mut sources = SourceMap::new();
    let (source, parsed) =
        unit_lower_test_support::parsed(&mut sources, "recursive_callable.ko", SOURCE);
    let (environment, types) = standard_environments();
    for input in [
        SourceUnitInput::new("root", "recursive_callable.ko", source, &parsed),
        SourceUnitInput::memory_document(source, &parsed),
    ] {
        let inputs = [input];
        let (names, typed, owned) =
            unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
        complete_compilation_unit_types(&sources, &inputs, &names, &types, typed.types()).unwrap();
        let entry = names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|d| d.name() == "run")
            .unwrap()
            .id();
        let (program, entry) = crate::ssa::unit_lower::lower_scalar_unit_with_entry(
            &sources, &inputs, &names, &types, &typed, &owned, entry,
        )
        .expect("the verified original BorrowCall must lower");
        crate::ssa::verify::verify_program(&program).unwrap();
        let module = program.module(entry.module()).unwrap();
        let run = module.function(entry).unwrap();
        assert_eq!(run.blocks.len(), 1);
        let one = |matches: &dyn Fn(&Operation) -> bool| {
            let mut matches = run
                .instructions
                .iter()
                .enumerate()
                .filter(|(_, i)| matches(&i.operation));
            let selected = matches.next().expect("actual instruction");
            assert!(matches.next().is_none());
            selected
        };
        let (allocate, allocation) = one(&|op| matches!(op, Operation::HeapAllocate { .. }));
        let [EntityId::Value(boxed)] = allocation.results.as_slice() else {
            panic!("owned Boxed")
        };
        let (form, construction) = one(&|op| matches!(op, Operation::ClosureConstruct { .. }));
        let Operation::ClosureConstruct {
            closure, captures, ..
        } = &construction.operation
        else {
            unreachable!()
        };
        let [ClosureCaptureOperand::Shared(capture)] = captures.as_slice() else {
            panic!("one original capture loan")
        };
        let [EntityId::Value(aux)] = construction.results.as_slice() else {
            panic!("owned closure")
        };
        assert!(matches!(
            module.type_kind(*closure),
            Some(SsaTypeKind::ConcreteClosure { .. })
        ));
        let (adapt, adaptation) = one(&|op| matches!(op, Operation::CallableViewOf { .. }));
        let Operation::CallableViewOf { source: aux_loan } = adaptation.operation else {
            unreachable!()
        };
        let [EntityId::Loan(view)] = adaptation.results.as_slice() else {
            panic!("view is a loan")
        };
        assert_eq!(sources.slice(adaptation.origin.span()).unwrap(), "aux");
        let (borrow, call) = one(&|op| matches!(op, Operation::BorrowCall { .. }));
        let Operation::BorrowCall {
            arguments,
            source: lender,
            ..
        } = &call.operation
        else {
            unreachable!()
        };
        assert_eq!(
            sources.slice(call.origin.span()).unwrap(),
            "view(boxed, aux)"
        );
        assert_eq!(
            arguments.as_slice(),
            [EntityId::Loan(*lender), EntityId::Loan(*view)]
        );
        let [EntityId::Loan(result)] = call.results.as_slice() else {
            panic!("actual borrowed Int result")
        };

        let begin = |loan, owner| {
            let (at, begin) = run
                .instructions
                .iter()
                .enumerate()
                .find(|(_, i)| i.results == [EntityId::Loan(loan)])
                .unwrap();
            let Operation::BorrowBegin {
                place,
                kind: LoanKind::Shared,
            } = begin.operation
            else {
                panic!("rooted Shared loan")
            };
            let (root, source) = run
                .instructions
                .iter()
                .enumerate()
                .find(|(_, i)| i.results == [EntityId::Place(place)])
                .unwrap();
            assert_eq!(source.operation, Operation::RootPlace { owner });
            assert!(root < at);
            (at, place)
        };
        let (capture_at, _) = begin(*capture, *boxed);
        let (lender_at, _) = begin(*lender, *boxed);
        let (aux_at, aux_place) = begin(aux_loan, *aux);
        assert_ne!(capture, lender);
        assert_eq!(
            run.entity(EntityId::Place(aux_place)).unwrap().ty,
            EntityType::Place(*closure)
        );
        assert!(allocate < capture_at && capture_at < form && form < lender_at);
        assert!(lender_at < aux_at && aux_at < adapt && adapt < borrow);

        let end = |loan| one(&|op| *op == Operation::BorrowEnd { loan }).0;
        let drop = |owner| one(&|op| *op == Operation::Drop { owner }).0;
        let read = one(&|op| matches!(op, Operation::Read { source: PlaceAccess::Loan(loan) } if loan == result)).0;
        assert!(borrow < end(*view) && end(*view) < end(aux_loan));
        assert!(end(aux_loan) < drop(*aux) && drop(*aux) < read);
        assert!(read < end(*result) && end(*result) < end(*lender));
        assert!(end(*lender) < drop(*boxed));

        let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
        let native =
            crate::native_tests::boxed_enum_tests::run_counted_allocations_in_order(&llvm, &[0]);
        crate::native_tests::boxed_enum_tests::assert_success(&native, b"");
    }
}
