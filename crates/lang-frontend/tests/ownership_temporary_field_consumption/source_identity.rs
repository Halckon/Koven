//! Generic field permissions and labels retain their provider identity across input order.
use super::*;
use lang_frontend::type_checking::Copyability;

#[test]
fn generic_field_consumption_keeps_provider_identity() {
    let providers = [
        "package models\nclass Left<T>(val value: T)",
        "package models\n\nclass Right<T>(val value: T)",
    ];
    for selected in 0..2 {
        for (body, reject, copyability) in [
            (
                "fun use(): String = Holder<String>(\"hi\").value",
                true,
                Copyability::MoveOnly,
            ),
            (
                "fun use(): Int = Holder<Int>(7).value",
                false,
                Copyability::Copyable,
            ),
            (
                "fun view(text: String): Unit {}\nfun use(): Unit { view(Holder<String>(\"hi\").value) }",
                false,
                Copyability::MoveOnly,
            ),
        ] {
            let selected_type = if selected == 0 { "Left" } else { "Right" };
            let consumer = format!(
                "package consumer\nimport models.{selected_type}\n{}",
                body.replace("Holder", selected_type)
            );
            for split_roots in [false, true] {
                for reversed in [false, true] {
                    for constants in [false, true] {
                        let mut sources = SourceMap::new();
                        let texts = [providers[0], providers[1], consumer.as_str()];
                        let ids = texts
                            .iter()
                            .enumerate()
                            .map(|(i, text)| {
                                sources.add_source(format!("file{i}.ko"), *text).unwrap()
                            })
                            .collect::<Vec<_>>();
                        let files = ids
                            .iter()
                            .map(|&source| {
                                parser_test_assertions::parse_file_twice(
                                    &sources,
                                    source,
                                    "generic field identity",
                                )
                            })
                            .collect::<Vec<_>>();
                        assert!(files.iter().all(|file| file.diagnostics().is_empty()));
                        let roots = if split_roots {
                            ["provider-p", "provider-q", "consumer"]
                        } else {
                            ["root"; 3]
                        };
                        let paths = if split_roots {
                            ["models/types.ko", "models/types.ko", "consumer/use.ko"]
                        } else {
                            ["models/left.ko", "models/right.ko", "consumer/use.ko"]
                        };
                        let mut inputs = (0..3)
                            .map(|i| SourceUnitInput::new(roots[i], paths[i], ids[i], &files[i]))
                            .collect::<Vec<_>>();
                        if reversed {
                            inputs.reverse();
                        }
                        let index = index_compilation_unit(&sources, &inputs).unwrap();
                        assert!(index.diagnostics().is_empty(), "{:?}", index.diagnostics());
                        let (environment, types) = standard_environments();
                        let names =
                            resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
                                .unwrap();
                        assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
                        let names = names.validate().unwrap();
                        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
                            .unwrap();
                        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
                        let [projection] = typed.aggregate_projections() else {
                            panic!("only consumer field access");
                        };
                        assert_eq!(typed.copyability(projection.ty()), copyability);
                        let field_source = names
                            .names()
                            .source_units()
                            .get(projection.field().source_unit().index())
                            .unwrap();
                        assert_eq!(field_source.resolution().source_id(), ids[selected]);
                        let field = field_source
                            .resolution()
                            .symbols()
                            .get(projection.field().symbol().index())
                            .unwrap();
                        let field_start = providers[selected].find("value:").unwrap();
                        let field_span = sources
                            .span(ids[selected], field_start, field_start + 5)
                            .unwrap();
                        assert_eq!(field.span(), field_span);
                        let expression_source = names
                            .names()
                            .source_units()
                            .get(projection.expression().source_unit().index())
                            .unwrap();
                        assert_eq!(expression_source.resolution().source_id(), ids[2]);
                        let owned = if constants {
                            let typed = typed.validate_constants().unwrap();
                            let recovery = check_compilation_unit_constant_ownership(
                                &sources, &inputs, &names, &types, &typed,
                            )
                            .unwrap();
                            assert!(recovery.is_compatible_with(&typed));
                            let owned = recovery.ownership().clone();
                            assert_eq!(recovery.materializations().is_none(), reject);
                            assert_eq!(recovery.validate().is_err(), reject);
                            owned
                        } else {
                            let typed = typed.validate().unwrap();
                            let owned = check_compilation_unit_ownership(
                                &sources, &inputs, &names, &types, &typed,
                            )
                            .unwrap();
                            assert!(owned.is_compatible_with(&typed));
                            assert_eq!(owned.clone().validate().is_err(), reject);
                            owned
                        };
                        if reject {
                            let start = consumer.rfind(".value").unwrap() + 1;
                            let catalog = DiagnosticCodeCatalog::try_new(&["L0132"]).unwrap();
                            let mut expected = Diagnostic::new(
                                &sources,
                                Severity::Error,
                                catalog.resolve("L0132").unwrap(),
                                "cannot move a non-Copyable component out of its owner",
                                sources.span(ids[2], start, start + 5).unwrap(),
                            )
                            .unwrap();
                            expected
                                .add_label(
                                    &sources,
                                    field_span,
                                    "non-Copyable component declared here",
                                )
                                .unwrap();
                            assert_eq!(owned.diagnostics(), [expected]);
                            assert!(owned.drops().is_empty());
                            assert!(owned.loans().is_empty());
                        } else {
                            assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
                            assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
                        }
                    }
                }
            }
        }
    }
}
