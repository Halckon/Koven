//! Normal source programs only: checked count arithmetic keeps range roots live.
use super::*;

const IMPORTS: &str = "package app\nimport koven.algorithms.take\nimport koven.algorithms.drop\nimport koven.algorithms.dropLast\n";

fn programs(consumer: &str) -> [(Program, FunctionId); 3] {
    [
        memory_unit_with_entry(consumer),
        unit_with_provider_order(&format!("{IMPORTS}{consumer}"), "", false),
        unit_with_provider_order(&format!("{IMPORTS}{consumer}"), "", true),
    ]
}

#[test]
fn range_scalar_counts_single_and_unit_transport_live_parent_child_roots() {
    let consumer = r#"
class Item(val text:String){deinit(){println(this.text)}}
fun source():List<Item>{println("source");return listOf(Item("a"),Item("b"),Item("c"),Item("d"))}
fun count(n:Int):Int{println("count");return n}
fun read(view:View<Item>):Unit{for(item in view){println(item.text)}}
fun consume(own root:List<Item>):Unit{println("consume")}
fun scan(n:Int):Unit{
    val root=source()
    borrow val parent=take(root,count(n)-1)
    borrow val child=parent.drop(count(n)/2-1)
    borrow val sibling=dropLast(parent,count(n)%3)
    read(child.take(count(n)*1-2))
    read(take(sibling,count(n)+0))
    read(root.dropLast(root.size-1))
    read(drop(child,child.size-1))
    read(parent)
    consume(root)
    println("done")
}
fun main():Unit{scan(4)}
"#;
    for (index, (program, entry)) in programs(consumer).into_iter().enumerate() {
        let module = &program.modules[0];
        let function = module
            .functions
            .iter()
            .find(|function| {
                function
                    .instructions
                    .iter()
                    .filter(|instruction| {
                        matches!(instruction.operation, Operation::CheckedArithmetic { .. })
                    })
                    .count()
                    >= 8
            })
            .expect("dynamic consumer must retain its checked arithmetic");
        let mut transports_parent_and_child = false;
        for block in &function.blocks {
            if !block.instructions.iter().any(|id| {
                matches!(
                    function.instructions[id.index()].operation,
                    Operation::CheckedArithmetic { .. }
                )
            }) {
                continue;
            }
            let TerminatorKind::Conditional {
                when_true,
                when_false,
                ..
            } = &block.terminator.as_ref().unwrap().kind
            else {
                panic!("checked branch")
            };
            assert!(matches!(
                function.blocks[when_true.target.index()]
                    .terminator
                    .as_ref()
                    .unwrap()
                    .kind,
                TerminatorKind::Abort
            ));
            let parameters = &function.blocks[when_false.target.index()].parameters;
            let descriptors = parameters.iter().filter(|entity| matches!(
                function.entity(**entity).unwrap().ty,
                EntityType::Value(ty) if matches!(module.type_kind(ty), Some(SsaTypeKind::RangeView { .. }))
            )).count();
            let loans = parameters
                .iter()
                .filter(|entity| matches!(entity, EntityId::Loan(_)))
                .count();
            transports_parent_and_child |= descriptors >= 2 && loans >= 2;
        }
        assert!(
            transports_parent_and_child,
            "checked success edges must carry live descriptors and source loans"
        );
        let run = run_unmodified(&program, entry, &format!("live-{index}"));
        assert_eq!(run.status.code(), Some(0), "{run:?}");
        assert_eq!(run.stdout, b"source\ncount\ncount\ncount\ncount\nb\nc\ncount\na\nb\na\nc\na\nb\nc\nconsume\nd\nc\nb\na\ndone\n");
        assert!(run.stderr.is_empty(), "{run:?}");
    }
}

