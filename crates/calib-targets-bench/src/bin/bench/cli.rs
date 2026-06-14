//! Command-line surface for the `bench` binary: the `clap` argument tree,
//! the value-enum knobs and their conversions into detector types, and the
//! small param-loading helpers shared by the subcommands.

use calib_targets::chessboard::{DetectorParams, GraphBuildAlgorithm, OrientationSource};
use calib_targets::detect::OrientationMethod;
use calib_targets_bench::dataset::ImageKind;
use calib_targets_bench::Engine;

use clap::{Args, Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "bench",
    about = "chessboard grid-builder regression / performance harness"
)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) cmd: Cmd,
}

#[derive(Subcommand)]
pub(crate) enum Cmd {
    /// Run the detector on every image and write a JSON report to bench_results/.
    Run(RunArgs),
    /// Run + diff against baselines. Exit non-zero on any regression.
    Check(RunArgs),
    /// Render PNG overlays under preview/ for visual inspection.
    Preview(PreviewArgs),
    /// Pin the current detector output as the new baseline.
    Bless(BlessArgs),
    /// Print per-stage corner counts and render a per-stage diagnostic
    /// overlay. Use this to investigate "why is this corner missing?"
    /// before changing detector code.
    Diagnose(DiagnoseArgs),
    /// Join two existing `run` JSON reports into a per-family comparison
    /// table (markdown + JSON). Re-runs nothing; reads + writes bench_results/.
    Compare(CompareArgs),
    /// Per-knob `AdvancedTuning` ablation: toggle each tuning knob one at a
    /// time over the dataset and emit a recall/precision/speed delta table
    /// (markdown + JSON) to bench_results/. Local-only output.
    Ablate(AblateArgs),
}

#[derive(Args)]
pub(crate) struct AblateArgs {
    /// Restrict to one kind of image set.
    #[arg(long, value_enum)]
    pub(crate) dataset: Option<DatasetKindArg>,
    /// Restrict to a single image (relative path under workspace root).
    #[arg(long)]
    pub(crate) image: Option<String>,
    /// Restrict to one `datasets.toml` group (the parent-directory name, e.g.
    /// `130x130_puzzle`). The key tractability lever for the private campaign.
    #[arg(long)]
    pub(crate) group: Option<String>,
    /// Graph-build algorithm of the baseline (and every variation). Default
    /// matches the production `GraphBuildAlgorithm` default; `topological.*`
    /// knob rows are only meaningful here.
    #[arg(long, value_enum, default_value_t = AlgorithmArg::Topological)]
    pub(crate) algorithm: AlgorithmArg,
    /// Detection engine. `pipeline` runs the full chessboard detector.
    #[arg(long, value_enum, default_value_t = EngineArg::Pipeline)]
    pub(crate) engine: EngineArg,
    /// Where the grid builder gets per-corner grid directions.
    #[arg(long, value_enum, default_value_t = OrientationSourceArg::ChessAxes)]
    pub(crate) orientation_source: OrientationSourceArg,
    /// Override chess-corners' axis-fit method.
    #[arg(long, value_enum, default_value_t = OrientationMethodArg::RingFit)]
    pub(crate) orientation_method: OrientationMethodArg,
    /// Optional JSON file with a partial `DetectorParams` that seeds the
    /// baseline (the ablation perturbs each knob relative to this base).
    #[arg(long)]
    pub(crate) chessboard_config: Option<String>,
    /// Output stem (workspace-relative unless absolute). `.md` and `.json`
    /// are written alongside it. Default `bench_results/ablation`.
    #[arg(long)]
    pub(crate) out: Option<String>,
    /// Scalar perturbation fraction: each scalar knob is run at `× (1 ± rel)`.
    #[arg(long, default_value_t = 0.25)]
    pub(crate) rel: f64,
    /// Restrict the ablation to these knob names (comma-separated or repeated).
    /// Overrides `--bool-only` / `--scalars-only`.
    #[arg(long, value_delimiter = ',')]
    pub(crate) only: Vec<String>,
    /// Ablate only the boolean (`enable_*`) flags.
    #[arg(long)]
    pub(crate) bool_only: bool,
    /// Ablate only the scalar knobs.
    #[arg(long)]
    pub(crate) scalars_only: bool,
    /// Also write each variation's full run report under `<out>_runs/`.
    #[arg(long)]
    pub(crate) dump_runs: bool,
}

