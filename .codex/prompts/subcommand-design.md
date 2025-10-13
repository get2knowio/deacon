# Generate Detailed Design Specification for DevContainer CLI Subcommand

## Command Usage

```
/subcommand-design <subcommand-name>
```

**Examples:**
- `/subcommand-design up`
- `/subcommand-design build`
- `/subcommand-design features-test`

---

## Mission

Generate a comprehensive, language-agnostic design specification for the specified DevContainer CLI subcommand. Reverse-engineer and document the TypeScript reference implementation in extreme detail, producing specifications that any competent developer could use to implement the CLI in any programming language.

## Context

You have access to:
1. **`repomix-output-devcontainers-cli.xml`** - The complete TypeScript reference implementation of the DevContainer CLI from Microsoft
2. **DevContainer Specification** at https://containers.dev/implementors/spec - The authoritative source for understanding design decisions
3. **`docs/TS_CLI_PARITY_ANALYSIS.md`** - Gap analysis between TypeScript and Rust implementations

## Required Output

Create a detailed design specification document with the following 16 sections:

### 1. Subcommand Overview
- **Purpose**: What problem does this subcommand solve?
- **User Personas**: Who uses this command and in what scenarios?
- **Specification References**: Link to relevant sections of https://containers.dev/implementors/spec that govern this command's behavior
- **Related Commands**: How does this subcommand interact with or complement other subcommands?

### 2. Command-Line Interface
- **Full Syntax**: Complete command signature with all flags, options, and arguments
- **Flag Taxonomy**: Categorize flags as:
  - Required vs. optional
  - Mutually exclusive groups
  - Deprecated flags and their replacements
- **Argument Validation Rules**: Precise rules for valid inputs, including:
  - Type constraints
  - Format requirements (regex patterns where applicable)
  - Range limits
  - Interdependencies between flags

### 3. Input Processing Pipeline

Document the complete flow from command invocation to initial validation:

```pseudocode
FUNCTION parse_command_arguments(args: CommandLineArgs) -> ParsedInput:
    // Step-by-step argument parsing logic
    // Include all validation steps
    // Show error handling for invalid inputs
END FUNCTION
```

### 4. Configuration Resolution

Detail how the command resolves its configuration:

- **Configuration Sources** (in precedence order):
  - Command-line flags
  - Environment variables
  - Configuration files (devcontainer.json, overrides, extends chain)
  - Default values
- **Merge Algorithm**: Pseudocode for how conflicting values are resolved
- **Variable Substitution**: Complete rules for variable expansion (${env:VAR}, ${localEnv:VAR}, etc.)

### 5. Core Execution Logic

Provide detailed pseudocode for the main execution flow:

```pseudocode
FUNCTION execute_subcommand(parsed_input: ParsedInput) -> ExecutionResult:
    // Phase 1: Initialization
    // - Set up logging
    // - Validate prerequisites
    // - Establish connections (Docker, registry, etc.)
    
    // Phase 2: Pre-execution validation
    // - Check dependencies
    // - Validate environment
    
    // Phase 3: Main execution
    // - Core business logic
    // - Step-by-step operations
    
    // Phase 4: Post-execution
    // - Cleanup
    // - Status reporting
    
    RETURN result
END FUNCTION
```

### 6. State Management

Document all stateful aspects:

- **Persistent State**: What files/databases are created or modified?
- **Cache Management**: What is cached and where? Cache invalidation rules?
- **Lock Files**: When and how are locks used?
- **Idempotency**: Is the command safe to run multiple times? What changes on re-runs?

### 7. External System Interactions

Detail all interactions with external systems:

#### Docker/Container Runtime
```pseudocode
FUNCTION interact_with_docker(operation: Operation) -> Result:
    // Exact docker CLI commands executed
    // Expected response formats
    // Error handling for common failures
END FUNCTION
```

#### OCI Registries
- Authentication flow (credential helpers, tokens, etc.)
- Manifest fetching algorithm
- Layer download and caching strategy
- Platform selection logic

#### File System
- Read/write patterns
- Permission requirements
- Cross-platform path handling
- Symlink handling

### 8. Data Flow Diagrams

