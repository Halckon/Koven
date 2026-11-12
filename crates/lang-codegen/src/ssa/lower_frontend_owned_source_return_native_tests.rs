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
