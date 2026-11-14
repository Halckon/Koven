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

fn with_unreachable_member() -> String {
    format!(
        "{GENERICS}\nvalue class Counter(val item: Int) {{\n    fun answer(own delta: Int): Int = item + delta\n}}"
    )
}

fn check(source: &str, root_names: &[&str], expected: usize, reject_roots: bool) {
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
    let plan_roots = |roots: &[DeclarationId]| {
        plan_unit_root_set_from_facts(
            &[&parsed],
            &names,
            typed.types(),
            owned.ownership(),
            roots,
            MAX_UNIT_GENERIC_INSTANCES,
            true,
        )
    };
    let run = |roots: &[DeclarationId]| plan_roots(roots).unwrap();
    let plan = run(&entries);
    let nominal = names
        .names()
        .index()
        .declarations()
        .iter()
        .find(|d| d.name() == "Counter");
    if let Some(declaration) = nominal {
        let signature = typed
            .types()
            .signatures()
            .declaration(declaration.id())
            .unwrap();
        let nominal = signature.nominal().unwrap();
        let [member] = nominal.members() else {
            panic!("one real receiver member")
        };
        assert!(matches!(member.target(), UnitCallableTarget::Symbol(_)));
        assert!(
            !plan
                .instances
                .iter()
                .any(|instance| instance.key.target() == member.target())
        );
    }
    if reject_roots {
        assert_eq!(std::env::var("GITHUB_ACTIONS").as_deref(), Ok("true"));
        let arena = typed.types().types().len();
        let generic = names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|d| d.name() == "identity")
            .unwrap();
        let generic_span = parsed.ast().items().get(generic.root()).unwrap().span();
        let mut mixed = entries.clone();
        mixed.push(generic.id());
        let mut reverse = mixed.clone();
        reverse.reverse();
        let mut rejected = vec![
            (vec![], LoweringErrorKind::InvalidModel, None),
            (
                vec![entries[0], entries[0]],
                LoweringErrorKind::InvalidModel,
                None,
            ),
            (
                mixed,
                LoweringErrorKind::UnsupportedNode,
                Some(generic_span),
            ),
            (
                reverse,
                LoweringErrorKind::UnsupportedNode,
                Some(generic_span),
            ),
        ];
        if let Some(nominal) = nominal {
            let mut mixed = entries.clone();
            mixed.push(nominal.id());
            let mut reverse = mixed.clone();
            reverse.reverse();
            for roots in [vec![nominal.id()], mixed, reverse] {
                rejected.push((
                    roots,
                    LoweringErrorKind::MissingFact,
                    Some(nominal.name_span()),
                ));
            }
        }
        for (roots, kind, span) in rejected {
            let error = plan_roots(&roots).unwrap_err();
            assert_eq!(error.kind, kind);
            assert_eq!(error.span, span);
            assert_eq!(typed.types().types().len(), arena);
            assert_eq!(run(&entries), plan, "same facts recover the complete plan");
        }
    }
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
        false,
    );
    check(
        &with_unreachable_member(),
        &["useInt", "useIntAgain", "useLong", "recursive"],
        9,
        false,
    );
}

#[test]
fn ordinary_overload_roots_keep_distinct_plan_targets() {
    check(OVERLOADS, &["one", "two"], 4, false);
}

// The reduced budget is an internal negative-control input, never run locally.
#[test]
#[ignore = "CI-only reduced budget; local compile/list only"]
fn ci_only_common_roots_share_budget_and_recover() {
    assert_eq!(
        std::env::var("GITHUB_ACTIONS").as_deref(),
        Ok("true"),
        "budget negative controls are CI-only"
    );
    let mut sources = SourceMap::new();
    let source_id = sources.add_source("test/roots.ko", GENERICS).unwrap();
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
    let entries = ["useInt", "useIntAgain", "useLong", "recursive"]
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

    let arena_len = typed.types().types().len();
    let plan = |roots: &[DeclarationId], limit| {
        plan_unit_root_set_from_facts(
            &[&parsed],
            &names,
            typed.types(),
            owned.ownership(),
            roots,
            limit,
            true,
        )
    };
    // Each root alone fits; only a shared budget can reject their combined frontier.
    for entry in &entries {
        assert!(plan(&[*entry], 2).is_ok());
    }
    let full = plan(&entries, 5).unwrap();
    assert_eq!(full.instances.len(), 9);
    let mut reversed = entries.clone();
    reversed.reverse();
    let failure = plan(&entries, 4).expect_err("five unique specializations exceed four");
    assert_eq!(failure.kind, LoweringErrorKind::InstanceLimitExceeded);
    let reversed_failure = plan(&reversed, 4).unwrap_err();
    assert_eq!(reversed_failure.kind, failure.kind);
    assert_eq!(reversed_failure.span, failure.span);
    assert!(failure.span.is_some());
    assert_eq!(
        plan(&entries, 5).unwrap(),
        full,
        "failed planning cannot poison a retry"
    );
    assert_eq!(plan(&reversed, 5).unwrap(), full);
    assert_eq!(typed.types().types().len(), arena_len);
}

// Invalid internal root sets are negative controls, not ordinary source programs.
#[test]
#[ignore = "CI-only root selection negative controls; local compile/list only"]
fn ci_only_root_selection_rejects_empty_duplicate_and_generic() {
    assert_eq!(std::env::var("GITHUB_ACTIONS").as_deref(), Ok("true"));
    check(
        GENERICS,
        &["useInt", "useIntAgain", "useLong", "recursive"],
        9,
        true,
    );
    check(
        &with_unreachable_member(),
        &["useInt", "useIntAgain", "useLong", "recursive"],
        9,
        true,
    );
}
