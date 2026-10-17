# Public test key

This deliberately public RSA fixture comes from the `MIT`-licensed
`jsonwebtoken` 11.1.0 test suite at `tests/rsa/private_rsa_key_pkcs8.pem`.
It has no production use.

`browser-test-certificate.pem` is a self-signed certificate for the reserved fixture
host `reader.example`, using the existing public test-only key. The browser
acceptance proxy uses it only on loopback. Chromium ignores certificate trust
for this isolated fixture. Never use this public key or certificate for deployment.
