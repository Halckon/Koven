//! Host-specific tools and object contracts shared by native regression tests.

pub(crate) mod diagnostics {
    //! Full test evidence stays in per-thread files; libtest receives bounded summaries.
    use std::{
        cell::RefCell,
        fmt,
        fs::{File, OpenOptions},
        io::{self, BufWriter, Write},
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };

    struct Log {
        path: PathBuf,
        file: BufWriter<File>,
    }

    impl Log {
        fn create() -> io::Result<Self> {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            loop {
                let id = NEXT.fetch_add(1, Ordering::Relaxed);
                let path = std::env::temp_dir().join(format!(
                    "koven-codegen-diagnostics-{}-{id}.log",
                    std::process::id()
                ));
                match OpenOptions::new().write(true).create_new(true).open(&path) {
                    Ok(file) => {
                        let mut file = BufWriter::new(file);
                        writeln!(
                            file,
                            "test: {}",
                            std::thread::current().name().unwrap_or("unnamed")
                        )?;
                        return Ok(Self { path, file });
                    }
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                    Err(error) => return Err(error),
                }
            }
        }

        fn append(&mut self, args: fmt::Arguments<'_>) -> io::Result<()> {
            // Debug emits many small fragments. Buffer them, then expose each complete record.
            writeln!(self.file, "{args}")?;
            self.file.flush()
        }
    }

    thread_local! {
        static LOG: RefCell<Option<Log>> = const { RefCell::new(None) };
    }

    /// Persist all bytes before publishing a path; never silently discard evidence.
    pub(crate) fn record(args: fmt::Arguments<'_>) -> PathBuf {
        LOG.with_borrow_mut(|slot| {
            let first = slot.is_none();
            if first {
                *slot = Some(Log::create().expect("create codegen test diagnostic file"));
            }
            let log = slot.as_mut().expect("diagnostic file initialized");
            log.append(args).unwrap_or_else(|error| {
                panic!("write test diagnostics {}: {error}", log.path.display())
            });
            if first {
                println!("full test diagnostics: {}", log.path.display());
            }
            log.path.clone()
        })
    }

    /// Keep the original assertion detail in the file, including multiline IR and facts.
    pub(crate) fn failure(args: fmt::Arguments<'_>) -> String {
        let detail = args.to_string();
        let path = record(format_args!("ASSERTION FAILURE:\n{detail}"));
        format!("{}; full diagnostics: {}", preview(&detail), path.display())
    }

    /// Like Result::expect, but persist an arbitrarily large error before panicking.
    #[track_caller]
    pub(crate) fn expect<T, E: fmt::Debug>(result: Result<T, E>, message: &str) -> T {
        match result {
            Ok(value) => value,
            Err(error) => panic!("{}", failure(format_args!("{message}: {error:?}"))),
        }
    }

    /// Compare every byte; large differences belong in the diagnostic artifact.
    #[track_caller]
    pub(crate) fn assert_text_equal(left: &str, right: &str, context: fmt::Arguments<'_>) {
        if left == right {
            return;
        }
        let context = context.to_string();
        let path = record(format_args!(
            "TEXT EQUALITY FAILURE: {context}\nLEFT ({} bytes):\n{left}\nRIGHT ({} bytes):\n{right}",
            left.len(),
            right.len(),
        ));
        let offset = left
            .bytes()
            .zip(right.bytes())
            .position(|(a, b)| a != b)
            .unwrap_or(left.len().min(right.len()));
        let boundary = char_boundary(left, offset);
        let prefix = &left[..boundary];
        let line = prefix.bytes().filter(|&byte| byte == b'\n').count() + 1;
        let column = prefix.rsplit('\n').next().unwrap_or("").chars().count() + 1;
        panic!(
            "text mismatch at byte {offset}, line {line}, column {column}; {}: left={} bytes next={:?} [{}]; right={} bytes next={:?} [{}]; full diagnostics: {}",
            preview(&context),
            left.len(),
            left[boundary..].chars().next(),
            excerpt(left, offset),
            right.len(),
            right[char_boundary(right, offset)..].chars().next(),
            excerpt(right, offset),
            path.display(),
        );
    }

    // A differing byte may be inside a UTF-8 code point; excerpts must use safe boundaries.
    fn char_boundary(text: &str, offset: usize) -> usize {
        let mut boundary = offset;
        while !text.is_char_boundary(boundary) {
            boundary -= 1;
        }
        boundary
    }

