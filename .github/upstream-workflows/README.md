# Upstream workflows (not active in this fork)

These are Horizon's own GitHub Actions workflows, kept as they were upstream. GitHub only runs workflows from
`.github/workflows/`, so none of these run here.

They were moved because they drive upstream's infrastructure, which this fork doesn't have: self-hosted Mac and
Linux runners (`horizon-game`, `horizon-parity-main`...), release signing keys, publishing to playhorizon.dev,
and bots that answer `/claim` and `/nightly` comments. Several run on a schedule every 10 minutes and would queue or
fail in a repository without that setup.

To turn one on, move it back to `.github/workflows/` and give it what it needs (secrets, runners). The Python tests
that check these files (`tests/test_nightly.py`, `tests/test_pr_labels.py`, `tests/test_claim_command.py`...) read
them from `.github/workflows/`, so they fail until the files are moved back.
