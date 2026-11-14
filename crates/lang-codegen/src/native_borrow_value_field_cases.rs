//! SPEC-0309: ordinary source inputs shared by SSA and unmodified native tests.
pub(crate) const COPYABLE: &str = include_str!("../tests/fixtures/borrow_value_fields/copyable.ko");
pub(crate) const STRING: &str = include_str!("../tests/fixtures/borrow_value_fields/string.ko");
pub(crate) const CLONE: &str = include_str!("../tests/fixtures/borrow_value_fields/clone.ko");

pub(crate) fn cases() -> [(&'static str, &'static str, &'static [u8]); 5] {
    [
        (
            "copyable-projection",
            include_str!("../tests/fixtures/borrow_value_fields/copyable_projection.ko"),
            b"seven\nseven\nseven\nconsumed\n",
        ),
        (
            "non-source",
            include_str!("../tests/fixtures/borrow_value_fields/non_source.ko"),
            b"kept\nkept\nkept\nconsumed\n",
        ),
        (
            "copyable",
            COPYABLE,
            b"seven\nseven\nseven\nseven\nseven\nseven\nconsumed\n",
        ),
        (
            "string",
            STRING,
            b"payload\npayload\npayload\npayload\npayload\npayload\nconsumed\n",
        ),
        ("clone", CLONE, b"payload\npayload\npayload\nconsumed\n"),
    ]
}

pub(crate) fn unit_controls() -> [(&'static str, &'static str, &'static [u8]); 2] {
    [
        (
            "copyable-capture",
            include_str!("../tests/fixtures/borrow_value_fields/copyable_capture.ko"),
            b"seven\nseven\n",
        ),
        (
            "class-capture",
            include_str!("../tests/fixtures/borrow_value_fields/class_capture.ko"),
            b"class-field\nseven\nclass-field\nseven\n",
        ),
    ]
}

pub(crate) mod unit_container_origin_bridge_tests {
    //! Real container Unit origins with synthetic backend failures; no source overflow claim.
    use lang_frontend::{diagnostic::DiagnosticDetail, name_resolution::SymbolKind};

    use crate::{
        NativeObjectErrorKind,
        llvm::{
            LlvmAdapterError,
            layout::{LayoutFailure, LayoutQuantity, TargetLayoutError},
        },
        native::map_backend_error,
        native_tests::boxed_enum_tests::unit_source::UnitSource,
        ssa::model::{
            EntityId, EntityType, LoanKind, MapContainerKind, SequentialContainerKind, SsaTypeId,
            SsaTypeKind, TypeOrigin,
        },
    };

    #[derive(Clone, Copy)]
    enum Container {
        Sequential(SequentialContainerKind),
        Map(MapContainerKind),
    }

    impl Container {
        fn matches(self, ty: &SsaTypeKind) -> bool {
            match (self, ty) {
                (Self::Sequential(expected), SsaTypeKind::SequentialContainer { kind, .. }) => {
                    expected == *kind
                }
                (Self::Map(expected), SsaTypeKind::MapContainer { kind, .. }) => expected == *kind,
                _ => false,
            }
        }
    }

    fn assert_bridge(text: &str, expected: Container, nominal: Option<&str>) {
        let unit = UnitSource::analyze("container-origin.ko", text)
            .expect("original factory source passes strict Unit admission");
        unit.assert_frontend_clean();
        let (program, entry) = unit
            .lower_entry(unit.declaration("entry", SymbolKind::Function))
            .expect("real Unit container lowers to verified SSA");
        let module = program.module(entry.module()).unwrap();
        let containers = module
            .types
            .iter()
            .enumerate()
            .filter_map(|(index, kind)| {
                expected.matches(kind).then_some(SsaTypeId {
                    module: entry.module(),
                    index,
                })
            })
            .collect::<Vec<_>>();
        assert_eq!(containers.len(), 1, "one exact container storage identity");
        let ty = containers[0];
        let origin = program
            .type_origin(ty)
            .expect("lowering must supply the container's own origin");
        let sources = unit.sources();
        assert_eq!(origin.primary, origin.declaration);
        assert_eq!(origin.primary.source_id(), unit.parsed().source_id());
        assert_eq!(sources.source_text(origin.primary.source_id()), Ok(text));
        assert!(!sources.slice(origin.primary).unwrap().is_empty());
        assert!(
            unit.parsed()
                .ast()
                .expressions()
                .iter()
                .any(|(_, node)| node.span() == origin.primary)
                || unit
                    .parsed()
                    .ast()
                    .items()
                    .iter()
                    .any(|(_, node)| node.span() == origin.primary)
                || unit
                    .parsed()
                    .ast()
                    .type_refs()
                    .iter()
                    .any(|(_, node)| node.span() == origin.primary),
            "container origin must retain a real source demand span"
        );
        if let Some(nominal) = nominal {
            let declaration = unit
                .names()
                .names()
                .index()
                .declarations()
                .iter()
                .find(|item| item.name() == nominal)
                .unwrap()
                .name_span();
            assert_eq!(sources.slice(declaration).unwrap(), nominal);
            assert_ne!(
                origin.declaration, declaration,
                "do not borrow the element's nominal label"
            );
            let SsaTypeKind::SequentialContainer { element, .. } = &module.types[ty.index()] else {
                panic!("nominal-element fixture uses List");
            };
            assert_eq!(
                program.type_origin(*element).unwrap().declaration,
                declaration
            );
        }
        let error = map_backend_error(
            sources,
            &program,
            LlvmAdapterError::InvalidLayout(TargetLayoutError {
                ty,
                quantity: LayoutQuantity::Stride,
                failure: LayoutFailure::ExceedsTarget {
                    value: u128::MAX,
                    maximum: u64::MAX.into(),
                },
            }),
        );
        assert_eq!(error.kind(), NativeObjectErrorKind::TargetLayout);
        assert_eq!(error.span(), Some(origin.primary));
        let diagnostic = error.diagnostic().expect("container origin enables L0145");
        assert_eq!(diagnostic.code().to_string(), "L0145");
        assert_eq!(diagnostic.primary_span(), origin.primary);
        assert!(
            diagnostic
                .details()
                .iter()
                .all(|detail| !matches!(detail, DiagnosticDetail::Label(_))),
            "an intrinsic container has no separate source declaration label"
        );
    }

    #[test]
    fn real_unit_container_origins_bridge_stride_failures_without_nominal_labels() {
        for (factory, kind) in [
            (
                "arrayOf(1)",
                Container::Sequential(SequentialContainerKind::Array),
            ),
            (
                "listOf(1)",
                Container::Sequential(SequentialContainerKind::List),
            ),
            (
                "mutableListOf(1)",
                Container::Sequential(SequentialContainerKind::MutableList),
            ),
            ("mapOf<Int, Int>()", Container::Map(MapContainerKind::Map)),
            (
                "mutableMapOf<Int, Int>()",
                Container::Map(MapContainerKind::MutableMap),
            ),
        ] {
            let text = format!("fun entry(): Unit {{ val items = {factory} }}");
            assert_bridge(&text, kind, None);
        }
    }

    #[test]
    fn container_origin_does_not_steal_its_nominal_element_declaration() {
        assert_bridge(
            "class Resource {}\nfun entry(): Unit { val items = listOf(Resource()) }",
            Container::Sequential(SequentialContainerKind::List),
            Some("Resource"),
        );
    }

    #[test]
    fn view_only_unit_parameter_supplies_its_synthetic_list_layout_origin() {
        for (definition, element_name) in [("", "Int"), ("class Resource {}", "Resource")] {
            // There is no List annotation or factory to fill in a missing origin later.
            let text = format!("{definition}\nfun entry(view: View<{element_name}>): Unit {{}}");
            let unit = UnitSource::analyze("view-only-origin.ko", &text)
                .expect("strict original View parameter admission");
            unit.assert_frontend_clean();
            let declaration = unit.declaration("entry", SymbolKind::Function);
            let signature = unit
                .types()
                .signatures()
                .declaration(declaration)
                .unwrap()
                .callable()
                .unwrap();
            let [parameter] = signature.parameters() else {
                panic!("one original View parameter");
            };
            let expected = TypeOrigin {
                primary: parameter.span(),
                declaration: parameter.span(),
            };
            let sources = unit.sources();
            assert_eq!(expected.primary.source_id(), unit.parsed().source_id());
            assert_eq!(
                sources.source_text(expected.primary.source_id()),
                Ok(text.as_str())
            );
            assert!(!sources.slice(expected.primary).unwrap().is_empty());
            let (program, entry) = unit
                .lower_entry(declaration)
                .expect("real View-only Unit signature lowers to verified SSA");
            let module = program.module(entry.module()).unwrap();
            let views = module
                .types
                .iter()
                .enumerate()
                .filter_map(|(index, kind)| match kind {
                    SsaTypeKind::RangeView { source } => Some((
                        SsaTypeId {
                            module: entry.module(),
                            index,
                        },
                        *source,
                    )),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(views.len(), 1, "one real parameter storage identity");
            let (view, source) = views[0];
            let function = module.function(entry).unwrap();
            let [loan @ EntityId::Loan(_)] = function.blocks[0].parameters.as_slice() else {
                panic!("View remains a Borrow parameter, not a newly owned value");
            };
            assert_eq!(
                function.entity(*loan).unwrap().ty,
                EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: view,
                }
            );
            let Some(SsaTypeKind::SequentialContainer {
                kind: SequentialContainerKind::List,
                element,
            }) = module.type_kind(source)
            else {
                panic!("View source is its existing synthetic List storage");
            };
            assert_eq!(program.type_origin(source), Some(expected));
            assert_eq!(
                program.type_origin(view),
                None,
                "no RangeView-origin expansion"
            );
            if element_name == "Resource" {
                let nominal = unit
                    .names()
                    .names()
                    .index()
                    .declarations()
                    .iter()
                    .find(|item| item.name() == "Resource")
                    .unwrap()
                    .name_span();
                assert_eq!(program.type_origin(*element).unwrap().declaration, nominal);
                assert_ne!(nominal, expected.declaration);
            }
            let failure = |ty| {
                LlvmAdapterError::InvalidLayout(TargetLayoutError {
                    ty,
                    quantity: LayoutQuantity::Stride,
                    failure: LayoutFailure::ExceedsTarget {
                        value: u128::MAX,
                        maximum: u64::MAX.into(),
                    },
                })
            };
            let error = map_backend_error(sources, &program, failure(source));
            assert_eq!(error.kind(), NativeObjectErrorKind::TargetLayout);
            assert_eq!(error.span(), Some(expected.primary));
            let diagnostic = error
                .diagnostic()
                .expect("synthetic List has an L0145 origin");
            assert_eq!(diagnostic.code().to_string(), "L0145");
            assert_eq!(diagnostic.primary_span(), expected.primary);
            assert!(
                diagnostic
                    .details()
                    .iter()
                    .all(|detail| !matches!(detail, DiagnosticDetail::Label(_))),
                "do not label the View or its element as a separate List declaration"
            );
            let unchanged = map_backend_error(sources, &program, failure(view));
            assert_eq!(unchanged.kind(), NativeObjectErrorKind::Backend);
            assert_eq!(unchanged.span(), None);
            assert!(unchanged.diagnostic().is_none());
        }
    }
}

pub(crate) mod native_multi_borrow_cases {
    //! Normal source shared by SSA and unmodified native tests; no IR/allocator rewriting.
    pub(crate) struct Case {
        pub name: &'static str,
        pub source: &'static str,
        pub stdout: &'static [u8],
        pub source_index: usize,
        pub reused_arguments: bool,
    }
    pub(crate) fn cases() -> Vec<Case> {
        vec![
            Case {
                name: "second_parameter",
                source: include_str!("../tests/fixtures/multi_borrow/second_parameter.ko"),
                stdout: b"kept\nkept\n",
                reused_arguments: false,
                source_index: 1,
            },
            Case {
                name: "named_reverse_middle",
                source: include_str!("../tests/fixtures/multi_borrow/named_reverse_middle.ko"),
                stdout: b"eval-right\neval-left\nkept\nkept\n",
                reused_arguments: false,
                source_index: 1,
            },
            Case {
                name: "same_type_different_roots",
                source: include_str!("../tests/fixtures/multi_borrow/same_type_different_roots.ko"),
                stdout: b"other\nkept\nkept\n",
                reused_arguments: false,
                source_index: 1,
            },
            Case {
                name: "same_owner_twice",
                source: include_str!("../tests/fixtures/multi_borrow/same_owner_twice.ko"),
                stdout: b"kept\nkept\n",
                reused_arguments: false,
                source_index: 1,
            },
            Case {
                name: "wrapper_changes_index",
                source: include_str!("../tests/fixtures/multi_borrow/wrapper_changes_index.ko"),
                stdout: b"other\nkept\nkept\n",
                reused_arguments: false,
                source_index: 1,
            },
            Case {
                name: "projected_field",
                source: include_str!("../tests/fixtures/multi_borrow/projected_field.ko"),
                stdout: b"other\nfield\nresource-owned\ndrop-resource\n",
                reused_arguments: false,
                source_index: 1,
            },
            Case {
                name: "first_parameter",
                source: include_str!("../tests/fixtures/multi_borrow/first_parameter.ko"),
                stdout: b"other\nkept\nkept\n",
                reused_arguments: false,
                source_index: 0,
            },
            Case {
                name: "last_parameter_three",
                source: include_str!("../tests/fixtures/multi_borrow/last_parameter_three.ko"),
                stdout: b"other\nkept\nkept\n",
                reused_arguments: false,
                source_index: 2,
            },
            Case {
                name: "copyable_int",
                source: include_str!("../tests/fixtures/multi_borrow/copyable_int.ko"),
                stdout: b"seven\nseven\n",
                reused_arguments: false,
                source_index: 1,
            },
            Case {
                name: "generic_move_only",
                source: include_str!("../tests/fixtures/multi_borrow/generic_move_only.ko"),
                stdout: b"packet\nowned\n",
                reused_arguments: false,
                source_index: 1,
            },
            Case {
                name: "same_entry_loan",
                source: include_str!("../tests/fixtures/multi_borrow/same_entry_loan.ko"),
                stdout: b"kept\nkept\n",
                reused_arguments: false,
                source_index: 1,
            },
            Case {
                name: "unrelated_resource_temporary",
                source: include_str!(
                    "../tests/fixtures/multi_borrow/unrelated_resource_temporary.ko"
                ),
                stdout: b"other-drop\nkept\n",
                reused_arguments: false,
                source_index: 1,
            },
            Case {
                name: "wrapper_field_arguments",
                source: include_str!("../tests/fixtures/multi_borrow/wrapper_field_arguments.ko"),
                stdout: b"owned\ndrop\nfield\nowned\ndrop\n",
                reused_arguments: false,
                source_index: 1,
            },
            Case {
                name: "caller_aliases",
                source: include_str!("../tests/fixtures/multi_borrow/caller_aliases.ko"),
                stdout: b"kept\nother\nkept\n",
                reused_arguments: true,
                source_index: 1,
            },
            Case {
                name: "caller_result_twice",
                source: include_str!("../tests/fixtures/multi_borrow/caller_result_twice.ko"),
                stdout: b"kept\nkept\nkept\n",
                reused_arguments: true,
                source_index: 1,
            },
            Case {
                name: "generic_string_payload",
                source: include_str!("../tests/fixtures/multi_borrow/generic_string_payload.ko"),
                stdout: b"payload\npayload\n",
                reused_arguments: false,
                source_index: 1,
            },
        ]
    }
}

pub(crate) mod bitwise_test_support {
    //! Shared source fixture: dynamic operands and independent decimal boundary oracles.

    pub(crate) fn binary_fixture(qualifier: &str) -> (String, String, String) {
        let mut functions = String::new();
        let mut checks = String::new();
        let mut stdout = String::new();
        let mut case_index = 0;
        for (ty, width, signed) in [
            ("Byte", 8, true),
            ("Short", 16, true),
            ("Int", 32, true),
            ("Long", 64, true),
            ("UByte", 8, false),
            ("UShort", 16, false),
            ("UInt", 32, false),
            ("ULong", 64, false),
        ] {
            let high = 1_i128 << (width - 1);
            let max = if signed { high - 1 } else { high * 2 - 1 };
            let top = if signed { -high } else { high };
            let ones = if signed { -1 } else { max };
            for op in ["and", "or", "xor", "shl", "shr", "ushr"] {
                functions.push_str(&format!(
                    "fun {op}{ty}(left: {ty}, right: {ty}): {ty} = left {op} right\n"
                ));
            }
            let mut cases = vec![
                ("and", top, high - 1, 0),
                ("and", top, ones, top),
                ("and", max, 0, 0),
                ("or", top, high - 1, ones),
                ("or", 0, top, top),
                ("or", max, 0, max),
                ("xor", top, ones, high - 1),
                ("xor", max, max, 0),
                ("xor", 0, ones, ones),
                ("shl", top, 1, 0),
                ("shl", max, 1, if signed { -2 } else { max - 1 }),
            ];
            // Expected effective counts are stated independently of the implementation's bit mask.
            let mut counts = vec![
                (0, 0),
                (1, 1),
                (width - 1, width - 1),
                (width, 0),
                (width + 1, 1),
                (width * 2, 0),
                (max, width - 1),
            ];
            if signed {
                counts.extend([
                    (-1, width - 1),
                    (-width, 0),
                    (-width - 1, width - 1),
                    (-high, 0),
                ]);
            }
            for (count, effective) in counts {
                let left_expected = if effective == width - 1 {
                    top
                } else {
                    1_i128 << effective
                };
                cases.push(("shl", 1, count, left_expected));
                cases.push((
                    "shr",
                    top,
                    count,
                    if signed {
                        -high >> effective
                    } else {
                        high >> effective
                    },
                ));
                cases.push((
                    "ushr",
                    top,
                    count,
                    if effective == 0 {
                        top
                    } else {
                        high >> effective
                    },
                ));
            }
            for (op, left, right, expected) in cases {
                checks.push_str(&format!(
                "val expected{case_index}: {ty} = {}\nif ({qualifier}{op}{ty}({}, {}) == expected{case_index}) {{ println(\"case-{case_index}\") }}\n",
                literal(expected, ty), literal(left, ty), literal(right, ty)
            ));
                stdout.push_str(&format!("case-{case_index}\n"));
                case_index += 1;
            }
        }
        for (index, op) in ["and", "or", "xor", "shl", "shr", "ushr"]
            .iter()
            .enumerate()
        {
            let left = if *op == "or" { -1 } else { 0 };
            functions.push_str(&format!(
            "fun left{index}(): Int {{ println(\"left-{index}\"); return {left} }}\nfun right{index}(): Int {{ println(\"right-{index}\"); return 1 }}\nfun eager{index}(): Int = left{index}() {op} right{index}()\n"
        ));
            checks.push_str(&format!(
                "val eager{index}: Int = {qualifier}eager{index}()\n"
            ));
            stdout.push_str(&format!("left-{index}\nright-{index}\n"));
        }
        (functions, checks, stdout)
    }

    fn literal(value: i128, ty: &str) -> String {
        let suffix = match ty {
            "Long" => "L",
            "ULong" => "uL",
            ty if ty.starts_with('U') => "u",
            _ => "",
        };
        format!("{value}{suffix}")
    }

    pub(crate) fn inv_fixture(qualifier: &str) -> (String, String, String) {
        let mut functions = String::new();
        let mut checks = String::new();
        let mut stdout = String::new();
        let mut case_index = 0;
        for (ty, width, signed) in [
            ("Byte", 8, true),
            ("Short", 16, true),
            ("Int", 32, true),
            ("Long", 64, true),
            ("UByte", 8, false),
            ("UShort", 16, false),
            ("UInt", 32, false),
            ("ULong", 64, false),
        ] {
            let high = 1_i128 << (width - 1);
            let max = if signed { high - 1 } else { high * 2 - 1 };
            let top = if signed { -high } else { high };
            let ones = if signed { -1 } else { max };
            functions.push_str(&format!(
                "fun invert{ty}(value: {ty}): {ty} = value.inv()\n"
            ));
            let cases = [
                (0, ones),
                (ones, 0),
                (top, high - 1),
                (high - 1, top),
                (1, if signed { -2 } else { max - 1 }),
            ];
            for (input, expected) in cases {
                checks.push_str(&format!("val expected{case_index}: {ty} = {}\nif ({qualifier}invert{ty}({}) == expected{case_index}) {{ println(\"inv-{case_index}\") }}\n", literal(expected, ty), literal(input, ty)));
                stdout.push_str(&format!("inv-{case_index}\n"));
                case_index += 1;
            }
        }
        functions.push_str("fun operand(): Int { println(\"operand\"); return 42 }\nfun eager(): Int = operand().inv()\n");
        checks.push_str(&format!(
            "if ({qualifier}eager() == -43) {{ println(\"inv-eager\") }}\n"
        ));
        stdout.push_str("operand\ninv-eager\n");
        (functions, checks, stdout)
    }

    pub(crate) fn inv_receiver_fixture() -> &'static str {
        r#"
class Number(val number: Int)
fun lastField(): Int { val value = Number(1); return value.number.inv() }
fun exercise(): Unit {
    if (1.inv().inv() == 1) { println("literal-chain") }
    if (lastField() == -2) { println("last-field") }
}
"#
    }

    pub(crate) const INV_RECEIVER_STDOUT: &str = "literal-chain\nlast-field\n";

    pub(crate) fn guide_litmus_12() -> &'static str {
        let guide = include_str!("../../../docs/guide/15-conformance-and-staging.md");
        guide
            .split_once("### Litmus 12:")
            .expect("Guide Litmus 12")
            .1
            .split_once("```kotlin\n")
            .expect("Koven source fence")
            .1
            .split_once("\n```")
            .expect("closed source fence")
            .0
    }

    pub(crate) fn control_fixture() -> (&'static str, &'static str, &'static str) {
        (
            r#"
fun left(): Int { println("left"); return 3 }
fun inverted(flag: Boolean): Int = (if (flag) { return 7 } else { left() }).inv()
fun binary(flag: Boolean): Int = left() and (if (flag) { return 9 } else { left() })
fun exerciseControl(): Unit {
    if (inverted(true) == 7) { println("inv-return") }
    if (inverted(false) == -4) { println("inv-normal") }
    if (binary(true) == 9) { println("binary-return") }
    if (binary(false) == 3) { println("binary-normal") }
}
"#,
            "exerciseControl()",
            "inv-return\nleft\ninv-normal\nleft\nbinary-return\nleft\nleft\nbinary-normal\n",
        )
    }

    pub(crate) fn constant_runtime_fixture(qualifier: &str) -> (String, String, String) {
        let mut functions = String::new();
        let mut checks = String::new();
        let mut stdout = String::new();
        let mut index = 0;
        for (ty, width, signed) in [
            ("Byte", 8, true),
            ("Short", 16, true),
            ("Int", 32, true),
            ("Long", 64, true),
            ("UByte", 8, false),
            ("UShort", 16, false),
            ("UInt", 32, false),
            ("ULong", 64, false),
        ] {
            let high = 1_i128 << (width - 1);
            let top = if signed { -high } else { high };
            let ones = if signed { -1 } else { high * 2 - 1 };
            for (op, left, right, expected) in [
                ("and", top, high - 1, 0),
                ("or", top, high - 1, ones),
                ("xor", top, ones, high - 1),
                ("shl", 1, if signed { -1 } else { width - 1 }, top),
                (
                    "shr",
                    top,
                    width + 1,
                    if signed { -high / 2 } else { high / 2 },
                ),
                ("ushr", top, if signed { -1 } else { width - 1 }, 1),
            ] {
                functions.push_str(&format!("fun dynamic{index}(left: {ty}, right: {ty}): {ty} = left {op} right\nconst val LEFT{index}: {ty} = {}\nconst val RIGHT{index}: {ty} = {}\nconst val CONST{index}: {ty} = LEFT{index} {op} RIGHT{index}\n", literal(left, ty), literal(right, ty)));
                checks.push_str(&format!("val oracle{index}: {ty} = {}\nval actual{index}: {ty} = {qualifier}dynamic{index}({}, {})\nif (actual{index} == oracle{index} && actual{index} == {qualifier}CONST{index}) {{ println(\"const-runtime-{index}\") }}\n", literal(expected, ty), literal(left, ty), literal(right, ty)));
                stdout.push_str(&format!("const-runtime-{index}\n"));
                index += 1;
            }
        }
        (functions, checks, stdout)
    }
}
