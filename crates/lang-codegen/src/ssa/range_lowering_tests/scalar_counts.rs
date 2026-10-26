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
        ("return", "return"),
    ] {
        let consumer = format!(
            "fun read(view:View<String>):Unit{{}}\nfun scan(n:Int):Unit{{val root=listOf(\"a\");borrow val part=take(root,1);{statement};read(part)}}\nfun main():Unit{{scan(1)}}"
        );
        assert_source_rejected(&consumer, fragment);
    }
}

#[test]
fn range_scalar_counts_do_not_widen_ordinary_borrow_or_metadata_alias_bindings() {
    for binding in [
        "val text=\"kept\";borrow val alias=text;println(alias)",
        "borrow val alias=part;read(alias)",
    ] {
        let consumer = format!(
            "fun read(view:View<String>):Unit{{}}\nfun scan(n:Int):Unit{{val root=listOf(\"a\");borrow val part=take(root,1);{binding};read(take(part,n-1));read(part)}}\nfun main():Unit{{scan(1)}}"
        );
        assert_source_rejected(&consumer, "n-1");
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
