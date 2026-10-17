# Book Smartz web app

The first web slice signs in with Google, persists browser sessions in PostgreSQL,
and reads the existing private ranking and pending placement. Search and ranking
submission controls belong to the following tickets in the minimum viable product specification.

Build the relocatable archive with `aspect build //book_smartz/web:package`.
It contains `book-smartz`, adjacent `assets`, and `integrity.json`. Run it from
any directory. `serve` verifies executable/asset integrity and the required schema
before listening. Ordinary requests never apply migrations.

Supply `DATABASE_URL` to `book-smartz migrate`. Supply the same database plus
`BOOK_SMARTZ_ORIGIN`, `GOOGLE_CLIENT_ID`, and `GOOGLE_CLIENT_SECRET` to
`book-smartz serve`. The origin must be the exact canonical HTTPS origin without
a trailing slash. Register `<origin>/auth/google/callback` with the Google client.
Keep credentials in runtime configuration. `HOST` and `PORT` configure the listener.
Use `HOST=0.0.0.0` behind the intended HTTPS reverse proxy.

`bazel run //book_smartz/web:run -- migrate` and
`bazel run //book_smartz/web:run -- serve` use that same archive and forward arguments.
The migration command has a two-minute bound. The initial pool allows five
connections, with five-second acquisition/lock and fifteen-second statement
timeouts. These are settings, not measured capacity claims.

Sessions use opaque Topcoat tokens, hashed records, explicit secure HttpOnly
cookies, a fixed 30-day expiry, and browser-scoped revocation. Ownership follows
Google issuer/subject rather than email. Logout requires its own session-bound cross-site request forgery
token and the configured canonical Origin. Ranking GETs derive reader authority
from the stored session. Reader identifiers supplied by browsers confer none.
The app installs a silent storage observer so existing reader-identifying
storage diagnostics don’t become routine web diagnostics. The operations and
release slice adds its restricted diagnostic listener and operations guide.

The HTTP and app suites run the actual archive with isolated PostgreSQL
and controlled Google token/JSON Web Key endpoints. Package acceptance checks delivered
CSS, local fonts, icons and runtime JavaScript from unrelated directories, plus
startup rejection of missing/mismatched assets and incompatible schema. These
checks don’t establish live Google compatibility or launch readiness.

Run `aspect test //book_smartz/web:http_test
//book_smartz/web:application_test //book_smartz/web:package_test` on one shell line.
The optional `GOOGLE_AUTHORIZATION_ENDPOINT`, `GOOGLE_TOKEN_ENDPOINT`, and
`GOOGLE_JWKS_ENDPOINT` settings let acceptance fixtures control provider I/O.
Production defaults use Google's endpoints. HTTP overrides accept only explicit
loopback addresses. The app contains no fixture identity or bypass route.

The protocol checks follow [Google's OpenID Connect validation requirements](https://developers.google.com/identity/openid-connect/openid-connect)
and [Proof Key for Code Exchange](https://www.rfc-editor.org/rfc/rfc7636.html).
