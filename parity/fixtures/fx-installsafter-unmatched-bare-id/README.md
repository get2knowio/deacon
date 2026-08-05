# fx-installsafter-unmatched-bare-id

One local Feature whose `installsAfter` names `nope-not-here` — a bare
single-segment id matching nothing in the feature set, nothing on disk, and
nothing in the eighteen-id deprecated-v1 table.

This is the half of #505 that looks harmless. `installsAfter` is the SOFT edge:
per `feature-dependencies.md` it orders a Feature "if and only if a given Feature
is already set to be installed", so an entry with no match is ordinary and
expected — every real Feature's `installsAfter` on `common-utils` is one when
`common-utils` is not declared. deacon therefore soft-skipped an unresolved entry
and exited 0, and an ILLEGAL entry was indistinguishable from that ordinary
non-match. Exactly the failure mode #495 found for absolute paths in the same
field, arrived at from the other direction.

MEASURED at oracle 0.87.0: the reference exits 1 with
`Legacy feature 'nope-not-here' not supported.` Its canonicalization is
unconditional and runs at feature-set assembly, so whether the entry would have
ordered anything is never reached.

The companion measurement that makes this a rule about validity rather than about
matching: `installsAfter: ["terraform"]` with no `terraform` in the set exits 0 on
BOTH sides. `terraform` is in the table, so it canonicalizes to
`ghcr.io/devcontainers/features/terraform:1`, finds no match, and is soft-skipped
— proving the reference rejects on the id being illegal, not on it being absent.
That direction is pinned hermetically by the
`deprecated_v1_and_path_form_dependency_references_are_accepted` unit test rather
than by a case, since it resolves nothing and would only add a registry fetch.

Its sibling `fx-installsafter-bare-metadata-id` covers the other path, where the
bare id DOES match a declared local Feature through the metadata-id alias and
deacon actively ordered by it.

Authored for #505, not vendored from upstream.
