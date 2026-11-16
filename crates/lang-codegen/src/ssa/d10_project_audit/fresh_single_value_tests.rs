//! Single fresh scope with a Value Boolean, preserving the original resource boundary.
use super::*;
use crate::ssa::{
    model::{EntityId, TerminatorKind},
    refine::fresh_loop_receipt::{self, Tag},
    unit_lower, unit_lower_test_support,
};
use lang_frontend::{
    name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
};
#[test]
fn single_value_flag_consumes_original_roles_and_zero_iteration_exit() {
    for reverse in [false, true] {
        for abi in [false, true] {
            for segments in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                for uses in [
                    "consume(item)",
                    "consume(item);other(item)",
                    "other(item);consume(item)",
                ] {
                    check(reverse, abi, segments, uses, true);
                }
            }
        }
    }
}
#[test]
fn single_value_flag_retains_long_body_and_source_segment_gaps() {
    for segments in [(0, 0), (1, 1)] {
        check(
            false,
            false,
            segments,
            "consume(item);other(item);consume(item)",
            false,
        );
    }
    for segments in [(2, 0), (0, 2), (2, 1), (1, 2)] {
        check(false, false, segments, "consume(item)", false);
    }
}
fn check(reverse: bool, abi: bool, segments: (usize, usize), uses: &str, certified: bool) {
    let parameters = if abi {
        "own flag:Boolean,source:String"
    } else {
        "source:String,own flag:Boolean"
    };
    check_parameters(reverse, parameters, segments, uses, certified);
}
pub(super) fn check_parameters(
    reverse: bool,
    parameters: &str,
    segments: (usize, usize),
    uses: &str,
    certified: bool,
) {
    let mut sources = SourceMap::new();
    let (provider_id, provider) = unit_lower_test_support::parsed(
        &mut sources,
        "single-value-provider.ko",
        "fun view(source:String):borrow String from source=source\nfun consume(item:String):Unit {}\nfun other(item:String):Unit {}\nfun before(item:String):Unit {}\nfun after(item:String):Unit {}",
    );
    let text = format!(
        "fun run({parameters}):Unit {{{}while(flag){{borrow val item=view(source);{uses}}};{}}}",
        "before(source);".repeat(segments.0),
        "after(source);".repeat(segments.1)
    );
    let (caller_id, caller) =
        unit_lower_test_support::parsed(&mut sources, "single-value-caller.ko", &text);
    let mut inputs = [
        SourceUnitInput::new("root", "single-value-provider.ko", provider_id, &provider),
        SourceUnitInput::new("root", "single-value-caller.ko", caller_id, &caller),
    ];
    if reverse {
        inputs.reverse();
    }
    let (environment, types) = standard_environments();
    let (names, typed, owned) =
        unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
    let declaration = |name| {
        names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|d| d.name() == name)
            .unwrap()
            .id()
    };
    let ((results, audits), receipts) = fresh_loop_receipt::observe(|| {
        observe(|| {
            (0..2)
                .map(|_| {
                    unit_lower::lower_scalar_unit_with_entry(
                        &sources,
                        &inputs,
                        &names,
                        &types,
                        &typed,
                        &owned,
                        declaration("run"),
                    )
                })
                .collect::<Vec<_>>()
        })
    });
    assert_eq!(audits.len(), 2);
    assert_eq!(receipts.len(), 2);
    for ((result, audit), receipt) in results.into_iter().zip(audits).zip(receipts) {
        let (program, entry) = result.unwrap();
        crate::ssa::verify::verify_program(&program).unwrap();
        let function = program
            .modules
            .iter()
            .flat_map(|m| &m.functions)
            .find(|f| f.id == entry)
            .unwrap();
        receipt.validate(function).unwrap();
        let edge = (declaration("run"), declaration("view"));
        if !certified {
            assert!(!audit.borrow_edges.contains(&edge));
            assert!(
                audit.unsupported.iter().any(|g| g
                    == "fresh no-carry missing/duplicate/outside-domain or final evidence gap")
            );
            continue;
        }
        assert!(
            audit.borrow_edges.contains(&edge),
            "{:?}",
            audit.unsupported
        );
        assert!(audit.unsupported.is_empty(), "{:?}", audit.unsupported);
        let local = &receipt.loops[0];
        assert_eq!(receipt.loops.len(), 1);
        let start = function.block(function.entry_block().unwrap()).unwrap();
        let params: Vec<_> = parameters.split(',').collect();
        let source_role = params.iter().position(|p| *p == "source:String").unwrap();
        let flag_role = params
            .iter()
            .position(|p| p.trim_start_matches("own ") == "flag:Boolean")
            .unwrap();
        let flag_owned = params[flag_role].starts_with("own ");
        assert_eq!(start.parameters.len(), params.len());
        assert!(matches!(start.parameters[source_role], EntityId::Loan(_)));
        assert_eq!(
            matches!(start.parameters[flag_role], EntityId::Value(_)),
            flag_owned
        );
        let mut roles: BTreeMap<_, _> = start
            .parameters
            .iter()
            .copied()
            .enumerate()
            .map(|(i, e)| (e, i))
            .collect();
        for _ in 0..function.blocks.len() {
            for (_, e) in &receipt.edges {
                for (a, p) in e
                    .arguments
                    .iter()
                    .zip(&function.block(e.target).unwrap().parameters)
                {
                    if let Some(r) = roles.get(a).copied()
                        && let Some(old) = roles.insert(*p, r)
                    {
                        assert_eq!(old, r);
                    }
                }
            }
        }
        let acquire = function
            .instruction(
                local
                    .instructions
                    .iter()
                    .find(|(t, _)| *t == Tag::Result)
                    .unwrap()
                    .1,
            )
            .unwrap();
        let headers:Vec<_>=function.blocks.iter().filter(|b|matches!(&b.terminator.as_ref().unwrap().kind,TerminatorKind::Conditional{when_true,..} if when_true.target==acquire.block)).collect();
        let [header] = headers.as_slice() else {
            panic!("one header");
        };
        let TerminatorKind::Conditional {
            condition,
            when_false,
            ..
        } = &header.terminator.as_ref().unwrap().kind
        else {
            panic!("conditional");
        };
        if flag_owned {
            assert!(!local.instructions.iter().any(|(t, _)| *t == Tag::Condition));
            assert_eq!(roles.get(&EntityId::Value(*condition)), Some(&flag_role));
        } else {
            let read = function
                .instruction(
                    local
                        .instructions
                        .iter()
                        .find(|(t, _)| *t == Tag::Condition)
                        .unwrap()
                        .1,
                )
                .unwrap();
            let Operation::Read {
                source: crate::ssa::model::PlaceAccess::Loan(loan),
            } = read.operation
            else {
                panic!("borrow Read");
            };
            assert_eq!(roles.get(&EntityId::Loan(loan)), Some(&flag_role));
            assert_eq!(read.results.as_slice(), &[EntityId::Value(*condition)]);
        }
        for (unused, parameter) in params
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != source_role && *i != flag_role)
        {
            let entity = start.parameters[unused];
            assert_eq!(
                matches!(entity, EntityId::Value(_)),
                parameter.starts_with("own ")
            );
            assert!(
                receipt
                    .edges
                    .iter()
                    .all(|(_, e)| !e.arguments.contains(&entity))
            );
            for block in &function.blocks {
                if block.id != start.id {
                    assert!(
                        block
                            .parameters
                            .iter()
                            .all(|p| roles.get(p) != Some(&unused))
                    );
                }
            }
        }
        let actual: BTreeSet<_> = when_false
            .arguments
            .iter()
            .map(|e| *roles.get(e).unwrap())
            .collect();
        assert_eq!(
            actual,
            if segments.1 == 0 {
                BTreeSet::new()
            } else {
                BTreeSet::from([source_role])
            }
        );
        let [fresh] = acquire.results.as_slice() else {
            panic!("one fresh");
        };
        assert_eq!(local.fact.consumers().len(), uses.split(';').count());
        let body = function.block(acquire.block).unwrap();
        let end = function
            .instruction(
                local
                    .instructions
                    .iter()
                    .find(|(t, _)| *t == Tag::ResultEnd)
                    .unwrap()
                    .1,
            )
            .unwrap();
        let mut previous = body
            .instructions
            .iter()
            .position(|id| *id == acquire.id)
            .unwrap();
        for (i, name) in uses.split(';').enumerate() {
            let call = function
                .instruction(
                    local
                        .instructions
                        .iter()
                        .find(|(t, _)| *t == Tag::Consumer(i))
                        .unwrap()
                        .1,
                )
                .unwrap();
            let Operation::DirectCall {
                callee,
                receiver: None,
                arguments,
            } = &call.operation
            else {
                panic!("consumer call");
            };
            assert_eq!(arguments.as_slice(), &[*fresh]);
            let target = program
                .modules
                .iter()
                .flat_map(|m| &m.functions)
                .find(|f| f.id == *callee)
                .unwrap();
            assert!(
                target
                    .name
                    .contains(&format!(".{}.", name.split('(').next().unwrap()))
            );
            let position = body
                .instructions
                .iter()
                .position(|id| *id == call.id)
                .unwrap();
            assert!(position > previous);
            previous = position;
        }
        assert!(
            body.instructions
                .iter()
                .position(|id| *id == end.id)
                .unwrap()
                > previous
        );
        assert!(matches!(end.operation,Operation::BorrowEnd{loan} if EntityId::Loan(loan)==*fresh));
        assert_eq!(function.instructions.iter().filter(|i|matches!(i.operation,Operation::BorrowEnd{loan} if EntityId::Loan(loan)==*fresh)).count(),1);
        assert!(!roles.contains_key(fresh));
        assert!(
            receipt
                .edges
                .iter()
                .all(|(_, e)| !e.arguments.contains(fresh))
        );
    }
}

