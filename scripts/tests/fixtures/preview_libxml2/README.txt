SPEC-0322 CI-only compare fixture

Source: https://github.com/Halckon/Koven/actions/runs/38667368062/attempts/1
Head: b388e6dea5571dae95219d170ac22e317c4226a9
Tree: 991e4e03f2c6aba64e357750eb86ac88e5fd4950

ci_38667368062_excerpt.json retains the measured host projection (excluding
large load_records), exact selected stdout lines, each original stdout SHA256,
and the original artifact member SHA256. Probe numbers match the retained
libxml2_diagnostic.commands list. No probe was rerun to produce this fixture.
The original CI run failed the strict libxml SHA gate before native commands.

Selection: probes 0/1/3/23/24 retain whole stdout; 2 retains only the owned
libxml MD5 entry; 4 retains Ubuntu main/amd64 release lines; 9 retains 16
LIBXML versioned UND lines; 10 retains their File/version needs; 25 retains
16 matching definitions; 26 retains the two corresponding version definitions.
Requirements/definitions are manually reviewable normalized projections of
those lines. These clipped tables are NOT full ELF parser inputs.

The test completes these measured facts with SYNTHETIC successful disk MD5,
os-release codename, and reduced full-closure ambiguity facts. Historical CI
did not measure the actual disk MD5 or every object's dynsym/version tables.
This is a compare protocol scenario, not real compatibility or installation
acceptance. Source URI and installed origin remain explicitly unproven.

Current tests call only compare_hosts, an existing API. The intended red is
its strict-hash error where the new bounded protocol expects an empty error
list; missing parser APIs/import errors are not valid red evidence. Parser,
probe fault and consumer failure coverage remain separate required work.
All protocol/mutation execution is CI-only; cloud authoring uses AST/static
review without importing the test module or running any test/helper.
