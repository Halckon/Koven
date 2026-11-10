//! Actual normal-completion observations, independent of descriptor presence.
use super::*;
use lang_frontend::{
    ownership_checking::{CompilationUnitOwnership, UnitDropPoint, UnitLoanTarget},
    type_checking::{CompilationUnitTypes, UnitExpressionId},
};

pub(super) fn assert_complete(typed: &CompilationUnitTypes, owned: &CompilationUnitOwnership) {
    assert_eq!(
        owned.string_clone_loan_ends().len(),
        typed.string_operations().len()
    );
    for operation in typed.string_operations() {
        let ends = owned
            .string_clone_loan_ends()
            .iter()
            .filter(|end| end.loan().call() == operation.expression())
            .collect::<Vec<_>>();
        assert_eq!(
            ends.len(),
            1,
            "one actual end for each original successful clone"
        );
        let end = ends[0];
        assert_eq!(
            end.point(),
            UnitDropPoint::CallReturn(operation.expression())
        );
        assert_eq!(end.loan().argument(), operation.receiver());
        assert_eq!(end.loan().kind(), LoanKind::Shared);
        let loans = owned
            .loans()
            .iter()
            .filter(|loan| {
                loan.call() == operation.expression() && loan.argument() == operation.receiver()
            })
            .collect::<Vec<_>>();
        assert_eq!(
            loans.as_slice(),
            [end.loan()],
            "preserve the complete original loan identity"
        );
    }
}

#[derive(Debug)]
enum Target<'a> {
    Named(&'a str),
    Temporary(&'a str),
}

fn check_both_ends(text: &str, operations: usize, expected: &[(&str, &str, Target<'_>)]) {
    let mut sources = SourceMap::new();
    let source = sources.add_source("ends.ko", text).unwrap();
    let lexed = lex(&sources, source).unwrap();
    assert!(lexed.diagnostics().is_empty());
    let parsed = parse_file(&sources, &lexed).unwrap();
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let (environment, types) = standard_environments();
    for input in [
        SourceUnitInput::memory_document(source, &parsed),
        SourceUnitInput::new("root", "ends.ko", source, &parsed),
    ] {
        let inputs = [input];
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        assert!(index.diagnostics().is_empty(), "{:?}", index.diagnostics());
        let names =
            resolve_compilation_unit_names(&sources, &inputs, &index, &environment).unwrap();
        assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
        let names = names.validate().unwrap();
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        assert_eq!(
            typed.string_operations().len(),
            operations,
            "typed presence is not the end oracle"
        );
        let typed = typed.validate().unwrap();
        let owned =
            check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap();
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
        assert!(owned.clone().validate().is_ok());
        let source_unit = names.names().index().source_units()[0].id();
        let expression = |text: &str| {
            let ids = parsed
                .ast()
                .expressions()
                .iter()
                .filter_map(|(id, node)| {
                    (sources.slice(node.span()).unwrap() == text).then_some(id)
                })
                .collect::<Vec<_>>();
            assert_eq!(ids.len(), 1, "unique real AST expression: {text}");
            UnitExpressionId::new(source_unit, ids[0])
        };
        assert_eq!(
            owned.string_clone_loan_ends().len(),
            expected.len(),
            "no missing or fabricated visits"
        );
        for (end, (call, argument, target)) in owned.string_clone_loan_ends().iter().zip(expected) {
            let call = expression(call);
            let argument = expression(argument);
            assert_eq!(end.point(), UnitDropPoint::CallReturn(call));
            assert_eq!(end.loan().call(), call);
            assert_eq!(end.loan().argument(), argument);
            assert_eq!(end.loan().kind(), LoanKind::Shared);
            assert_eq!(end.loan().parameter_span(), None);
            assert_eq!(
                end.loan().begin_span(),
                parsed
                    .ast()
                    .expressions()
                    .get(argument.expression())
                    .unwrap()
                    .span()
            );
            assert_eq!(
                end.loan().end_span(),
                parsed
                    .ast()
                    .expressions()
                    .get(call.expression())
                    .unwrap()
                    .span()
            );
            match (end.loan().target(), target) {
                (UnitLoanTarget::Place(place), Target::Named(name)) => {
                    assert!(place.is_root());
                    assert_eq!(place.root().source_unit(), source_unit);
                    let symbols = names.names().source_units()[source_unit.index()]
                        .resolution()
                        .symbols();
                    let expected = symbols
                        .iter()
                        .filter(|symbol| symbol.name() == *name)
                        .collect::<Vec<_>>();
                    assert_eq!(expected.len(), 1, "unique declared owner");
                    assert_eq!(place.root().symbol(), expected[0].id());
                }
                (UnitLoanTarget::Temporary(owner), Target::Temporary(text)) => {
                    assert_eq!(*owner, expression(text))
                }
                actual => panic!("unexpected target: {actual:?}"),
            }
            let loans = owned
                .loans()
                .iter()
                .filter(|loan| loan.call() == call && loan.argument() == argument)
                .collect::<Vec<_>>();
            assert_eq!(loans.as_slice(), [end.loan()]);
        }
    }
}

#[test]
fn named_and_nested_temporary_receivers_have_exact_normal_ends() {
    check_both_ends(
        "fun run(): Unit { val source = \"kept\"; val first = source.clone(); val second = (\"nested\".clone()).clone(); println(first); println(second) }",
        3,
        &[
            ("source.clone()", "source", Target::Named("source")),
            (
                "\"nested\".clone()",
                "\"nested\"",
                Target::Temporary("\"nested\""),
            ),
            (
                "(\"nested\".clone()).clone()",
                "(\"nested\".clone())",
                Target::Temporary("(\"nested\".clone())"),
            ),
        ],
    );
}

#[test]
fn borrowed_receiver_has_exact_normal_end() {
    check_both_ends(
        "fun copy(text: String): String = text.clone()",
        1,
        &[("text.clone()", "text", Target::Named("text"))],
    );
}

#[test]
fn clone_after_return_does_not_publish_a_normal_end() {
    check_both_ends(
        "fun copy(text: String): String { return \"done\"; val unused = text.clone() }",
        1,
        &[],
    );
}

#[test]
fn completed_inner_clone_survives_returning_outer_receiver() {
    check_both_ends(
        "fun select(first: String, second: String): String = first.clone()\nfun copy(): String { return select(\"inner\".clone(), return \"done\").clone() }",
        3,
        &[
            ("first.clone()", "first", Target::Named("first")),
            (
                "\"inner\".clone()",
                "\"inner\"",
                Target::Temporary("\"inner\""),
            ),
        ],
    );
}

#[test]
fn ordinary_clone_and_container_size_are_not_string_ends() {
    check_both_ends(
        "class Sample { fun clone(): Int = 1 }\nfun read(sample: Sample, values: List<String>, entries: Map<String, Int>): Int { val copy = sample.clone(); return values.size + entries.size + copy }",
        0,
        &[],
    );
}
