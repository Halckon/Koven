//! Ordinary borrowed-for/nullable-while first-red candidates; no fabricated facts.
use crate::test_support::diagnostics::{diagnostic, expect};
use lang_frontend::{
    lexer::lex,
    parser::{Expression, Item, NameMarker, Statement, parse_file},
    source::{SourceMap, Span},
};

use super::{
    collect_memory_entry_observations, collect_shape_boundaries, collect_sources, holder,
    verify_program,
};

#[path = "borrowed_proof_contract.rs"]
mod borrowed_proof_contract;
#[path = "outer_borrowed_proof_contract.rs"]
mod outer_borrowed_proof_contract;

struct Spans {
    outer: Span,
    outer_condition: Span,
    borrowed: Span,
    construct: Span,
    branch: Span,
    continued: Span,
    bindings: [Span; 2],
    members: [Span; 2],
}

struct OuterSpans {
    outer: Span,
    inner: Span,
    inner_condition: Span,
    construct: Span,
    bindings: [Span; 2],
    member: Span,
    items: [Span; 2],
    additions: [Span; 3],
    broken: Span,
    returned: Span,
}

pub(in crate::ssa) fn borrowed_for_inside_while_source() -> String {
    holder(
        "val items = listOf(1, 2)\nwhile (current != null) {\nfor (item in items) { if (item == 1) { continue }\ntotal = total + item + current.value }\nreturn total + current.value\n}\nreturn total",
    )
}

pub(in crate::ssa) fn while_inside_borrowed_for_source() -> String {
    holder(
        "val items = listOf(1, 2)\nfor (item in items) {\nwhile (current != null) { total = total + item + current.value\nbreak }\ntotal = total + item\n}\nreturn total",
    )
}

pub(in crate::ssa) fn borrowed_for_inside_while_sources() -> [String; 2] {
    let source = borrowed_for_inside_while_source();
    let grouped = source.replace("in items", "in ((items))");
    [source, grouped]
}

pub(in crate::ssa) fn while_inside_borrowed_for_sources() -> [String; 2] {
    let source = while_inside_borrowed_for_source();
    let grouped = source.replace("in items", "in ((items))");
    [source, grouped]
}

#[test]
fn borrowed_for_preserves_the_outer_nullable_while_proof() {
    let sources = borrowed_for_inside_while_sources();
    let memory = collect_memory_entry_observations(&sources);
    observe_borrowed_sources("for-inside-while", &borrowed_for_inside_while_sources());
    assert_eq!(memory.len(), 2);
    for (case, result) in memory.into_iter().enumerate() {
        let source = expect(
            result,
            "original-layout Unit borrowed proof reaches SSA and LLVM",
        );
        let spans = source_spans(&sources[case], false);
        borrowed_proof_contract::assert_borrowed_proof(&source.program, &spans, true);
        diagnostic!("SHAPE4 MEMORY borrowed case={case} oracle=ok");
    }
}

#[test]
fn nullable_while_preserves_the_outer_borrowed_for_access() {
    observe_borrowed_sources("while-inside-for", &while_inside_borrowed_for_sources());
}

fn observe_borrowed_sources(relation: &str, sources: &[String; 2]) {
    let observations = if relation == "for-inside-while" {
        collect_shape_boundaries(sources)
    } else {
        assert_eq!(relation, "while-inside-for");
        collect_sources(sources)
    };
    let outcomes = observations
        .into_iter()
        .enumerate()
        .map(|(index, result)| {
            let result = result.and_then(|program| {
                verify_program(&program).map_err(|error| format!("SSA verify: {error:?}"))?;
                let llvm = crate::llvm::render_verified_program(&program)
                    .map_err(|error| format!("LLVM: {error:?}"))?;
                diagnostic!("BORROWED PROOF LLVM relation={relation} case={index} {llvm}");
                Ok(program)
            });
            diagnostic!(
                "BORROWED PROOF OUTCOME relation={relation} source={} unit={} {:?}",
                index / 2,
                index % 2 == 1,
                result.as_ref().map(|_| ()),
            );
            result
        })
        .collect::<Vec<_>>();
    // Retain all original/Group single/unit verify/render outcomes before asserting success.
    assert_eq!(outcomes.len(), 4);
    for (index, outcome) in outcomes.into_iter().enumerate() {
        let program = expect(
            outcome,
            "ordinary borrowed for and nullable while preserve enclosing access",
        );
        if relation == "for-inside-while" {
            let unit = index % 2 == 1;
            let spans = source_spans(&sources[index / 2], unit);
            borrowed_proof_contract::assert_borrowed_proof(&program, &spans, unit);
        } else {
            assert_eq!(relation, "while-inside-for");
            let spans = outer_source_spans(&sources[index / 2], index % 2 == 1);
            outer_borrowed_proof_contract::assert_outer_borrowed(&program, &spans);
        }
    }
}

