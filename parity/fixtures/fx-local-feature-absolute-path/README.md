# fx-local-feature-absolute-path

A workspace holding one perfectly legal local Feature — there is a
`.devcontainer/` folder, the Feature lives in a sub-folder of it, and the
sub-folder name matches the Feature's `id`, so the containment rule
`fx-upstream-local-feature-missing-folder` (#488) covers is satisfied and cannot
be what a rejection is about. `devcontainer.json` itself declares no Feature.

The case that uses this fixture references that same directory **by absolute
path**, through `--additional-features` — "JSON as per the `features` section",
the same ingress, on both CLIs.

`devcontainer-features-distribution.md` §Locally Referenced Features:

> A local Feature may **not** be referenced by absolute path.

## Why the reference is in argv rather than in the config

It has to be, and the first draft of this case proved why. `${WORKSPACE}` is
substituted in an operation's **argv** and never inside fixture file contents,
so a config-declared absolute path can only ever name a directory that does not
exist in the materialized temp workspace — and then BOTH sides exit 1 whether or
not the absolute-path rule is enforced, because resolution fails on the missing
directory instead. That first draft agreed with the reference with the rejection
deliberately compiled out: a case that cannot fail proves nothing.

Passing the path in argv makes `${WORKSPACE}` resolve to the real temp workspace,
so the absolute path names a Feature that genuinely **exists and would resolve**.
Enforcement is then the only thing standing between deacon and exit 0 — verified
by removing the check and watching deacon return 0 against the reference's 1.

`./localFeatureA` is the spelling that would make this same reference legal;
`case-readconfig-upstream-local-feature-inside-devcontainer` pins that it
resolves.

Authored for #495, not vendored from upstream — the reference CLI's e2e suite
has no fixture for this clause.
