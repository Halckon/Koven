//! Ordinary scalar-prefix completion facts; no fault injection.
use lang_frontend::{
    lexer::lex,
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::check_compilation_unit_ownership,
    parser::parse_file,
    source::SourceMap,
    type_checking::{check_compilation_unit_types, standard_environments},
};
fn check(prefix: &str, accepted: bool) {
    for mode in ["", "own "] {
        let mut sources = SourceMap::new();
        let text = format!(
            "fun view(source:String):borrow String from source=source\nfun consume(item:String):Unit {{}}\nfun tick(own value:Int):Unit {{}}\nfun read(value:Int):Unit {{}}\nfun run({mode}choose:Boolean,source:String,{mode}flag:Boolean):Unit {{{prefix}while(flag){{if(choose){{borrow val a=view(source);consume(a)}}else{{borrow val b=view(source);consume(b)}};break}};consume(source)}}"
        );
        let source = sources.add_source("prefix.ko", &text).unwrap();
        let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
        assert!(parsed.diagnostics().is_empty());
        let (env, types) = standard_environments();
        for rooted in [false, true] {
            let inputs = [if rooted {
                SourceUnitInput::new("root", "prefix.ko", source, &parsed)
            } else {
                SourceUnitInput::memory_document(source, &parsed)
            }];
            let index = index_compilation_unit(&sources, &inputs).unwrap();
            let names = resolve_compilation_unit_names(&sources, &inputs, &index, &env)
                .unwrap()
                .validate()
                .unwrap();
            let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
                .unwrap()
                .validate()
                .unwrap();
            let run = || {
                check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
                    .unwrap()
                    .validate()
                    .unwrap()
            };
            let owned = run();
            assert_eq!(
                owned.ownership().no_carry_break_prefixes().len(),
                usize::from(accepted),
                "{prefix}"
            );
            let other = run();
            #[cfg(feature = "loan-observation-test-support")]
            {
                let off = lang_frontend::ownership_checking::with_loan_observation(false, run);
                assert_eq!(
                    off.ownership().no_carry_break_prefixes().len(),
                    usize::from(accepted)
                );
                assert!(lang_frontend::ownership_checking::legacy_ownership_eq(
                    owned.ownership(),
                    off.ownership()
                ));
            }
            if accepted {
                let p = &owned.ownership().no_carry_break_prefixes()[0];
                assert!(owned.ownership().owns_no_carry_break_prefix(p));
                assert!(!other.ownership().owns_no_carry_break_prefix(p));
                assert!(owned.ownership().owns_no_carry_break_branch(p.pair()));
                assert_ne!(p.calls()[0], p.calls()[1]);
                assert_ne!(p.prefix(), p.condition());
                assert_ne!(p.prefix(), p.pair().pair().branch());
            }
        }
    }
}
#[test]
fn scalar_prefix_closes_both_actual_paths_before_break_loop() {
    check("if(choose){tick(1)}else{tick(2)};", true);
}
#[test]
fn scalar_prefix_capability_remains_narrow() {
    for prefix in [
        "",
        "if(true){tick(1)}else{tick(2)};",
        "if(choose){read(1)}else{read(2)};",
        "if(choose){consume(source)}else{consume(source)};",
        "if(choose){tick(1);tick(2)}else{tick(2)};",
        "if(choose){if(flag){tick(1)}else{tick(2)}}else{tick(2)};",
        "if(choose){tick(1)}else{tick(2)};if(flag){tick(3)}else{tick(4)};",
    ] {
        check(prefix, false);
    }
}
