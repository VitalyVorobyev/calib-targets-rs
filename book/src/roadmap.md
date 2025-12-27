# Roadmap

This project is in early development, and APIs can change. The roadmap below focuses on a correctness-first path toward a stable, composable API.

## Cross-cutting goals

- Expand test coverage with real and synthetic datasets.
- Clarify data conventions (bit order, polarity, grid indexing) in docs and code.
- Add benchmark fixtures once correctness stabilizes.
- MSRV is set to Rust 1.70; add an explicit CI check for it.

## calib-targets-aruco

**Phase 1 (stabilize decoding)**

- Document bit packing, polarity, and border rules in code and docs.
- Add golden tests for built-in dictionaries and common marker sizes.
- Improve error reporting and return metadata (score, border quality).

**Phase 2 (robust scanning)**

- Add adaptive thresholding and better sampling strategies in rectified space.
- Support configurable polarity handling and sub-sampling strategies.
- Optimize matching with precomputed structures for large dictionaries.

**Phase 3 (optional image-space detection)**

- Consider a separate crate or feature for quad detection if needed.
- Keep grid-first decoding as the primary, predictable path.

## calib-targets-charuco

**Phase 1 (layout + alignment robustness)**

- Add more marker layouts beyond OpenCV-style.
- Improve alignment robustness with stronger inlier selection.
- Expose alignment diagnostics and failure reasons.

**Phase 2 (multi-board and calibration hooks)**

- Support multiple boards per image.
- Provide object-space corner coordinates for calibration pipelines.
- Optional integration points for camera intrinsics in rectification.

**Phase 3 (performance and UX)**

- Cache per-board data (marker positions, transforms).
- Provide stable, ergonomic defaults and presets.

## calib-targets-marker

**Phase 1 (complete the detector)**

- Circle detection and scoring on top of the existing grid model (done).
- Match circle centers to the layout and expose grid offsets; add explicit marker IDs.
- Expand tests and add a synthetic data generator to validate circle scoring.

**Phase 2 (robustness)**

- Partial boards and missing corners are supported; refine alignment validation.
- Improve circle scoring under blur, noise, and non-uniform lighting.

## calib-targets (facade)

**Phase 1 (usable facade)**

- Add a real library crate with re-exports and top-level builders.
- Provide a minimal trait for corner detectors and adapters.
- Unify error types and configuration defaults.

**Phase 2 (API stabilization)**

- Consolidate configuration structs and feature flags.
- Ship a stable v0.x API with clear upgrade notes.

**Phase 3 (long-term support)**

- Document versioning policy and compatibility expectations.
- Expand examples and tutorial-style docs in this book.
