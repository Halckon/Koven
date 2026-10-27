//! E2 Stage A: retain the original single-file oracle and compare its exact source via Unit.
use lang_frontend::{
    lexer::lex,
    name_resolution::{SymbolKind, resolve_names},
    ownership_checking::check_ownership,
    parser::parse_file,
    source::SourceMap,
    type_checking::{check_types, standard_environments},
};

use super::{
    lower_frontend::orchestrate::lower_scalar_file,
    model::{
        CheckedArithmeticOperator, EntityId, EntityType, Function, LoanKind, Operation, Origin,
        PlaceAccess, ScalarConstant, TerminatorKind, ValueId,
    },
    render::render_program,
    verify::verify_program,
};
use crate::{
    llvm::render_verified_program, native_tests::boxed_enum_tests::unit_source::UnitSource,
};

// Byte-for-byte source from lowers_straight_line_blocks_locals_assignments_and_returns.
// Do not add own, a package, a wrapper, or a synthetic main to this comparison.
const SOURCE: &str = "fun compute(input: Int): Int {\n\
             val doubled: Int = input + input\n\
             var total: Int = doubled\n\
             { total += 1 }\n\
             return total\n\
         }\n\
         fun observe(input: Int): Unit {\n\
             val local: Int = input\n\
         }";

#[test]
fn compares_original_straight_line_roots_through_strict_memory_document_unit() {
    // Run the preserved legacy route with the same bytes, not a rewritten fixture.
    let mut sources = SourceMap::new();
    let source = sources.add_source("lowering.ko", SOURCE).expect("source");
    let lexed = lex(&sources, source).expect("lexing");
    let parsed = parse_file(&sources, &lexed).expect("parsing");
    let (environment, types) = standard_environments();
    let names = resolve_names(&sources, &parsed, &environment).expect("names");
    let typed = check_types(&sources, &parsed, &names, &types).expect("types");
    let owned = check_ownership(&sources, &parsed, &names, &typed).expect("ownership");
    assert!(parsed.diagnostics().is_empty());
    assert!(names.diagnostics().is_empty());
    assert!(typed.diagnostics().is_empty());
    assert!(owned.diagnostics().is_empty());
    let legacy = lower_scalar_file(&sources, &parsed, &names, &typed, &owned)
        .expect("original straight-line scalar blocks lower");
    verify_program(&legacy).expect("legacy SSA and ownership verify");
    let rendered = render_program(&legacy);
    assert!(rendered.contains("func \"compute\""));
    assert!(rendered.contains("func \"observe\""));
    assert_eq!(rendered.matches("checked.add").count(), 2);
    assert_eq!(rendered.matches("abort @source").count(), 2);
    render_verified_program(&legacy).expect("legacy LLVM verifies");

    // UnitSource uses MemoryDocument, strict final admission, and real ownership products.
    let unit = UnitSource::analyze("lowering.ko", SOURCE).expect("strict original Unit source");
    unit.assert_frontend_clean();
    assert_eq!(
        unit.sources().source_text(unit.parsed().source_id()),
        Ok(SOURCE)
    );
    assert_eq!(sources.source_text(source), Ok(SOURCE));
    let mut counts = Vec::new();
    for (name, expected) in [("compute", 2), ("observe", 0)] {
        let declaration = unit.declaration(name, SymbolKind::Function);
        let span = unit.declaration_span(declaration);
        let origin = Origin::Source(span);
        let (program, entry) = unit
            .lower_entry(declaration)
            .expect("real declaration root");
        verify_program(&program).expect("Unit SSA including ownership verifies");
        let module = program.module(entry.module()).expect("entry module");
        let function = module.function(entry).expect("selected source function");
        assert_eq!(function.origin, origin);
        assert_eq!(
            module
                .functions
                .iter()
                .filter(|f| f.origin == origin)
                .count(),
            1
        );
        let original = legacy
            .modules
            .iter()
            .flat_map(|m| &m.functions)
            .find(|f| f.name == name)
            .expect("original named declaration");
        assert_eq!(checked_count(original), expected);
        assert_eq!(abort_count(original), expected);
        assert_eq!(checked_count(function), expected);
        assert_eq!(abort_count(function), expected);
        counts.push(abort_count(function));

        let [entry_loan @ EntityId::Loan(_)] = function.blocks[0].parameters.as_slice() else {
            panic!("unchanged default Int parameter must remain Borrow");
        };
        assert!(matches!(
            function.entity(*entry_loan).expect("entry loan").ty,
            EntityType::Loan {
                kind: LoanKind::Shared,
                ..
            }
        ));
        for instruction in &function.instructions {
            if matches!(instruction.operation, Operation::CheckedArithmetic { .. }) {
                let Origin::Source(operation_span) = instruction.origin else {
                    panic!("checked operation has original source provenance");
                };
                assert_eq!(operation_span.source_id(), span.source_id());
                assert!(
                    span.start() <= operation_span.start() && operation_span.end() <= span.end()
                );
            }
        }
        if name == "compute" {
            assert_compute(function, *entry_loan);
        } else {
            assert!(function.return_types.is_empty());
            assert!(function.instructions.iter().any(|instruction| matches!(
                instruction.operation,
                Operation::Read { source: PlaceAccess::Loan(loan) } if EntityId::Loan(loan) == *entry_loan
            )), "observe must read its input for the original local binding");
            assert_eq!(returns(function), [Vec::<ValueId>::new()]);
        }
        let ssa = render_program(&program);
        let llvm = render_verified_program(&program).expect("original Unit root LLVM verifies");
        let (repeated, repeated_entry) = unit.lower_entry(declaration).expect("repeat same root");
        verify_program(&repeated).expect("repeated SSA and ownership verify");
        // FunctionId carries a program-local owner; compare stable indices across runs.
        assert_eq!(entry.module().index(), repeated_entry.module().index());
        assert_eq!(entry.index(), repeated_entry.index());
        assert_eq!(function.id, entry);
        let repeated_module = repeated
            .module(repeated_entry.module())
            .expect("repeated entry module belongs to repeated program");
        let repeated_function = repeated_module
            .function(repeated_entry)
            .expect("repeated selected source function");
        assert_eq!(repeated_function.id, repeated_entry);
        assert_eq!(repeated_function.origin, origin);
        assert_eq!(
            repeated_module
                .functions
                .iter()
                .filter(|f| f.origin == origin)
                .count(),
            1,
            "repeated root retains unique original declaration identity"
        );
        assert_eq!(ssa, render_program(&repeated));
        assert_eq!(
            llvm,
            render_verified_program(&repeated).expect("repeated LLVM verifies")
        );
    }
    assert_eq!(counts, [2, 0]);
}