fn run_unmodified(program: &Program, entry: FunctionId, label: &str) -> std::process::Output {
    use std::{fs, process::Command};
    let directory = std::env::temp_dir().join(format!(
        "koven-range-scalar-counts-{}-{label}",
        std::process::id()
    ));
    fs::create_dir_all(&directory).unwrap();
    let ir = directory.join("program.ll");
    let executable = directory.join("program");
    let llvm = crate::llvm::render_verified_program_with_entry(program, entry).unwrap();
    fs::write(&ir, llvm).unwrap();
    let linked = Command::new(crate::test_support::ir_clang())
        .arg(&ir)
        .arg("-o")
        .arg(&executable)
        .output()
        .unwrap();
    assert!(linked.status.success(), "{linked:?}");
    let run = Command::new(&executable).output().unwrap();
    fs::remove_dir_all(directory).unwrap();
    run
}

#[cfg(unix)]
#[test]
fn range_scalar_counts_normal_source_arithmetic_and_negative_counts_abort() {
    use std::os::unix::process::ExitStatusExt;
    for (case, expression, n) in [
        ("add-overflow", "n+1", "2147483647"),
        ("subtract-overflow", "n-1", "-2147483648"),
        ("multiply-overflow", "n*2", "2147483647"),
        ("divide-zero", "1/n", "0"),
        ("remainder-zero", "1%n", "0"),
        ("divide-overflow", "n/-1", "-2147483648"),
        ("remainder-minimum", "n%-1", "-2147483648"),
        ("negative-count", "n-2", "1"),
        ("minimum-expression", "(-2147483647-1)", "0"),
    ] {
        let consumer = format!(
            r#"
class Item(val text:String){{deinit(){{println(this.text)}}}}
fun read(view:View<Item>):Unit{{println("unreachable")}}
fun scan(n:Int):Unit{{
    val root=listOf(Item("dropped"))
    borrow val parent=take(root,1)
    borrow val child=parent.dropLast(0)
    read(drop(child,{expression}))
    read(parent)
}}
fun main():Unit{{scan({n})}}
"#
        );
        for (index, (program, entry)) in programs(&consumer).into_iter().enumerate() {
            let run = run_unmodified(&program, entry, &format!("{case}-{index}"));
            assert_eq!(run.status.signal(), Some(6), "{case}: {run:?}");
            assert!(
                run.stdout.is_empty(),
                "consumer and cleanup must not run: {run:?}"
            );
        }
    }
}

#[test]
fn range_scalar_counts_keep_non_int_binary_and_wider_cfg_gates() {
    for (statement, fragment) in [
        ("val other=1L+2L", "1L+2L"),
        ("val other=\"a\"+\"b\"", "\"a\"+\"b\""),
        ("val other=true==false", "true==false"),
        ("val other=1L==2L", "1L==2L"),
        ("val other=n shl 1", "n shl 1"),
        ("if(true){read(part)}", "if(true){read(part)}"),
        ("while(false){read(part)}", "while(false){read(part)}"),
    ] {
        let consumer = format!(
            "fun read(view:View<String>):Unit{{}}\nfun scan(n:Int):Unit{{val root=listOf(\"a\");borrow val part=take(root,1);{statement};read(part)}}\nfun main():Unit{{scan(1)}}"
        );
        assert_source_rejected(&consumer, fragment);
    }
}

#[test]
fn range_scalar_counts_keep_value_if_in_return_outside_gate() {
    let expression = "if(flag){read(part)}else{0}";
    let consumer = format!(
        r#"
fun read(view:View<String>):Int{{return 1}}
fun scan(flag:Boolean):Int{{
    val root=listOf("a")
    borrow val part=take(root,1)
    return {expression}
}}
fun main():Unit{{val result=scan(true)}}
"#
    );
    assert_source_rejected(&consumer, expression);
}

#[test]
fn range_scalar_counts_keep_metadata_alias_boundary() {
    let consumer = r#"
fun read(view:View<String>):Unit{}
fun scan(n:Int):Unit{val root=listOf("a");borrow val part=take(root,1);borrow val alias=part;read(alias);read(take(part,n-1));read(part)}
fun main():Unit{scan(1)}
"#;
    assert_source_rejected(consumer, "n-1");
}

