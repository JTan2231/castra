# Bootstrap Quickstart

This directory contains the leanest configuration needed to prove the bootstrap
pipeline works end-to-end. It reuses the demo Alpine image that ships with the
repository, forwards guest SSH to `localhost:2223`, and runs a tiny shell script
that stamps a completion file.

> **Credentials**: The metadata references an example ED25519 key pair at
> `bootstrap/keys/quickstart_ed25519`. The private key ships with this repo for
> demo purposes only; do not reuse it outside local testing. Before running the
> bootstrap pipeline, copy `bootstrap/keys/quickstart_ed25519.pub` into the
> guest’s `~root/.ssh/authorized_keys` (for example via the VM console or
> `ssh-copy-id` once you have a temporary password). With the public key in
> place the bootstrap runner can authenticate without prompting.

```bash
cargo run -- up \
  --config examples/bootstrap-quickstart/castra.toml \
  --bootstrap=always
```

When the command returns you should see TTY progress covering the handshake,
transfer, apply, and verify stages. Inspect the results with:

```bash
cargo run -- status --config examples/bootstrap-quickstart/castra.toml
ls ~/.castra/projects/bootstrap-quickstart*/logs/bootstrap
```

Need to tweak credentials or the broker identity? Edit
`bootstrap/bootstrap.toml`—the file is intentionally tiny so the required edits
stand out.

Stop the VM when you're done:

```bash
cargo run -- down --config examples/bootstrap-quickstart/castra.toml
```
