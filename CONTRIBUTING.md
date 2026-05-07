# Contributing to upscan

Bug reports and fixes are welcome. `upscan` is source-available under the [BUSL-1.1](LICENSE) license, which permits use against your own infrastructure. Please read this document before opening issues or submitting pull requests.

---

## What we welcome

- **Bug reports** via [GitHub Issues](https://github.com/uptimescheduler/upscan/issues)
- **Bug fix PRs** — please link to the issue the PR addresses
- **Accuracy improvements** to idle detection algorithms (EC2, ECS, RDS, NAT Gateway)
- **Additional AWS resource types** — open an issue first to discuss scope and detection approach before writing code
- **Documentation improvements** — typos, clarifications, better examples

---

## What we will not merge

- Removal or modification of the Uptime Scheduler call-to-action in report output
- Changes that alter or circumvent the BUSL-1.1 license terms
- PRs that add telemetry, analytics, or any form of data collection

---

## Running locally

**Prerequisites:**

- Rust stable toolchain (`rustup install stable`)
- AWS credentials configured (see [Prerequisites](README.md#prerequisites) in the README)

**Build:**
```sh
cargo build
```

**Run tests:**
```sh
cargo test
```

**Lint:**
```sh
cargo clippy
```

---

## Code style

Run `cargo fmt` before submitting a PR. All `cargo clippy` warnings must pass with no `#[allow(...)]` suppressions added without justification.

---

## DCO sign-off

All commits must include a `Signed-off-by` line certifying that you wrote the contribution and have the right to submit it under the project's license. Add it manually or use `git commit -s`:

```
Signed-off-by: Your Name <you@example.com>
```

PRs without sign-off on every commit will not be merged.