fn source_spans(source: &str, unit: bool) -> Spans {
    let text = if unit {
        format!("package p\n{source}")
    } else {
        source.to_owned()
    };
    let mut sources = SourceMap::new();
    let id = sources
        .add_source(if unit { "p/range.ko" } else { "range.ko" }, &text)
        .unwrap();
    let lexed = lex(&sources, id).unwrap();
    assert!(lexed.diagnostics().is_empty());
    let file = parse_file(&sources, &lexed).unwrap();
    assert!(file.diagnostics().is_empty());
    let ast = file.ast();
    let (outer, outer_condition) = only(ast.statements().iter().filter_map(|(_, node)| {
        let Statement::While { mut condition, .. } = *node.payload() else {
            return None;
        };
        while let Expression::Group { expression } =
            ast.expressions().get(condition).unwrap().payload()
        {
            condition = *expression;
        }
        Some((
            node.span(),
            ast.expressions().get(condition).unwrap().span(),
        ))
    }));
    let (borrowed, mut source) = only(ast.statements().iter().filter_map(|(_, node)| {
        let Statement::For { source, .. } = node.payload() else {
            return None;
        };
        Some((node.span(), *source))
    }));
    while let Expression::Group { expression } = ast.expressions().get(source).unwrap().payload() {
        source = *expression;
    }
    let source = ast.expressions().get(source).unwrap();
    assert!(matches!(source.payload(), Expression::Name));
    assert_eq!(&text[source.span().start()..source.span().end()], "items");
    let bindings = ["current", "items"].map(|expected| {
        only(ast.items().iter().filter_map(|(_, node)| {
            let Item::Variable {
                name: NameMarker::Present(span),
                ..
            } = node.payload()
            else {
                return None;
            };
            (&text[span.start()..span.end()] == expected).then_some(*span)
        }))
    });
    let expression = |predicate: fn(&Expression) -> bool| {
        only(
            ast.expressions()
                .iter()
                .filter_map(|(_, node)| predicate(node.payload()).then_some(node.span())),
        )
    };
    let construct = expression(|node| matches!(node, Expression::Call { .. }));
    let branch = expression(|node| matches!(node, Expression::If { .. }));
    let continued = expression(|node| matches!(node, Expression::Continue { .. }));
    let mut members = ast
        .expressions()
        .iter()
        .filter_map(|(_, node)| {
            matches!(node.payload(), Expression::Member { .. }).then_some(node.span())
        })
        .collect::<Vec<_>>();
    members.sort_by_key(|span| span.start());
    let [inner, final_read] = members.as_slice() else {
        panic!("two real Members")
    };
    Spans {
        outer,
        outer_condition,
        borrowed,
        construct,
        branch,
        continued,
        bindings,
        members: [*inner, *final_read],
    }
}

fn only<T>(mut values: impl Iterator<Item = T>) -> T {
    let value = values.next().expect("one real source node");
    assert!(values.next().is_none(), "source node is unique");
    value
}

