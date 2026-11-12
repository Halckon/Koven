//! SPEC-0182: local source loan identity follows body CFG edges and merges.
use crate::test_support::diagnostics::diagnostic;
use std::collections::BTreeSet;

use super::cleanup_tests::{cleanup, loan, transport, value};
use super::{
    Definition, EntityId, Function, Instruction, LoanKind, Operation, SourceProgram,
    TerminatorKind, analyze, defining_instruction, incoming_edges,
};

fn verified(source: &str) -> SourceProgram {
    let analysis = analyze(source, "scan");
    crate::llvm::render_verified_program(&analysis.program).expect("source CFG must lower to LLVM");
    analysis
}

fn only_instruction(function: &Function, predicate: impl Fn(&Operation) -> bool) -> &Instruction {
    let instructions = function
        .instructions
        .iter()
        .filter(|instruction| predicate(&instruction.operation))
        .collect::<Vec<_>>();
    assert_eq!(
        instructions.len(),
        1,
        "expected one instruction: {instructions:?}"
    );
    instructions[0]
}

fn rooted_transport(function: &Function, root: EntityId) -> BTreeSet<EntityId> {
    function.entity(root).expect("known transport root");
    let mut rooted = BTreeSet::from([root]);
    loop {
        let mut changed = false;
        for block in &function.blocks {
            for (_, edge) in incoming_edges(function, block.id) {
                assert_eq!(edge.arguments.len(), block.parameters.len());
                for (argument, parameter) in edge.arguments.iter().zip(&block.parameters) {
                    if rooted.contains(argument) {
                        changed |= rooted.insert(*parameter);
                    }
                }
            }
        }
        if !changed {
            break;
        }
    }
    // Only exact edge slots preserve identity. Instructions, including SharedReborrow,
    // create distinct roots; a mixed-root merge or an unrooted cycle is not an alias.
    for &entity in &rooted {
        if entity == root {
            continue;
        }
        let Definition::BlockParameter { block, index } = function
            .entity(entity)
            .expect("known transported entity")
            .definition
        else {
            panic!("transport must produce a block parameter: {entity:?}");
        };
        let block = function.block(block).expect("known transport target");
        assert_eq!(block.parameters.get(index), Some(&entity));
        let incoming = incoming_edges(function, block.id);
        assert!(!incoming.is_empty(), "transport must have an incoming edge");
        for (_, edge) in incoming {
            assert_eq!(edge.arguments.len(), block.parameters.len());
            assert!(
                rooted.contains(&edge.arguments[index]),
                "mixed transport roots at {entity:?}: {edge:?}"
            );
        }
    }
    rooted
}

#[test]
fn conditional_break_ends_rebound_source_before_temporary_drop() {
    for constructor in ["listOf", "arrayOf", "mutableListOf"] {
        let text = format!(
            "fun scan(): Unit {{ for (value in {constructor}(1)) {{ if (value == 1) {{ break }} }} }}"
        );
        let analysis = verified(&text);
        let function = analysis.entry();
        let element = function
            .instructions
            .iter()
            .find(|instruction| {
                matches!(
                    instruction.operation,
                    Operation::ContainerElementPlace { .. }
                )
            })
            .unwrap();
        let Operation::ContainerElementPlace {
            owner: body_source, ..
        } = element.operation
        else {
            unreachable!();
        };
        let body = function.block(element.block).unwrap();
        let element_loan = function
            .instructions
            .iter()
            .find_map(|instruction| {
                matches!(instruction.operation,
                    Operation::BorrowBegin { place, kind: LoanKind::Shared }
                    if EntityId::Place(place) == element.results[0]
                )
                .then_some(instruction.results.first().copied())
                .flatten()
            })
            .unwrap();
        let length = only_instruction(function, |operation| {
            matches!(operation, Operation::ContainerLength { .. })
        });
        let Operation::ContainerLength { owner: source } = length.operation else {
            unreachable!();
        };
        let source_borrow = defining_instruction(function, source);
        let Operation::BorrowBegin {
            place,
            kind: LoanKind::Shared,
        } = source_borrow.operation
        else {
            panic!("temporary source shared loan");
        };
        assert_eq!(source_borrow.results, [source]);
        let source_place = defining_instruction(function, EntityId::Place(place));
        let Operation::RootPlace { owner } = source_place.operation else {
            panic!("temporary source root owner");
        };
        assert_eq!(source_place.results, [EntityId::Place(place)]);
        let source_aliases = rooted_transport(function, source);
        let owner_aliases = rooted_transport(function, EntityId::Value(owner));
        assert!(source_aliases.contains(&body_source));
        let body_owners = body
            .parameters
            .iter()
            .copied()
            .filter(|entity| owner_aliases.contains(entity))
            .collect::<Vec<_>>();
        assert_eq!(body_owners.len(), 1, "one transported temporary owner");
        let body_owner = body_owners[0];
        let TerminatorKind::Conditional {
            when_true,
            when_false,
            ..
        } = &body.terminator.as_ref().unwrap().kind
        else {
            panic!("conditional break body");
        };
        let break_block = function.block(when_true.target).unwrap();
        assert_eq!(
            cleanup(function, break_block),
            [
                Operation::BorrowEnd {
                    loan: loan(transport(function, when_true, element_loan))
                },
                Operation::BorrowEnd {
                    loan: loan(transport(function, when_true, body_source))
                },
                Operation::Drop {
                    owner: value(transport(function, when_true, body_owner))
                },
            ]
        );
        let normal_block = function.block(when_false.target).unwrap();
        assert_eq!(
            cleanup(function, normal_block),
            [Operation::BorrowEnd {
                loan: loan(transport(function, when_false, element_loan))
            },]
        );
        let TerminatorKind::Branch(backedge) = &normal_block.terminator.as_ref().unwrap().kind
        else {
            panic!("normal sibling returns to header");
        };
        assert!(
            backedge
                .arguments
                .contains(&transport(function, when_false, body_source))
        );
        assert!(
            backedge
                .arguments
                .contains(&transport(function, when_false, body_owner))
        );
        assert!(matches!(
            defining_instruction(function, element_loan).operation,
            Operation::BorrowBegin { .. }
        ));
    }
}

