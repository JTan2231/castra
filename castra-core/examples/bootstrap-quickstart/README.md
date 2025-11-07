# Bootstrap Quickstart

This directory contains the leanest configuration needed to prove the bootstrap
pipeline works end-to-end. It reuses the demo Alpine image that ships with the
repository, forwards guest SSH to `localhost:2222`, and runs a tiny shell script
that stamps a completion file.

> **Credentials**: The example assumes `root@localhost:2222` accepts one of your
> local SSH keys without a password prompt. If you need to point Castra at a
> specific identity, add an `[ssh]` stanza to `bootstrap/bootstrap.toml`.

```bash
cargo run -- up \
  --config examples/bootstrap-quickstart/castra.toml \
  --bootstrap=always
```

When the command returns you should see TTY progress covering the readiness
(SSH wait), transfer, apply, and verify stages. Inspect the results with:

```bash
cargo run -- status --config examples/bootstrap-quickstart/castra.toml
ls ~/.castra/projects/bootstrap-quickstart*/logs/bootstrap
```

Need to tweak credentials? Edit `bootstrap/bootstrap.toml`—the file is
intentionally tiny so the required edits stand out. If a run fails midway, use
`castra down` or `castra clean` to reset the workspace before retrying.

Stop the VM when you're done:

```bash
cargo run -- down --config examples/bootstrap-quickstart/castra.toml
```