fn outer_source_spans(source: &str, unit: bool) -> OuterSpans {
    let text = if unit {
        format!("package p\n{source}")
    } else {
        source.to_owned()
    };
    let mut sources = SourceMap::new();
    let id = sources.add_source("outer-borrowed.ko", &text).unwrap();
    let lexed = lex(&sources, id).unwrap();
    assert!(lexed.diagnostics().is_empty());
    let file = parse_file(&sources, &lexed).unwrap();
    assert!(file.diagnostics().is_empty());
    let ast = file.ast();
    let (inner, inner_condition) = only(ast.statements().iter().filter_map(|(_, node)| {
        let Statement::While { mut condition, .. } = *node.payload() else {
            return None;
        };
        while let Expression::Group { expression } =
            ast.expressions().get(condition).unwrap().payload()
        {
            condition = *expression;
        }
        Some((
            node.span(),
            ast.expressions().get(condition).unwrap().span(),
        ))
    }));
    let (outer, mut source) = only(ast.statements().iter().filter_map(|(_, node)| {
        let Statement::For { source, .. } = node.payload() else {
            return None;
        };
        Some((node.span(), *source))
    }));
    assert!(outer.start() < inner.start() && inner.end() < outer.end());
    while let Expression::Group { expression } = ast.expressions().get(source).unwrap().payload() {
        source = *expression;
    }
    let source = ast.expressions().get(source).unwrap();
    assert!(matches!(source.payload(), Expression::Name));
    assert_eq!(&text[source.span().start()..source.span().end()], "items");
    let bindings = ["current", "items"].map(|expected| {
        only(ast.items().iter().filter_map(|(_, node)| {
            let Item::Variable {
                name: NameMarker::Present(span),
                ..
            } = node.payload()
            else {
                return None;
            };
            (&text[span.start()..span.end()] == expected).then_some(*span)
        }))
    });
    let expression = |predicate: fn(&Expression) -> bool| {
        only(
            ast.expressions()
                .iter()
                .filter_map(|(_, node)| predicate(node.payload()).then_some(node.span())),
        )
    };
    let construct = expression(|node| matches!(node, Expression::Call { .. }));
    let member = expression(|node| matches!(node, Expression::Member { .. }));
    let broken = expression(|node| matches!(node, Expression::Break { .. }));
    let returned = expression(|node| matches!(node, Expression::Return { .. }));
    let mut items = ast
        .expressions()
        .iter()
        .filter_map(|(_, node)| {
            (matches!(node.payload(), Expression::Name)
                && &text[node.span().start()..node.span().end()] == "item")
                .then_some(node.span())
        })
        .collect::<Vec<_>>();
    items.sort_by_key(|span| span.start());
    let [inner_item, post_item] = items.as_slice() else {
        panic!("two real item uses")
    };
    assert!(inner.start() < inner_item.start() && inner_item.end() < member.start());
    assert!(member.end() < broken.start() && broken.end() < inner.end());
    assert!(inner.end() < post_item.start() && post_item.end() < outer.end());
    assert!(outer.end() < returned.start());
    let additions = ast
        .expressions()
        .iter()
        .filter_map(|(_, node)| {
            matches!(
                node.payload(),
                Expression::Binary {
                    operator: lang_frontend::parser::BinaryOperator::Add,
                    ..
                }
            )
            .then_some(node.span())
        })
        .collect::<Vec<_>>();
    let additions: [Span; 3] = additions.try_into().expect("three source additions");
    OuterSpans {
        outer,
        inner,
        inner_condition,
        construct,
        bindings,
        member,
        items: [*inner_item, *post_item],
        additions,
        broken,
        returned,
    }
}

