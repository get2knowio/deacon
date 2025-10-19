# Outdated Subcommand — Task Roadmap (Dependency Order)

Recommended execution order for the tasks in this folder:

1. 01 - Implement CLI Subcommand and Flags
2. 02 - Infrastructure: Lockfile Data Structures and I/O
3. 03 - Infrastructure: OCI Tag Listing and Semver Filtering (adds semver dep)
4. 04 - Version Resolution and Digest Metadata Helpers
5. 11 - Config Discovery and Feature Extraction
6. 05 - Core Command Skeleton and Pipeline
7. 08 - Parallel Execution and Deterministic Ordering
8. 07 - Error Handling and Graceful Degradation
9. 12 - JSON Schema Alignment and Serialization
10. 06 - Output Rendering (Text and JSON) with Terminal Hints
11. 09 - Testing: Unit, Integration, Smoke, and Examples
12. 10 - Documentation and Help Text
13. 13 - Feature Identifier Parsing and Filtering (if not covered by helpers)
14. 14 - OCI Authentication Wiring for Tags/Manifests (if additional glue needed)

Notes:
- Some tasks may be implemented in parallel by different contributors, but observe the dependencies for a smooth path to green CI.
- Tasks 13 and 14 are focused follow-ups ensuring robust identifier handling and authenticated registry calls.