    fn excerpt(text: &str, offset: usize) -> String {
        let boundary = char_boundary(text, offset);
        let start = text[..boundary]
            .char_indices()
            .rev()
            .nth(32)
            .map_or(0, |(index, _)| index);
        preview(&text[start..])
    }

    macro_rules! assert_text_eq {
    ($left:expr, $right:expr $(,)?) => {
        $crate::test_support::diagnostics::assert_text_eq!($left, $right, "")
    };
    ($left:expr, $right:expr, $($args:tt)+) => {
        match (&$left, &$right) {
            (left, right) => {
                if !(*left == *right) {
                    $crate::test_support::diagnostics::assert_text_equal(
                        left, right, format_args!($($args)+),
                    );
                }
            }
        }
    };
}
    pub(crate) use assert_text_eq;

    fn preview(detail: &str) -> String {
        let mut chars = detail.chars();
        let mut result: String = chars
            .by_ref()
            .take(240)
            .map(|ch| if ch.is_control() { ' ' } else { ch })
            .collect();
        if chars.next().is_some() {
            result.push_str(" … [truncated]");
        }
        result
    }

    macro_rules! diagnostic {
    ($($args:tt)*) => {{
        $crate::test_support::diagnostics::record(format_args!($($args)*));
    }};
}
    pub(crate) use diagnostic;

    macro_rules! assert_diagnostic {
    ($condition:expr, $($args:tt)+) => {
        if !$condition {
            panic!("{}", $crate::test_support::diagnostics::failure(format_args!($($args)+)));
        }
    };
}
    pub(crate) use assert_diagnostic;

    // Keep ordinary equality semantics and lazy details, including non-text values.
    macro_rules! assert_eq_diagnostic {
    ($left:expr, $right:expr, $($args:tt)+) => {
        match (&$left, &$right) {
            (left, right) => {
                if !(*left == *right) {
                    panic!("{}", $crate::test_support::diagnostics::failure(format_args!(
                        "assertion `left == right` failed\nleft: {left:?}\nright: {right:?}\n{}",
                        format_args!($($args)+),
                    )));
                }
            }
        }
    };
}
    pub(crate) use assert_eq_diagnostic;

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn long_unicode_and_multiline_failure_is_bounded_but_preserved_in_file() {
            let detail = format!("case=unicode stage=LLVM\n{}\nEND", "界🚀\t".repeat(10000));
            let message = failure(format_args!("{detail}"));
            assert!(
                message.chars().count() < 600,
                "terminal failure must stay bounded"
            );
            assert!(!message.contains('\n'), "terminal preview must be one line");
            LOG.with_borrow(|slot| {
                let contents = std::fs::read_to_string(&slot.as_ref().unwrap().path).unwrap();
                assert!(
                    contents.contains(&detail),
                    "full Unicode evidence must survive"
                );
            });
        }

        #[test]
        fn records_append_without_truncation_and_threads_have_distinct_files() {
            let paths = (0..4)
                .map(|_| {
                    std::thread::spawn(|| {
                        diagnostic!("header");
                        let first = record(format_args!("first\nline"));
                        let second = record(format_args!("last"));
                        assert_eq!(first, second);
                        let contents = std::fs::read_to_string(&first).unwrap();
                        assert!(contents.ends_with("first\nline\nlast\n"));
                        first
                    })
                })
                .collect::<Vec<_>>()
                .into_iter()
                .map(|h| h.join().unwrap())
                .collect::<Vec<_>>();
            let mut unique = paths.clone();
            unique.sort();
            unique.dedup();
            assert_eq!(
                unique.len(),
                paths.len(),
                "parallel evidence must not overwrite"
            );
        }

        #[test]
        fn successful_assertion_does_not_format_failure_detail() {
            struct Unformattable;
            impl fmt::Display for Unformattable {
                fn fmt(&self, _: &mut fmt::Formatter<'_>) -> fmt::Result {
                    panic!("success must not format detail")
                }
            }
            assert_diagnostic!(true, "{}", Unformattable);
        }

        #[test]
        fn expect_preserves_success_and_bounds_the_original_error() {
            assert_eq!(expect(Ok::<_, String>(42), "success"), 42);
            let detail = "original LLVM error".repeat(1000);
            let panic = std::panic::catch_unwind(|| expect(Err::<(), _>(&detail), "case=bad"));
            let message = panic.unwrap_err().downcast::<String>().unwrap();
            assert!(message.contains("case=bad"));
            assert!(message.len() < 1500);
            LOG.with_borrow(|slot| {
                let contents = std::fs::read_to_string(&slot.as_ref().unwrap().path).unwrap();
                assert!(contents.contains(&detail));
            });
        }

