//! Legal source at the still-unsupported boundaries, never mutated facts or IR.
use super::super::support::{UnitExpectation, observe_original_unit};
use super::*;
use crate::ssa::LoweringErrorKind;
use crate::test_support::diagnostics::assert_eq_diagnostic;

fn scoped(declaration: &str, setup: &str, statement: &str) -> String {
    format!(
        r#"
{declaration}
fun read(view:View<String>):Unit{{}}
fun show(flag:Boolean):Unit{{}}
fun scan(flag:Boolean):Unit{{val root=listOf("a");{setup};{{borrow val part=take(root,1);{statement};read(part)}}}}
fun main():Unit{{scan(true)}}
"#
    )
}
fn reject(source: &str, fragment: &str) {
    for pipeline in [
        Pipeline::Single,
        Pipeline::Unit,
        Pipeline::UnitReversed,
        Pipeline::UnitConstant,
        Pipeline::UnitConstantReversed,
    ] {
        if matches!(pipeline, Pipeline::Single) {
            observe_original_unit(
                source,
                fragment,
                pipeline,
                UnitExpectation::Reject(fragment),
                "short-circuit-boundary",
            );
            continue;
        }
        let lowered = lower(source, fragment, pipeline);
        let error = lowered
            .result
            .err()
            .expect("this unselected combination must retain the gate");
        assert_eq_diagnostic!(
            error.kind,
            LoweringErrorKind::UnsupportedNode,
            "{pipeline:?}: {error:?}"
        );
        assert_eq_diagnostic!(
            error.span,
            Some(lowered.expected_span),
            "{pipeline:?}: {error:?}"
        );
    }
}

#[test]
fn range_short_circuit_boundary_nested_and_hidden_cfg() {
    for (expression, fragment) in [
        ("flag && (flag || flag)", "flag || flag"),
        ("(flag && flag) || flag", "flag && flag"),
        ("flag && values[0]", "flag && values[0]"),
        ("flag || nullable!!", "flag || nullable!!"),
        ("flag && (n+1>0)", "flag && (n+1>0)"),
        ("flag && (n>-1)", "flag && (n>-1)"),
        (
            "flag && (if(flag){true}else{false})",
            "if(flag){true}else{false}",
        ),
    ] {
        reject(
            &scoped(
                "",
                "val n=1;val values=listOf(true);val nullable:Boolean?=true",
                &format!("show({expression})"),
            ),
            fragment,
        );
    }
}

#[test]
fn range_short_circuit_boundary_owning_and_inout_calls() {
    reject(
        &scoped(
            "fun takeText(own text:String):Boolean{return true}",
            "val text=\"owned\"",
            "show(flag && takeText(text))",
        ),
        "flag && takeText(text)",
    );
    reject(
        &scoped(
            "fun change(inout n:Int):Boolean{n=2;return true}",
            "var n=1",
            "show(flag || change(&n))",
        ),
        "flag || change(&n)",
    );
}

#[test]
fn range_short_circuit_boundary_range_creation_and_aliases() {
    reject(
        &scoped(
            "fun test(view:View<String>):Boolean{return true}",
            "",
            "show(flag && test(take(part,1)))",
        ),
        "flag && test(take(part,1))",
    );
    for binding in [
        "borrow val alias=part;read(alias)",
        "val text=\"kept\";borrow val alias=text;println(alias)",
    ] {
        reject(
            &scoped("", "", &format!("{binding};show(flag && flag)")),
            "flag && flag",
        );
    }
}

