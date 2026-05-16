// This module previously contained `cluster_orientations`,
// `compute_orientation_histogram`, and `estimate_grid_axes_from_orientations`
// which operated on `Corner`. All three have been removed: the only caller
// was `calib-targets-chessboard`, which carries its own self-contained
// histogram + 2-means implementation in `cluster.rs`. No remaining public
// surface referenced these functions, so the module is now empty.
