# Rectification

`calib-targets-chessboard` ships two ways to turn a detected board into a
fronto-parallel image — useful for inspecting a detection or for sampling
cells (markers, dots) in board space:

| Function | Model | Input |
|---|---|---|
| `rectify_from_chessboard_result` → `RectifiedBoardView` | One global homography fitted to all labelled corners | `&ChessboardDetection` |
| `rectify_mesh_from_grid` → `RectifiedMeshView` | One homography per grid cell; tolerates lens distortion | `&[LabeledCorner]` + trusted indices |

![Global rectification](img/rectified_small.png)
*Global homography rectification.*

![Mesh rectification](img/mesh_rectified_small.png)
*Per-cell mesh rectification of the same frame.*

```rust,no_run
use calib_targets::chessboard::{
    rectify_from_chessboard_result, rectify_mesh_from_grid, ChessboardParams,
};
use calib_targets::detect::{default_chess_config, detect_chessboard, gray_view};
use calib_targets::LabeledCorner;

# fn main() -> Result<(), Box<dyn std::error::Error>> {
let img = image::open("testdata/mid.png")?.to_luma8();
let det = detect_chessboard(&img, &default_chess_config(), &ChessboardParams::default())?;
let src = gray_view(&img);

// Global: 60 px per square, half a square of margin around the corner grid.
let global = rectify_from_chessboard_result(&src, &det, 60.0, 0.5)?;
println!("rectified {}x{}", global.rect.width, global.rect.height);

// Mesh: pattern-agnostic, takes any consistently labelled corner set.
let corners: Vec<LabeledCorner> = det
    .corners
    .iter()
    .map(|c| LabeledCorner::new(c.position, c.score).with_grid(c.grid))
    .collect();
let inliers: Vec<usize> = (0..corners.len()).collect();
let mesh = rectify_mesh_from_grid(&src, &corners, &inliers, 60.0)?;
# let _ = mesh;
# Ok(())
# }
```

`RectifiedBoardView` also carries both homographies (`h_img_from_rect`,
`h_rect_from_img`) and the covered grid range; `RectifiedMeshView` maps
rectified points back with `rect_to_img` and `cell_corners_img`. Use the mesh
variant whenever the lens is visibly distorted — a single homography
cannot absorb radial distortion.
