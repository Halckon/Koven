//! Original MemoryDocument contracts for the migrated Map control consumers.
//! Every original non-generic function is an independent Unit root, even if uncalled.
use crate::native_tests::boxed_enum_tests::unit_source::UnitSource;
use lang_frontend::{
    name_resolution::{SourceUnitKey, SymbolKind},
    parser::Item,
};

use super::model::{
    EntityType, FunctionId, LoanKind, Operation, Origin, Program, SsaTypeKind, TerminatorKind,
};

struct RootProgram {
    name: String,
    program: Program,
    entry: FunctionId,
}

pub(super) fn assert_source(text: &str, roots: &[&str]) {
    let analysis = analyze(text);
    lower_roots(&analysis, "lowering.ko", text, roots);
}

pub(super) fn assert_return_controls(name: &str, text: &str, roots: &[&str]) {
    let analysis = analyze(text);
    for root in lower_roots(&analysis, "lowering.ko", text, roots) {
        assert_returned_owner_not_dropped(&root.program);
    }
    println!("CONTROL {name}: VERIFIED_LLVM");
}

pub(super) fn assert_outer_resource(name: &str, text: &str) {
    let analysis = analyze(text);
    let programs = lower_roots(&analysis, "lowering.ko", text, &["operand", "cleanup"]);
    for root in &programs {
        assert_returned_owner_not_dropped(&root.program);
    }
    let deinits = analysis
        .parsed()
        .ast()
        .items()
        .iter()
        .filter_map(|(_, node)| {
            matches!(node.payload(), Item::Deinit { .. }).then_some(node.span())
        })
        .collect::<Vec<_>>();
    let [deinit] = deinits.as_slice() else {
        panic!("one original Guard deinit");
    };
    assert_eq!((deinit.start(), deinit.end()), (32, 63));
    let cleanup = programs.iter().find(|root| root.name == "cleanup").unwrap();
    let definitions = cleanup
        .program
        .modules
        .iter()
        .flat_map(|module| &module.functions)
        .filter(|function| function.origin == Origin::Source(*deinit))
        .collect::<Vec<_>>();
    let [definition] = definitions.as_slice() else {
        panic!("cleanup must emit the real Guard deinit dependency");
    };
    let Some(EntityType::Loan {
        kind: LoanKind::Shared,
        target,
    }) = definition.receiver()
    else {
        panic!("Guard deinit must retain its exact shared owner receiver");
    };
    let module = cleanup.program.module(definition.id.module()).unwrap();
    assert!(matches!(
        module.type_kind(target),
        Some(SsaTypeKind::HeapOwner { .. })
    ));
    assert_eq!(module.deinit(target), Some(definition.id));
    let function = cleanup
        .program
        .module(cleanup.entry.module())
        .unwrap()
        .function(cleanup.entry)
        .unwrap();
    assert!(function.instructions.iter().any(|instruction| {
        matches!(instruction.operation, Operation::HeapAllocate { owner, .. } if owner == target)
            && analysis.sources().slice(instruction.origin.span()).unwrap() == "Guard(\"outer\")"
    }), "the emitted deinit belongs to the actual original outer Guard owner");
    println!("CONTROL {name}: VERIFIED_LLVM");
}

pub(super) fn assert_trusted_range(text: &str) {
    let analysis =
        UnitSource::analyze_with_type_environment("trusted.ko", text, |sources, source, types| {
            types.authorize_range_source(sources, source).unwrap();
            types
                .authorize_range_extension_source(sources, source)
                .unwrap();
        })
        .unwrap_or_else(|error| panic!("original trusted range admission: {error:?}"));
    analysis.assert_frontend_clean();
    let programs = lower_roots(&analysis, "trusted.ko", text, &["main"]);
    let [root] = programs.as_slice() else {
        panic!("one original main root");
    };
    let templates = analysis
        .names()
        .names()
        .index()
        .declarations()
        .iter()
        .filter(|declaration| {
            declaration.kind() == SymbolKind::Function
                && analysis
                    .types()
                    .signatures()
                    .declaration(declaration.id())
                    .and_then(|signature| signature.callable())
                    .is_some_and(|signature| !signature.type_parameters().is_empty())
        })
        .collect::<Vec<_>>();
    assert_eq!(
        templates.len(),
        12,
        "the complete original generic library remains admitted"
    );
    let take = templates
        .iter()
        .filter(|declaration| {
            let span = analysis.declaration_span(declaration.id());
            declaration.name() == "take" && (span.start(), span.end()) == (93, 343)
        })
        .collect::<Vec<_>>();
    let [take] = take.as_slice() else {
        panic!("the real List<T> take declaration, not a synthetic generic entry");
    };
    let span = analysis.declaration_span(take.id());
    let instances = root
        .program
        .modules
        .iter()
        .flat_map(|module| &module.functions)
        .filter(|function| function.origin == Origin::Source(span))
        .collect::<Vec<_>>();
    let [instance] = instances.as_slice() else {
        panic!("main must emit its actual take instance");
    };
    let main = root
        .program
        .module(root.entry.module())
        .unwrap()
        .function(root.entry)
        .unwrap();
    assert!(main.instructions.iter().any(|instruction| {
        matches!(instruction.operation, Operation::RangeCall { callee, .. } if callee == instance.id)
    }));
    // The other eleven templates remain FE/ownership coverage, not codegen claims.
}

