# clin [![Build Status](https://travis-ci.org/jaemk/clin.svg?branch=master)](https://travis-ci.org/jaemk/clin) [![crates.io:clin](https://img.shields.io/crates/v/clin.svg?label=clin)](https://crates.io/crates/clin)

> Command completion notifications -- client & listener -- OSX & Linux

## Installation

See [`releases`](https://github.com/jaemk/clin/releases) for binary releases, or:

```
cargo install clin
```

## Usage

`clin` provides desktop notifications of completed commands.

```
# Pass a command as trailing arguments
clin -- cargo build --release

# Pass a command as a string argument
clin -c 'cargo build --release'
```

`clin` can also be used on remote machines by using `ssh` remote port forwarding.
If you're sharing the remote machine, you should probably use a non-default port (default is `6445`)
to avoid any port conflicts and misdirected notifications. See `--help` for supported environment
variables to avoid having to specify `--send` and `--port`.

```
# Listening for incoming notifications on 127.0.0.1:3443
clin listen --log --port 3443

# Connect to a different machine, forwarding your `clin` port
ssh -R 3443:localhost:3443 you@host

# Use the `--send` arg
clin -s -p 3443 -- ./some-build-script.sh  # -> Get a local notification!
```

If you happen to be on the same network as your remote machine, you can listen "publicly" and specify the listener's hostname.
See `--help` for supported environment variables to avoid having to specify `--send` and `--host`.

```
# Listen publicly
clin listen --public --log

# Don't need to do any port forwarding now
clin -s --host <clever-hostname-here> -c 'cargo build --release'
```
