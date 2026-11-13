//! Same-source Unit collector plus the independently reviewed formed-local exact contract.
//! All eighteen entries are collected before input/completeness or compatibility assertions.

use super::{LowerOutcome, Observation, Trace, print_outcome};
use crate::test_support::diagnostics::diagnostic;

mod inputs {
    //! Formation-specific public typed/ownership facts; no Resource Case or synthetic capability.

    use lang_frontend::{
        name_resolution::{
            SourceUnitId, UnitReferenceTarget, UnitSymbolId, ValidatedCompilationUnitNames,
        },
        ownership_checking::CompilationUnitOwnership,
        type_checking::{
            BuiltinType, CompilationUnitTypes, IntrinsicCallable, IntrinsicTypeConstructor,
            UnitExpressionId, UnitTypeId, UnitTypeKind, UnitTypeRefId,
        },
    };

    use super::{
        super::{ParsedFile, SourceMap, Trace},
        sources::{Case, Initializer, Shape},
        syntax,
    };

    fn unit_shape(typed: &CompilationUnitTypes, ty: UnitTypeId, shape: Shape) -> bool {
        let int = typed.types().builtin(BuiltinType::Int);
        if matches!(shape, Shape::Function) {
            return matches!(typed.types().get(ty), Some(UnitTypeKind::Function { move_only: false, parameters, return_type })
                if parameters.is_empty() && Some(*return_type) == typed.types().builtin(BuiltinType::Unit));
        }
        let inner = match shape {
            Shape::Map => ty,
            _ => match typed.types().get(ty) {
                Some(UnitTypeKind::Nullable(inner)) => *inner,
                _ => return false,
            },
        };
        let Some(UnitTypeKind::Intrinsic {
            constructor: IntrinsicTypeConstructor::MutableMap,
            arguments,
        }) = typed.types().get(inner)
        else {
            return false;
        };
        let [key, value] = arguments.as_slice() else {
            return false;
        };
        Some(*key) == int
            && if matches!(shape, Shape::NullableMapOfNullableInt) {
                matches!(typed.types().get(*value), Some(UnitTypeKind::Nullable(value)) if Some(*value) == int)
            } else {
                Some(*value) == int
            }
    }

    pub(super) fn unit(
        case: &Case,
        sources: &SourceMap,
        file: (SourceUnitId, &ParsedFile),
        names: &ValidatedCompilationUnitNames,
        typed: &CompilationUnitTypes,
        trace: &mut Trace,
    ) {
        unit_in_layout(case, sources, file, names, typed, true, trace);
    }

    pub(super) fn memory(
        case: &Case,
        sources: &SourceMap,
        file: (SourceUnitId, &ParsedFile),
        names: &ValidatedCompilationUnitNames,
        typed: &CompilationUnitTypes,
        trace: &mut Trace,
    ) {
        unit_in_layout(case, sources, file, names, typed, false, trace);
    }

