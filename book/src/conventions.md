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

- **Image coordinates put pixel centres at integers.** Pixel `(x, y)` is centred
  at `(x, y)` and covers `[x − ½, x + ½) × [y − ½, y + ½)`. Every detected
  position uses this convention: a corner on the boundary between pixel columns
  15 and 16 is reported at `x = 15.5`. (OpenCV uses the same convention.)
- The bilinear samplers (`sample_bilinear*`) take image coordinates in this
  convention: sampling at integer `(x, y)` returns pixel `(x, y)` exactly.
- Rectified views are the exception that needs the `+ 0.5`: a rectified pixel
  `(x, y)` covers `[x, x + 1)` in rectified space, so `warp_perspective_gray`
  samples its centre at `(x + 0.5, y + 0.5)` before mapping it into the image.
- To convert a position to "area" coordinates, where pixel `x` covers
  `[x, x + 1)` (some renderers and rasterizers), add `0.5` to each coordinate.

## Orientation angles

- ChESS-style corner orientations are in radians and defined modulo `pi` (not `2*pi`).
- Orientation clustering finds two dominant directions and assigns each corner to cluster 0 or 1, or marks it as an outlier.

## Marker bit conventions

- Marker codes are packed in row-major order.
- Black pixels represent bit value 1.
- Border width is defined in whole cells (`border_bits`).

If you introduce new algorithms or data structures, document any additional conventions in the relevant crate chapter.
