//! Whole-source regressions for the same-owner nullable field proof.
//! No native execution and no altered diagnostic fixtures or verifier contracts.
use std::collections::BTreeSet;

use crate::ssa::{
    model::{
        Block, Definition, EntityId, EntityType, FunctionId, LoanKind, Operation, Program,
        SsaTypeKind, TerminatorKind,
    },
    unit_lower::lower_owned_unit_with_entry,
    verify::verify_program,
};
use lang_frontend::{
    analysis::{
        UnitNameSnapshot, UnitSourceDescriptor, analyze_basic_unit_ownership,
        analyze_parsed_unit_names,
    },
    ast::StatementId,
    diagnostic::Severity,
    lexer::lex,
    name_resolution::{SourceUnitInput, SymbolKind},
    ownership_checking::{
        OwnedCompilationUnitView, UnitDropPoint, UnitDropTarget, owned_compilation_unit_view,
    },
    parser::{Expression, parse_file},
    source::{SourceMap, Span},
    type_checking::{
        BuiltinType, DeferredReason, FinalTypeAdmissionError, TypeEnvironment,
        UnitAggregateProjectionReceiver, UnitExpressionId, UnitTypeKind,
        check_compilation_unit_types, complete_compilation_unit_types, standard_environments,
    },
};

const ORIGINAL: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/pr120-verifier-diagnostics/p14b_nullable_simple.ko"
));
const STANDARD: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../lang-std/koven/algorithms/ranges.ko"
));

#[test]
fn original_p14b_whole_source_has_same_owner_field_proof_in_both_layouts() {
    assert_eq!(ORIGINAL.len(), 122);
    for root in ["single", "src"] {
        check_source(ORIGINAL, root, 1);
    }
}

#[test]
fn nullable_field_proof_respects_polarity_and_transparent_groups() {
    for body in [
        "val a: Node? = Node(5); if (a == null) {} else { if (a.v == 5) { println(\"smart\") } }",
        "val a: Node? = Node(5); if ((null != (a))) { if (((a).v) == (5)) { println(\"smart\") } }",
        "val a: Node? = Node(5); if (((a) == (null))) {} else { if ((5) != ((a).v)) { println(\"different\") } }",
        "val a: Node? = null; if (null == a) {} else { if (a.v == 5) { println(\"unreachable\") } }",
    ] {
        let text = format!("class Node(val v: Int)\nfun main(): Unit {{ {body} }}\n");
        for root in ["single", "src"] {
            check_source(&text, root, 1);
        }
    }
}