fn analyze(text: &str) -> UnitSource {
    let analysis = UnitSource::analyze("lowering.ko", text)
        .unwrap_or_else(|error| panic!("original Map source admission: {error:?}"));
    analysis.assert_frontend_clean();
    analysis
}

fn lower_roots(
    analysis: &UnitSource,
    path: &str,
    text: &str,
    expected_roots: &[&str],
) -> Vec<RootProgram> {
    let index = analysis.names().names().index();
    let [source] = index.source_units() else {
        panic!("one original MemoryDocument");
    };
    assert_eq!(source.key(), &SourceUnitKey::MemoryDocument);
    assert_eq!(source.source_id(), analysis.parsed().source_id());
    assert_eq!(
        analysis.sources().source_name(source.source_id()).unwrap(),
        path
    );
    assert_eq!(
        analysis.sources().source_text(source.source_id()).unwrap(),
        text
    );
    let mut declarations = index.declarations().iter().collect::<Vec<_>>();
    declarations.sort_by_key(|declaration| declaration.name_span().start());
    let mut roots = Vec::new();
    for declaration in declarations {
        assert_eq!(declaration.source_unit(), source.id());
        assert_eq!(declaration.package(), source.package());
        assert_eq!(declaration.name_span().source_id(), source.source_id());
        assert!(analysis.parsed().roots().contains(&declaration.root()));
        if declaration.kind() != SymbolKind::Function {
            continue;
        }
        let signature = analysis
            .types()
            .signatures()
            .declaration(declaration.id())
            .and_then(|signature| signature.callable())
            .expect("every original function has a real callable signature");
        if signature.type_parameters().is_empty() {
            roots.push((declaration, signature.parameters().len()));
        }
    }
    assert_eq!(
        roots
            .iter()
            .map(|(declaration, _)| declaration.name())
            .collect::<Vec<_>>(),
        expected_roots,
        "all original non-generic roots, in original source order"
    );
    roots
        .into_iter()
        .map(|(declaration, parameters)| {
            let name = declaration.name();
            let span = analysis.declaration_span(declaration.id());
            let (program, entry) = analysis
                .lower_entry(declaration.id())
                .unwrap_or_else(|error| panic!("{path} root {name}: {error:?}"));
            let function = program
                .module(entry.module())
                .unwrap()
                .function(entry)
                .unwrap();
            assert_eq!(function.origin, Origin::Source(span));
            super::verify::verify_program(&program)
                .unwrap_or_else(|error| panic!("{path} root {name} SSA: {error:?}"));
            let llvm = if name == "main" && parameters == 0 {
                crate::llvm::render_verified_program_with_entry(&program, entry)
            } else {
                crate::llvm::render_verified_program(&program)
            };
            llvm.unwrap_or_else(|error| panic!("{path} root {name} LLVM: {error:?}"));
            RootProgram {
                name: name.to_owned(),
                program,
                entry,
            }
        })
        .collect()
}

fn assert_returned_owner_not_dropped(program: &Program) {
    for function in program.modules.iter().flat_map(|module| &module.functions) {
        for block in &function.blocks {
            if let TerminatorKind::Return { values } = &block.terminator.as_ref().unwrap().kind {
                for value in values {
                    assert!(
                        !function.instructions.iter().any(|instruction| {
                            matches!(instruction.operation, Operation::Drop { owner } if owner == *value)
                        }),
                        "returned owner is never also dropped"
                    );
                }
            }
        }
    }
}