        #[test]
        fn write_failure_is_not_silently_accepted() {
            let mut log = Log::create().unwrap();
            log.file = BufWriter::new(File::open(&log.path).unwrap());
            assert!(log.append(format_args!("evidence")).is_err());
        }

        #[test]
        fn assertion_still_panics_and_preserves_complete_failure() {
            let payload = "failure tail".repeat(1000);
            let panic =
                std::panic::catch_unwind(|| assert_diagnostic!(false, "case=bad {payload}"));
            let message = panic.unwrap_err().downcast::<String>().unwrap();
            assert!(message.contains("case=bad"));
            assert!(message.len() < 1500);
            LOG.with_borrow(|slot| {
                let contents = std::fs::read_to_string(&slot.as_ref().unwrap().path).unwrap();
                assert!(contents.contains(&payload));
            });
        }

        #[test]
        fn text_difference_retains_both_inputs_and_reports_bounded_context() {
            let left = format!("header\n{}LEFT\n", "same".repeat(10000));
            let right = format!("header\n{}RIGHT\n", "same".repeat(10000));
            let panic = std::panic::catch_unwind(|| assert_text_eq!(left, right, "IR determinism"));
            let message = panic.unwrap_err().downcast::<String>().unwrap();
            assert!(
                message.len() < 2000,
                "mismatch must not dump both whole programs"
            );
            assert!(message.contains("line 2"));
            assert!(message.contains("LEFT") && message.contains("RIGHT"));
            LOG.with_borrow(|slot| {
                let contents = std::fs::read_to_string(&slot.as_ref().unwrap().path).unwrap();
                assert!(contents.contains(&left) && contents.contains(&right));
                assert!(contents.contains("IR determinism"));
            });
        }

        #[test]
        fn text_equality_is_exact_for_newlines_empty_and_unicode() {
            for (left, right) in [("", "x"), ("x", "x\n"), ("甲", "乙"), ("a\r\n", "a\n")] {
                let panic = std::panic::catch_unwind(|| assert_text_eq!(left, right));
                let message = panic.unwrap_err().downcast::<String>().unwrap();
                assert!(message.contains("text mismatch"));
                assert!(message.len() < 2000);
            }
            assert_text_eq!("甲\n", "甲\n");
        }

        #[test]
        fn text_operands_are_evaluated_once_and_success_does_not_format_details() {
            let calls = std::cell::Cell::new(0);
            let input = || {
                calls.set(calls.get() + 1);
                String::from("same")
            };
            let detail = || -> String { panic!("equal text must not evaluate failure detail") };
            assert_text_eq!(input(), input(), "{}", detail());
            assert_eq!(calls.get(), 2);
            LOG.with_borrow(|slot| assert!(slot.is_none(), "success must not create evidence"));
        }

        #[test]
        fn equality_failure_preserves_values_and_large_context() {
            let detail = "full LLVM detail".repeat(1000);
            let panic = std::panic::catch_unwind(|| assert_eq_diagnostic!(1, 2, "{detail}"));
            let message = panic.unwrap_err().downcast::<String>().unwrap();
            assert!(message.len() < 1500);
            LOG.with_borrow(|slot| {
                let contents = std::fs::read_to_string(&slot.as_ref().unwrap().path).unwrap();
                assert!(contents.contains("left: 1\nright: 2"));
                assert!(contents.contains(&detail));
            });
        }

        #[test]
        fn equality_success_evaluates_operands_once_but_not_failure_details() {
            let calls = std::cell::Cell::new(0);
            let input = || {
                calls.set(calls.get() + 1);
                42
            };
            let detail = || -> String { panic!("equal values must not evaluate details") };
            assert_eq_diagnostic!(input(), input(), "{}", detail());
            assert_eq!(calls.get(), 2);
            LOG.with_borrow(|slot| assert!(slot.is_none()));
        }

