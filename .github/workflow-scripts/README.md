# Workflow Scripts

This directory contains supporting scripts for GitHub Actions workflows.

## Directory Structure

```
workflow-scripts/
├── lib/                  # Shared library functions
│   └── gh_graphql.sh    # GraphQL API helper (legacy)
├── maverick/            # Maverick automation scripts
│   ├── intake_poller.py         # Main intake automation script
│   └── test_intake_poller.py    # Unit tests
└── README.md            # This file
```

## Maverick Intake Poller

The `maverick/intake_poller.py` script automates the intake process for GitHub Projects v2:
- Polls project boards for items in a "Ready for Takeoff" status
- Creates Copilot agent tasks for ready items
- Updates project status to "In Flight"
- Implements gating logic to prevent multiple concurrent items

### Environment Variables

Required:
- `ORG` - GitHub organization login
- `PROJECT_NUMBER` - Projects v2 board number
- `STATUS_READY` - Name of the ready status option
- `STATUS_INFLIGHT` - Name of the in-flight status option
- `GH_TOKEN` - GitHub token with repo and projects write permissions
- `GITHUB_REPOSITORY` - Repository in format `owner/repo`

Optional:
- `COPILOT_KICKOFF` - Comment body to post when starting work

### Testing

Run the tests using Python's unittest framework:

```bash
# Run all tests
python3 -m unittest discover -s .github/workflow-scripts/maverick -p "test_*.py" -v

# Run specific test file
python3 -m unittest .github/workflow-scripts/maverick/test_intake_poller.py -v

# Run specific test class
python3 -m unittest test_intake_poller.TestNormalize -v

# Run specific test method
python3 -m unittest test_intake_poller.TestNormalize.test_normalize_basic -v
```

Or with pytest (if available):

```bash
python3 -m pytest .github/workflow-scripts/maverick/test_intake_poller.py -v
```

### Test Coverage

The test suite covers:
- ✅ String normalization for status matching
- ✅ GraphQL API calls and error handling
- ✅ Pagination logic for fetching project items
- ✅ Preflight checks for token and project access
- ✅ Gate logic to prevent concurrent items
- ✅ Environment variable validation
- ✅ Integration scenarios

### Development

When modifying `intake_poller.py`:

1. **Format**: Ensure code uses consistent 4-space indentation
   ```bash
   python3 -m py_compile .github/workflow-scripts/maverick/intake_poller.py
   ```

2. **Test**: Run the test suite
   ```bash
   python3 -m unittest .github/workflow-scripts/maverick/test_intake_poller.py -v
   ```

3. **Validate**: Check syntax and indentation
   ```bash
   python3 -c "import ast; ast.parse(open('.github/workflow-scripts/maverick/intake_poller.py').read())"
   ```

## Adding New Scripts

When adding new Python scripts:

1. Follow PEP 8 style guidelines (4-space indentation)
2. Add a module docstring explaining purpose
3. Create corresponding `test_*.py` file with unit tests
4. Update this README with usage instructions
5. Use type hints where appropriate
6. Handle errors gracefully with clear messages

## CI Integration

These scripts are used by GitHub Actions workflows in `.github/workflows/`. 
Tests should be run as part of the CI pipeline to ensure quality.
