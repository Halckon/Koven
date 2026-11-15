//! Real-source independent Boolean roles; no synthetic receipts or native execution.
use super::*;
use crate::ssa::{
    model::{EntityId, PlaceAccess, TerminatorKind},
    refine::fresh_loop_receipt::{self, Tag},
    unit_lower, unit_lower_test_support,
};
use lang_frontend::{
    name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
};

#[test]
fn independent_flags_follow_all_abi_positions_and_condition_orders() {
    for parameters in [
        "source:String,a:Boolean,b:Boolean",
        "source:String,b:Boolean,a:Boolean",
        "a:Boolean,source:String,b:Boolean",
        "b:Boolean,source:String,a:Boolean",
        "a:Boolean,b:Boolean,source:String",
        "b:Boolean,a:Boolean,source:String",
    ] {
        for flags in [["a", "b"], ["b", "a"]] {
            for reverse in [false, true] {
                for suffix in [false, true] {
                    check(parameters, flags, reverse, suffix, true);
                }
            }
        }
    }
}

#[test]
fn extra_and_non_name_flags_do_not_acquire_sequence_evidence() {
    for (parameters, flags) in [
        (
            "source:String,a:Boolean,b:Boolean,unused:Boolean",
            ["a", "b"],
        ),
        ("source:String,a:Boolean,b:Boolean", ["a", "a"]),
        ("source:String,a:Boolean,b:Boolean", ["(a)", "b"]),
    ] {
        check(parameters, flags, false, false, false);
    }
}