        #[test]
        fn equality_macro_uses_eq_like_the_standard_assertion() {
            #[derive(Debug)]
            struct UnusualEquality;
            // Deliberately disagree so this test detects substituting ne for standard assert_eq.
            #[allow(clippy::partialeq_ne_impl)]
            impl PartialEq for UnusualEquality {
                fn eq(&self, _: &Self) -> bool {
                    true
                }
                fn ne(&self, _: &Self) -> bool {
                    true
                }
            }
            assert_eq!(UnusualEquality, UnusualEquality);
            assert_eq_diagnostic!(UnusualEquality, UnusualEquality, "must use eq");
        }
    }
}

use std::path::PathBuf;

/// `String.clone()` lowers only in the refinement layer (SPEC-0394). A source forced onto
/// the per-family path (`with_per_family_lowering`) forms its owned strings by
/// concatenation instead: `"text".clone()` becomes `("text" + "")`.
pub(crate) fn per_family_source(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find("\".clone()") {
        let open = rest[..at]
            .rfind('"')
            .expect("string literal before .clone()");
        out.push_str(&rest[..open]);
        out.push('(');
        out.push_str(&rest[open..=at]);
        out.push_str(" + \"\")");
        rest = &rest[at + "\".clone()".len()..];
    }
    out.push_str(rest);
    out
}

pub(crate) fn clang() -> PathBuf {
    if cfg!(target_os = "macos") {
        PathBuf::from("/usr/bin/clang")
    } else {
        ir_clang()
    }
}

// Instrumented LLVM IR must be read by the same LLVM major that emitted it.
// The system macOS driver is still used above when linking existing objects.
pub(crate) fn ir_clang() -> PathBuf {
    std::env::var_os("LLVM_SYS_211_PREFIX")
        .map(PathBuf::from)
        .map(|prefix| prefix.join("bin/clang"))
        .filter(|path| path.is_file())
        .unwrap_or_else(|| PathBuf::from("clang"))
}

pub(crate) fn assert_native_object(bytes: &[u8]) {
    if cfg!(target_os = "macos") {
        assert_eq!(&bytes[..4], b"\xcf\xfa\xed\xfe");
        assert_eq!(&bytes[4..8], &[0x0c, 0x00, 0x00, 0x01]); // CPU_TYPE_ARM64
        assert_eq!(&bytes[12..16], &[0x01, 0x00, 0x00, 0x00]); // MH_OBJECT
    } else {
        assert_eq!(&bytes[..4], b"\x7fELF");
        assert_eq!(bytes[4], 2); // ELFCLASS64
        assert_eq!(bytes[5], 1); // ELFDATA2LSB
        assert_eq!(&bytes[16..18], &[1, 0]); // ET_REL
        assert_eq!(&bytes[18..20], &[62, 0]); // EM_X86_64
    }
}

pub(crate) fn main_symbol() -> &'static str {
    if cfg!(target_os = "macos") {
        "_main"
    } else {
        "main"
    }
}

/// Removes the SPEC-0355 abort report helpers (`@koven.abort.*` definitions and their
/// private text globals) so IR assertions inspect only the code under test.
pub(crate) fn without_abort_runtime(llvm: &str) -> String {
    let mut kept = String::with_capacity(llvm.len());
    let mut skipping = false;
    for line in llvm.lines() {
        if skipping {
            if line == "}" {
                skipping = false;
            }
            continue;
        }
        if line.starts_with("define ") && line.contains("@koven.abort.") {
            skipping = true;
            continue;
        }
        if line.starts_with("@abort.") {
            continue;
        }
        kept.push_str(line);
        kept.push('\n');
    }
    kept
}

pub(crate) mod native_nested_borrow_cases {
    //! SPEC-0307 ordinary source cases shared by SSA and unmodified native tests.
    pub(crate) struct Case {
        pub name: &'static str,
        pub source: &'static str,
        pub stdout: &'static [u8],
        pub nested: bool,
    }