#[test]
fn range_short_circuit_boundary_scope_and_roots() {
    for body in [
        "show(flag && flag);{borrow val part=take(root,1);read(part)}",
        "{borrow val part=take(root,1);read(part)};show(flag && flag)",
        "{borrow val part=take(root,1);show(flag && flag);borrow val child=take(part,1);read(child);read(part)}",
        "{borrow val part=take(root,1);borrow val second=take(other,1);show(flag && flag);read(part);read(second)}",
    ] {
        let source = format!(
            "fun show(flag:Boolean):Unit{{}}\nfun read(view:View<String>):Unit{{}}\nfun scan(flag:Boolean):Unit{{val root=listOf(\"a\");val other=listOf(\"b\");{body}}}\nfun main():Unit{{scan(true)}}"
        );
        reject(&source, "flag && flag");
    }
}

#[test]
fn range_short_circuit_boundary_loop_lambda_and_control_exit() {
    for (statement, fragment) in [
        ("for(item in part){show(flag && flag)}", "flag && flag"),
        ("val callback:()->Boolean={->flag && flag}", "flag && flag"),
        ("show(flag && (if(flag){return}else{true}))", "return"),
    ] {
        reject(&scoped("", "", statement), fragment);
    }
}

#[test]
fn range_short_circuit_boundary_receiver_effects() {
    for (declaration, setup, expression) in [
        (
            "class Gate(var n:Int){inout fun test():Boolean{n=2;return true}}",
            "var gate=Gate(1)",
            "flag && gate.test()",
        ),
        (
            "value class Gate(var n:Int){inout fun test():Boolean{n=2;return true}}",
            "var gate=Gate(1)",
            "flag && gate.test()",
        ),
        (
            "value class Gate(val text:String){own fun test():Boolean{return true}}",
            "val gate=Gate(\"owned\")",
            "flag || gate.test()",
        ),
        (
            "class Gate(val text:String){own fun test():Boolean{return true}}",
            "val gate=Gate(\"owned\")",
            "flag || gate.test()",
        ),
    ] {
        reject(
            &scoped(declaration, setup, &format!("show({expression})")),
            expression,
        );
    }
}

// These invalid forms stop in frontend, and must not be counted as backend gates.
#[test]
fn range_short_circuit_frontend_nothing_and_borrow_result_diagnostics() {
    use crate::native_tests::boxed_enum_tests::unit_source::{SourceRejectionKind, UnitSource};
    for (declaration, expression, fragment, code) in [
        ("", "flag || error(\"stop\")", "||", "L0085"),
        (
            "fun view(source:Boolean):borrow Boolean from source=source",
            "flag && view(flag)",
            "view(flag)",
            "L0164",
        ),
    ] {
        let consumer = scoped(declaration, "", &format!("show({expression})"));
        let text = format!(
            "{}\n{consumer}",
            include_str!("../../../../lang-std/koven/algorithms/ranges.ko")
                .strip_prefix("package koven.algorithms\n")
                .unwrap()
        );
        let mut authorized_source = None;
        let rejection = UnitSource::analyze_with_type_environment(
            "0317-frontier.ko",
            &text,
            |sources, source, types| {
                types.authorize_range_source(sources, source).unwrap();
                types
                    .authorize_range_extension_source(sources, source)
                    .unwrap();
                authorized_source = Some(source);
            },
        )
        .err()
        .expect("the original source must retain its frontend diagnostic");
        assert!(matches!(rejection.kind, SourceRejectionKind::Diagnostics));
        let expected_stage = if code == "L0085" {
            assert_eq!(
                rejection.report.stages,
                [("parse", 0), ("names", 0), ("types", 1)]
            );
            "types"
        } else {
            assert_eq!(
                rejection.report.stages,
                [("parse", 0), ("names", 0), ("types", 0), ("ownership", 1)]
            );
            "ownership"
        };
        let [(stage, diagnostic)] = rejection.report.diagnostics.as_slice() else {
            panic!("one exact original diagnostic: {:?}", rejection.report);
        };
        assert_eq!(*stage, expected_stage);
        assert_eq!(diagnostic.code().to_string(), code);
        let span = diagnostic.primary_span();
        assert_eq!(Some(span.source_id()), authorized_source);
        assert_eq!(
            rejection.sources.source_name(span.source_id()).unwrap(),
            "0317-frontier.ko"
        );
        assert_eq!(
            rejection.sources.source_text(span.source_id()).unwrap(),
            text
        );
        let start = text.rfind(fragment).unwrap();
        assert_eq!((span.start(), span.end()), (start, start + fragment.len()));
        assert_eq!(rejection.sources.slice(span).unwrap(), fragment);
    }
}

