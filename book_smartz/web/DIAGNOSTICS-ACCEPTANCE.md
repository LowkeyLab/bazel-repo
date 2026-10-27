# Book Smartz deployed diagnostics acceptance

This report records bounded production evidence for [#2703](https://github.com/LowkeyLab/bazel-repo/issues/2703).
The observations occurred on 2026-10-27. All times use Coordinated Universal Time, abbreviated `UTC`.
The maintainer authorized the existing Railway command-line session for these reads.
No deployment, provisioning, credential change, or reader action occurred during verification.

## Release identity

| Item                          | Observed value                                                                                                   |
| ----------------------------- | ---------------------------------------------------------------------------------------------------------------- |
| Source revision               | `3e4096d3beea65e9f878a1e45b13d79305382c94`                                                                       |
| Main test workflow            | [Successful Bazel Tests run](https://github.com/LowkeyLab/bazel-repo/actions/runs/40425878498)                   |
| Image publication             | [Successful Deploy run](https://github.com/LowkeyLab/bazel-repo/actions/runs/40426704164), published at 00:32:24 |
| Intended image                | `ghcr.io/lowkeylab/book_smartz@sha256:3e15a718ee2e7b51ac2c528b406665a15f5e0f03f03183dbef05577465baad08`          |
| Railway target                | Existing `book-smartz` project, `production` environment, `book-smartz` service                                  |
| Existing deployment           | `0f6e6dc6-e2d0-48e6-a793-50d8bbdafbfc`, created at 00:44:03.105                                                  |
| Deployment reason             | `autoupdate`                                                                                                     |
| Railway resolved image digest | `sha256:3e15a718ee2e7b51ac2c528b406665a15f5e0f03f03183dbef05577465baad08`                                        |
| Executable fingerprint        | `f87f0a0e502cbc95426d5e2e72052018960f4c33981421fd95696f39715a2843`                                               |
| State at inspection           | `SLEEPING`                                                                                                       |

The authenticated API returned the resolved digest from `deployment.meta.imageDigest`.
It matches the published digest.
The manifest follows the Open Container Initiative format.
The downloaded manifest and package layer matched their declared `SHA-256` digests.
The image's `integrity.json` names the same executable fingerprint that Railway's lifecycle facts report.

These checks establish release identity separately from image publication.
They do not establish current reader journeys, provider availability, or continuous readiness.

The existing automatic update occurred before this verification.
No additional rollout was necessary to observe the changed diagnostics.
The ticket's separate-authorization requirement still applies to any future deployment, provisioning, or credential change.

## Diagnostic arrival and volume

The authenticated tool retrieved deployment logs from 00:40:00 through 01:00:00.
The request allowed 500 entries and returned eight.
Five entries contain structured lifecycle facts. Three are platform container messages.

| Platform timestamp | Event                | Reported result                                   |
| ------------------ | -------------------- | ------------------------------------------------- |
| 00:44:07.945241990 | `migration_finished` | `succeeded`, stage `complete`, reason `none`      |
| 00:44:15.117475418 | `startup_finished`   | `succeeded`, stage `complete`, reason `none`      |
| 00:44:15.426396452 | `readiness_changed`  | `ready`, stage `complete`, reason `none`          |
| 00:50:57.726180730 | `shutdown_started`   | `started`, stage `request_drain`, reason `signal` |
| 00:50:57.726184310 | `shutdown_finished`  | `completed`, stage `complete`, reason `none`      |

The four serving facts share the recorded executable fingerprint.
The migration fact omits release context, as its contract permits.
Railway parsed each fact into a structured `fact` attribute.
The tool exposes that attribute as an object instead of the original stderr line.

The five compact JSON fact objects total 1,109 UTF-8 bytes.
The complete export from the tool totals 1,803 bytes, including platform messages, timestamps, envelopes, and newlines.
These are export measurements. They are not original stderr byte counts, stored-log sizes, or billed volume.
This startup and sleep window has little activity and cannot predict reader traffic or monthly diagnostic volume.

Retrieval establishes that these particular facts reached Railway's log store.
It does not prove complete ingestion of all process output.
The records contain no emission timestamp that permits an ingestion-latency calculation.
Queue saturation, failed output, deadline disposal, and abrupt termination can still omit records.
The readiness observation does not prove continuous readiness.
The shutdown result proves neither its own delivery in every occurrence nor successful process exit.

## Access and retention

The authenticated API inspected account and project settings at 09:55:51.
The workspace plan was `HOBBY`. Its subscription state was `ACTIVE`, and `isTrialing` was false.
The project had `isPublic=false`.

The workspace had one member with role `ADMIN`. That member matched the authenticated maintainer's user ID.
The project had no direct members or additional entries in `projectMembers`.
The authenticated maintainer could retrieve the listed diagnostic records.

These settings support maintainer-only configured access.
This inspection did not inspect credential distribution or attempt access with another person's identity.
No personal account identifiers, reader data, or privileged action evidence appear in this report.

[Railway's log-retention policy](https://docs.railway.com/observability/logs#log-retention) assigns a seven-day window to Hobby.
The observed active plan therefore matches the accepted seven-day Hobby log-window policy.
The inspected schema exposes no separate log-retention override.
This project is younger than seven days, so its available records cannot establish an expiration boundary empirically.
This acceptance concerns the platform log window. It makes no physical-deletion claim.

## Usage and cost

The tool read the current billing period at 09:53:43.
The period starts at 2026-10-25 21:06:50. It ends at 2026-11-25 21:06:50.
Amounts use United States dollars, rounded to six decimal places.

| Scope                     | Reported resource usage |
| ------------------------- | ----------------------: |
| Book Smartz app           |               $0.001018 |
| Book Smartz PostgreSQL    |               $0.022544 |
| Book Smartz project total |               $0.023562 |
| Entire shared workspace   |               $0.120814 |
| Estimated workspace bill  |               $0.907952 |

The project total includes the app and PostgreSQL together.
The shared-workspace figure includes other projects without exposing their details.
The difference between the accepted $5 resource-usage benchmark and current workspace usage is $4.879186.
This comparison describes planning headroom, not remaining credits or an enforced spending cap.
The tool reported no workspace usage limit.

[Railway's pricing policy](https://docs.railway.com/pricing/plans) describes the Hobby subscription and its included usage.
The tool estimate is not a final invoice.
Reported usage can lag actual consumption.
Subscription fees, credit treatment, taxes, and the final invoice were not reconciled during verification.

Future activity can change the estimate. The accepted possible overages remain possible.
No pricing, resource limit, or spend setting changed.

A separate API read requested the preceding 24 hours at 300-second intervals.
Each listed measurement returned 289 samples.

| Measurement      | App mean / maximum  | PostgreSQL mean / maximum |
| ---------------- | ------------------- | ------------------------- |
| CPU usage        | 0.000121 / 0.012593 | 0.000222 / 0.000372       |
| Memory usage, GB | 0.041338 / 0.044614 | 0.041191 / 0.049755       |
| Disk usage, GB   | Not requested       | 0.136062 / 0.136110       |

These summaries use the provider's measurement units and sampling behavior.
They are not billing integrals, a capacity test, or a representative traffic forecast.
The request defines the rolling window and sample interval. The connector does not return every sample timestamp in this summary.

## Authoritative history boundary

PostgreSQL uses its existing persistent volume at `/var/lib/postgresql/data`.
The database service has no public domain or TCP proxy.
Platform log retention operates separately from that volume.
Verification did not read, replay, expire, delete, or change private ranking or recovery history.

The deployed source retains PostgreSQL as the authority for ranking and recovery evidence.
The diagnostic path remains advisory and retains the silent storage observer in web composition.
This report does not claim a backup restore, disaster-recovery test, or validation of private history contents.

## Reproduction and evidence handling

Railway command-line tool version: `5.30.4`, using the maintainer's existing authenticated session.
The reads used explicit project, environment, service, and deployment selectors.
They did not link the repository or modify Railway configuration.

The retained private evidence contains the tool output, schema descriptions, image manifest, checked package layer, and bounded log export.
It also contains account-member identifiers needed for the identity comparison.
Those identifiers and raw account metadata remain outside the repository.
This report publishes only the bounded operational results needed for acceptance.

The read commands were:

- `railway whoami --json`
- `railway usage --workspace <workspace-id> --json`
- `railway usage projects --project <project-id> --workspace <workspace-id> --json`
- `railway usage limit status --target workspace --workspace <workspace-id> --json`
- `railway api --file <read-only-query> --variables <selector-json>`
- `railway logs <deployment-id> --project <project-id> --service <service-id> --environment <environment-id> --json --lines 500 --since 2026-10-27T00:40:00Z --until 2026-10-27T01:00:00Z`

The API query selected the active plan, membership roles, project privacy, and deployment metadata.
It contained no mutation and selected no variable values, payment details, or credential values.
