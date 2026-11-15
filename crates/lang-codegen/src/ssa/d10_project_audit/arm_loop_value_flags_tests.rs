//! Real ordinary-arm Boolean mode/role transport; no synthetic receipt or IR mutation.
use super::*;
use crate::ssa::{
    model::{EntityId, PlaceAccess},
    refine::arm_loop_receipt,
    unit_lower, unit_lower_test_support,
};
use lang_frontend::{
    name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
};
#[test]
fn ordinary_arm_value_flags_keep_original_abi_and_conditional_roles() {
    let names = ["source", "flag", "choose"];
    for order in [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ] {
        for owned in [[false, true], [true, false], [true, true]] {
            let parameters = order
                .map(|i| {
                    format!(
                        "{}{}:{}",
                        if i > 0 && owned[i - 1] { "own " } else { "" },
                        names[i],
                        if i == 0 { "String" } else { "Boolean" }
                    )
                })
                .join(",");
            for swapped in [false, true] {
                for reverse in [false, true] {
                    let flags = if swapped {
                        ["choose", "flag"]
                    } else {
                        ["flag", "choose"]
                    };
                    check(&parameters, flags, reverse, true);
                }
            }
        }
    }
}
#[test]
fn ordinary_arm_value_flags_do_not_ignore_extra_or_repeated_roles() {
    for (parameters, flags) in [
        (
            "source:String,own flag:Boolean,own choose:Boolean,extra:Boolean",
            ["flag", "choose"],
        ),
        (
            "source:String,own flag:Boolean,own choose:Boolean",
            ["flag", "flag"],
        ),
    ] {
        check(parameters, flags, false, false);
    }
}
fn check(parameters: &str, flags: [&str; 2], reverse: bool, certified: bool) {
    let mut sources = SourceMap::new();
    let (p, provider) = unit_lower_test_support::parsed(
        &mut sources,
        "arm-value-provider.ko",
        "fun view(source:String):borrow String from source=source\nfun next(source:String):borrow String from source=source\nfun consume(item:String):Unit {}\nfun other(item:String):Unit {}",
    );
    let text = format!(
        "fun run({parameters}):Unit {{while({}){{if({}){{borrow val a=view(source);consume(a)}}else{{borrow val b=next(source);other(b);consume(b)}}}}}}",
        flags[0], flags[1]
    );
    let (c, caller) = unit_lower_test_support::parsed(&mut sources, "arm-value-caller.ko", &text);
    let mut inputs = [
        SourceUnitInput::new("root", "arm-value-provider.ko", p, &provider),
        SourceUnitInput::new("root", "arm-value-caller.ko", c, &caller),
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
    let modes: Vec<_> = parameters
        .split(',')
        .map(|p| p.starts_with("own "))
        .collect();
    let flag_roles = flags.map(|name| {
        parameters
            .split(',')
            .position(|p| {
                p.trim_start_matches("own ")
                    .starts_with(&format!("{name}:"))
            })
            .unwrap()
    });
    for _ in 0..2 {
        let ((result, audits), receipts, _) = arm_loop_receipt::observe(|| {
            observe(|| {
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
        });
        let (program, entry) = result.expect("real original ordinary mode route");
        crate::ssa::verify::verify_program(&program).unwrap();
        assert_eq!(audits.len(), 1);
        assert_eq!(receipts.len(), 1);
        let function = program
            .modules
            .iter()
            .flat_map(|m| &m.functions)
            .find(|f| f.id == entry)
            .unwrap();
        receipts[0].validate(function).unwrap();
        for name in ["view", "next"] {
            assert_eq!(
                audits[0]
                    .borrow_edges
                    .contains(&(declaration("run"), declaration(name))),
                certified
            );
        }
        if !certified {
            assert!(
                audits[0]
                    .unsupported
                    .iter()
                    .any(|g| g == "ordinary arm-loop full-caller evidence gap")
            );
            continue;
        }
        assert!(
            !audits[0]
                .unsupported
                .iter()
                .any(|g| g.starts_with("ordinary arm-loop")
                    || g.starts_with("unclassified operation: BorrowCall"))
        );
        let entry = function.block(function.entry_block().unwrap()).unwrap();
        assert_eq!(entry.parameters.len(), 3);
        for (entity, mode) in entry.parameters.iter().zip(&modes) {
            assert_eq!(matches!(entity, EntityId::Value(_)), *mode);
            assert_eq!(matches!(entity, EntityId::Loan(_)), !*mode);
        }
        let mut roles: BTreeMap<_, _> = entry
            .parameters
            .iter()
            .copied()
            .enumerate()
            .map(|(i, e)| (e, i))
            .collect();
        for _ in 0..function.blocks.len() {
            for block in &function.blocks {
                let edges = match &block.terminator.as_ref().unwrap().kind {
                    TerminatorKind::Branch(e) => vec![e],
                    TerminatorKind::Conditional {
                        when_true,
                        when_false,
                        ..
                    } => vec![when_true, when_false],
                    _ => Vec::new(),
                };
                for edge in edges {
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
        }
        let mut seen = BTreeSet::new();
        for block in &function.blocks {
            let TerminatorKind::Conditional {
                condition,
                when_false,
                ..
            } = &block.terminator.as_ref().unwrap().kind
            else {
                continue;
            };
            let outer = matches!(
                function
                    .block(when_false.target)
                    .unwrap()
                    .terminator
                    .as_ref()
                    .unwrap()
                    .kind,
                TerminatorKind::Return { .. }
            );
            let role = flag_roles[usize::from(!outer)];
            seen.insert(role);
            if modes[role] {
                assert!(block.instructions.is_empty());
                assert_eq!(roles.get(&EntityId::Value(*condition)), Some(&role));
            } else {
                assert_eq!(block.instructions.len(), 1);
                let read = function.instruction(block.instructions[0]).unwrap();
                let Operation::Read {
                    source: PlaceAccess::Loan(flag),
                } = read.operation
                else {
                    panic!("original borrowed condition read");
                };
                assert_eq!(roles.get(&EntityId::Loan(flag)), Some(&role));
                assert_eq!(read.results, [EntityId::Value(*condition)]);
            }
            if outer {
                assert!(
                    when_false.arguments.is_empty(),
                    "zero iteration exits without live ABI roles"
                );
            }
        }
        assert_eq!(seen, flag_roles.into_iter().collect());
        let results: Vec<_> = function
            .instructions
            .iter()
            .filter(|i| matches!(i.operation, Operation::BorrowCall { .. }))
            .map(|i| i.results[0])
            .collect();
        assert_eq!(results.len(), 2);
        assert_ne!(results[0], results[1]);
        for result in results {
            assert!(!roles.contains_key(&result));
            assert_eq!(function.instructions.iter().filter(|i|matches!(i.operation,Operation::BorrowEnd{loan} if EntityId::Loan(loan)==result)).count(),1);
        }
    }
}

#[test]
fn ordinary_arm_value_flags_keep_original_owned_source_and_group_rejections() {
    use lang_frontend::{
        name_resolution::{index_compilation_unit, resolve_compilation_unit_names},
        ownership_checking::check_compilation_unit_ownership,
        type_checking::check_compilation_unit_types,
    };
    for (parameters, condition) in [
        (
            "own source:String,own flag:Boolean,own choose:Boolean",
            "flag",
        ),
        (
            "source:String,own flag:Boolean,own choose:Boolean",
            "(flag)",
        ),
    ] {
        let mut sources = SourceMap::new();
        let (p, provider) = unit_lower_test_support::parsed(
            &mut sources,
            "arm-mode-boundary-provider.ko",
            "fun view(source:String):borrow String from source=source\nfun next(source:String):borrow String from source=source\nfun consume(item:String):Unit {}\nfun other(item:String):Unit {}",
        );
        let text = format!(
            "fun run({parameters}):Unit {{while({condition}){{if(choose){{borrow val a=view(source);consume(a)}}else{{borrow val b=next(source);other(b);consume(b)}}}}}}"
        );
        let (c, caller) =
            unit_lower_test_support::parsed(&mut sources, "arm-mode-boundary-caller.ko", &text);
        let inputs = [
            SourceUnitInput::new("root", "arm-mode-boundary-provider.ko", p, &provider),
            SourceUnitInput::new("root", "arm-mode-boundary-caller.ko", c, &caller),
        ];
        let (environment, types) = standard_environments();
        let ((diagnostics, audits), receipts, _) = arm_loop_receipt::observe(|| {
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
                        .expect_err("original unsupported ownership domain");
                rejected
                    .diagnostics()
                    .iter()
                    .map(|d| (format!("{:?}", d.code()), d.primary_span()))
                    .collect::<Vec<_>>()
            })
        });
        assert_eq!(diagnostics.len(), 2);
        for ((code, span), (start, _)) in diagnostics
            .into_iter()
            .zip(text.match_indices("borrow val"))
        {
            assert_eq!(code, "L0164");
            assert_eq!(
                (span.source_id(), span.start(), span.end()),
                (c, start, start + 6)
            );
        }
        assert!(audits.is_empty());
        assert!(receipts.is_empty());
    }
}