#[derive(Args)]
pub(crate) struct CompareArgs {
    /// First report JSON (the "A" column, e.g. the topological run). Resolved
    /// as given, then relative to `bench_results/`.
    #[arg(long)]
    pub(crate) a: String,
    /// Second report JSON (the "B" column, e.g. the seed-and-grow run).
    #[arg(long)]
    pub(crate) b: String,
    /// Output stem (relative to workspace root unless absolute). Defaults to
    /// `bench_results/compare.<a_alg>_vs_<b_alg>`. `.md` and `.json` are
    /// written alongside it.
    #[arg(long)]
    pub(crate) out: Option<String>,
}

#[derive(Args)]
pub(crate) struct DiagnoseArgs {
    /// Image path (relative to workspace root). Stitched composites accept
    /// a `#k` suffix to pick one sub-snap, e.g.
    /// `privatedata/<dataset>/target_15.png#3`.
    pub(crate) image: String,
    /// Output overlay path (default: `preview/diagnose/<stem>.png`).
    #[arg(long)]
    pub(crate) out: Option<String>,
    /// Optional path to dump the full `DebugFrame` (cluster histogram +
    /// per-corner stages + iteration traces) as JSON for offline triage.
    /// Local-only output; do not commit.
    #[arg(long)]
    pub(crate) dump_frame: Option<String>,
    /// Which graph-build algorithm to diagnose. `seed-and-grow` produces a
    /// full `DebugFrame`; `topological` runs the production topological
    /// detector and renders an overlay of which corners ended up labelled.
    /// Default matches the production `GraphBuildAlgorithm` default.
    #[arg(long, value_enum, default_value_t = AlgorithmArg::Topological)]
    pub(crate) algorithm: AlgorithmArg,
    /// Where the topological builder gets per-corner grid directions.
    /// `neighbour-edges` synthesizes them from neighbour geometry (topological
    /// only). Has no effect on the `seed-and-grow` diagnose path.
    #[arg(long, value_enum, default_value_t = OrientationSourceArg::ChessAxes)]
    pub(crate) orientation_source: OrientationSourceArg,
    /// Override the topological pipeline's `axis_align_tol_rad` (in degrees).
    /// Larger values accept more edges as "grid" (potentially raising recall
    /// in distorted regions at the cost of precision).
    #[arg(long)]
    pub(crate) axis_align_tol_deg: Option<f32>,
    /// Optional JSON file with a serialised `DetectorParams` to override
    /// seed-and-grow defaults. Use for parameter sweeps without
    /// recompilation. Unspecified fields fall back to defaults via the
    /// `DetectorParams` `#[serde(default = ...)]` attributes.
    #[arg(long)]
    pub(crate) chessboard_config: Option<String>,
    /// Draw each non-Raw corner's two axis directions as short line
    /// segments (`axes[0]` in warm orange, `axes[1]` in cool teal). Useful
    /// for inspecting RingFit vs DiskFit axis disagreements visually:
    /// labelled corners' axes should alternate cleanly with cardinal
    /// neighbours, and stuck-at-Clustered corners reveal whether the
    /// axis estimate looks off.
    #[arg(long)]
    pub(crate) draw_axes: bool,
    /// Override chess-corners' axis-fit method. Default `ring-fit` matches
    /// upstream behaviour; `disk-fit` opts into the more accurate (slower)
    /// disk-sector fit added in chess-corners 0.9.
    #[arg(long, value_enum, default_value_t = OrientationMethodArg::RingFit)]
    pub(crate) orientation_method: OrientationMethodArg,
}

