//! Same analyzed fixture owner shared by public unit native contracts.
use super::*;
pub(super) struct UnitAnalysis {
    pub(super) sources: SourceMap,
    pub(super) provider_source: SourceId,
    pub(super) provider: ParsedFile,
    pub(super) consumer_source: SourceId,
    pub(super) consumer: ParsedFile,
    pub(super) names: ValidatedCompilationUnitNames,
    pub(super) environment: TypeEnvironment,
    pub(super) typed: ValidatedCompilationUnitTypes,
    pub(super) owned: ValidatedCompilationUnitOwnership,
}

impl UnitAnalysis {
    pub(super) fn inputs(&self) -> [SourceUnitInput<'_>; 2] {
        [
            SourceUnitInput::new(
                "root",
                "p/provider.ko",
                self.provider_source,
                &self.provider,
            ),
            SourceUnitInput::new(
                "root",
                "q/consumer.ko",
                self.consumer_source,
                &self.consumer,
            ),
        ]
    }

    pub(super) fn declaration(&self, package: &str, name: &str) -> DeclarationId {
        self.names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|declaration| {
                declaration.name() == name
                    && self.names.names().index().packages()[declaration.package().index()]
                        .name()
                        .segments()
                        .iter()
                        .map(String::as_str)
                        .eq(package.split('.'))
            })
            .expect("fixture declaration exists")
            .id()
    }
}

pub(super) fn analyze_unit() -> UnitAnalysis {
    analyze_sources(
        "package p\n\
         value class Token(val item: Int)\n\
         class Bundle(val text: String, val count: Int)\n\
         fun make(number: Int): String = if (number == 5) {\n\
             \"provider\" + \"!\"\n\
         } else {\n\
             \"fallback\" + \"!\"\n\
         }\n\
         fun inspect(message: String): Unit {}\n\
         fun makeBundle(): Bundle = Bundle(count = 7, text = \"bundle\" + \"!\")\n\
         fun count(own bundle: Bundle): Int = bundle.count\n\
         fun boxed(): Box<Token> = Box(Token(9))\n\
         fun inspectBox(own resource: Box<Token>): Int = 1\n\
         fun buildRc(): Rc<Int> = Rc(40)\n\
         fun useRc(own owner: Rc<Int>): Int {\n\
             val retained = owner.share()\n\
             val copied = retained.value\n\
             return copied + owner.value\n\
         }",
        "package q\n\
         import p.make as build\n\
         import p.inspect\n\
         fun exercise(flag: Boolean): Unit {\n\
             val bundle = p.makeBundle()\n\
             val boxed = p.boxed()\n\
             val shared = p.buildRc()\n\
             if (flag) { return }\n\
             val counted = p.count(bundle)\n\
             val inspected = p.inspectBox(boxed)\n\
             val used = p.useRc(shared)\n\
         }\n\
         fun entry(): Unit {\n\
             val offset = 2\n\
             val action: move (borrow Int) -> String = move { item ->\n\
                 if (item == 3) {\n\
                     build(item + offset)\n\
                 } else {\n\
                     \"unused\" + \"!\"\n\
                 }\n\
             }\n\
             val message = action(3)\n\
             val seen = inspect(message)\n\
             val ownedAction: move (own String) -> Unit = move { owned -> inspect(owned) }\n\
             val ownedInvoked = ownedAction(\"native-owned\")\n\
             val early = exercise(true)\n\
             val normal = exercise(false)\n\
         }\n\
         fun argvEntry(args: Array<String>): Unit {}\n\
         fun invalidEntry(number: Int): Unit {}\n\
         fun supportedBorrow(): Unit {\n\
             val action: move (borrow p.Bundle) -> Unit = move { message -> }\n\
             val invoked = action(p.makeBundle())\n\
         }\n\
         fun unsupportedBorrow(): Unit {\n\
             val action: move (borrow Unit) -> Unit = move { message -> }\n\
             val invoked = action(inspect(\"unsupported\"))\n\
         }",
    )
}

pub(super) fn analyze_sources(provider_text: &str, consumer_text: &str) -> UnitAnalysis {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(&mut sources, "p/provider.ko", provider_text);
    let (consumer_source, consumer) = parsed(&mut sources, "q/consumer.ko", consumer_text);
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
    ];
    let (name_environment, environment) = standard_environments();
    let index = index_compilation_unit(&sources, &inputs).expect("valid unit input");
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &name_environment)
        .expect("name resolution succeeds")
        .validate()
        .expect("valid names");
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &environment)
        .expect("type checking succeeds")
        .validate()
        .expect("valid types");
    let owned = check_compilation_unit_ownership(&sources, &inputs, &names, &environment, &typed)
        .expect("ownership checking succeeds")
        .validate()
        .expect("valid ownership");
    UnitAnalysis {
        sources,
        provider_source,
        provider,
        consumer_source,
        consumer,
        names,
        environment,
        typed,
        owned,
    }
}

pub(super) fn parsed(sources: &mut SourceMap, name: &str, text: &str) -> (SourceId, ParsedFile) {
    let source = sources.add_source(name, text).expect("unique source");
    let lexed = lex(sources, source).expect("lexing succeeds");
    let parsed = parse_file(sources, &lexed).expect("parsing succeeds");
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    (source, parsed)
}