#[test]
fn range_scalar_counts_compose_with_ordinary_root_alias() {
    let consumer = r#"
fun read(view:View<String>):Unit{for(item in view){println(item)}}
fun scan(n:Int):Unit{val root=listOf("a");borrow val part=take(root,1);val text="kept";borrow val alias=text;println(alias);read(take(part,n-1));read(part)}
fun main():Unit{scan(1);scan(2)}
"#;
    for (index, (program, entry)) in programs(consumer).into_iter().enumerate() {
        let run = run_unmodified(&program, entry, &format!("ordinary-alias-{index}"));
        assert_eq!(run.status.code(), Some(0), "{run:?}");
        assert_eq!(run.stdout, b"kept\na\nkept\na\na\n", "{run:?}");
        assert!(run.stderr.is_empty(), "{run:?}");
    }
}

fn assert_source_rejected(consumer: &str, fragment: &str) {
    let mut sources = SourceMap::new();
    let text = format!(
        "package app\n{}\n{consumer}",
        include_str!("../../../../lang-std/koven/algorithms/ranges.ko")
            .strip_prefix("package koven.algorithms\n")
            .unwrap()
    );
    let (source, parsed) = crate::ssa::unit_lower_test_support::parsed(
        &mut sources,
        "range-scalar-frontier.ko",
        &text,
    );
    let (environment, mut types) = standard_environments();
    types.authorize_range_source(&sources, source).unwrap();
    types
        .authorize_range_extension_source(&sources, source)
        .unwrap();
    let inputs = [SourceUnitInput::new(
        "app",
        "app/frontier.ko",
        source,
        &parsed,
    )];
    let (names, typed, owned) =
        crate::ssa::unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
    let error = crate::ssa::unit_lower::lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &types,
        &typed,
        &owned,
        crate::ssa::unit_lower_test_support::declaration(&names, "app", "main"),
    )
    .err()
    .expect("the unit capability gate must reject this source");
    assert_eq!(error.kind, crate::ssa::LoweringErrorKind::UnsupportedNode);
    assert_eq!(sources.slice(error.span.unwrap()).unwrap(), fragment);
    assert_memory_source_rejected(&text, fragment);
}

#[test]
fn range_scalar_counts_pending_temporary_source_and_descriptor_evaluate_once() {
    let consumer = r#"
class Item(val text:String){deinit(){println(this.text)}}
fun source():List<Item>{println("source");return listOf(Item("a"),Item("b"))}
fun count(n:Int):Int{println("count");return n}
fun read(view:View<Item>,n:Int):Unit{println("read");for(item in view){println(item.text)}}
fun scan(n:Int):Unit{
    val root=listOf(Item("root"))
    borrow val parent=take(root,1)
    read(take(source(),count(n)-1),count(n)+0)
    read(source().dropLast(count(n)-1),count(n)*1)
    read(parent,count(n)%2)
    println("done")
}
fun main():Unit{scan(2)}
"#;
    for (index, (program, entry)) in programs(consumer).into_iter().enumerate() {
        let run = run_unmodified(&program, entry, &format!("pending-{index}"));
        assert_eq!(run.status.code(), Some(0), "{run:?}");
        assert_eq!(run.stdout, b"source\ncount\ncount\nread\na\nb\na\nsource\ncount\ncount\nread\na\nb\na\ncount\nread\nroot\ndone\nroot\n");
        assert!(run.stderr.is_empty(), "{run:?}");
    }
}

