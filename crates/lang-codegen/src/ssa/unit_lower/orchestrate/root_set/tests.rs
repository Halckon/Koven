//! Unchanged ordinary fixtures; no fault mutation, no synthetic entry wrapper.
use super::*;
use crate::{
    llvm::render_verified_program,
    ssa::{model::Origin, render::render_program},
};
use lang_frontend::{
    lexer::lex,
    name_resolution::{index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::{check_compilation_unit_ownership, owned_compilation_unit_view},
    parser::parse_file,
    type_checking::{check_compilation_unit_types, standard_environments},
};
const GENERICS: &str = "fun <T> identity(own input: T): T = input\n\
         fun <T> relay(own input: T): T = identity(input)\n\
         fun <T> recurse(own input: T): T = recurse(input)\n\
         fun <T> unused(own input: T): T = input\n\
         fun useInt(input: Int): Int = relay<Int>(input)\n\
         fun useIntAgain(input: Int): Int = relay(input)\n\
         fun useLong(input: Long): Long = relay(input)\n\
         fun recursive(input: Int): Int = recurse(input)";
const OVERLOADS: &str = "fun <T> choose(own input: T): T = input\n\
         fun <T> choose(own input: T, fallback: Boolean): T = input\n\
         fun one(input: Int): Int = choose(input)\n\
         fun two(input: Int): Int = choose(input, true)";

fn check(source: &str, root_names: &[&str], expected: usize) {
    let mut sources = SourceMap::new();
    let source_id = sources.add_source("test/roots.ko", source).unwrap();
    let lexed = lex(&sources, source_id).unwrap();
    let parsed = parse_file(&sources, &lexed).unwrap();
    assert!(parsed.diagnostics().is_empty());
    let inputs = [SourceUnitInput::new("test", "roots.ko", source_id, &parsed)];
    let (name_env, type_env) = standard_environments();
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &name_env)
        .unwrap()
        .validate()
        .unwrap();
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_env)
        .unwrap()
        .validate()
        .unwrap();
    let owned = check_compilation_unit_ownership(&sources, &inputs, &names, &type_env, &typed)
        .unwrap()
        .validate()
        .unwrap();
    let entries = root_names
        .iter()
        .map(|name| {
            let matches = names
                .names()
                .index()
                .declarations()
                .iter()
                .filter(|d| d.name() == *name)
                .map(|d| d.id())
                .collect::<Vec<_>>();
            let [id] = matches.as_slice() else {
                panic!("unique original root")
            };
            *id
        })
        .collect::<Vec<_>>();

    let view =
        owned_compilation_unit_view(&sources, &inputs, &names, &type_env, &typed, &owned).unwrap();
    let (program, roots) = lower_roots(&view, &entries).unwrap();
    assert_eq!(roots.len(), entries.len());
    assert_eq!(program.modules.len(), 1);
    let module = &program.modules[0];
    assert_eq!(module.functions.len(), expected);
    let llvm = render_verified_program(&program).unwrap();
    assert_eq!(llvm.matches("define internal ").count(), expected);
    for declaration in &entries {
        let id = roots[declaration];
        let function = module.function(id).unwrap();
        let source_decl = &names.names().index().declarations()[declaration.index()];
        assert_eq!(
            function.origin,
            Origin::Source(parsed.ast().items().get(source_decl.root()).unwrap().span())
        );
        let (single, single_id) = lower_unit_impl(
            (&sources, &inputs),
            &names,
            typed.types(),
            owned.ownership(),
            None,
            *declaration,
            None,
        )
        .unwrap();
        let (one, one_roots) = lower_roots(&view, &[*declaration]).unwrap();
        assert_eq!(single_id.index(), one_roots[declaration].index());
        assert_eq!(
            single_id.module().index(),
            one_roots[declaration].module().index()
        );
        assert_eq!(render_program(&single), render_program(&one));
    }
    let mut reversed = entries.clone();
    reversed.reverse();
    let (again, again_roots) = lower_roots(&view, &reversed).unwrap();
    // IDs include Program identity; compare stable positions across separate Programs.
    let positions = |map: &BTreeMap<DeclarationId, FunctionId>| {
        map.iter()
            .map(|(decl, id)| (*decl, id.module().index(), id.index()))
            .collect::<Vec<_>>()
    };
    assert_eq!(positions(&roots), positions(&again_roots));
    assert_eq!(render_program(&program), render_program(&again));
    assert_eq!(llvm, render_verified_program(&again).unwrap());
    let targets = |id| {
        module
            .function(id)
            .unwrap()
            .instructions
            .iter()
            .filter_map(|i| {
                if let Operation::DirectCall { callee, .. } = i.operation {
                    Some(callee)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
    };
    if root_names.len() == 4 {
        assert_eq!(targets(roots[&entries[0]]), targets(roots[&entries[1]]));
        assert_ne!(targets(roots[&entries[0]]), targets(roots[&entries[2]]));
        let recursive = targets(roots[&entries[3]]);
        assert_eq!(recursive.len(), 1);
        assert_eq!(targets(recursive[0]), recursive);
    } else {
        let one = targets(roots[&entries[0]]);
        let two = targets(roots[&entries[1]]);
        assert_eq!(one.len(), 1);
        assert_eq!(two.len(), 1);
        assert_ne!(one, two);
        assert_ne!(
            module.function(one[0]).unwrap().origin,
            module.function(two[0]).unwrap().origin
        );
    }
}

#[test]
fn ordinary_generic_roots_share_one_verified_program() {
    check(
        GENERICS,
        &["useInt", "useIntAgain", "useLong", "recursive"],
        9,
    );
}
#[test]
fn ordinary_overload_roots_keep_distinct_verified_targets() {
    check(OVERLOADS, &["one", "two"], 4);
}