#[test]
fn range_short_circuit_boundary_outer_call_effects_and_new_metadata_prefix() {
    for (declaration, setup, statement) in [
        (
            "fun deliver(own text:String,result:Boolean):Unit{}",
            "val text=\"owned\"",
            "deliver(text,flag && flag)",
        ),
        (
            "fun deliver(inout n:Int,result:Boolean):Unit{}",
            "var n=1",
            "deliver(&n,flag && flag)",
        ),
        (
            "fun deliver(view:View<String>,result:Boolean):Unit{}",
            "",
            "deliver(take(part,1),flag && flag)",
        ),
    ] {
        reject(&scoped(declaration, setup, statement), "flag && flag");
    }
}

pub(in crate::ssa) mod local_control_support {
    //! Source-only setup and execution for the bounded local-control cases.
    use crate::ssa::range_lowering_tests::*;
    use crate::native_tests::boxed_enum_tests::unit_source::UnitSource;
    use crate::ssa::{LoweringError, LoweringErrorKind};
    use crate::test_support::diagnostics::{assert_eq_diagnostic, diagnostic};
    use lang_frontend::{
        name_resolution::{DeclarationId, SourceUnitKey, SymbolKind},
        parser::{Item, NameMarker},
        source::{SourceId, Span},
    };

    #[derive(Clone, Copy, Debug)]
    pub(in crate::ssa) enum Pipeline {
        Single,
        Unit,
        UnitReversed,
        SingleConstant,
        UnitConstant,
        UnitConstantReversed,
    }

    pub(in crate::ssa) struct SourceLowering {
        pub(in crate::ssa) sources: SourceMap,
        pub(in crate::ssa) expected_span: Span,
        pub(in crate::ssa) result: Result<(Program, FunctionId), LoweringError>,
    }

    pub(in crate::ssa) struct PositiveLowering {
        pub(in crate::ssa) expected_span: Span,
        products: PositiveProducts,
    }

    enum PositiveProducts {
        ExistingUnit(SourceLowering),
        MemoryUnit {
            // Keep the complete same-round owner, not an extracted or copied SourceMap.
            analysis: Box<UnitSource>,
            result: Result<(Program, FunctionId), (DeclarationId, LoweringError)>,
        },
    }

    // This entry is explicit at positive call sites. The old `lower` remains visible
    // to boundary submodules and the receiver differential through `use super::*`.
    pub(in crate::ssa) fn lower_positive(
        consumer: &str,
        fragment: &str,
        pipeline: Pipeline,
    ) -> PositiveLowering {
        if !matches!(pipeline, Pipeline::Single | Pipeline::SingleConstant) {
            let lowered = lower(consumer, fragment, pipeline);
            return PositiveLowering {
                expected_span: lowered.expected_span,
                products: PositiveProducts::ExistingUnit(lowered),
            };
        }
        let provider = include_str!("../../../../lang-std/koven/algorithms/ranges.ko");
        let text = format!(
            "{}\n{consumer}",
            provider.strip_prefix("package koven.algorithms\n").unwrap()
        );
        let mut authorized_source = None;
        let analysis = UnitSource::analyze_with_type_environment(
            "range-local-control-single.ko",
            &text,
            |sources, source, types| {
                types.authorize_range_source(sources, source).unwrap();
                types
                    .authorize_range_extension_source(sources, source)
                    .unwrap();
                authorized_source = Some(source);
            },
        )
        .unwrap_or_else(|error| panic!("original Range local source admission: {error:?}"));
        analysis.assert_frontend_clean();
        let source = analysis.parsed().source_id();
        assert_eq!(authorized_source, Some(source));
        let [input] = analysis.names().names().index().source_units() else {
            panic!("one original Range local MemoryDocument");
        };
        assert_eq!(input.key(), &SourceUnitKey::MemoryDocument);
        assert_eq!(input.source_id(), source);
        assert_eq!(
            analysis.sources().source_name(source).unwrap(),
            "range-local-control-single.ko"
        );
        assert_eq!(analysis.sources().source_text(source).unwrap(), text);
        let expected_span = fragment_span(analysis.sources(), source, consumer, fragment);
        let result = lower_positive_roots(&analysis);
        PositiveLowering {
            expected_span,
            products: PositiveProducts::MemoryUnit {
                analysis: Box::new(analysis),
                result,
            },
        }
    }

