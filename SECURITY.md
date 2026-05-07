# Security Policy

## Supported versions

| Version | Supported |
|---------|-----------|
| 0.1.0   | ✓         |

---

## What upscan accesses

`upscan` makes read-only API calls to AWS on your behalf:

- **Read-only CloudWatch metrics** — `GetMetricStatistics`, `GetMetricData` to retrieve utilisation and traffic data
- **Read-only resource discovery** — `Describe*` calls only; no resource state is modified
- **Read-only Cost Explorer** — `GetCostAndUsage` to retrieve spend data for saving estimates

`upscan` never performs any write, modify, stop, start, or delete operations against any AWS resource.

---

## What upscan does NOT do

- Does not store, log, or transmit your AWS credentials
- Does not send any data to Uptime Scheduler servers during a scan
- Does not phone home or collect telemetry of any kind
- Does not cache credentials to disk
- Uses the standard AWS SDK credential chain — credentials are resolved by the SDK and never pass through `upscan` code directly

---

## Reporting a vulnerability

Please report security vulnerabilities by emailing **security@uptimescheduler.com**.

We aim to acknowledge all reports within **72 hours** and will work with you to understand and address the issue. We follow a responsible disclosure policy: we ask that you give us **90 days** to release a fix before any public disclosure. We will credit reporters in release notes unless you prefer to remain anonymous.
