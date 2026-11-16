//! Original overload source, one strict Unit transaction; no native execution.
use crate::ssa::{model::*, unit_lower::orchestrate::lower_roots, verify::verify_program};
use lang_frontend::{
    analysis::analyze_basic_unit_ownership,
    lexer::lex,
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::owned_compilation_unit_view,
    parser::parse_file,
    source::SourceMap,
    type_checking::{
        check_compilation_unit_types, complete_compilation_unit_types, standard_environments,
    },
};

fn signature(module: &Module, function: &Function, borrowed: bool, fallback: bool) {
    let block = function.block(function.entry_block().unwrap()).unwrap();
    assert_eq!(block.parameters.len(), 1 + usize::from(fallback));
    for (index, parameter) in block.parameters.iter().enumerate() {
        let actual = function.entity(*parameter).unwrap().ty;
        let ty = actual.semantic_type();
        assert_eq!(
            actual,
            if borrowed || index == 1 {
                EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: ty,
                }
            } else {
                EntityType::Value(ty)
            }
        );
        assert_eq!(
            module.type_kind(ty),
            Some(&if index == 1 {
                SsaTypeKind::Boolean
            } else {
                SsaTypeKind::Integer {
                    bits: 32,
                    signed: true,
                }
            })
        );
    }
    let [result] = function.return_types.as_slice() else {
        panic!("one original Int return")
    };
    assert_eq!(
        module.type_kind(*result),
        Some(&SsaTypeKind::Integer {
            bits: 32,
            signed: true
        })
    );
    assert!(function.receiver.is_none());
    assert!(function.borrow_return.is_none());
    assert!(function.carrier_return.is_none());
    assert!(!function.never_returns);
}

fn symbol(function: &Function) -> String {
    format!("f{}.{}", function.id.index(), function.name)
}

fn llvm_body<'a>(llvm: &'a str, function: &Function) -> &'a str {
    let name = symbol(function);
    let headers = llvm
        .lines()
        .filter(|line| {
            line.starts_with("define internal i32 ")
                && (line.contains(&format!("@\"{name}\"(")) || line.contains(&format!("@{name}(")))
        })
        .collect::<Vec<_>>();
    let [header] = headers.as_slice() else {
        panic!("one exact LLVM definition for {name}: {llvm}")
    };
    llvm.split_once(header)
        .unwrap()
        .1
        .split("\n}")
        .next()
        .unwrap()
}

pub(super) fn assert_distinct_targets() {
    let source = "fun <T> choose(own input: T): T = input\n\
         fun <T> choose(own input: T, fallback: Boolean): T = input\n\
         fun one(input: Int): Int = choose(input)\n\
         fun two(input: Int): Int = choose(input, true)";
    let mut sources = SourceMap::new();
    let source_id = sources.add_source("lowering.ko", source).unwrap();
    let lexed = lex(&sources, source_id).unwrap();
    assert!(lexed.diagnostics().is_empty());
    let parsed = parse_file(&sources, &lexed).unwrap();
    assert!(parsed.diagnostics().is_empty());
    let inputs = [SourceUnitInput::memory_document(source_id, &parsed)];
    let (name_env, type_env) = standard_environments();
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let raw_names = resolve_compilation_unit_names(&sources, &inputs, &index, &name_env).unwrap();
    assert!(raw_names.diagnostics().is_empty());
    let names = raw_names.validate().unwrap();
    let [document] = names.names().index().source_units() else {
        panic!("one memory document")
    };
    assert_eq!(document.source_id(), source_id);
    assert_eq!(
        document.key(),
        &lang_frontend::name_resolution::SourceUnitKey::MemoryDocument
    );
    assert_eq!(sources.source_text(source_id), Ok(source));
    let raw_types = check_compilation_unit_types(&sources, &inputs, &names, &type_env).unwrap();
    assert!(raw_types.diagnostics().is_empty());
    let complete =
        complete_compilation_unit_types(&sources, &inputs, &names, &type_env, &raw_types).unwrap();
    assert!(complete.is_same_analysis(&raw_types));
    let (typed, raw_owned) =
        analyze_basic_unit_ownership(&sources, &inputs, &names, &type_env, raw_types)
            .unwrap()
            .into_result()
            .unwrap();
    assert!(raw_owned.diagnostics().is_empty());
    assert!(raw_owned.deferred().is_empty());
    let owned = raw_owned.validate().unwrap();
    let original = |text: &str| {
        let declarations = names
            .names()
            .index()
            .declarations()
            .iter()
            .filter_map(|declaration| {
                let span = parsed.ast().items().get(declaration.root()).unwrap().span();
                (sources.slice(span).unwrap() == text).then_some((declaration.id(), span))
            })
            .collect::<Vec<_>>();
        let [declaration] = declarations.as_slice() else {
            panic!("unique exact original declaration: {text}")
        };
        *declaration
    };
    let one = original("fun one(input: Int): Int = choose(input)");
    let two = original("fun two(input: Int): Int = choose(input, true)");
    let expected = [
        original("fun <T> choose(own input: T): T = input"),
        original("fun <T> choose(own input: T, fallback: Boolean): T = input"),
    ];
    let view =
        owned_compilation_unit_view(&sources, &inputs, &names, &type_env, &typed, &owned).unwrap();
    let (program, roots) = lower_roots(&view, &[one.0, two.0]).unwrap();
    verify_program(&program).unwrap();
    assert_eq!(program.modules.len(), 1);
    let module = &program.modules[0];
    assert_eq!(module.functions.len(), 4);
    assert_eq!(roots.len(), 2);
    let choices = expected.map(|(_, span)| {
        let functions = module
            .functions
            .iter()
            .filter(|function| function.origin == Origin::Source(span))
            .collect::<Vec<_>>();
        let [function] = functions.as_slice() else {
            panic!("one concrete instance for each original overload")
        };
        *function
    });
    assert_ne!(choices[0].id, choices[1].id);
    let llvm = crate::llvm::render_verified_program(&program)
        .expect("distinct overload instances must lower");
    for (index, root) in [one, two].into_iter().enumerate() {
        let caller = module.function(roots[&root.0]).unwrap();
        assert_eq!(caller.origin, Origin::Source(root.1));
        signature(module, caller, true, false);
        signature(module, choices[index], false, index == 1);
        let targets = caller
            .instructions
            .iter()
            .filter_map(|instruction| match instruction.operation {
                Operation::DirectCall { callee, .. } => Some(callee),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(targets, [choices[index].id]);
        let own = symbol(choices[index]);
        let other = symbol(choices[1 - index]);
        // Unit names encode concrete type identity as .tN, not Legacy <Int> text.
        // Original generic source Span plus the Int ABI above proves specialization.
        let count = |text: &str, prefix: &str, name: &str| {
            text.matches(&format!("{prefix}@\"{name}\"(")).count()
                + text.matches(&format!("{prefix}@{name}(")).count()
        };
        assert_eq!(count(&llvm, "define internal i32 ", &own), 1);
        let body = llvm_body(&llvm, caller);
        assert_eq!(count(body, "call i32 ", &own), 1);
        assert_eq!(count(body, "call i32 ", &other), 0);
    }
}
