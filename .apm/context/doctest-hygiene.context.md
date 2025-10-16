# context: doctest-hygiene

Common pitfalls and fixes:
- Import required traits (e.g., clap::Parser) in examples
- Use external paths for public APIs in doctests
- Ensure Default implemented where examples use it
- Avoid referencing private functions
