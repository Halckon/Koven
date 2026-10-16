//! Read-only Phase 3 observation, deliberately stopping before either lowering entrance.
use std::collections::BTreeSet;

use lang_frontend::{
    ast::ExpressionId,
    lexer::lex,
    name_resolution::{
        ReferenceTarget, SourceUnitInput, UnitReferenceTarget, index_compilation_unit,
        resolve_compilation_unit_names, resolve_names,
    },
    ownership_checking::{
        CleanupCondition, CleanupConditionId, CleanupConditions, DropTarget, IterationOwnershipPlan,
        UnitDropTarget, UnitIterationOwnershipPlan, check_compilation_unit_ownership,
        check_ownership,
    },
    parser::{Expression, ParsedFile, parse_file},
    source::{SourceMap, Span},
    type_checking::{
        TypeKind, UnitExpressionId, UnitTypeKind, check_compilation_unit_types, check_types,
        complete_compilation_unit_types, complete_file_types, standard_environments,
    },
};

#[derive(Debug)]
struct Observation {
    nullable_current_storage: bool,
    current_drops: usize,
    // UnitDropFact has no public condition/owner fields or cleanup-condition table.
    conditional_current_drops: Option<usize>,
}

#[test]
fn nullable_if_retest_cleanup_facts_are_observed_before_lowering() {
    let source = super::retested_after_branch_source();
    let grouped = source.replace("if (current != null)", "if (((current) != null))");
    let mut outcomes = Vec::new();
    for (case, source) in [source, grouped].iter().enumerate() {
        for unit in [false, true] {
            println!("IF CLEANUP BEGIN case={case} unit={unit}");
            let outcome = observe_source(source, unit);
            println!("IF CLEANUP OUTCOME case={case} unit={unit} {outcome:?}");
            outcomes.push(outcome);
        }
    }
    // All four ordinary-source entrances have been observed before any oracle.
    // Exact owner versions, selectors and DropPoints await the first real run.
    assert_eq!(outcomes.len(), 4);
    let observations = outcomes
        .into_iter()
        .enumerate()
        .map(|(index, outcome)| {
            outcome.unwrap_or_else(|error| {
                panic!(
                    "ordinary frontend prerequisites failed: case={} unit={}: {error}",
                    index / 2,
                    index % 2 == 1
                )
            })
        })
        .collect::<Vec<_>>();
    for (index, observation) in observations.into_iter().enumerate() {
        assert!(observation.nullable_current_storage, "{observation:?}");
        assert!(observation.current_drops > 0, "{observation:?}");
        if index % 2 == 0 {
            assert!(
                observation
                    .conditional_current_drops
                    .is_some_and(|count| count > 0),
                "single-file public facts must contain a conditional current drop: {observation:?}"
            );
        } else {
            assert_eq!(observation.conditional_current_drops, None);
        }
    }
}

