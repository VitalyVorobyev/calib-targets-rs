use calib_targets_aruco::builtins::{builtin_dictionary, BUILTIN_DICTIONARY_NAMES};
use calib_targets_print::{
    write_target_bundle, CharucoTargetSpec, ChessboardTargetSpec, MarkerBoardTargetSpec,
    MarkerCircleSpec, PageOrientation, PageSize, PageSpec, PrintableTargetDocument,
    PrintableTargetError, RenderOptions, TargetSpec,
};
use clap::{Args, Parser, Subcommand, ValueEnum};
use std::{fs, path::PathBuf, process::ExitCode, str::FromStr};

#[derive(Parser, Debug)]
#[command(name = "calib-targets")]
#[command(about = "Repo-local CLI for printable calibration target generation")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Render a validated printable spec into .json, .svg, and .png outputs.
    Generate {
        /// Path to the input printable spec JSON file.
        #[arg(long)]
        spec: PathBuf,
        /// Output path stem; the CLI writes `stem.json`, `stem.svg`, and `stem.png`.
        #[arg(long)]
        out_stem: PathBuf,
    },
    /// Validate a printable spec without writing any output files.
    Validate {
        /// Path to the printable spec JSON file to validate.
        #[arg(long)]
        spec: PathBuf,
    },
    /// List the built-in dictionary names available for ChArUco initialization.
    ListDictionaries,
    /// Initialize a printable spec JSON file for one target family.
    Init {
        #[command(subcommand)]
        target: InitCommand,
    },
}

