# Data Model — Outdated Subcommand

## Entities

- Feature Identifier
  - Fields: `canonical_id` (string; fully‑qualified without version)
  - Relationships: maps to `FeatureVersionInfo`

- FeatureVersionInfo
  - Fields:
    - `current` (string|null): lockfile version if present; else wanted
    - `wanted` (string|null): derived from tag/digest rules
    - `wantedMajor` (string|null): major of wanted
    - `latest` (string|null): highest stable semver tag
    - `latestMajor` (string|null): major of latest

- OutdatedResult
  - Fields:
    - `features` (map<string, FeatureVersionInfo>): keyed by canonical fully‑qualified feature ID without version

## Validation Rules

- Keys must be canonical IDs without version suffixes.
- Values must include all fields; unknowns represented as null.
- Ordering when rendered in text preserves declaration order from config.

## State Transitions

- Read-only; no persisted state changes. Lockfile is read to compute `current`.