fn observe_source(text: &str, unit: bool) -> Result<Observation, String> {
    let text = if unit {
        format!("package p\n{text}")
    } else {
        text.to_owned()
    };
    let mut sources = SourceMap::new();
    let path = if unit { "p/range.ko" } else { "range.ko" };
    let source = sources
        .add_source(path, &text)
        .map_err(|error| format!("source: {error:?}"))?;
    println!("IF CLEANUP SOURCE unit={unit} id={source:?} path={path} {text:?}");
    let lexed = lex(&sources, source).map_err(|error| format!("lex API: {error:?}"))?;
    if !lexed.diagnostics().is_empty() {
        return Err(format!("lex diagnostics: {:?}", lexed.diagnostics()));
    }
    let file = parse_file(&sources, &lexed).map_err(|error| format!("parse API: {error:?}"))?;
    if !file.diagnostics().is_empty() {
        return Err(format!("parse diagnostics: {:?}", file.diagnostics()));
    }
    for (expression, node) in file.ast().expressions().iter() {
        println!(
            "IF CLEANUP AST EXPRESSION id={expression:?} span={:?} payload={:?}",
            node.span(),
            node.payload()
        );
    }
    for (statement, node) in file.ast().statements().iter() {
        println!(
            "IF CLEANUP AST STATEMENT id={statement:?} span={:?} payload={:?}",
            node.span(),
            node.payload()
        );
    }
    let (environment, types) = standard_environments();
    if unit {
        // Same ordinary unit analysis chain as integer_range_value_tests::lower_source.
        let inputs = [SourceUnitInput::new("root", path, source, &file)];
        let index = index_compilation_unit(&sources, &inputs)
            .map_err(|error| format!("index API: {error:?}"))?;
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
            .map_err(|error| format!("names API: {error:?}"))?
            .validate()
            .map_err(|error| format!("names diagnostics: {error:?}"))?;
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
            .map_err(|error| format!("types API: {error:?}"))?;
        complete_compilation_unit_types(&sources, &inputs, &names, &types, &typed)
            .map_err(|error| format!("complete: {error:?}"))?;
        let typed = typed
            .validate()
            .map_err(|error| format!("types validation: {error:?}"))?;
        let owned = check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
            .map_err(|error| format!("ownership API: {error:?}"))?;
        println!(
            "IF CLEANUP UNIT SYMBOLS {:#?}",
            names.names().source_units()
        );
        println!(
            "IF CLEANUP UNIT REFERENCES {:#?}",
            names.names().references()
        );
        println!("IF CLEANUP UNIT TYPE TABLE {:#?}", typed.types().types());
        println!("CLEANUP UNIT ITERATION PLANS {:#?}", owned.iterations());
        print_unit_iterations(owned.iterations());
        println!(
            "IF CLEANUP UNIT STORAGE {:#?}",
            typed.types().body_symbol_types()
        );
        println!("IF CLEANUP UNIT DROPS {:#?}", owned.drops());
        println!(
            "IF CLEANUP UNIT PUBLIC CAPABILITY: UnitDropFact exposes point, target and \
             value_origin; owner/version and condition DAG are not published"
        );
        for (index, fact) in owned.drops().iter().enumerate() {
            let storage = match fact.target() {
                UnitDropTarget::Named(symbol) => typed.types().symbol_type(symbol),
                _ => None,
            };
            println!(
                "IF CLEANUP UNIT DROP index={index} point={:?} target/root={:?} \
                 storage={storage:?} kind={:?} value_origin={:?} full={fact:?}",
                fact.point(),
                fact.target(),
                storage.and_then(|ty| typed.types().types().get(ty)),
                fact.value_origin(),
            );
        }
        let owned = owned
            .validate()
            .map_err(|error| format!("ownership diagnostics: {error:?}"))?;
        let (_, current_span) = current_name(&file, &text)?;
        let current = names
            .names()
            .references()
            .iter()
            .find_map(|reference| match reference.target() {
                UnitReferenceTarget::Symbol(symbol) if reference.span() == current_span => {
                    Some(*symbol)
                }
                _ => None,
            })
            .ok_or_else(|| "current Name has no resolved unit symbol".to_owned())?;
        let storage = typed.types().symbol_type(current);
        let storage_kind = storage.and_then(|ty| typed.types().types().get(ty));
        println!(
            "IF CLEANUP UNIT CURRENT symbol={current:?} name_span={current_span:?} \
             storage={storage:?} kind={storage_kind:?}"
        );
        for (expression, node) in file.ast().expressions().iter() {
            if matches!(node.payload(), Expression::Name)
                && names.names().references().iter().any(|reference| {
                    reference.span() == node.span()
                        && reference.target() == &UnitReferenceTarget::Symbol(current)
                })
            {
                let expression = UnitExpressionId::new(current.source_unit(), expression);
                let ty = typed.types().expression_type(expression);
                println!(
                    "IF CLEANUP UNIT CURRENT NAME expression={expression:?} span={:?} \
                     type={ty:?} kind={:?}",
                    node.span(),
                    ty.and_then(|ty| typed.types().types().get(ty)),
                );
            }
        }
        let current_drops = owned
            .ownership()
            .drops()
            .iter()
            .filter(|fact| fact.target() == UnitDropTarget::Named(current))
            .collect::<Vec<_>>();
        println!("IF CLEANUP UNIT CURRENT DROPS symbol={current:?} {current_drops:#?}");
        Ok(Observation {
            nullable_current_storage: matches!(storage_kind, Some(UnitTypeKind::Nullable(_))),
            current_drops: current_drops.len(),
            conditional_current_drops: None,
        })
    } else {
        // Same ordinary file analysis chain, with errors retained instead of early assertions.
        let names = resolve_names(&sources, &file, &environment)
            .map_err(|error| format!("names API: {error:?}"))?;
        if !names.diagnostics().is_empty() {
            return Err(format!("names diagnostics: {:?}", names.diagnostics()));
        }
        let typed = check_types(&sources, &file, &names, &types)
            .map_err(|error| format!("types API: {error:?}"))?;
        complete_file_types(&sources, &file, &names, &types, &typed)
            .map_err(|error| format!("complete: {error:?}"))?;
        let owned = check_ownership(&sources, &file, &names, &typed)
            .map_err(|error| format!("ownership API: {error:?}"))?;
        println!("IF CLEANUP SINGLE SYMBOLS {:#?}", names.symbols());
        println!("IF CLEANUP SINGLE REFERENCES {:#?}", names.references());
        println!("IF CLEANUP SINGLE TYPE TABLE {:#?}", typed.types());
        for symbol in names.symbols() {
            let storage = typed.symbol_type(symbol.id());
            println!(
                "IF CLEANUP SINGLE STORAGE symbol={:?} type={storage:?} kind={:?}",
                symbol.id(),
                storage.and_then(|ty| typed.types().get(ty)),
            );
        }
        println!("IF CLEANUP SINGLE DROPS {:#?}", owned.drops());
        println!("CLEANUP SINGLE ITERATION PLANS {:#?}", owned.iterations());
        let table = owned.cleanup_conditions();
        // Debug retains the complete published table, including every owner definition.
        println!("IF CLEANUP SINGLE CONDITION TABLE {table:#?}");
        println!("IF CLEANUP SINGLE CONDITION NODES {:#?}", table.nodes());
        println!("IF CLEANUP SINGLE SELECTORS {:#?}", table.selectors());
        print_single_iterations(owned.iterations(), table);
        for (index, fact) in owned.drops().iter().enumerate() {
            println!(
                "IF CLEANUP SINGLE DROP index={index} point={:?} target/root={:?} \
                 owner/version={:?} owner_node={:?} condition={:?} value_origin={:?} full={fact:?}",
                fact.point(),
                fact.target(),
                fact.owner(),
                fact.owner().and_then(|owner| table.owner_value(owner)),
                fact.condition(),
                fact.value_origin(),
            );
            if let Some(condition) = fact.condition() {
                print_condition(table, condition, &mut BTreeSet::new());
            }
        }
        if !owned.diagnostics().is_empty() || !owned.deferred().is_empty() {
            return Err(format!(
                "ownership: {:?}, deferred: {:?}",
                owned.diagnostics(),
                owned.deferred()
            ));
        }
        let (_, current_span) = current_name(&file, &text)?;
        let current = names
            .references()
            .iter()
            .find_map(|reference| match reference.target() {
                ReferenceTarget::Symbol(symbol) if reference.span() == current_span => {
                    Some(*symbol)
                }
                _ => None,
            })
            .ok_or_else(|| "current Name has no resolved file symbol".to_owned())?;
        let storage = typed.symbol_type(current);
        let storage_kind = storage.and_then(|ty| typed.types().get(ty));
        println!(
            "IF CLEANUP SINGLE CURRENT symbol={current:?} name_span={current_span:?} \
             storage={storage:?} kind={storage_kind:?}"
        );
        for (expression, node) in file.ast().expressions().iter() {
            if matches!(node.payload(), Expression::Name)
                && names.references().iter().any(|reference| {
                    reference.span() == node.span()
                        && reference.target() == &ReferenceTarget::Symbol(current)
                })
            {
                let ty = typed.expression_type(expression);
                println!(
                    "IF CLEANUP SINGLE CURRENT NAME expression={expression:?} span={:?} \
                     type={ty:?} kind={:?}",
                    node.span(),
                    ty.and_then(|ty| typed.types().get(ty)),
                );
            }
        }
        let current_drops = owned
            .drops()
            .iter()
            .filter(|fact| fact.target() == DropTarget::Named(current))
            .collect::<Vec<_>>();
        println!("IF CLEANUP SINGLE CURRENT DROPS symbol={current:?} {current_drops:#?}");
        let conditional_current_drops = current_drops
            .iter()
            .filter(|fact| {
                matches!(
                    fact.condition().and_then(|condition| table.get(condition)),
                    Some(CleanupCondition::Choice { .. })
                )
            })
            .count();
        Ok(Observation {
            nullable_current_storage: matches!(storage_kind, Some(TypeKind::Nullable(_))),
            current_drops: current_drops.len(),
            conditional_current_drops: Some(conditional_current_drops),
        })
    }
}

