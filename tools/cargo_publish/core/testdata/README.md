# Cargo report interoperability fixtures

The library, binary, and combined report/archive pairs come from actual Python Cargo validation runs on October 11, 2026. They provide an independent baseline for schema and archive interoperability during the Rust migration. They are not evidence of fresh validation by the Rust command.

The retained source directory is `2161-acquire-dvje1ygg/real-evidence` under the private worktree-pool agent receipts:

- Library: `library/runs/2944a486-dac2-4678-ab6f-cd796f303a6a`.
- Binary: `binary/runs/014b0470-e299-4ac9-b805-0bbe7a7618e7`.
- Combined: `combined/runs/159fb035-f6b7-4e22-b510-965bec196b18`.

Each report preserves the evidence copied from `report.json`. Each archive was copied byte for byte from `target/package/<name>-0.1.0.crate`. Tests embed these repository files, copy the archives to temporary locations, and require the reported hashes, sizes, inventory, manifests, and lockfile to match the actual copied bytes. Absolute paths in reports are diagnostic evidence from the original runs; they are not used to find or extract fixture bytes.

The failed-spawn test uses a literal matching the existing Python producer's error schema. It does not claim a retained production failure run.
