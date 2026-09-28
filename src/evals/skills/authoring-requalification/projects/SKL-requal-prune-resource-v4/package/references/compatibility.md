# Compatibility review

For each API change, identify affected and unaffected consumers, compare the old
and new contract, and distinguish source, wire, and behavioral compatibility.
Require an observable migration and rollback path. Record validation as pass,
fail, or not_run with its environment; missing evidence is not a pass.