fn current_name(file: &ParsedFile, text: &str) -> Result<(ExpressionId, Span), String> {
    // Text locates the unchanged fixture guard only. Identity and type come from the
    // actual Name AST node, its resolved symbol and that symbol's typed storage.
    let start = text
        .find("while (current != null)")
        .ok_or_else(|| "fixture has no original while guard".to_owned())?
        + "while (".len();
    file.ast()
        .expressions()
        .iter()
        .find(|(_, node)| {
            matches!(node.payload(), Expression::Name)
                && node.span().start() == start
                && node.span().end() == start + "current".len()
        })
        .map(|(expression, node)| (expression, node.span()))
        .ok_or_else(|| "fixture guard has no actual current Name node".to_owned())
}

fn print_condition(
    table: &CleanupConditions,
    condition: CleanupConditionId,
    seen: &mut BTreeSet<CleanupConditionId>,
) {
    if !seen.insert(condition) {
        println!("IF CLEANUP DAG REFERENCE condition={condition:?}");
        return;
    }
    let node = table.get(condition);
    println!("IF CLEANUP DAG condition={condition:?} node={node:?}");
    if let Some(CleanupCondition::Choice { selector, branches }) = node {
        println!(
            "IF CLEANUP DAG SELECTOR condition={condition:?} selector={selector:?} node={:?}",
            table.selector(*selector)
        );
        for (branch, child) in branches.iter().copied().enumerate() {
            println!("IF CLEANUP DAG EDGE condition={condition:?} branch={branch} child={child:?}");
            print_condition(table, child, seen);
        }
    }
}

