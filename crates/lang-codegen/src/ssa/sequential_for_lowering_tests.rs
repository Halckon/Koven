mod cleanup_tests;
mod determinism_tests;
mod provider_lifetime_tests;
mod source_cfg_tests;
mod unit_storage_boundary_tests;
mod unit_storage_tests;

use lang_frontend::{
    lexer::lex,
    name_resolution::resolve_names,
    ownership_checking::check_ownership,
    parser::parse_file,
    source::SourceMap,
    type_checking::{check_types, standard_environments},
};

use super::{
    lower_frontend::orchestrate::lower_scalar_file,
    model::{
        BinaryOperator, Block, BlockId, ComparisonOperator, Definition, Edge, EntityId, Function,
        Instruction, LoanKind, Operation, ScalarConstant, TerminatorKind,
    },
    render::render_program,
    verify::verify_program,
};

fn analyze(source_text: &str) -> super::model::Program {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source("sequential-for.ko", source_text)
        .expect("source");
    let lexed = lex(&sources, source).expect("lexer");
    let parsed = parse_file(&sources, &lexed).expect("parser");
    let (environment, types) = standard_environments();
    let names = resolve_names(&sources, &parsed, &environment).expect("names");
    let typed = check_types(&sources, &parsed, &names, &types).expect("types");
    let owned = check_ownership(&sources, &parsed, &names, &typed).expect("ownership");
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let program = lower_scalar_file(&sources, &parsed, &names, &typed, &owned)
        .expect("sequential for loop must lower");
    verify_program(&program).expect("lowered for loop SSA must verify");
    program
}

#[test]
fn borrowed_named_array_iteration_lowers_to_verified_ssa_cfg() {
    let text = r#"
        fun sum(xs: Array<Int>): Int {
            var total = 0
            for (x in xs) {
                total = total + x
            }
            return total
        }
    "#;
    let program = analyze(text);
    let ssa = render_program(&program);
    assert!(ssa.contains("container.length"), "{ssa}");
    assert!(ssa.contains("container.element_place"), "{ssa}");
    assert!(ssa.contains("borrow.shared"), "{ssa}");
    assert!(ssa.contains("end_borrow"), "{ssa}");
}

#[test]
fn discard_binding_lowers_to_verified_ssa() {
    let text = r#"
        fun count(xs: List<Int>): Int {
            var c = 0
            for (_ in xs) {
                c = c + 1
            }
            return c
        }
    "#;
    let program = analyze(text);
    let ssa = render_program(&program);
    assert!(ssa.contains("container.length"), "{ssa}");
    assert!(ssa.contains("container.element_place"), "{ssa}");
    assert!(ssa.contains("end_borrow"), "{ssa}");
}

#[test]
fn temporary_list_source_iteration_lowers_to_verified_ssa() {
    let text = r#"
        fun test(): Int {
            var total = 0
            for (x in listOf(10, 20, 30)) {
                total = total + x
            }
            return total
        }
    "#;
    let program = analyze(text);
    let ssa = render_program(&program);
    assert!(ssa.contains("container.length"), "{ssa}");
    assert!(ssa.contains("container.element_place"), "{ssa}");
    assert!(ssa.contains("end_borrow"), "{ssa}");
}

#[test]
fn mutable_list_with_break_and_continue_lowers_to_verified_ssa() {
    let text = r#"
        fun test(xs: MutableList<Int>): Int {
            var sum = 0
            for (x in xs) {
                if (x < 0) {
                    continue
                }
                if (x > 100) {
                    break
                }
                sum = sum + x
            }
            return sum
        }
    "#;
    let program = analyze(text);
    let ssa = render_program(&program);
    assert!(ssa.contains("container.length"), "{ssa}");
    assert!(ssa.contains("container.element_place"), "{ssa}");
}

#[test]
fn early_return_from_for_loop_lowers_to_verified_ssa() {
    let text = r#"
        fun find_first_even(xs: Array<Int>): Int {
            for (x in xs) {
                if (x == 2) {
                    return x
                }
            }
            return -1
        }
    "#;
    let program = analyze(text);
    let ssa = render_program(&program);
    assert!(ssa.contains("container.length"), "{ssa}");
    assert!(ssa.contains("return %"), "{ssa}");
}