    fn lower_positive_roots(
        analysis: &UnitSource,
    ) -> Result<(Program, FunctionId), (DeclarationId, LoweringError)> {
        let parsed = analysis.parsed();
        let index = analysis.names().names().index();
        let [source] = index.source_units() else {
            panic!("one original Range local MemoryDocument");
        };
        let mut functions = Vec::new();
        let mut concrete_roots = Vec::new();
        // Start from the actual AST, so an omitted index declaration cannot silently
        // turn the old whole-file obligation into main-only reachability.
        for root in parsed.roots() {
            let original = parsed.ast().items().get(*root).unwrap();
            let mut node = original;
            while let Item::Modified { declaration, .. } = node.payload() {
                node = parsed.ast().items().get(*declaration).unwrap();
            }
            let Item::Function {
                name: NameMarker::Present(name),
                type_parameters,
                parameters,
                ..
            } = node.payload()
            else {
                continue;
            };
            let declarations = index
                .declarations()
                .iter()
                .filter(|declaration| {
                    declaration.root() == *root
                        && declaration.source_unit() == source.id()
                        && declaration.kind() == SymbolKind::Function
                })
                .collect::<Vec<_>>();
            let [declaration] = declarations.as_slice() else {
                panic!("each original function root has one real declaration: {declarations:?}");
            };
            assert_eq!(declaration.package(), source.package());
            assert_eq!(declaration.name_span(), *name);
            assert_eq!(name.source_id(), parsed.source_id());
            assert_eq!(analysis.sources().slice(*name).unwrap(), declaration.name());
            assert_eq!(analysis.declaration_span(declaration.id()), original.span());
            let signature = analysis
                .types()
                .signatures()
                .declaration(declaration.id())
                .and_then(|signature| signature.callable())
                .expect("every original function has a same-round callable signature");
            assert_eq!(signature.type_parameters().len(), type_parameters.len());
            assert_eq!(signature.parameters().len(), parameters.len());
            assert!(!functions.contains(&declaration.id()));
            functions.push(declaration.id());
            if type_parameters.is_empty() {
                concrete_roots.push(*declaration);
            }
        }
        assert_eq!(
            functions.len(),
            index
                .declarations()
                .iter()
                .filter(|declaration| declaration.kind() == SymbolKind::Function)
                .count(),
            "all original function roots and index declarations are reconciled"
        );
        let main = analysis.declaration("main", SymbolKind::Function);
        assert!(concrete_roots.iter().any(|root| root.id() == main));
        let mut programs = Vec::new();
        for root in concrete_roots {
            let (program, entry) = analysis
                .lower_entry(root.id())
                .map_err(|error| (root.id(), error))?;
            let function = program
                .module(entry.module())
                .unwrap()
                .function(entry)
                .unwrap();
            assert_eq!(
                function.origin,
                Origin::Source(analysis.declaration_span(root.id()))
            );
            verify_program(&program)
                .unwrap_or_else(|error| panic!("Range local root {root:?} SSA: {error:?}"));
            programs.push((root, program, entry));
        }
        // All roots must lower and verify before any LLVM/native work. Each extra
        // Program retains its own IDs; never merge or compare entities across roots.
        let mut main_program = None;
        for (root, program, entry) in programs {
            if root.id() == main {
                assert!(main_program.replace((program, entry)).is_none());
            } else {
                crate::llvm::render_verified_program(&program)
                    .unwrap_or_else(|error| panic!("Range local root {root:?} LLVM: {error:?}"));
            }
        }
        Ok(main_program.expect("the original main was lowered exactly once"))
    }