    pub(crate) fn cases() -> Vec<Case> {
        vec![
            Case {
                name: "single_direct",
                source: include_str!("../tests/fixtures/nested_borrow/single_direct.ko"),
                stdout: b"kept\nkept\n",
                nested: true,
            },
            Case {
                name: "single_explicit_control",
                source: include_str!("../tests/fixtures/nested_borrow/single_explicit_control.ko"),
                stdout: b"kept\nkept\n",
                nested: false,
            },
            Case {
                name: "single_grouped",
                source: include_str!("../tests/fixtures/nested_borrow/single_grouped.ko"),
                stdout: b"kept\nkept\n",
                nested: true,
            },
            Case {
                name: "multi_source_first",
                source: include_str!("../tests/fixtures/nested_borrow/multi_source_first.ko"),
                stdout: b"kept\nkept\n",
                nested: true,
            },
            Case {
                name: "multi_source_middle",
                source: include_str!("../tests/fixtures/nested_borrow/multi_source_middle.ko"),
                stdout: b"kept\nkept\n",
                nested: true,
            },
            Case {
                name: "multi_source_last",
                source: include_str!("../tests/fixtures/nested_borrow/multi_source_last.ko"),
                stdout: b"kept\nkept\n",
                nested: true,
            },
            Case {
                name: "mixed_copyable_aux",
                source: include_str!("../tests/fixtures/nested_borrow/mixed_copyable_aux.ko"),
                stdout: b"kept\nkept\n",
                nested: true,
            },
            Case {
                name: "mixed_resource_aux",
                source: include_str!("../tests/fixtures/nested_borrow/mixed_resource_aux.ko"),
                stdout: b"drop-aux\nkept\nkept\n",
                nested: true,
            },
            Case {
                name: "mixed_resource_explicit_control",
                source: include_str!(
                    "../tests/fixtures/nested_borrow/mixed_resource_explicit_control.ko"
                ),
                stdout: b"drop-aux\nkept\nkept\n",
                nested: false,
            },
            Case {
                name: "inner_named_reverse",
                source: include_str!("../tests/fixtures/nested_borrow/inner_named_reverse.ko"),
                stdout: b"eval-right\neval-left\nkept\nkept\n",
                nested: true,
            },
            Case {
                name: "outer_named_reverse",
                source: include_str!("../tests/fixtures/nested_borrow/outer_named_reverse.ko"),
                stdout: b"eval-tail\nkept\ntail\nkept\n",
                nested: true,
            },
            Case {
                name: "same_root_inner",
                source: include_str!("../tests/fixtures/nested_borrow/same_root_inner.ko"),
                stdout: b"kept\nkept\n",
                nested: true,
            },
            Case {
                name: "same_root_outer",
                source: include_str!("../tests/fixtures/nested_borrow/same_root_outer.ko"),
                stdout: b"kept\nkept\nkept\n",
                nested: true,
            },
            Case {
                name: "same_root_outer_explicit_control",
                source: include_str!(
                    "../tests/fixtures/nested_borrow/same_root_outer_explicit_control.ko"
                ),
                stdout: b"kept\nkept\nkept\n",
                nested: false,
            },
            Case {
                name: "different_roots_outer",
                source: include_str!("../tests/fixtures/nested_borrow/different_roots_outer.ko"),
                stdout: b"left\nright\nleft\nright\n",
                nested: true,
            },
            Case {
                name: "inner_borrow_and_owned_resource",
                source: include_str!(
                    "../tests/fixtures/nested_borrow/inner_borrow_and_owned_resource.ko"
                ),
                stdout: b"drop-aux\ndrop-temp\nkept\nkept\n",
                nested: true,
            },
            Case {
                name: "outer_borrow_temp_after",
                source: include_str!("../tests/fixtures/nested_borrow/outer_borrow_temp_after.ko"),
                stdout: b"kept\ndrop-temp\nafter\nkept\n",
                nested: true,
            },
            Case {
                name: "outer_borrow_temp_before",
                source: include_str!("../tests/fixtures/nested_borrow/outer_borrow_temp_before.ko"),
                stdout: b"kept\ndrop-temp\nafter\nkept\n",
                nested: true,
            },
            Case {
                name: "outer_owned_temp",
                source: include_str!("../tests/fixtures/nested_borrow/outer_owned_temp.ko"),
                stdout: b"kept\ndrop-aux\nafter\nkept\n",
                nested: true,
            },
            Case {
                name: "outer_temp_explicit_control",
                source: include_str!(
                    "../tests/fixtures/nested_borrow/outer_temp_explicit_control.ko"
                ),
                stdout: b"kept\ndrop-aux\ndrop-temp\nafter\nkept\n",
                nested: false,
            },
            Case {
                name: "wrapper_direct",
                source: include_str!("../tests/fixtures/nested_borrow/wrapper_direct.ko"),
                stdout: b"kept\nkept\n",
                nested: true,
            },
            Case {
                name: "wrapper_explicit_control",
                source: include_str!("../tests/fixtures/nested_borrow/wrapper_explicit_control.ko"),
                stdout: b"kept\nkept\n",
                nested: false,
            },
            Case {
                name: "generic_copyable",
                source: include_str!("../tests/fixtures/nested_borrow/generic_copyable.ko"),
                stdout: b"seven\nseven\n",
                nested: true,
            },
            Case {
                name: "generic_moveonly",
                source: include_str!("../tests/fixtures/nested_borrow/generic_moveonly.ko"),
                stdout: b"kept\nkept\n",
                nested: true,
            },
            Case {
                name: "field_projection",
                source: include_str!("../tests/fixtures/nested_borrow/field_projection.ko"),
                stdout: b"field\nconsume-source\ndrop-source\n",
                nested: true,
            },
            Case {
                name: "reuse_existing_borrow",
                source: include_str!("../tests/fixtures/nested_borrow/reuse_existing_borrow.ko"),
                stdout: b"kept\nkept\nkept\n",
                nested: true,
            },
            Case {
                name: "outer_owned_copyable_result",
                source: include_str!(
                    "../tests/fixtures/nested_borrow/outer_owned_copyable_result.ko"
                ),
                stdout: b"kept\nkept\n",
                nested: true,
            },
            Case {
                name: "copyable_explicit_owned_control",
                source: include_str!(
                    "../tests/fixtures/nested_borrow/copyable_explicit_owned_control.ko"
                ),
                stdout: b"seven\nseven\n",
                nested: false,
            },
            Case {
                name: "outer_later_operand_reads_root",
                source: include_str!(
                    "../tests/fixtures/nested_borrow/outer_later_operand_reads_root.ko"
                ),
                stdout: b"eval-read\nkept\nkept\n",
                nested: true,
            },
            Case {
                name: "field_projection_explicit_control",
                source: include_str!(
                    "../tests/fixtures/nested_borrow/field_projection_explicit_control.ko"
                ),
                stdout: b"field\nconsume-source\ndrop-source\n",
                nested: false,
            },
            Case {
                name: "same_root_existing_parent_control",
                source: include_str!(
                    "../tests/fixtures/nested_borrow/same_root_existing_parent_control.ko"
                ),
                stdout: b"kept\nkept\nkept\n",
                nested: false,
            },
            Case {
                name: "asap_same_root_twice",
                source: include_str!("../tests/fixtures/nested_borrow/asap_same_root_twice.ko"),
                stdout: b"kept\nkept\nafter\n",
                nested: true,
            },
            Case {
                name: "resource_lexical_source",
                source: include_str!("../tests/fixtures/nested_borrow/resource_lexical_source.ko"),
                stdout: b"field\nafter\ndrop-source\n",
                nested: true,
            },
            Case {
                name: "outer_owned_moveonly_result",
                source: include_str!(
                    "../tests/fixtures/nested_borrow/outer_owned_moveonly_result.ko"
                ),
                stdout: b"kept\nkept\nowned\n",
                nested: true,
            },
            Case {
                name: "borrow_entry_source",
                source: include_str!("../tests/fixtures/nested_borrow/borrow_entry_source.ko"),
                stdout: b"kept\nkept\nkept\n",
                nested: true,
            },
            Case {
                name: "inner_named_resource_slots",
                source: include_str!(
                    "../tests/fixtures/nested_borrow/inner_named_resource_slots.ko"
                ),
                stdout: b"eval-right\neval-left\ndrop-right\ndrop-left\nkept\nkept\n",
                nested: true,
            },
            Case {
                name: "existing_auxiliary_parent",
                source: include_str!(
                    "../tests/fixtures/nested_borrow/existing_auxiliary_parent.ko"
                ),
                stdout: b"kept\naux\nkept\n",
                nested: true,
            },
            Case {
                name: "wrapper_field_payload",
                source: include_str!("../tests/fixtures/nested_borrow/wrapper_field_payload.ko"),
                stdout: b"field\nconsume-source\ndrop-source\n",
                nested: true,
            },
            Case {
                name: "different_outer_lifetimes",
                source: include_str!(
                    "../tests/fixtures/nested_borrow/different_outer_lifetimes.ko"
                ),
                stdout: b"kept\nkept\nkept\n",
                nested: true,
            },
            Case {
                name: "different_outer_lifetimes_asap",
                source: include_str!(
                    "../tests/fixtures/nested_borrow/different_outer_lifetimes_asap.ko"
                ),
                stdout: b"kept\nkept\n",
                nested: true,
            },
        ]
    }
}
