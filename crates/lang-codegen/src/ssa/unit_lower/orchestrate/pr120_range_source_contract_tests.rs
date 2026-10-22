//! CI-only whole-source contracts for source-call -> provider root handoff.
//! No observer, forged facts/SSA, native execution, or historical-error acceptance.
use std::collections::{BTreeMap, BTreeSet};

use lang_frontend::{
    analysis::{UnitSourceDescriptor, analyze_basic_unit_ownership, analyze_parsed_unit_names},
    lexer::lex,
    name_resolution::{SourceUnitId, SymbolKind, UnitReferenceTarget},
    ownership_checking::{
        BorrowBindingStorage, CompilationUnitOwnership, RangeUseSite, UnitDropFact, UnitDropPoint,
        UnitDropTarget, UnitIterationCleanupAction as Action, UnitIterationExitKind as Exit,
        UnitIterationOwnershipPlan, UnitLoanTarget, owned_compilation_unit_view,
    },
    parser::{ParsedFile, parse_file},
    source::SourceMap,
    type_checking::{
        check_compilation_unit_types, complete_compilation_unit_types, standard_environments,
    },
};

use crate::ssa::{
    model::{Definition, Edge, EntityId, Function, FunctionId, Operation, Program, TerminatorKind},
    unit_lower::lower_owned_unit_with_entry,
};

const STANDARD: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../lang-std/koven/algorithms/ranges.ko"
));