    impl PositiveLowering {
        pub(in crate::ssa) fn sources(&self) -> &SourceMap {
            match &self.products {
                PositiveProducts::ExistingUnit(lowered) => &lowered.sources,
                PositiveProducts::MemoryUnit { analysis, .. } => analysis.sources(),
            }
        }

        pub(in crate::ssa) fn require_success(self, label: &str) -> (Program, FunctionId) {
            match self.products {
                PositiveProducts::ExistingUnit(lowered) => lowered.require_success(label),
                PositiveProducts::MemoryUnit { analysis, result } => {
                    let (program, entry) = result.unwrap_or_else(|(root, error)| {
                        let fragment = error.span.and_then(|span| analysis.sources().slice(span).ok());
                        let declaration = analysis
                            .names()
                            .names()
                            .index()
                            .declarations()
                            .iter()
                            .find(|declaration| declaration.id() == root)
                            .expect("failed root belongs to the original source");
                        if root == analysis.declaration("main", SymbolKind::Function) {
                            assert_eq_diagnostic!(
                                error.kind,
                                LoweringErrorKind::UnsupportedNode,
                                "{label}: unexpected lowering failure: {error:?}; source={fragment:?}"
                            );
                            assert_eq_diagnostic!(
                                error.span,
                                Some(self.expected_span),
                                "{label}: wrong rejection span: {error:?}; source={fragment:?}"
                            );
                        }
                        // An auxiliary-root error is not relabeled as the main probe.
                        panic!(
                            "{label}: positive source root {root:?} {} must lower successfully: {error:?}; source={fragment:?}; probe={:?}",
                            declaration.name(), self.expected_span
                        );
                    });
                    verify_program(&program).unwrap();
                    (program, entry)
                }
            }
        }
    }