#[derive(Args)]
pub(crate) struct RunArgs {
    /// Restrict to one kind of image set.
    #[arg(long, value_enum)]
    pub(crate) dataset: Option<DatasetKindArg>,
    /// Restrict to a single image (relative path under workspace root).
    #[arg(long)]
    pub(crate) image: Option<String>,
    /// Which graph-build algorithm to exercise. Default matches the
    /// production `GraphBuildAlgorithm` default, which is also the cell
    /// `bless` pins baselines from.
    #[arg(long, value_enum, default_value_t = AlgorithmArg::Topological)]
    pub(crate) algorithm: AlgorithmArg,
    /// Detection engine. `pipeline` runs the full chessboard detector;
    /// `grid` drives the projective-grid grid builder directly (the
    /// orientation-source head-to-head, bypassing chessboard recovery).
    /// The slug is part of the report filename so cells coexist.
    #[arg(long, value_enum, default_value_t = EngineArg::Pipeline)]
    pub(crate) engine: EngineArg,
    /// Where the grid builder gets per-corner grid directions.
    /// `neighbour-edges` synthesizes them from neighbour geometry
    /// (topological / grid engine only); `chess-axes` uses the ChESS
    /// estimates. The slug is part of the report filename.
    #[arg(long, value_enum, default_value_t = OrientationSourceArg::ChessAxes)]
    pub(crate) orientation_source: OrientationSourceArg,
    /// Optional JSON file with a serialised partial `DetectorParams` that
    /// overrides seed-and-grow defaults. Same semantics as the diagnose
    /// subcommand's `--chessboard-config` flag.
    #[arg(long)]
    pub(crate) chessboard_config: Option<String>,
    /// Override chess-corners' axis-fit method. Default `ring-fit` matches
    /// upstream behaviour; `disk-fit` opts into the more accurate (slower)
    /// disk-sector fit added in chess-corners 0.9.
    #[arg(long, value_enum, default_value_t = OrientationMethodArg::RingFit)]
    pub(crate) orientation_method: OrientationMethodArg,
}

#[derive(Args)]
pub(crate) struct PreviewArgs {
    /// Output directory (relative to workspace root).
    #[arg(long, default_value = "preview")]
    pub(crate) out: String,
    /// Restrict to one kind of image set.
    #[arg(long, value_enum)]
    pub(crate) dataset: Option<DatasetKindArg>,
    /// Restrict to a single image.
    #[arg(long)]
    pub(crate) image: Option<String>,
    /// Render every image, even when the dataset filter / image filter would skip.
    #[arg(long)]
    pub(crate) all: bool,
    /// Which graph-build algorithm to exercise. Output filenames carry the
    /// algorithm name as a suffix so two runs can coexist in the same `--out`
    /// directory. Default matches the production `GraphBuildAlgorithm` default.
    #[arg(long, value_enum, default_value_t = AlgorithmArg::Topological)]
    pub(crate) algorithm: AlgorithmArg,
    /// Detection engine (see `run --help`). The slug is part of the overlay
    /// filename so cells coexist in the same `--out` directory.
    #[arg(long, value_enum, default_value_t = EngineArg::Pipeline)]
    pub(crate) engine: EngineArg,
    /// Where the grid builder gets per-corner grid directions (see
    /// `run --help`). The slug is part of the overlay filename.
    #[arg(long, value_enum, default_value_t = OrientationSourceArg::ChessAxes)]
    pub(crate) orientation_source: OrientationSourceArg,
    /// Override chess-corners' axis-fit method. Default `ring-fit` matches
    /// upstream behaviour; `disk-fit` opts into the more accurate (slower)
    /// disk-sector fit added in chess-corners 0.9.
    #[arg(long, value_enum, default_value_t = OrientationMethodArg::RingFit)]
    pub(crate) orientation_method: OrientationMethodArg,
}

#[derive(Args)]
pub(crate) struct BlessArgs {
    /// Image to bless (relative path, e.g. `testdata/mid.png`). Pass --all to bless every entry instead.
    #[arg(long)]
    pub(crate) image: Option<String>,
    /// Bless every entry of the chosen kind.
    #[arg(long)]
    pub(crate) all: bool,
    /// Restrict --all to one kind of image set.
    #[arg(long, value_enum)]
    pub(crate) dataset: Option<DatasetKindArg>,
}

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub(crate) enum DatasetKindArg {
    Public,
    Private,
}