fn checked_count(function: &Function) -> usize {
    function
        .instructions
        .iter()
        .filter(|instruction| matches!(instruction.operation, Operation::CheckedArithmetic { .. }))
        .count()
}

fn abort_count(function: &Function) -> usize {
    function
        .blocks
        .iter()
        .filter(|block| {
            block
                .terminator
                .as_ref()
                .is_some_and(|terminator| matches!(terminator.kind, TerminatorKind::Abort))
        })
        .count()
}

fn returns(function: &Function) -> Vec<Vec<ValueId>> {
    function
        .blocks
        .iter()
        .filter_map(|block| match &block.terminator.as_ref()?.kind {
            TerminatorKind::Return { values } => Some(values.clone()),
            _ => None,
        })
        .collect()
}

fn producer(function: &Function, mut value: ValueId) -> &Operation {
    // Follow finite Copy chains so assertions express source dataflow, not copy counts.
    for _ in 0..=function.instructions.len() {
        let instruction = function
            .instructions
            .iter()
            .find(|instruction| instruction.results.contains(&EntityId::Value(value)))
            .expect("value has an instruction producer");
        if let Operation::Copy { source } = instruction.operation {
            value = source;
        } else {
            return &instruction.operation;
        }
    }
    panic!("cyclic Copy chain");
}

fn assert_compute(function: &Function, entry_loan: EntityId) {
    let checked = function
        .instructions
        .iter()
        .filter(|instruction| matches!(instruction.operation, Operation::CheckedArithmetic { .. }))
        .collect::<Vec<_>>();
    assert_eq!(checked.len(), 2);
    let mut current_loan = entry_loan;
    let mut current_block = function.blocks[0].id;
    for instruction in &checked {
        assert!(matches!(
            instruction.operation,
            Operation::CheckedArithmetic {
                operator: CheckedArithmeticOperator::Add,
                ..
            }
        ));
        assert_eq!(
            instruction.block, current_block,
            "ordered straight-line success chain"
        );
        let [EntityId::Value(_), EntityId::Value(failure)] = instruction.results.as_slice() else {
            panic!("checked add produces its result and failure flag");
        };
        let block = function.block(current_block).expect("checked block");
        assert_eq!(block.instructions.last(), Some(&instruction.id));
        let TerminatorKind::Conditional {
            condition,
            when_true,
            when_false,
        } = &block.terminator.as_ref().expect("checked terminator").kind
        else {
            panic!("checked add ends with failure conditional");
        };
        assert_eq!(condition, failure);
        assert_ne!(when_true.target, when_false.target);
        // The second add carries the first success block's rebound loan, not entry IDs.
        assert_eq!(when_true.arguments, [current_loan]);
        assert_eq!(when_false.arguments, [current_loan]);
        for edge in [when_true, when_false] {
            let target = function.block(edge.target).expect("target");
            assert_eq!(target.parameters.len(), 1);
            assert!(matches!(target.parameters[0], EntityId::Loan(_)));
            assert_eq!(
                function
                    .entity(target.parameters[0])
                    .expect("target loan")
                    .ty,
                function.entity(current_loan).expect("current loan").ty
            );
        }
        let abort = function
            .block(when_true.target)
            .expect("abort block")
            .terminator
            .as_ref()
            .expect("abort terminator");
        assert!(matches!(abort.kind, TerminatorKind::Abort));
        assert_eq!(
            abort.origin, instruction.origin,
            "exact checked source Abort"
        );
        current_block = when_false.target;
        current_loan = function
            .block(current_block)
            .expect("success block")
            .parameters[0];
    }
    let Operation::CheckedArithmetic { left, right, .. } = checked[0].operation else {
        unreachable!()
    };
    for operand in [left, right] {
        assert!(
            matches!(producer(function, operand),
                Operation::Read { source: PlaceAccess::Loan(loan) } if EntityId::Loan(*loan) == entry_loan
            ),
            "doubled reads the unchanged Borrow input twice"
        );
    }
    let Operation::CheckedArithmetic { left, right, .. } = checked[1].operation else {
        unreachable!()
    };
    assert!(
        std::ptr::eq(producer(function, left), &checked[0].operation),
        "nested total += 1 uses doubled through the mutable local"
    );
    assert!(matches!(
        producer(function, right),
        Operation::Constant(ScalarConstant::Integer(1))
    ));
    let result = returns(function);
    let [values] = result.as_slice() else {
        panic!("one successful return");
    };
    let [value] = values.as_slice() else {
        panic!("compute returns one Int");
    };
    assert!(
        std::ptr::eq(producer(function, *value), &checked[1].operation),
        "return observes the nested assignment, not stale doubled"
    );
    assert!(matches!(
        function
            .block(current_block)
            .expect("final success")
            .terminator
            .as_ref()
            .expect("return")
            .kind,
        TerminatorKind::Return { .. }
    ));
}
