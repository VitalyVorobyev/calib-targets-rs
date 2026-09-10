# How to choose a target

Five target families ship here, and the differences that matter are not
subtle. This page is the decision, not the tour — each row links to the
reference page for the family.

## The short version

**Calibrating one camera, board fully visible, no other constraints?** Use a
[chessboard](chessboard.md). It is the simplest thing that works and there is
nothing to configure.

**Anything else?** Read the table.

## The table

| Family | Corner IDs | Partial views | Needs | Reference |
|---|---|---|---|---|
| [Chessboard](chessboard.md) | none | ✗ whole board | nothing | [pipeline](pipeline_chessboard.md) |
| [ChArUco](charuco.md) | absolute | ✓ | an ArUco dictionary | [pipeline](pipeline_charuco.md) |
| [PuzzleBoard](puzzleboard.md) | absolute | ✓ down to a small fragment | nothing | [pipeline](pipeline_puzzleboard.md) |
| [Marker board](marker.md) | absolute | ✓ | a custom circle layout | [pipeline](pipeline_marker.md) |
| Regular grid | none | ✗ | nothing | [pipeline](pipeline_regular_grid.md) |

**Corner IDs** is the property that decides most cases. Without them a
detection is a set of corners in *some* order, so the board has to be fully
visible for its corners to mean anything. With them every corner carries an
absolute identity, so a partial view is still useful and the board's
orientation is never ambiguous.

## Choosing between the three identified families

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

**Marker board** is a plain checkerboard with three circular markers placed to
break its symmetry. It carries far less information than the other two — just
enough to fix the frame — but it prints as an ordinary chessboard with three
dots, which suits setups where the full pattern is impractical.

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
