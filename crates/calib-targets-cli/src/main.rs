use calib_targets_print::{
    write_target_bundle, CharucoTargetSpec, ChessboardTargetSpec, MarkerBoardTargetSpec,
    MarkerCircleSpec, PageOrientation, PageSize, PageSpec, PrintableTargetDocument,
    PrintableTargetError, RenderOptions, TargetSpec,
};
use clap::{Args, Parser, Subcommand, ValueEnum};
use std::{fs, path::PathBuf, process::ExitCode, str::FromStr};

#[derive(Parser, Debug)]
#[command(name = "calib-targets")]
#[command(about = "Generate printable calibration targets")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    Generate {
        #[arg(long)]
        spec: PathBuf,
        #[arg(long)]
        out_stem: PathBuf,
    },
    Init {
        #[command(subcommand)]
        target: InitCommand,
    },
}

#[derive(Subcommand, Debug)]
enum InitCommand {
    Chessboard(ChessboardInitArgs),
    Charuco(CharucoInitArgs),
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
    #[arg(long, value_enum, default_value_t = PageSizeArg::A4)]
    page_size: PageSizeArg,
    #[arg(long)]
    page_width_mm: Option<f64>,
    #[arg(long)]
    page_height_mm: Option<f64>,
    #[arg(long, value_enum, default_value_t = OrientationArg::Portrait)]
    orientation: OrientationArg,
    #[arg(long, default_value_t = 10.0)]
    margin_mm: f64,
}

#[derive(Args, Debug, Clone)]
struct RenderArgs {
    #[arg(long, default_value_t = false)]
    debug_annotations: bool,
    #[arg(long, default_value_t = 300)]
    png_dpi: u32,
}

#[derive(Args, Debug)]
struct ChessboardInitArgs {
    #[arg(long)]
    out: PathBuf,
    #[arg(long)]
    inner_rows: u32,
    #[arg(long)]
    inner_cols: u32,
    #[arg(long)]
    square_size_mm: f64,
    #[command(flatten)]
    page: PageArgs,
    #[command(flatten)]
    render: RenderArgs,
}

#[derive(Args, Debug)]
struct CharucoInitArgs {
    #[arg(long)]
    out: PathBuf,
    #[arg(long)]
    rows: u32,
    #[arg(long)]
    cols: u32,
    #[arg(long)]
    square_size_mm: f64,
    #[arg(long)]
    marker_size_rel: f64,
    #[arg(long)]
    dictionary: String,
    #[arg(long, value_enum, default_value_t = MarkerLayoutArg::OpencvCharuco)]
    marker_layout: MarkerLayoutArg,
    #[arg(long, default_value_t = 1)]
    border_bits: usize,
    #[command(flatten)]
    page: PageArgs,
    #[command(flatten)]
    render: RenderArgs,
}

#[derive(Args, Debug)]
struct MarkerBoardInitArgs {
    #[arg(long)]
    out: PathBuf,
    #[arg(long)]
    inner_rows: u32,
    #[arg(long)]
    inner_cols: u32,
    #[arg(long)]
    square_size_mm: f64,
    #[arg(long, default_value_t = 0.5)]
    circle_diameter_rel: f64,
    #[arg(long = "circle")]
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
    #[error("unknown dictionary {0}")]
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
                let dictionary =
                    calib_targets_aruco::builtins::builtin_dictionary(&args.dictionary)
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
