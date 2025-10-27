# Example GitHub Actions Workflow for Testing Workflow Scripts

# Add this job to your CI workflow to test Python workflow scripts

```yaml
test-workflow-scripts:
  name: Test Workflow Scripts
  runs-on: ubuntu-latest
  steps:
    - name: Checkout code
      uses: actions/checkout@v4

    - name: Set up Python
      uses: actions/setup-python@v5
      with:
        python-version: '3.12'

    - name: Run workflow script tests
      run: |
        cd .github/workflow-scripts
        chmod +x test_workflow_scripts.sh
        ./test_workflow_scripts.sh

    - name: Upload test results (optional)
      if: always()
      uses: actions/upload-artifact@v4
      with:
        name: workflow-script-test-results
        path: .github/workflow-scripts/**/*.xml
        if-no-files-found: ignore
```

## Alternative: Direct Python unittest

```yaml
test-workflow-scripts:
  name: Test Workflow Scripts
  runs-on: ubuntu-latest
  steps:
    - name: Checkout code
      uses: actions/checkout@v4

    - name: Set up Python
      uses: actions/setup-python@v5
      with:
        python-version: '3.12'

    - name: Test intake_poller
      run: |
        python3 -m unittest discover \
          -s .github/workflow-scripts/maverick \
          -p "test_*.py" \
          -v
```

## Integration into Existing CI

Add to your existing `.github/workflows/ci.yml`:

```yaml
jobs:
  # ... existing jobs ...
  
  test-python-scripts:
    name: Python Script Tests
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-python@v5
        with:
          python-version: '3.12'
      - name: Test workflow scripts
        run: |
          cd .github/workflow-scripts
          ./test_workflow_scripts.sh
```

## Coverage Reporting (Optional)

To add coverage reporting:

```yaml
    - name: Install coverage tools
      run: pip install coverage

    - name: Run tests with coverage
      run: |
        cd .github/workflow-scripts/maverick
        coverage run -m unittest discover -p "test_*.py"
        coverage report
        coverage xml

    - name: Upload coverage
      uses: codecov/codecov-action@v4
      with:
        files: .github/workflow-scripts/maverick/coverage.xml
        flags: workflow-scripts
```
