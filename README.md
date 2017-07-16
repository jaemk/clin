# clin

> Command completion notifications, client & server

## Usage

`clin` provides desktop notifications of completed commands.

```
# Pass a command as trailing arguments
clin -- cargo build --release

# Pass a command as a string argument
clin -c 'cargo build --release'
```

`clin` can also be used on remote machines by using `ssh` remote port forwarding.

```
# Listening for incoming notifications on 127.0.0.1:6445
clin listen

# Connect to a different machine, forwarding your `clin` port
ssh -R 6445:localhost:6445 you@host

# Use the `--send` arg
clin -s -- ./some-build-script.sh
```