#[test]
fn borrowed_destructuring_iteration_lowers_to_verified_ssa() {
    let text = r#"
        value class Pair(val first: Int, val second: Int)
        fun sum_pairs(pairs: Array<Pair>): Int {
            var total = 0
            for ((a, b) in pairs) {
                total = total + a + b
            }
            return total
        }
    "#;
    let program = analyze(text);
    let ssa = render_program(&program);
    assert!(ssa.contains("container.length"), "{ssa}");
    assert!(ssa.contains("container.element_place"), "{ssa}");
}

#[test]
fn nested_for_loops_lower_to_verified_ssa() {
    let text = r#"
        fun matrix_sum(rows: Array<Array<Int>>): Int {
            var total = 0
            for (row in rows) {
                for (x in row) {
                    total = total + x
                }
            }
            return total
        }
    "#;
    let program = analyze(text);
    let ssa = render_program(&program);
    assert!(ssa.contains("container.length"), "{ssa}");
}

fn defining_instruction(function: &Function, entity: EntityId) -> &Instruction {
    let Definition::InstructionResult { instruction, .. } =
        function.entity(entity).expect("known entity").definition
    else {
        panic!("expected instruction result, got {entity:?}");
    };
    function
        .instruction(instruction)
        .expect("known instruction")
}

fn incoming_edges(function: &Function, target: BlockId) -> Vec<(BlockId, &Edge)> {
    function
        .blocks
        .iter()
        .flat_map(|block| {
            let edges = match &block.terminator.as_ref().expect("terminator").kind {
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
            };
            edges.into_iter().map(move |edge| (block.id, edge))
        })
        .filter(|(_, edge)| edge.target == target)
        .collect()
}

fn parameter_slot(block: &Block, entity: EntityId) -> usize {
    block
        .parameters
        .iter()
        .position(|parameter| *parameter == entity)
        .expect("expected block parameter")
}