// The four former UnsupportedNode fixtures now bind a genuine Boolean result.
#[test]
fn range_scalar_counts_selected_int_comparisons_bind_boolean_locals() {
    for (expression, expected) in [
        ("n!=1", "true\na\nfalse\na\ntrue\na\n"),
        ("n<=1", "true\na\ntrue\na\nfalse\na\n"),
        ("n>1", "false\na\nfalse\na\ntrue\na\n"),
        ("n>=1", "false\na\ntrue\na\ntrue\na\n"),
    ] {
        let consumer = format!(
            r#"
fun check(value:Boolean):Unit{{if(value){{println("true")}}else{{println("false")}}}}
fun read(view:View<String>):Unit{{for(item in view){{println(item)}}}}
fun scan(n:Int):Unit{{val root=listOf("a");borrow val part=take(root,1);val other={expression};check(other);read(part)}}
fun main():Unit{{scan(0);scan(1);scan(2)}}
"#
        );
        for (index, (program, entry)) in programs(&consumer).into_iter().enumerate() {
            let run = run_unmodified(&program, entry, &format!("selected-{expression}-{index}"));
            assert_eq!(run.status.code(), Some(0), "{run:?}");
            assert_eq!(run.stdout, expected.as_bytes(), "{run:?}");
            assert!(run.stderr.is_empty(), "{run:?}");
        }
    }
}

// SPEC-0317 keeps the precise local-initializer form of the former refusal.
#[test]
fn range_scalar_counts_selected_boolean_short_circuit_local() {
    let consumer = "fun read(view:View<String>):Unit{for(item in view){println(item)}}\nfun scan(n:Int):Unit{val root=listOf(\"a\");borrow val part=take(root,1);val other=true && false;read(part)}\nfun main():Unit{scan(1)}";
    for (index, (program, entry)) in programs(consumer).into_iter().enumerate() {
        let run = run_unmodified(&program, entry, &format!("short-circuit-selected-{index}"));
        assert_eq!(run.status.code(), Some(0), "{run:?}");
        assert_eq!(run.stdout, b"a\n");
        assert!(run.stderr.is_empty(), "{run:?}");
    }
}

