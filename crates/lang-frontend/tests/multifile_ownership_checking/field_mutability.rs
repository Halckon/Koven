//! Field permissions belong to the declaration source, not the caller's symbol table.

use super::*;

fn check_fields(field_kind: &str, caller: &str, expected_codes: &[&str], expected_plans: usize) {
    for reverse in [false, true] {
        let mut sources = SourceMap::new();
        // The unrelated field has the same local symbol ID and opposite mutability.
        let decoy_kind = if field_kind == "var" { "val" } else { "var" };
        let (decoy_id, decoy) = parsed(
            &mut sources,
            "a-decoy.ko",
            &format!("package other\nclass Holder({decoy_kind} text: String)"),
        );
        let (model_id, model) = parsed(
            &mut sources,
            "model.ko",
            &format!("package app\nclass Holder({field_kind} text: String)"),
        );
        let (caller_id, caller_file) = parsed(&mut sources, "use.ko", caller);
        let mut inputs = vec![
            SourceUnitInput::new("root", "other/a-decoy.ko", decoy_id, &decoy),
            SourceUnitInput::new("root", "app/model.ko", model_id, &model),
            SourceUnitInput::new("root", "app/use.ko", caller_id, &caller_file),
        ];
        if reverse {
            inputs.reverse();
        }
        let (environment, types) = standard_environments();
        let names = validated_names(&sources, &inputs, &environment);
        let typed = validated_types(&sources, &inputs, &names, &types);
        let model_source = source_unit(&names, model_id);
        let decoy_source = source_unit(&names, decoy_id);
        let field = |source: SourceUnitId| {
            typed
                .types()
                .signatures()
                .declarations()
                .iter()
                .filter_map(|declaration| declaration.nominal())
                .flat_map(|nominal| nominal.fields())
                .find(|field| field.name() == "text" && field.symbol().source_unit() == source)
                .expect("field declaration")
                .symbol()
        };
        assert_eq!(field(model_source).symbol(), field(decoy_source).symbol());
        assert_ne!(field(model_source), field(decoy_source));
        let ownership = check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
            .expect("valid ownership inputs");
        assert_eq!(
            diagnostic_codes(&ownership),
            expected_codes,
            "reverse={reverse}: {:?}",
            ownership.diagnostics()
        );
        assert_eq!(ownership.field_replacements().len(), expected_plans);
        for plan in ownership.field_replacements() {
            assert_eq!(plan.place().fields(), &[field(model_source)]);
            assert_eq!(
                plan.descriptor().expression().source_unit(),
                source_unit(&names, caller_id)
            );
        }
        if expected_codes.is_empty() {
            assert!(
                ownership.deferred().is_empty(),
                "{:?}",
                ownership.deferred()
            );
            assert!(ownership.validate().is_ok());
        } else {
            for diagnostic in ownership.diagnostics() {
                assert_eq!(diagnostic.primary_span().source_id(), caller_id);
                assert_eq!(sources.slice(diagnostic.primary_span()).unwrap(), "&");
                assert!(caller[diagnostic.primary_span().start()..].starts_with("&holder.text"));
            }
            assert!(ownership.loans().is_empty());
            assert!(ownership.drops().is_empty());
            assert!(ownership.value_deliveries().is_empty());
            assert!(ownership.field_replacements().is_empty());
        }
    }
}

#[test]
fn cross_file_var_field_replace_uses_declaration_identity() {
    for binding in ["val", "var"] {
        check_fields(
            "var",
            &format!(
                "package app\nfun run(): Unit {{\n{binding} holder = Holder(\"old\")\nval old = replace(&holder.text, \"new\")\nprintln(old)\n}}"
            ),
            &[],
            1,
        );
    }
}

#[test]
fn cross_file_val_field_rejects_even_with_mutable_same_id_decoy() {
    check_fields(
        "val",
        "package app\nfun run(): Unit {\nval holder = Holder(\"old\")\nreplace(&holder.text, \"new\")\n}",
        &["L0134"],
        0,
    );
}

#[test]
fn cross_file_var_field_keeps_shared_receiver_rejection() {
    check_fields(
        "var",
        "package app\nfun run(holder: Holder): Unit {\nreplace(&holder.text, \"new\")\n}",
        &["L0134"],
        0,
    );
}

#[test]
fn cross_file_field_inout_call_uses_same_mutability_query() {
    check_fields(
        "var",
        "package app\nfun set(inout text: String): Unit { text = \"new\" }\nfun run(): Unit {\nval holder = Holder(\"old\")\nset(&holder.text)\n}",
        &[],
        0,
    );
}

#[test]
fn cross_file_field_error_clears_an_earlier_replace_plan() {
    check_fields(
        "var",
        "package app\nfun run(shared: Holder): Unit {\nval holder = Holder(\"old\")\nreplace(&holder.text, \"new\")\n}\nfun rejected(holder: Holder): Unit {\nreplace(&holder.text, \"bad\")\n}",
        &["L0134"],
        0,
    );
}
