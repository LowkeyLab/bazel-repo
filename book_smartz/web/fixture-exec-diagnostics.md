# Fixture executable diagnostics

Fixture process failures now report their stage, executable identity, and bounded writer observations.
The original intermittent writer remains unknown.
The two bounded natural probes passed. A later full validation reproduced error 26 and retained writer evidence.
This change adds diagnostic evidence rather than a lifecycle repair.

The [original failed coverage attempt](https://github.com/LowkeyLab/bazel-repo/actions/runs/40560487083/attempts/1)
reported `Text file busy (os error 26)` in these app tests:

```text
action_test::evidence_recovery_preserves_validation_scope_first_facts_and_terminal_precedence
action_test::concurrent_registration_converges_and_rejected_start_retains_metadata
```

That output did not identify the executable, start operation, or writer.
The unchanged failed-job retry passed. These results do not establish a cause.

Each `Fixture` owns its PostgreSQL container, provider, process guard, package directory, working directory, and diagnostic file.
The package comes from the production `:package` archive.
The fixture extracts it into its own temporary directory before the first migration.
It then starts the server and waits for readiness. Restarts and additional instances use the same fixture-owned package.

| Stage                        | Existing operation                                                      |
| ---------------------------- | ----------------------------------------------------------------------- |
| Preparation contexts         | PostgreSQL startup, archive open, and archive extraction                |
| `fixture_migrate`            | `Command::status` for the unpacked binary's migration                   |
| `fixture_serve`              | `Command::spawn` for initial, restarted, or additional server instances |
| `package_rejected_startup`   | Start the binary for existing rejection assertions                      |
| `package_concurrent_migrate` | Start each existing concurrent migration process                        |
| `launcher_migrate`           | Run the existing launcher from an unrelated directory                   |

Each wrapper calls the existing `Command` method once.
The caller retains its standard-stream configuration and child ownership.
Failure remains an error. Existing assertions and test parallelism remain in use.
The wrappers collect observations only after a method fails, before fixture ownership unwinds.
They do not trace commands inside a running launcher.

The error context starts with `fixture_process_failure` and a versioned JSON record.
It retains the stage, method, operating-system error number, error kind, and current process ID.
It also retains the basename, device number, and `inode` of the executable when available.
The original I/O error remains available through the error chain.
A `status` error can also describe a wait failure, as the [Rust method contract](https://doc.rust-lang.org/std/process/struct.Command.html#method.status) explains.

For error 26, the observer examines open descriptors that match the device and `inode` of the executable.
It records matching process IDs, descriptor numbers, and write access modes.
Device and `inode` identify the file within that worker's filesystem.
The [Linux descriptor format](https://man7.org/linux/man-pages/man5/proc_pid_fdinfo.5.html) defines the octal access flags.
Other operating-system errors retain context without a writer scan.

| Bound                  | Limit                                                         |
| ---------------------- | ------------------------------------------------------------- |
| Process enumeration    | Own process first, then at most 256 `/proc` directory entries |
| Descriptor enumeration | At most 4,096 entries across the scan                         |
| Retained writers       | At most 16                                                    |
| Descriptor information | At most 4,096 bytes per matching descriptor                   |
| Executable label       | At most 80 characters from the basename                       |

The record includes budget exhaustion and counts of unreadable or changing entries.
The observer discards an identity that changes during flag collection.
It excludes command arguments, environment values, working-directory paths, process command lines, executable contents, and reader payloads.
The observer excludes read-only descriptors and unrelated files with the same basename.

These are entry bounds rather than a wall-clock deadline.
Observations describe the visible snapshot after the error.
A writer can close before observation or be outside the visible namespace.
An incomplete scan, or a scan without an observed writer, cannot establish absence at the failure instant.
The `identity_observed_after_error` field makes this timing explicit.

The controlled test holds a writable descriptor on the actual unpacked production binary.
Its launch returns error 26, and the report identifies the held descriptor and matching file identity.
The [Linux execution contract](https://man7.org/linux/man-pages/man2/execve.2.html) documents this open-writer failure.
Closing the artificial writers permits execution again.
This demonstrates the reporting mechanism. It does not establish the writer behind the original intermittent failure.

Run the controlled diagnostic assertions with:

```sh
nix develop --command aspect test //book_smartz/web:fixture_process_test \
  --nocache_test_results --noremote_accept_cached --flaky_test_attempts=1
```

The target verifies write-only and read/write descriptors, read-only exclusion, unrelated-file exclusion, private-value exclusion, missing executables, and writer-limit exhaustion.
It also verifies that both process methods preserve the original error.
Private filesystem fixtures verify missing process directories, vanished descriptor targets, and enumeration limits.
They also verify unavailable flags and malformed flags.
A controlled error input checks identity loss after a failed start.
These checks use the same observer with a private root path. The normal wrapper supplies `/proc`.
They do not simulate the kernel execution refusal or prove failure-time descriptor races.
The `controlled-held-writer.json` test output retains the controlled observation.
The [initial regression](https://app.buildbuddy.io/invocation/e7af5fcc-7d16-429a-bf2b-e7d6e0c7dbaa) failed because the structured diagnostic was missing.
The [first instrumented control](https://app.buildbuddy.io/invocation/49ec6bc6-1a37-41d7-bcfb-f410ebd8511b) passed and retained the writer identity.

Run a bounded natural probe with:

```sh
nix develop --command aspect test \
  //book_smartz/web:package_test //book_smartz/web:application_test \
  --runs_per_test=2 --nocache_test_results --noremote_accept_cached \
  --flaky_test_attempts=1 --test_output=errors
```

The following target executions used real PostgreSQL and the existing controlled providers.
Every listed test run executed. Build inputs can still use caches.
The two bounded probes completed eight natural target executions in total.

| Probe                                                                                               | Target  | Executions | Result | Recorded duration range |
| --------------------------------------------------------------------------------------------------- | ------- | ---------- | ------ | ----------------------- |
| [Before instrumentation](https://app.buildbuddy.io/invocation/e59bc873-bb6c-46ac-85cc-c852f39ec887) | App     | 2          | Passed | 256.6 to 283.3 seconds  |
| Before instrumentation                                                                              | Package | 2          | Passed | 42.0 to 52.7 seconds    |
| [With instrumentation](https://app.buildbuddy.io/invocation/ff36190c-09b6-44ad-a223-ad13b150ab2c)   | App     | 2          | Passed | 242.9 to 272.6 seconds  |
| With instrumentation                                                                                | Package | 2          | Passed | 38.4 to 47.7 seconds    |

The instrumented probe also executed the diagnostic target twice. Both runs passed in 0.1 seconds each.
Individual test logs, XML results, and controlled observations remain in the invocation artifacts.
These samples do not establish a performance change or reproduce the original intermittent failure.
A [later full validation](https://app.buildbuddy.io/invocation/1224ecad-9392-411d-85bc-bed03b9b3d09) failed two HTTP cases during `fixture_migrate`:

```text
catalog_test::explicit_search_normalizes_work_results_and_issues_selections
diagnostic_test::catalog_schema_failure_keeps_its_stage_and_safe_reason
```

Both records identified `book-smartz`, operation `status`, and error 26.
The first record found no writer across 124 processes and 502 descriptors, with no unreadable entries.
That post-error snapshot does not establish absence at the failure instant.
The second record retained this matching writer:

```json
{
  "pid": 3057,
  "fd": 39,
  "access": "write_only",
  "device": 65024,
  "inode": 135161
}
```

The reporting test process was PID 462. The second scan marked one entry unreadable and `complete: false`.
These identifiers describe that worker only. The snapshot did not identify the writer's command or owning operation.
The controlled tests passed in that validation.
The app case `placement_actions_authorize_each_post_and_reconcile_before_expired_session_resume` separately reported `application exited exit status: 1`.
That output did not report error 26. A documentation lint error also failed and required correction.
The retained failed test output remains evidence even if a later configured validation passes.
A lifecycle repair still requires evidence of the writer's owning operation and lifetime.
