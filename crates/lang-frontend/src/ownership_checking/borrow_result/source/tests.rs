use super::*;
use crate::{
    lexer::lex,
    parser::{BorrowReturnSyntax, CarrierReturnSyntax, FunctionResultSource, Item, parse_file},
};

fn syntax(text: &str) -> (SourceMap, FunctionForm, Vec<ValueParameter>) {
    let mut sources = SourceMap::new();
    let source = sources.add_source("selector.ko", text).unwrap();
    let lexed = lex(&sources, source).unwrap();
    assert!(lexed.diagnostics().is_empty());
    let parsed = parse_file(&sources, &lexed).unwrap();
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let functions = parsed
        .ast()
        .items()
        .iter()
        .filter_map(|(_, node)| {
            if let Item::Function {
                form, parameters, ..
            } = node.payload()
            {
                Some((*form, parameters.clone()))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let [(form, parameters)] = functions.as_slice() else {
        panic!("one function")
    };
    (sources, *form, parameters.clone())
}

#[test]
fn modes_first_match_and_failed_lookup_keep_existing_selection() {
    for mode in ["", "borrow ", "inout ", "own "] {
        let text = format!("fun view({mode}source: String): borrow String from source = source");
        let (sources, form, parameters) = syntax(&text);
        let mut calls = Vec::new();
        let selected = borrow_return_source(&sources, form, &parameters, None, |name| {
            calls.push(name);
            Some(7)
        })
        .unwrap();
        let allowed = matches!(mode, "" | "borrow ");
        assert_eq!(selected.symbol, allowed.then_some(7));
        assert_eq!(
            calls,
            if allowed {
                vec![parameters[0].name]
            } else {
                vec![]
            }
        );
        assert_eq!(selected.span, marker_span(parameters[0].name));
        assert_eq!(sources.slice(selected.marker).unwrap(), "borrow");
        assert!(!selected.new_range);
    }
    let (sources, form, parameters) =
        syntax("fun view(source: String): borrow String from source = source");
    let mut duplicate = parameters.clone();
    duplicate[0].mode_marker = Some(ParameterModeMarker::Own(parameters[0].span));
    duplicate.extend(parameters.clone());
    let selected = borrow_return_source(&sources, form, &duplicate, None::<u8>, |_| {
        panic!("first own match must not resolve")
    })
    .unwrap();
    assert_eq!(selected.symbol, None);
    // Preserve historical failed-slice equality: both .ok() values are None.
    let foreign = SourceMap::new();
    let selected = borrow_return_source(&foreign, form, &parameters, None, |_| Some(9)).unwrap();
    assert_eq!(selected.symbol, Some(9));
    assert_eq!(selected.span, marker_span(parameters[0].name));
}

#[test]
fn syntax_markers_receiver_recovery_and_absent_source_are_preserved() {
    let (sources, mut form, parameters) =
        syntax("fun view(source: String): borrow String from source = source");
    let FunctionForm::Explicit {
        result_source: Some(FunctionResultSource::Borrow(original)),
        ..
    } = form
    else {
        panic!("borrow syntax")
    };
    for declared in [
        BorrowReturnSource::Receiver(original.from_span),
        BorrowReturnSource::Parameter(NameMarker::Missing(original.from_span)),
        BorrowReturnSource::Parameter(NameMarker::Error(original.from_span)),
        BorrowReturnSource::Parameter(NameMarker::Present(original.borrow_span)),
    ] {
        for carrier in [false, true] {
            let FunctionForm::Explicit { result_source, .. } = &mut form else {
                unreachable!()
            };
            *result_source = Some(if carrier {
                FunctionResultSource::Carrier(CarrierReturnSyntax {
                    from_span: original.from_span,
                    source: declared,
                })
            } else {
                FunctionResultSource::Borrow(BorrowReturnSyntax {
                    source: declared,
                    ..original
                })
            });
            let selected = borrow_return_source(&sources, form, &parameters, Some(11), |_| {
                panic!("no matching parameter")
            })
            .unwrap();
            assert_eq!(
                selected.symbol,
                matches!(declared, BorrowReturnSource::Receiver(_)).then_some(11)
            );
            let expected_span = match declared {
                BorrowReturnSource::Receiver(span) => span,
                BorrowReturnSource::Parameter(name) => marker_span(name),
            };
            assert_eq!(selected.span, expected_span);
            assert_eq!(
                selected.marker,
                if carrier {
                    original.from_span
                } else {
                    original.borrow_span
                }
            );
            assert_eq!(selected.new_range, carrier);
        }
    }
    let FunctionForm::Explicit { result_source, .. } = &mut form else {
        unreachable!()
    };
    *result_source = None;
    for form in [form, FunctionForm::ImplicitUnitAbsent] {
        assert!(
            borrow_return_source(&sources, form, &parameters, Some(11), |_| panic!(
                "no source"
            ))
            .is_none()
        );
    }
}

#[test]
fn typed_inout_borrow_return_keeps_p3_unsupported_flow_diagnostic() {
    use crate::{
        name_resolution::{
            SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names, resolve_names,
        },
        ownership_checking::{check_compilation_unit_ownership, check_ownership},
        type_checking::{check_compilation_unit_types, check_types, standard_environments},
    };
    let text = "fun view(inout source: String): borrow String from source = source";
    let mut sources = SourceMap::new();
    let source = sources.add_source("inout-source.ko", text).unwrap();
    let lexed = lex(&sources, source).unwrap();
    assert!(lexed.diagnostics().is_empty());
    let parsed = parse_file(&sources, &lexed).unwrap();
    assert!(parsed.diagnostics().is_empty());
    let (environment, types) = standard_environments();
    let names = resolve_names(&sources, &parsed, &environment).unwrap();
    assert!(names.diagnostics().is_empty());
    let typed = check_types(&sources, &parsed, &names, &types).unwrap();
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    let single = check_ownership(&sources, &parsed, &names, &typed).unwrap();
    let inputs = [SourceUnitInput::memory_document(source, &parsed)];
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
        .unwrap()
        .validate()
        .unwrap();
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    let typed = typed.validate().unwrap();
    let unit = check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap();
    let marker_start = text.find("borrow").unwrap();
    let marker = sources
        .span(source, marker_start, marker_start + "borrow".len())
        .unwrap();
    for diagnostics in [single.diagnostics(), unit.diagnostics()] {
        let [diagnostic] = diagnostics else {
            panic!("one unsupported flow: {diagnostics:?}")
        };
        assert_eq!(diagnostic.code().to_string(), "L0164");
        assert_eq!(diagnostic.primary_span(), marker);
    }
    assert!(single.borrow_return_origins().is_empty());
    assert!(unit.borrow_return_origins().is_empty());
}