#[test]
fn single_value_flag_preserves_resource_extra_parameter_and_condition_boundaries() {
    use lang_frontend::{
        name_resolution::{index_compilation_unit, resolve_compilation_unit_names},
        ownership_checking::check_compilation_unit_ownership,
        type_checking::check_compilation_unit_types,
    };
    for (parameters, condition, rejected) in [
        ("own source:String,own flag:Boolean", "flag", true),
        ("source:String,own flag:Boolean", "(flag)", true),
        (
            "source:String,own flag:Boolean,unused:Boolean,extra:Boolean",
            "flag",
            false,
        ),
    ] {
        let mut sources = SourceMap::new();
        let (p, provider) = unit_lower_test_support::parsed(
            &mut sources,
            "boundary-provider.ko",
            "fun view(source:String):borrow String from source=source\nfun consume(item:String):Unit {}",
        );
        let text = format!(
            "fun run({parameters}):Unit {{while({condition}){{borrow val item=view(source);consume(item)}}}}"
        );
        let (c, caller) =
            unit_lower_test_support::parsed(&mut sources, "boundary-caller.ko", &text);
        let inputs = [
            SourceUnitInput::new("root", "boundary-provider.ko", p, &provider),
            SourceUnitInput::new("root", "boundary-caller.ko", c, &caller),
        ];
        let (environment, types) = standard_environments();
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
            .unwrap()
            .validate()
            .unwrap();
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
            .unwrap()
            .validate()
            .unwrap();
        let (((outcome, errors), audits), receipts) = fresh_loop_receipt::observe(|| {
            observe(|| {
                let raw =
                    check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
                        .unwrap();
                match raw.validate() {
                    Err(raw) => (
                        None,
                        raw.diagnostics()
                            .iter()
                            .map(|d| (format!("{:?}", d.code()), d.primary_span()))
                            .collect::<Vec<_>>(),
                    ),
                    Ok(owned) => {
                        let entry = names
                            .names()
                            .index()
                            .declarations()
                            .iter()
                            .find(|d| d.name() == "run")
                            .unwrap()
                            .id();
                        (
                            Some(unit_lower::lower_scalar_unit_with_entry(
                                &sources, &inputs, &names, &types, &typed, &owned, entry,
                            )),
                            vec![],
                        )
                    }
                }
            })
        });
        if rejected {
            assert!(outcome.is_none());
            assert_eq!(errors.len(), 1);
            let (code, span) = &errors[0];
            assert_eq!(code, "L0164");
            let start = text.find("borrow val").unwrap();
            assert_eq!(span.source_id(), c);
            assert_eq!((span.start(), span.end()), (start, start + 6));
            assert!(audits.is_empty());
            assert!(receipts.is_empty());
        } else {
            assert!(errors.is_empty());
            let (program, entry) = outcome.unwrap().unwrap();
            crate::ssa::verify::verify_program(&program).unwrap();
            assert_eq!(receipts.len(), 1);
            assert_eq!(audits.len(), 1);
            let function = program
                .modules
                .iter()
                .flat_map(|m| &m.functions)
                .find(|f| f.id == entry)
                .unwrap();
            receipts[0].validate(function).unwrap();
            assert!(
                audits[0].unsupported.iter().any(|g| g
                    == "fresh no-carry missing/duplicate/outside-domain or final evidence gap")
            );
            assert!(audits[0].borrow_edges.is_empty());
        }
    }
}
