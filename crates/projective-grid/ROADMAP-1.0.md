# Road to `projective-grid` 1.0

The crate remains on the independent `0.14.x` line. The ordinary facade is
already the intended shape:

```rust
let request = DetectionRequest::new(lattice, evidence)
    .with_dimensions(dimensions)
    .with_params(params);
let detection = detect_grid(request)?;
```

Before 1.0:

1. establish real-image evidence for Square `Positions` / `Oriented1` and all
   Hex paths;
2. freeze or narrow the `expert` surface;
3. complete a public-API and semver audit against the latest published 0.x;
4. publish and exercise a release candidate.

Release procedure (independent `projective-grid-vX.Y.Z` tags, Trusted
Publishing, release notes): see
[`docs/development/release-gates.md`](../../docs/development/release-gates.md#releasing).