#[test]
fn source_cleanup_rebinds_sibling_single_and_multiple_exit_cfg() {
    for body in [
        "if (flag) { break } else { break }",
        "if (flag) { break } else { continue }",
        "if (flag) { if (value == 1) { break } } else { continue }",
        "if (flag) { println(\"left\") } else { println(\"right\") }; break",
        "if (flag) { continue }; if (value == 1) { break }",
        "if (flag) { println(\"normal\") } else { break }; break",
        "if (flag) { return value } else { return 0 }",
        "for (inner in arrayOf(3, 4)) { if (flag) { break } }; if (flag) { break }",
        "for (inner in arrayOf(3, 4)) { if (flag) { return inner } }; break",
    ] {
        diagnostic!("source cleanup CFG: {body}");
        verified(&format!(
            "fun scan(flag: Boolean): Int {{ for (value in arrayOf(1, 2)) {{ {body} }}; return 0 }}"
        ));
    }
    // Preserve the full original determinism failure as well as its reduced CFG cases.
    diagnostic!("original determinism red input: factory List + continue + break + total");
    verified(
        r#"
        fun source(): List<Int> = listOf(7, 2, 9)
        fun scan(): Int {
            var total = 0
            for (value in source()) {
                if (value == 2) { continue }
                if (value == 9) { break }
                total = total + value
            }
            return total
        }
        "#,
    );
}