    fn unit_in_layout(
        case: &Case,
        sources: &SourceMap,
        file: (SourceUnitId, &ParsedFile),
        names: &ValidatedCompilationUnitNames,
        typed: &CompilationUnitTypes,
        rooted: bool,
        trace: &mut Trace,
    ) {
        let resolutions: Vec<_> = names
            .names()
            .source_units()
            .iter()
            .filter(|source| {
                source.source_unit() == file.0
                    && source.resolution().source_id() == file.1.source_id()
            })
            .collect();
        trace.check(
            "one exact provider source-qualified name resolution",
            resolutions.len() == 1,
            &resolutions,
        );
        let [resolution] = resolutions.as_slice() else {
            return;
        };
        let locals = syntax::inspect(
            case,
            sources,
            file.1,
            resolution.resolution(),
            rooted,
            trace,
        );
        let symbols: Vec<Option<UnitSymbolId>> = locals
            .iter()
            .map(|local| {
                let candidates: Vec<_> = typed
                    .body_symbol_types()
                    .keys()
                    .copied()
                    .filter(|symbol| {
                        symbol.source_unit() == file.0 && symbol.symbol() == local.symbol
                    })
                    .collect();
                trace.check(
                    "same source-qualified body symbol identity",
                    candidates.len() == 1,
                    (&candidates, local),
                );
                match candidates.as_slice() {
                    [symbol] => Some(*symbol),
                    _ => None,
                }
            })
            .collect();
        let counts = [
            typed.map_constructions().len(),
            typed.map_sizes().len(),
            typed.map_contains_calls().len(),
            typed.map_gets().len(),
            typed.map_puts().len(),
            typed.map_removes().len(),
            typed.map_with_values().len(),
            typed.map_consumes().len(),
            typed.map_require_values().len(),
            typed.non_null_assertions().len(),
        ];
        trace.check(
            "formation exact qualified descriptor counts, no result pre-registration",
            counts == [case.constructions, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            counts,
        );
        for (local, symbol) in locals.iter().zip(&symbols) {
            let expression = UnitExpressionId::new(file.0, local.initializer);
            let storage = symbol.and_then(|symbol| typed.symbol_type(symbol));
            let actual = typed.expression_type(expression);
            let declared = local
                .type_ref
                .and_then(|id| typed.type_ref_type(UnitTypeRefId::new(file.0, id)));
            trace.check(
                "exact qualified local storage and declared type",
                storage.is_some_and(|ty| unit_shape(typed, ty, local.expected.shape))
                    && (local.type_ref.is_none() || declared == storage),
                (local.item, local, symbol, storage, declared, actual),
            );
            let valid = match local.expected.initializer {
                Initializer::EmptyMap => actual.is_some_and(|ty| unit_shape(typed, ty, Shape::Map))
                    && typed.map_construction(expression).is_some_and(|descriptor| {
                        descriptor.expression() == expression
                            && descriptor.callable() == IntrinsicCallable::MutableMapOf
                            && Some(descriptor.map_type()) == actual
                            && Some(descriptor.key_type()) == typed.types().builtin(BuiltinType::Int)
                            && Some(descriptor.value_type()) == typed.types().builtin(BuiltinType::Int)
                    }),
                Initializer::Null | Initializer::Lambda => actual.is_some() && actual == storage,
                Initializer::Return => actual == typed.types().builtin(BuiltinType::Nothing),
                Initializer::Name(index) => symbols.get(index).copied().flatten().is_some_and(|previous| {
                    let span = file.1.ast().expressions().get(local.initializer).ok().map(|node| node.span());
                    let references: Vec<_> = names.names().references().iter().filter(|reference|
                        reference.source_unit() == file.0 && Some(reference.span()) == span).collect();
                    trace.stages.push(format!("actual qualified move source={symbol:?}; local={local:#?}; references={references:#?}"));
                    actual.is_some() && actual == typed.symbol_type(previous)
                        && matches!(references.as_slice(), [reference] if matches!(reference.target(), UnitReferenceTarget::Symbol(symbol) if *symbol == previous))
                }),
            };
            trace.check(
                "initializer has exact qualified payload and source identity",
                valid,
                (local, symbol, actual, typed.map_construction(expression)),
            );
        }
        trace.stages.push(format!(
            "actual qualified formation locals={locals:#?}; symbols={symbols:#?}; constructions={:#?}",
            typed.map_constructions()
        ));
    }

    pub(super) fn unit_formed(
        case: &Case,
        sources: &SourceMap,
        file: (SourceUnitId, &ParsedFile),
        names: &ValidatedCompilationUnitNames,
        typed: &CompilationUnitTypes,
        owned: &CompilationUnitOwnership,
        trace: &mut Trace,
    ) {
        unit_formed_in_layout(case, sources, (file, true), names, typed, owned, trace);
    }

    pub(super) fn memory_formed(
        case: &Case,
        sources: &SourceMap,
        file: (SourceUnitId, &ParsedFile),
        names: &ValidatedCompilationUnitNames,
        typed: &CompilationUnitTypes,
        owned: &CompilationUnitOwnership,
        trace: &mut Trace,
    ) {
        unit_formed_in_layout(case, sources, (file, false), names, typed, owned, trace);
    }

    fn unit_formed_in_layout(
        case: &Case,
        sources: &SourceMap,
        (file, rooted): ((SourceUnitId, &ParsedFile), bool),
        names: &ValidatedCompilationUnitNames,
        typed: &CompilationUnitTypes,
        owned: &CompilationUnitOwnership,
        trace: &mut Trace,
    ) {
        let resolution = names.names().source_units().iter().find(|source| {
            source.source_unit() == file.0 && source.resolution().source_id() == file.1.source_id()
        });
        let Some(resolution) = resolution else {
            trace.check(
                "formed locals retain exact provider name resolution",
                false,
                file.0,
            );
            return;
        };
        let locals = syntax::inspect(
            case,
            sources,
            file.1,
            resolution.resolution(),
            rooted,
            trace,
        );
        let symbols: Vec<_> = locals
            .iter()
            .map(|local| {
                typed.body_symbol_types().keys().copied().find(|symbol| {
                    symbol.source_unit() == file.0 && symbol.symbol() == local.symbol
                })
            })
            .collect();
        inspect_formed(case, &symbols, owned.formed_owned_locals(), trace);
    }

    fn inspect_formed<Symbol: Copy + Ord + std::fmt::Debug>(
        case: &Case,
        symbols: &[Option<Symbol>],
        formed: &[Symbol],
        trace: &mut Trace,
    ) {
        // Source-ordered declarations come from exact NameMarker/symbol/scope queries above.
        // P2 retains seed and source after moves; K3 is analyzed despite constant false.
        // K4 includes the lambda-owned inner fact; K5 excludes only the inner dead symbol.
        let positions: &[usize] = match case.id {
            "P1" | "P3" | "K3" | "K5" => &[0],
            "P2" => &[0, 1, 2],
            "K4" | "K6" => &[0, 1],
            "K1" | "K2" => &[],
            _ => {
                trace.check("known formed-local source case", false, case.id);
                return;
            }
        };
        let mut expected: Vec<_> = positions
            .iter()
            .filter_map(|&position| symbols.get(position).copied().flatten())
            .collect();
        expected.sort_unstable();
        trace.check(
            "exact formed-owned local history from ordinary source-qualified symbols",
            expected.len() == positions.len() && formed == expected.as_slice(),
            (case.id, symbols, &expected, formed),
        );
        trace.check(
            "ordinary formed-owned local facts are sorted and unique",
            formed.windows(2).all(|pair| pair[0] < pair[1]),
            formed,
        );
    }
}
mod query {
    //! Exact ordinary formed-local demands and reviewed lower outcomes, separate from the collector.

