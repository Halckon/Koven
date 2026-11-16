//! Same original readonly receiver source, strict Unit identity/lifetime proof.
//! The caller independently preserves its legacy rejection; no production changes.
use super::*;

pub(super) fn assert_unit(text: &str) {
    let source = format!("package app\n{text}");
    let analysis = UnitSource::analyze("app/readonly-method.ko", &source).unwrap();
    analysis.assert_frontend_clean();
    assert_eq!(analysis.ownership().borrow_results().bindings().len(), 1);
    let entry_decl = analysis.declaration("entry", SymbolKind::Function);
    let view_decl = analysis.declaration("view", SymbolKind::Function);
    let entry_span = analysis.declaration_span(entry_decl);
    assert_eq!(
        analysis
            .sources()
            .source_text(entry_span.source_id())
            .unwrap(),
        source
    );
    let (program, entry_id) = analysis.lower_entry(entry_decl).unwrap();
    verify_program(&program).unwrap();
    let module = program.module(entry_id.module()).unwrap();
    let entry = module.function(entry_id).unwrap();
    assert_eq!(entry.origin, Origin::Source(entry_span));
    let at = |fragment| {
        let found = entry
            .instructions
            .iter()
            .filter(|i| analysis.sources().slice(i.origin.span()).unwrap() == fragment)
            .collect::<Vec<_>>();
        let [instruction] = found.as_slice() else {
            panic!("unique actual instruction for {fragment}")
        };
        *instruction
    };
    let construct = at("Item(7)");
    let Operation::AggregateConstruct { aggregate, .. } = construct.operation else {
        panic!("actual Item aggregate")
    };
    let [EntityId::Value(owner)] = construct.results.as_slice() else {
        panic!("Item value")
    };
    let roots = entry.instructions.iter().filter(|i| matches!(i.operation, Operation::RootPlace { owner: candidate } if candidate == *owner)).collect::<Vec<_>>();
    let [root] = roots.as_slice() else {
        panic!("unique place for original Item")
    };
    let [EntityId::Place(place)] = root.results.as_slice() else {
        panic!("Item place")
    };
    let begins = entry.instructions.iter().filter(|i| matches!(i.operation, Operation::BorrowBegin { place: candidate, kind: LoanKind::Shared } if candidate == *place)).collect::<Vec<_>>();
    let [begin] = begins.as_slice() else {
        panic!("unique original shared loan")
    };
    let [EntityId::Loan(source_loan)] = begin.results.as_slice() else {
        panic!("source loan")
    };
    let call = at("view(item)");
    let Operation::BorrowCall {
        callee,
        arguments,
        source: actual_source,
    } = &call.operation
    else {
        panic!("actual borrow-result call")
    };
    assert_eq!(*actual_source, *source_loan);
    assert_eq!(arguments, &[EntityId::Loan(*source_loan)]);
    let view = module.function(*callee).unwrap();
    assert_eq!(
        view.origin,
        Origin::Source(analysis.declaration_span(view_decl))
    );
    assert_eq!(view.borrow_return, Some(0));
    let [EntityId::Loan(result_loan)] = call.results.as_slice() else {
        panic!("borrowed result")
    };
    assert_ne!(source_loan, result_loan);
    for loan in [*source_loan, *result_loan] {
        assert_eq!(
            entry.entity(EntityId::Loan(loan)).unwrap().ty,
            EntityType::Loan {
                kind: LoanKind::Shared,
                target: aggregate
            }
        );
    }
    let read = at("alias.read()");
    let Operation::DirectCall {
        callee,
        receiver,
        arguments,
    } = &read.operation
    else {
        panic!("actual readonly method call")
    };
    assert_eq!(*receiver, Some(EntityId::Loan(*result_loan)));
    assert!(arguments.is_empty());
    let method = module.function(*callee).unwrap();
    assert_eq!(
        analysis.sources().slice(method.origin.span()).unwrap(),
        "fun read():Int=this.n"
    );
    assert_eq!(
        method.receiver,
        Some(EntityType::Loan {
            kind: LoanKind::Shared,
            target: aggregate
        })
    );
    let end = |loan| {
        let ends = entry.instructions.iter().filter(|i| matches!(i.operation, Operation::BorrowEnd { loan: candidate } if candidate == loan)).collect::<Vec<_>>();
        let [end] = ends.as_slice() else {
            panic!("unique end for actual loan")
        };
        *end
    };
    let result_end = end(*result_loan);
    let source_end = end(*source_loan);
    let ordered = [construct, *root, *begin, call, read, result_end, source_end];
    let block = entry.block(read.block).unwrap();
    let mut previous = None;
    for instruction in ordered {
        assert_eq!(instruction.block, block.id);
        let position = block
            .instructions
            .iter()
            .position(|id| *id == instruction.id)
            .unwrap();
        assert!(previous.is_none_or(|previous| previous < position));
        previous = Some(position);
    }
    assert!(!entry.instructions.iter().any(|i| matches!(
        i.operation,
        Operation::Copy { .. } | Operation::SharedRetain { .. }
    )));
    crate::llvm::render_verified_program_with_entry(&program, entry_id).unwrap();
}