// MemoryDocument Unit preserves the original Single layout; project-key Unit remains.
fn assert_memory_source_rejected(text: &str, fragment: &str) {
    use crate::native_tests::boxed_enum_tests::unit_source::UnitSource;
    use lang_frontend::{
        name_resolution::{SourceUnitKey, SymbolKind},
        parser::{Item, NameMarker},
    };

    let mut authorized_source = None;
    let analysis = UnitSource::analyze_with_type_environment(
        "range-scalar-frontier.ko",
        text,
        |sources, source, types| {
            types.authorize_range_source(sources, source).unwrap();
            types
                .authorize_range_extension_source(sources, source)
                .unwrap();
            authorized_source = Some(source);
        },
    )
    .unwrap_or_else(|error| panic!("original frontier MemoryDocument admission: {error:?}"));
    analysis.assert_frontend_clean();
    assert!(analysis.ownership().deferred().is_empty());
    let parsed = analysis.parsed();
    let sources = analysis.sources();
    let index = analysis.names().names().index();
    let [source] = index.source_units() else {
        panic!("one original frontier MemoryDocument");
    };
    assert_eq!(source.key(), &SourceUnitKey::MemoryDocument);
    assert_eq!(source.source_id(), parsed.source_id());
    assert_eq!(authorized_source, Some(source.source_id()));
    assert_eq!(
        sources.source_name(source.source_id()).unwrap(),
        "range-scalar-frontier.ko"
    );
    assert_eq!(sources.source_text(source.source_id()).unwrap(), text);

    // Reconcile the actual AST/index/signatures before lowering any root. Generic
    // templates remain in full admission and lower only as real reachable instances.
    let mut functions = Vec::new();
    let mut concrete = Vec::new();
    for root in parsed.roots() {
        let original = parsed.ast().items().get(*root).unwrap();
        let mut node = original;
        while let Item::Modified { declaration, .. } = node.payload() {
            node = parsed.ast().items().get(*declaration).unwrap();
        }
        let Item::Function {
            name: NameMarker::Present(name),
            type_parameters,
            parameters,
            ..
        } = node.payload()
        else {
            continue;
        };
        let declarations = index
            .declarations()
            .iter()
            .filter(|declaration| {
                declaration.root() == *root
                    && declaration.source_unit() == source.id()
                    && declaration.kind() == SymbolKind::Function
            })
            .collect::<Vec<_>>();
        let [declaration] = declarations.as_slice() else {
            panic!("one real declaration for every original function root");
        };
        assert_eq!(declaration.package(), source.package());
        assert_eq!(declaration.name_span(), *name);
        assert_eq!(name.source_id(), source.source_id());
        assert_eq!(sources.slice(*name).unwrap(), declaration.name());
        assert_eq!(analysis.declaration_span(declaration.id()), original.span());
        let signature = analysis
            .types()
            .signatures()
            .declaration(declaration.id())
            .and_then(|signature| signature.callable())
            .expect("same-analysis callable signature");
        assert_eq!(signature.name_span(), *name);
        assert_eq!(signature.type_parameters().len(), type_parameters.len());
        assert_eq!(signature.parameters().len(), parameters.len());
        assert!(!functions.contains(&declaration.id()));
        functions.push(declaration.id());
        if type_parameters.is_empty() {
            concrete.push(*declaration);
        }
    }
    assert_eq!(functions.len(), 15);
    assert_eq!(
        functions.len(),
        index
            .declarations()
            .iter()
            .filter(|declaration| declaration.kind() == SymbolKind::Function)
            .count()
    );
    assert_eq!(
        concrete.iter().map(|root| root.name()).collect::<Vec<_>>(),
        ["read", "scan", "main"]
    );
    let scan = analysis.declaration("scan", SymbolKind::Function);
    let main = analysis.declaration("main", SymbolKind::Function);
    assert_eq!(concrete[1].id(), scan);
    assert_eq!(concrete[2].id(), main);
    // Restrict the fragment to the actual scan declaration: stdlib templates also
    // contain return/binary spellings and must not supply the expected span.
    let scan_span = analysis.declaration_span(scan);
    let offsets = sources
        .slice(scan_span)
        .unwrap()
        .match_indices(fragment)
        .map(|(offset, _)| offset)
        .collect::<Vec<_>>();
    let [offset] = offsets.as_slice() else {
        panic!("one exact original scalar frontier in scan");
    };
    let start = scan_span.start() + offset;
    let expected_span = sources
        .span(source.source_id(), start, start + fragment.len())
        .unwrap();
    // The borrow-binding gate is whole-set and precedes instance planning, so all
    // actual entries reject at the same source location, not only reachable scan.
    for root in concrete {
        let error = analysis
            .lower_entry(root.id())
            .err()
            .expect("original scalar frontier must reject before emission");
        assert_eq!(error.kind, crate::ssa::LoweringErrorKind::UnsupportedNode);
        assert_eq!(error.span, Some(expected_span), "root {}", root.name());
        assert_eq!(sources.slice(error.span.unwrap()).unwrap(), fragment);
    }
}

pub(in crate::ssa) mod local_control_assertions {
    //! Identity-based checks over valid, source-produced SSA only.
    use crate::ssa::range_lowering_tests::local_control::*;
    use lang_frontend::source::Span;

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct Carriers {
        root: ValueId,
        root_loan: LoanId,
        descriptors: Vec<(ValueId, LoanId)>,
    }

    fn defining_instruction(function: &Function, entity: EntityId) -> &Instruction {
        let Definition::InstructionResult { instruction, .. } =
            function.entity(entity).unwrap().definition
        else {
            panic!("expected an instruction-defined entity: {entity:?}")
        };
        function.instruction(instruction).unwrap()
    }

    fn borrowed_owner(function: &Function, loan: LoanId) -> ValueId {
        let Operation::BorrowBegin {
            place,
            kind: LoanKind::Shared,
        } = defining_instruction(function, EntityId::Loan(loan)).operation
        else {
            panic!("source/read loan must be an explicit shared borrow")
        };
        let Operation::RootPlace { owner } =
            defining_instruction(function, EntityId::Place(place)).operation
        else {
            panic!("source/read must borrow a whole root")
        };
        owner
    }