    pub(in crate::ssa) enum UnitExpectation<'a> {
        Reject(&'a str),
        Success(&'a [u8]),
    }

    // Retain one main-entry contract per original single-layout label. This does not
    // upgrade previously unobserved auxiliary roots to success obligations or claim
    // equivalence with the retired whole-file planner. Do not use lower_positive_roots.
    pub(in crate::ssa) fn observe_original_unit(
        consumer: &str,
        fragment: &str,
        pipeline: Pipeline,
        expected: UnitExpectation<'_>,
        label: &str,
    ) {
        assert!(matches!(
            pipeline,
            Pipeline::Single | Pipeline::SingleConstant
        ));
        let label = format!("local24-original-unit-{label}-{pipeline:?}");
        let name = "range-local-control-single.ko";
        let provider = include_str!("../../../../lang-std/koven/algorithms/ranges.ko");
        let text = format!(
            "{}\n{consumer}",
            provider.strip_prefix("package koven.algorithms\n").unwrap()
        );
        diagnostic!("{label}: stage=admission; source={name}; entry=main; fragment={fragment:?}");
        let mut authorized_source = None;
        let analysis =
            UnitSource::analyze_with_type_environment(name, &text, |sources, source, types| {
                types.authorize_range_source(sources, source).unwrap();
                types
                    .authorize_range_extension_source(sources, source)
                    .unwrap();
                authorized_source = Some(source);
            })
            .unwrap_or_else(|error| {
                panic!("{label}: stage=admission, not a backend rejection: {error:?}")
            });
        analysis.assert_frontend_clean();
        let source = analysis.parsed().source_id();
        assert_eq!(authorized_source, Some(source));
        let [input] = analysis.names().names().index().source_units() else {
            panic!("{label}: one original MemoryDocument");
        };
        assert_eq!(input.key(), &SourceUnitKey::MemoryDocument);
        assert_eq!(input.source_id(), source);
        assert_eq!(analysis.sources().source_name(source).unwrap(), name);
        assert_eq!(analysis.sources().source_text(source).unwrap(), text);
        let probe = fragment_span(analysis.sources(), source, consumer, fragment);
        let main = analysis.declaration("main", SymbolKind::Function);
        let scan = analysis.declaration("scan", SymbolKind::Function);
        for (function, declaration) in [("main", main), ("scan", scan)] {
            let spans = original_function_spans(analysis.sources(), analysis.parsed(), function);
            assert_eq!(analysis.declaration_span(declaration), spans[0]);
            let indexed = analysis
                .names()
                .names()
                .index()
                .declarations()
                .iter()
                .find(|candidate| candidate.id() == declaration)
                .unwrap();
            assert_eq!(indexed.name_span(), spans[1]);
            for span in spans {
                assert_eq!(span.source_id(), source);
                assert_eq!(
                    analysis.sources().slice(span).unwrap(),
                    &text[span.start()..span.end()]
                );
            }
            if function == "scan" {
                assert!(spans[2].start() <= probe.start() && probe.end() <= spans[2].end());
            }
        }
        diagnostic!(
            "{label}: stage=lowering; entry=main/{main:?}/{:?}; scan={scan:?}/{:?}; probe={probe:?}",
            analysis.declaration_span(main),
            analysis.declaration_span(scan)
        );
        // UnitSource chooses basic/constant only from the admitted typed products;
        // the legacy SingleConstant label does not itself confer const capability.
        let result = analysis.lower_entry(main);
        if let Err(error) = &result {
            let actual = error
                .span
                .and_then(|span| analysis.sources().slice(span).ok());
            let roots = analysis
                .names()
                .names()
                .index()
                .declarations()
                .iter()
                .filter(|declaration| analysis.parsed().roots().contains(&declaration.root()))
                .filter_map(|declaration| {
                    let span = analysis.declaration_span(declaration.id());
                    error
                        .span
                        .filter(|error_span| {
                            error_span.source_id() == span.source_id()
                                && span.start() <= error_span.start()
                                && error_span.end() <= span.end()
                        })
                        .map(|_| (declaration.name(), declaration.id(), span))
                })
                .collect::<Vec<_>>();
            diagnostic!(
                "{label}: stage=lowering; actual_roots={roots:?}; error={error:?}; source={actual:?}"
            );
        }
        match expected {
            UnitExpectation::Reject(expected_fragment) => {
                assert_eq!(expected_fragment, fragment);
                let error = result.err().unwrap_or_else(|| {
                    panic!("{label}: stage=lowering; main unexpectedly succeeded; frozen rejection retained")
                });
                assert_eq_diagnostic!(
                    error.kind,
                    LoweringErrorKind::UnsupportedNode,
                    "{label}: {error:?}"
                );
                assert_eq_diagnostic!(error.span, Some(probe), "{label}: {error:?}");
                assert_eq!(analysis.sources().slice(probe).unwrap(), expected_fragment);
                diagnostic!("{label}: outcome=exact-backend-reject; probe={probe:?}");
            }
            UnitExpectation::Success(stdout) => {
                let (program, entry) = result.unwrap_or_else(|error| {
                    panic!("{label}: stage=lowering; main must succeed: {error:?}")
                });
                let function = program
                    .module(entry.module())
                    .unwrap()
                    .function(entry)
                    .unwrap();
                assert_eq!(
                    function.origin,
                    Origin::Source(analysis.declaration_span(main))
                );
                verify_program(&program)
                    .unwrap_or_else(|error| panic!("{label}: stage=SSA: {error:?}"));
                diagnostic!("{label}: stage=LLVM/native; entry={entry:?}");
                run_unmodified(&program, entry, &label, stdout);
                diagnostic!("{label}: outcome=native-success; exit=0; stderr=empty; stdout={stdout:?}");
            }
        }
    }

    // Declaration, name, and body spans from each real AST, used only for identity.
    fn original_function_spans(
        sources: &SourceMap,
        parsed: &lang_frontend::parser::ParsedFile,
        function: &str,
    ) -> [Span; 3] {
        use lang_frontend::parser::{FunctionBody, FunctionForm};
        let mut matches = Vec::new();
        for root in parsed.roots() {
            let original = parsed.ast().items().get(*root).unwrap();
            let mut node = original;
            while let Item::Modified { declaration, .. } = node.payload() {
                node = parsed.ast().items().get(*declaration).unwrap();
            }
            let Item::Function {
                name: NameMarker::Present(name),
                form,
                ..
            } = node.payload()
            else {
                continue;
            };
            if sources.slice(*name).unwrap() != function {
                continue;
            }
            let body = match form {
                FunctionForm::ImplicitUnitBlock(body)
                | FunctionForm::Explicit {
                    body: FunctionBody::Block(body),
                    ..
                } => *body,
                _ => panic!("original {function} has a block body"),
            };
            matches.push([
                original.span(),
                *name,
                parsed.ast().statements().get(body).unwrap().span(),
            ]);
        }
        let [spans] = matches.as_slice() else {
            panic!("one real original top-level {function}: {matches:?}");
        };
        *spans
    }

    // Retain the Result and SourceMap: the initial positive-test failure must identify
    // the exact rejected expression, rather than panic inside an unwrap-only harness.
    pub(in crate::ssa) fn lower(consumer: &str, fragment: &str, pipeline: Pipeline) -> SourceLowering {
        let mut sources = SourceMap::new();
        let provider = include_str!("../../../../lang-std/koven/algorithms/ranges.ko");
        let (expected_span, result) = match pipeline {
            Pipeline::Single | Pipeline::SingleConstant => {
                unreachable!("single-layout labels use their explicit UnitSource adapters")
            }
            Pipeline::Unit
            | Pipeline::UnitReversed
            | Pipeline::UnitConstant
            | Pipeline::UnitConstantReversed => {
                let (provider_source, provider) =
                    crate::ssa::unit_lower_test_support::parsed(&mut sources, "ranges.ko", provider);
                let text = format!("package app\nimport koven.algorithms.take\n{consumer}");
                let (source, parsed) = crate::ssa::unit_lower_test_support::parsed(
                    &mut sources,
                    "range-local-control-unit.ko",
                    &text,
                );
                let expected_span = fragment_span(&sources, source, consumer, fragment);
                let mut inputs = [
                    SourceUnitInput::new(
                        "std",
                        "koven/algorithms/ranges.ko",
                        provider_source,
                        &provider,
                    ),
                    SourceUnitInput::new("app", "app/main.ko", source, &parsed),
                ];
                if matches!(
                    pipeline,
                    Pipeline::UnitReversed | Pipeline::UnitConstantReversed
                ) {
                    inputs.reverse();
                }
                let (environment, mut types) = standard_environments();
                types
                    .authorize_range_source(&sources, provider_source)
                    .unwrap();
                types
                    .authorize_range_extension_source(&sources, provider_source)
                    .unwrap();
                let result = if matches!(
                    pipeline,
                    Pipeline::UnitConstant | Pipeline::UnitConstantReversed
                ) {
                    use lang_frontend::{
                        name_resolution::{index_compilation_unit, resolve_compilation_unit_names},
                        ownership_checking::check_compilation_unit_constant_ownership,
                        type_checking::check_compilation_unit_types,
                    };
                    let index = index_compilation_unit(&sources, &inputs).unwrap();
                    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
                        .unwrap()
                        .validate()
                        .unwrap();
                    let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
                        .unwrap()
                        .validate_constants()
                        .unwrap();
                    let owned = check_compilation_unit_constant_ownership(
                        &sources, &inputs, &names, &types, &typed,
                    )
                    .unwrap()
                    .validate()
                    .unwrap();
                    let entry = crate::ssa::unit_lower_test_support::declaration(&names, "app", "main");
                    crate::ssa::unit_lower::constant::lower_constant_unit_with_entry(
                        &sources, &inputs, &names, &types, &typed, &owned, entry,
                    )
                } else {
                    let (names, typed, owned) = crate::ssa::unit_lower_test_support::analyze(
                        &sources,
                        &inputs,
                        &environment,
                        &types,
                    );
                    let entry = crate::ssa::unit_lower_test_support::declaration(&names, "app", "main");
                    crate::ssa::unit_lower::lower_scalar_unit_with_entry(
                        &sources, &inputs, &names, &types, &typed, &owned, entry,
                    )
                };
                (expected_span, result)
            }
        };
        SourceLowering {
            sources,
            expected_span,
            result,
        }
    }

    fn fragment_span(sources: &SourceMap, source: SourceId, consumer: &str, fragment: &str) -> Span {
        let text = sources.source_text(source).unwrap();
        assert!(text.ends_with(consumer));
        let consumer_start = text.len() - consumer.len();
        let offsets = consumer
            .match_indices(fragment)
            .map(|(offset, _)| offset)
            .collect::<Vec<_>>();
        assert_eq!(
            offsets.len(),
            1,
            "the source probe must be unique: {fragment}"
        );
        sources
            .span(
                source,
                consumer_start + offsets[0],
                consumer_start + offsets[0] + fragment.len(),
            )
            .unwrap()
    }

    impl SourceLowering {
        pub(in crate::ssa) fn require_success(self, label: &str) -> (Program, FunctionId) {
            if let Err(error) = self.result {
                let fragment = error.span.and_then(|span| self.sources.slice(span).ok());
                assert_eq_diagnostic!(
                    error.kind,
                    LoweringErrorKind::UnsupportedNode,
                    "{label}: unexpected lowering failure: {error:?}; source={fragment:?}"
                );
                assert_eq_diagnostic!(
                    error.span,
                    Some(self.expected_span),
                    "{label}: wrong rejection span: {error:?}; source={fragment:?}"
                );
                panic!(
                    "{label}: positive source must lower successfully: {error:?}; source={fragment:?}"
                );
            }
            let (program, entry) = self.result.unwrap();
            verify_program(&program).unwrap();
            (program, entry)
        }
    }

    pub(in crate::ssa) fn run_unmodified(program: &Program, entry: FunctionId, label: &str, expected: &[u8]) {
        use std::{fs, process::Command};
        let directory = std::env::temp_dir().join(format!(
            "koven-range-local-control-{}-{label}",
            std::process::id()
        ));
        fs::create_dir_all(&directory).unwrap();
        let ir = directory.join("program.ll");
        let executable = directory.join("program");
        let llvm = crate::llvm::render_verified_program_with_entry(program, entry).unwrap();
        fs::write(&ir, llvm).unwrap();
        let linked = Command::new(crate::test_support::ir_clang())
            .arg(&ir)
            .arg("-o")
            .arg(&executable)
            .output()
            .unwrap();
        assert!(linked.status.success(), "{label}: {linked:?}");
        let run = Command::new(&executable).output().unwrap();
        assert_eq!(run.status.code(), Some(0), "{label}: {run:?}");
        assert_eq!(run.stdout, expected, "{label}: {run:?}");
        assert!(run.stderr.is_empty(), "{label}: {run:?}");
        fs::remove_dir_all(directory).unwrap();
    }
}
