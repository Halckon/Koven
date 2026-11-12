//! Newly rebuilt finite Map<String, Resource> normal-exhaustion pipelines.
//! Resource values remain container-owned; the loop creates Shared component loans.
use crate::test_support::diagnostics::{assert_diagnostic, diagnostic};
pub(in crate::ssa) mod continuation;
pub(super) mod continued {
    //! Resource continue retains provider/source and lexical owner across every backedge.
    use super::continuation::{Case, sources as continuations};
    use crate::test_support::diagnostics::assert_diagnostic;
    pub(crate) fn sources() -> Vec<Case> {
        let originals = continuations();
        let mut cases = vec![Case {
            label: "original-continue",
            owned: true,
            source: include_str!(
                "../../../../tests/fixtures/map-provider-resource-0331/boundary-continue.ko"
            )
            .to_owned(),
            golden: include_str!(
                "../../../../tests/fixtures/map-provider-resource-0331/map-resource.stdout"
            )
            .to_owned(),
        }];
        for (index, guard, label) in [
            (1, false, "continue-last-use"),
            (4, false, "continue-later-use"),
            (1, true, "guard-continue-last-use"),
            (4, true, "guard-continue-later-use"),
        ] {
            let original = &originals[index];
            let body = if guard {
                "val local = Guard(); read(key, value); continue"
            } else {
                "read(key, value); continue"
            };
            let mut source = original.source.replace("read(key, value)", body);
            let golden = if guard {
                source = "class Guard { deinit() { println(\"local-drop\") } }\n".to_owned() + &source;
                original
                    .golden
                    .replace("value-borrow\n", "value-borrow\nlocal-drop\n")
            } else {
                original.golden.clone()
            };
            cases.push(Case {
                label,
                owned: true,
                source,
                golden,
            });
        }
        for mutable in [false, true] {
            let container = if mutable { "MutableMap" } else { "Map" };
            let conversion = if mutable {
                "return source"
            } else {
                "val result=source.consume(); return result"
            };
            let mutation = if mutable {
                "populated.put(\"later\".clone(),Resource());"
            } else {
                ""
            };
            let size = if mutable { 3 } else { 2 };
            let source = format!(
                r#"
    class Resource {{ deinit() {{ println("resource-drop") }} }}
    fun read(key:String,value:Resource):Unit {{ println(key); println("value-borrow") }}
    fun makeMap():{container}<String,Resource> {{ var source=mutableMapOf<String,Resource>(); source.put("z".clone(),Resource()); source.put("a".clone(),Resource()); {conversion} }}
    fun scan(source:{container}<String,Resource>):Unit {{ for((key,value) in source){{read(key,value);continue}};println("exhausted") }}
    fun entry():Unit {{ println("nonempty"); var populated=makeMap(); scan(populated); println("after-scan"); {mutation}
     if(populated.size=={size}){{println("post-size")}}else{{error("bad size")}};
     println("empty");val empty={factory}<String,Resource>();scan(empty);println("entry-end") }}
    "#,
                factory = if mutable { "mutableMapOf" } else { "mapOf" }
            );
            let drops = "resource-drop\n".repeat(size as usize);
            cases.push(Case{label:if mutable {"borrow-mutable-continue"}else{"borrow-map-continue"},owned:false,source,golden:format!("nonempty\nz\nvalue-borrow\na\nvalue-borrow\nexhausted\nafter-scan\npost-size\nempty\nexhausted\nentry-end\n{drops}")});
        }
        cases
    }
    #[test]
    fn resource_continue_reaches_both_verified_pipelines() {
        let results = sources()
            .iter()
            .flat_map(|case| {
                [false, true].map(|unit| (case.label, unit, super::continuation::observe(case, unit)))
            })
            .collect::<Vec<_>>();
        assert_eq!(results.len(), 14);
        assert_diagnostic!(results.iter().all(|(_, _, r)| r.is_ok()), "{results:#?}");
    }
}
mod identity;
pub(super) mod nullable {
    //! Existing nullable Map value representation, borrowed as a nullable handle without extraction.
    use super::super::fixture::{Failure, failure, need};
    use super::continuation::Case;
    use crate::ssa::model::*;
    use crate::test_support::diagnostics::assert_diagnostic;
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
            include_str!("../../../tests/fixtures/map-provider-resource-0331/boundary-nullable.ko");
        let normal =
            include_str!("../../../tests/fixtures/map-provider-resource-0331/map-resource.stdout");
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
        assert_diagnostic!(results.iter().all(|(_, _, r)| r.is_ok()), "{results:#?}");
    }
}
pub(super) mod terminated;