pub(super) fn check(
    parameters: &str,
    flags: [&str; 2],
    reverse: bool,
    suffix: bool,
    certified: bool,
) {
    let mut sources = SourceMap::new();
    let (provider_id, provider) = unit_lower_test_support::parsed(
        &mut sources,
        "flags-provider.ko",
        "fun view(source:String):borrow String from source=source\nfun next(source:String):borrow String from source=source\nfun consume(item:String):Unit {}\nfun other(item:String):Unit {}\nfun outer(item:String):Unit {}",
    );
    let tail = if suffix { "outer(source);" } else { "" };
    let text = format!(
        "fun run({parameters}):Unit {{outer(source);while({}){{borrow val x=view(source);consume(x);other(x)}};outer(source);while({}){{borrow val y=next(source);other(y)}};{tail}}}",
        flags[0], flags[1]
    );
    let (caller_id, caller) =
        unit_lower_test_support::parsed(&mut sources, "flags-caller.ko", &text);
    let mut inputs = [
        SourceUnitInput::new("root", "flags-provider.ko", provider_id, &provider),
        SourceUnitInput::new("root", "flags-caller.ko", caller_id, &caller),
    ];
    if reverse {
        inputs.reverse();
    }
    let (environment, types) = standard_environments();
    if flags[0] == "(a)" || parameters.split(',').any(|p| p.starts_with("own source:")) {
        use lang_frontend::{
            name_resolution::{index_compilation_unit, resolve_compilation_unit_names},
            ownership_checking::check_compilation_unit_ownership,
            type_checking::check_compilation_unit_types,
        };
        let ((diagnostics, audits), receipts) = fresh_loop_receipt::observe(|| {
            observe(|| {
                let index = index_compilation_unit(&sources, &inputs).unwrap();
                let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
                    .unwrap()
                    .validate()
                    .unwrap();
                let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
                    .unwrap()
                    .validate()
                    .unwrap();
                let rejected =
                    check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
                        .unwrap()
                        .validate()
                        .expect_err("Group/owned source retains original ownership rejection");
                rejected
                    .diagnostics()
                    .iter()
                    .map(|d| (format!("{:?}", d.code()), d.primary_span()))
                    .collect::<Vec<_>>()
            })
        });
        assert_eq!(diagnostics.len(), 2);
        let starts: Vec<_> = text.match_indices("borrow val").map(|(i, _)| i).collect();
        for ((code, span), start) in diagnostics.into_iter().zip(starts) {
            assert_eq!(code, "L0164");
            assert_eq!(span.source_id(), caller_id);
            assert_eq!((span.start(), span.end()), (start, start + 6));
        }
        assert!(audits.is_empty());
        assert!(receipts.is_empty());
        return;
    }
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
    assert_eq!(results.len(), 2);
    if !certified {
        assert!(
            results.iter().all(Result::is_ok),
            "domain-negative remains valid original lowering"
        );
        assert_eq!(audits.len(), 2);
        assert_eq!(receipts.len(), 2);
        for (result, receipt) in results.iter().zip(&receipts) {
            let (program, entry) = result.as_ref().unwrap();
            crate::ssa::verify::verify_program(program).unwrap();
            let function = program
                .modules
                .iter()
                .flat_map(|m| &m.functions)
                .find(|f| f.id == *entry)
                .unwrap();
            receipt.validate(function).unwrap();
        }
        for audit in &audits {
            assert!(
                audit
                    .unsupported
                    .iter()
                    .any(|g| g == "fresh no-carry sequential caller evidence gap")
            );
            for callee in ["view", "next"] {
                assert!(
                    !audit
                        .borrow_edges
                        .contains(&(declaration("run"), declaration(callee)))
                );
            }
        }
        return;
    }
    assert_eq!(audits.len(), 2);
    assert_eq!(receipts.len(), 2);
    let source_role = parameters
        .split(',')
        .position(|p| p.starts_with("source:"))
        .unwrap();
    let flag_roles = flags.map(|name| {
        parameters
            .split(',')
            .position(|p| {
                p.trim_start_matches("own ")
                    .starts_with(&format!("{name}:"))
            })
            .unwrap()
    });
    let modes: Vec<_> = parameters
        .split(',')
        .map(|p| p.starts_with("own "))
        .collect();
    assert_eq!(flag_roles[0] == flag_roles[1], modes.len() == 2);
    for ((result, audit), receipt) in results.into_iter().zip(audits).zip(receipts) {
        let (program, entry) = result.unwrap();
        crate::ssa::verify::verify_program(&program).unwrap();
        assert!(audit.unsupported.is_empty(), "{:?}", audit.unsupported);
        for callee in ["view", "next"] {
            assert!(
                audit
                    .borrow_edges
                    .contains(&(declaration("run"), declaration(callee)))
            );
        }
        let function = program
            .modules
            .iter()
            .flat_map(|m| &m.functions)
            .find(|f| f.id == entry)
            .unwrap();
        receipt.validate(function).unwrap();
        let entry = function.block(function.entry_block().unwrap()).unwrap();
        assert_eq!(entry.parameters.len(), modes.len());
        for (entity, owned) in entry.parameters.iter().zip(&modes) {
            assert_eq!(matches!(entity, EntityId::Value(_)), *owned);
            assert_eq!(matches!(entity, EntityId::Loan(_)), !*owned);
        }
        let mut roles: BTreeMap<_, _> = entry
            .parameters
            .iter()
            .copied()
            .enumerate()
            .map(|(i, e)| (e, i))
            .collect();
        for _ in 0..function.blocks.len() {
            for (_, edge) in &receipt.edges {
                for (argument, parameter) in edge
                    .arguments
                    .iter()
                    .zip(&function.block(edge.target).unwrap().parameters)
                {
                    if let Some(role) = roles.get(argument).copied()
                        && let Some(old) = roles.insert(*parameter, role)
                    {
                        assert_eq!(old, role);
                    }
                }
            }
        }
        for (i, local) in receipt.loops.iter().enumerate() {
            let result_id = local
                .instructions
                .iter()
                .find(|(tag, _)| *tag == Tag::Result)
                .unwrap()
                .1;
            let body = function.instruction(result_id).unwrap().block;
            let headers:Vec<_>=function.blocks.iter().filter(|block|matches!(&block.terminator.as_ref().unwrap().kind,TerminatorKind::Conditional{when_true,..} if when_true.target==body)).collect();
            let [header] = headers.as_slice() else {
                panic!("unique real loop header");
            };
            let TerminatorKind::Conditional {
                condition,
                when_false,
                ..
            } = &header.terminator.as_ref().unwrap().kind
            else {
                panic!("loop conditional");
            };
            if modes[flag_roles[i]] {
                assert!(
                    !local
                        .instructions
                        .iter()
                        .any(|(tag, _)| *tag == Tag::Condition)
                );
                assert_eq!(
                    roles.get(&EntityId::Value(*condition)),
                    Some(&flag_roles[i])
                );
            } else {
                let id = local
                    .instructions
                    .iter()
                    .find(|(tag, _)| *tag == Tag::Condition)
                    .unwrap()
                    .1;
                let read = function.instruction(id).unwrap();
                let Operation::Read {
                    source: PlaceAccess::Loan(flag),
                } = read.operation
                else {
                    panic!("borrowed flag Read");
                };
                assert_eq!(roles.get(&EntityId::Loan(flag)), Some(&flag_roles[i]));
                assert_eq!(read.block, header.id);
                assert_eq!(read.results.as_slice(), &[EntityId::Value(*condition)]);
            }
            let actual: BTreeSet<_> = when_false
                .arguments
                .iter()
                .map(|a| *roles.get(a).unwrap())
                .collect();
            let expected = if i == 0 {
                BTreeSet::from([source_role, flag_roles[1]])
            } else if suffix {
                BTreeSet::from([source_role])
            } else {
                BTreeSet::new()
            };
            assert_eq!(
                actual, expected,
                "zero-iteration exit transports only still-live original roles"
            );
            let fresh = local
                .instructions
                .iter()
                .find(|(tag, _)| *tag == Tag::Result)
                .unwrap()
                .1;
            for entity in &function.instruction(fresh).unwrap().results {
                assert!(!roles.contains_key(entity));
            }
        }
    }
}