#[derive(Subcommand, Debug)]
enum InitCommand {
    /// Initialize a chessboard printable spec.
    Chessboard(ChessboardInitArgs),
    /// Initialize a ChArUco printable spec.
    Charuco(CharucoInitArgs),
    /// Initialize a checkerboard marker-board printable spec.
    MarkerBoard(MarkerBoardInitArgs),
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum PageSizeArg {
    A4,
    Letter,
    Custom,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum OrientationArg {
    Portrait,
    Landscape,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum MarkerLayoutArg {
    OpencvCharuco,
}

#[derive(Args, Debug, Clone)]
struct PageArgs {
    /// Output page size preset.
    #[arg(long, value_enum, default_value_t = PageSizeArg::A4)]
    page_size: PageSizeArg,
    /// Custom page width in millimeters; requires --page-size custom.
    #[arg(long)]
    page_width_mm: Option<f64>,
    /// Custom page height in millimeters; requires --page-size custom.
    #[arg(long)]
    page_height_mm: Option<f64>,
    /// Page orientation.
    #[arg(long, value_enum, default_value_t = OrientationArg::Portrait)]
    orientation: OrientationArg,
    /// Margin on each side of the page in millimeters.
    #[arg(long, default_value_t = 10.0)]
    margin_mm: f64,
}

#[derive(Args, Debug, Clone)]
struct RenderArgs {
    /// Add guide overlays to the rendered outputs.
    #[arg(long, default_value_t = false)]
    debug_annotations: bool,
    /// Raster DPI for the generated PNG output.
    #[arg(long, default_value_t = 300)]
    png_dpi: u32,
}

#[derive(Args, Debug)]
struct ChessboardInitArgs {
    /// Path to the spec JSON file to write.
    #[arg(long)]
    out: PathBuf,
    /// Number of inner chessboard corners vertically.
    #[arg(long)]
    inner_rows: u32,
    /// Number of inner chessboard corners horizontally.
    #[arg(long)]
    inner_cols: u32,
    /// Square side length in millimeters.
    #[arg(long)]
    square_size_mm: f64,
    #[command(flatten)]
    page: PageArgs,
    #[command(flatten)]
    render: RenderArgs,
}

#[derive(Args, Debug)]
struct CharucoInitArgs {
    /// Path to the spec JSON file to write.
    #[arg(long)]
    out: PathBuf,
    /// Number of board squares vertically.
    #[arg(long)]
    rows: u32,
    /// Number of board squares horizontally.
    #[arg(long)]
    cols: u32,
    /// Square side length in millimeters.
    #[arg(long)]
    square_size_mm: f64,
    /// Marker side length relative to square size.
    #[arg(long)]
    marker_size_rel: f64,
    /// Built-in dictionary name. Use `list-dictionaries` to discover valid values.
    #[arg(long)]
    dictionary: String,
    /// Marker placement scheme.
    #[arg(long, value_enum, default_value_t = MarkerLayoutArg::OpencvCharuco)]
    marker_layout: MarkerLayoutArg,
    /// Marker border width in bit cells.
    #[arg(long, default_value_t = 1)]
    border_bits: usize,
    #[command(flatten)]
    page: PageArgs,
    #[command(flatten)]
    render: RenderArgs,
}

#[derive(Args, Debug)]
struct MarkerBoardInitArgs {
    /// Path to the spec JSON file to write.
    #[arg(long)]
    out: PathBuf,
    /// Number of inner chessboard corners vertically.
    #[arg(long)]
    inner_rows: u32,
    /// Number of inner chessboard corners horizontally.
    #[arg(long)]
    inner_cols: u32,
    /// Square side length in millimeters.
    #[arg(long)]
    square_size_mm: f64,
    /// Circle diameter relative to square size.
    #[arg(long, default_value_t = 0.5)]
    circle_diameter_rel: f64,
    /// Marker circles as `i,j,polarity`; repeat exactly three times when overriding defaults.
    #[arg(long = "circle", value_name = "I,J,POLARITY")]
    circles: Vec<String>,
    #[command(flatten)]
    page: PageArgs,
    #[command(flatten)]
    render: RenderArgs,
}

#[derive(thiserror::Error, Debug)]
enum CliError {
    #[error(transparent)]
    Printable(#[from] PrintableTargetError),
    #[error("invalid page configuration: {0}")]
    InvalidPage(String),
    #[error("unknown dictionary {0}; run `list-dictionaries` to inspect built-ins")]
    UnknownDictionary(String),
    #[error("invalid --circle '{0}', expected i,j,polarity")]
    InvalidCircle(String),
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("{err}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    match cli.command {
        Command::Generate { spec, out_stem } => {
            let doc = PrintableTargetDocument::load_json(&spec)?;
            let written = write_target_bundle(&doc, out_stem)?;
            println!("{}", written.json_path.display());
            println!("{}", written.svg_path.display());
            println!("{}", written.png_path.display());
        }
        Command::Validate { spec } => {
            let doc = PrintableTargetDocument::load_json(&spec)?;
            println!("valid {}", doc.target.kind_name());
        }
        Command::ListDictionaries => {
            for name in BUILTIN_DICTIONARY_NAMES {
                println!("{name}");
            }
        }
        Command::Init { target } => match target {
            InitCommand::Chessboard(args) => {
                let doc = PrintableTargetDocument {
                    schema_version: 1,
                    target: TargetSpec::Chessboard(ChessboardTargetSpec {
                        inner_rows: args.inner_rows,
                        inner_cols: args.inner_cols,
                        square_size_mm: args.square_size_mm,
                    }),
                    page: build_page_spec(&args.page)?,
                    render: build_render_options(&args.render),
                };
                write_document_json(&doc, args.out)?;
            }
            InitCommand::Charuco(args) => {
                let dictionary = builtin_dictionary(&args.dictionary)
                    .ok_or_else(|| CliError::UnknownDictionary(args.dictionary.clone()))?;
                let marker_layout = match args.marker_layout {
                    MarkerLayoutArg::OpencvCharuco => {
                        calib_targets_charuco::MarkerLayout::OpenCvCharuco
                    }
                };
                let doc = PrintableTargetDocument {
                    schema_version: 1,
                    target: TargetSpec::Charuco(CharucoTargetSpec {
                        rows: args.rows,
                        cols: args.cols,
                        square_size_mm: args.square_size_mm,
                        marker_size_rel: args.marker_size_rel,
                        dictionary,
                        marker_layout,
                        border_bits: args.border_bits,
                    }),
                    page: build_page_spec(&args.page)?,
                    render: build_render_options(&args.render),
                };
                write_document_json(&doc, args.out)?;
            }
            InitCommand::MarkerBoard(args) => {
                let circles = if args.circles.is_empty() {
                    MarkerBoardTargetSpec::default_circles(args.inner_rows, args.inner_cols)
                } else {
                    parse_circles(&args.circles)?
                };
                let doc = PrintableTargetDocument {
                    schema_version: 1,
                    target: TargetSpec::MarkerBoard(MarkerBoardTargetSpec {
                        inner_rows: args.inner_rows,
                        inner_cols: args.inner_cols,
                        square_size_mm: args.square_size_mm,
                        circles,
                        circle_diameter_rel: args.circle_diameter_rel,
                    }),
                    page: build_page_spec(&args.page)?,
                    render: build_render_options(&args.render),
                };
                write_document_json(&doc, args.out)?;
            }
        },
    }
    Ok(())
}

fn write_document_json(
    doc: &PrintableTargetDocument,
    out: PathBuf,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent)?;
    }
    doc.write_json(&out)?;
    println!("{}", out.display());
    Ok(())
}

fn build_page_spec(args: &PageArgs) -> Result<PageSpec, CliError> {
    let size = match args.page_size {
        PageSizeArg::A4 => PageSize::A4,
        PageSizeArg::Letter => PageSize::Letter,
        PageSizeArg::Custom => PageSize::Custom {
            width_mm: args
                .page_width_mm
                .ok_or_else(|| CliError::InvalidPage("missing --page-width-mm".to_string()))?,
            height_mm: args
                .page_height_mm
                .ok_or_else(|| CliError::InvalidPage("missing --page-height-mm".to_string()))?,
        },
    };
    if !matches!(args.page_size, PageSizeArg::Custom)
        && (args.page_width_mm.is_some() || args.page_height_mm.is_some())
    {
        return Err(CliError::InvalidPage(
            "--page-width-mm/--page-height-mm require --page-size custom".to_string(),
        ));
    }
    Ok(PageSpec {
        size,
        orientation: match args.orientation {
            OrientationArg::Portrait => PageOrientation::Portrait,
            OrientationArg::Landscape => PageOrientation::Landscape,
        },
        margin_mm: args.margin_mm,
    })
}

fn build_render_options(args: &RenderArgs) -> RenderOptions {
    RenderOptions {
        debug_annotations: args.debug_annotations,
        png_dpi: args.png_dpi,
    }
}

fn parse_circles(values: &[String]) -> Result<[MarkerCircleSpec; 3], CliError> {
    if values.len() != 3 {
        return Err(CliError::InvalidCircle(
            "expected exactly three --circle values".to_string(),
        ));
    }
    let mut parsed = Vec::with_capacity(3);
    for value in values {
        parsed.push(parse_circle(value)?);
    }
    Ok([parsed[0], parsed[1], parsed[2]])
}

fn parse_circle(value: &str) -> Result<MarkerCircleSpec, CliError> {
    let parts: Vec<_> = value.split(',').map(str::trim).collect();
    if parts.len() != 3 {
        return Err(CliError::InvalidCircle(value.to_string()));
    }
    let i = u32::from_str(parts[0]).map_err(|_| CliError::InvalidCircle(value.to_string()))?;
    let j = u32::from_str(parts[1]).map_err(|_| CliError::InvalidCircle(value.to_string()))?;
    let polarity = match parts[2] {
        "white" => calib_targets_marker::CirclePolarity::White,
        "black" => calib_targets_marker::CirclePolarity::Black,
        _ => return Err(CliError::InvalidCircle(value.to_string())),
    };
    Ok(MarkerCircleSpec { i, j, polarity })
}
