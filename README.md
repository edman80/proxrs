# proxrs

[![CI](https://github.com/edman80/proxrs/actions/workflows/ci.yml/badge.svg)](https://github.com/edman80/proxrs/actions/workflows/ci.yml)

A tiny TLS-terminating reverse proxy. It listens for HTTPS connections, picks
a certificate and a backend by SNI hostname, and forwards each request to
that backend over plain HTTP, streaming the response back.

That's the whole feature set: no load balancing, no caching, no path-based
routing, no connection pooling. One hostname maps to one backend. If you
need more than that, you probably want nginx or Caddy.

## Building

```
cargo build --release
```

The binary is `target/release/proxrs`.

### Debian package

A `.deb` can be built with [cargo-deb](https://github.com/kornelski/cargo-deb):

```
cargo install cargo-deb
cargo deb
```

This produces `target/debian/proxrs_<version>-1_<arch>.deb`, which installs:

- the binary to `/usr/bin/proxrs`
- an example config to `/etc/proxrs/proxy.toml.example`
- a systemd unit to `/lib/systemd/system/proxrs.service`

To use it:

```
sudo dpkg -i target/debian/proxrs_*.deb
sudo cp /etc/proxrs/proxy.toml.example /etc/proxrs/proxy.toml
sudo $EDITOR /etc/proxrs/proxy.toml   # point it at your real certs/backends
sudo systemctl daemon-reload
sudo systemctl enable --now proxrs
```

The unit does not start automatically on install, since it can't run
correctly until you've supplied a real config and certs.

Every tagged push (`vX.Y.Z`) builds a `.deb` in CI and attaches it to a
GitHub release — see [Releasing](#releasing) below.

## Configuring

Copy the example config and edit it:

```
cp proxy.toml.example proxy.toml
```

```toml
listen = "0.0.0.0:443"

[[hosts]]
domain = "etf.sh"
cert = "/etc/proxrs/certs/etf.sh.pem"
key = "/etc/proxrs/certs/etf.sh.key"
backend = "127.0.0.1:8000"

[[hosts]]
domain = "counter.etf.sh"
cert = "/etc/proxrs/certs/counter.etf.sh.pem"
key = "/etc/proxrs/certs/counter.etf.sh.key"
backend = "127.0.0.1:8001"
```

- `listen` — address:port the proxy accepts HTTPS connections on.
- `[[hosts]]` — one entry per hostname you're fronting. Add as many as you
  like; each gets its own cert/key and backend.
  - `domain` — the hostname a client connects with (matched via TLS SNI).
  - `cert` / `key` — PEM-encoded certificate chain and private key for that
    domain.
  - `backend` — where to forward requests for that domain, as `host:port`.
    The proxy talks plain HTTP to this address, so it should be on a
    trusted network (localhost, a private VPC, etc.).

`proxrs` does **not** obtain or renew certificates itself — point it at
files managed by something else (certbot, an ACME client run separately,
certs handed to you by your host). Re-run/restart the proxy after certs are
renewed so it picks up the new files.

## Running

```
proxrs [path/to/config.toml]
```

The config path defaults to `proxy.toml` in the current directory.

Binding to port 443 requires elevated privileges on most systems; either
run as root or grant the binary the capability, e.g. on Linux:

```
sudo setcap 'cap_net_bind_service=+ep' target/release/proxrs
```

Logging goes to stdout via `tracing`. Control verbosity with `RUST_LOG`:

```
RUST_LOG=info proxrs proxy.toml
```

Each proxied request logs its method, path, backend, and response status;
backend failures are logged and answered with `502 Bad Gateway`. A
connection for a hostname not listed in the config is rejected during the
TLS handshake (no certificate is presented for it).

## Testing locally

Generate a throwaway self-signed cert and point a dummy backend at it:

```
openssl req -x509 -newkey rsa:2048 -keyout localhost.key -out localhost.pem \
  -days 3 -nodes -subj "/CN=localhost"

python3 -m http.server 9000 &
```

```toml
# proxy.toml
listen = "127.0.0.1:8443"

[[hosts]]
domain = "localhost"
cert = "localhost.pem"
key = "localhost.key"
backend = "127.0.0.1:9000"
```

```
cargo run -- proxy.toml
curl -k https://localhost:8443/
```

## CI

GitHub Actions (`.github/workflows/ci.yml`) runs on every push to `main` and
every pull request: `cargo fmt --check`, `cargo clippy -- -D warnings`,
`cargo build`, and `cargo test`.

## Releasing

Pushing a tag matching `v*` (e.g. `v0.1.0`) triggers
`.github/workflows/release.yml`, which builds a `.deb` on `ubuntu-latest`
and publishes it as a GitHub release.

Tags are cut with [cargo-release](https://github.com/crate-ci/cargo-release)
(`cargo install cargo-release`), which bumps the version in `Cargo.toml`,
commits, tags, and pushes in one step. `[package.metadata.release]` in
`Cargo.toml` sets `publish = false` (this crate isn't published to
crates.io) and `tag-name = "v{{version}}"` to match the release workflow's
trigger. It defaults to a dry run — pass `--execute` to actually do it:

```
cargo release patch --execute   # 0.1.0 -> 0.1.1
cargo release minor --execute   # 0.1.0 -> 0.2.0
```
