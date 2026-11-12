//! Ordinary source only: original LLVM, object, link and native execution.
use crate::native_tests::{boxed_enum_tests::assert_success, emit_link_and_run};

fn matrix(factory: &str, ty: &str, own: bool, live: bool, map: bool) {
    let mut source = String::new();
    let mut calls = String::new();
    let mut expected = String::new();
    for count in [0, 1, 3] {
        let items = (1..=count)
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        source.push_str(&format!("fun make{count}(): {ty}<Int> {{ println(\"factory\"); return {factory}<Int>({items}) }}\n"));
        let parameter = if own {
            format!("own keys: {ty}<Int>, ")
        } else {
            String::new()
        };
        let seed = if own {
            String::new()
        } else {
            format!("val keys = make{count}();")
        };
        let before = if map {
            "var cache = mutableMapOf<Int, String>();"
        } else {
            ""
        };
        let update = if map {
            "cache.put(key, \"latest\".clone());"
        } else {
            ""
        };
        let tail = if live {
            format!(
                "if (keys.size == {count}) {{ println(\"live\") }} else {{ println(\"wrong\") }};"
            )
        } else {
            String::new()
        };
        let map_tail = if map {
            "if (cache.contains(1)) { println(\"present\") } else { println(\"empty\") };"
        } else {
            ""
        };
        source.push_str(&format!("fun work{count}({parameter}stop: Int): Unit {{ {seed} {before} for (key in keys) {{ {update} println(\"head\"); if (key == stop) {{ println(\"return\"); return }}; println(\"tail\") }}; {tail} {map_tail} println(\"done\") }}\n"));
        for stop in [0, 1, 3] {
            let argument = if own {
                format!("make{count}(), ")
            } else {
                String::new()
            };
            calls.push_str(&format!("work{count}({argument}{stop});"));
            expected.push_str("factory\n");
            let mut returned = false;
            for key in 1..=count {
                expected.push_str("head\n");
                if key == stop {
                    expected.push_str("return\n");
                    returned = true;
                    break;
                }
                expected.push_str("tail\n");
            }
            if !returned {
                if live {
                    expected.push_str("live\n");
                }
                if map {
                    expected.push_str(if count == 0 { "empty\n" } else { "present\n" });
                }
                expected.push_str("done\n");
            }
        }
    }
    source.push_str(&format!("fun main(): Unit {{ {calls} }}"));
    let run = emit_link_and_run("owned-source-return-cardinality.ko", &source, "main");
    assert_success(&run, expected.as_bytes());
}

#[test]
fn owned_source_return_native_array_local_dead_no_map() {
    matrix("arrayOf", "Array", false, false, false);
}

#[test]
fn owned_source_return_native_array_local_dead_map() {
    matrix("arrayOf", "Array", false, false, true);
}

#[test]
fn owned_source_return_native_array_local_live_no_map() {
    matrix("arrayOf", "Array", false, true, false);
}

#[test]
fn owned_source_return_native_array_local_live_map() {
    matrix("arrayOf", "Array", false, true, true);
}

#[test]
fn owned_source_return_native_array_own_dead_no_map() {
    matrix("arrayOf", "Array", true, false, false);
}

#[test]
fn owned_source_return_native_array_own_dead_map() {
    matrix("arrayOf", "Array", true, false, true);
}

#[test]
fn owned_source_return_native_array_own_live_no_map() {
    matrix("arrayOf", "Array", true, true, false);
}

#[test]
fn owned_source_return_native_array_own_live_map() {
    matrix("arrayOf", "Array", true, true, true);
}

#[test]
fn owned_source_return_native_list_local_dead_no_map() {
    matrix("listOf", "List", false, false, false);
}

#[test]
fn owned_source_return_native_list_local_dead_map() {
    matrix("listOf", "List", false, false, true);
}

#[test]
fn owned_source_return_native_list_local_live_no_map() {
    matrix("listOf", "List", false, true, false);
}

#[test]
fn owned_source_return_native_list_local_live_map() {
    matrix("listOf", "List", false, true, true);
}

#[test]
fn owned_source_return_native_list_own_dead_no_map() {
    matrix("listOf", "List", true, false, false);
}

#[test]
fn owned_source_return_native_list_own_dead_map() {
    matrix("listOf", "List", true, false, true);
}