#[test]
fn nullable_field_proof_is_fresh_after_root_replacement() {
    let text = r#"class Node(val v: Int)
fun main(): Unit {
    var a: Node? = Node(5)
    if (a != null) { if (a.v == 5) { println("first") } }
    a = Node(6)
    if (a == null) {} else { if (a.v == 6) { println("second") } }
}
"#;
    for root in ["single", "src"] {
        let (program, entry) = check_source(text, root, 2);
        let function = program
            .module(entry.module())
            .unwrap()
            .function(entry)
            .unwrap();
        let wraps = function
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::NullableWrap { .. }))
            .map(|instruction| only(instruction.results.iter().copied()))
            .collect::<Vec<_>>();
        assert_eq!(wraps.len(), 2);
        let branches = function
            .blocks
            .iter()
            .filter_map(|block| match &block.terminator.as_ref()?.kind {
                TerminatorKind::NullableBranch { owner, view, .. } => Some((*owner, *view)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(branches.len(), 2);
        assert_eq!(EntityId::Value(branches[0].0), wraps[0]);
        assert_eq!(EntityId::Value(branches[1].0), wraps[1]);
        assert_ne!(branches[0].0, branches[1].0);
        assert_ne!(branches[0].1, branches[1].1);
    }
}

/// SPEC-0350: `main` is inside the refinement subset, which proves every null test with
/// a `NullableBranch`. Without a field use the view ends at once, before the owner's
/// published drop on that edge; the null edge drops the same owner.
#[test]
fn ordinary_null_check_without_field_use_ends_its_view_before_the_drop() {
    let text = r#"class Node(val v: Int)
fun main(): Unit {
    val a: Node? = Node(5)
    if (a != null) { println("present") }
}
"#;
    for root in ["single", "src"] {
        let (program, entry) = check_source_with(text, root, |_, _, _| {});
        let function = program
            .module(entry.module())
            .unwrap()
            .function(entry)
            .unwrap();
        let (owner, null_edge, non_null_edge, loan) = only(function.blocks.iter().filter_map(
            |block| match &block.terminator.as_ref()?.kind {
                TerminatorKind::NullableBranch {
                    owner,
                    when_null,
                    when_non_null,
                    view,
                } => Some((*owner, when_null, when_non_null, *view)),
                _ => None,
            },
        ));
        let body = function.block(non_null_edge.target).unwrap();
        let null = function.block(null_edge.target).unwrap();
        let column = only(
            non_null_edge
                .arguments
                .iter()
                .enumerate()
                .filter_map(|(index, entity)| (*entity == EntityId::Value(owner)).then_some(index)),
        );
        assert_eq!(body.parameters.last(), Some(&EntityId::Loan(loan)));
        let users = function
            .instructions
            .iter()
            .filter(|instruction| {
                instruction
                    .operation
                    .entities()
                    .contains(&EntityId::Loan(loan))
            })
            .collect::<Vec<_>>();
        let [end] = users.as_slice() else {
            panic!("the view's only use is its end")
        };
        assert!(matches!(end.operation, Operation::BorrowEnd { .. }));
        let drops = |block: &crate::ssa::model::Block, owner: EntityId| {
            block
                .instructions
                .iter()
                .filter(|id| {
                    matches!(function.instruction(**id).unwrap().operation,
                        Operation::Drop { owner: dropped } if EntityId::Value(dropped) == owner)
                })
                .count()
        };
        assert_eq!(drops(body, body.parameters[column]), 1);
        assert_eq!(drops(null, null.parameters[column]), 1);
        let position = |id| {
            body.instructions
                .iter()
                .position(|candidate| *candidate == id)
        };
        let drop = body
            .instructions
            .iter()
            .copied()
            .find(|id| {
                matches!(
                    function.instruction(*id).unwrap().operation,
                    Operation::Drop { .. }
                )
            })
            .unwrap();
        assert!(position(end.id) < position(drop));
        assert!(
            function.instructions.iter().all(|instruction| !matches!(
                instruction.operation,
                Operation::HeapFieldRead { .. }
            ))
        );
    }
}

#[test]
fn nullable_field_access_without_current_root_fact_stops_in_frontend() {
    for (body, expected_member) in [
        "val a: Node? = Node(5); if (a.v == 5) { println(\"bad\") }",
        "val a: Node? = Node(5); val b: Node? = Node(6); if (a != null) { if (b.v == 6) { println(\"bad\") } }",
        "var a: Node? = Node(5); if (a != null) { a = null; if (a.v == 5) { println(\"stale\") } }",
    ]
    .into_iter()
    .zip(["a.v", "b.v", "a.v"])
    {
        let text = format!("class Node(val v: Int)\nfun main(): Unit {{ {body} }}\n");
        let (snapshot, environment) = snapshot(&text, "single");
        let inputs = snapshot.inputs();
        let typed = check_compilation_unit_types(
            snapshot.sources(),
            &inputs,
            snapshot.validated_names().unwrap(),
            &environment,
        )
        .unwrap();
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        let before = typed.clone();
        let input = only(inputs.iter().filter(|input| {
            snapshot.sources().source_name(input.source_id()).unwrap() == "p14b_nullable_simple.ko"
        }));
        let unit = only(
            snapshot
                .names()
                .index()
                .source_units()
                .iter()
                .filter(|unit| unit.source_id() == input.source_id()),
        )
        .id();
        let (member, member_node, receiver) = only(
            input
                .parsed()
                .ast()
                .expressions()
                .iter()
                .filter_map(|(id, node)| match node.payload() {
                    Expression::Member { receiver, .. } => Some((id, node, *receiver)),
                    _ => None,
                }),
        );
        let member = UnitExpressionId::new(unit, member);
        let receiver = UnitExpressionId::new(unit, receiver);
        assert_eq!(member_node.span().source_id(), input.source_id());
        assert_eq!(
            snapshot.sources().slice(member_node.span()),
            Ok(expected_member)
        );
        assert!(matches!(
            typed.types().get(typed.expression_type(receiver).unwrap()),
            Some(UnitTypeKind::Nullable(_))
        ));
        // A different use, such as the pre-write target, may have a real fact.
        assert!(typed.non_null_use(receiver).is_none());
        assert!(typed.aggregate_projection(member).is_none());
        assert!(matches!(
            typed.types().get(typed.expression_type(member).unwrap()),
            Some(UnitTypeKind::Deferred(DeferredReason::MemberAccess))
        ));
        // Raw recovery is not final admission. Reject the original whole source
        // at its real frontend root, never as a lowering/verifier failure.
        let diagnostics = match complete_compilation_unit_types(
            snapshot.sources(),
            &inputs,
            snapshot.validated_names().unwrap(),
            &environment,
            &typed,
        ) {
            Err(FinalTypeAdmissionError::Diagnostics(diagnostics)) => diagnostics,
            other => panic!("expected source diagnostics, got {other:?}: {text}"),
        };
        assert_eq!(typed, before, "final admission must not alter raw facts");
        assert!(typed.is_same_analysis(&before));
        let diagnostic = only(diagnostics.iter());
        assert_eq!(diagnostic.severity(), Severity::Error);
        assert_eq!(diagnostic.code().to_string(), "L0165");
        assert_eq!(diagnostic.primary_span(), member_node.span());
        assert_eq!(
            diagnostic.message(),
            "type checking could not complete: the member's type is unresolved"
        );
    }
}

fn snapshot(text: &str, root: &str) -> (UnitNameSnapshot, TypeEnvironment) {
    let mut sources = SourceMap::new();
    let source = sources.add_source("p14b_nullable_simple.ko", text).unwrap();
    let standard = sources
        .add_source("koven-std/koven/algorithms/ranges.ko", STANDARD)
        .unwrap();
    let (names_environment, mut environment) = standard_environments();
    environment
        .authorize_range_source(&sources, standard)
        .unwrap();
    environment
        .authorize_range_extension_source(&sources, standard)
        .unwrap();
    let descriptors = [
        UnitSourceDescriptor::new(root, "p14b_nullable_simple.ko", source),
        UnitSourceDescriptor::new("koven-std", "koven/algorithms/ranges.ko", standard),
    ];
    let parsed = descriptors
        .into_iter()
        .map(|descriptor| {
            let lexed = lex(&sources, descriptor.source_id()).unwrap();
            assert!(lexed.diagnostics().is_empty());
            let file = parse_file(&sources, &lexed).unwrap();
            assert!(file.diagnostics().is_empty(), "{:?}", file.diagnostics());
            (descriptor, file)
        })
        .collect();
    let snapshot = analyze_parsed_unit_names(sources, parsed, names_environment).unwrap();
    assert!(snapshot.names().diagnostics().is_empty());
    (snapshot, environment)
}

fn check_source(text: &str, root: &str, expected: usize) -> (Program, FunctionId) {
    check_source_with(text, root, |program, entry, view| {
        assert_field_proofs(program, entry, view, expected);
    })
}

fn check_source_with(
    text: &str,
    root: &str,
    shape: impl FnOnce(&Program, FunctionId, &OwnedCompilationUnitView<'_, '_>),
) -> (Program, FunctionId) {
    let (snapshot, environment) = snapshot(text, root);
    let sources = snapshot.sources();
    let inputs = snapshot.inputs();
    let names = snapshot.validated_names().unwrap();
    let raw = check_compilation_unit_types(sources, &inputs, names, &environment).unwrap();
    assert!(raw.diagnostics().is_empty(), "{:?}", raw.diagnostics());
    let complete =
        complete_compilation_unit_types(sources, &inputs, names, &environment, &raw).unwrap();
    assert!(complete.is_same_analysis(&raw));
    assert!(std::ptr::eq(complete.types(), &raw));
    let outcome = analyze_basic_unit_ownership(sources, &inputs, names, &environment, raw).unwrap();
    let (typed, owned) = outcome.into_result().unwrap();
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
    assert!(owned.is_compatible_with(&typed));
    let owned = owned.validate().unwrap();
    let view =
        owned_compilation_unit_view(sources, &inputs, names, &environment, &typed, &owned).unwrap();
    assert!(std::ptr::eq(view.types(), typed.types()));
    assert!(std::ptr::eq(view.ownership(), owned.ownership()));
    let declaration = only(
        names
            .names()
            .index()
            .declarations()
            .iter()
            .filter(|item| item.kind() == SymbolKind::Function && item.name() == "main"),
    );
    let (program, entry) = lower_owned_unit_with_entry(&view, declaration.id())
        .unwrap_or_else(|error| panic!("whole {root} source failed: {error:?}\n{text}"));
    verify_program(&program).unwrap();
    shape(&program, entry, &view);
    let llvm = crate::llvm::render_verified_program(&program).unwrap();
    assert!(!llvm.is_empty());
    (program, entry)
}

fn assert_field_proofs(
    program: &Program,
    entry: FunctionId,
    view: &OwnedCompilationUnitView<'_, '_>,
    expected: usize,
) {
    let module = program.module(entry.module()).unwrap();
    let function = module.function(entry).unwrap();
    let branches = function
        .blocks
        .iter()
        .filter_map(|block| match &block.terminator.as_ref()?.kind {
            TerminatorKind::NullableBranch {
                owner,
                when_null,
                when_non_null,
                view,
            } => Some((block, *owner, when_null, when_non_null, *view)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(branches.len(), expected);
    for (header, owner, null_edge, non_null_edge, loan) in branches {
        let control_span = header.terminator.as_ref().unwrap().origin.span();
        let input = only(
            view.inputs()
                .iter()
                .copied()
                .filter(|input| input.source_id() == control_span.source_id()),
        );
        let source = only(
            view.names()
                .names()
                .index()
                .source_units()
                .iter()
                .filter(|unit| unit.source_id() == input.source_id()),
        )
        .id();
        let nodes = input.parsed().ast().expressions();
        let (control_id, control) =
            only(nodes.iter().filter(|(_, node)| node.span() == control_span));
        let body = function.block(non_null_edge.target).unwrap();
        let null = function.block(null_edge.target).unwrap();
        let comparison = if let Expression::If {
            mut condition,
            then_branch,
            else_branch,
            ..
        } = *control.payload()
        {
            while let Expression::Group { expression } = nodes.get(condition).unwrap().payload() {
                condition = *expression;
            }
            let comparison = view
                .types()
                .null_comparison(UnitExpressionId::new(source, condition))
                .unwrap();
            let (present, absent) = if comparison.non_null_when_true() {
                (then_branch, else_branch)
            } else {
                (else_branch.unwrap(), Some(then_branch))
            };
            assert_if_block_origins(input, (body, null), (present, absent), control_span);
            comparison
        } else {
            // SPEC-0350: a function inside the refinement subset attributes the proof
            // branch to the null comparison itself; its blocks carry the function origin.
            let comparison = view
                .types()
                .null_comparison(UnitExpressionId::new(source, control_id))
                .expect("proof originates at a source If or its null comparison");
            assert!(comparison.non_null_when_true());
            comparison
        };
        let column = only(
            non_null_edge
                .arguments
                .iter()
                .enumerate()
                .filter_map(|(index, entity)| (*entity == EntityId::Value(owner)).then_some(index)),
        );
        assert_eq!(null_edge.arguments[column], EntityId::Value(owner));
        let EntityId::Value(body_owner) = body.parameters[column] else {
            panic!("same nullable owner column")
        };
        let EntityId::Value(null_owner) = null.parameters[column] else {
            panic!("null-side owner column")
        };
        assert_eq!(body.parameters.len(), non_null_edge.arguments.len() + 1);
        assert_eq!(body.parameters.last(), Some(&EntityId::Loan(loan)));
        assert_eq!(null.parameters.len(), null_edge.arguments.len());
        let EntityType::Value(nullable) = function.entity(EntityId::Value(owner)).unwrap().ty
        else {
            panic!("nullable owner")
        };
        let Some(SsaTypeKind::NullableHandle { inner }) = module.type_kind(nullable) else {
            panic!("storage stays nullable")
        };
        let Some(SsaTypeKind::HeapOwner {
            payload: Some(payload),
            ..
        }) = module.type_kind(*inner)
        else {
            panic!("ordinary class owner")
        };
        let Some(SsaTypeKind::Aggregate { fields, .. }) = module.type_kind(*payload) else {
            panic!("actual payload")
        };
        assert_eq!(fields.len(), 1);
        assert_eq!(
            module.type_kind(fields[0]),
            Some(&SsaTypeKind::Integer {
                bits: 32,
                signed: true
            })
        );
        for current in [body_owner, null_owner] {
            assert_eq!(
                function.entity(EntityId::Value(current)).unwrap().ty,
                EntityType::Value(nullable)
            );
        }
        let Definition::InstructionResult {
            instruction: producer,
            index,
        } = function.entity(EntityId::Value(owner)).unwrap().definition
        else {
            panic!("fresh current nullable generation, not an old branch owner")
        };
        let producer = function.instruction(producer).unwrap();
        assert_eq!(producer.results[index], EntityId::Value(owner));
        assert!(matches!(
            producer.operation,
            Operation::NullableWrap { .. } | Operation::NullableNull { .. }
        ));
        assert_eq!(producer.block, header.id);
        for instruction in &function.instructions {
            if instruction
                .results
                .iter()
                .any(|entity| function.entity(*entity).unwrap().ty == EntityType::Value(*inner))
            {
                assert!(
                    matches!(instruction.operation, Operation::HeapAllocate { .. }),
                    "only a real allocation produces a Node owner"
                );
            }
        }
        assert!(function.blocks.iter().all(|block| {
            block
                .parameters
                .iter()
                .all(|entity| function.entity(*entity).unwrap().ty != EntityType::Value(*inner))
        }));
        assert_eq!(
            function.entity(EntityId::Loan(loan)).unwrap().ty,
            EntityType::Loan {
                kind: LoanKind::Shared,
                target: *inner
            }
        );
        assert_eq!(
            function.entity(EntityId::Loan(loan)).unwrap().definition,
            Definition::BlockParameter {
                block: body.id,
                index: body.parameters.len() - 1
            }
        );
        let read = only(function.instructions.iter().filter(|instruction| {
            matches!(instruction.operation, Operation::HeapFieldRead { receiver, .. } if receiver == loan)
        }));
        assert!(matches!(
            read.operation,
            Operation::HeapFieldRead { field: 0, .. }
        ));
        let (expression, _) = only(
            nodes
                .iter()
                .filter(|(_, node)| node.span() == read.origin.span()),
        );
        let projection = view
            .types()
            .aggregate_projection(UnitExpressionId::new(source, expression))
            .unwrap();
        let UnitAggregateProjectionReceiver::Expression(receiver) = projection.receiver() else {
            panic!("real field receiver")
        };
        let non_null_use = view.types().non_null_use(receiver).unwrap();
        assert_eq!(non_null_use.symbol(), comparison.symbol());
        assert_eq!(non_null_use.declared_type(), comparison.nullable_type());
        assert_eq!(
            view.types().types().get(comparison.nullable_type()),
            Some(&UnitTypeKind::Nullable(non_null_use.narrowed_type()))
        );
        assert_eq!(
            view.types().types().get(projection.ty()),
            Some(&UnitTypeKind::Builtin(BuiltinType::Int))
        );
        let EntityId::Value(value) = only(read.results.iter().copied()) else {
            panic!("copyable Int result")
        };
        assert_eq!(
            function.entity(EntityId::Value(value)).unwrap().ty,
            EntityType::Value(fields[0])
        );
        let end = only(function.instructions.iter().filter(|instruction| matches!(instruction.operation, Operation::BorrowEnd { loan: ended } if ended == loan)));
        let drop = only(function.instructions.iter().filter(|instruction| matches!(instruction.operation, Operation::Drop { owner } if owner == body_owner)));
        let null_drop = only(function.instructions.iter().filter(|instruction| matches!(instruction.operation, Operation::Drop { owner } if owner == null_owner)));
        assert_eq!(null_drop.block, null.id);
        let comparison_instruction = only(function.instructions.iter().filter(|instruction| {
            matches!(instruction.operation, Operation::Compare { left, right, .. } if left == value || right == value)
        }));
        let mut previous = None;
        for instruction in [read, end, drop, comparison_instruction] {
            assert_eq!(instruction.block, body.id);
            let index = body
                .instructions
                .iter()
                .position(|id| *id == instruction.id)
                .unwrap();
            if let Some(previous) = previous {
                assert!(previous < index);
            }
            previous = Some(index);
        }
        let terminal_drop = only(view.ownership().drops().iter().filter(|fact| {
            fact.target() == UnitDropTarget::Named(comparison.symbol())
                && matches!(fact.point(), UnitDropPoint::AfterExpression(id)
                    if id.source_unit() == source && nodes.get(id.expression()).is_ok_and(|node|
                        node.span().start() <= read.origin.span().start() && node.span().end() >= read.origin.span().end()))
        }));
        assert_eq!(drop.origin.span(), terminal_drop.value_origin());
        assert_eq!(end.origin.span(), terminal_drop.value_origin());
        assert!(matches!(&body.terminator.as_ref().unwrap().kind,
            TerminatorKind::Conditional { condition, .. }
                if comparison_instruction.results.as_slice() == [EntityId::Value(*condition)]));
        let users = function
            .instructions
            .iter()
            .filter(|instruction| {
                instruction
                    .operation
                    .entities()
                    .contains(&EntityId::Loan(loan))
            })
            .map(|instruction| instruction.id)
            .collect::<BTreeSet<_>>();
        assert_eq!(users, BTreeSet::from([read.id, end.id]));
        for block in &function.blocks {
            assert!(
                !block
                    .terminator
                    .as_ref()
                    .unwrap()
                    .kind
                    .entities()
                    .contains(&EntityId::Loan(loan)),
                "view ends before inner If or any join"
            );
        }
        let mut pending = vec![null.id];
        let mut seen = BTreeSet::new();
        while let Some(id) = pending.pop() {
            if !seen.insert(id) {
                continue;
            }
            assert_ne!(id, body.id, "null cannot reach this field read");
            let block = function.block(id).unwrap();
            assert!(!block.parameters.contains(&EntityId::Loan(loan)));
            for id in &block.instructions {
                assert!(
                    !function
                        .instruction(*id)
                        .unwrap()
                        .operation
                        .entities()
                        .contains(&EntityId::Loan(loan))
                );
            }
            pending.extend(block.terminator.as_ref().unwrap().kind.targets());
        }
    }
    assert!(
        function.instructions.iter().all(|instruction| !matches!(
            instruction.operation,
            Operation::HeapPayloadPlace { .. }
                | Operation::Read { .. }
                | Operation::NullableTake { .. }
        )),
        "no nullable storage masquerades as payload or a second inner owner"
    );
}

/// The per-family `If` lowering gives the proof's blocks their branch statement origins.
fn assert_if_block_origins(
    input: SourceUnitInput<'_>,
    (body, null): (&Block, &Block),
    (present, absent): (StatementId, Option<StatementId>),
    control_span: Span,
) {
    let statement = |id: StatementId| input.parsed().ast().statements().get(id).unwrap().span();
    assert_eq!(body.origin.span(), statement(present));
    assert_eq!(
        null.origin.span(),
        absent.map(statement).unwrap_or(control_span)
    );
}

fn only<T>(mut values: impl Iterator<Item = T>) -> T {
    let value = values.next().unwrap();
    assert!(values.next().is_none(), "unique actual identity");
    value
}