#[test]
fn nullable_numeric_for_cleanup_facts_are_observed_before_lowering() {
    let source = super::numeric_nested_source();
    let grouped = source.replace("in 0..<2", "in ((0..<2))");
    let mut outcomes = Vec::new();
    for (case, source) in [source, grouped].iter().enumerate() {
        for unit in [false, true] {
            println!("NUMERIC CLEANUP BEGIN case={case} unit={unit}");
            let outcome = observe_source(source, unit);
            println!("NUMERIC CLEANUP OUTCOME case={case} unit={unit} {outcome:?}");
            outcomes.push(outcome);
        }
    }
    assert_eq!(outcomes.len(), 4);
    for outcome in outcomes {
        let observation = outcome.expect("ordinary numeric frontend prerequisites succeed");
        assert!(observation.nullable_current_storage, "{observation:?}");
        assert!(observation.current_drops > 0, "{observation:?}");
    }
}

#[test]
fn nullable_borrowed_for_cleanup_facts_are_observed_before_lowering() {
    let relations = [
        super::borrowed_proof_cases::borrowed_for_inside_while_sources(),
        super::borrowed_proof_cases::while_inside_borrowed_for_sources(),
    ];
    let mut outcomes = Vec::new();
    for (case, source) in relations.iter().flatten().enumerate() {
        for unit in [false, true] {
            println!("BORROWED CLEANUP BEGIN case={case} unit={unit}");
            let outcome = observe_source(source, unit);
            println!("BORROWED CLEANUP OUTCOME case={case} unit={unit} {outcome:?}");
            outcomes.push(outcome);
        }
    }
    // Observe both relations, changing only the for source Group, before any oracle.
    assert_eq!(outcomes.len(), 8);
    for outcome in outcomes {
        let observation = outcome.expect("ordinary borrowed frontend prerequisites succeed");
        assert!(observation.nullable_current_storage, "{observation:?}");
        assert!(observation.current_drops > 0, "{observation:?}");
    }
}