#[test]
fn owned_source_return_native_list_own_live_no_map() {
    matrix("listOf", "List", true, true, false);
}

#[test]
fn owned_source_return_native_list_own_live_map() {
    matrix("listOf", "List", true, true, true);
}

#[test]
fn owned_source_return_native_mutable_list_local_dead_no_map() {
    matrix("mutableListOf", "MutableList", false, false, false);
}

#[test]
fn owned_source_return_native_mutable_list_local_dead_map() {
    matrix("mutableListOf", "MutableList", false, false, true);
}

#[test]
fn owned_source_return_native_mutable_list_local_live_no_map() {
    matrix("mutableListOf", "MutableList", false, true, false);
}

#[test]
fn owned_source_return_native_mutable_list_local_live_map() {
    matrix("mutableListOf", "MutableList", false, true, true);
}

#[test]
fn owned_source_return_native_mutable_list_own_dead_no_map() {
    matrix("mutableListOf", "MutableList", true, false, false);
}

#[test]
fn owned_source_return_native_mutable_list_own_dead_map() {
    matrix("mutableListOf", "MutableList", true, false, true);
}

#[test]
fn owned_source_return_native_mutable_list_own_live_no_map() {
    matrix("mutableListOf", "MutableList", true, true, false);
}

#[test]
fn owned_source_return_native_mutable_list_own_live_map() {
    matrix("mutableListOf", "MutableList", true, true, true);
}

#[test]
fn owned_source_return_native_operand_cleanup() {
    let run = emit_link_and_run(
        "owned-source-return-cleanup.ko",
        include_str!("../../tests/fixtures/owned-source-return-0314/operand-cleanup.ko"),
        "main",
    );
    assert_success(&run, concat!("first\noperand\nlater\nearlier\nreturned\n", "fallthrough\nlater\nearlier\nfallthrough\nlater\nearlier\ntail kept\nexhausted\ncompleted\n", "copy returned\ncopy exhausted\n").as_bytes());
}

#[test]
fn owned_source_return_native_distinct_maps() {
    let run = emit_link_and_run(
        "owned-source-return-distinct.ko",
        include_str!("../../tests/fixtures/owned-source-return-0314/distinct-owners.ko"),
        "main",
    );
    assert_success(
        &run,
        concat!(
            "parameter empty\nparameter no tail\nparameter missing\n",
            "return\n",
            "return\n",
            "first no last\nfirst no tail\nsecond tail\nfirst after\nsecond after removed\n",
            "second removed\nreturn\n",
            "second removed\nsecond removed\nsecond removed\nreturn\n"
        )
        .as_bytes(),
    );
}

#[test]
fn owned_source_return_native_independent_results() {
    let run = emit_link_and_run(
        "owned-source-return-results.ko",
        include_str!("../../tests/fixtures/owned-source-return-0314/owned-results.ko"),
        "main",
    );
    let one = "returned\ntail\ntail\nreturned\ntail\ntail\ntail\nfallback\n";
    assert_success(&run, one.repeat(4).as_bytes());
}

#[test]
fn owned_source_return_native_post_loop_permissions() {
    let source = r#"
fun consume(own source: Array<Int>): Unit { if (source.size == 3) { println("moved") } }
fun handoff(stop: Int): Unit {
    var keys = arrayOf(1, 2, 3)
    for (key in keys) { if (key == stop) { return } }
    consume(keys)
}
fun mutate(stop: Int): Unit {
    var keys = mutableListOf(1, 2, 3)
    for (key in keys) { if (key == stop) { return } }
    keys.add(4)
    keys.removeLast()
    if (keys.size == 3) { println("mutated") }
}
fun main(): Unit { handoff(2); handoff(9); mutate(2); mutate(9) }
"#;
    assert_success(
        &emit_link_and_run("owned-source-return-permissions.ko", source, "main"),
        b"moved\nmutated\n",
    );
}

