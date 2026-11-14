//! Ordinary sources only: experimental common-plan roots, not whole Program lowering.
use super::*;
use lang_frontend::{
    lexer::lex,
    name_resolution::{index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::check_compilation_unit_ownership,
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
    let run = |roots: &[DeclarationId]| {
        plan_unit_root_set_from_facts(
            &[&parsed],
            &names,
            typed.types(),
            owned.ownership(),
            roots,
            MAX_UNIT_GENERIC_INSTANCES,
            true,
        )
        .unwrap()
    };
    let plan = run(&entries);
    assert_eq!(plan.instances.len(), expected);
    assert_eq!(
        plan.instances
            .iter()
            .map(|i| i.key.clone())
            .collect::<BTreeSet<_>>()
            .len(),
        expected
    );
    let mut reachable_union = BTreeSet::new();
    for entry in &entries {
        assert_eq!(
            plan.instances
                .iter()
                .filter(|i| i.key == UnitFunctionInstanceKey::for_entry(*entry))
                .count(),
            1
        );
        let single = plan_unit_instances_from_facts(
            &[&parsed],
            &names,
            typed.types(),
            owned.ownership(),
            *entry,
            MAX_UNIT_GENERIC_INSTANCES,
        )
        .unwrap();
        assert_eq!(single, run(&[*entry]));
        for instance in single.instances {
            reachable_union.insert(instance.key.clone());
            assert!(plan.instances.iter().any(|i| i.key == instance.key));
        }
    }
    assert_eq!(
        reachable_union,
        plan.instances.iter().map(|i| i.key.clone()).collect()
    );
    let mut reversed = entries.clone();
    reversed.reverse();
    assert_eq!(
        plan,
        run(&reversed),
        "root order must not change common plan identity"
    );
}

#[test]
fn ordinary_generic_roots_share_one_instance_plan() {
    check(
        GENERICS,
        &["useInt", "useIntAgain", "useLong", "recursive"],
        9,
    );
}

#[test]
fn ordinary_overload_roots_keep_distinct_plan_targets() {
    check(OVERLOADS, &["one", "two"], 4);
}