pub(in crate::ssa) mod rejection_diagnostics {
//! Exact negative diagnostic oracles with full failure evidence on disk.
use crate::test_support::diagnostics::{assert_diagnostic, assert_eq_diagnostic};

pub(in crate::ssa) fn assert_rejection(group: &str, source: &str, unit: bool, error: &str) {
    let offset = if unit { "package p\n".len() } else { 0 };
    let primary = |start: usize, length: usize| {
        format!(
            "primary_span: Span {{ source_id: SourceId(0), start: {}, end: {} }}",
            start + offset,
            start + offset + length
        )
    };
    if group == "pending-child" {
        assert_diagnostic!(error.starts_with("ownership"), "{error}");
        assert_eq_diagnostic!(
            error.matches("Diagnostic { severity:").count(),
            1,
            "{error}"
        );
        assert_eq_diagnostic!(error.matches("code: L0135,").count(), 1, "{error}");
        let mutation = source.find("current = null").unwrap();
        let borrowed = source.find("current.text").unwrap();
        assert_diagnostic!(
            error.contains(&primary(mutation, "current".len())),
            "{error}"
        );
        let label = format!(
            "Label(DiagnosticLabel {{ span: Span {{ source_id: SourceId(0), start: {}, end: {} }}, message: DiagnosticText(\"conflicting loan starts here\") }})",
            borrowed + offset,
            borrowed + offset + "current.text".len()
        );
        assert_diagnostic!(error.contains(&label), "{error}");
    } else {
        assert_diagnostic!(error.starts_with("complete:"), "{error}");
        let count = if group == "same-root" { 2 } else { 1 };
        assert_eq_diagnostic!(
            error.matches("Diagnostic { severity:").count(),
            count,
            "{error}"
        );
        assert_eq_diagnostic!(error.matches("code: L0165,").count(), count, "{error}");
        for (start, _) in source.rmatch_indices("current.value").take(count) {
            assert_diagnostic!(
                error.contains(&primary(start, "current.value".len())),
                "{error}"
            );
        }
    }
}
}