    use super::super::{LowerOutcome, LoweringErrorKind};
    use crate::test_support::diagnostics::diagnostic;

    #[derive(Debug)]
    enum ExpectedLower {
        VerifiedLlvm,
        Rejected {
            kind: LoweringErrorKind,
            path: &'static str,
            start: usize,
            end: usize,
            text: &'static str,
        },
    }

    #[derive(Debug)]
    struct Expected {
        label: &'static str,
        entry_name: &'static str,
        entry_path: &'static str,
        lower: ExpectedLower,
    }

    // Fourteen LLVM successes plus exact K3/K4 capability rejections in both Unit layouts.
    // K4 retains each original null origin; the historical p/q MissingFact is not a contract.
    // Each row is independent of observed outcomes; no wildcard failure counts as preservation.
    const EXPECTED: &[Expected] = &[
        Expected {
            label: "P1/single",
            entry_name: "main",
            entry_path: "formed_p1_prefix_present.ko",
            lower: ExpectedLower::VerifiedLlvm,
        },
        Expected {
            label: "P1/unit",
            entry_name: "entry",
            entry_path: "q/consumer.ko",
            lower: ExpectedLower::VerifiedLlvm,
        },
        Expected {
            label: "P2/single",
            entry_name: "main",
            entry_path: "formed_p2_seed_then_move.ko",
            lower: ExpectedLower::VerifiedLlvm,
        },
        Expected {
            label: "P2/unit",
            entry_name: "entry",
            entry_path: "q/consumer.ko",
            lower: ExpectedLower::VerifiedLlvm,
        },
        Expected {
            label: "P3/single",
            entry_name: "main",
            entry_path: "formed_p3_branch_present.ko",
            lower: ExpectedLower::VerifiedLlvm,
        },
        Expected {
            label: "P3/unit",
            entry_name: "entry",
            entry_path: "q/consumer.ko",
            lower: ExpectedLower::VerifiedLlvm,
        },
        Expected {
            label: "K1/single",
            entry_name: "main",
            entry_path: "unreachable_nullable_map.ko",
            lower: ExpectedLower::VerifiedLlvm,
        },
        Expected {
            label: "K1/unit",
            entry_name: "entry",
            entry_path: "q/consumer.ko",
            lower: ExpectedLower::VerifiedLlvm,
        },
        Expected {
            label: "K2/single",
            entry_name: "main",
            entry_path: "formed_k2_initializer_return.ko",
            lower: ExpectedLower::VerifiedLlvm,
        },
        Expected {
            label: "K2/unit",
            entry_name: "entry",
            entry_path: "q/consumer.ko",
            lower: ExpectedLower::VerifiedLlvm,
        },
        Expected {
            label: "K3/single",
            entry_name: "main",
            entry_path: "formed_k3_constant_false_unsupported.ko",
            lower: ExpectedLower::Rejected {
                kind: LoweringErrorKind::UnsupportedNode,
                path: "formed_k3_constant_false_unsupported.ko",
                start: 50,
                end: 56,
                text: "branch",
            },
        },
        Expected {
            label: "K3/unit",
            entry_name: "entry",
            entry_path: "q/consumer.ko",
            lower: ExpectedLower::Rejected {
                kind: LoweringErrorKind::UnsupportedNode,
                path: "p/provider.ko",
                start: 64,
                end: 70,
                text: "branch",
            },
        },
        Expected {
            label: "K4/single",
            entry_name: "main",
            entry_path: "formed_k4_lambda_scope.ko",
            lower: ExpectedLower::Rejected {
                kind: LoweringErrorKind::UnsupportedNode,
                path: "formed_k4_lambda_scope.ko",
                start: 96,
                end: 100,
                text: "null",
            },
        },
        Expected {
            label: "K4/unit",
            entry_name: "entry",
            entry_path: "q/consumer.ko",
            lower: ExpectedLower::Rejected {
                kind: LoweringErrorKind::UnsupportedNode,
                path: "p/provider.ko",
                start: 110,
                end: 114,
                text: "null",
            },
        },
        Expected {
            label: "K5/single",
            entry_name: "main",
            entry_path: "formed_k5_shadow_dead_tail.ko",
            lower: ExpectedLower::VerifiedLlvm,
        },
        Expected {
            label: "K5/unit",
            entry_name: "entry",
            entry_path: "q/consumer.ko",
            lower: ExpectedLower::VerifiedLlvm,
        },
        Expected {
            label: "K6/single",
            entry_name: "main",
            entry_path: "formed_k6_loop_existing_header.ko",
            lower: ExpectedLower::VerifiedLlvm,
        },
        Expected {
            label: "K6/unit",
            entry_name: "entry",
            entry_path: "q/consumer.ko",
            lower: ExpectedLower::VerifiedLlvm,
        },
    ];