Provide ASCII/text-based data flow diagrams showing:

```
┌─────────────────┐
│ User Input      │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ Parse & Validate│
└────────┬────────┘
         │
         ▼
    [Continue flow...]
```

### 9. Error Handling Strategy

Categorize and document all error cases:

- **User Errors**: Invalid input, missing files, etc.
  - Error code
  - Error message format
  - Suggested remediation
- **System Errors**: Docker unavailable, network failures, etc.
  - Detection mechanism
  - Retry logic
  - Fallback behavior
- **Configuration Errors**: Invalid devcontainer.json, etc.
  - Validation rules
  - Error reporting format

### 10. Output Specifications

#### Standard Output (stdout)
- **JSON Mode**: Complete schema for JSON output
- **Text Mode**: Format specifications, column widths, etc.
- **Quiet Mode**: What is suppressed?

#### Standard Error (stderr)
- Log level filtering
- Progress indicators
- Diagnostic information

#### Exit Codes
Complete table of exit codes and their meanings

### 11. Performance Considerations

Document performance characteristics:

- **Caching Strategy**: What is cached to avoid redundant work?
- **Parallelization**: What operations run concurrently?
- **Resource Limits**: Memory constraints, file descriptor limits, etc.
- **Optimization Opportunities**: Where are the bottlenecks?

### 12. Security Considerations

- **Secrets Handling**: How are secrets passed, stored, and redacted?
- **Privilege Escalation**: When does the command require elevated privileges?
- **Input Sanitization**: How is user input sanitized to prevent injection attacks?
- **Container Isolation**: Security boundaries and their enforcement

### 13. Cross-Platform Behavior

Document platform-specific behaviors:

| Aspect | Linux | macOS | Windows | WSL2 |
|--------|-------|-------|---------|------|
| Path handling | ... | ... | ... | ... |
| Docker socket | ... | ... | ... | ... |
| User ID mapping | ... | ... | ... | ... |

### 14. Edge Cases and Corner Cases

Document unusual scenarios:

- Empty configuration files
- Circular dependencies
- Network partitions during execution
- Container exits during lifecycle hooks
- Permission denied scenarios
- Read-only file systems

### 15. Testing Strategy

Outline how to verify the implementation:

```pseudocode
TEST SUITE for subcommand:
    
    TEST "happy path":
        GIVEN typical valid input
        WHEN command executes
        THEN expect standard success output
    
    TEST "invalid configuration":
        GIVEN malformed devcontainer.json
        WHEN command executes
        THEN expect specific error message
    
    [Additional test cases...]
END TEST SUITE
```

### 16. Migration Notes

If behavior has changed from previous versions:

- **Deprecated Behavior**: What old behaviors are no longer supported?
- **Breaking Changes**: What changes require user action?
- **Compatibility Shims**: What temporary compatibility is provided?

## Pseudocode Style Guide

Use consistent, language-agnostic pseudocode:

```pseudocode
// Use UPPERCASE for keywords
FUNCTION function_name(param: Type) -> ReturnType:
    // Single-line comments use //
    
    /* Multi-line comments
       use this style */
    
    // Variable declarations
    DECLARE variable_name: Type = initial_value
    
    // Control structures
    IF condition THEN
        statements
    ELSE IF other_condition THEN
        statements
    ELSE
        statements
    END IF
    
    FOR item IN collection DO
        statements
    END FOR
    
    WHILE condition DO
        statements
    END WHILE
    
    // Error handling
    TRY
        risky_operation()
    CATCH error_type AS e
        handle_error(e)
    FINALLY
        cleanup()
    END TRY
    
    RETURN value
END FUNCTION

// Struct/record definitions
STRUCT StructName:
    field_name: FieldType
    another_field: AnotherType
END STRUCT
```

## Critical Analysis Requirements

For **every** design decision you encounter in the TypeScript code, you MUST:

1. **Consult the Specification** at https://containers.dev/implementors/spec
2. **Document the Rationale**: Why is it implemented this way?
3. **Note Deviations**: If the implementation differs from the spec, document why
4. **Identify Ambiguities**: Where does the spec leave room for interpretation?