impl From<DatasetKindArg> for ImageKind {
    fn from(v: DatasetKindArg) -> Self {
        match v {
            DatasetKindArg::Public => ImageKind::Public,
            DatasetKindArg::Private => ImageKind::Private,
        }
    }
}

#[derive(Clone, Copy, Debug, clap::ValueEnum, PartialEq, Eq)]
pub(crate) enum AlgorithmArg {
    Topological,
    SeedAndGrow,
}

impl AlgorithmArg {
    pub(crate) fn slug(self) -> &'static str {
        match self {
            AlgorithmArg::Topological => "topological",
            AlgorithmArg::SeedAndGrow => "seed_and_grow",
        }
    }
}

impl From<AlgorithmArg> for GraphBuildAlgorithm {
    fn from(v: AlgorithmArg) -> Self {
        match v {
            AlgorithmArg::Topological => GraphBuildAlgorithm::Topological,
            AlgorithmArg::SeedAndGrow => GraphBuildAlgorithm::SeedAndGrow,
        }
    }
}

#[derive(Clone, Copy, Debug, clap::ValueEnum, PartialEq, Eq)]
pub(crate) enum EngineArg {
    /// Full chessboard production pipeline (`Detector::detect`).
    Pipeline,
    /// Raw projective-grid grid builder — the orientation-source head-to-head.
    Grid,
}

impl EngineArg {
    pub(crate) fn slug(self) -> &'static str {
        match self {
            EngineArg::Pipeline => "pipeline",
            EngineArg::Grid => "grid",
        }
    }
}

impl From<EngineArg> for Engine {
    fn from(v: EngineArg) -> Self {
        match v {
            EngineArg::Pipeline => Engine::Pipeline,
            EngineArg::Grid => Engine::Grid,
        }
    }
}

#[derive(Clone, Copy, Debug, clap::ValueEnum, PartialEq, Eq)]
pub(crate) enum OrientationSourceArg {
    /// Per-corner ChESS axis estimates (production default).
    ChessAxes,
    /// Synthesize the two grid directions from neighbour-edge geometry.
    /// Topological / grid engine only (the native seed-and-grow pipeline
    /// consumes ChESS axes directly and panics on this combination).
    NeighbourEdges,
}

impl OrientationSourceArg {
    pub(crate) fn slug(self) -> &'static str {
        match self {
            OrientationSourceArg::ChessAxes => "chess_axes",
            OrientationSourceArg::NeighbourEdges => "neighbour_edges",
        }
    }
}

impl From<OrientationSourceArg> for OrientationSource {
    fn from(v: OrientationSourceArg) -> Self {
        match v {
            OrientationSourceArg::ChessAxes => OrientationSource::ChessAxes,
            OrientationSourceArg::NeighbourEdges => OrientationSource::NeighbourEdges,
        }
    }
}

#[derive(Clone, Copy, Debug, clap::ValueEnum, PartialEq, Eq)]
pub(crate) enum OrientationMethodArg {
    RingFit,
    DiskFit,
}

impl OrientationMethodArg {
    pub(crate) fn slug(self) -> &'static str {
        match self {
            OrientationMethodArg::RingFit => "ring_fit",
            OrientationMethodArg::DiskFit => "disk_fit",
        }
    }
}

impl From<OrientationMethodArg> for OrientationMethod {
    fn from(v: OrientationMethodArg) -> Self {
        match v {
            OrientationMethodArg::RingFit => OrientationMethod::RingFit,
            OrientationMethodArg::DiskFit => OrientationMethod::DiskFit,
        }
    }
}

pub(crate) fn params_with(
    algorithm: AlgorithmArg,
    orientation_source: OrientationSourceArg,
) -> DetectorParams {
    let mut p = DetectorParams::default();
    p.graph_build_algorithm = algorithm.into();
    p.orientation_source = orientation_source.into();
    p
}

// Partial-config loading lives in the library so the studio server shares
// the exact `--chessboard-config` merge semantics.
pub(crate) use calib_targets_bench::config::load_chessboard_config;