    fn matches_lower(expected: &ExpectedLower, actual: &LowerOutcome) -> bool {
        match (expected, actual) {
            (ExpectedLower::VerifiedLlvm, LowerOutcome::VerifiedLlvm) => true,
            (
                ExpectedLower::Rejected {
                    kind,
                    path,
                    start,
                    end,
                    text,
                },
                LowerOutcome::LowerRejected {
                    error,
                    location: Some(location),
                    ..
                },
            ) => {
                error.kind == *kind
                    && error
                        .span
                        .is_some_and(|span| span.start() == *start && span.end() == *end)
                    && location
                        .path
                        .as_ref()
                        .is_ok_and(|actual| actual.as_str() == *path)
                    && location
                        .text
                        .as_ref()
                        .is_ok_and(|actual| actual.as_str() == *text)
            }
            // Prerequisite, verifier, LLVM, missing-span and wrong-origin failures do not match.
            _ => false,
        }
    }

    #[test]
    fn nullable_map_formed_local_demands_preserve_and_advance_exact_outcomes() {
        let observations = super::collect(true);
        assert_eq!(observations.len(), EXPECTED.len());
        let mut mismatches = Vec::new();
        for (actual, expected) in observations.iter().zip(EXPECTED) {
            let matched = actual.label == expected.label
                && actual.outcome.as_ref().is_ok_and(|observed| {
                    observed.entry.name == expected.entry_name
                        && observed
                            .entry
                            .path
                            .as_ref()
                            .is_ok_and(|path| path.as_str() == expected.entry_path)
                        && !observed.entry.span.is_empty()
                        && matches_lower(&expected.lower, &observed.outcome)
                });
            diagnostic!("FORMATION EXACT {}: matched={matched}", expected.label);
            if !matched {
                mismatches.push(format!("expected={expected:#?}; actual={actual:#?}"));
            }
        }
        assert!(
            mismatches.is_empty(),
            "exact formed-local lower outcome mismatches:\n{}",
            mismatches.join("\n")
        );
    }
}
mod sources {
    //! Frozen reviewed nine-program source matrix; all strings are stored verbatim.

