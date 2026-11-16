//! Original eight-declaration generic reachability through one strict Unit transaction.
use super::assert_text_eq;
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

pub(super) fn assert_reachable_instances() {
    let source = "fun <T> identity(own input: T): T = input\n\
         fun <T> relay(own input: T): T = identity(input)\n\
         fun <T> recurse(own input: T): T = recurse(input)\n\
         fun <T> unused(own input: T): T = input\n\
         fun useInt(input: Int): Int = relay<Int>(input)\n\
         fun useIntAgain(input: Int): Int = relay(input)\n\
         fun useLong(input: Long): Long = relay(input)\n\
         fun recursive(input: Int): Int = recurse(input)";
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

    let identity = original("fun <T> identity(own input: T): T = input");
    let relay = original("fun <T> relay(own input: T): T = identity(input)");
    let recurse = original("fun <T> recurse(own input: T): T = recurse(input)");
    let unused = original("fun <T> unused(own input: T): T = input");
    let concrete = [
        original("fun useInt(input: Int): Int = relay<Int>(input)"),
        original("fun useIntAgain(input: Int): Int = relay(input)"),
        original("fun useLong(input: Long): Long = relay(input)"),
        original("fun recursive(input: Int): Int = recurse(input)"),
    ];
    assert_eq!(names.names().index().declarations().len(), 8);
    let view =
        owned_compilation_unit_view(&sources, &inputs, &names, &type_env, &typed, &owned).unwrap();
    let root_ids = concrete.map(|(id, _)| id);
    let (program, roots) =
        lower_roots(&view, &root_ids).expect("original four roots through strict Unit");
    verify_program(&program).unwrap();
    let (repeated, repeated_roots) =
        lower_roots(&view, &root_ids).expect("repeat original Unit roots");
    verify_program(&repeated).unwrap();
    assert_text_eq!(
        crate::ssa::render_program(&program),
        crate::ssa::render_program(&repeated)
    );
    assert_eq!(
        roots.keys().collect::<Vec<_>>(),
        repeated_roots.keys().collect::<Vec<_>>()
    );
    for (declaration, id) in &roots {
        let other = repeated_roots[declaration];
        // Program ownership is deliberately fresh; compare stable placement, not opaque IDs.
        assert_eq!(
            (id.module().index(), id.index()),
            (other.module().index(), other.index())
        );
        assert_eq!(
            program.modules[0].function(*id).unwrap().origin,
            repeated.modules[0].function(other).unwrap().origin
        );
    }
    assert_eq!(program.modules.len(), 1);
    let module = &program.modules[0];
    assert_eq!(roots.len(), 4);
    assert_eq!(module.functions.len(), 9);
    let instance = |span, bits| {
        let matching = module
            .functions
            .iter()
            .filter(|function| {
                function.origin == Origin::Source(span)
                    && function.return_types.len() == 1
                    && module.type_kind(function.return_types[0])
                        == Some(&SsaTypeKind::Integer { bits, signed: true })
            })
            .collect::<Vec<_>>();
        let [function] = matching.as_slice() else {
            panic!("one instance for original declaration and concrete type")
        };
        signature(module, function, bits, false);
        *function
    };
    let identities = [instance(identity.1, 32), instance(identity.1, 64)];
    let relays = [instance(relay.1, 32), instance(relay.1, 64)];
    let recursive = instance(recurse.1, 32);
    assert!(
        !module
            .functions
            .iter()
            .any(|f| f.origin == Origin::Source(unused.1))
    );
    for (index, caller) in concrete.into_iter().enumerate() {
        let function = module.function(roots[&caller.0]).unwrap();
        assert_eq!(function.origin, Origin::Source(caller.1));
        signature(module, function, if index == 2 { 64 } else { 32 }, true);
        assert_eq!(
            calls(function),
            [if index == 3 {
                recursive.id
            } else {
                relays[usize::from(index == 2)].id
            }]
        );
    }
    for index in 0..2 {
        assert_ne!(identities[index].id, relays[index].id);
        assert_eq!(calls(relays[index]), [identities[index].id]);
        assert!(calls(identities[index]).is_empty());
    }
    assert_ne!(identities[0].id, identities[1].id);
    assert_ne!(relays[0].id, relays[1].id);
    assert_eq!(calls(recursive), [recursive.id]);
}

fn calls(function: &Function) -> Vec<FunctionId> {
    function
        .instructions
        .iter()
        .filter_map(|i| match i.operation {
            Operation::DirectCall { callee, .. } => Some(callee),
            _ => None,
        })
        .collect()
}

fn signature(module: &Module, function: &Function, bits: u16, borrowed: bool) {
    let block = function.block(function.entry_block().unwrap()).unwrap();
    let [parameter] = block.parameters.as_slice() else {
        panic!("one original scalar parameter")
    };
    let actual = function.entity(*parameter).unwrap().ty;
    let ty = actual.semantic_type();
    assert_eq!(
        actual,
        if borrowed {
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
        Some(&SsaTypeKind::Integer { bits, signed: true })
    );
    assert_eq!(function.return_types, [ty]);
    assert!(function.receiver.is_none());
    assert!(function.borrow_return.is_none());
    assert!(function.carrier_return.is_none());
    assert!(!function.never_returns);
}
