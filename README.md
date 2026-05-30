# srvcs-slope

The slope orchestrator of the srvcs.cloud distributed standard library.

Its single concern: **geometry: slope between two points.** It owns the
*control flow* — composing two float primitives — but does no arithmetic of its
own. It asks [`srvcs-floatsubtract`](https://github.com/srvcs/floatsubtract) for
the rise `dy = y2 - y1` and the run `dx = x2 - x1`, then asks
[`srvcs-floatdivide`](https://github.com/srvcs/floatdivide) for the slope
`dy / dx`.

```
slope(x1, y1, x2, y2):
    dy = floatsubtract(y2, y1)     # rise
    dx = floatsubtract(x2, x1)     # run
    return floatdivide(dy, dx)     # dy / dx
```

The result is an `f64` — a JSON number that may be fractional. For example
`slope(0, 0, 2, 4) == 2.0` (rise `4`, run `2`).

A **vertical line** has `dx == 0` and no defined slope. This service does not
special-case it: `srvcs-floatdivide` rejects division by zero with a `422`,
which is forwarded verbatim.

Validation is not handled here either. This service never calls
`srvcs-isnumber` directly; instead its dependencies validate their own operands,
and any `422` they raise is forwarded.

## API

| Method | Path | Purpose |
| --- | --- | --- |
| `GET` | `/` | Service identity, concern, and dependency list |
| `POST` | `/` | Compute the slope of the line through two points |
| `GET` | `/healthz` `/readyz` `/metrics` `/openapi.json` | srvcs service standard surface |

```sh
curl -s -X POST localhost:8080/ -H 'content-type: application/json' \
  -d '{"x1": 0, "y1": 0, "x2": 2, "y2": 4}'
# {"x1":0,"y1":0,"x2":2,"y2":4,"result":2.0}
```

Responses:

- `200 {"x1", "y1", "x2", "y2", "result": n}` — evaluated; `result` is a float.
- `422` — a dependency rejected an input, or the line is vertical (`dx == 0`);
  forwarded verbatim.
- `500` — a reachable dependency returned a `200` without a numeric `result`
  (a contract violation).
- `503` — a dependency is unavailable.

## Dependencies

- [`srvcs-floatsubtract`](https://github.com/srvcs/floatsubtract)
- [`srvcs-floatdivide`](https://github.com/srvcs/floatdivide)

## Configuration

| Variable | Default | Purpose |
| --- | --- | --- |
| `SRVCS_BIND_ADDR` | `0.0.0.0:8080` | Bind address |
| `SRVCS_FLOATSUBTRACT_URL` | `http://127.0.0.1:8090` | Base URL of `srvcs-floatsubtract` |
| `SRVCS_FLOATDIVIDE_URL` | `http://127.0.0.1:8091` | Base URL of `srvcs-floatdivide` |
| `SRVCS_ENV` | `development` | Environment label for logs |
| `RUST_LOG` | `info,tower_http=info` | Tracing filter |

## Local checks

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Orchestration tests stand up *computing* mock dependency services in-process —
they read the request body and return the real `a - b` / `a / b`, so the
composition is genuinely exercised against the asserted cases (compared
approximately, since the result is a float). See
[`srvcs/platform`](https://github.com/srvcs/platform) for the shared standard.

> Note: the `cargoHash` in `flake.nix` is inherited from the template and must be
> refreshed with a `nix build` before the Nix gates pass.