    #[derive(Clone, Copy, Debug)]
    pub(super) enum Shape {
        Map,
        NullableMap,
        NullableMapOfNullableInt,
        Function,
    }

    #[derive(Clone, Copy, Debug)]
    pub(super) enum Initializer {
        EmptyMap,
        Null,
        Return,
        Name(usize),
        Lambda,
    }

    #[derive(Clone, Copy, Debug)]
    pub(super) enum Owner {
        Function(&'static str),
        Lambda,
    }

    #[derive(Debug)]
    pub(super) struct Local {
        pub name: &'static str,
        pub shape: Shape,
        pub initializer: Initializer,
        pub owner: Owner,
    }

    pub(super) struct Case {
        pub id: &'static str,
        pub path: &'static str,
        pub single: &'static str,
        pub provider: &'static str,
        pub consumer: &'static str,
        pub constructions: usize,
        pub locals: &'static [Local],
    }

    pub(super) const CASES: &[Case] = &[
        Case {
            id: "P1",
            path: "formed_p1_prefix_present.ko",
            single: r#"fun main(): Unit {
      println("prefix")
      val source: MutableMap<Int, Int>? = mutableMapOf<Int, Int>()
    }
    "#,
            provider: r#"package p
    fun main(): Unit {
      println("prefix")
      val source: MutableMap<Int, Int>? = mutableMapOf<Int, Int>()
    }
    "#,
            consumer: r#"package q
    fun entry(): Unit { p.main() }
    "#,
            constructions: 1,
            locals: &[Local {
                name: "source",
                shape: Shape::NullableMap,
                initializer: Initializer::EmptyMap,
                owner: Owner::Function("main"),
            }],
        },
        Case {
            id: "P2",
            path: "formed_p2_seed_then_move.ko",
            single: r#"fun main(): Unit {
      val seed = mutableMapOf<Int, Int>()
      val source: MutableMap<Int, Int>? = seed
      val delivered = source
    }
    "#,
            provider: r#"package p
    fun main(): Unit {
      val seed = mutableMapOf<Int, Int>()
      val source: MutableMap<Int, Int>? = seed
      val delivered = source
    }
    "#,
            consumer: r#"package q
    fun entry(): Unit { p.main() }
    "#,
            constructions: 1,
            locals: &[
                Local {
                    name: "seed",
                    shape: Shape::Map,
                    initializer: Initializer::EmptyMap,
                    owner: Owner::Function("main"),
                },
                Local {
                    name: "source",
                    shape: Shape::NullableMap,
                    initializer: Initializer::Name(0),
                    owner: Owner::Function("main"),
                },
                Local {
                    name: "delivered",
                    shape: Shape::NullableMap,
                    initializer: Initializer::Name(1),
                    owner: Owner::Function("main"),
                },
            ],
        },
        Case {
            id: "P3",
            path: "formed_p3_branch_present.ko",
            single: r#"fun run(flag: Boolean): Unit {
      if (flag) {
        val branch: MutableMap<Int, Int>? = mutableMapOf<Int, Int>()
      }
    }
    fun main(): Unit { run(true) }
    "#,
            provider: r#"package p
    fun run(flag: Boolean): Unit {
      if (flag) {
        val branch: MutableMap<Int, Int>? = mutableMapOf<Int, Int>()
      }
    }
    fun main(): Unit { run(true) }
    "#,
            consumer: r#"package q
    fun entry(): Unit { p.main() }
    "#,
            constructions: 1,
            locals: &[Local {
                name: "branch",
                shape: Shape::NullableMap,
                initializer: Initializer::EmptyMap,
                owner: Owner::Function("run"),
            }],
        },
        Case {
            id: "K1",
            path: "unreachable_nullable_map.ko",
            single: r#"fun main(): Unit { return; val dead: MutableMap<Int, Int?>? = null }
    "#,
            provider: r#"package p
    fun main(): Unit { return; val dead: MutableMap<Int, Int?>? = null }
    "#,
            consumer: r#"package q
    fun entry(): Unit { p.main() }
    "#,
            constructions: 0,
            locals: &[Local {
                name: "dead",
                shape: Shape::NullableMapOfNullableInt,
                initializer: Initializer::Null,
                owner: Owner::Function("main"),
            }],
        },
        Case {
            id: "K2",
            path: "formed_k2_initializer_return.ko",
            single: r#"fun main(): Unit {
      val dead: MutableMap<Int, Int?>? = return
    }
    "#,
            provider: r#"package p
    fun main(): Unit {
      val dead: MutableMap<Int, Int?>? = return
    }
    "#,
            consumer: r#"package q
    fun entry(): Unit { p.main() }
    "#,
            constructions: 0,
            locals: &[Local {
                name: "dead",
                shape: Shape::NullableMapOfNullableInt,
                initializer: Initializer::Return,
                owner: Owner::Function("main"),
            }],
        },
        Case {
            id: "K3",
            path: "formed_k3_constant_false_unsupported.ko",
            single: r#"fun main(): Unit {
      if (false) {
        val branch: MutableMap<Int, Int?>? = null
      }
    }
    "#,
            provider: r#"package p
    fun main(): Unit {
      if (false) {
        val branch: MutableMap<Int, Int?>? = null
      }
    }
    "#,
            consumer: r#"package q
    fun entry(): Unit { p.main() }
    "#,
            constructions: 0,
            locals: &[Local {
                name: "branch",
                shape: Shape::NullableMapOfNullableInt,
                initializer: Initializer::Null,
                owner: Owner::Function("main"),
            }],
        },
        Case {
            id: "K4",
            path: "formed_k4_lambda_scope.ko",
            single: r#"fun main(): Unit {
      val action: () -> Unit = {
        val inner: MutableMap<Int, Int?>? = null
      }
    }
    "#,
            provider: r#"package p
    fun main(): Unit {
      val action: () -> Unit = {
        val inner: MutableMap<Int, Int?>? = null
      }
    }
    "#,
            consumer: r#"package q
    fun entry(): Unit { p.main() }
    "#,
            constructions: 0,
            locals: &[
                Local {
                    name: "action",
                    shape: Shape::Function,
                    initializer: Initializer::Lambda,
                    owner: Owner::Function("main"),
                },
                Local {
                    name: "inner",
                    shape: Shape::NullableMapOfNullableInt,
                    initializer: Initializer::Null,
                    owner: Owner::Lambda,
                },
            ],
        },
        Case {
            id: "K5",
            path: "formed_k5_shadow_dead_tail.ko",
            single: r#"fun main(): Unit {
      val source: MutableMap<Int, Int>? = null;
      {
        return
        val source: MutableMap<Int, Int?>? = null
      }
    }
    "#,
            provider: r#"package p
    fun main(): Unit {
      val source: MutableMap<Int, Int>? = null;
      {
        return
        val source: MutableMap<Int, Int?>? = null
      }
    }
    "#,
            consumer: r#"package q
    fun entry(): Unit { p.main() }
    "#,
            constructions: 0,
            locals: &[
                Local {
                    name: "source",
                    shape: Shape::NullableMap,
                    initializer: Initializer::Null,
                    owner: Owner::Function("main"),
                },
                Local {
                    name: "source",
                    shape: Shape::NullableMapOfNullableInt,
                    initializer: Initializer::Null,
                    owner: Owner::Function("main"),
                },
            ],
        },
        Case {
            id: "K6",
            path: "formed_k6_loop_existing_header.ko",
            single: r#"fun run(flag: Boolean): Unit {
      val warm: MutableMap<Int, Int>? = null
      while (flag) {
        val local: MutableMap<Int, Int>? = null
        break
      }
    }
    fun main(): Unit { run(false) }
    "#,
            provider: r#"package p
    fun run(flag: Boolean): Unit {
      val warm: MutableMap<Int, Int>? = null
      while (flag) {
        val local: MutableMap<Int, Int>? = null
        break
      }
    }
    fun main(): Unit { run(false) }
    "#,
            consumer: r#"package q
    fun entry(): Unit { p.main() }
    "#,
            constructions: 0,
            locals: &[
                Local {
                    name: "warm",
                    shape: Shape::NullableMap,
                    initializer: Initializer::Null,
                    owner: Owner::Function("run"),
                },
                Local {
                    name: "local",
                    shape: Shape::NullableMap,
                    initializer: Initializer::Null,
                    owner: Owner::Function("run"),
                },
            ],
        },
    ];
}
mod syntax;