#[test]
fn borrowed_source_cfg_does_not_end_the_callers_source_loan() {
    let analysis = verified(
        "fun scan(xs: List<Int>, flag: Boolean): Unit { for (value in xs) { if (flag) { break } else { continue } } }",
    );
    let function = analysis.entry();
    let preheader = function.block(function.entry_block().unwrap()).unwrap();
    let caller = preheader.parameters[0];
    assert_eq!(
        function.entity(caller).unwrap().definition,
        Definition::BlockParameter {
            block: preheader.id,
            index: 0,
        }
    );
    let length = only_instruction(function, |operation| {
        matches!(operation, Operation::ContainerLength { .. })
    });
    let Operation::ContainerLength { owner: source } = length.operation else {
        unreachable!();
    };
    let reborrow = only_instruction(function, |operation| {
        matches!(operation, Operation::SharedReborrow { .. })
    });
    assert_eq!(reborrow, defining_instruction(function, source));
    assert_eq!(reborrow.results, [source]);
    assert_eq!(
        reborrow.operation,
        Operation::SharedReborrow {
            source: loan(caller)
        }
    );
    assert_eq!(reborrow.block, preheader.id);
    assert_eq!(length.block, preheader.id);
    assert_ne!(source, caller);
    let caller_aliases = rooted_transport(function, caller);
    let source_aliases = rooted_transport(function, source);
    assert!(caller_aliases.is_disjoint(&source_aliases));
    for instruction in &function.instructions {
        if let Operation::BorrowEnd { loan: ended } = instruction.operation {
            assert!(
                !caller_aliases.contains(&EntityId::Loan(ended)),
                "caller loan must remain live: {instruction:?}"
            );
        }
    }

    let TerminatorKind::Branch(enter) = &preheader.terminator.as_ref().unwrap().kind else {
        panic!("enter provider header");
    };
    let header = function.block(enter.target).unwrap();
    let header_source = transport(function, enter, source);
    let TerminatorKind::Conditional {
        when_true: body_edge,
        when_false: exhausted_edge,
        ..
    } = &header.terminator.as_ref().unwrap().kind
    else {
        panic!("provider guard");
    };
    let body = function.block(body_edge.target).unwrap();
    let exhausted = function.block(exhausted_edge.target).unwrap();
    let body_source = transport(function, body_edge, header_source);
    let element = only_instruction(function, |operation| {
        matches!(operation, Operation::ContainerElementPlace { .. })
    });
    assert_eq!(element.block, body.id);
    assert!(matches!(
        element.operation,
        Operation::ContainerElementPlace { owner, .. } if owner == body_source
    ));
    let element_borrow = only_instruction(function, |operation| {
        matches!(operation,
            Operation::BorrowBegin { place, kind: LoanKind::Shared }
                if EntityId::Place(*place) == element.results[0]
        )
    });
    assert_eq!(element_borrow.block, body.id);
    assert_eq!(element_borrow.results.len(), 1);
    let element_loan = element_borrow.results[0];
    assert_eq!(
        cleanup(function, exhausted),
        [Operation::BorrowEnd {
            loan: loan(transport(function, exhausted_edge, header_source))
        }]
    );
    let TerminatorKind::Conditional {
        when_true: break_edge,
        when_false: continue_edge,
        ..
    } = &body.terminator.as_ref().unwrap().kind
    else {
        panic!("break/continue body");
    };
    let break_block = function.block(break_edge.target).unwrap();
    let continue_block = function.block(continue_edge.target).unwrap();
    assert_eq!(
        cleanup(function, break_block),
        [
            Operation::BorrowEnd {
                loan: loan(transport(function, break_edge, element_loan))
            },
            Operation::BorrowEnd {
                loan: loan(transport(function, break_edge, body_source))
            },
        ]
    );
    assert_eq!(
        cleanup(function, continue_block),
        [Operation::BorrowEnd {
            loan: loan(transport(function, continue_edge, element_loan))
        }]
    );
    let continued_source = transport(function, continue_edge, body_source);
    let TerminatorKind::Branch(backedge) = &continue_block.terminator.as_ref().unwrap().kind else {
        panic!("continue returns to provider header");
    };
    assert_eq!(backedge.target, header.id);
    assert_eq!(
        transport(function, backedge, continued_source),
        header_source
    );
    for block in &function.blocks {
        if ![exhausted.id, break_block.id, continue_block.id].contains(&block.id) {
            assert!(
                cleanup(function, block).is_empty(),
                "unexpected cleanup in {block:?}"
            );
        }
    }
}

pub(in crate::ssa) mod fixture_6 {
//! 普通 fixture、真实 AST 位置和阶段化结果；无生产 facts 构造器。

use crate::ssa::{
    LoweringError,
    model::{FunctionId, Origin, Program},
    verify::verify_program,
};
use lang_frontend::{
    ast::{ExpressionId, StatementId},
    diagnostic::Diagnostic,
    lexer::lex,
    name_resolution::{SourceUnitInput, SourceUnitKey, ValidatedCompilationUnitNames},
    parser::{ForBinding, Item, NameMarker, ParsedFile, Statement, parse_file},
    source::{SourceMap, Span},
    type_checking::{IntrinsicTypeConstructor, MapIterationKind},
};

pub(in crate::ssa) const FOR_TEXT: &str =
    "for ((key, value) in source) { val copied: Int = key; println(value) }";

#[derive(Clone, Copy)]
pub(in crate::ssa) struct SourceCase {
    pub path: &'static str,
    pub text: &'static str,
    pub constructor: Option<(IntrinsicTypeConstructor, MapIterationKind)>,
}

#[derive(Debug)]
pub(in crate::ssa) struct Stopped {
    pub stage: &'static str,
    pub original: String,
}

pub(in crate::ssa) fn stopped(stage: &'static str, error: impl std::fmt::Debug) -> Stopped {
    Stopped {
        stage,
        original: format!("{error:?}"),
    }
}

pub(in crate::ssa) fn clean(
    sources: &SourceMap,
    stage: &'static str,
    diagnostics: &[Diagnostic],
) -> Result<(), Stopped> {
    if diagnostics.is_empty() {
        Ok(())
    } else {
        Err(stopped(
            stage,
            diagnostics
                .iter()
                .map(|item| {
                    (
                        item,
                        sources.source_name(item.primary_span().source_id()),
                        sources.slice(item.primary_span()),
                    )
                })
                .collect::<Vec<_>>(),
        ))
    }
}

pub(in crate::ssa) struct File {
    pub path: String,
    pub parsed: ParsedFile,
    pub constructor: Option<(IntrinsicTypeConstructor, MapIterationKind)>,
}

pub(in crate::ssa) struct Fixture {
    pub sources: SourceMap,
    pub files: Vec<File>,
}

impl Fixture {
    pub fn new(cases: &[SourceCase]) -> Result<Self, Stopped> {
        let mut fixture = Self {
            sources: SourceMap::new(),
            files: Vec::new(),
        };
        for case in cases {
            fixture.add(case.path, case.text, case.constructor)?;
        }
        Ok(fixture)
    }

