cargo run --release -p calib-targets-charuco --example charuco_investigate -- \
  single --image testdata/3536119669/target_0.png --strip 0 \
  --out-dir tmpdata/manual_inspect_overlay_check --min-marker-inliers 3

.venv/bin/python tools/plot_charuco_overlay.py \
  tmpdata/manual_inspect_overlay_check/strip_0/report.json --show
