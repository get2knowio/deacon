# Non-blocking Lifecycle Example

Demonstrates `postStart` and `postAttach` running asynchronously. They append to `/tmp/nb_log` after a short delay.

Tip: To skip non-blocking commands during `up`, you can pass:
```sh
deacon up . --skip-non-blocking-commands
```

Validate config:
```sh
deacon config validate .
```