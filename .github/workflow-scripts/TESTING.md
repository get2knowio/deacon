# Quick Start: Testing Workflow Scripts

## Running Tests

### Using the Test Runner (Recommended)
```bash
cd .github/workflow-scripts
./test_workflow_scripts.sh
```

### Using Python unittest Directly
```bash
# From repository root
python3 -m unittest discover -s .github/workflow-scripts/maverick -p "test_*.py" -v

# From workflow-scripts directory
cd .github/workflow-scripts
python3 -m unittest discover -s maverick -p "test_*.py" -v
```

### Running Specific Tests
```bash
# Single test class
python3 -m unittest maverick.test_intake_poller.TestNormalize -v

# Single test method
python3 -m unittest maverick.test_intake_poller.TestNormalize.test_normalize_basic -v
```

## Test Coverage Summary

### `maverick/intake_poller.py`
- ✅ 19 test cases
- ✅ Core functionality: normalize(), gh_graphql(), fetch_all_items()
- ✅ Integration tests: main() workflow scenarios
- ✅ Error handling: missing env vars, API errors, gate logic
- ✅ Edge cases: pagination, empty results, concurrent items

## Adding New Tests

1. Create `test_<module>.py` in the same directory as the module
2. Import the module to test
3. Use `unittest.TestCase` for test classes
4. Use `@patch` decorators for mocking external dependencies
5. Follow naming convention: `test_<function>_<scenario>`

### Example Test Structure
```python
import unittest
from unittest.mock import patch, Mock

import module_to_test

class TestMyFunction(unittest.TestCase):
    """Tests for my_function."""
    
    @patch("module_to_test.external_dependency")
    def test_my_function_success(self, mock_dep):
        """Test successful execution."""
        mock_dep.return_value = "expected"
        result = module_to_test.my_function()
        self.assertEqual(result, "expected")
```

## CI Integration

Add to `.github/workflows/ci.yml`:
```yaml
- name: Test workflow scripts
  run: |
    cd .github/workflow-scripts
    ./test_workflow_scripts.sh
```

## Pre-commit Hook (Optional)

Add to `.git/hooks/pre-commit`:
```bash
#!/bin/bash
cd .github/workflow-scripts && ./test_workflow_scripts.sh
```
