//! Selected Unit roots and current budget behavior on exact former Single sources.
use super::*;

const ROOTS: &str = "fun <T> identity(own value: T): T = value\n\
             fun <T> recurse(own input: T): T = recurse(input)\n\
             fun <T> unused(own item: T): T = item\n\
             fun uncalled(): Unit {}\n\
             fun first(input: Int): Int = identity(input)\n\
             fun second(input: Int): Int = identity<Int>(input)\n\
             fun third(input: Long): Long = identity(input)\n\
             fun recursive(input: Int): Int = recurse(input)";

#[test]
fn original_generic_dedup_uses_only_explicitly_selected_roots() {
    use crate::ssa::unit_plan::{UnitFunctionInstanceKey, plan_unit_root_set_from_facts};
    use lang_frontend::{
        ownership_checking::owned_compilation_unit_view, type_checking::MAX_UNIT_GENERIC_INSTANCES,
    };

    const PATH: &str = "original-generic-roots.ko";
    for rooted in [false, true] {
        let mut sources = SourceMap::new();
        let (source, file) = parsed(&mut sources, PATH, ROOTS);
        let inputs = [if rooted {
            SourceUnitInput::new("root", PATH, source, &file)
        } else {
            SourceUnitInput::memory_document(source, &file)
        }];
        let (name_environment, environment) = standard_environments();
        let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &environment);
        let view =
            owned_compilation_unit_view(&sources, &inputs, &names, &environment, &typed, &owned)
                .expect("actual admitted Unit facts");
        let entry_names = ["first", "second", "third", "recursive"];
        let entries = entry_names.map(|name| declaration(&names, name));
        let run = |roots: &[DeclarationId]| {
            plan_unit_root_set_from_facts(
                &[&file],
                view.names(),
                view.types(),
                view.ownership(),
                roots,
                MAX_UNIT_GENERIC_INSTANCES,
                true,
            )
            .expect("four explicitly selected ordinary roots")
        };
        let plan = run(&entries);
        let mut reversed = entries;
        reversed.reverse();
        assert_eq!(
            plan,
            run(&reversed),
            "the same explicit root set is deterministic"
        );
        assert_eq!(plan.len(), 7);
        for id in entries {
            assert_eq!(
                plan.iter()
                    .filter(|instance| instance.key() == &UnitFunctionInstanceKey::for_entry(id))
                    .count(),
                1
            );
        }
        let target = |name| UnitCallableTarget::Declaration(declaration(&names, name));
        for name in ["uncalled", "unused"] {
            assert!(
                plan.iter()
                    .all(|instance| instance.key().target() != target(name))
            );
        }
        let identity_keys = plan
            .iter()
            .filter(|instance| instance.key().target() == target("identity"))
            .map(|instance| instance.key())
            .collect::<Vec<_>>();
        let int = typed.types().types().builtin(BuiltinType::Int).unwrap();
        let long = typed.types().types().builtin(BuiltinType::Long).unwrap();
        assert_eq!(identity_keys.len(), 2);
        for ty in [int, long] {
            assert_eq!(
                identity_keys
                    .iter()
                    .filter(|key| key.type_arguments() == [ty])
                    .count(),
                1
            );
        }
        let recursive = plan
            .iter()
            .filter(|instance| instance.key().target() == target("recurse"))
            .collect::<Vec<_>>();
        let [recursive] = recursive.as_slice() else {
            panic!("exactly one concrete recursive source");
        };
        assert_eq!(recursive.key().type_arguments(), [int]);
        let routes = typed
            .types()
            .calls()
            .iter()
            .filter_map(|call| {
                plan.callable_plan()
                    .call_site(recursive.source_token(), call.expression())
            })
            .collect::<Vec<_>>();
        let [recursive_call] = routes.as_slice() else {
            panic!("one real recursive call inside the concrete source");
        };
        assert_eq!(recursive_call.key(), recursive.key());
    }
}
