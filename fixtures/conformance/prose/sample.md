# Sample Consumer Document

Intro prose describing the sample. This paragraph is purely descriptive and
carries no obligation.

## Lifecycle

The tool MUST run onCreateCommand exactly once. The tool MUST NOT run it again
on a subsequent start.

The command should generally complete quickly, but timing is not guaranteed.

### Output contract

The resolver emits a single JSON document:

```
{
  "outcome": "success",
  "containerId": "abc123"
}
```

## Notes

This section is explanatory background and does not define a requirement.