pub(in crate::ssa) mod owned_field_contract {
//! Query-only synchronous owned-local field loan identities.
use crate::ssa::model::{
    EntityId, EntityType, LoanKind, Operation, PlaceAccess, Program, SsaTypeKind,
};

pub(in crate::ssa) fn assert_field_loan(program: &Program) {
    let module = program.modules.first().expect("one module");
    assert_eq!(program.modules.len(), 1);
    let functions = module
        .functions
        .iter()
        .filter(|function| {
            function.name.contains("entry")
                && function.instructions.iter().any(|instruction| {
                    matches!(
                        instruction.operation,
                        Operation::HeapPayloadPlace { .. } | Operation::SharedHeapFieldLoan { .. }
                    )
                })
        })
        .collect::<Vec<_>>();
    assert_eq!(functions.len(), 1, "the entry's ordinary owned field loan");
    let function = functions[0];
    if let Some(shared_loan) = function
        .instructions
        .iter()
        .find(|instruction| matches!(instruction.operation, Operation::SharedHeapFieldLoan { .. }))
    {
        let Operation::SharedHeapFieldLoan { base: _, field } = shared_loan.operation else {
            unreachable!()
        };
        assert_eq!(field, 0);
        let EntityId::Loan(loan) = only(shared_loan.results.iter().copied()) else {
            panic!("shared field loan")
        };
        let EntityType::Loan { kind, target } = function.entity(EntityId::Loan(loan)).unwrap().ty
        else {
            panic!("loan entity")
        };
        assert_eq!(kind, LoanKind::Shared);
        assert!(matches!(
            module.type_kind(target),
            Some(SsaTypeKind::StringOwner)
        ));
        let read = only(function.instructions.iter().filter(|instruction| matches!(instruction.operation, Operation::PrintString { value } if value == loan)));
        let end = only(function.instructions.iter().filter(|instruction| matches!(instruction.operation, Operation::BorrowEnd { loan: ended } if ended == loan)));
        let block = function.block(shared_loan.block).unwrap();
        let mut last = None;
        for instruction in [shared_loan, read, end] {
            assert_eq!(instruction.block, block.id);
            let index = block
                .instructions
                .iter()
                .position(|id| *id == instruction.id)
                .unwrap();
            if let Some(last) = last {
                assert!(last < index);
            }
            last = Some(index);
        }
        assert!(!function.instructions.iter().any(|instruction| matches!(instruction.operation, Operation::Read { source: PlaceAccess::Loan(source) } if source == loan)), "String is borrowed rather than copied out");
        return;
    }
    let root =
        only(function.instructions.iter().filter(|instruction| {
            matches!(instruction.operation, Operation::HeapPayloadPlace { .. })
        }));
    let Operation::HeapPayloadPlace { owner } = root.operation else {
        unreachable!()
    };
    let EntityType::Value(owner_type) = function.entity(EntityId::Value(owner)).unwrap().ty else {
        panic!("owned root")
    };
    let Some(SsaTypeKind::HeapOwner {
        payload: Some(payload),
        ..
    }) = module.type_kind(owner_type)
    else {
        panic!("registered heap owner")
    };
    let base = only(root.results.iter().copied());
    assert_eq!(
        function.entity(base).unwrap().ty,
        EntityType::Place(*payload)
    );
    let field = only(function.instructions.iter().filter(|instruction| matches!(instruction.operation, Operation::FieldPlace { base: operand, field: 0 } if EntityId::Place(operand) == base)));
    let field_place = only(field.results.iter().copied());
    let EntityType::Place(field_type) = function.entity(field_place).unwrap().ty else {
        panic!("field place")
    };
    assert!(matches!(
        module.type_kind(field_type),
        Some(SsaTypeKind::StringOwner)
    ));
    let begin = only(function.instructions.iter().filter(|instruction| matches!(instruction.operation, Operation::BorrowBegin { place, kind: LoanKind::Shared } if EntityId::Place(place) == field_place)));
    let EntityId::Loan(loan) = only(begin.results.iter().copied()) else {
        panic!("shared field loan")
    };
    assert_eq!(
        function.entity(EntityId::Loan(loan)).unwrap().ty,
        EntityType::Loan {
            kind: LoanKind::Shared,
            target: field_type
        }
    );
    let read = only(function.instructions.iter().filter(|instruction| matches!(instruction.operation, Operation::PrintString { value } if value == loan)));
    let end = only(function.instructions.iter().filter(|instruction| matches!(instruction.operation, Operation::BorrowEnd { loan: ended } if ended == loan)));
    let drop = only(function.instructions.iter().filter(|instruction| matches!(instruction.operation, Operation::Drop { owner: dropped } if dropped == owner)));
    let block = function.block(root.block).unwrap();
    let mut last = None;
    for instruction in [root, field, begin, read, end, drop] {
        assert_eq!(instruction.block, block.id);
        let index = block
            .instructions
            .iter()
            .position(|id| *id == instruction.id)
            .unwrap();
        if let Some(last) = last {
            assert!(last < index);
        }
        last = Some(index);
    }
    assert!(!function.instructions.iter().any(|instruction| matches!(instruction.operation, Operation::Read { source: PlaceAccess::Loan(source) } if source == loan)), "String is borrowed rather than copied out");
}
fn only<T>(mut values: impl Iterator<Item = T>) -> T {
    let value = values.next().expect("one actual identity");
    assert!(values.next().is_none(), "identity is unique");
    value
}
}