    pub fn single_whole(text: &str) -> Result<Self, Stopped> {
        let mut fixture = Self {
            sources: SourceMap::new(),
            files: Vec::new(),
        };
        fixture.add("whole.ko", text, None)?;
        Ok(fixture)
    }

    pub fn inputs(
        &self,
        pathway: crate::ssa::map_iteration_stage_gate_tests::Pathway,
        reverse_inputs: bool,
    ) -> Result<Vec<SourceUnitInput<'_>>, Stopped> {
        if matches!(pathway, crate::ssa::map_iteration_stage_gate_tests::Pathway::Single) {
            let [file] = self.files.as_slice() else {
                return Err(stopped("single unit input", "expected one original source"));
            };
            return Ok(vec![SourceUnitInput::memory_document(
                file.parsed.source_id(),
                &file.parsed,
            )]);
        }
        let mut inputs = self
            .files
            .iter()
            .map(|file| {
                SourceUnitInput::new("root", &file.path, file.parsed.source_id(), &file.parsed)
            })
            .collect::<Vec<_>>();
        if reverse_inputs {
            inputs.reverse();
        }
        Ok(inputs)
    }

    pub fn check_input_identity(
        &self,
        pathway: crate::ssa::map_iteration_stage_gate_tests::Pathway,
        names: &ValidatedCompilationUnitNames,
        report: &mut crate::ssa::map_iteration_stage_gate_tests::Report,
    ) {
        let units = names.names().index().source_units();
        let memory = matches!(pathway, crate::ssa::map_iteration_stage_gate_tests::Pathway::Single);
        report.check(
            "unit original input kind and complete source identity",
            units.len() == self.files.len()
                && (!memory || units.len() == 1)
                && self.files.iter().all(|file| {
                    file.parsed.package().is_none()
                        && units
                            .iter()
                            .filter(|unit| {
                                unit.source_id() == file.parsed.source_id()
                                    && if memory {
                                        unit.key() == &SourceUnitKey::MemoryDocument
                                    } else {
                                        unit.key()
                                            .root()
                                            .is_some_and(|root| root.as_str() == "root")
                                            && unit.key().logical_path().is_some_and(|path| {
                                                path.as_str() == file.path.as_str()
                                            })
                                    }
                            })
                            .count()
                            == 1
                }),
            units,
        );
    }

    pub fn function_location(&self, name_span: Span) -> Option<Location> {
        let spans = self
            .files
            .iter()
            .filter(|file| file.parsed.source_id() == name_span.source_id())
            .flat_map(|file| file.parsed.ast().items().iter())
            .filter_map(|(_, node)| {
                matches!(node.payload(), Item::Function { name: NameMarker::Present(name), .. } if *name == name_span)
                    .then_some(node.span())
            })
            .collect::<Vec<_>>();
        let [span] = spans.as_slice() else {
            return None;
        };
        location(&self.sources, *span)
    }

    fn add(
        &mut self,
        path: &str,
        text: &str,
        constructor: Option<(IntrinsicTypeConstructor, MapIterationKind)>,
    ) -> Result<(), Stopped> {
        let source = self
            .sources
            .add_source(path, text)
            .map_err(|error| stopped("source infrastructure", error))?;
        let lexed =
            lex(&self.sources, source).map_err(|error| stopped("lex infrastructure", error))?;
        clean(&self.sources, "lex diagnostics", lexed.diagnostics())?;
        let parsed = parse_file(&self.sources, &lexed)
            .map_err(|error| stopped("parse infrastructure", error))?;
        clean(&self.sources, "parse diagnostics", parsed.diagnostics())?;
        self.files.push(File {
            path: path.to_owned(),
            parsed,
            constructor,
        });
        Ok(())
    }
}

