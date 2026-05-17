cargo run --release -p calib-targets-bench --bin topo_stage_timing -- \
  --image-dir testdata/02-topo-grid \
  --out tools/out/topo-grid-performance/stage-breakdown.json \
  --repeats 50 \
  --warmup 5