#[derive(Clone, Copy)]
enum Contract {
    Iteration,
    Binding(&'static str),
    OuterCall,
    Temporary,
    AllExits,
    Nested,
    SourceReturn,
    SourceAbort,
    PendingOuter,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum ExitTag {
    Fallthrough,
    Continue,
    Break,
    Return,
    Exhaustion,
}

fn exit_tag(exit: Exit) -> ExitTag {
    match exit {
        Exit::Fallthrough => ExitTag::Fallthrough,
        Exit::Continue(_) => ExitTag::Continue,
        Exit::Break(_) => ExitTag::Break,
        Exit::Return(_) => ExitTag::Return,
        Exit::Exhaustion => ExitTag::Exhaustion,
    }
}

fn exercise(path: &str, text: &str, contract: Contract) {
    for root in ["single", "src"] {
        let mut sources = SourceMap::new();
        let source = sources.add_source(path, text).unwrap();
        let standard = sources
            .add_source("koven-std/koven/algorithms/ranges.ko", STANDARD)
            .unwrap();
        let (name_environment, mut environment) = standard_environments();
        environment
            .authorize_range_source(&sources, standard)
            .unwrap();
        environment
            .authorize_range_extension_source(&sources, standard)
            .unwrap();
        let mut files = Vec::new();
        for descriptor in [
            UnitSourceDescriptor::new(root, path, source),
            UnitSourceDescriptor::new("koven-std", "koven/algorithms/ranges.ko", standard),
        ] {
            let lexed = lex(&sources, descriptor.source_id()).unwrap();
            assert!(
                lexed.diagnostics().is_empty(),
                "{path}/{root}: {:?}",
                lexed.diagnostics()
            );
            let parsed = parse_file(&sources, &lexed).unwrap();
            assert!(
                parsed.diagnostics().is_empty(),
                "{path}/{root}: {:?}",
                parsed.diagnostics()
            );
            files.push((descriptor, parsed));
        }
        let snapshot = analyze_parsed_unit_names(sources, files, name_environment).unwrap();
        assert!(
            snapshot.names().diagnostics().is_empty(),
            "{path}/{root}: {:?}",
            snapshot.names().diagnostics()
        );
        let names = snapshot.validated_names().unwrap();
        let sources = snapshot.sources();
        let inputs = snapshot.inputs();
        let raw = check_compilation_unit_types(sources, &inputs, names, &environment).unwrap();
        assert!(
            raw.diagnostics().is_empty(),
            "{path}/{root}: {:?}",
            raw.diagnostics()
        );
        let admission =
            complete_compilation_unit_types(sources, &inputs, names, &environment, &raw).unwrap();
        assert!(admission.is_same_analysis(&raw));
        assert!(std::ptr::eq(admission.types(), &raw));
        let (typed, owned) =
            analyze_basic_unit_ownership(sources, &inputs, names, &environment, raw)
                .unwrap()
                .into_result()
                .unwrap();
        assert!(
            owned.diagnostics().is_empty(),
            "{path}/{root}: {:?}",
            owned.diagnostics()
        );
        assert!(
            owned.deferred().is_empty(),
            "{path}/{root}: {:?}",
            owned.deferred()
        );
        assert!(owned.is_compatible_with(&typed));
        let unit = names
            .names()
            .index()
            .source_units()
            .iter()
            .find(|unit| unit.source_id() == source)
            .unwrap()
            .id();
        let local = names
            .names()
            .source_units()
            .iter()
            .find(|local| local.source_unit() == unit)
            .unwrap();
        let xs = local
            .resolution()
            .symbols()
            .iter()
            .filter(|symbol| symbol.name() == "xs")
            .map(|symbol| {
                let ids = names
                    .names()
                    .references()
                    .iter()
                    .filter(|reference| reference.source_unit() == unit)
                    .filter_map(|reference| match reference.target() {
                        UnitReferenceTarget::Symbol(id)
                            if id.source_unit() == unit && id.symbol() == symbol.id() =>
                        {
                            Some(*id)
                        }
                        _ => None,
                    })
                    .collect::<BTreeSet<_>>();
                assert_eq!(ids.len(), 1, "one actual unit-qualified xs identity");
                *ids.first().unwrap()
            })
            .collect::<Vec<_>>();
        let parsed = inputs
            .iter()
            .find(|input| input.source_id() == source)
            .unwrap()
            .parsed();
        let target = if matches!(contract, Contract::Temporary) {
            assert!(xs.is_empty());
            let uses = owned
                .borrow_results()
                .range_uses()
                .iter()
                .filter(|fact| fact.expression().source_unit() == unit)
                .collect::<Vec<_>>();
            let [fact] = uses.as_slice() else {
                panic!("one actual temporary source")
            };
            let UnitLoanTarget::Temporary(owner) = fact.origin() else {
                panic!("backing temporary, not View metadata")
            };
            assert!(
                sources
                    .slice(
                        parsed
                            .ast()
                            .expressions()
                            .get(owner.expression())
                            .unwrap()
                            .span()
                    )
                    .unwrap()
                    .starts_with("listOf(")
            );
            UnitDropTarget::Temporary(*owner)
        } else {
            let [xs] = xs.as_slice() else {
                panic!("unique actual xs binding")
            };
            UnitDropTarget::Named(*xs)
        };
        let root_drops = assert_owned(&owned, parsed, sources, unit, target, contract);
        let owned = owned.validate().unwrap();
        let view =
            owned_compilation_unit_view(sources, &inputs, names, &environment, &typed, &owned)
                .unwrap();
        assert!(std::ptr::eq(view.types(), typed.types()));
        assert!(std::ptr::eq(view.ownership(), owned.ownership()));
        let entries = names
            .names()
            .index()
            .declarations()
            .iter()
            .filter(|item| {
                item.source_unit() == unit
                    && item.kind() == SymbolKind::Function
                    && item.name() == "main"
            })
            .collect::<Vec<_>>();
        let [entry] = entries.as_slice() else {
            panic!("one original main")
        };
        let (program, entry) =
            lower_owned_unit_with_entry(&view, entry.id()).unwrap_or_else(|error| {
                panic!("{path}/{root}: ordinary verified lowering failed: {error:?}")
            });
        assert_ssa(&program, entry, sources, &root_drops, contract);
        // This performs the real SSA gate and LLVM module verification, in memory only.
        let llvm = crate::llvm::render_verified_program_with_entry(&program, entry)
            .unwrap_or_else(|error| panic!("{path}/{root}: verified LLVM failed: {error:?}"));
        assert!(!llvm.is_empty());
    }
}

fn target_of(origin: &UnitLoanTarget) -> UnitDropTarget {
    match origin {
        UnitLoanTarget::Place(place) => UnitDropTarget::Named(place.root()),
        UnitLoanTarget::Temporary(expression) => UnitDropTarget::Temporary(*expression),
        _ => panic!("fixture uses a real named or temporary backing root"),
    }
}

fn assert_owned(
    owned: &CompilationUnitOwnership,
    parsed: &ParsedFile,
    sources: &SourceMap,
    unit: SourceUnitId,
    target: UnitDropTarget,
    contract: Contract,
) -> Vec<UnitDropFact> {
    let drops = owned
        .drops()
        .iter()
        .copied()
        .filter(|fact| fact.target() == target)
        .collect::<Vec<_>>();
    assert!(
        !drops.is_empty(),
        "the backing owner obligation must survive source return"
    );
    let results = owned.borrow_results();
    let uses = results
        .range_uses()
        .iter()
        .filter(|fact| {
            fact.expression().source_unit() == unit && target_of(fact.origin()) == target
        })
        .collect::<Vec<_>>();
    for fact in &uses {
        assert!(
            drops
                .iter()
                .all(|drop| drop.point() != UnitDropPoint::CallReturn(fact.expression())),
            "a live descriptor cannot lose its backing owner at its own CallReturn"
        );
    }
    if let Contract::Binding(end_text) = contract {
        assert!(uses.is_empty());
        let bindings = results
            .bindings()
            .iter()
            .filter(|fact| target_of(fact.origin()) == target)
            .collect::<Vec<_>>();
        let [binding] = bindings.as_slice() else {
            panic!("one actual borrow val")
        };
        assert_eq!(binding.storage(), BorrowBindingStorage::NewRangeDescriptor);
        assert_eq!(binding.parent(), None);
        assert_eq!(binding.source_loan().unwrap().call(), binding.initializer());
        let ends = results
            .ends()
            .iter()
            .filter(|end| end.binding() == binding.binding())
            .collect::<Vec<_>>();
        let [end] = ends.as_slice() else {
            panic!("one actual result end")
        };
        let [drop] = drops.as_slice() else {
            panic!("one root Drop, not one for metadata")
        };
        assert_eq!(drop.point(), end.point());
        let UnitDropPoint::AfterStatement(statement) = end.point() else {
            panic!("unchanged explicit-binding end")
        };
        let span = parsed
            .ast()
            .statements()
            .get(statement.statement())
            .unwrap()
            .span();
        assert!(sources.slice(span).unwrap().starts_with(end_text));
        assert!(
            owned
                .drops()
                .iter()
                .all(|drop| drop.target() != UnitDropTarget::Named(binding.binding()))
        );
    } else if matches!(contract, Contract::OuterCall) {
        let [fact] = uses.as_slice() else {
            panic!("one short outer call")
        };
        let RangeUseSite::Call(outer) = fact.site() else {
            panic!("actual outer consumer")
        };
        let [drop] = drops.as_slice() else {
            panic!("unique outer-consumer Drop")
        };
        assert_eq!(drop.point(), UnitDropPoint::CallReturn(outer));
    } else if matches!(contract, Contract::PendingOuter) {
        let [fact] = uses.as_slice() else {
            panic!("one inner provider in the pending outer call")
        };
        assert_eq!(fact.site(), RangeUseSite::Iteration);
        let plan = owned
            .iterations()
            .iter()
            .find(|plan| plan.descriptor().source() == fact.expression())
            .unwrap();
        assert_exits(plan, target, false);
        let [drop] = drops.as_slice() else {
            panic!("one outer call still owns the cleanup obligation")
        };
        let UnitDropPoint::CallReturn(call) = drop.point() else {
            panic!("outer Borrow keeps xs past inner provider")
        };
        assert_ne!(call, fact.expression());
        let span = parsed
            .ast()
            .expressions()
            .get(call.expression())
            .unwrap()
            .span();
        assert!(sources.slice(span).unwrap().starts_with("keep(xs,"));
    } else {
        assert_eq!(
            uses.len(),
            if matches!(contract, Contract::Nested) {
                2
            } else {
                1
            }
        );
        let mut plans = uses
            .iter()
            .map(|fact| {
                assert_eq!(fact.site(), RangeUseSite::Iteration);
                let plan = owned
                    .iterations()
                    .iter()
                    .find(|plan| plan.descriptor().source() == fact.expression())
                    .unwrap();
                assert_eq!(plan.source(), Some(fact.origin()));
                plan
            })
            .collect::<Vec<_>>();
        plans.sort_by_key(|plan| {
            parsed
                .ast()
                .expressions()
                .get(plan.descriptor().source().expression())
                .unwrap()
                .span()
                .start()
        });
        let outer = plans[0];
        let expected = if matches!(contract, Contract::AllExits) {
            vec![
                ExitTag::Fallthrough,
                ExitTag::Continue,
                ExitTag::Break,
                ExitTag::Return,
                ExitTag::Exhaustion,
            ]
        } else {
            vec![ExitTag::Fallthrough, ExitTag::Exhaustion]
        };
        assert_eq!(
            outer
                .exits()
                .iter()
                .map(|exit| exit_tag(exit.kind()))
                .collect::<BTreeSet<_>>(),
            expected.into_iter().collect()
        );
        assert_exits(outer, target, true);
        if matches!(contract, Contract::Nested) {
            assert_exits(plans[1], target, false);
            assert!(
                plans[1]
                    .exits()
                    .iter()
                    .any(|exit| matches!(exit.kind(), Exit::Break(_)))
            );
        }
        let terminal = outer
            .exits()
            .iter()
            .filter(|exit| {
                matches!(
                    exit.kind(),
                    Exit::Break(_) | Exit::Return(_) | Exit::Exhaustion
                )
            })
            .collect::<Vec<_>>();
        let source_returns = drops
            .iter()
            .filter(|drop| !terminal.iter().any(|exit| exit.point() == drop.point()))
            .collect::<Vec<_>>();
        if matches!(contract, Contract::SourceReturn) {
            let [drop] = source_returns.as_slice() else {
                panic!("one source-evaluation return cleanup")
            };
            let UnitDropPoint::ControlTransfer(expression) = drop.point() else {
                panic!("source return, not scope extension")
            };
            let span = parsed
                .ast()
                .expressions()
                .get(expression.expression())
                .unwrap()
                .span();
            assert_eq!(sources.slice(span).unwrap(), "return");
            let source_span = parsed
                .ast()
                .expressions()
                .get(outer.descriptor().source().expression())
                .unwrap()
                .span();
            assert!(source_span.start() <= span.start() && span.end() <= source_span.end());
            assert!(
                owned.iteration_cleanup_at(drop.point()).is_none(),
                "provider not acquired on this path"
            );
        } else {
            assert!(
                source_returns.is_empty(),
                "no unrelated scope-end or call-return root Drop"
            );
        }
        assert_eq!(
            drops.len(),
            terminal.len() + usize::from(matches!(contract, Contract::SourceReturn))
        );
    }
    drops
}

fn assert_exits(
    plan: &UnitIterationOwnershipPlan,
    target: UnitDropTarget,
    owns_last_dependency: bool,
) {
    let statement = plan.descriptor().statement();
    for exit in plan.exits() {
        match exit.kind() {
            Exit::Exhaustion => assert_eq!(exit.point(), UnitDropPoint::LoopExit(statement)),
            Exit::Continue(expression) | Exit::Break(expression) | Exit::Return(expression) => {
                assert_eq!(exit.point(), UnitDropPoint::ControlTransfer(expression));
            }
            Exit::Fallthrough => assert!(matches!(exit.point(), UnitDropPoint::AfterStatement(_))),
        }
        let actions = exit.actions();
        let drops = actions
            .iter()
            .enumerate()
            .filter_map(|(index, action)| match action {
                Action::Drop(fact) if fact.target() == target => Some((index, fact)),
                _ => None,
            })
            .collect::<Vec<_>>();
        let finishes = matches!(
            exit.kind(),
            Exit::Break(_) | Exit::Return(_) | Exit::Exhaustion
        );
        if finishes {
            let provider = actions
                .iter()
                .position(|action| *action == Action::FinishProvider(statement))
                .unwrap();
            let source = actions
                .iter()
                .position(|action| *action == Action::EndSource(statement))
                .unwrap();
            assert!(provider < source);
            if owns_last_dependency {
                let [(drop_index, drop)] = drops.as_slice() else {
                    panic!("exactly one root Drop on this terminal edge")
                };
                assert!(
                    source < *drop_index,
                    "root after its real provider/source end"
                );
                assert_eq!(drop.point(), exit.point());
            } else {
                assert!(
                    drops.is_empty(),
                    "outer provider still protects the same root"
                );
            }
        } else {
            assert!(
                drops.is_empty(),
                "fallthrough/continue keeps provider and root alive"
            );
            assert!(!actions.contains(&Action::FinishProvider(statement)));
            assert!(!actions.contains(&Action::EndSource(statement)));
        }
        if !matches!(exit.kind(), Exit::Exhaustion) {
            let element = actions
                .iter()
                .position(|action| *action == Action::EndElement(statement))
                .unwrap();
            assert!(actions[..element].iter().any(|action| matches!(action, Action::EndBinding { statement: owner, .. } if *owner == statement)));
        }
    }
}

fn edges(terminator: &TerminatorKind) -> Vec<&Edge> {
    match terminator {
        TerminatorKind::Branch(edge) => vec![edge],
        TerminatorKind::Conditional {
            when_true,
            when_false,
            ..
        } => vec![when_true, when_false],
        TerminatorKind::NullableBranch {
            when_null,
            when_non_null,
            ..
        } => vec![when_null, when_non_null],
        _ => Vec::new(),
    }
}

// Least fixed point of actual definition roots, propagated only toward phi results.
// A same-typed other incoming root or an ungrounded incoming SCC is never an alias.
fn aliases(function: &Function, seed: EntityId) -> BTreeSet<EntityId> {
    let entities = function
        .blocks
        .iter()
        .flat_map(|block| &block.parameters)
        .chain(
            function
                .instructions
                .iter()
                .flat_map(|instruction| &instruction.results),
        )
        .copied()
        .collect::<BTreeSet<_>>();
    let incoming = function
        .blocks
        .iter()
        .flat_map(|block| edges(&block.terminator.as_ref().unwrap().kind))
        .collect::<Vec<_>>();
    let mut dependencies = BTreeMap::new();
    let mut origins = BTreeMap::new();
    for entity in entities {
        let inputs = match function.entity(entity).unwrap().definition {
            Definition::BlockParameter { block, index } => {
                assert_eq!(function.block(block).unwrap().parameters[index], entity);
                (Some(block) != function.entry_block()).then(|| {
                    incoming
                        .iter()
                        .filter(|edge| edge.target == block)
                        .map(|edge| edge.arguments[index])
                        .collect::<Vec<_>>()
                })
            }
            Definition::InstructionResult { instruction, index } => {
                let instruction = function.instruction(instruction).unwrap();
                assert_eq!(instruction.results[index], entity);
                match instruction.operation {
                    Operation::Copy { source } => Some(vec![EntityId::Value(source)]),
                    _ => None,
                }
            }
        };
        if let Some(inputs) = inputs {
            dependencies.insert(entity, inputs);
            origins.insert(entity, BTreeSet::new());
        } else {
            origins.insert(entity, BTreeSet::from([entity]));
        }
    }
    loop {
        let mut changed = false;
        for (&entity, inputs) in &dependencies {
            let roots = inputs
                .iter()
                .flat_map(|input| origins[input].iter().copied())
                .collect::<BTreeSet<_>>();
            let current = origins.get_mut(&entity).unwrap();
            let before = current.len();
            current.extend(roots);
            changed |= current.len() != before;
        }
        if !changed {
            break;
        }
    }
    let expected = &origins[&seed];
    assert_eq!(
        expected.len(),
        1,
        "seed must reach exactly one real definition root"
    );
    let root = *expected.first().unwrap();
    let result = origins
        .iter()
        .filter_map(|(&entity, roots)| roots.contains(&root).then_some(entity))
        .collect::<BTreeSet<_>>();
    for &entity in &result {
        assert_eq!(
            &origins[&entity], expected,
            "related phi cannot merge another root"
        );
        if let Some(inputs) = dependencies.get(&entity) {
            assert!(!inputs.is_empty(), "no unanchored related transport");
            for input in inputs {
                assert_eq!(
                    &origins[input], expected,
                    "every incoming must be grounded in the same root"
                );
            }
        }
        assert_eq!(
            function.entity(entity).unwrap().ty,
            function.entity(seed).unwrap().ty
        );
    }
    result
}

fn borrowed_loans(function: &Function, owners: &BTreeSet<EntityId>) -> BTreeSet<EntityId> {
    let places = function
        .instructions
        .iter()
        .filter_map(|instruction| match instruction.operation {
            Operation::RootPlace { owner } if owners.contains(&EntityId::Value(owner)) => {
                Some(instruction.results[0])
            }
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    function
        .instructions
        .iter()
        .filter_map(|instruction| match instruction.operation {
            Operation::BorrowBegin { place, .. } if places.contains(&EntityId::Place(place)) => {
                Some(instruction.results[0])
            }
            _ => None,
        })
        .flat_map(|loan| aliases(function, loan))
        .collect()
}

fn assert_ssa(
    program: &Program,
    entry: FunctionId,
    sources: &SourceMap,
    drops: &[UnitDropFact],
    contract: Contract,
) {
    let module = program.module(entry.module()).unwrap();
    let function = module.function(entry).unwrap();
    let constructors = function
        .instructions
        .iter()
        .filter(|instruction| {
            matches!(instruction.operation, Operation::ContainerConstruct { .. })
                && sources
                    .slice(instruction.origin.span())
                    .unwrap()
                    .starts_with("listOf(")
        })
        .collect::<Vec<_>>();
    let [constructor] = constructors.as_slice() else {
        panic!("one actual backing List construction")
    };
    let root = aliases(function, constructor.results[0]);
    let root_loans = borrowed_loans(function, &root);
    let calls = function
        .instructions
        .iter()
        .filter_map(|instruction| match instruction.operation {
            Operation::RangeCall { source, .. } => {
                assert!(
                    root_loans.contains(&EntityId::Loan(source)),
                    "RangeCall borrows the real backing owner"
                );
                let [EntityId::Value(view), EntityId::Loan(protecting)] =
                    instruction.results.as_slice()
                else {
                    panic!("real V/P pair")
                };
                let views = aliases(function, EntityId::Value(*view));
                let protecting = aliases(function, EntityId::Loan(*protecting));
                let source = aliases(function, EntityId::Loan(source));
                let metadata = borrowed_loans(function, &views);
                assert!(!metadata.is_empty(), "consumer borrows descriptor metadata");
                Some((views, protecting, source, metadata))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(!calls.is_empty());
    let root_drops = function.instructions.iter().filter(|instruction| {
        matches!(instruction.operation, Operation::Drop { owner } if root.contains(&EntityId::Value(owner)))
    }).collect::<Vec<_>>();
    assert_eq!(
        root_drops.len(),
        drops.len(),
        "every published owner Drop lowers exactly once"
    );
    for drop in root_drops {
        assert!(
            drops
                .iter()
                .any(|fact| fact.value_origin() == drop.origin.span()),
            "actual owner origin, not metadata"
        );
        let block = function.block(drop.block).unwrap();
        let at = block
            .instructions
            .iter()
            .position(|&id| id == drop.id)
            .unwrap();
        let prefix = block.instructions[..at]
            .iter()
            .map(|&id| function.instruction(id).unwrap())
            .collect::<Vec<_>>();
        let matching = prefix
            .iter()
            .enumerate()
            .filter_map(|(index, instruction)| {
                let Operation::RangeEnd { view, source } = instruction.operation else {
                    return None;
                };
                calls
                    .iter()
                    .find(|(views, protecting, _, _)| {
                        views.contains(&EntityId::Value(view))
                            && protecting.contains(&EntityId::Loan(source))
                    })
                    .map(|call| (index, call))
            })
            .collect::<Vec<_>>();
        if matching.is_empty() && matches!(contract, Contract::PendingOuter) {
            assert!(prefix.iter().any(|instruction| matches!(&instruction.operation, Operation::DirectCall { arguments, .. } if arguments.iter().any(|argument| root_loans.contains(argument)))));
            assert!(prefix.iter().any(|instruction| matches!(instruction.operation, Operation::BorrowEnd { loan } if root_loans.contains(&EntityId::Loan(loan)))));
        } else if matching.is_empty() {
            assert!(matches!(contract, Contract::SourceReturn));
            assert!(matches!(
                block.terminator.as_ref().unwrap().kind,
                TerminatorKind::Return { .. }
            ));
            assert_eq!(
                sources
                    .slice(block.terminator.as_ref().unwrap().origin.span())
                    .unwrap(),
                "return"
            );
            assert!(prefix.iter().any(|instruction| matches!(instruction.operation, Operation::BorrowEnd { loan } if root_loans.contains(&EntityId::Loan(loan)))));
        } else {
            for (ended, (_, _, source, metadata)) in matching {
                assert!(prefix[..ended].iter().any(|instruction| matches!(instruction.operation, Operation::BorrowEnd { loan } if metadata.contains(&EntityId::Loan(loan)))), "M ends before range.end(V,P)");
                assert!(prefix[ended + 1..].iter().any(|instruction| matches!(instruction.operation, Operation::BorrowEnd { loan } if source.contains(&EntityId::Loan(loan)))), "continued A ends after V/P and before R Drop");
            }
        }
    }
    if matches!(contract, Contract::SourceAbort) {
        let aborts = function
            .blocks
            .iter()
            .filter(|block| {
                matches!(
                    block.terminator.as_ref().unwrap().kind,
                    TerminatorKind::Abort
                )
            })
            .collect::<Vec<_>>();
        assert!(!aborts.is_empty());
        for block in aborts {
            assert!(block.instructions.iter().all(|&id| !matches!(function.instruction(id).unwrap().operation, Operation::Drop { owner } if root.contains(&EntityId::Value(owner)))), "abort is not an invented unwind/normal provider exit");
        }
    }
    for function in &module.functions {
        for place in function.instructions.iter().filter(|instruction| {
            matches!(instruction.operation, Operation::RangeElementPlace { .. })
        }) {
            let loans = function.instructions.iter().filter(|instruction| matches!(instruction.operation, Operation::BorrowBegin { place: source, .. } if place.results == [EntityId::Place(source)])).collect::<Vec<_>>();
            assert_eq!(
                loans.len(),
                1,
                "the actual element place establishes one loan"
            );
            for loan in loans {
                let element = aliases(function, loan.results[0]);
                assert!(function.instructions.iter().any(|instruction| matches!(instruction.operation, Operation::BorrowEnd { loan } if element.contains(&EntityId::Loan(loan)))), "actual per-iteration element loan must end");
            }
        }
    }
}

macro_rules! original {
    ($test:ident, $file:literal, $contract:expr) => {
        #[test]
        fn $test() {
            exercise(
                $file,
                include_str!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/tests/fixtures/pr120-verifier-diagnostics/",
                    $file
                )),
                $contract,
            );
        }
    };
}

original!(
    pr120_y01_range_root_contract,
    "y01_import_take_for_sum.ko",
    Contract::Iteration
);
original!(
    pr120_y03_receiver_range_root_contract,
    "y03_method_take_for.ko",
    Contract::Iteration
);
original!(
    pr120_y04_string_range_root_contract,
    "y04_take_for_println.ko",
    Contract::Iteration
);
original!(
    pr120_y05_borrow_binding_call_contract,
    "y05_take_borrow_val_call.ko",
    Contract::Binding("read(v)")
);
original!(
    pr120_y06_borrow_binding_for_contract,
    "y06_take_borrow_val_for.ko",
    Contract::Binding("{ val xs =")
);

#[test]
fn pr120_range_source_all_exit_contracts() {
    exercise(
        "all-exits.ko",
        r#"import koven.algorithms.take
fun main(): Unit {
    val xs = listOf(1, 2, 3, 4)
    for (x in take(xs, 4)) {
        if (x == 1) { continue }
        if (x == 2) { break }
        if (x == 3) { return }
        println("body")
    }
}
"#,
        Contract::AllExits,
    );
}

#[test]
fn pr120_range_source_outer_borrow_contract() {
    exercise(
        "outer-borrow.ko",
        r#"import koven.algorithms.take
fun read(view: View<String>): Unit { for (s in view) { println(s) } }
fun main(): Unit { val xs = listOf("a", "b"); read(take(xs, 2)) }
"#,
        Contract::OuterCall,
    );
}

#[test]
fn pr120_range_source_temporary_backing_contract() {
    exercise(
        "temporary-backing.ko",
        r#"import koven.algorithms.take
fun main(): Unit { for (s in take(listOf("a", "b"), 2)) { println(s) } }
"#,
        Contract::Temporary,
    );
}

#[test]
fn pr120_range_source_nested_same_root_contract() {
    exercise(
        "nested-root.ko",
        r#"import koven.algorithms.take
fun main(): Unit {
    val xs = listOf("a", "b")
    for (a in take(xs, 2)) {
        while (true) { break }
        for (b in take(xs, 1)) { println(b); break }
        println(a)
    }
}
"#,
        Contract::Nested,
    );
}

#[test]
fn pr120_range_source_unacquired_return_contract() {
    exercise(
        "source-return.ko",
        r#"import koven.algorithms.take
fun main(): Unit {
    val xs = listOf("a", "b")
    for (s in take(xs, if (true) { return } else { 2 })) { println(s) }
}
"#,
        Contract::SourceReturn,
    );
}

#[test]
fn pr120_range_source_zero_count_contract() {
    exercise(
        "zero-count.ko",
        r#"import koven.algorithms.take
fun main(): Unit { val xs = listOf("a", "b"); for (s in take(xs, 0)) { println(s) } }
"#,
        Contract::Iteration,
    );
}

#[test]
fn pr120_range_source_unacquired_abort_contract() {
    exercise(
        "source-abort.ko",
        r#"import koven.algorithms.take
fun main(): Unit {
    val xs = listOf("a", "b")
    for (s in take(xs, if (true) { error("stop") } else { 2 })) { println(s) }
}
"#,
        Contract::SourceAbort,
    );
}

#[test]
fn pr120_range_source_preserves_outer_pending_root_contract() {
    exercise(
        "pending-outer.ko",
        r#"import koven.algorithms.take
fun keep(source: List<Int>, number: Int): Unit { }
fun main(): Unit {
    val xs = listOf(1, 2)
    keep(xs, if (true) { for (x in take(xs, 1)) { println("inner") }; 0 } else { 0 })
}
"#,
        Contract::PendingOuter,
    );
}
