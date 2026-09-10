# Build and detect a PuzzlePole

A **PuzzlePole** is the [PuzzleBoard](puzzleboard.md) pattern wrapped round a
cylinder. It is the only target here that stays identifiable from a full 360° —
walk round it and it keeps telling you where you are — and the only one whose
detected corners carry a **3-D** object point rather than a point on a plane.

This tutorial goes from nothing to a set of 2D↔3D correspondences ready for a
PnP solver. You do not need a printer to finish it: step 4 renders a pole
synthetically, so you can see the whole loop before committing anything to
paper or tube.

## 1. Choose a pole

Two numbers describe one:

- **circumference**, in pieces around the cylinder — not free, see below;
- **axial extent**, in pieces along it — free, at 4 or more.

The circumference is constrained because the wrap has to be *seamless*: the
strip's last row of puzzle pieces must be the same row as its first, or the
join would create a local code that appears nowhere in the pattern and the
decoder would have to guess. Only seven circumferences do that on the shipped
code maps.

| Pieces around | Diameter at 20 mm pieces | Strips available |
|---|---|---|
| 12 | 76 mm | 6 |
| 18 | 115 mm | 3 |
| 24 | 153 mm | 3 |
| 30 | 191 mm | 9 |
| 36 | 229 mm | 5 |
| 42 | 267 mm | 6 |
| 48 | 306 mm | 3 |