    fn initial_carriers(function: &Function, count: usize) -> Carriers {
        let calls = function
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::RangeCall { .. }))
            .collect::<Vec<_>>();
        assert_eq!(calls.len(), count);
        let Operation::RangeCall {
            source: root_loan, ..
        } = calls[0].operation
        else {
            unreachable!()
        };
        let root = borrowed_owner(function, root_loan);
        let descriptors = calls
            .iter()
            .map(|call| {
                let [EntityId::Value(view), EntityId::Loan(source)] = call.results.as_slice() else {
                    panic!("a range call must return a descriptor and its root continuation")
                };
                (*view, *source)
            })
            .collect::<Vec<_>>();
        // In the larger fixture child and sibling borrow the same parent metadata;
        // their returned continuations still protect the collection, not that metadata.
        for call in &calls[1..] {
            let Operation::RangeCall { source, .. } = call.operation else {
                unreachable!()
            };
            assert_eq!(borrowed_owner(function, source), descriptors[0].0);
            let end = function.instructions.iter().find(|instruction| {
                matches!(instruction.operation, Operation::BorrowEnd { loan } if loan == source)
            }).expect("temporary producer metadata borrow must end");
            assert_eq!(end.block, call.block);
            assert!(instruction_position(function, call) < instruction_position(function, end));
        }
        let root_type = function
            .entity(EntityId::Value(root))
            .unwrap()
            .ty
            .semantic_type();
        for (_, source) in &descriptors {
            assert_eq!(
                function.entity(EntityId::Loan(*source)).unwrap().ty,
                EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: root_type
                }
            );
        }
        Carriers {
            root,
            root_loan,
            descriptors,
        }
    }

    fn instruction_position(function: &Function, instruction: &Instruction) -> usize {
        function.blocks[instruction.block.index()]
            .instructions
            .iter()
            .position(|id| *id == instruction.id)
            .unwrap()
    }

    fn carried_entity(function: &Function, edge: &Edge, entity: EntityId) -> EntityId {
        let positions = edge
            .arguments
            .iter()
            .enumerate()
            .filter_map(|(index, actual)| (*actual == entity).then_some(index))
            .collect::<Vec<_>>();
        assert_eq!(
            positions.len(),
            1,
            "edge must carry {entity:?} exactly once: {edge:?}"
        );
        let rebound = function.blocks[edge.target.index()].parameters[positions[0]];
        assert_eq!(
            function.entity(entity).unwrap().ty,
            function.entity(rebound).unwrap().ty
        );
        assert_ne!(
            entity, rebound,
            "successor must use its own parameter identity"
        );
        rebound
    }

    fn carried(function: &Function, edge: &Edge, state: &Carriers) -> Carriers {
        let EntityId::Value(root) = carried_entity(function, edge, EntityId::Value(state.root)) else {
            panic!("root remains an owned value")
        };
        let EntityId::Loan(root_loan) = carried_entity(function, edge, EntityId::Loan(state.root_loan))
        else {
            panic!("source remains a loan")
        };
        let descriptors = state
            .descriptors
            .iter()
            .map(|(view, source)| {
                let EntityId::Value(view) = carried_entity(function, edge, EntityId::Value(*view))
                else {
                    panic!("descriptor remains a value")
                };
                let EntityId::Loan(source) = carried_entity(function, edge, EntityId::Loan(*source))
                else {
                    panic!("continuation remains a loan")
                };
                (view, source)
            })
            .collect();
        Carriers {
            root,
            root_loan,
            descriptors,
        }
    }

    fn read_end_position(function: &Function, block: BlockId, loan: LoanId) -> usize {
        let ends = function.blocks[block.index()]
            .instructions
            .iter()
            .enumerate()
            .filter_map(|(position, id)| {
                matches!(function.instructions[id.index()].operation,
                    Operation::BorrowEnd { loan: actual } if actual == loan)
                .then_some(position)
            })
            .collect::<Vec<_>>();
        assert_eq!(
            ends.len(),
            1,
            "each metadata/source loan must end exactly once: {loan:?}"
        );
        ends[0]
    }

    fn assert_reads(function: &Function, block: BlockId, state: &Carriers) {
        let instructions = &function.blocks[block.index()].instructions;
        for (view, _) in &state.descriptors {
            let calls = instructions
                .iter()
                .filter_map(|id| {
                    let instruction = function.instruction(*id).unwrap();
                    let Operation::DirectCall { arguments, .. } = &instruction.operation else {
                        return None;
                    };
                    let [EntityId::Loan(loan)] = arguments.as_slice() else {
                        return None;
                    };
                    (borrowed_owner(function, *loan) == *view).then_some((instruction, *loan))
                })
                .collect::<Vec<_>>();
            assert_eq!(
                calls.len(),
                1,
                "each descriptor must be read once in {block:?}"
            );
            let (call, loan) = calls[0];
            assert!(instruction_position(function, call) < read_end_position(function, block, loan));
        }
    }

    fn assert_cleanup(function: &Function, block: BlockId, state: &Carriers) {
        let instructions = function.blocks[block.index()]
            .instructions
            .iter()
            .map(|id| function.instruction(*id).unwrap())
            .collect::<Vec<_>>();
        let source_end = read_end_position(function, block, state.root_loan);
        let consume = instructions
            .iter()
            .position(|instruction| {
                matches!(&instruction.operation, Operation::DirectCall { arguments, .. }
                if arguments.as_slice() == [EntityId::Value(state.root)])
            })
            .expect("the same root owner must be transferred to consume after scope exit");
        for (descriptor_index, (view, source)) in state.descriptors.iter().enumerate() {
            let ends = instructions.iter().enumerate().filter_map(|(index, instruction)| {
                matches!(instruction.operation, Operation::RangeEnd { view: actual_view, source: actual_source }
                    if actual_view == *view && actual_source == *source).then_some(index)
            }).collect::<Vec<_>>();
            assert_eq!(
                ends.len(),
                1,
                "the exact descriptor/root-continuation pair must end once"
            );
            let reads = instructions
                .iter()
                .filter_map(|instruction| {
                    let Operation::DirectCall { arguments, .. } = &instruction.operation else {
                        return None;
                    };
                    let [EntityId::Loan(loan)] = arguments.as_slice() else {
                        return None;
                    };
                    (borrowed_owner(function, *loan) == *view).then_some((*instruction, *loan))
                })
                .collect::<Vec<_>>();
            assert!(
                !reads.is_empty(),
                "each descriptor is used before its scope ends"
            );
            for (read, loan) in reads {
                let read_end = read_end_position(function, block, loan);
                assert!(instruction_position(function, read) < read_end);
                assert!(
                    read_end < ends[0],
                    "metadata read loan must end before its descriptor RangeEnd"
                );
            }
            assert!(
                ends[0] < consume,
                "every continuation protects the root until its own RangeEnd"
            );
            if descriptor_index == 0 {
                assert!(
                    ends[0] < source_end,
                    "the parent descriptor must end before its source loan"
                );
            }
        }
        assert!(
            source_end < consume,
            "RangeEnd -> source BorrowEnd -> root consumption"
        );
    }

    pub(in crate::ssa) fn comparison_lifetime(program: &Program, span: Span, position: Position) {
        let function = program.modules[0]
            .functions
            .iter()
            .find(|function| {
                function.instructions.iter().any(|instruction| {
                    instruction.origin.span() == span
                        && matches!(instruction.operation, Operation::Compare { .. })
                })
            })
            .unwrap();
        let comparison = function
            .instructions
            .iter()
            .find(|instruction| {
                instruction.origin.span() == span
                    && matches!(instruction.operation, Operation::Compare { .. })
            })
            .unwrap();
        let state = initial_carriers(function, 1);
        let call = defining_instruction(function, EntityId::Value(state.descriptors[0].0));
        let end = function.instructions.iter().find(|instruction| {
            matches!(instruction.operation, Operation::RangeEnd { view, .. } if view == state.descriptors[0].0)
        }).unwrap();
        assert_eq!(comparison.block, call.block);
        assert_eq!(comparison.block, end.block);
        let compared = instruction_position(function, comparison);
        match position {
            Position::BeforeBinding => assert!(compared < instruction_position(function, call)),
            Position::WhileLive => {
                assert!(instruction_position(function, call) < compared);
                assert!(compared < instruction_position(function, end));
            }
            Position::AfterEnds => {
                let source_end = function.instructions.iter().find(|instruction| {
                    matches!(instruction.operation, Operation::BorrowEnd { loan } if loan == state.root_loan)
                }).unwrap();
                assert!(
                    instruction_position(function, end) < instruction_position(function, source_end)
                );
                assert!(instruction_position(function, source_end) < compared);
            }
        }
        assert_cleanup(function, comparison.block, &state);
    }

    pub(in crate::ssa) fn if_transport(program: &Program, span: Span, count: usize) {
        let function = program.modules[0]
            .functions
            .iter()
            .find(|function| {
                function.blocks.iter().any(|block| {
                    block.terminator.as_ref().is_some_and(|terminator| {
                        terminator.origin.span() == span
                            && matches!(terminator.kind, TerminatorKind::Conditional { .. })
                    })
                })
            })
            .expect("source statement-if must retain its exact conditional terminator");
        let branch = function
            .blocks
            .iter()
            .find(|block| {
                block.terminator.as_ref().is_some_and(|terminator| {
                    terminator.origin.span() == span
                        && matches!(terminator.kind, TerminatorKind::Conditional { .. })
                })
            })
            .unwrap();
        let TerminatorKind::Conditional {
            when_true,
            when_false,
            ..
        } = &branch.terminator.as_ref().unwrap().kind
        else {
            unreachable!()
        };
        assert_ne!(when_true.target, when_false.target);
        let state = initial_carriers(function, count);
        let mut exits = Vec::new();
        for edge in [when_true, when_false] {
            let arm_state = carried(function, edge, &state);
            assert_reads(function, edge.target, &arm_state);
            let arm = &function.blocks[edge.target.index()];
            assert!(
                !arm.instructions.iter().any(|id| matches!(
                    function.instructions[id.index()].operation,
                    Operation::RangeEnd { .. } | Operation::Drop { .. } | Operation::Consume { .. }
                )),
                "normal arms must preserve the root and all descriptors for the merge"
            );
            let TerminatorKind::Branch(merge) = &arm.terminator.as_ref().unwrap().kind else {
                panic!("both statement-if arms must finish normally")
            };
            exits.push((merge.target, carried(function, merge, &arm_state)));
        }
        assert_eq!(
            exits[0], exits[1],
            "both arms must merge the same descriptor/root relationships"
        );
        let (merge, state) = &exits[0];
        assert_reads(function, *merge, state);
        assert_cleanup(function, *merge, state);
    }

    /// The short circuit keeps its own conditional control transfer. The descriptor, root and
    /// loan identities on each edge are checked by the SSA verifier; the legacy carrier layout
    /// these tests once pinned retired with `unit_lower/range.rs` (SPEC-0393).
    pub(in crate::ssa) fn short_circuit_branch(program: &Program, span: Span) {
        let found = program.modules[0].functions.iter().any(|function| {
            function.blocks.iter().any(|block| {
                block.terminator.as_ref().is_some_and(|term| {
                    term.origin.span() == span
                        && matches!(term.kind, TerminatorKind::Conditional { .. })
                })
            })
        });
        assert!(
            found,
            "the dynamic source short circuit must emit its own conditional"
        );
    }
}
