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

The 0.14 release adds the lattice-orientation parity invariant to the shared
precision pass and makes the component merge take one shared `positions` slice,
so a labelled set stays injective in both directions — one lattice coordinate
holds one corner, and one corner sits at one coordinate. That guarantee is only
expressible when every component indexes the same corner array, which is why
`ComponentInput` is gone.

## Independent releases

Push `projective-grid-vX.Y.Z` only after `Cargo.toml` contains the same `X.Y.Z`
and the release commit is on `main`. The dedicated GitHub workflow tests,
packages, and publishes only `projective-grid`; ordinary workspace releases
continue to use `vX.Y.Z` and do not publish this crate. When workspace crates
raise their dependency to a new unpublished Projective Grid version, publish
the Projective Grid tag first and wait for crates.io indexing before the
workspace tag.

crates.io Trusted Publishing must authorize repository
`VitalyVorobyev/calib-targets-rs`, workflow `publish-projective-grid.yml`, and
environment `crates-io`. No long-lived registry token is stored in GitHub.
The workflow requires a matching section in `CHANGELOG.md` and uses that
section verbatim to create the GitHub Release after crates.io publication.
Preview the exact release body locally with
`python3 scripts/projective_grid_release_notes.py X.Y.Z`.
