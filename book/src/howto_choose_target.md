# How to choose a target

Six target families ship here, and the differences that matter are not
subtle. This page is the decision, not the tour — each row links to the
reference page for the family.

## The short version

**Calibrating one camera, board fully visible, no other constraints?** Use a
[chessboard](chessboard.md). It is the simplest thing that works and there is
nothing to configure.

**Need pose from any direction, or from a target that cannot be kept facing
the camera?** Use a [PuzzlePole](tutorial_puzzlepole.md) — it is the only
family here that is not flat.

**Anything else?** Read the table.

## The table

| Family | Corner IDs | Partial views | Needs | Reference |
|---|---|---|---|---|
| [Chessboard](chessboard.md) | none | ✗ whole board | nothing | [pipeline](pipeline_chessboard.md) |
| [ChArUco](charuco.md) | absolute | ✓ | an ArUco dictionary | [pipeline](pipeline_charuco.md) |
| [PuzzleBoard](puzzleboard.md) | absolute | ✓ down to a small fragment | nothing | [pipeline](pipeline_puzzleboard.md) |
| [Marker board](marker.md) | absolute | ✓ | a custom circle layout | [pipeline](pipeline_marker.md) |
| [PuzzlePole](tutorial_puzzlepole.md) | absolute, **3-D** | ✓ from any azimuth | a tube to wrap it on | [how-to](howto_print_puzzlepole.md) |
| Regular grid | none | ✗ | nothing | [pipeline](pipeline_regular_grid.md) |

**Corner IDs** is the property that decides most cases. Without them a
detection is a set of corners in *some* order, so the board has to be fully
visible for its corners to mean anything. With them every corner carries an
absolute identity, so a partial view is still useful and the board's
orientation is never ambiguous.

## Choosing between the identified families

They differ in how the identity is encoded, which is what drives the
resolution each one needs.

**ChArUco** puts an ArUco marker inside every white square. Each marker is a
self-contained code, so a single readable marker locates the board — but a
marker needs enough pixels to resolve its bit grid, so ChArUco wants the most
resolution of the three. Pick it when you already have ArUco in the stack, or
when you want a format other tools recognise.

**PuzzleBoard** distributes the code across neighbouring cells: one bit per
edge, read as *is the dot between these two corners black or white*. A bit
therefore costs one dot rather than a whole marker grid, which is why it stays
decodable at markedly lower resolution and from smaller fragments. Pick it
when the board will be small in frame, partly occluded, or oblique. It is the
best-covered family in this book — see [the decode](algo_puzzleboard_decode.md)
and [the code maps](algo_puzzleboard_code_maps.md).

**PuzzlePole** is the PuzzleBoard pattern wrapped round a cylinder, so it is
the same code read the same way — but on a surface with no back. Every other
family here degrades as it turns away from the camera and stops working
altogether near edge-on; a pole presents the same face at every azimuth, and its
corners come with 3-D object points instead of points on a plane. Pick it when
the target must be viewed from several directions at once, when it cannot be
aimed at the camera, or when you want a single-view pose without a rig. The
cost is that it must be wrapped accurately on a tube of the right diameter, and
that only seven circumferences wrap seamlessly.

**Marker board** is a plain checkerboard with three circular markers placed to
break its symmetry. It carries far less information than the
others — just enough to fix the frame — but it prints as an ordinary chessboard with three
dots, which suits setups where the full pattern is impractical.

## Under blur

If the board will be out of focus — a shallow depth of field, a tilted
sensor, motion — the families part ways. The table gives the fraction of a
board's 63 inner corners delivered with the correct label, against the blur
`σ` as a fraction of the square size. These are synthetic renders (the
accuracy suite's renderer): a Gaussian lens blur convolved with the pixel's
area (`σ` is the total), squares of 24 and 32 px, rotated 8° and 30°, with and
without a 30° perspective tilt, sensor noise 1 and 4 gray levels, default
parameters; each entry pools 8 renders. No render delivered a wrong label.

| Family | Square | σ/cell 0.05 | 0.1 | 0.15 | 0.2 | 0.25 | 0.3 |
|---|---|---|---|---|---|---|---|
| Chessboard | 24 px | 1.00 | 1.00 | 1.00 | 1.00 | 1.00 | 0.99 |
| | 32 px | 1.00 | 1.00 | 1.00 | 1.00 | 0.99 | 0.75 |
| ChArUco | 24 px | 0.88 | 0 | 0 | 0 | 0 | 0 |
| | 32 px | 1.00 | 0.52 | 0 | 0 | 0 | 0 |
| PuzzleBoard | 24 px | 1.00 | 1.00 | 1.00 | 0.62 | 0 | 0 |
| | 32 px | 1.00 | 1.00 | 1.00 | 0.62 | 0 | 0 |
| Marker board | 24 px | 1.00 | 1.00 | 0 | 0 | 0 | 0 |
| | 32 px | 1.00 | 1.00 | 0 | 0 | 0 | 0 |

What an identified family survives is set by the size of the feature that
carries its code, relative to the square. A ChArUco marker bit is 1/8 of a
square (a 6 × 6-bit marker filling 0.75 of it), and the marker's black border
comes within 1/8 of a square of every corner: from `σ` = 0.1 of a square the
detector mostly fails to find the corner grid at all. A PuzzleBoard bit is a
dot a third of a square across ([Stelldinger et al.
2024](references.md#stelldinger2024)), and the board decodes fully up to
0.15 — two to three times the blur ChArUco tolerates at the same square
size — partly at 0.2 (where about half of the delivered corners lie more than
1 px from the truth), and not at 0.25. The marker board's circles are half a square,
yet its circle search finds none from 0.15: there its detector, not the
feature size, is the limit. A plain chessboard needs only its corners and
keeps them longest, but carries no identity.

## Then choose the size

Two independent numbers, and both are constrained by the camera rather than by
taste:

- **Square size.** Big enough that a square spans a comfortable number of
  pixels at your working distance. If corners come out noisy, this is usually
  the reason before any parameter is.
- **Board extent.** Big enough to fill a useful fraction of the frame across
  the poses you intend to shoot. A board that only ever covers the middle
  third constrains the lens model poorly no matter how clean its corners are.

[Printing them](printable.md) covers the mechanics — page size, DPI, and why
scale-to-fit is the single most common way to invalidate a calibration.