#[derive(Debug)]
pub(in crate::ssa) struct LoopSite {
    pub statement: StatementId,
    pub span: Span,
    pub source: ExpressionId,
    pub source_span: Span,
    pub bindings: [Span; 2],
}

pub(in crate::ssa) fn loop_sites(file: &File) -> Result<Vec<LoopSite>, Stopped> {
    let ast = file.parsed.ast();
    let mut sites = Vec::new();
    for (statement, node) in ast.statements().iter() {
        let Statement::For {
            binding, source, ..
        } = node.payload()
        else {
            continue;
        };
        let ForBinding::Destructuring { names, .. } = binding else {
            return Err(stopped("AST query", "For requires destructuring"));
        };
        let [NameMarker::Present(key), NameMarker::Present(value)] = names.as_slice() else {
            return Err(stopped("AST query", "For requires two real bindings"));
        };
        let source_span = ast
            .expressions()
            .get(*source)
            .map_err(|error| stopped("AST query", error))?
            .span();
        sites.push(LoopSite {
            statement,
            span: node.span(),
            source: *source,
            source_span,
            bindings: [*key, *value],
        });
    }
    Ok(sites)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::ssa) struct Location {
    pub span: Span,
    pub path: String,
    pub text: String,
}

pub(in crate::ssa) fn location(sources: &SourceMap, span: Span) -> Option<Location> {
    Some(Location {
        span,
        path: sources.source_name(span.source_id()).ok()?.to_owned(),
        text: sources.slice(span).ok()?.to_owned(),
    })
}

/// 每条原始观察现在只接受真实 SSA/LLVM 成功；前置 query 失败仍独立失败。
#[derive(Debug)]
pub(in crate::ssa) struct LowerOutcome {
    error: Option<LoweringError>,
    error_location: Option<Location>,
    success: Option<SuccessfulLower>,
}

#[derive(Debug)]
struct SuccessfulLower {
    entry: FunctionId,
    expected_entry: Option<Location>,
    actual_entry: Option<Location>,
    entry_identity: Option<FunctionId>,
    ssa_verifier: Result<(), String>,
    verified_llvm_bytes: Result<usize, String>,
}

impl LowerOutcome {
    pub fn new(
        sources: &SourceMap,
        expected_entry: Option<Location>,
        result: Result<(Program, FunctionId), LoweringError>,
    ) -> Self {
        let (error, success) = match result {
            Err(error) => (Some(error), None),
            Ok((program, entry)) => {
                // 使用 lower 返回的真实 ID 查询本 Program，不按函数名或固定槽索引替代。
                let function = program
                    .module(entry.module())
                    .and_then(|module| module.function(entry));
                let actual_entry = function.and_then(|function| match &function.origin {
                    Origin::Source(span) => location(sources, *span),
                    Origin::Synthetic { .. } => None,
                });
                let entry_identity = function
                    .filter(|_| expected_entry.is_some() && actual_entry == expected_entry)
                    .map(|function| function.id());
                // 来源 query 即使失败仍执行两条真实 verifier 路径；LLVM 入口会再次验证 SSA。
                let ssa_verifier = verify_program(&program).map_err(|error| format!("{error:?}"));
                let verified_llvm_bytes = crate::llvm::render_verified_program(&program)
                    .map(|llvm| llvm.len())
                    .map_err(|error| format!("{error:?}"));
                (
                    None,
                    Some(SuccessfulLower {
                        entry,
                        expected_entry,
                        actual_entry,
                        entry_identity,
                        ssa_verifier,
                        verified_llvm_bytes,
                    }),
                )
            }
        };
        let error_location = error
            .as_ref()
            .and_then(|error| error.span)
            .and_then(|span| location(sources, span));
        Self {
            error,
            error_location,
            success,
        }
    }

    pub fn matches(&self) -> bool {
        self.error.is_none()
            && self.error_location.is_none()
            && self.success.as_ref().is_some_and(|success| {
                // 同一分析的真实返回 ID 与源身份；无跨分析裸 ID 比较。
                success
                    .actual_entry
                    .as_ref()
                    .is_some_and(|actual| Some(actual) == success.expected_entry.as_ref())
                    && success.entry_identity == Some(success.entry)
                    && success.ssa_verifier.is_ok()
                    && success
                        .verified_llvm_bytes
                        .as_ref()
                        .is_ok_and(|bytes| *bytes > 0)
            })
    }
}
}