#[test]
fn owned_source_return_native_moveonly_patterns() {
    let source = r#"
value class Item(val id: Int, val name: String)
fun destructured(stop: Int): Unit {
    val items = listOf(Item(1, "one"), Item(2, "two"), Item(3, "three"))
    for ((id, name) in items) { println(name); if (id == stop) { return }; println("tail") }
    if (items.size == 3) { println("retained") }
}
fun discarded(stop: Boolean): Unit {
    val keys = arrayOf("one", "two", "three")
    for (_ in keys) { if (stop) { return }; println("tail") }
    println("done")
}
fun main(): Unit { destructured(2); destructured(9); discarded(true); discarded(false) }
"#;
    assert_success(
        &emit_link_and_run("owned-source-return-patterns.ko", source, "main"),
        b"one\ntail\ntwo\none\ntail\ntwo\ntail\nthree\ntail\nretained\ntail\ntail\ntail\ndone\n",
    );
}

#[test]
fn owned_source_return_native_fresh_inner_instances() {
    let mut source = String::new();
    for live in [false, true] {
        let tail = if live {
            "if (keys.size == 2) { println(\"live\") }"
        } else {
            ""
        };
        source.push_str(&format!("fun scan{live}(stop: String): Unit {{ for (outer in arrayOf(\"first\", \"second\")) {{ val keys = arrayOf(outer.clone(), \"tail\"); for (key in keys) {{ println(key); if (key == stop) {{ return }}; println(\"body\") }}; {tail} }} }}\n"));
    }
    source.push_str("fun main(): Unit { scanfalse(\"tail\"); scanfalse(\"never\"); scantrue(\"tail\"); scantrue(\"never\") }");
    assert_success(
        &emit_link_and_run("owned-source-return-fresh.ko", &source, "main"),
        concat!(
            "first\nbody\ntail\n",
            "first\nbody\ntail\nbody\nsecond\nbody\ntail\nbody\n",
            "first\nbody\ntail\n",
            "first\nbody\ntail\nbody\nlive\nsecond\nbody\ntail\nbody\nlive\n"
        )
        .as_bytes(),
    );
}

