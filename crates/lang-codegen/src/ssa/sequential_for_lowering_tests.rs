mod cleanup_tests;
mod determinism_tests;
mod provider_lifetime_tests;
mod source_cfg_tests;
mod unit_storage_boundary_tests;
mod unit_storage_tests;

use lang_frontend::{
    analysis::analyze_basic_unit_ownership,
    lexer::lex,
    name_resolution::{
        SourceUnitInput, SymbolKind, ValidatedCompilationUnitNames, index_compilation_unit,
        resolve_compilation_unit_names,
    },
    ownership_checking::owned_compilation_unit_view,
    parser::{ParsedFile, parse_file},
    source::SourceMap,
    type_checking::{
        check_compilation_unit_types, complete_compilation_unit_types, standard_environments,
    },
};

use super::{
    model::{
        BinaryOperator, Block, BlockId, ComparisonOperator, Definition, Edge, EntityId, Function,
        FunctionId, Instruction, LoanKind, Operation, Origin, Program, ScalarConstant,
        TerminatorKind,
    },
    render::render_program,
    unit_lower::lower_owned_unit_with_entry,
    verify::verify_program,
};

struct SourceProgram {
    program: Program,
    sources: SourceMap,
    parsed: ParsedFile,
    names: ValidatedCompilationUnitNames,
    entry: FunctionId,
}

impl SourceProgram {
    fn entry(&self) -> &Function {
        self.program
            .module(self.entry.module())
            .unwrap()
            .function(self.entry)
            .unwrap()
    }

    fn entry_mut(&mut self) -> &mut Function {
        self.program
            .module_mut(self.entry.module())
            .unwrap()
            .function_mut(self.entry)
            .unwrap()
    }

    fn function(&self, name: &str) -> &Function {
        let mut declarations =
            self.names
                .names()
                .index()
                .declarations()
                .iter()
                .filter(|declaration| {
                    declaration.name() == name
                        && declaration.kind() == SymbolKind::Function
                        && declaration.name_span().source_id() == self.parsed.source_id()
                });
        let declaration = declarations.next().expect("source function declaration");
        assert!(
            declarations.next().is_none(),
            "unique source function declaration"
        );
        let span = self
            .parsed
            .ast()
            .items()
            .get(declaration.root())
            .unwrap()
            .span();
        let mut functions = self
            .program
            .modules
            .iter()
            .flat_map(|module| &module.functions)
            .filter(|function| function.origin == Origin::Source(span));
        let function = functions
            .next()
            .expect("same-source function body is emitted");
        assert!(
            functions.next().is_none(),
            "one body for the original source declaration"
        );
        function
    }
}

fn analyze(source_text: &str, entry_name: &str) -> SourceProgram {
    analyze_case("sequential-for.ko", source_text, entry_name)
}

fn analyze_case(filename: &str, source_text: &str, entry_name: &str) -> SourceProgram {
    let mut sources = SourceMap::new();
    let source = sources.add_source(filename, source_text).expect("source");
    let lexed = lex(&sources, source).expect("lexer");
    assert!(lexed.diagnostics().is_empty(), "{:?}", lexed.diagnostics());
    let parsed = parse_file(&sources, &lexed).expect("parser");
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let inputs = [SourceUnitInput::memory_document(source, &parsed)];
    let (environment, types) = standard_environments();
    let index = index_compilation_unit(&sources, &inputs).expect("source input");
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
        .expect("names")
        .validate()
        .expect("valid names");
    let raw = check_compilation_unit_types(&sources, &inputs, &names, &types).expect("types");
    assert!(raw.diagnostics().is_empty(), "{:?}", raw.diagnostics());
    complete_compilation_unit_types(&sources, &inputs, &names, &types, &raw)
        .expect("all original source types must be complete");
    let (typed, owned) = analyze_basic_unit_ownership(&sources, &inputs, &names, &types, raw)
        .expect("ownership")
        .into_result()
        .expect("valid types");
    let owned = owned.validate().expect("valid ownership");
    // Same original map, parsed tree, unit index, name owner and type environment throughout.
    assert_eq!(parsed.source_id(), source);
    assert_eq!(names.names().index().source_units().len(), 1);
    assert_eq!(names.names().index().source_units()[0].source_id(), source);
    assert_eq!(names.names().source_units().len(), 1);
    assert_eq!(
        names.names().source_units()[0].resolution().source_id(),
        source
    );
    assert!(
        typed
            .types()
            .is_compatible_with(&sources, &inputs, &names, &types)
    );
    assert!(owned.ownership().is_compatible_with(&typed));
    for diagnostics in [
        names.names().diagnostics(),
        typed.types().diagnostics(),
        owned.ownership().diagnostics(),
    ] {
        assert!(diagnostics.is_empty(), "{filename}: {diagnostics:?}");
    }
    let mut entries = names
        .names()
        .index()
        .declarations()
        .iter()
        .filter(|declaration| {
            declaration.name() == entry_name
                && declaration.kind() == SymbolKind::Function
                && declaration.name_span().source_id() == source
        });
    let declaration = entries.next().expect("selected source function exists");
    assert!(
        entries.next().is_none(),
        "selected source function is unique"
    );
    let entry_span = parsed.ast().items().get(declaration.root()).unwrap().span();
    let unit = owned_compilation_unit_view(&sources, &inputs, &names, &types, &typed, &owned)
        .expect("same-analysis owned unit");
    let (program, entry) =
        lower_owned_unit_with_entry(&unit, declaration.id()).unwrap_or_else(|error| {
            panic!("{filename}: sequential for loop must lower: {error:?}\n{source_text}")
        });
    verify_program(&program).expect("lowered for loop SSA must verify");
    assert_eq!(
        program
            .module(entry.module())
            .unwrap()
            .function(entry)
            .unwrap()
            .origin,
        Origin::Source(entry_span)
    );
    SourceProgram {
        program,
        sources,
        parsed,
        names,
        entry,
    }
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
    let analysis = analyze(text, "sum");
    let ssa = render_program(&analysis.program);
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
    let analysis = analyze(text, "count");
    let ssa = render_program(&analysis.program);
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
    let analysis = analyze(text, "test");
    let ssa = render_program(&analysis.program);
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
    let analysis = analyze(text, "test");
    let ssa = render_program(&analysis.program);
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
    let analysis = analyze(text, "find_first_even");
    let ssa = render_program(&analysis.program);
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
    let analysis = analyze(text, "sum_pairs");
    let ssa = render_program(&analysis.program);
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
    let analysis = analyze(text, "matrix_sum");
    let ssa = render_program(&analysis.program);
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
                TerminatorKind::Return { .. }
                | TerminatorKind::RangeReturn { .. }
                | TerminatorKind::BorrowReturn { .. }
                | TerminatorKind::Abort => Vec::new(),
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
        let analysis = analyze(&text, "run");
        let source = analysis.function("source");
        let run = analysis.function("run");
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
        let body_snapshot = transport(EntityId::Value(snapshot));
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
        assert_eq!(backedge.arguments[length_slot], body_snapshot);
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
