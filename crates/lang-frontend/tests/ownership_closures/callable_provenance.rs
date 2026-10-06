//! 先用现有阶段 API 验证 provenance fixtures；尚不调用未实施的新接口。
use super::*;
use lang_frontend::type_checking::{TypeKind, standard_environments};

#[path = "callable_contract.rs"]
mod contract;

fn preflight(
    text: &str,
) -> (
    SourceMap,
    ParsedFile,
    NameResolution,
    TypedFile,
    OwnershipCheckedFile,
) {
    let result = analyzed_with(text, standard_environments());
    assert!(
        result.4.diagnostics().is_empty(),
        "{:?}",
        result.4.diagnostics()
    );
    assert!(result.4.deferred().is_empty(), "{:?}", result.4.deferred());
    result
}

const FUNCTIONS: &str = "fun identity(index: Int): Int = index\n\
                        fun other(index: Int): Int = index + 1\n\
                        fun take(callback: (Int) -> Int): Unit {}\n";

#[test]
fn callable_provenance_preflight_bare_function_alias_group_and_parameter() {
    let text = format!(
        "{FUNCTIONS}\
        fun relay(forwarded: (Int) -> Int): Unit {{ take((forwarded)) }}\n\
        fun use(): Unit {{\n\
            take(identity)\n\
            val named = identity\nval alias = (named)\ntake(alias)\n\
            val lambda: (Int) -> Int = {{ index -> index }}\n\
            val lambdaAlias = (lambda)\ntake(lambdaAlias)\n\
        }}"
    );
    let (_, parsed, names, typed, owned) = preflight(&text);
    let callback = symbol(&names, "forwarded");
    assert_eq!(typed.parameter_mode(callback), Some(ParameterMode::Borrow));
    for name in ["lambda", "lambdaAlias"] {
        assert!(
            matches!(
                typed
                    .symbol_type(symbol(&names, name))
                    .and_then(|ty| typed.types().get(ty)),
                Some(TypeKind::Function {
                    move_only: false,
                    ..
                })
            ),
            "{name}: {:?}",
            typed
                .symbol_type(symbol(&names, name))
                .and_then(|ty| typed.types().get(ty))
        );
    }
    for name in ["named", "alias"] {
        assert!(
            matches!(
                typed
                    .symbol_type(symbol(&names, name))
                    .and_then(|ty| typed.types().get(ty)),
                Some(TypeKind::Deferred(_))
            ),
            "unselected function names cannot define a concrete ABI"
        );
    }
    assert_eq!(lambdas(&parsed).len(), 1);
    assert!(
        owned
            .loans()
            .iter()
            .any(|loan| loan.kind() == LoanKind::Shared)
    );
    // Future origins retain Lambda and Parameter; unresolved bare function names stay unsupported.
}

#[test]
fn callable_provenance_preflight_caller_first_pointer_factories() {
    let text = format!(
        "{FUNCTIONS}\
        fun caller(): Unit {{\n\
            val result = factory()\nval alias = (result)\ntake(alias)\n\
            take(namedFactory())\n\
        }}\n\
        fun factory(): (Int) -> Int {{ println(\"factory\")\nreturn ({{ index -> index }}) }}\n\
        fun namedFactory(): (Int) -> Int = (identity)"
    );
    let (_, parsed, _, typed, owned) = preflight(&text);
    assert_eq!(lambdas(&parsed).len(), 1);
    assert!(owned.captures().is_empty());
    assert!(typed.calls().iter().any(|call| matches!(
        typed.types().get(call.return_type()),
        Some(TypeKind::Function { .. })
    )));
    // Future summaries keep both source identities and publish caller-first FactoryResult aliases.
}

#[test]
fn callable_provenance_preflight_legal_returns_outside_pointer_summary() {
    let text = format!(
        "{FUNCTIONS}\
        fun multiple(flag: Boolean): (Int) -> Int {{\n\
            if (flag) {{ return identity }}\nreturn other\n\
        }}\n\
        fun captured(own label: String): (Int) -> Boolean =\n\
            move {{ index -> label == \"captured\" && index == 0 }}"
    );
    let (_, parsed, _, _, owned) = preflight(&text);
    let lambda = lambdas(&parsed)[0];
    let captures = owned.captures_of(lambda).collect::<Vec<_>>();
    assert_eq!(captures.len(), 1);
    assert_eq!(captures[0].mode(), ClosureCaptureMode::Owned);
    // Future pointer summaries are absent for two normal returns and capturing returns.
}

#[test]
fn callable_provenance_preflight_same_and_different_origin_cfg() {
    let text = format!(
        "{FUNCTIONS}\
        fun use(flag: Boolean): Unit {{\n\
            val same: (Int) -> Int = if(flag) {{ identity }} else {{ identity }}\n\
            take(same)\n\
            val different: (Int) -> Int = if(flag) {{ identity }} else {{ other }}\n\
            take(different)\n\
        }}"
    );
    preflight(&text);
    // Future facts retain identity for same, but publish no guessed origin for different.
}

#[test]
fn callable_provenance_preflight_loop_alias_stable_overwrite_and_conflict() {
    for (prefix, replacement) in [
        ("", "identity"),
        ("callback = identity\n", "other"),
        ("", "other"),
    ] {
        let text = format!(
            "{FUNCTIONS}\
            fun use(flag: Boolean): Unit {{\n\
                var callback: (Int) -> Int = identity\n\
                while(flag) {{\n\
                    {prefix}\
                    val alias = (callback)\ntake((alias))\n\
                    callback = {replacement}\n\
                }}\n\
            }}"
        );
        preflight(&text);
        // The third case is Unknown across the backedge; the first two remain identity.
        // A single ownership traversal cannot itself prove these repeated-use origins.
    }
}

#[test]
fn callable_provenance_preflight_ownership_error_suppresses_executable_facts() {
    let text = "fun invalid(label: String): (Int) -> Boolean = move { index -> label == \"borrowed\" && index == 0 }";
    let (_, _, _, _, owned) = analyzed_with(text, standard_environments());
    assert_eq!(codes(&owned), ["L0138"]);
    assert!(owned.captures().is_empty());
    assert!(owned.loans().is_empty());
    assert!(owned.drops().is_empty());
    // Future provenance and pointer summaries must also be empty after a P3 error.
}

#[test]
fn callable_provenance_preflight_type_error_and_analysis_witness() {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source("bad-return.ko", "fun invalid(): (Int) -> Int = 1")
        .expect("source");
    let parsed = parse_file_twice(&sources, source, "invalid return fixture");
    assert!(parsed.diagnostics().is_empty());
    let (environment, type_environment) = standard_environments();
    let names = resolve_names(&sources, &parsed, &environment).expect("names");
    assert!(names.diagnostics().is_empty());
    let typed = check_types(&sources, &parsed, &names, &type_environment).expect("typed recovery");
    assert_eq!(
        typed
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0084"]
    );
    let owned = check_ownership(&sources, &parsed, &names, &typed).expect("ownership recovery");
    assert!(owned.is_compatible_with(&names, &typed));
    let fresh_names = resolve_names(&sources, &parsed, &environment).expect("fresh names");
    assert!(matches!(
        check_ownership(&sources, &parsed, &fresh_names, &typed),
        Err(lang_frontend::ownership_checking::OwnershipCheckingError::MismatchedAnalysisIdentity)
    ));
    // Single P3 accepts typed recovery; new facts must explicitly suppress this P2 error.
}