fn print_unit_iterations(plans: &[UnitIterationOwnershipPlan]) {
    println!(
        "CLEANUP UNIT ITERATION PUBLIC CAPABILITY: provider/source/root/element delivery, \
         bindings and ordered exits/actions are published; Header/Exit owner phis, \
         incoming conditions, exit conditions and their condition DAG are not published"
    );
    for plan in plans {
        let descriptor = plan.descriptor();
        let statement = descriptor.statement();
        println!(
            "CLEANUP UNIT ITERATION statement={statement:?} provider={:?} source={:?} \
             source_type={:?} source_kind={:?} source/root={:?} source_access={:?} \
             element_type={:?} element_delivery={:?} element_binding={:?}",
            descriptor.provider(),
            descriptor.source(),
            descriptor.source_type(),
            plan.source_kind(),
            plan.source(),
            plan.source_access(),
            descriptor.element_type(),
            descriptor.delivery(),
            descriptor.binding(),
        );
        for binding in plan.bindings() {
            println!(
                "CLEANUP UNIT ELEMENT statement={statement:?} symbol={:?} mode={:?}",
                binding.symbol(),
                binding.kind(),
            );
        }
        for exit in plan.exits() {
            println!(
                "CLEANUP UNIT EXIT statement={statement:?} kind={:?} point={:?} actions={:#?}",
                exit.kind(),
                exit.point(),
                exit.actions(),
            );
        }
    }
}

fn print_single_iterations(plans: &[IterationOwnershipPlan], table: &CleanupConditions) {
    for plan in plans {
        let descriptor = plan.descriptor();
        let statement = descriptor.statement();
        println!(
            "CLEANUP SINGLE ITERATION statement={statement:?} provider={:?} source={:?} \
             source_type={:?} source_kind={:?} source/root={:?} element_type={:?} \
             element_delivery={:?} element_binding={:?}",
            descriptor.provider(),
            descriptor.source(),
            descriptor.source_type(),
            plan.source_kind(),
            plan.source(),
            descriptor.element_type(),
            descriptor.delivery(),
            descriptor.binding(),
        );
        for binding in plan.bindings() {
            println!(
                "CLEANUP SINGLE ELEMENT statement={statement:?} symbol={:?} mode={:?}",
                binding.symbol(),
                binding.kind(),
            );
        }
        println!(
            "CLEANUP SINGLE PHIS statement={statement:?} count={} incomings={}",
            plan.closure_phis().len(),
            plan.closure_phi_incomings().len(),
        );
        for phi in plan.closure_phis() {
            println!(
                "CLEANUP SINGLE PHI statement={statement:?} boundary={:?} root={:?} \
                 owner/version={:?} owner_node={:?} selector={:?} selector_node={:?} \
                 condition={:?} full={phi:#?}",
                phi.boundary(),
                phi.symbol(),
                phi.owner(),
                table.owner_value(phi.owner()),
                phi.availability_selector(),
                table.selector(phi.availability_selector()),
                phi.availability_condition(),
            );
            print_condition(table, phi.availability_condition(), &mut BTreeSet::new());
        }
        for incoming in plan.closure_phi_incomings() {
            println!(
                "CLEANUP SINGLE INCOMING statement={statement:?} boundary={:?} kind={:?} \
                 point={:?} condition={:?} bindings={:#?}",
                incoming.boundary(),
                incoming.kind(),
                incoming.point(),
                incoming.condition(),
                incoming.bindings(),
            );
            print_condition(table, incoming.condition(), &mut BTreeSet::new());
            for binding in incoming.bindings() {
                println!(
                    "CLEANUP SINGLE INCOMING BINDING target={:?} owner_node={:?} \
                     available_when={:?}",
                    binding.target(),
                    table.owner_value(binding.target()),
                    binding.available_when(),
                );
                print_condition(table, binding.available_when(), &mut BTreeSet::new());
                for value in binding.values() {
                    println!(
                        "CLEANUP SINGLE INCOMING VALUE source={:?} owner_node={:?} condition={:?}",
                        value.source(),
                        table.owner_value(value.source()),
                        value.condition(),
                    );
                    print_condition(table, value.condition(), &mut BTreeSet::new());
                }
            }
        }
        for exit in plan.exits() {
            println!(
                "CLEANUP SINGLE EXIT statement={statement:?} kind={:?} point={:?} \
                 condition={:?} actions={:#?}",
                exit.kind(),
                exit.point(),
                exit.condition(),
                exit.actions(),
            );
            print_condition(table, exit.condition(), &mut BTreeSet::new());
        }
    }
}