pub(in crate::ssa) mod readonly_nested_contract {
//! Query only the SSA produced by the two ordinary read-only nested sources.
use std::collections::BTreeSet;

use crate::ssa::model::{
    Block, Edge, EntityId, EntityType, LoanId, LoanKind, Operation, Origin, Program, SsaTypeKind,
    TerminatorKind,
};

/// SPEC-0356: both layouts lower `entry` through the refinement layer. The inner `while`
/// re-tests the same root with its own `NullableBranch`; the outer proof view is carried
/// across the inner loop on every exit and backs the final read.
pub(in crate::ssa) fn assert_inherited_proof(program: &Program, source: &str, unit: bool) {
    let module = only(program.modules.iter());
    let function = only(module.functions.iter().filter(|function| {
        function.blocks.iter().any(|block| {
            matches!(
                block.terminator.as_ref().map(|term| &term.kind),
                Some(TerminatorKind::NullableBranch { .. })
            )
        })
    }));
    let mut branches = function
        .blocks
        .iter()
        .filter_map(|block| {
            let terminator = block.terminator.as_ref()?;
            let TerminatorKind::NullableBranch { owner, view, .. } = &terminator.kind else {
                return None;
            };
            let Origin::Source(span) = terminator.origin else {
                return None;
            };
            Some((span.start(), *owner, *view))
        })
        .collect::<Vec<_>>();
    branches.sort_unstable();
    let [(_, outer_owner, outer_view), (_, inner_owner, inner_view)] = branches[..] else {
        panic!("the outer guard and the re-tested inner condition: {branches:?}")
    };
    let EntityType::Value(nullable) = function.entity(EntityId::Value(outer_owner)).unwrap().ty
    else {
        panic!("nullable owner")
    };
    let Some(SsaTypeKind::NullableHandle { inner }) = module.type_kind(nullable) else {
        panic!("nullable storage")
    };
    assert!(matches!(
        module.type_kind(*inner),
        Some(SsaTypeKind::HeapOwner { .. })
    ));
    assert_eq!(
        function.entity(EntityId::Value(inner_owner)).unwrap().ty,
        EntityType::Value(nullable)
    );
    let outer = traced(function, outer_view);
    let inner_loans = traced(function, inner_view);
    assert!(
        outer.is_disjoint(&inner_loans),
        "the inner test mints a fresh view"
    );
    for loan in outer.iter().chain(&inner_loans) {
        assert_eq!(
            function.entity(EntityId::Loan(*loan)).unwrap().ty,
            EntityType::Loan {
                kind: LoanKind::Shared,
                target: *inner
            }
        );
    }
    let reads = function
        .instructions
        .iter()
        .filter(|instruction| {
            matches!(
                instruction.operation,
                Operation::HeapFieldRead { field: 0, .. }
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(reads.len(), 3);
    let offset = if unit { "package p\n".len() } else { 0 };
    let receiver_at = |start: usize| {
        let read = only(reads.iter().filter(
            |read| matches!(read.origin, Origin::Source(span) if span.start() == start + offset),
        ));
        let Operation::HeapFieldRead { receiver, .. } = read.operation else {
            unreachable!()
        };
        (receiver, read)
    };
    let first = source.find("current.value").unwrap();
    let middle = first + 1 + source[first + 1..].find("current.value").unwrap();
    let last = source.rfind("current.value").unwrap();
    assert!(outer.contains(&receiver_at(first).0));
    assert!(inner_loans.contains(&receiver_at(middle).0));
    let (final_receiver, final_read) = receiver_at(last);
    assert!(
        outer.contains(&final_receiver) && final_receiver != outer_view,
        "the final read uses the outer proof carried across the inner loop"
    );
    let ends = |loans: &BTreeSet<_>| {
        function
            .instructions
            .iter()
            .filter(|instruction| {
                matches!(instruction.operation, Operation::BorrowEnd { loan } if loans.contains(&loan))
            })
            .collect::<Vec<_>>()
    };
    assert!(!ends(&inner_loans).is_empty(), "the inner view ends");
    let outer_end = only(ends(&outer).into_iter());
    assert_eq!(outer_end.block, final_read.block);
    let block = function.block(final_read.block).unwrap();
    let position = |id| {
        block
            .instructions
            .iter()
            .position(|candidate| *candidate == id)
            .unwrap()
    };
    assert!(position(final_read.id) < position(outer_end.id));
}

/// The loan and every block parameter it is passed into.
fn traced(function: &crate::ssa::model::Function, view: LoanId) -> BTreeSet<LoanId> {
    let mut loans = BTreeSet::from([view]);
    loop {
        let before = loans.len();
        for block in &function.blocks {
            for edge in edges(block) {
                let target = function.block(edge.target).unwrap();
                for (column, argument) in edge.arguments.iter().enumerate() {
                    if let EntityId::Loan(loan) = argument
                        && loans.contains(loan)
                        && let EntityId::Loan(next) = target.parameters[column]
                    {
                        loans.insert(next);
                    }
                }
            }
        }
        if loans.len() == before {
            return loans;
        }
    }
}

fn edges(block: &Block) -> Vec<&Edge> {
    match &block.terminator.as_ref().unwrap().kind {
        TerminatorKind::Branch(edge) => vec![edge],
        TerminatorKind::Conditional {
            when_true,
            when_false,
            ..
        } => vec![when_true, when_false],
        TerminatorKind::NullableBranch {
            when_null,
            when_non_null,
            ..
        } => vec![when_null, when_non_null],
        TerminatorKind::Return { .. } | TerminatorKind::Abort => Vec::new(),
        _ => panic!("ordinary owned nullable fixture has no borrowed return"),
    }
}

fn only<T>(mut values: impl Iterator<Item = T>) -> T {
    let value = values.next().expect("one actual identity");
    assert!(values.next().is_none(), "identity must be unique");
    value
}
}

pub(in crate::ssa) mod assignment_contract {
//! Read-only identities for the original integer compound source in both lowering modes.
use lang_frontend::{
    lexer::lex,
    parser::{AssignmentOperator, BinaryOperator, Expression, LiteralKind, parse_file},
    source::{SourceMap, Span},
};

use crate::ssa::model::{
    BlockId, CheckedArithmeticOperator, Definition, EntityId, EntityType, Function, Instruction,
    Operation, Origin, Ownership, Program, ScalarConstant, SsaTypeKind, TerminatorKind, ValueId,
};

pub(in crate::ssa) fn assert_compound(program: &Program, source: &str, unit: bool) {
    let [zero_span, assignment_span, sum_span, return_span] = source_spans(source, unit);
    let [module] = program.modules.as_slice() else {
        panic!("one original source module")
    };
    let [function] = module.functions.as_slice() else {
        panic!("only the actual entry is generated")
    };
    assert_eq!(function.receiver, None);
    assert_eq!(function.borrow_return, None);
    assert_eq!(function.carrier_return, None);
    let entry = function.block(function.entry_block().unwrap()).unwrap();
    let [EntityId::Value(number)] = entry.parameters.as_slice() else {
        panic!("the original own number is the sole value parameter")
    };
    let number = *number;
    let parameter = function.entity(EntityId::Value(number)).unwrap();
    let EntityType::Value(integer) = parameter.ty else {
        panic!("number is a value, not a borrowed or place identity")
    };
    assert_eq!(
        module.type_kind(integer),
        Some(&SsaTypeKind::Integer {
            bits: 32,
            signed: true
        })
    );
    assert_eq!(module.type_ownership(integer), Some(Ownership::Copyable));
    assert_eq!(function.return_types, [integer]);
    assert_eq!(
        parameter.definition,
        Definition::BlockParameter {
            block: entry.id,
            index: 0
        }
    );
    let [zero, first, second] = function.instructions.as_slice() else {
        panic!("the source has one initializer and exactly two checked additions")
    };
    assert_eq!(
        zero.operation,
        Operation::Constant(ScalarConstant::Integer(0))
    );
    assert_source(&zero.origin, zero_span, function);
    let [EntityId::Value(initial)] = zero.results.as_slice() else {
        panic!("the real initializer produces one value")
    };
    assert_eq!(
        function.entity(EntityId::Value(*initial)).unwrap().ty,
        parameter.ty
    );
    assert_eq!(zero.block, entry.id);
    assert_eq!(first.block, entry.id);
    assert_eq!(entry.instructions, [zero.id, first.id]);
    let (assigned, normal) = checked_success(function, first, *initial, number, assignment_span);
    assert_eq!(
        second.block, normal,
        "the return sum follows compound success"
    );
    assert_eq!(function.block(normal).unwrap().instructions, [second.id]);
    let (returned, done) = checked_success(function, second, assigned, number, sum_span);
    for instruction in [first, second] {
        let [value, failure] = instruction.results.as_slice() else {
            panic!("checked arithmetic has value and failure results")
        };
        assert_eq!(function.entity(*value).unwrap().ty, parameter.ty);
        let EntityType::Value(boolean) = function.entity(*failure).unwrap().ty else {
            panic!("the checked failure is a Boolean value")
        };
        assert_eq!(module.type_kind(boolean), Some(&SsaTypeKind::Boolean));
    }
    let done = function.block(done).unwrap();
    assert!(done.instructions.is_empty());
    let terminator = done.terminator.as_ref().unwrap();
    let TerminatorKind::Return { values } = &terminator.kind else {
        panic!("the second normal edge returns its actual checked result")
    };
    assert_eq!(values.as_slice(), &[returned]);
    assert_source(&terminator.origin, return_span, function);
    assert_eq!(
        function.blocks.len(),
        5,
        "entry, two failure and two success blocks"
    );
}

fn checked_success(
    function: &Function,
    instruction: &Instruction,
    left: ValueId,
    right: ValueId,
    span: Span,
) -> (ValueId, BlockId) {
    assert_eq!(
        instruction.operation,
        Operation::CheckedArithmetic {
            operator: CheckedArithmeticOperator::Add,
            left,
            right
        }
    );
    assert_source(&instruction.origin, span, function);
    let [EntityId::Value(value), EntityId::Value(failure)] = instruction.results.as_slice() else {
        panic!("the original checked addition produces value and failure identities")
    };
    let block = function.block(instruction.block).unwrap();
    let terminator = block.terminator.as_ref().unwrap();
    let TerminatorKind::Conditional {
        condition,
        when_true,
        when_false,
    } = &terminator.kind
    else {
        panic!("checked overflow is tested before the normal continuation")
    };
    assert_eq!(condition, failure);
    assert_source(&terminator.origin, span, function);
    assert_ne!(when_true.target, when_false.target);
    assert!(when_true.arguments.is_empty());
    assert!(when_false.arguments.is_empty());
    let failure_block = function.block(when_true.target).unwrap();
    assert!(failure_block.parameters.is_empty());
    assert!(failure_block.instructions.is_empty());
    let abort = failure_block.terminator.as_ref().unwrap();
    assert_eq!(abort.kind, TerminatorKind::Abort);
    assert_source(&abort.origin, span, function);
    assert!(
        function
            .block(when_false.target)
            .unwrap()
            .parameters
            .is_empty()
    );
    (*value, when_false.target)
}

fn source_spans(source: &str, unit: bool) -> [Span; 4] {
    let text = if unit {
        format!("package p\n{source}")
    } else {
        source.to_owned()
    };
    let mut sources = SourceMap::new();
    let id = sources
        .add_source(if unit { "p/range.ko" } else { "range.ko" }, &text)
        .unwrap();
    let lexed = lex(&sources, id).unwrap();
    assert!(lexed.diagnostics().is_empty());
    let parsed = parse_file(&sources, &lexed).unwrap();
    assert!(parsed.diagnostics().is_empty());
    let span = |predicate: fn(&Expression) -> bool| {
        let mut nodes = parsed
            .ast()
            .expressions()
            .iter()
            .filter_map(|(_, node)| predicate(node.payload()).then_some(node.span()));
        let span = nodes.next().expect("one real source node");
        assert!(nodes.next().is_none(), "the source identity is unique");
        span
    };
    [
        span(|node| matches!(node, Expression::Literal(LiteralKind::Integer(_)))),
        span(|node| {
            matches!(
                node,
                Expression::Assignment {
                    operator: AssignmentOperator::AddAssign,
                    ..
                }
            )
        }),
        span(|node| {
            matches!(
                node,
                Expression::Binary {
                    operator: BinaryOperator::Add,
                    ..
                }
            )
        }),
        span(|node| matches!(node, Expression::Return { .. })),
    ]
}

fn assert_source(origin: &Origin, expected: Span, function: &Function) {
    let Origin::Source(actual) = origin else {
        panic!("the original operation retains its real source origin")
    };
    // The independent parse supplies coordinates, not transferable SourceMap identities.
    assert_eq!(
        (actual.start(), actual.end()),
        (expected.start(), expected.end())
    );
    assert_eq!(actual.source_id(), function.origin.span().source_id());
}
}