#[test]
fn temporary_source_boundaries_keep_call_and_length_in_preheader() {
    for (provider, constructor, cardinality, arguments) in [
        ("Array", "arrayOf", "empty", ""),
        ("Array", "arrayOf", "single", "7"),
        ("Array", "arrayOf", "multi", "7, 2, 9"),
        ("List", "listOf", "empty", ""),
        ("List", "listOf", "single", "7"),
        ("List", "listOf", "multi", "7, 2, 9"),
        ("MutableList", "mutableListOf", "empty", ""),
        ("MutableList", "mutableListOf", "single", "7"),
        ("MutableList", "mutableListOf", "multi", "7, 2, 9"),
    ] {
        let text = format!(
            r#"
                fun source(): {provider}<Int> = {constructor}<Int>({arguments})
                fun run(): Unit {{
                    for (x in source()) {{
                        println("body")
                    }}
                }}
            "#
        );
        eprintln!("temporary_{provider}_{cardinality}.ko:\n{text}");
        let program = analyze(&text);
        let functions = || program.modules.iter().flat_map(|module| &module.functions);
        let source = functions().find(|f| f.name == "source").expect("source");
        let run = functions().find(|f| f.name == "run").expect("run");
        let calls = run.instructions.iter().filter(|instruction| {
            matches!(instruction.operation, Operation::DirectCall { callee, .. } if callee == source.id)
        }).collect::<Vec<_>>();
        let lengths = run
            .instructions
            .iter()
            .filter(|instruction| {
                matches!(instruction.operation, Operation::ContainerLength { .. })
            })
            .collect::<Vec<_>>();
        assert_eq!(calls.len(), 1, "source must have exactly one call site");
        assert_eq!(lengths.len(), 1, "length must have exactly one snapshot");
        let (call, length) = (calls[0], lengths[0]);
        let preheader = run
            .block(run.entry_block().expect("entry"))
            .expect("preheader");
        assert_eq!(call.block, preheader.id);
        assert_eq!(length.block, preheader.id);
        assert!(incoming_edges(run, preheader.id).is_empty());

        // Follow the actual owner -> place -> shared loan identities, not a rendered snapshot.
        let Operation::ContainerLength { owner: source_loan } = length.operation else {
            unreachable!();
        };
        assert!(matches!(source_loan, EntityId::Loan(_)));
        let borrow = defining_instruction(run, source_loan);
        let Operation::BorrowBegin {
            place,
            kind: LoanKind::Shared,
        } = borrow.operation
        else {
            panic!("snapshot must read through a shared source loan");
        };
        let root = defining_instruction(run, EntityId::Place(place));
        let Operation::RootPlace { owner } = root.operation else {
            panic!("temporary source must have a root place");
        };
        assert_eq!(call.results, [EntityId::Value(owner)]);
        let positions = [call, root, borrow, length].map(|instruction| {
            assert_eq!(instruction.block, preheader.id);
            preheader
                .instructions
                .iter()
                .position(|id| *id == instruction.id)
                .expect("preheader instruction")
        });
        assert!(positions.windows(2).all(|pair| pair[0] < pair[1]));

        let TerminatorKind::Branch(enter) = &preheader.terminator.as_ref().unwrap().kind else {
            panic!("preheader must enter the loop header");
        };
        let header = run.block(enter.target).expect("header");
        let TerminatorKind::Conditional {
            condition,
            when_true,
            when_false,
        } = &header.terminator.as_ref().unwrap().kind
        else {
            panic!("header must guard the body");
        };
        let guard = defining_instruction(run, EntityId::Value(*condition));
        assert_eq!(guard.block, header.id);
        let Operation::Compare {
            operator: ComparisonOperator::LessThan,
            left: cursor,
            right: snapshot,
        } = guard.operation
        else {
            panic!("guard must compare cursor < length");
        };
        let cursor_slot = parameter_slot(header, EntityId::Value(cursor));
        let length_slot = parameter_slot(header, EntityId::Value(snapshot));
        assert_eq!(length.results, [enter.arguments[length_slot]]);
        let zero = defining_instruction(run, enter.arguments[cursor_slot]);
        assert_eq!(zero.block, preheader.id);
        assert_eq!(
            zero.operation,
            Operation::Constant(ScalarConstant::Integer(0))
        );
        let source_slot = enter
            .arguments
            .iter()
            .position(|argument| *argument == source_loan)
            .expect("source transport");
        let header_source = header.parameters[source_slot];
        assert!(matches!(header_source, EntityId::Loan(_)));

        let body = run.block(when_true.target).expect("body");
        assert_ne!(when_true.target, when_false.target);
        assert_eq!(incoming_edges(run, body.id), [(header.id, when_true)]);
        let transport = |entity| {
            let slot = when_true
                .arguments
                .iter()
                .position(|argument| *argument == entity)
                .expect("body transport");
            body.parameters[slot]
        };
        let body_source = transport(header_source);
        let body_cursor = transport(EntityId::Value(cursor));
        let elements = run
            .instructions
            .iter()
            .filter(|instruction| {
                matches!(
                    instruction.operation,
                    Operation::ContainerElementPlace { .. }
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(elements.len(), 1);
        let element = elements[0];
        assert_eq!(element.block, body.id);
        let Operation::ContainerElementPlace { owner, index } = element.operation else {
            unreachable!();
        };
        assert_eq!(owner, body_source);
        assert_eq!(EntityId::Value(index), body_cursor);

        let TerminatorKind::Branch(backedge) = &body.terminator.as_ref().unwrap().kind else {
            panic!("normal body must return to the header");
        };
        assert_eq!(backedge.target, header.id);
        let header_incoming = incoming_edges(run, header.id);
        assert_eq!(header_incoming.len(), 2);
        assert!(header_incoming.contains(&(preheader.id, enter)));
        assert!(header_incoming.contains(&(body.id, backedge)));
        assert_eq!(backedge.arguments[source_slot], body_source);
        assert_eq!(backedge.arguments[length_slot], EntityId::Value(snapshot));
        let advance = defining_instruction(run, backedge.arguments[cursor_slot]);
        assert_eq!(advance.block, body.id);
        let Operation::Binary {
            operator: BinaryOperator::Add,
            left,
            right,
        } = advance.operation
        else {
            panic!("backedge cursor must advance by one");
        };
        assert_eq!(EntityId::Value(left), body_cursor);
        let step = defining_instruction(run, EntityId::Value(right));
        assert_eq!(step.block, preheader.id);
        assert_eq!(
            step.operation,
            Operation::Constant(ScalarConstant::Integer(1))
        );
    }
}
