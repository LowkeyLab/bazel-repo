# Conventional Rust crate roots

Library/binary root migration for #1957, following the maintainer override to #1955.
Integration runners and build scripts retain Rust conventions. Target labels remain unchanged.

| Previous root                              | New root                                       | Preserved target                                         |
| ------------------------------------------ | ---------------------------------------------- | -------------------------------------------------------- |
| `nicknamer2/src/auth/claims.rs`            | `nicknamer2/src/auth/claims/lib.rs`            | `//nicknamer2/src/auth:auth_claims`                      |
| `nicknamer2/src/auth/auth.rs`              | `nicknamer2/src/auth/lib.rs`                   | `//nicknamer2/src/auth:auth`                             |
| `nicknamer2/src/discord_server/server.rs`  | `nicknamer2/src/discord_server/server/lib.rs`  | `//nicknamer2/src/discord_server:discord_server`         |
| `nicknamer2/src/discord_server/repo.rs`    | `nicknamer2/src/discord_server/repo/lib.rs`    | `//nicknamer2/src/discord_server:discord_server_repo`    |
| `nicknamer2/src/discord_server/service.rs` | `nicknamer2/src/discord_server/service/lib.rs` | `//nicknamer2/src/discord_server:discord_server_service` |
| `nicknamer2/src/server/server.rs`          | `nicknamer2/src/server/lib.rs`                 | `//nicknamer2/src/server:server`                         |
| `nicknamer2/src/name/name.rs`              | `nicknamer2/src/name/lib.rs`                   | `//nicknamer2/src/name:name`                             |
| `nicknamer2/src/name/repo.rs`              | `nicknamer2/src/name/repo/lib.rs`              | `//nicknamer2/src/name:name_repo`                        |
| `nicknamer2/src/name/service.rs`           | `nicknamer2/src/name/service/lib.rs`           | `//nicknamer2/src/name:name_service`                     |
| `nicknamer2/src/migrations/migrations.rs`  | `nicknamer2/src/migrations/lib.rs`             | `//nicknamer2/src/migrations:migrations`                 |
| `nicknamer2/src/graphql/context.rs`        | `nicknamer2/src/graphql/context/lib.rs`        | `//nicknamer2/src/graphql:graphql_context`               |
| `nicknamer2/src/graphql/relay.rs`          | `nicknamer2/src/graphql/relay/lib.rs`          | `//nicknamer2/src/graphql:graphql_relay`                 |
| `nicknamer2/src/graphql/model.rs`          | `nicknamer2/src/graphql/model/lib.rs`          | `//nicknamer2/src/graphql:graphql_model`                 |
| `nicknamer2/src/graphql/query.rs`          | `nicknamer2/src/graphql/query/lib.rs`          | `//nicknamer2/src/graphql:graphql_query`                 |
| `nicknamer2/src/graphql/mutation.rs`       | `nicknamer2/src/graphql/mutation/lib.rs`       | `//nicknamer2/src/graphql:graphql_mutation`              |
| `nicknamer2/src/graphql/schema.rs`         | `nicknamer2/src/graphql/schema/lib.rs`         | `//nicknamer2/src/graphql:graphql_schema`                |
| `nicknamer2/src/graphql/mod.rs`            | `nicknamer2/src/graphql/lib.rs`                | `//nicknamer2/src/graphql:graphql`                       |
| `nicknamer2/src/config/config.rs`          | `nicknamer2/src/config/lib.rs`                 | `//nicknamer2/src/config:nicknamer2_config`              |
| `book_smartz/storage/usage.rs`             | `book_smartz/storage/usage/lib.rs`             | `//book_smartz/storage:usage`                            |
| `test_images/rust/probe.rs`                | `test_images/rust/probe/main.rs`               | `//test_images/rust:postgres_probe`                      |

The maintained parser helper also moved from `upstream/rust_parser/lockfile_crates.rs`
to `upstream/rust_parser/lockfile_crates/lib.rs`, preserving
`@gazelle_rust//rust_parser:lockfile_crates`.