pub(in crate::ssa) mod factory {
//! Constructed storage consumes the actual owned payload and returns its current owner.
use crate::ssa::map_provider_exits_pipeline_tests::temporary_delivery::graph::*;
use lang_frontend::{
    ast::ExpressionId,
    parser::{Expression, Item, NameMarker},
    type_checking::ParameterMode,
};

fn original_call(
    sources: &SourceMap,
    parsed: &ParsedFile,
    scope: Span,
    wanted: &str,
    member: bool,
) -> Result<(ExpressionId, Span), Failure> {
    one(parsed.ast().expressions().iter().filter_map(|(id,node)|{
        let Expression::Call {callee,..}=node.payload()else{return None;};
        if !ast::inside(node.span(),scope){return None;}
        let target=parsed.ast().expressions().get(*callee).ok()?;
        let selected=if member{matches!(target.payload(),Expression::Member {name_span,..} if sources.slice(*name_span)==Ok(wanted))}
            else{matches!(target.payload(),Expression::Name)&&sources.slice(target.span())==Ok(wanted)};
        selected.then_some((id,node.span()))
    }),"delivery actual factory body Call")
}

pub(in crate::ssa) fn check(
    case: &Case,
    sources: &SourceMap,
    parsed: &ParsedFile,
    module: &Module,
    f: &Function,
    input: &ast::Input,
    map: SsaTypeId,
) -> Result<(), Failure> {
    let (constructor, constructor_span) =
        original_call(sources, parsed, input.factory.span, "mutableMapOf", false)?;
    let (put_ast, put_span) = original_call(sources, parsed, input.factory.span, "put", true)?;
    let local = |initializer| {
        one(
            parsed.ast().items().iter().filter_map(|(_, node)| {
                let Item::Variable {
                    name: NameMarker::Present(name),
                    initializer: actual,
                    ..
                } = node.payload()
                else {
                    return None;
                };
                (ast::inside(node.span(), input.factory.span) && *actual == initializer)
                    .then_some(*name)
            }),
            "delivery actual factory local binding",
        )
    };
    let root_marker = local(constructor)?;
    let receiver = |call| -> Result<(), Failure> {
        let Expression::Call { callee, .. } =
            parsed.ast().expressions().get(call).unwrap().payload()
        else {
            unreachable!()
        };
        let Expression::Member { receiver, .. } =
            parsed.ast().expressions().get(*callee).unwrap().payload()
        else {
            return Err(failure("delivery factory original member Call", call));
        };
        need(
            matches!(
                parsed.ast().expressions().get(*receiver).unwrap().payload(),
                Expression::Name
            ) && sources.slice(ast::expression(parsed, *receiver)) == sources.slice(root_marker),
            "delivery put/consume receiver is actual constructed root",
            call,
        )
    };
    receiver(put_ast)?;
    let (consume_span, returned_marker) = if case.mutable {
        (None, root_marker)
    } else {
        let (id, span) = original_call(sources, parsed, input.factory.span, "consume", true)?;
        receiver(id)?;
        (Some(span), local(id)?)
    };
    let returned = one(
        parsed.ast().expressions().iter().filter_map(|(_, node)| {
            let Expression::Return {
                value: Some(value), ..
            } = node.payload()
            else {
                return None;
            };
            ast::inside(node.span(), input.factory.span).then_some((node.span(), *value))
        }),
        "delivery actual factory sole source Return",
    )?;
    need(
        matches!(
            parsed
                .ast()
                .expressions()
                .get(returned.1)
                .unwrap()
                .payload(),
            Expression::Name
        ) && sources.slice(ast::expression(parsed, returned.1)) == sources.slice(returned_marker),
        "delivery Return names actual current put/consume binding",
        returned,
    )?;
    let g = Graph::with_abort(f)?;
    let construct = at(&g, constructor_span, |o| {
        matches!(o, Operation::MapConstruct { .. })
    })?;
    let put = at(&g, put_span, |o| matches!(o, Operation::MapPut { .. }))?;
    let Operation::MapConstruct { map_type: mutable } = construct.operation else {
        unreachable!()
    };
    let Operation::MapPut {
        owner,
        key,
        value: payload,
    } = put.operation
    else {
        unreachable!()
    };
    let (_, key_type, value_type) = module
        .map_container(map)
        .ok_or_else(|| failure("delivery factory result representation", map))?;
    let initial = value(construct)?;
    need(
        module.map_container(mutable) == Some((MapContainerKind::MutableMap, key_type, value_type))
            && owner == initial
            && construct.block == put.block
            && g.position(construct)? < g.position(put)?
            && f.entity(EntityId::Value(key)).unwrap().ty == EntityType::Value(key_type)
            && f.entity(EntityId::Value(payload)).unwrap().ty == EntityType::Value(value_type)
            && f.entity(result(construct)?).unwrap().ty == EntityType::Value(mutable)
            && f.entity(result(put)?).unwrap().ty == EntityType::Value(mutable)
            && g.instructions
                .iter()
                .filter(|i| matches!(i.operation, Operation::MapConstruct { .. }))
                .count()
                == 1
            && g.instructions
                .iter()
                .filter(|i| matches!(i.operation, Operation::MapPut { .. }))
                .count()
                == 1,
        "delivery actual MapConstruct to exact current MapPut and represented payload",
        (construct, put),
    )?;
    let mut uses = Uses::default();
    uses.protect(EntityId::Value(initial));
    uses.permit(EntityId::Value(initial), put);
    let mut current = value(put)?;
    uses.protect(EntityId::Value(current));
    let mut owners = BTreeSet::from([initial.index(), current.index()]);
    if let Some(span) = consume_span {
        let consume = at(&g, span, |o| matches!(o, Operation::MapConsume { .. }))?;
        need(
            consume.operation
                == Operation::MapConsume {
                    owner: current,
                    map_type: map,
                }
                && consume.block == put.block
                && g.position(put)? < g.position(consume)?
                && g.instructions
                    .iter()
                    .filter(|i| matches!(i.operation, Operation::MapConsume { .. }))
                    .count()
                    == 1,
            "delivery readonly conversion consumes actual populated owner",
            consume,
        )?;
        uses.permit(EntityId::Value(current), consume);
        current = value(consume)?;
        owners.insert(current.index());
        uses.protect(EntityId::Value(current));
    } else {
        need(
            mutable == map
                && !g
                    .instructions
                    .iter()
                    .any(|i| matches!(i.operation, Operation::MapConsume { .. })),
            "delivery mutable result is original current Put result",
            map,
        )?;
    }
    let formals = &g.block(f.entry_block().unwrap())?.parameters;
    let own_slots = input
        .factory
        .parameters
        .iter()
        .enumerate()
        .filter(|(_, p)| p.mode == ParameterMode::Value)
        .collect::<Vec<_>>();
    if let [(slot, parameter)] = own_slots.as_slice() {
        let parameter_entity = formals[*slot];
        anchored(&g, EntityId::Value(payload), &[parameter_entity])?;
        let Expression::Call { arguments, .. } =
            parsed.ast().expressions().get(put_ast).unwrap().payload()
        else {
            unreachable!()
        };
        need(
            arguments.len() == 2
                && matches!(
                    parsed
                        .ast()
                        .expressions()
                        .get(arguments[1].value)
                        .unwrap()
                        .payload(),
                    Expression::Name
                )
                && sources.slice(ast::expression(parsed, arguments[1].value))
                    == sources.slice(parameter.name)
                && f.entity(parameter_entity).unwrap().ty == EntityType::Value(value_type),
            "delivery Put source payload is actual own parameter, not borrowed argument",
            (put, parameter_entity),
        )?;
        // A Value parameter is consumed by this Put. Any separate Drop/read or
        // second consumer fails the complete operand-use check.
        let mut family = BTreeSet::from([parameter_entity]);
        loop {
            let before = family.len();
            for (_, edge) in &g.edges {
                for (&from, &to) in edge.arguments.iter().zip(&g.block(edge.target)?.parameters) {
                    if family.contains(&from) {
                        family.insert(to);
                    }
                }
            }
            if before == family.len() {
                break;
            }
        }
        for entity in family {
            anchored(&g, entity, &[parameter_entity])?;
            uses.protect(entity);
        }
        uses.permit(EntityId::Value(payload), put);
    } else {
        need(
            own_slots.is_empty() && matches!(case.delivery, Delivery::SameMapBorrow),
            "delivery only actual Resource payload is Value parameter",
            own_slots,
        )?;
    }
    if matches!(case.delivery, Delivery::SameMapBorrow) {
        let size_ast=one(parsed.ast().expressions().iter().filter_map(|(_,n)|
            (ast::inside(n.span(),input.factory.span)&&matches!(n.payload(),Expression::Member {name_span,..} if sources.slice(*name_span)==Ok("size"))).then_some(n.span())),
            "delivery actual borrowed-input factory size")?;
        let size = at(&g, size_ast, |o| matches!(o, Operation::MapSize { .. }))?;
        let Operation::MapSize { owner: input_loan } = size.operation else {
            unreachable!()
        };
        let input_parameter = one(
            formals.iter().copied(),
            "delivery actual borrowed Map formal",
        )?;
        need(
            matches!(f.entity(input_parameter).unwrap().ty,EntityType::Loan {kind:LoanKind::Shared,target}
            if module.map_container(target).is_some_and(|(_,key,val)|key==key_type&&val==value_type)),
            "delivery same-K/V input remains Shared ABI rather than returned owning storage",
            input_parameter,
        )?;
        anchored(&g, input_loan, &[input_parameter])?;
        let mut family = BTreeSet::from([input_parameter]);
        loop {
            let before = family.len();
            for (_, edge) in &g.edges {
                for (&from, &to) in edge.arguments.iter().zip(&g.block(edge.target)?.parameters) {
                    if family.contains(&from) {
                        family.insert(to);
                    }
                }
            }
            if before == family.len() {
                break;
            }
        }
        for entity in family {
            anchored(&g, entity, &[input_parameter])?;
            uses.protect(entity);
        }
        uses.permit(input_loan, size);
    }
    let normal = one(
        g.blocks
            .iter()
            .copied()
            .filter(|&b| matches!(g.term(b).unwrap().kind, TerminatorKind::Return { .. })),
        "delivery factory sole normal Return",
    )?;
    need(
        normal == construct.block
            && g.term(normal)?.origin == Origin::Source(returned.0)
            && matches!(&g.term(normal)?.kind,TerminatorKind::Return {values} if values.as_slice()==[current])
            && f.entity(EntityId::Value(current)).unwrap().ty == EntityType::Value(map),
        "delivery actual Return carries current construct/Put/consume chain",
        g.term(normal)?,
    )?;
    let all = f
        .values
        .iter()
        .enumerate()
        .filter_map(|(index, data)| {
            module
                .map_container(data.ty.semantic_type())
                .is_some()
                .then_some(index)
        })
        .collect::<BTreeSet<_>>();
    need(
        all == owners
            && !g.instructions.iter().any(|i| {
                matches!(
                    i.operation,
                    Operation::DirectCall { .. } | Operation::BorrowCall { .. }
                )
            }),
        "delivery complete output owner chain has no alternative producer",
        (all, owners),
    )?;
    uses.check(&g, Some((normal, current)))
}
}

