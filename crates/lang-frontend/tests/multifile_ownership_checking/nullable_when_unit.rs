//! Complete ordinary sources. This old-API version is a behavioral-red candidate, not a run log.
use super::*;
use lang_frontend::{
    diagnostic::Diagnostic,
    ownership_checking::{
        UnitCallArgumentOwnershipContract, UnitOwnershipPlace, owned_compilation_unit_view,
    },
    parser::Expression,
    source::Span,
    type_checking::{UnitCallTarget, complete_compilation_unit_types},
};

#[derive(Clone, Copy, Debug)]
enum Owner {
    Node,
    Box,
    Rc,
}

impl Owner {
    fn ty(self) -> &'static str {
        match self {
            Self::Node => "Node",
            Self::Box => "Box<Cell>",
            Self::Rc => "Rc<Int>",
        }
    }

    fn constructor(self) -> &'static str {
        match self {
            Self::Node => "Node()",
            Self::Box => "Box(Cell(7))",
            Self::Rc => "Rc(7)",
        }
    }

    fn provider(self) -> String {
        let declaration = match self {
            Self::Node => "class Node {}\n",
            Self::Box => "value class Cell(val value: Int)\n",
            Self::Rc => "",
        };
        format!(
            "{declaration}fun make(): {ty}? = {constructor}\n\
             fun read(value: {ty}): Int = 1\n\
             fun readNullable(value: {ty}?): Int = 1\n\
             fun take(own value: {ty}): Unit {{}}\n",
            ty = self.ty(),
            constructor = self.constructor(),
        )
    }

    fn consumer(self, source: &str) -> String {
        source
            .replace("$T", self.ty())
            .replace("$M", self.constructor())
    }
}

struct Checked<'a> {
    sources: &'a SourceMap,
    file: &'a ParsedFile,
    typed: &'a ValidatedCompilationUnitTypes,
    owned: &'a CompilationUnitOwnership,
    names: &'a ValidatedCompilationUnitNames,
    unit: SourceUnitId,
}

impl Checked<'_> {
    fn expression(&self, text: &str) -> UnitExpressionId {
        UnitExpressionId::new(
            self.unit,
            expression_with_text(self.sources, self.file, text),
        )
    }

    fn span(&self, expression: UnitExpressionId) -> Span {
        assert_eq!(expression.source_unit(), self.unit);
        self.file
            .ast()
            .expressions()
            .get(expression.expression())
            .unwrap()
            .span()
    }

    fn text(&self, expression: UnitExpressionId) -> &str {
        self.sources.slice(self.span(expression)).unwrap()
    }

    fn when_subjects(&self) -> Vec<(UnitExpressionId, UnitExpressionId)> {
        self.file
            .ast()
            .expressions()
            .iter()
            .filter_map(|(id, node)| match node.payload() {
                Expression::When {
                    subject: Some(subject),
                    ..
                } => Some((
                    UnitExpressionId::new(self.unit, id),
                    UnitExpressionId::new(self.unit, *subject),
                )),
                _ => None,
            })
            .collect()
    }

    fn contract(&self, call: &str, index: usize) -> UnitCallArgumentOwnershipContract {
        let call = self.expression(call);
        *self
            .owned
            .call_argument_contracts()
            .iter()
            .filter(|fact| fact.call() == call)
            .nth(index)
            .expect("real selected argument contract")
    }

    fn root(&self, name: &str) -> UnitSymbolId {
        symbol_named(self.owned, self.names, self.unit, name)
    }

    fn named_drops(&self, root: UnitSymbolId) -> Vec<UnitDropPoint> {
        self.owned
            .drops()
            .iter()
            .filter(|fact| fact.target() == UnitDropTarget::Named(root))
            .map(|fact| fact.point())
            .collect()
    }
}

