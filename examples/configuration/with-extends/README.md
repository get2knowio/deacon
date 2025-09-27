# Extends Configuration Example

Shows configuration chaining via `extends` (base → middle → app) including:
- Env merge with precedence (app > middle > base)
- runArgs concatenation in order

Try (from this directory):
```sh
deacon config validate app
```