pub(crate) mod packed_local_object_constant_qualifier_tests_2 {
//! Ordinary local constant qualifiers and a same-spelling runtime receiver.
//! Every assertion reads original Unit products; no facts, IDs, or SSA are injected.

use crate::native_tests::boxed_enum_tests::unit_source::{OwnedProducts, UnitSource};
use crate::ssa::{
    model::{EntityType, Operation, Origin, ScalarConstant, SsaTypeKind},
    verify::verify_program,
};
use lang_frontend::{
    name_resolution::{
        Namespace, Symbol, SymbolKind, UnitNameReference, UnitReferenceTarget, UnitSymbolId,
    },
    ownership_checking::ConstantMaterializationKind,
    parser::Expression,
    source::Span,
    type_checking::{
        BuiltinType, ConstValue, UnitAggregateProjectionKind, UnitAggregateProjectionReceiver,
        UnitExpressionId, UnitTypeKind,
    },
};

fn expression(source: &UnitSource, text: &str) -> (UnitExpressionId, Span) {
    let [unit] = source.names.names().index().source_units() else {
        panic!("one real MemoryDocument")
    };
    assert_eq!(unit.source_id(), source.parsed.source_id());
    let found = source
        .parsed
        .ast()
        .expressions()
        .iter()
        .filter(|(_, node)| source.sources.slice(node.span()) == Ok(text))
        .collect::<Vec<_>>();
    let [(id, node)] = found.as_slice() else {
        panic!("one actual expression {text:?}")
    };
    (UnitExpressionId::new(unit.id(), *id), node.span())
}

fn symbol(source: &UnitSource, identity: UnitSymbolId) -> &Symbol {
    let local = source
        .names
        .names()
        .source_units()
        .iter()
        .find(|local| local.source_unit() == identity.source_unit())
        .expect("symbol belongs to the original source unit");
    let symbol = local
        .resolution()
        .symbols()
        .get(identity.symbol().index())
        .expect("original source symbol");
    assert_eq!(symbol.id(), identity.symbol());
    assert_eq!(symbol.span().source_id(), source.parsed.source_id());
    assert!(!symbol.is_synthetic());
    symbol
}

fn role(source: &UnitSource, span: Span, namespace: Namespace) -> &UnitNameReference {
    let references = source
        .names
        .names()
        .references()
        .iter()
        .filter(|reference| reference.span() == span && reference.namespace() == Some(namespace))
        .collect::<Vec<_>>();
    let [reference] = references.as_slice() else {
        panic!("exactly one original {namespace:?} reference at {span:?}: {references:?}")
    };
    assert_eq!(
        reference.source_unit(),
        source.names.names().index().source_units()[0].id()
    );
    reference
}

#[test]
fn local_object_and_companion_constants_keep_qualifier_roles_and_real_materializations() {
    let text = r#"object Labels { const val COUNT: Int = 42 }
class Tokens { companion { const val LETTER: Char = '文' } }
fun count(): Int = Labels.COUNT
fun letter(): Char = Tokens.LETTER"#;
    // Analyze retains the original strict complete gate before either ownership path.
    let source = UnitSource::analyze("local-constant-qualifiers.ko", text)
        .expect("ordinary associated constants pass strict Unit admission and ownership");
    source.assert_frontend_clean();
    let OwnedProducts::Constant(typed, owned) = &source.products else {
        panic!("real associated declarations require Constant capability")
    };
    assert!(owned.is_compatible_with(typed));
    assert_eq!(typed.constants().declarations().len(), 2);
    assert_eq!(typed.constants().uses().len(), 2);
    assert_eq!(owned.materializations().len(), 2);

    for (owner, member, function_name, builtin, expected_value, expected_ssa, expected_type) in [
        (
            "Labels",
            "Labels.COUNT",
            "count",
            BuiltinType::Int,
            ConstValue::Integer {
                ty: BuiltinType::Int,
                value: 42,
            },
            ScalarConstant::Integer(42),
            SsaTypeKind::Integer {
                bits: 32,
                signed: true,
            },
        ),
        (
            "Tokens",
            "Tokens.LETTER",
            "letter",
            BuiltinType::Char,
            ConstValue::Char('文'),
            ScalarConstant::Char(u32::from('文')),
            SsaTypeKind::Char,
        ),
    ] {
        let (member_id, member_span) = expression(&source, member);
        let node = source
            .parsed
            .ast()
            .expressions()
            .get(member_id.expression())
            .unwrap();
        let Expression::Member {
            receiver,
            name_span,
            safe: false,
            ..
        } = node.payload()
        else {
            panic!("the original source is an ordinary qualified member")
        };
        let receiver = UnitExpressionId::new(member_id.source_unit(), *receiver);
        let receiver_node = source
            .parsed
            .ast()
            .expressions()
            .get(receiver.expression())
            .unwrap();
        assert!(matches!(receiver_node.payload(), Expression::Name));
        assert_eq!(source.sources.slice(receiver_node.span()), Ok(owner));
        let declaration = source.declaration(owner, SymbolKind::Classifier);
        let names = source.names.names();
        let classifier = &names.index().declarations()[declaration.index()];
        assert_eq!(classifier.namespace(), Namespace::Type);
        assert_eq!(
            role(&source, receiver_node.span(), Namespace::Type).target(),
            &UnitReferenceTarget::Declaration(declaration),
            "each qualifier has exactly one real Type declaration role"
        );
        let classifier_symbol = symbol(&source, names.declaration_symbol(declaration).unwrap());
        assert_eq!(classifier_symbol.kind(), SymbolKind::Classifier);
        assert_eq!(classifier_symbol.span(), classifier.name_span());
        if owner == "Labels" {
            let value = source.declaration(owner, SymbolKind::ObjectValue);
            assert_ne!(value, declaration);
            let value_declaration = &names.index().declarations()[value.index()];
            assert_eq!(value_declaration.namespace(), Namespace::Value);
            assert_eq!(value_declaration.name_span(), classifier.name_span());
            assert_eq!(value_declaration.root(), classifier.root());
            assert_eq!(
                role(&source, receiver_node.span(), Namespace::Value).target(),
                &UnitReferenceTarget::Declaration(value),
                "the lexical ObjectValue role is retained, not replaced"
            );
            assert_eq!(
                symbol(&source, names.declaration_symbol(value).unwrap()).kind(),
                SymbolKind::ObjectValue
            );
        }
        assert_eq!(typed.types().expression_type(receiver), None);
        assert!(typed.types().aggregate_projection(member_id).is_none());
        let uses = typed
            .constants()
            .uses()
            .iter()
            .filter(|usage| usage.expression() == member_id)
            .collect::<Vec<_>>();
        let [usage] = uses.as_slice() else {
            panic!("one real constant descriptor for the whole Member expression")
        };
        assert_eq!(
            role(&source, *name_span, Namespace::Value).target(),
            &UnitReferenceTarget::Symbol(usage.target())
        );
        let target = symbol(&source, usage.target());
        assert_eq!(target.kind(), SymbolKind::Constant);
        assert_eq!(source.sources.slice(*name_span), Ok(target.name()));
        let declarations = typed
            .constants()
            .declarations()
            .iter()
            .filter(|constant| constant.symbol() == usage.target())
            .collect::<Vec<_>>();
        let [constant] = declarations.as_slice() else {
            panic!("the use selects one actual constant declaration")
        };
        assert_eq!(constant.declaration_span(), target.span());
        assert_eq!(constant.ty(), usage.ty());
        assert_eq!(constant.value(), &expected_value);
        assert_eq!(usage.value(), constant.value());
        assert_eq!(typed.types().expression_type(member_id), Some(usage.ty()));
        assert_eq!(
            typed.types().types().get(usage.ty()),
            Some(&UnitTypeKind::Builtin(builtin))
        );
        let materialization = owned.materialization_at(member_id).unwrap();
        assert_eq!(materialization.descriptor(), *usage);
        assert_eq!(
            materialization.kind(),
            ConstantMaterializationKind::InlineCopy
        );
        assert!(owned.materialization_at(receiver).is_none());

        let (program, entry) = source
            .lower_entry(source.declaration(function_name, SymbolKind::Function))
            .expect("original Constant products lower to SSA");
        verify_program(&program).expect("real scalar materialization verifies");
        let module = program.module(entry.module()).unwrap();
        assert_eq!(
            module.functions.len(),
            1,
            "no qualifier or initializer runtime function"
        );
        let function = module.function(entry).unwrap();
        let [return_type] = function.return_types.as_slice() else {
            panic!("one real scalar return")
        };
        assert_eq!(module.types[return_type.index()], expected_type);
        let constants = function
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::Constant(_)))
            .collect::<Vec<_>>();
        let [constant] = constants.as_slice() else {
            panic!("one source-produced scalar constant")
        };
        assert_eq!(constant.origin, Origin::Source(member_span));
        assert_eq!(constant.operation, Operation::Constant(expected_ssa));
        let [result] = constant.results.as_slice() else {
            panic!("one real materialized result")
        };
        assert_eq!(
            function.entity(*result).unwrap().ty,
            EntityType::Value(*return_type)
        );
    }
}

