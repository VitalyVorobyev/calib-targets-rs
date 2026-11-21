# Road to `projective-grid` 1.0

The crate is versioned on its own `0.x` line, independent of the workspace.
The ordinary facade is the intended 1.0 shape:

```rust
let request = DetectionRequest::new(lattice, evidence)
    .with_dimensions(dimensions)
    .with_params(params);
let detection = detect_grid(request)?;
```

Before 1.0:

1. real-image evidence for the experimental evidence kinds — Gap 21 in
   [`algorithmic_gaps.md`](../../docs/algorithms/algorithmic_gaps.md);
2. an API-surface pass that makes `expert` a compatibility boundary — Gap 22;
3. complete a public-API and semver audit against the latest published 0.x;
4. publish and exercise a release candidate.

Release procedure (independent `projective-grid-vX.Y.Z` tags, Trusted
Publishing, release notes): see
[`docs/development/release-gates.md`](../../docs/development/release-gates.md#releasing).