fn checked(owner: Owner, source: &str, inspect: impl FnOnce(&Checked<'_>)) {
    let mut sources = SourceMap::new();
    let provider_text = owner.provider();
    let consumer_text = owner.consumer(source);
    let (provider_id, provider) = parsed(&mut sources, "when-provider.ko", &provider_text);
    let (consumer_id, consumer) = parsed(&mut sources, "when-consumer.ko", &consumer_text);
    let inputs = [
        SourceUnitInput::new("root", "when-provider.ko", provider_id, &provider),
        SourceUnitInput::new("root", "when-consumer.ko", consumer_id, &consumer),
    ];
    let (name_environment, environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = validated_types(&sources, &inputs, &names, &environment);
    complete_compilation_unit_types(&sources, &inputs, &names, &environment, typed.types())
        .expect("ordinary sources pass strict final type admission");
    let owned = check_compilation_unit_ownership(&sources, &inputs, &names, &environment, &typed)
        .expect("real ownership analysis");
    assert!(owned.is_compatible_with(&typed));
    if owned.diagnostics().is_empty() {
        let validated = owned.clone().validate().expect("complete ownership facts");
        owned_compilation_unit_view(&sources, &inputs, &names, &environment, &typed, &validated)
            .expect("same-round strict typed/owned handoff");
    }
    let unit = source_unit(&names, consumer_id);
    inspect(&Checked {
        sources: &sources,
        file: &consumer,
        typed: &typed,
        owned: &owned,
        names: &names,
        unit,
    });
}

#[test]
fn unit_when_call_observe_and_consume_follow_real_contracts() {
    let source = r#"
fun reads(own value: $T?): Int = when (value) {
    null -> 0;
    else -> read(value) + read((value))
}
fun consumes(own value: $T?): Int = when (value) {
    null -> 0;
    else -> { take((value)); 1 }
}
fun unnamed(): Int = when (make()) { null -> 0; else -> 1 }
fun noUse(own value: $T?): Int = when (value) { null -> 0; else -> return 1 }
"#;
    for owner in [Owner::Node, Owner::Box, Owner::Rc] {
        checked(owner, source, |unit| {
            assert!(unit.owned.diagnostics().is_empty());
            assert_eq!(unit.when_subjects().len(), 4);
            for call in ["read(value)", "read((value))"] {
                let contract = unit.contract(call, 0);
                assert_eq!(contract.kind(), UnitCallArgumentOwnershipKind::SharedLoan);
                assert!(matches!(
                    unit.typed.types().call(contract.call()).unwrap().target(),
                    UnitCallTarget::Declaration(_)
                ));
                let loans = unit
                    .owned
                    .loans()
                    .iter()
                    .filter(|loan| {
                        loan.call() == contract.call() && loan.argument() == contract.argument()
                    })
                    .collect::<Vec<_>>();
                assert_eq!(loans.len(), 1);
                assert_eq!(loans[0].kind(), LoanKind::Shared);
                assert!(
                    matches!(loans[0].target(), UnitLoanTarget::Place(place) if place.is_root())
                );
                assert!(
                    !unit
                        .owned
                        .value_deliveries()
                        .iter()
                        .any(|fact| fact.call() == contract.call())
                );
            }
            let contract = unit.contract("take((value))", 0);
            assert_eq!(contract.kind(), UnitCallArgumentOwnershipKind::Value);
            let deliveries = unit
                .owned
                .value_deliveries()
                .iter()
                .filter(|delivery| {
                    delivery.call() == contract.call() && delivery.argument() == contract.argument()
                })
                .collect::<Vec<_>>();
            assert_eq!(deliveries.len(), 1);
            assert_eq!(deliveries[0].kind(), UnitValueDeliveryKind::Move);
            assert!(
                deliveries[0]
                    .place()
                    .is_some_and(UnitOwnershipPlace::is_root)
            );
            assert_eq!(unit.text(contract.argument()), "(value)");
            assert!(
                unit.owned.rc_effects().is_empty(),
                "observations never retain"
            );
            // GREEN_CALL_PRODUCTS
        });
    }
}

#[test]
fn unit_when_temporary_cleanup_follows_completion_and_exits() {
    let cases = [
        (
            "fun normal(): Int = when (make()) { null -> 0; else -> 1 }",
            None,
            true,
        ),
        (
            "fun returned(): Int = when (make()) { null -> 0; else -> return 1 }",
            Some("return 1"),
            true,
        ),
        (
            "fun broken(): Unit { var n = 0\nwhile (n < 1) { n = n + 1\nwhen (make()) { null -> {}; else -> break } } }",
            Some("break"),
            true,
        ),
        (
            "fun continued(): Unit { var n = 0\nwhile (n < 1) { n = n + 1\nwhen (make()) { null -> {}; else -> continue } } }",
            Some("continue"),
            true,
        ),
        (
            "fun ended(): Int = when (make()) { null -> return 0; else -> return 1 }",
            Some("return 1"),
            false,
        ),
    ];
    for owner in [Owner::Node, Owner::Box, Owner::Rc] {
        for (source, transfer, normal) in cases {
            checked(owner, source, |unit| {
                assert!(unit.owned.diagnostics().is_empty());
                let [(control, subject)] = unit.when_subjects()[..] else {
                    panic!("one complete When per independent source");
                };
                assert_eq!(unit.text(subject), "make()");
                let target = UnitDropTarget::Temporary(subject);
                let facts = unit
                    .owned
                    .drops()
                    .iter()
                    .filter(|fact| fact.target() == target)
                    .collect::<Vec<_>>();
                assert_eq!(
                    facts
                        .iter()
                        .filter(|fact| fact.point() == UnitDropPoint::AfterExpression(control))
                        .count(),
                    usize::from(normal),
                    "{owner:?}: {source}"
                );
                assert!(
                    facts
                        .iter()
                        .all(|fact| fact.point() != UnitDropPoint::AfterExpression(subject))
                );
                if let Some(transfer) = transfer {
                    let transfer = unit.expression(transfer);
                    assert_eq!(
                        facts
                            .iter()
                            .filter(|fact| fact.point() == UnitDropPoint::ControlTransfer(transfer))
                            .count(),
                        1
                    );
                }
                if !normal {
                    let other = unit.expression("return 0");
                    assert_eq!(
                        facts
                            .iter()
                            .filter(|fact| fact.point() == UnitDropPoint::ControlTransfer(other))
                            .count(),
                        1
                    );
                }
                assert_eq!(
                    facts.len(),
                    usize::from(normal) + usize::from(transfer.is_some()) + usize::from(!normal)
                );
                assert!(
                    facts
                        .iter()
                        .all(|fact| fact.value_origin() == unit.span(subject))
                );
                // GREEN_TEMP_PRODUCTS
            });
        }
        checked(
            owner,
            r#"
fun innerLoop(): Int = when (make()) {
    null -> 0;
    else -> { var n = 0; while (n < 1) { n = n + 1; break }; 1 }
}
"#,
            |unit| {
                let [(control, subject)] = unit.when_subjects()[..] else {
                    panic!("one When")
                };
                let target = UnitDropTarget::Temporary(subject);
                let points = unit
                    .owned
                    .drops()
                    .iter()
                    .filter(|fact| fact.target() == target)
                    .map(|fact| fact.point())
                    .collect::<Vec<_>>();
                assert_eq!(points, [UnitDropPoint::AfterExpression(control)]);
                assert!(
                    !points.contains(&UnitDropPoint::ControlTransfer(unit.expression("break")))
                );
            },
        );
        checked(
            owner,
            r#"
fun outer(first: $T, own count: Int): Int = count
fun pending(): Int = outer($M, when (make()) { null -> 0; else -> return 1 })
"#,
            |unit| {
                let [(control, subject)] = unit.when_subjects()[..] else {
                    panic!("one When")
                };
                let transfer = UnitDropPoint::ControlTransfer(unit.expression("return 1"));
                let call = unit.expression(&format!(
                    "outer({}, {})",
                    owner.constructor(),
                    unit.text(control)
                ));
                let first = unit.contract(unit.text(call), 0).argument();
                for expression in [first, subject] {
                    assert_eq!(
                        unit.owned
                            .drops()
                            .iter()
                            .filter(
                                |fact| fact.target() == UnitDropTarget::Temporary(expression)
                                    && fact.point() == transfer
                            )
                            .count(),
                        1
                    );
                }
                assert_eq!(
                    unit.owned
                        .drops()
                        .iter()
                        .filter(|fact| fact.target() == UnitDropTarget::Temporary(first)
                            && fact.point() == UnitDropPoint::CallReturn(call))
                        .count(),
                    1
                );
            },
        );
    }
}

#[test]
fn unit_when_named_asap_preserves_later_live_owners() {
    let cases = [
        (
            "fun early(own value: $T?): Int = when (value) { null -> return 0; else -> return 1 }",
            true,
        ),
        (
            "fun later(own value: $T?): Int { val n = when (value) { null -> 0; else -> 1 }\nreturn n + readNullable(value) }",
            false,
        ),
        (
            "fun repeated(own value: $T?): Int { var n = 0\nwhile (n < 2) { when (value) { null -> {}; else -> read(value) }\nn = n + 1 }\nreturn readNullable(value) }",
            false,
        ),
    ];
    for owner in [Owner::Node, Owner::Box, Owner::Rc] {
        for (source, early) in cases {
            checked(owner, source, |unit| {
                assert!(unit.owned.diagnostics().is_empty());
                let [(control, subject)] = unit.when_subjects()[..] else {
                    panic!("one When")
                };
                let root = unit.root("value");
                let drops = unit.named_drops(root);
                if early {
                    assert_eq!(drops, [UnitDropPoint::AfterExpression(subject)]);
                } else {
                    assert!(!drops.contains(&UnitDropPoint::AfterExpression(subject)));
                    assert!(!drops.iter().any(|point| matches!(point,
                        UnitDropPoint::BranchExit { control: actual, .. } if *actual == control)));
                    assert_eq!(
                        drops,
                        [UnitDropPoint::CallReturn(
                            unit.expression("readNullable(value)")
                        )]
                    );
                }
                assert!(
                    !unit
                        .owned
                        .value_deliveries()
                        .iter()
                        .any(|delivery| delivery.kind() == UnitValueDeliveryKind::Move
                            && delivery.place().is_some_and(|place| place.root() == root))
                );
                // GREEN_NAMED_PRODUCTS
            });
        }
    }
}

fn labels(diagnostic: &Diagnostic) -> Vec<(Span, &str)> {
    diagnostic
        .details()
        .iter()
        .filter_map(|detail| match detail {
            DiagnosticDetail::Label(label) => Some((label.span(), label.message())),
            _ => None,
        })
        .collect()
}

#[test]
fn unit_when_ownership_failure_and_assignment_rollback_clear_plans() {
    let cases = [
        (
            "fun borrowed(value: Node?): Unit { when (value) { null -> {}; else -> take(value) } }",
            "L0133",
        ),
        (
            "fun exclusive(inout value: Node?): Unit { when (value) { null -> {}; else -> take(value) } }",
            "L0133",
        ),
        (
            "fun moved(own value: Node?): Int { when (value) { null -> {}; else -> take(value) }\nreturn readNullable(value) }",
            "L0131",
        ),
        (
            "fun pair(first: Node?, own second: Node): Int = 1\nfun conflict(own value: Node?): Int = pair(value, when (value) { null -> Node(); else -> value })",
            "L0135",
        ),
        (
            "fun rejected(own value: Node?): Int { val target = Node()\ntarget = when (value) { null -> Node(); else -> value }\nreturn readNullable(value) }",
            "L0134",
        ),
    ];
    for (source, code) in cases {
        let source = format!(
            "fun prior(own priorValue: Node?): Int = when (priorValue) {{ null -> 0; else -> read(priorValue) }}\n{source}"
        );
        checked(Owner::Node, &source, |unit| {
            assert_eq!(diagnostic_codes(unit.owned), [code]);
            let diagnostic = &unit.owned.diagnostics()[0];
            let expected = match code {
                "L0133" => {
                    let contract = unit.contract("take(value)", 0);
                    let root = unit.root("value");
                    let binding = unit
                        .owned
                        .bindings()
                        .iter()
                        .find(|fact| fact.symbol() == root)
                        .unwrap();
                    assert_eq!(
                        labels(diagnostic),
                        [
                            (
                                binding.declaration_span(),
                                "non-owning binding established here"
                            ),
                            (
                                contract.parameter_span().unwrap(),
                                "selected parameter declared here"
                            ),
                        ]
                    );
                    unit.span(contract.argument())
                }
                "L0131" => {
                    let moved = unit.contract("take(value)", 0);
                    let read = unit.contract("readNullable(value)", 0);
                    assert_eq!(
                        labels(diagnostic),
                        [
                            (unit.span(moved.argument()), "value was moved here"),
                            (
                                read.parameter_span().unwrap(),
                                "selected parameter declared here"
                            ),
                        ]
                    );
                    unit.span(read.argument())
                }
                "L0135" => {
                    let call_text = "pair(value, when (value) { null -> Node(); else -> value })";
                    let first = unit.contract(call_text, 0);
                    assert_eq!(
                        labels(diagnostic),
                        [(first.loan_begin_span(), "conflicting loan starts here")]
                    );
                    let (_, node) = unit
                        .file
                        .ast()
                        .expressions()
                        .iter()
                        .filter(|(_, node)| unit.sources.slice(node.span()).unwrap() == "value")
                        .last()
                        .unwrap();
                    node.span()
                }
                "L0134" => {
                    let declaration = unit.names.names().source_units()[unit.unit.index()]
                        .resolution()
                        .symbols()
                        .iter()
                        .find(|symbol| symbol.name() == "target")
                        .unwrap();
                    assert_eq!(
                        labels(diagnostic),
                        [(declaration.span(), "immutable binding declared here")]
                    );
                    let (_, node) = unit
                        .file
                        .ast()
                        .expressions()
                        .iter()
                        .find(|(_, node)| {
                            matches!(node.payload(), Expression::Name)
                                && unit.sources.slice(node.span()).unwrap() == "target"
                        })
                        .unwrap();
                    node.span()
                }
                _ => unreachable!(),
            };
            assert_eq!(diagnostic.primary_span(), expected);
            assert!(unit.owned.loans().is_empty());
            assert!(unit.owned.value_deliveries().is_empty());
            assert!(unit.owned.drops().is_empty());
            assert!(unit.owned.clone().validate().is_err());
            // GREEN_FAILURE_PRODUCTS
        });
    }
}

#[test]
fn unit_when_place_subject_reads_preserve_move_and_loan_guards() {
    // Only a fully checked legacy L0136 is collected; every other mismatch fails immediately.
    let positives = [
        (
            "element-ordinary",
            r#"fun entry(items: List<Node?>): Unit { when (items[0]) { null -> {}; else -> {} } }
"#,
            "items[0]",
            "items[0]",
            true,
            false,
        ),
        (
            "field-ordinary",
            r#"class Holder(val item: Node?)
fun entry(holder: Holder): Unit { when (holder.item) { null -> {}; else -> {} } }
"#,
            "holder.item",
            "holder.item",
            false,
            false,
        ),
        (
            "element-group",
            r#"fun entry(items: List<Node?>): Unit { when ((items[0])) { null -> {}; else -> {} } }
"#,
            "items[0]",
            "(items[0])",
            true,
            false,
        ),
        (
            "field-group",
            r#"class Holder(val item: Node?)
fun entry(holder: Holder): Unit { when ((holder.item)) { null -> {}; else -> {} } }
"#,
            "holder.item",
            "(holder.item)",
            false,
            false,
        ),
        (
            "element-escaping",
            r#"fun entry(items: List<Node?>): Int { return when ((items[0])) { null -> 0; else -> 1 } }
"#,
            "items[0]",
            "(items[0])",
            true,
            true,
        ),
        (
            "field-escaping",
            r#"class Holder(val item: Node?)
fun entry(holder: Holder): Int { return when ((holder.item)) { null -> 0; else -> 1 } }
"#,
            "holder.item",
            "(holder.item)",
            false,
            true,
        ),
    ];
    let mut legacy_rejections = Vec::new();
    let mut visited = Vec::new();
    for (name, source, leaf_text, subject_text, element, escaping) in positives {
        checked(Owner::Node, source, |unit| {
            assert!(
                unit.owned.deferred().is_empty(),
                "{name}: no deferred ownership outcome is permitted"
            );
            let [(control, subject)] = unit.when_subjects()[..] else {
                panic!("{name}: one real When in this independent source");
            };
            assert_eq!(unit.text(subject), subject_text, "{name}");
            let leaf = unit.expression(leaf_text);
            assert_eq!(
                unit.typed.types().expression_category(subject),
                Some(lang_frontend::type_checking::ExpressionCategory::Place),
                "{name}"
            );
            assert_eq!(
                unit.file.ast().expressions().iter().any(|(_, node)| {
                    matches!(node.payload(), Expression::Return { value: Some(value), .. }
                        if *value == control.expression())
                }),
                escaping,
                "{name}: explicit Return reaches the escaping When entry"
            );
            if element {
                let place = unit
                    .typed
                    .types()
                    .element_place(leaf)
                    .expect("real element descriptor");
                assert_eq!(place.expression(), leaf);
                assert_eq!(place.receiver().source_unit(), unit.unit);
                assert_eq!(place.index().source_unit(), unit.unit);
            } else {
                let projection = unit
                    .typed
                    .types()
                    .aggregate_projection(leaf)
                    .expect("real field projection descriptor");
                assert_eq!(projection.expression(), leaf);
                assert_eq!(
                    projection.kind(),
                    lang_frontend::type_checking::UnitAggregateProjectionKind::Field
                );
            }
            if !unit.owned.diagnostics().is_empty() {
                assert!(
                    element,
                    "{name}: unexpected field ownership error: {:?}",
                    diagnostic_codes(unit.owned)
                );
                assert_eq!(
                    diagnostic_codes(unit.owned),
                    ["L0136"],
                    "{name}: unexpected diagnostic"
                );
                let diagnostic = &unit.owned.diagnostics()[0];
                assert_eq!(
                    diagnostic.message(),
                    "cannot move a non-Copyable element out of a sequential container"
                );
                assert_eq!(diagnostic.primary_span(), unit.span(leaf), "{name}");
                let root = unit.root("items");
                let binding = unit
                    .owned
                    .bindings()
                    .iter()
                    .find(|fact| fact.symbol() == root)
                    .unwrap();
                let owner_span = unit
                    .names
                    .names()
                    .source_units()[binding.symbol().source_unit().index()]
                    .resolution()
                    .symbols()[binding.symbol().symbol().index()]
                    .span();
                assert_eq!(
                    labels(diagnostic),
                    [(
                        owner_span,
                        "container owner remains responsible for every initialized element"
                    )]
                );
                assert_eq!(
                    diagnostic.details().len(),
                    1,
                    "{name}: no extra cascade/details"
                );
                assert!(unit.owned.clone().validate().is_err());
                legacy_rejections.push(name);
            }
            assert!(
                unit.owned.loans().is_empty(),
                "{name}: discriminating is not a source loan delivery"
            );
            assert!(
                unit.owned.value_deliveries().is_empty(),
                "{name}: subject read cannot extract a Value"
            );
            assert!(
                unit.owned.rc_effects().is_empty(),
                "{name}: no retain or copy"
            );
            visited.push(name);
        });
    }
    let negatives = [
        (
            "moved-element-owner",
            r#"fun consumeItems(own values: List<Node?>): Unit {}
fun entry(own items: List<Node?>): Unit { consumeItems(items); when (items[0]) { null -> {}; else -> {} } }
"#,
            "L0131",
            "use of moved value",
            (120, 125),
            "items",
            (106, 111),
            "items",
            "value was moved here",
            "consumeItems(items)",
            UnitCallArgumentOwnershipKind::Value,
        ),
        (
            "exclusive-element-owner",
            r#"fun holdItems(inout values: List<Node?>, own result: Unit): Unit {}
fun entry(own input: List<Node?>): Unit { var items = input
val result = holdItems(&items, when (items[0]) { null -> {}; else -> {} }) }
"#,
            "L0135",
            "read conflicts with an active exclusive loan",
            (165, 173),
            "items[0]",
            (151, 152),
            "&",
            "conflicting loan starts here",
            "holdItems(&items, when (items[0]) { null -> {}; else -> {} })",
            UnitCallArgumentOwnershipKind::ExclusiveLoan,
        ),
    ];
    for (
        name,
        source,
        code,
        message,
        primary,
        primary_text,
        label,
        label_text,
        label_message,
        call,
        kind,
    ) in negatives
    {
        checked(Owner::Node, source, |unit| {
            assert!(
                unit.owned.deferred().is_empty(),
                "{name}: no deferred ownership outcome is permitted"
            );
            assert_eq!(
                diagnostic_codes(unit.owned),
                [code],
                "{name}: unexpected diagnostic"
            );
            let diagnostic = &unit.owned.diagnostics()[0];
            assert_eq!(diagnostic.message(), message, "{name}");
            let primary_span = diagnostic.primary_span();
            assert_eq!(
                (primary_span.start(), primary_span.end()),
                primary,
                "{name}"
            );
            assert_eq!(
                unit.sources.slice(primary_span).unwrap(),
                primary_text,
                "{name}"
            );
            let contract = unit.contract(call, 0);
            assert_eq!(
                contract.kind(),
                kind,
                "{name}: real preceding move/loan contract"
            );
            let origin = if kind == UnitCallArgumentOwnershipKind::Value {
                contract.argument_span()
            } else {
                contract.loan_begin_span()
            };
            assert_eq!(primary_span.source_id(), origin.source_id());
            assert_eq!((origin.start(), origin.end()), label, "{name}");
            assert_eq!(unit.sources.slice(origin).unwrap(), label_text, "{name}");
            assert_eq!(labels(diagnostic), [(origin, label_message)], "{name}");
            assert_eq!(
                diagnostic.details().len(),
                1,
                "{name}: no extra cascade/details"
            );
            assert!(unit.owned.loans().is_empty());
            assert!(unit.owned.value_deliveries().is_empty());
            assert!(unit.owned.drops().is_empty());
            assert!(unit.owned.clone().validate().is_err());
            visited.push(name);
        });
    }
    assert_eq!(
        visited.len(),
        8,
        "only reached and checked cases count: {visited:?}"
    );
    assert!(
        legacy_rejections.is_empty(),
        "read-only nullable When subjects hit the old MoveOnly element gate: {legacy_rejections:?}; actually checked: {visited:?}"
    );
}