#[test]
fn shadowing_parameter_keeps_value_role_and_basic_field_projection() {
    let text = r#"object Labels {}
class Holder(val COUNT: Boolean)
fun inspect(Labels: Holder): Boolean = Labels.COUNT"#;
    let source = UnitSource::analyze("shadowed-object-qualifier.ko", text)
        .expect("ordinary parameter field access passes strict Unit admission and ownership");
    source.assert_frontend_clean();
    let OwnedProducts::Basic(typed, owned) = &source.products else {
        panic!("no constant declaration or selection may force Constant capability")
    };
    assert!(owned.ownership().diagnostics().is_empty());
    assert!(owned.ownership().deferred().is_empty());
    let typed = typed.types();
    let constants = typed
        .constants()
        .expect("clean source publishes empty constant facts");
    assert!(constants.declarations().is_empty());
    assert!(constants.uses().is_empty());
    let (receiver, receiver_span) = expression(&source, "Labels");
    let (member, _) = expression(&source, "Labels.COUNT");
    assert!(source.names.names().references().iter().all(|reference| {
        reference.span() != receiver_span || reference.namespace() != Some(Namespace::Type)
    }));
    let UnitReferenceTarget::Symbol(parameter_symbol) =
        role(&source, receiver_span, Namespace::Value).target()
    else {
        panic!("the lexical receiver selects its parameter, not the object declaration")
    };
    let parameter = symbol(&source, *parameter_symbol);
    assert_eq!(parameter.kind(), SymbolKind::ValueParameter);
    assert_eq!(parameter.name(), "Labels");
    let inspect = source.declaration("inspect", SymbolKind::Function);
    let callable = typed
        .signatures()
        .declaration(inspect)
        .unwrap()
        .callable()
        .unwrap();
    let [parameter_signature] = callable.parameters() else {
        panic!("one actual inspect parameter")
    };
    assert_eq!(parameter_signature.symbol(), Some(*parameter_symbol));
    assert_eq!(
        typed.expression_type(receiver),
        Some(parameter_signature.ty())
    );
    let holder = source.declaration("Holder", SymbolKind::Classifier);
    assert!(matches!(
        typed.types().get(parameter_signature.ty()),
        Some(UnitTypeKind::Nominal { declaration, arguments })
            if *declaration == holder && arguments.is_empty()
    ));
    let nominal = typed
        .signatures()
        .declaration(holder)
        .unwrap()
        .nominal()
        .unwrap();
    let [field] = nominal.fields() else {
        panic!("one actual Holder field")
    };
    assert_eq!(field.name(), "COUNT");
    assert_eq!(symbol(&source, field.symbol()).kind(), SymbolKind::Field);
    let projection = typed
        .aggregate_projection(member)
        .expect("ordinary runtime field projection");
    assert_eq!(projection.kind(), UnitAggregateProjectionKind::Field);
    assert_eq!(
        projection.receiver(),
        UnitAggregateProjectionReceiver::Expression(receiver)
    );
    assert_eq!(projection.field(), field.symbol());
    assert_eq!(projection.ty(), field.ty());
    assert_eq!(typed.expression_type(member), Some(field.ty()));
    assert_eq!(
        typed.types().get(field.ty()),
        Some(&UnitTypeKind::Builtin(BuiltinType::Boolean))
    );
}
}
