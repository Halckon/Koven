//! Original nominal construction/projection/drop roots in one strict Unit transaction.
use super::{assert_diagnostic, assert_eq_diagnostic, render_program, render_verified_program};
use crate::ssa::{model::*, unit_lower::orchestrate::lower_roots, verify::verify_program};
use lang_frontend::{
    analysis::analyze_basic_unit_ownership,
    lexer::lex,
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::owned_compilation_unit_view,
    parser::parse_file,
    source::SourceMap,
    type_checking::{
        BuiltinType, IntrinsicTypeConstructor, UnitTypeKind, check_compilation_unit_types,
        complete_compilation_unit_types, standard_environments,
    },
};

pub(super) fn assert_original_nominal_roots() {
    let source = "value class Wrapped(val item: Int)\n\
         class Holder(val item: Int)\n\
         class Resource {}\n\
         enum class Flag { On, Off }\n\
         enum class Maybe<T> { Some(item: T), None }\n\
         enum class Owned<T> { Some(item: T), None }\n\
         fun wrapped(): Wrapped = Wrapped(1)\n\
         fun holder(): Holder = Holder(2)\n\
         fun boxed(): Box<Wrapped> = Box(Wrapped(3))\n\
         fun flag(): Flag = Flag.On\n\
         fun some(): Maybe<Int> = Maybe.Some(4)\n\
         fun enumProjected(): Int { val maybe: Maybe<Int> = Maybe.Some(8); val result: Int = when (maybe) {\n\
             is Maybe.Some<Int> -> maybe.item\n\
             is Maybe.None<Int> -> 0\n\
         }; return result }\n\
         fun projected(): Int { val wrapped = Wrapped(5); return wrapped.item }\n\
         fun destructured(): Int { val (item) = Wrapped(6); return item }\n\
         fun classProjected(): Int { val holder = Holder(7); return holder.item }\n\
         fun dropEnum(): Unit { val event: Owned<Resource> = Owned.Some(Resource()) }";
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

    let root_names = [
        "wrapped",
        "holder",
        "boxed",
        "flag",
        "some",
        "enumProjected",
        "projected",
        "destructured",
        "classProjected",
        "dropEnum",
    ];
    let declarations = root_names.map(|name| {
        let matches = names
            .names()
            .index()
            .declarations()
            .iter()
            .filter(|d| {
                d.name() == name
                    && parsed.roots().contains(&d.root())
                    && d.kind() == lang_frontend::name_resolution::SymbolKind::Function
            })
            .collect::<Vec<_>>();
        let [declaration] = matches.as_slice() else {
            panic!("unique original root {name}")
        };
        (
            declaration.id(),
            parsed.ast().items().get(declaration.root()).unwrap().span(),
        )
    });
    let view =
        owned_compilation_unit_view(&sources, &inputs, &names, &type_env, &typed, &owned).unwrap();
    let (program, roots) = lower_roots(&view, &declarations.map(|(id, _)| id))
        .expect("all original nominal roots in one Unit Program");
    verify_program(&program).unwrap();
    assert_eq!(program.modules.len(), 1);
    assert_eq!(roots.len(), 10);
    assert_eq!(program.modules[0].functions.len(), 10);
    for (declaration, span) in declarations {
        let function = program.modules[0].function(roots[&declaration]).unwrap();
        assert_eq!(function.origin, Origin::Source(span));
        assert!(
            function
                .block(function.entry_block().unwrap())
                .unwrap()
                .parameters
                .is_empty()
        );
    }
    let ssa = render_program(&program);
    assert_eq_diagnostic!(ssa.matches("aggregate.construct").count(), 11, "{ssa}");
    assert_eq_diagnostic!(ssa.matches("heap.allocate").count(), 4, "{ssa}");
    assert_eq_diagnostic!(ssa.matches("tagged.construct").count(), 4, "{ssa}");
    assert_eq_diagnostic!(ssa.matches("aggregate.project").count(), 1, "{ssa}");
    assert_eq_diagnostic!(ssa.matches("aggregate.copy_explode").count(), 1, "{ssa}");
    assert_eq_diagnostic!(ssa.matches("heap.payload_place").count(), 1, "{ssa}");
    assert_eq_diagnostic!(ssa.matches("tagged.payload_place").count(), 1, "{ssa}");
    assert_eq_diagnostic!(ssa.matches("tagged.discriminant").count(), 2, "{ssa}");
    assert_eq_diagnostic!(ssa.matches("drop ").count(), 2, "{ssa}");
    // Unit encodes original declaration/type identity rather than Legacy display names.
    let nominal = |name: &str, arguments: Vec<_>| {
        let matching = names
            .names()
            .index()
            .declarations()
            .iter()
            .filter(|d| d.name() == name && parsed.roots().contains(&d.root()))
            .collect::<Vec<_>>();
        let [declaration] = matching.as_slice() else {
            panic!("unique original classifier {name}")
        };
        let ty = typed
            .types()
            .types()
            .find(&UnitTypeKind::Nominal {
                declaration: declaration.id(),
                arguments,
            })
            .expect("original concrete nominal type");
        (declaration.id(), ty)
    };
    let int = typed.types().types().builtin(BuiltinType::Int).unwrap();
    let wrapped = nominal("Wrapped", vec![]);
    let holder = nominal("Holder", vec![]);
    let resource = nominal("Resource", vec![]);
    let flag = nominal("Flag", vec![]);
    let maybe = nominal("Maybe", vec![int]);
    let owned_enum = nominal("Owned", vec![resource.1]);
    let boxed = typed
        .types()
        .types()
        .find(&UnitTypeKind::Intrinsic {
            constructor: IntrinsicTypeConstructor::Box,
            arguments: vec![wrapped.1],
        })
        .unwrap();
    for (expected, category) in [
        (
            format!("value#d{}.u{}", wrapped.0.index(), wrapped.1.index()),
            0,
        ),
        (
            format!("class#d{}.u{}", holder.0.index(), holder.1.index()),
            1,
        ),
        (format!("Box#u{}", boxed.index()), 1),
        (format!("enum#d{}.u{}", flag.0.index(), flag.1.index()), 2),
        (format!("enum#d{}.u{}", maybe.0.index(), maybe.1.index()), 2),
        (
            format!("enum#d{}.u{}", owned_enum.0.index(), owned_enum.1.index()),
            2,
        ),
    ] {
        assert_eq!(
            program.modules[0]
                .types
                .iter()
                .filter(|kind| match kind {
                    SsaTypeKind::Aggregate { name, .. } => category == 0 && name == &expected,
                    SsaTypeKind::HeapOwner { name, .. } => category == 1 && name == &expected,
                    SsaTypeKind::TaggedUnion { name, .. } => category == 2 && name == &expected,
                    _ => false,
                })
                .count(),
            1,
            "unique original nominal witness {expected}"
        );
    }

    let llvm = render_verified_program(&program).expect("nominal SSA must lower to LLVM");
    assert_eq_diagnostic!(llvm.matches("call ptr @malloc").count(), 4, "{llvm}");
    assert_eq_diagnostic!(llvm.matches("call void @free").count(), 2, "{llvm}");
    assert_diagnostic!(llvm.contains("%koven.enum.t"), "{llvm}");
    assert_diagnostic!(!llvm.contains("retain"), "{llvm}");
    assert_diagnostic!(!llvm.contains("clone"), "{llvm}");
}