The diameter is a *consequence* of the circumference and the piece size, never
an input: `diameter = pieces × piece_size / π`. If the tube is what you already
have, invert it with
[`PuzzlePolePeriod::piece_size_for_diameter`](https://docs.rs/calib-targets-puzzleboard/latest/calib_targets_puzzleboard/pole/periods/struct.PuzzlePolePeriod.html#method.piece_size_for_diameter)
— every period can wrap every diameter, and what changes is how many pixels a
code dot gets. [Print and wrap a PuzzlePole](howto_print_puzzlepole.md) covers
that choice properly.

This table is not transcribed from the paper. It is re-derived from the shipped
map bytes by `pole::periods`, and a test re-derives it on every run — which is
how we know the paper's period-36 entry does not close on these maps and 327
does.

## 2. How many different poles are there

More than you will use. Three independent choices multiply:

- **Seven circumferences** — seven physical diameters, at a given piece size.
- **35 seamless strips** in total, the "Strips available" column above: a
  period can start at several different master rows, and each start row is a
  different pattern, not a rotation of the same one.
- **Disjoint axial windows.** A pole is cut from a column window of the
  501-column master, and windows that do not overlap share no corner at all.
  A pole *n* pieces tall occupies `n + 1` columns, so one strip yields
  `⌊501 / (n + 1)⌋` poles whose corner ids cannot collide by construction.

That last count reproduces the paper's: 6 pieces tall gives 71 poles, 20 gives
23. Enumerate them rather than computing offsets by hand:

```rust
use calib_targets_puzzleboard::PuzzlePoleSpec;

# fn main() -> Result<(), Box<dyn std::error::Error>> {
let family = PuzzlePoleSpec::distinct_poles(24, 12, 20.0)?;
assert_eq!(family.len(), 38);   // 501 / 13 columns
# Ok(())
# }
```

One caveat worth stating plainly: each of those is a distinct *target*, and the
detector is configured for one pole at a time. Detecting several different
poles in a single frame means running the detector once per spec, and how well
a fragment of one pole is *rejected* by a detector configured for another has
not been measured here.

## 3. Generate the strip

```bash
calib-targets gen puzzlepole \
  --out-stem pole24 \
  --circumference-squares 24 \
  --axial-squares 12 \
  --square-size-mm 20 \
  --page-size custom --page-width-mm 280 --page-height-mm 560
```

That writes `pole24.{json,svg,png,dxf}`. The strip is 26 pieces tall, not 24:
the two extra are the overlap, trimmed through the *middle* of the end pieces
when you wrap, because a code dot sits on the joint. The JSON carries the
resolved corner points and their ids, so it is also the ground truth for
whatever you build next.

Printing, trimming and mounting are their own page — see
[Print and wrap a PuzzlePole](howto_print_puzzlepole.md). The rest of this
tutorial needs no physical pole.

## 4. See it before you print

The workspace can render a pole analytically: every pixel casts a ray at a
cylinder and the pattern is evaluated at the surface point it lands on. No
mesh, no texture, no resampling — so the picture is a statement about geometry,
and nothing in it is an artefact of the renderer.

```bash
cargo run -p calib-targets-puzzleboard --example render_puzzlepole -- out
# out/puzzlepole_view.png      what a camera sees
# out/puzzlepole_detected.png  the same frame, decoded
```

![A PuzzlePole rendered as a camera would see it](img/puzzlepole_view.png)

That is a 24-piece pole at 20 mm, seen from 18° above the horizontal. Note what
the curvature does: the rows around the circumference bow, the pieces compress
towards each edge, and the pattern runs off the silhouette rather than ending
at a border. None of that is a problem the planar detector had to solve.

Now the same frame with the decode drawn on it:

![The same frame with every identified corner marked](img/puzzlepole_detected.png)

Every dot is a corner the detector found *and identified*. The colour runs
round the circumference, so following the ramp shows the wrap closing on
itself; the cyan line is the **seam**, the join where the strip's two ends
meet. That it decodes straight through the seam is the whole point — a pole
that only worked away from its join would be a worse chessboard.

Pass an azimuth and elevation to look from anywhere:

```bash
cargo run -p calib-targets-puzzleboard --example render_puzzlepole -- out 90 25
```

## 5. Detect it

The facade takes an image and a spec and hands back identified corners:

```rust,no_run
use calib_targets::detect;
use calib_targets::puzzleboard::{PuzzlePoleParams, PuzzlePoleSpec};

# fn main() -> Result<(), Box<dyn std::error::Error>> {
let img = image::open("out/puzzlepole_view.png")?.to_luma8();

let spec = PuzzlePoleSpec::new(24, 12, 20.0)?;
let found = detect::detect_puzzlepole_best(&img, &PuzzlePoleParams::sweep_for_pole(&spec))?;

println!("{} corners identified", found.corners.len());
# Ok(())
# }
```

Each corner carries three positions, and the distinction matters:

| Field | Meaning |
|---|---|
| `position` | where it landed in the **image**, in pixels |
| `surface_position` | where it sits on the **unrolled strip**, in mm |
| `object_position` | where it sits in the pole's **3-D frame**, in mm |

`object_position` is the one PnP wants. `surface_position` is what
`LabeledCorner::target_position` carries, so a pole detection still fits every
carrier and every binding that expects a planar target.

## 6. Hand it to a PnP solver

This library does not solve PnP — it gives you the correspondences and stops
there, because the pose solver you already use is better than one bolted on
here.

```rust,no_run
# use calib_targets::detect;
# use calib_targets::puzzleboard::{PuzzlePoleParams, PuzzlePoleSpec};
# fn main() -> Result<(), Box<dyn std::error::Error>> {
# let img = image::open("out/puzzlepole_view.png")?.to_luma8();
# let spec = PuzzlePoleSpec::new(24, 12, 20.0)?;
# let found = detect::detect_puzzlepole_best(&img, &PuzzlePoleParams::sweep_for_pole(&spec))?;
for (image_point, object_point) in found.correspondences() {
    // image_point: Point2<f32> in pixels
    // object_point: Point3<f32> in millimetres on the cylinder
    let _ = (image_point, object_point);
}
# Ok(())
# }
```

Feed those to `cv2.solvePnP` with `flags=cv2.SOLVEPNP_ITERATIVE` — or to
`SOLVEPNP_SQPNP`, which does not need the points to be coplanar and is a
better fit for a cylinder — and you have a pose from a single view, at any
azimuth.

## 7. What there is to configure

Start with `PuzzlePoleParams::for_pole(spec)` and change nothing. If a view
fails, `sweep_for_pole` is the multi-config version used above; it crosses the
chessboard front-end sweep with a second, more permissive scoring pass, which
matters more on a pole than on a flat board because every dot near the limb is
compressed.

The one knob that is genuinely pole-specific is the **window floor**, and it is
a *pair* where a flat board has a single number:

| Knob | Default | What it demands |
|---|---|---|
| `decode.min_circumference_span` | 5 | corner rows visible **around** the pole |
| `decode.min_axial_span` | 8 | corner columns visible **along** it |

Both are measurements, not preferences. A `5 × 8` corner window is unambiguous
on every supported pole at any placement, and one corner less is not; the axial
floor cannot go below 5 for a structural reason — a 4-corner extent yields two
columns of `map_a`, and a 3×2 block of a sub-perfect map is never unique.
Lowering either admits placements that provably alias, which costs you a wrong
id — and a wrong id is unrecoverable downstream where a miss is merely
inconvenient. See `research/puzzleboard-rings/report/puzzlepole-floor.md` for
the sweep.

Everything else — corner strength, the ChESS front end, the grid builder — is
the planar PuzzleBoard's, documented under [Tune the detector](tuning.md).

## Related

- [Print and wrap a PuzzlePole](howto_print_puzzlepole.md) — the physical half.
- [PuzzleBoard edge-code decode](algo_puzzleboard_decode.md) — how the code is
  read, which is the same machinery with one period changed.
- [Choose a target](howto_choose_target.md) — when a pole is and is not the
  right answer.
