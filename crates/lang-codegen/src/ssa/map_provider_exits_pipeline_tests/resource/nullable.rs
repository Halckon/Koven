//! Existing nullable Map value representation, borrowed as a nullable handle without extraction.
use super::super::fixture::{Failure, failure, need};
use super::continuation::Case;
use crate::ssa::model::*;
use lang_frontend::{
    parser::{Item, NameMarker, ParsedFile},
    source::SourceMap,
};

fn nullable(text: &str) -> String {
    text.replace("String, Resource>", "String, Resource?>")
        .replace("String,Resource>", "String,Resource?>")
        .replace("value: Resource)", "value: Resource?)")
        .replace("value:Resource)", "value:Resource?)")
}
fn presence(text: &str) -> String {
    text.replace(
        "println(\"value-borrow\")",
        "val absent = value == null; if(absent){println(\"none\")}else{println(\"some\")}",
    )
}
pub(crate) fn sources() -> Vec<Case> {
    let base =
        include_str!("../../../../tests/fixtures/map-provider-resource-0331/boundary-nullable.ko");
    let normal =
        include_str!("../../../../tests/fixtures/map-provider-resource-0331/map-resource.stdout");
    let mut cases = vec![Case {
        label: "original-nullable",
        owned: true,
        source: base.to_owned(),
        golden: normal.to_owned(),
    }];
    cases.push(Case{label:"nullable-mixed",owned:true,source:presence(&base.replace("  source.put(\"a\".clone(), Resource())","  source.put(\"n\".clone(), null)\n  source.put(\"a\".clone(), Resource())")),golden:"nonempty\nz\nsome\nn\nnone\na\nsome\nexhausted\nresource-drop\nresource-drop\nempty\nexhausted\nentry-end\n".to_owned()});
    cases.push(Case {
        label: "nullable-all-null",
        owned: true,
        source: presence(&base.replace("Resource())", "null)")),
        golden: "nonempty\nz\nnone\na\nnone\nexhausted\nempty\nexhausted\nentry-end\n".to_owned(),
    });
    let continued = super::continued::sources();
    for (index, label) in [
        (2, "nullable-own-continue"),
        (5, "nullable-borrow-map-continue"),
        (6, "nullable-borrow-mutable-continue"),
    ] {
        let case = &continued[index];
        cases.push(Case {
            label,
            owned: case.owned,
            source: presence(&nullable(&case.source)),
            golden: case.golden.replace("value-borrow\n", "some\n"),
        });
    }
    let own = &cases[3];
    cases.push(Case {
        label: "nullable-null-continue",
        owned: true,
        source: own.source.replace("Resource())", "null)"),
        golden: own
            .golden
            .replace("some\n", "none\n")
            .replace("resource-drop\n", ""),
    });
    assert_eq!(cases.len(), 7);
    cases
}

fn representation(
    program: &Program,
    input: &super::identity::Input,
    checked_presence: bool,
    sources: &SourceMap,
    parsed: &ParsedFile,
) -> Result<(), Failure> {
    use super::super::query_inputs::one;
    let module = one(program.modules.iter(), "one nullable program module")?;
    let scan = one(
        module
            .functions
            .iter()
            .filter(|f| f.origin == Origin::Source(input.scan)),
        "actual nullable scan",
    )?;
    let value = one(
        scan.instructions
            .iter()
            .filter(|i| matches!(i.operation, Operation::MapIterationValueLoan { .. })),
        "actual nullable value loan",
    )?;
    let [EntityId::Loan(loan)] = value.results.as_slice() else {
        return Err(failure("nullable shared value", value));
    };
    let EntityType::Loan {
        kind: LoanKind::Shared,
        target,
    } = scan.entity(EntityId::Loan(*loan)).unwrap().ty
    else {
        return Err(failure("shared nullable loan type", value));
    };
    let Some(SsaTypeKind::NullableHandle { inner }) = module.type_kind(target) else {
        return Err(failure("existing nullable handle ABI", target));
    };
    need(
        matches!(
            module.type_kind(*inner),
            Some(SsaTypeKind::HeapOwner { .. })
        ) && module.deinits.contains_key(inner),
        "nullable inner retains actual resource drop glue",
        inner,
    )?;
    let class = one(
        parsed.roots().iter().filter_map(|id| {
            let n = parsed.ast().items().get(*id).ok()?;
            let Item::Classifier(class) = n.payload() else {
                return None;
            };
            matches!(class.name, NameMarker::Present(name) if sources.slice(name) == Ok("Resource"))
                .then_some(class)
        }),
        "original nullable Resource class",
    )?;
    let body = class
        .body
        .as_ref()
        .ok_or_else(|| failure("Resource body", class))?;
    let member = one(
        body.members.iter().filter_map(|id| {
            let n = parsed.ast().items().get(*id).ok()?;
            matches!(n.payload(), Item::Deinit { .. }).then_some(n)
        }),
        "original Resource deinit",
    )?;
    let actual = module
        .deinits
        .get(inner)
        .and_then(|id| module.function(*id))
        .ok_or_else(|| failure("actual nullable Resource drop glue", inner))?;
    need(
        actual.origin == Origin::Source(member.span()),
        "nullable inner uses original Resource deinit identity",
        actual,
    )?;
    let read = one(
        module
            .functions
            .iter()
            .filter(|f| f.origin == Origin::Source(input.read)),
        "actual nullable reader",
    )?;
    let parameters = &read.block(read.entry_block().unwrap()).unwrap().parameters;
    need(
        parameters.len() == 2
            && read.entity(parameters[1]).unwrap().ty
                == EntityType::Loan {
                    kind: LoanKind::Shared,
                    target,
                },
        "reader uses exact nullable component type",
        read,
    )?;
    if checked_presence {
        let checks = read
            .instructions
            .iter()
            .filter(|i| matches!(i.operation, Operation::NullableLoanIsNull { .. }))
            .collect::<Vec<_>>();
        need(
            matches!(checks.as_slice(),[check] if matches!(check.operation,Operation::NullableLoanIsNull{source} if EntityId::Loan(source)==parameters[1])),
            "presence reads original borrowed nullable, no owner copy",
            &checks,
        )?;
        need(
            !read.instructions.iter().any(|i| {
                matches!(
                    i.operation,
                    Operation::NullableTake { .. }
                        | Operation::NullableWrap { .. }
                )
            }) && !read.instructions.iter().any(|i|matches!(i.operation,Operation::Drop{owner} if matches!(read.entity(EntityId::Value(owner)).unwrap().ty,EntityType::Value(t) if t==target || t==*inner))),
            "presence test does not acquire or drop Resource owner",
            read,
        )?;
    }
    Ok(())
}

#[test]
fn nullable_resource_components_reach_both_verified_pipelines() {
    let results = sources()
        .iter()
        .flat_map(|case| {
            [false, true].map(|unit| {
                (
                    case.label,
                    unit,
                    super::continuation::observe_with(
                        case,
                        unit,
                        |program, input, sources, parsed| {
                            representation(
                                program,
                                input,
                                case.label != "original-nullable",
                                sources,
                                parsed,
                            )
                        },
                    ),
                )
            })
        })
        .collect::<Vec<_>>();
    assert_eq!(results.len(), 14);
    assert!(results.iter().all(|(_, _, r)| r.is_ok()), "{results:#?}");
}