### Analysis Template

For each significant design decision:

```markdown
#### Design Decision: [Short Title]

**Implementation Behavior**: 
[What the TypeScript code actually does]

**Specification Guidance**: 
[What containers.dev/implementors/spec says about this]

**Rationale**: 
[Why this approach was chosen - reference GitHub issues, comments in code, etc.]

**Alternatives Considered**: 
[What other approaches were possible]

**Trade-offs**: 
[Pros and cons of this decision]
```

## Output Files

Create these files in the `docs/subcommand-specs/` directory:

1. **Main Specification Document**
   - File: `docs/subcommand-specs/<subcommand-name>/SPEC.md`
   - Follow the 16-section structure above
   - Include extensive pseudocode
   - Cross-reference specification sections

2. **Sequence Diagrams**
   - File: `docs/subcommand-specs/<subcommand-name>/DIAGRAMS.md`
   - Show interaction between components using Mermaid syntax
   - Include error flows

3. **Data Structure Definitions**
   - File: `docs/subcommand-specs/<subcommand-name>/DATA-STRUCTURES.md`
   - All data structures used
   - JSON schemas where applicable

## Quality Criteria

Your specifications will be evaluated on:

1. **Completeness**: Does it cover ALL aspects of the command?
2. **Clarity**: Could a developer unfamiliar with the codebase implement from this spec?
3. **Accuracy**: Does it faithfully represent the TypeScript implementation?
4. **Language Agnosticism**: Is it free of TypeScript/JavaScript-specific concepts?
5. **Specification Alignment**: Are deviations from containers.dev spec noted and explained?
6. **Actionability**: Can this be used as a technical blueprint for implementation?

## Prohibited Content

Do NOT include:

- TypeScript code snippets (convert everything to pseudocode)
- JavaScript/Node.js-specific idioms (use generic concepts)
- Vague descriptions like "processes the input" (be specific about HOW)
- Assumptions without verification (check the spec or code)

## Process

1. **Locate Implementation**: Find all TypeScript files implementing the subcommand in `repomix-output-devcontainers-cli.xml`
2. **Read Specification**: Review relevant sections of https://containers.dev/implementors/spec
3. **Trace Execution Flow**: Follow the code from CLI parsing to completion
4. **Identify Dependencies**: Map all external dependencies and interactions
5. **Document Edge Cases**: Map out error conditions and corner cases
6. **Create Pseudocode**: Convert all logic to language-agnostic pseudocode
7. **Generate Diagrams**: Create sequence and data flow diagrams
8. **Write Analysis**: Document design decisions with rationale

## Available Subcommands

### High Priority (Core Functionality)
- `up` - Start and connect to a dev container
- `build` - Build a dev container image
- `run-user-commands` - Execute lifecycle commands
- `read-configuration` - Parse and display configuration
- `exec` - Execute command in running container

### Medium Priority (Essential Workflows)
- `set-up` - Configure existing container
- `features-test` - Test feature implementations
- `features-publish` - Publish features to registry
- `templates-apply` - Apply template to directory
- `templates-publish` - Publish templates to registry

### Lower Priority (Utility Commands)
- `features-package` - Package features for distribution
- `templates-package` - Package templates for distribution
- `features-info` - Display feature metadata
- `outdated` - Check for outdated dependencies
- `upgrade` - Update lockfile versions

## Questions During Analysis

As you work, if you encounter:

- **Ambiguities**: Document them in an "Open Questions" section
- **Apparent Bugs**: Note them with evidence in a "Potential Issues" section
- **Missing Specification**: Flag areas where the spec is silent in a "Specification Gaps" section
- **Implementation Choices**: Explain the reasoning in the "Design Decision" format

## Final Deliverable

Your output should be so detailed and clear that it could serve as the authoritative reference for implementing a DevContainer CLI in any language, matching the behavior of the TypeScript reference implementation exactly.

Upon completion, create a summary document at `docs/subcommand-specs/<subcommand-name>/README.md` that provides:
- Executive summary of the subcommand
- Quick reference for most common use cases
- Links to the three detailed specification files
- Implementation checklist for developers