use super::{
    Report,
    fixture::{Failure, clean, failure, verify_failure},
    pipeline,
};
use lang_frontend::{lexer::lex, parser::parse_file, source::SourceMap};

pub(crate) fn sources() -> [(&'static str, &'static str, &'static str); 2] {
    [
        (
            "map-resource",
            include_str!("../../../tests/fixtures/map-provider-resource-0331/map-resource.ko"),
            include_str!("../../../tests/fixtures/map-provider-resource-0331/map-resource.stdout"),
        ),
        (
            "mutable-map-resource",
            include_str!(
                "../../../tests/fixtures/map-provider-resource-0331/mutable-map-resource.ko"
            ),
            include_str!(
                "../../../tests/fixtures/map-provider-resource-0331/mutable-map-resource.stdout"
            ),
        ),
    ]
}

fn observe(label: &str, text: &str, unit: bool, report: &mut Report) -> Result<(), Failure> {
    let mut sources = SourceMap::new();
    let id = sources
        .add_source("provider.ko", text)
        .map_err(|e| failure("source", e))?;
    let lexed = lex(&sources, id).map_err(|e| failure("lex", e))?;
    clean(&sources, "lex diagnostics", lexed.diagnostics())?;
    let parsed = parse_file(&sources, &lexed).map_err(|e| failure("parse", e))?;
    clean(&sources, "parse diagnostics", parsed.diagnostics())?;
    let input = identity::ast(&sources, &parsed, label == "map-resource")?;
    let (program, entry) = pipeline::unit_input_observed(
        &sources,
        &parsed,
        "provider.ko",
        report,
        unit,
        |names, typed, owned, report| {
            report.query(
                "resource original unit FE obligations",
                &identity::unit(&input, &parsed, names, typed, owned),
            );
            report.check(
                "unit one actual Resource Map provider",
                typed.map_iterations().len() == 1 && owned.map_iterations().len() == 1,
                (typed.map_iterations(), owned.map_iterations()),
            );
        },
    )?;
    report.query(
        "resource independent SSA incoming/current lexical drop",
        &identity::ssa(&program, &input, true),
    );
    super::super::verify::verify_program(&program).map_err(|e| verify_failure(&sources, e))?;
    report.check("SSA verifier", true, entry);
    let llvm = crate::llvm::render_verified_program(&program)
        .map_err(|e| failure("LLVM render/verify", e))?;
    report.check("LLVM render/verify", true, llvm.len());
    Ok(())
}

#[test]
fn resource_map_normal_sources_reach_verified_llvm_in_both_entries() {
    let mut results = Vec::new();
    for (label, text, _) in sources() {
        for unit in [false, true] {
            let mut report = Report::default();
            let outcome = observe(label, text, unit, &mut report);
            diagnostic!(
                "SPEC0331 RESOURCE case={label} rooted={unit} outcome={outcome:?} report={report:#?}"
            );
            results.push((label, unit, report.passed() && outcome.is_ok()));
        }
    }
    assert_diagnostic!(
        results.len() == 4 && results.iter().all(|(_, _, passed)| *passed),
        "all four original Resource Map pipelines must pass: {results:?}"
    );
}

pub(in crate::ssa) mod identity {
//! 返回的真实 entry ID、源码 Origin 与从 entry block 出发的 CFG/DirectCall 可达身份。
use super::super::fixture::{Exit, Failure, Fixture, failure, location, loop_site, need};
use crate::ssa::model::{EntityId, EntityType, FunctionId, Operation, Origin, Program};
use crate::test_support::diagnostics::diagnostic;
use lang_frontend::{
    parser::{Item, NameMarker},
    source::{SourceMap, Span},
};

pub(in crate::ssa) fn source_functions(fixture: &Fixture) -> Result<Vec<(&'static str, Span)>, Failure> {
    let parsed = &fixture.parsed;
    let root_count = if fixture.exit == Exit::Return { 6 } else { 5 };
    if parsed.roots().len() != root_count {
        return Err(failure(
            "source function identity query",
            (
                "expected Guard plus actual top-level function roots",
                parsed.roots().len(),
            ),
        ));
    }
    let factory = if fixture.map {
        "makeMap"
    } else {
        "makeMutable"
    };
    let mut functions = Vec::new();
    for expected in ["entry", "scan", "read", factory]
        .into_iter()
        .chain((fixture.exit == Exit::Return).then_some("operand"))
    {
        let mut spans = Vec::new();
        for root in parsed.roots() {
            let node = parsed
                .ast()
                .items()
                .get(*root)
                .map_err(|error| failure("source function identity query", error))?;
            if let Item::Function {
                name: NameMarker::Present(name),
                ..
            } = node.payload()
                && fixture.sources.slice(*name).ok() == Some(expected)
            {
                if node.span().source_id() != parsed.source_id()
                    || fixture.sources.source_name(node.span().source_id()) != Ok(fixture.path)
                {
                    return Err(failure(
                        "source function identity query",
                        location(&fixture.sources, node.span()),
                    ));
                }
                spans.push(node.span());
            }
        }
        let [span] = spans.as_slice() else {
            return Err(failure("source function identity query", (expected, spans)));
        };
        functions.push((expected, *span));
    }
    let guards = parsed.roots().iter().filter_map(|root| {
        let node = parsed.ast().items().get(*root).ok()?;
        let Item::Classifier(class) = node.payload() else { return None };
        matches!(class.name, NameMarker::Present(name) if fixture.sources.slice(name) == Ok("Guard")).then_some((node, class))
    }).collect::<Vec<_>>();
    let [(node, guard)] = guards.as_slice() else {
        return Err(failure("AST unique actual Guard", guards));
    };
    let body = guard
        .body
        .as_ref()
        .ok_or_else(|| failure("AST Guard body", node))?;
    let [deinit] = body.members.as_slice() else {
        return Err(failure("AST sole Guard deinit member", body));
    };
    let deinit = parsed
        .ast()
        .items()
        .get(*deinit)
        .map_err(|e| failure("AST Guard deinit", e))?;
    need(
        matches!(deinit.payload(), Item::Deinit { .. })
            && fixture.sources.slice(deinit.span()) == Ok("deinit() { println(\"local-drop\") }"),
        "AST real local-drop deinit body",
        deinit,
    )?;
    functions.push(("Guard.deinit", deinit.span()));
    Ok(functions)
}

pub(in crate::ssa) fn entry_sources(
    sources: &SourceMap,
    program: &Program,
    entry: FunctionId,
    expected: &[(&str, Span)],
) -> Result<(), Failure> {
    let [module] = program.modules.as_slice() else {
        return Err(failure(
            "SSA source identity query",
            "expected one actual source module",
        ));
    };
    let mut required = Vec::new();
    for (name, span) in expected {
        let matches = module
            .functions
            .iter()
            .filter(|function| matches!(&function.origin, Origin::Source(actual) if actual == span))
            .collect::<Vec<_>>();
        let [function] = matches.as_slice() else {
            return Err(failure(
                "SSA source identity query",
                (name, location(sources, *span), matches.len()),
            ));
        };
        diagnostic!(
            "SPEC0331 PROVIDER EXIT FUNCTION source_name={name} id={:?} lowered_name={:?} {}",
            function.id,
            function.name,
            location(sources, *span)
        );
        if *name == "entry" && function.id != entry {
            return Err(failure(
                "SSA entry identity query",
                (
                    "lower returned a different source function",
                    entry,
                    function.id,
                ),
            ));
        }
        required.push(function.id);
    }
    let mut pending = vec![entry];
    let mut reached = Vec::new();
    while let Some(id) = pending.pop() {
        if reached.contains(&id) {
            continue;
        }
        let function = module.function(id).ok_or_else(|| {
            failure(
                "SSA entry call graph query",
                ("missing actual function", id),
            )
        })?;
        reached.push(id);
        let entry_block = function
            .entry_block()
            .ok_or_else(|| failure("SSA entry call graph query", ("missing entry block", id)))?;
        let mut blocks = vec![entry_block];
        let mut reached_blocks = Vec::new();
        while let Some(block_id) = blocks.pop() {
            if reached_blocks.contains(&block_id) {
                continue;
            }
            let block = function.block(block_id).ok_or_else(|| {
                failure(
                    "SSA entry call graph query",
                    ("missing actual block", block_id),
                )
            })?;
            reached_blocks.push(block_id);
            // 只读当前可达 block 引用的 instructions，不扫描 function instruction arena。
            for instruction_id in &block.instructions {
                let instruction = function.instruction(*instruction_id).ok_or_else(|| {
                    failure(
                        "SSA entry call graph query",
                        ("missing referenced instruction", instruction_id),
                    )
                })?;
                if let Operation::DirectCall { callee, .. } = &instruction.operation {
                    diagnostic!(
                        "SPEC0331 PROVIDER EXIT CALL caller={id:?} block={block_id:?} callee={callee:?} origin={:?}",
                        instruction.origin
                    );
                    pending.push(*callee);
                }
                // Drop invokes the registered deinit implicitly; this is a real call graph edge.
                if let Operation::Drop { owner } = instruction.operation {
                    let entity = function
                        .entity(EntityId::Value(owner))
                        .ok_or_else(|| failure("SSA Drop owner entity", owner))?;
                    if let EntityType::Value(ty) = entity.ty
                        && let Some(deinit) = module.deinits.get(&ty)
                    {
                        diagnostic!(
                            "SPEC0331 EXIT DEINIT EDGE caller={id:?} block={block_id:?} owner={owner:?} type={ty:?} deinit={deinit:?} origin={:?}",
                            instruction.origin
                        );
                        pending.push(*deinit);
                    }
                }
            }
            let terminator = block.terminator.as_ref().ok_or_else(|| {
                failure(
                    "SSA entry call graph query",
                    ("missing actual terminator", block_id),
                )
            })?;
            blocks.extend(terminator.kind.targets());
        }
    }
    if !required.iter().all(|id| reached.contains(id)) {
        return Err(failure("SSA entry call graph query", (required, reached)));
    }
    Ok(())
}

/// No loop-carrier shape is guessed here. Inspect actual reachable construction, Drop and operand.
pub(in crate::ssa) fn guard_and_operand(
    f: &Fixture,
    program: &Program,
    expected: &[(&str, Span)],
) -> Result<(), Failure> {
    let site = loop_site(f)?;
    let [module] = program.modules.as_slice() else {
        return Err(failure("SSA exit entity query", "expected one module"));
    };
    let function = |name: &str| -> Result<_, Failure> {
        let span = expected
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, s)| *s)
            .ok_or_else(|| failure("SSA expected source function", name))?;
        let found = module
            .functions
            .iter()
            .filter(|f| matches!(f.origin, Origin::Source(actual) if actual == span))
            .collect::<Vec<_>>();
        let [function] = found.as_slice() else {
            return Err(failure("SSA unique source function", (name, found.len())));
        };
        Ok(*function)
    };
    let scan = function("scan")?;
    let deinit = function("Guard.deinit")?;
    let consumer = function(f.exit.callee())?;
    let mut pending = vec![
        scan.entry_block()
            .ok_or_else(|| failure("SSA scan entry block", scan.id))?,
    ];
    let mut blocks = Vec::new();
    let mut reachable = Vec::new();
    while let Some(id) = pending.pop() {
        if blocks.contains(&id) {
            continue;
        }
        let block = scan
            .block(id)
            .ok_or_else(|| failure("SSA actual reachable block", id))?;
        blocks.push(id);
        for instruction in &block.instructions {
            reachable.push(
                scan.instruction(*instruction)
                    .ok_or_else(|| failure("SSA reachable instruction", instruction))?,
            );
        }
        let terminator = block
            .terminator
            .as_ref()
            .ok_or_else(|| failure("SSA reachable terminator", id))?;
        pending.extend(terminator.kind.targets());
    }
    let allocations = reachable
        .iter()
        .filter_map(|i| match i.operation {
            Operation::HeapAllocate { owner, .. }
                if i.origin.span() == site.guard_span
                    && module.deinits.get(&owner) == Some(&deinit.id) =>
            {
                Some((*i, owner))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let [(allocation, guard_type)] = allocations.as_slice() else {
        return Err(failure(
            "SSA actual Guard allocation and registered deinit",
            allocations,
        ));
    };
    let [EntityId::Value(guard_owner)] = allocation.results.as_slice() else {
        return Err(failure(
            "SSA Guard allocation unique actual Value result",
            &allocation.results,
        ));
    };
    let guard_drops = reachable
        .iter()
        .filter(|i| {
            let Operation::Drop { owner } = i.operation else {
                return false;
            };
            scan.entity(EntityId::Value(owner))
                .is_some_and(|e| e.ty == EntityType::Value(*guard_type))
        })
        .collect::<Vec<_>>();
    need(
        matches!(guard_drops.as_slice(), [drop] if matches!(drop.operation, Operation::Drop { owner } if owner == *guard_owner)),
        "SSA unique Guard Drop consumes exact allocation result",
        &guard_drops,
    )?;
    let calls = reachable
        .iter()
        .filter(|i| {
            matches!(&i.operation, Operation::DirectCall { callee, .. } if *callee == consumer.id)
                && i.origin.span() == site.call_span
        })
        .collect::<Vec<_>>();
    let [call] = calls.as_slice() else {
        return Err(failure("SSA reachable actual component call", calls));
    };
    diagnostic!(
        "SPEC0331 EXIT ACTUAL ENTITIES scan={:?} allocation={allocation:#?} guard_type={guard_type:?} drops={guard_drops:#?} body_call={call:#?}",
        scan.id
    );
    if f.exit == Exit::Return {
        use crate::ssa::model::{Definition, TerminatorKind as T};
        let [EntityId::Value(result)] = call.results.as_slice() else {
            return Err(failure("SSA operand actual scalar result", &call.results));
        };
        let returned = blocks
            .iter()
            .filter_map(|id| scan.block(*id))
            .filter_map(|b| b.terminator.as_ref())
            .filter(|t| t.origin.span() == site.transfer_span)
            .filter_map(|t| {
                if let T::Return { values } = &t.kind {
                    Some((t, values))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        let [(terminator, values)] = returned.as_slice() else {
            return Err(failure("SSA unique actual transfer Return", returned));
        };
        let [returned_value] = values.as_slice() else {
            return Err(failure(
                "SSA actual transfer unique returned Value",
                terminator,
            ));
        };
        // A has no body CFG: require the operand result itself or a real backwards Copy chain.
        // A block parameter is not accepted from a single matching incoming; no phi colouring.
        let mut cursor = *returned_value;
        let mut chain = Vec::new();
        while cursor != *result {
            need(
                !chain.contains(&cursor),
                "SSA return Copy chain is rooted, not cyclic",
                &chain,
            )?;
            chain.push(cursor);
            let entity = scan
                .entity(EntityId::Value(cursor))
                .ok_or_else(|| failure("SSA return Copy actual entity", cursor))?;
            let Definition::InstructionResult { instruction, index } = entity.definition else {
                return Err(failure(
                    "SSA A return requires direct result/Copy, no unproved phi transport",
                    (cursor, entity),
                ));
            };
            let copy = scan
                .instruction(instruction)
                .ok_or_else(|| failure("SSA return Copy actual instruction", instruction))?;
            need(
                index == 0
                    && copy.results.as_slice() == [EntityId::Value(cursor)]
                    && reachable.iter().any(|i| i.id == instruction),
                "SSA actual reachable Copy result definition",
                copy,
            )?;
            let Operation::Copy { source } = copy.operation else {
                return Err(failure(
                    "SSA returned value must be actual operand result or Copy",
                    copy,
                ));
            };
            cursor = source;
        }
        diagnostic!(
            "SPEC0331 EXIT RETURN ROOT operand_result={result:?} returned={returned_value:?} backwards_copies={chain:?} terminator={terminator:#?}"
        );
    }
    Ok(())
}
}