fn collect(with_formed_queries: bool) -> Vec<Observation> {
    let mut observations = Vec::new();
    for case in sources::CASES {
        for unit in [false, true] {
            let mut trace = Trace::default();
            let outcome = if unit {
                super::unit::observe_sources(
                    case.provider,
                    case.consumer,
                    |sources, file, names, typed, trace| {
                        inputs::unit(case, sources, file, names, typed, trace)
                    },
                    |sources, file, names, typed, owned, trace| {
                        if with_formed_queries {
                            inputs::unit_formed(case, sources, file, names, typed, owned, trace);
                        }
                    },
                    0,
                    &mut trace,
                )
            } else {
                super::memory::observe_source(
                    case.path,
                    case.single,
                    |sources, file, names, typed, trace| {
                        inputs::memory(case, sources, file, names, typed, trace)
                    },
                    |sources, file, names, typed, owned, trace| {
                        inputs::memory_formed(case, sources, file, names, typed, owned, trace)
                    },
                    0,
                    |_, _, _, _, _| {},
                    &mut trace,
                )
            };
            observations.push(Observation {
                label: format!("{}/{}", case.id, if unit { "unit" } else { "single" }),
                trace,
                outcome,
            });
        }
    }
    for observation in &observations {
        diagnostic!(
            "FORMATION BASELINE {}\n{}",
            observation.label,
            observation.trace.stages.join("\n")
        );
        diagnostic!("input failures={:?}", observation.trace.input_failures);
        if let Ok(observed) = &observation.outcome {
            diagnostic!("actual selected entry={:#?}", observed.entry);
        }
        print_outcome(&observation.outcome);
    }
    // No source is skipped or asserted before all eighteen observations are printed.
    assert_eq!(observations.len(), 18);
    let labels: Vec<_> = observations
        .iter()
        .map(|observation| observation.label.as_str())
        .collect();
    assert_eq!(
        labels,
        [
            "P1/single",
            "P1/unit",
            "P2/single",
            "P2/unit",
            "P3/single",
            "P3/unit",
            "K1/single",
            "K1/unit",
            "K2/single",
            "K2/unit",
            "K3/single",
            "K3/unit",
            "K4/single",
            "K4/unit",
            "K5/single",
            "K5/unit",
            "K6/single",
            "K6/unit",
        ]
    );
    assert!(
        observations
            .iter()
            .all(|observation| observation.outcome.is_ok()
                && observation.trace.input_failures.is_empty()),
        "all eighteen source/FE/input/ownership gates must pass and actual lowering must be attempted"
    );
    observations
}

#[test]
fn nullable_map_formed_local_sources_collect_baseline_outcomes() {
    let observations = collect(false);
    // Historical collection-only contract: non-K1 lower outcomes remain observations.
    let frozen: Vec<_> = observations
        .iter()
        .filter(|observation| matches!(observation.label.as_str(), "K1/single" | "K1/unit"))
        .collect();
    assert_eq!(frozen.len(), 2);
    assert!(
        frozen.iter().all(
            |observation| observation.outcome.as_ref().is_ok_and(|actual| {
                let (name, path) = if observation.label == "K1/single" {
                    ("main", "unreachable_nullable_map.ko")
                } else {
                    ("entry", "q/consumer.ko")
                };
                actual.entry.name == name
                    && actual
                        .entry
                        .path
                        .as_ref()
                        .is_ok_and(|actual| actual == path)
                    && matches!(&actual.outcome, LowerOutcome::VerifiedLlvm)
            })
        ),
        "the previously frozen K1 dead-tail LLVM successes must be preserved"
    );
    diagnostic!(
        "COLLECTION ONLY: all eighteen ordinary entries attempted lowering; K1 retains two LLVM successes. Other outcomes are observations only; original MemoryDocument facts are checked without freezing other lower outcomes."
    );
}
