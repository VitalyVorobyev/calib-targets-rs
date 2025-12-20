# Conventions

These conventions are used throughout the workspace. They are not optional and should not change silently.

## Coordinate systems

- Image pixels: origin at top-left, `x` increases right, `y` increases down.
- Grid coordinates: `i` increases right, `j` increases down.
- Grid indices in detections are **corner indices** (intersections), not square indices, unless explicitly stated otherwise.

## Homography and quad ordering

- Quad corner order is always **TL, TR, BR, BL** (clockwise).
- The ordering must match in both source and destination spaces.
- Never use self-crossing orders like TL, TR, BL, BR.

## Sampling and pixel centers

- Warping and sampling should be consistent about pixel centers.
- When in doubt, treat sample locations as `(x + 0.5, y + 0.5)` in pixel space.

## Orientation angles

- ChESS-style corner orientations are in radians and defined modulo `pi` (not `2*pi`).
- Orientation clustering finds two dominant directions and assigns each corner to cluster 0 or 1, or marks it as an outlier.

## Marker bit conventions

- Marker codes are packed in row-major order.
- Black pixels represent bit value 1.
- Border width is defined in whole cells (`border_bits`).

If you introduce new algorithms or data structures, document any additional conventions in the relevant crate chapter.
