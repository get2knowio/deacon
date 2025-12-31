# Cycle Detection Test Fixture

This fixture is used to test circular dependency detection in the features plan command.

The configuration includes three features that would have circular dependencies:
- feature-a depends on feature-b
- feature-b depends on feature-c
- feature-c depends on feature-a

Note: This fixture requires mocked feature metadata to actually trigger the cycle.
In a real end-to-end test, the features would need to exist with these dependencies.
