use ::calib_targets::{aruco, charuco, chessboard, core, detect, marker};
use chess_corners::{ChessConfig, ChessParams, CoarseToFineParams, PyramidParams};
use numpy::{PyArrayDyn, PyArrayMethods, PyUntypedArrayMethods};
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyDict, PyList, PyString, PyTuple};
use pyo3::PyRef;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{Map, Number, Value};

#[pyclass(name = "ChessCornerParams", module = "calib_targets")]
#[derive(Clone, Debug)]
/// Parameters for the ChESS corner detector.
struct PyChessCornerParams {
    inner: ChessParams,
}

#[pymethods]
impl PyChessCornerParams {
    #[new]
    #[pyo3(signature = (*, use_radius10=None, descriptor_use_radius10=None, threshold_rel=None, threshold_abs=None, nms_radius=None, min_cluster_size=None))]
    fn new(
        use_radius10: Option<bool>,
        descriptor_use_radius10: Option<bool>,
        threshold_rel: Option<f32>,
        threshold_abs: Option<f32>,
        nms_radius: Option<u32>,
        min_cluster_size: Option<u32>,
    ) -> PyResult<Self> {
        let mut params = ChessParams::default();
        if let Some(use_radius10) = use_radius10 {
            params.use_radius10 = use_radius10;
        }
        if let Some(descriptor_use_radius10) = descriptor_use_radius10 {
            params.descriptor_use_radius10 = Some(descriptor_use_radius10);
        }
        if let Some(threshold_rel) = threshold_rel {
            params.threshold_rel = threshold_rel;
        }
        if let Some(threshold_abs) = threshold_abs {
            params.threshold_abs = Some(threshold_abs);
        }
        if let Some(nms_radius) = nms_radius {
            params.nms_radius = nms_radius;
        }
        if let Some(min_cluster_size) = min_cluster_size {
            params.min_cluster_size = min_cluster_size;
        }
        Ok(Self { inner: params })
    }
}

#[pyclass(name = "PyramidParams", module = "calib_targets")]
#[derive(Clone, Debug)]
/// Parameters for image pyramid generation.
struct PyPyramidParams {
    inner: PyramidParams,
}

#[pymethods]
impl PyPyramidParams {
    #[new]
    #[pyo3(signature = (*, num_levels=None, min_size=None))]
    fn new(num_levels: Option<u8>, min_size: Option<usize>) -> PyResult<Self> {
        let mut params = PyramidParams::default();
        if let Some(num_levels) = num_levels {
            params.num_levels = num_levels;
        }
        if let Some(min_size) = min_size {
            params.min_size = min_size;
        }
        Ok(Self { inner: params })
    }
}

#[pyclass(name = "CoarseToFineParams", module = "calib_targets")]
#[derive(Clone, Debug)]
/// Coarse-to-fine multiscale detector parameters.
struct PyCoarseToFineParams {
    inner: CoarseToFineParams,
}

#[pymethods]
impl PyCoarseToFineParams {
    #[new]
    #[pyo3(signature = (*, pyramid=None, refinement_radius=None, merge_radius=None))]
    fn new(
        pyramid: Option<&Bound<'_, PyAny>>,
        refinement_radius: Option<u32>,
        merge_radius: Option<f32>,
    ) -> PyResult<Self> {
        let mut params = CoarseToFineParams::default();
        if let Some(pyramid) = pyramid {
            params.pyramid = pyramid_params_from_obj(pyramid, "pyramid", params.pyramid.clone())?;
        }
        if let Some(refinement_radius) = refinement_radius {
            params.refinement_radius = refinement_radius;
        }
        if let Some(merge_radius) = merge_radius {
            params.merge_radius = merge_radius;
        }
        Ok(Self { inner: params })
    }
}

#[pyclass(name = "ChessConfig", module = "calib_targets")]
#[derive(Clone, Debug)]
/// ChESS detector configuration (corner params + multiscale tuning).
struct PyChessConfig {
    inner: ChessConfig,
}

#[pymethods]
impl PyChessConfig {
    #[new]
    #[pyo3(signature = (*, params=None, multiscale=None))]
    fn new(
        params: Option<&Bound<'_, PyAny>>,
        multiscale: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        let mut cfg = detect::default_chess_config();
        if let Some(params) = params {
            cfg.params = chess_params_from_obj(params, "params", cfg.params.clone())?;
        }
        if let Some(multiscale) = multiscale {
            cfg.multiscale =
                coarse_to_fine_params_from_obj(multiscale, "multiscale", cfg.multiscale.clone())?;
        }
        Ok(Self { inner: cfg })
    }

    /// Return a JSON-like dict for debugging.
    fn to_dict(&self, py: Python<'_>) -> PyResult<PyObject> {
        let json = chess_config_to_json(&self.inner);
        json_to_py(py, &json)
    }
}

#[pyclass(name = "OrientationClusteringParams", module = "calib_targets")]
#[derive(Clone, Debug)]
/// Orientation clustering parameters for chessboard detection.
struct PyOrientationClusteringParams {
    inner: core::OrientationClusteringParams,
}

#[pymethods]
impl PyOrientationClusteringParams {
    #[new]
    #[pyo3(signature = (*, num_bins=None, max_iters=None, peak_min_separation_deg=None, outlier_threshold_deg=None, min_peak_weight_fraction=None, use_weights=None))]
    fn new(
        num_bins: Option<usize>,
        max_iters: Option<usize>,
        peak_min_separation_deg: Option<f32>,
        outlier_threshold_deg: Option<f32>,
        min_peak_weight_fraction: Option<f32>,
        use_weights: Option<bool>,
    ) -> PyResult<Self> {
        let mut params = core::OrientationClusteringParams::default();
        if let Some(num_bins) = num_bins {
            params.num_bins = num_bins;
        }
        if let Some(max_iters) = max_iters {
            params.max_iters = max_iters;
        }
        if let Some(peak_min_separation_deg) = peak_min_separation_deg {
            params.peak_min_separation_deg = peak_min_separation_deg;
        }
        if let Some(outlier_threshold_deg) = outlier_threshold_deg {
            params.outlier_threshold_deg = outlier_threshold_deg;
        }
        if let Some(min_peak_weight_fraction) = min_peak_weight_fraction {
            params.min_peak_weight_fraction = min_peak_weight_fraction;
        }
        if let Some(use_weights) = use_weights {
            params.use_weights = use_weights;
        }
        Ok(Self { inner: params })
    }
}

#[pyclass(name = "GridGraphParams", module = "calib_targets")]
#[derive(Clone, Debug)]
/// Parameters for grid graph construction in chessboard detection.
struct PyGridGraphParams {
    inner: chessboard::GridGraphParams,
}

#[pymethods]
impl PyGridGraphParams {
    #[new]
    #[pyo3(signature = (*, min_spacing_pix=None, max_spacing_pix=None, k_neighbors=None, orientation_tolerance_deg=None))]
    fn new(
        min_spacing_pix: Option<f32>,
        max_spacing_pix: Option<f32>,
        k_neighbors: Option<usize>,
        orientation_tolerance_deg: Option<f32>,
    ) -> PyResult<Self> {
        let mut params = chessboard::GridGraphParams::default();
        if let Some(min_spacing_pix) = min_spacing_pix {
            params.min_spacing_pix = min_spacing_pix;
        }
        if let Some(max_spacing_pix) = max_spacing_pix {
            params.max_spacing_pix = max_spacing_pix;
        }
        if let Some(k_neighbors) = k_neighbors {
            params.k_neighbors = k_neighbors;
        }
        if let Some(orientation_tolerance_deg) = orientation_tolerance_deg {
            params.orientation_tolerance_deg = orientation_tolerance_deg;
        }
        Ok(Self { inner: params })
    }
}

#[pyclass(name = "ChessboardParams", module = "calib_targets")]
#[derive(Clone, Debug)]
/// Parameters for chessboard detection from ChESS corners.
struct PyChessboardParams {
    inner: chessboard::ChessboardParams,
}

#[pymethods]
impl PyChessboardParams {
    #[new]
    #[pyo3(signature = (*, min_corner_strength=None, min_corners=None, expected_rows=None, expected_cols=None, completeness_threshold=None, use_orientation_clustering=None, orientation_clustering_params=None))]
    fn new(
        min_corner_strength: Option<f32>,
        min_corners: Option<usize>,
        expected_rows: Option<u32>,
        expected_cols: Option<u32>,
        completeness_threshold: Option<f32>,
        use_orientation_clustering: Option<bool>,
        orientation_clustering_params: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        let mut params = chessboard::ChessboardParams::default();
        if let Some(min_corner_strength) = min_corner_strength {
            params.min_corner_strength = min_corner_strength;
        }
        if let Some(min_corners) = min_corners {
            params.min_corners = min_corners;
        }
        if let Some(expected_rows) = expected_rows {
            params.expected_rows = Some(expected_rows);
        }
        if let Some(expected_cols) = expected_cols {
            params.expected_cols = Some(expected_cols);
        }
        if let Some(completeness_threshold) = completeness_threshold {
            params.completeness_threshold = completeness_threshold;
        }
        if let Some(use_orientation_clustering) = use_orientation_clustering {
            params.use_orientation_clustering = use_orientation_clustering;
        }
        if let Some(orientation_clustering_params) = orientation_clustering_params {
            params.orientation_clustering_params = orientation_clustering_params_from_obj(
                orientation_clustering_params,
                "orientation_clustering_params",
                params.orientation_clustering_params.clone(),
            )?;
        }
        Ok(Self { inner: params })
    }
}

#[pyclass(name = "ScanDecodeConfig", module = "calib_targets")]
#[derive(Clone, Debug)]
/// Marker scan/decoder configuration.
struct PyScanDecodeConfig {
    inner: aruco::ScanDecodeConfig,
}

#[pymethods]
impl PyScanDecodeConfig {
    #[new]
    #[pyo3(signature = (*, border_bits=None, inset_frac=None, marker_size_rel=None, min_border_score=None, dedup_by_id=None))]
    fn new(
        border_bits: Option<usize>,
        inset_frac: Option<f32>,
        marker_size_rel: Option<f32>,
        min_border_score: Option<f32>,
        dedup_by_id: Option<bool>,
    ) -> PyResult<Self> {
        let mut params = aruco::ScanDecodeConfig::default();
        if let Some(border_bits) = border_bits {
            params.border_bits = border_bits;
        }
        if let Some(inset_frac) = inset_frac {
            params.inset_frac = inset_frac;
        }
        if let Some(marker_size_rel) = marker_size_rel {
            params.marker_size_rel = marker_size_rel;
        }
        if let Some(min_border_score) = min_border_score {
            params.min_border_score = min_border_score;
        }
        if let Some(dedup_by_id) = dedup_by_id {
            params.dedup_by_id = dedup_by_id;
        }
        Ok(Self { inner: params })
    }
}

#[pyclass(name = "CharucoDetectorParams", module = "calib_targets")]
#[derive(Clone, Debug, Default)]
/// Optional overrides for ChArUco detector parameters.
struct PyCharucoDetectorParams {
    overrides: CharucoDetectorParamsOverrides,
}

#[pymethods]
impl PyCharucoDetectorParams {
    #[new]
    #[pyo3(signature = (*, px_per_square=None, chessboard=None, graph=None, scan=None, max_hamming=None, min_marker_inliers=None))]
    fn new(
        px_per_square: Option<f32>,
        chessboard: Option<&Bound<'_, PyAny>>,
        graph: Option<&Bound<'_, PyAny>>,
        scan: Option<&Bound<'_, PyAny>>,
        max_hamming: Option<u8>,
        min_marker_inliers: Option<usize>,
    ) -> PyResult<Self> {
        let mut overrides = CharucoDetectorParamsOverrides::default();
        if let Some(px_per_square) = px_per_square {
            overrides.px_per_square = Some(px_per_square);
        }
        if let Some(chessboard) = chessboard {
            overrides.chessboard = Some(chessboard_overrides_from_obj(chessboard, "chessboard")?);
        }
        if let Some(graph) = graph {
            overrides.graph = Some(grid_graph_overrides_from_obj(graph, "graph")?);
        }
        if let Some(scan) = scan {
            overrides.scan = Some(scan_decode_overrides_from_obj(scan, "scan")?);
        }
        if let Some(max_hamming) = max_hamming {
            overrides.max_hamming = Some(max_hamming);
        }
        if let Some(min_marker_inliers) = min_marker_inliers {
            overrides.min_marker_inliers = Some(min_marker_inliers);
        }
        Ok(Self { overrides })
    }
}

#[pyclass(name = "CircleScoreParams", module = "calib_targets")]
#[derive(Clone, Debug)]
/// Parameters for scoring circle markers.
struct PyCircleScoreParams {
    inner: marker::CircleScoreParams,
}

#[pymethods]
impl PyCircleScoreParams {
    #[new]
    #[pyo3(signature = (*, patch_size=None, diameter_frac=None, ring_thickness_frac=None, ring_radius_mul=None, min_contrast=None, samples=None, center_search_px=None))]
    fn new(
        patch_size: Option<usize>,
        diameter_frac: Option<f32>,
        ring_thickness_frac: Option<f32>,
        ring_radius_mul: Option<f32>,
        min_contrast: Option<f32>,
        samples: Option<usize>,
        center_search_px: Option<i32>,
    ) -> PyResult<Self> {
        let mut params = marker::CircleScoreParams::default();
        if let Some(patch_size) = patch_size {
            params.patch_size = patch_size;
        }
        if let Some(diameter_frac) = diameter_frac {
            params.diameter_frac = diameter_frac;
        }
        if let Some(ring_thickness_frac) = ring_thickness_frac {
            params.ring_thickness_frac = ring_thickness_frac;
        }
        if let Some(ring_radius_mul) = ring_radius_mul {
            params.ring_radius_mul = ring_radius_mul;
        }
        if let Some(min_contrast) = min_contrast {
            params.min_contrast = min_contrast;
        }
        if let Some(samples) = samples {
            params.samples = samples;
        }
        if let Some(center_search_px) = center_search_px {
            params.center_search_px = center_search_px;
        }
        Ok(Self { inner: params })
    }
}

#[pyclass(name = "CircleMatchParams", module = "calib_targets")]
#[derive(Clone, Debug)]
/// Parameters for matching detected circles to the board layout.
struct PyCircleMatchParams {
    inner: marker::CircleMatchParams,
}

#[pymethods]
impl PyCircleMatchParams {
    #[new]
    #[pyo3(signature = (*, max_candidates_per_polarity=None, max_distance_cells=None, min_offset_inliers=None))]
    fn new(
        max_candidates_per_polarity: Option<usize>,
        max_distance_cells: Option<f32>,
        min_offset_inliers: Option<usize>,
    ) -> PyResult<Self> {
        let mut params = marker::CircleMatchParams::default();
        if let Some(max_candidates_per_polarity) = max_candidates_per_polarity {
            params.max_candidates_per_polarity = max_candidates_per_polarity;
        }
        if let Some(max_distance_cells) = max_distance_cells {
            params.max_distance_cells = Some(max_distance_cells);
        }
        if let Some(min_offset_inliers) = min_offset_inliers {
            params.min_offset_inliers = min_offset_inliers;
        }
        Ok(Self { inner: params })
    }
}

#[pyclass(name = "MarkerBoardParams", module = "calib_targets")]
#[derive(Clone, Debug)]
/// Parameters for marker-board detection.
struct PyMarkerBoardParams {
    inner: marker::MarkerBoardParams,
}

#[pymethods]
impl PyMarkerBoardParams {
    #[new]
    #[pyo3(signature = (*, layout=None, chessboard=None, grid_graph=None, circle_score=None, match_params=None, roi_cells=None))]
    fn new(
        layout: Option<&Bound<'_, PyAny>>,
        chessboard: Option<&Bound<'_, PyAny>>,
        grid_graph: Option<&Bound<'_, PyAny>>,
        circle_score: Option<&Bound<'_, PyAny>>,
        match_params: Option<&Bound<'_, PyAny>>,
        roi_cells: Option<[i32; 4]>,
    ) -> PyResult<Self> {
        let mut params = marker::MarkerBoardParams::default();
        if let Some(layout) = layout {
            let layout = marker_board_layout_from_obj(layout, "layout")?;
            params = marker::MarkerBoardParams::new(layout);
        }
        if let Some(chessboard) = chessboard {
            params.chessboard =
                chessboard_params_from_obj(chessboard, "chessboard", params.chessboard.clone())?;
        }
        if let Some(grid_graph) = grid_graph {
            params.grid_graph =
                grid_graph_params_from_obj(grid_graph, "grid_graph", params.grid_graph.clone())?;
        }
        if let Some(circle_score) = circle_score {
            params.circle_score =
                circle_score_params_from_obj(circle_score, "circle_score", params.circle_score)?;
        }
        if let Some(match_params) = match_params {
            params.match_params = circle_match_params_from_obj(
                match_params,
                "match_params",
                params.match_params.clone(),
            )?;
        }
        if let Some(roi_cells) = roi_cells {
            params.roi_cells = Some(roi_cells);
        }
        Ok(Self { inner: params })
    }
}

#[derive(Debug, Default, Deserialize, Clone)]
struct ChessConfigOverrides {
    #[serde(default)]
    params: Option<ChessParamsOverrides>,
    #[serde(default)]
    multiscale: Option<CoarseToFineOverrides>,
}

impl ChessConfigOverrides {
    fn apply(self, cfg: &mut ChessConfig) {
        if let Some(params) = self.params {
            params.apply(&mut cfg.params);
        }
        if let Some(multiscale) = self.multiscale {
            multiscale.apply(&mut cfg.multiscale);
        }
    }
}

#[derive(Debug, Default, Deserialize, Clone)]
struct ChessParamsOverrides {
    #[serde(default)]
    use_radius10: Option<bool>,
    #[serde(default)]
    descriptor_use_radius10: Option<bool>,
    #[serde(default)]
    threshold_rel: Option<f32>,
    #[serde(default)]
    threshold_abs: Option<f32>,
    #[serde(default)]
    nms_radius: Option<u32>,
    #[serde(default)]
    min_cluster_size: Option<u32>,
}

impl ChessParamsOverrides {
    fn apply(self, params: &mut ChessParams) {
        if let Some(use_radius10) = self.use_radius10 {
            params.use_radius10 = use_radius10;
        }
        if let Some(descriptor_use_radius10) = self.descriptor_use_radius10 {
            params.descriptor_use_radius10 = Some(descriptor_use_radius10);
        }
        if let Some(threshold_rel) = self.threshold_rel {
            params.threshold_rel = threshold_rel;
        }
        if let Some(threshold_abs) = self.threshold_abs {
            params.threshold_abs = Some(threshold_abs);
        }
        if let Some(nms_radius) = self.nms_radius {
            params.nms_radius = nms_radius;
        }
        if let Some(min_cluster_size) = self.min_cluster_size {
            params.min_cluster_size = min_cluster_size;
        }
    }
}

#[derive(Debug, Default, Deserialize, Clone)]
struct CoarseToFineOverrides {
    #[serde(default)]
    pyramid: Option<PyramidOverrides>,
    #[serde(default)]
    refinement_radius: Option<u32>,
    #[serde(default)]
    merge_radius: Option<f32>,
}

impl CoarseToFineOverrides {
    fn apply(self, params: &mut CoarseToFineParams) {
        if let Some(pyramid) = self.pyramid {
            pyramid.apply(&mut params.pyramid);
        }
        if let Some(refinement_radius) = self.refinement_radius {
            params.refinement_radius = refinement_radius;
        }
        if let Some(merge_radius) = self.merge_radius {
            params.merge_radius = merge_radius;
        }
    }
}

#[derive(Debug, Default, Deserialize, Clone)]
struct PyramidOverrides {
    #[serde(default)]
    num_levels: Option<u8>,
    #[serde(default)]
    min_size: Option<usize>,
}

impl PyramidOverrides {
    fn apply(self, params: &mut PyramidParams) {
        if let Some(num_levels) = self.num_levels {
            params.num_levels = num_levels;
        }
        if let Some(min_size) = self.min_size {
            params.min_size = min_size;
        }
    }
}

#[derive(Debug, Default, Deserialize, Clone)]
struct OrientationClusteringParamsOverrides {
    #[serde(default)]
    num_bins: Option<usize>,
    #[serde(default)]
    max_iters: Option<usize>,
    #[serde(default)]
    peak_min_separation_deg: Option<f32>,
    #[serde(default)]
    outlier_threshold_deg: Option<f32>,
    #[serde(default)]
    min_peak_weight_fraction: Option<f32>,
    #[serde(default)]
    use_weights: Option<bool>,
}

impl OrientationClusteringParamsOverrides {
    fn apply(self, params: &mut core::OrientationClusteringParams) {
        if let Some(num_bins) = self.num_bins {
            params.num_bins = num_bins;
        }
        if let Some(max_iters) = self.max_iters {
            params.max_iters = max_iters;
        }
        if let Some(peak_min_separation_deg) = self.peak_min_separation_deg {
            params.peak_min_separation_deg = peak_min_separation_deg;
        }
        if let Some(outlier_threshold_deg) = self.outlier_threshold_deg {
            params.outlier_threshold_deg = outlier_threshold_deg;
        }
        if let Some(min_peak_weight_fraction) = self.min_peak_weight_fraction {
            params.min_peak_weight_fraction = min_peak_weight_fraction;
        }
        if let Some(use_weights) = self.use_weights {
            params.use_weights = use_weights;
        }
    }

    fn from_params(params: &core::OrientationClusteringParams) -> Self {
        Self {
            num_bins: Some(params.num_bins),
            max_iters: Some(params.max_iters),
            peak_min_separation_deg: Some(params.peak_min_separation_deg),
            outlier_threshold_deg: Some(params.outlier_threshold_deg),
            min_peak_weight_fraction: Some(params.min_peak_weight_fraction),
            use_weights: Some(params.use_weights),
        }
    }
}

#[derive(Debug, Default, Deserialize, Clone)]
struct ChessboardParamsOverrides {
    #[serde(default)]
    min_corner_strength: Option<f32>,
    #[serde(default)]
    min_corners: Option<usize>,
    #[serde(default)]
    expected_rows: Option<Option<u32>>,
    #[serde(default)]
    expected_cols: Option<Option<u32>>,
    #[serde(default)]
    completeness_threshold: Option<f32>,
    #[serde(default)]
    use_orientation_clustering: Option<bool>,
    #[serde(default)]
    orientation_clustering_params: Option<OrientationClusteringParamsOverrides>,
}

impl ChessboardParamsOverrides {
    fn apply(self, params: &mut chessboard::ChessboardParams) {
        if let Some(min_corner_strength) = self.min_corner_strength {
            params.min_corner_strength = min_corner_strength;
        }
        if let Some(min_corners) = self.min_corners {
            params.min_corners = min_corners;
        }
        if let Some(expected_rows) = self.expected_rows {
            params.expected_rows = expected_rows;
        }
        if let Some(expected_cols) = self.expected_cols {
            params.expected_cols = expected_cols;
        }
        if let Some(completeness_threshold) = self.completeness_threshold {
            params.completeness_threshold = completeness_threshold;
        }
        if let Some(use_orientation_clustering) = self.use_orientation_clustering {
            params.use_orientation_clustering = use_orientation_clustering;
        }
        if let Some(orientation_clustering_params) = self.orientation_clustering_params {
            orientation_clustering_params.apply(&mut params.orientation_clustering_params);
        }
    }

    fn from_params(params: &chessboard::ChessboardParams) -> Self {
        Self {
            min_corner_strength: Some(params.min_corner_strength),
            min_corners: Some(params.min_corners),
            expected_rows: params.expected_rows.map(Some),
            expected_cols: params.expected_cols.map(Some),
            completeness_threshold: Some(params.completeness_threshold),
            use_orientation_clustering: Some(params.use_orientation_clustering),
            orientation_clustering_params: Some(OrientationClusteringParamsOverrides::from_params(
                &params.orientation_clustering_params,
            )),
        }
    }
}

#[derive(Debug, Default, Deserialize, Clone)]
struct GridGraphParamsOverrides {
    #[serde(default)]
    min_spacing_pix: Option<f32>,
    #[serde(default)]
    max_spacing_pix: Option<f32>,
    #[serde(default)]
    k_neighbors: Option<usize>,
    #[serde(default)]
    orientation_tolerance_deg: Option<f32>,
}

impl GridGraphParamsOverrides {
    fn apply(self, params: &mut chessboard::GridGraphParams) {
        if let Some(min_spacing_pix) = self.min_spacing_pix {
            params.min_spacing_pix = min_spacing_pix;
        }
        if let Some(max_spacing_pix) = self.max_spacing_pix {
            params.max_spacing_pix = max_spacing_pix;
        }
        if let Some(k_neighbors) = self.k_neighbors {
            params.k_neighbors = k_neighbors;
        }
        if let Some(orientation_tolerance_deg) = self.orientation_tolerance_deg {
            params.orientation_tolerance_deg = orientation_tolerance_deg;
        }
    }

    fn from_params(params: &chessboard::GridGraphParams) -> Self {
        Self {
            min_spacing_pix: Some(params.min_spacing_pix),
            max_spacing_pix: Some(params.max_spacing_pix),
            k_neighbors: Some(params.k_neighbors),
            orientation_tolerance_deg: Some(params.orientation_tolerance_deg),
        }
    }
}

#[derive(Debug, Default, Deserialize, Clone)]
struct ScanDecodeConfigOverrides {
    #[serde(default)]
    border_bits: Option<usize>,
    #[serde(default)]
    inset_frac: Option<f32>,
    #[serde(default)]
    marker_size_rel: Option<f32>,
    #[serde(default)]
    min_border_score: Option<f32>,
    #[serde(default)]
    dedup_by_id: Option<bool>,
}

impl ScanDecodeConfigOverrides {
    fn apply(self, params: &mut aruco::ScanDecodeConfig) {
        if let Some(border_bits) = self.border_bits {
            params.border_bits = border_bits;
        }
        if let Some(inset_frac) = self.inset_frac {
            params.inset_frac = inset_frac;
        }
        if let Some(marker_size_rel) = self.marker_size_rel {
            params.marker_size_rel = marker_size_rel;
        }
        if let Some(min_border_score) = self.min_border_score {
            params.min_border_score = min_border_score;
        }
        if let Some(dedup_by_id) = self.dedup_by_id {
            params.dedup_by_id = dedup_by_id;
        }
    }

    fn from_params(params: &aruco::ScanDecodeConfig) -> Self {
        Self {
            border_bits: Some(params.border_bits),
            inset_frac: Some(params.inset_frac),
            marker_size_rel: Some(params.marker_size_rel),
            min_border_score: Some(params.min_border_score),
            dedup_by_id: Some(params.dedup_by_id),
        }
    }
}

#[derive(Debug, Default, Deserialize, Clone)]
struct CharucoDetectorParamsOverrides {
    #[serde(default)]
    px_per_square: Option<f32>,
    #[serde(default)]
    chessboard: Option<ChessboardParamsOverrides>,
    #[serde(default)]
    charuco: Option<charuco::CharucoBoardSpec>,
    #[serde(default)]
    graph: Option<GridGraphParamsOverrides>,
    #[serde(default)]
    scan: Option<ScanDecodeConfigOverrides>,
    #[serde(default)]
    max_hamming: Option<u8>,
    #[serde(default)]
    min_marker_inliers: Option<usize>,
}

impl CharucoDetectorParamsOverrides {
    fn apply(self, params: &mut charuco::CharucoDetectorParams) {
        if let Some(px_per_square) = self.px_per_square {
            params.px_per_square = px_per_square;
        }
        if let Some(chessboard) = self.chessboard {
            chessboard.apply(&mut params.chessboard);
        }
        if let Some(charuco) = self.charuco {
            params.charuco = charuco;
        }
        if let Some(graph) = self.graph {
            graph.apply(&mut params.graph);
        }
        if let Some(scan) = self.scan {
            scan.apply(&mut params.scan);
        }
        if let Some(max_hamming) = self.max_hamming {
            params.max_hamming = max_hamming;
        }
        if let Some(min_marker_inliers) = self.min_marker_inliers {
            params.min_marker_inliers = min_marker_inliers;
        }
    }
}

#[derive(Debug, Default, Deserialize, Clone)]
struct CircleScoreParamsOverrides {
    #[serde(default)]
    patch_size: Option<usize>,
    #[serde(default)]
    diameter_frac: Option<f32>,
    #[serde(default)]
    ring_thickness_frac: Option<f32>,
    #[serde(default)]
    ring_radius_mul: Option<f32>,
    #[serde(default)]
    min_contrast: Option<f32>,
    #[serde(default)]
    samples: Option<usize>,
    #[serde(default)]
    center_search_px: Option<i32>,
}

impl CircleScoreParamsOverrides {
    fn apply(self, params: &mut marker::CircleScoreParams) {
        if let Some(patch_size) = self.patch_size {
            params.patch_size = patch_size;
        }
        if let Some(diameter_frac) = self.diameter_frac {
            params.diameter_frac = diameter_frac;
        }
        if let Some(ring_thickness_frac) = self.ring_thickness_frac {
            params.ring_thickness_frac = ring_thickness_frac;
        }
        if let Some(ring_radius_mul) = self.ring_radius_mul {
            params.ring_radius_mul = ring_radius_mul;
        }
        if let Some(min_contrast) = self.min_contrast {
            params.min_contrast = min_contrast;
        }
        if let Some(samples) = self.samples {
            params.samples = samples;
        }
        if let Some(center_search_px) = self.center_search_px {
            params.center_search_px = center_search_px;
        }
    }
}

#[derive(Debug, Default, Deserialize, Clone)]
struct CircleMatchParamsOverrides {
    #[serde(default)]
    max_candidates_per_polarity: Option<usize>,
    #[serde(default)]
    max_distance_cells: Option<f32>,
    #[serde(default)]
    min_offset_inliers: Option<usize>,
}

impl CircleMatchParamsOverrides {
    fn apply(self, params: &mut marker::CircleMatchParams) {
        if let Some(max_candidates_per_polarity) = self.max_candidates_per_polarity {
            params.max_candidates_per_polarity = max_candidates_per_polarity;
        }
        if let Some(max_distance_cells) = self.max_distance_cells {
            params.max_distance_cells = Some(max_distance_cells);
        }
        if let Some(min_offset_inliers) = self.min_offset_inliers {
            params.min_offset_inliers = min_offset_inliers;
        }
    }
}

#[derive(Debug, Default, Deserialize, Clone)]
struct MarkerBoardLayoutOverrides {
    #[serde(default)]
    rows: Option<u32>,
    #[serde(default)]
    cols: Option<u32>,
    #[serde(default)]
    cell_size: Option<f32>,
    #[serde(default)]
    circles: Option<[marker::MarkerCircleSpec; 3]>,
}

impl MarkerBoardLayoutOverrides {
    fn apply(self, layout: &mut marker::MarkerBoardLayout) {
        if let Some(rows) = self.rows {
            layout.rows = rows;
        }
        if let Some(cols) = self.cols {
            layout.cols = cols;
        }
        if let Some(cell_size) = self.cell_size {
            layout.cell_size = Some(cell_size);
        }
        if let Some(circles) = self.circles {
            layout.circles = circles;
        }
    }
}

#[derive(Debug, Default, Deserialize, Clone)]
struct MarkerBoardParamsOverrides {
    #[serde(default)]
    layout: Option<MarkerBoardLayoutOverrides>,
    #[serde(default)]
    chessboard: Option<ChessboardParamsOverrides>,
    #[serde(default)]
    grid_graph: Option<GridGraphParamsOverrides>,
    #[serde(default)]
    circle_score: Option<CircleScoreParamsOverrides>,
    #[serde(default)]
    match_params: Option<CircleMatchParamsOverrides>,
    #[serde(default)]
    roi_cells: Option<[i32; 4]>,
}

impl MarkerBoardParamsOverrides {
    fn apply(self, params: &mut marker::MarkerBoardParams) {
        if let Some(layout) = self.layout {
            layout.apply(&mut params.layout);
        }
        if let Some(chessboard) = self.chessboard {
            chessboard.apply(&mut params.chessboard);
        }
        if let Some(grid_graph) = self.grid_graph {
            grid_graph.apply(&mut params.grid_graph);
        }
        if let Some(circle_score) = self.circle_score {
            circle_score.apply(&mut params.circle_score);
        }
        if let Some(match_params) = self.match_params {
            match_params.apply(&mut params.match_params);
        }
        if let Some(roi_cells) = self.roi_cells {
            params.roi_cells = Some(roi_cells);
        }
    }
}

fn value_error(message: impl Into<String>) -> PyErr {
    PyValueError::new_err(message.into())
}

fn format_key_list(keys: &[String]) -> String {
    let quoted: Vec<String> = keys.iter().map(|key| format!("\"{key}\"")).collect();
    format!("[{}]", quoted.join(", "))
}

fn validate_dict_keys(dict: &Bound<'_, PyDict>, path: &str, allowed: &[&str]) -> PyResult<()> {
    let mut unknown = Vec::new();
    for (key, _) in dict.iter() {
        let key_str: String = key.extract().map_err(|_| {
            value_error(format!(
                "{path}: dictionary keys must be strings for JSON conversion"
            ))
        })?;
        if !allowed.iter().any(|k| *k == key_str) {
            unknown.push(key_str);
        }
    }

    if unknown.is_empty() {
        return Ok(());
    }

    unknown.sort_unstable();
    let mut valid: Vec<String> = allowed.iter().map(|key| (*key).to_string()).collect();
    valid.sort_unstable();
    Err(value_error(format!(
        "{path}: unknown keys {}; valid keys: {}",
        format_key_list(&unknown),
        format_key_list(&valid)
    )))
}

fn get_optional_dict<'py>(
    dict: &'py Bound<'py, PyDict>,
    key: &str,
) -> PyResult<Option<Bound<'py, PyDict>>> {
    let Some(value) = dict.get_item(key)? else {
        return Ok(None);
    };
    Ok(value.downcast::<PyDict>().ok().cloned())
}

fn validate_chess_cfg_dict(dict: &Bound<'_, PyDict>, path: &str) -> PyResult<()> {
    validate_dict_keys(dict, path, &["params", "multiscale"])?;
    if let Some(params) = get_optional_dict(dict, "params")? {
        validate_chess_params_dict(&params, &format!("{path}.params"))?;
    }
    if let Some(multiscale) = get_optional_dict(dict, "multiscale")? {
        validate_coarse_to_fine_dict(&multiscale, &format!("{path}.multiscale"))?;
    }
    Ok(())
}

fn validate_chess_params_dict(dict: &Bound<'_, PyDict>, path: &str) -> PyResult<()> {
    validate_dict_keys(
        dict,
        path,
        &[
            "use_radius10",
            "descriptor_use_radius10",
            "threshold_rel",
            "threshold_abs",
            "nms_radius",
            "min_cluster_size",
        ],
    )
}

fn validate_coarse_to_fine_dict(dict: &Bound<'_, PyDict>, path: &str) -> PyResult<()> {
    validate_dict_keys(
        dict,
        path,
        &["pyramid", "refinement_radius", "merge_radius"],
    )?;
    if let Some(pyramid) = get_optional_dict(dict, "pyramid")? {
        validate_pyramid_dict(&pyramid, &format!("{path}.pyramid"))?;
    }
    Ok(())
}

fn validate_pyramid_dict(dict: &Bound<'_, PyDict>, path: &str) -> PyResult<()> {
    validate_dict_keys(dict, path, &["num_levels", "min_size"])
}

fn validate_orientation_clustering_dict(dict: &Bound<'_, PyDict>, path: &str) -> PyResult<()> {
    validate_dict_keys(
        dict,
        path,
        &[
            "num_bins",
            "max_iters",
            "peak_min_separation_deg",
            "outlier_threshold_deg",
            "min_peak_weight_fraction",
            "use_weights",
        ],
    )
}

fn validate_chessboard_params_dict(dict: &Bound<'_, PyDict>, path: &str) -> PyResult<()> {
    validate_dict_keys(
        dict,
        path,
        &[
            "min_corner_strength",
            "min_corners",
            "expected_rows",
            "expected_cols",
            "completeness_threshold",
            "use_orientation_clustering",
            "orientation_clustering_params",
        ],
    )?;
    if let Some(orientation) = get_optional_dict(dict, "orientation_clustering_params")? {
        validate_orientation_clustering_dict(
            &orientation,
            &format!("{path}.orientation_clustering_params"),
        )?;
    }
    Ok(())
}

fn validate_grid_graph_dict(dict: &Bound<'_, PyDict>, path: &str) -> PyResult<()> {
    validate_dict_keys(
        dict,
        path,
        &[
            "min_spacing_pix",
            "max_spacing_pix",
            "k_neighbors",
            "orientation_tolerance_deg",
        ],
    )
}

fn validate_scan_decode_dict(dict: &Bound<'_, PyDict>, path: &str) -> PyResult<()> {
    validate_dict_keys(
        dict,
        path,
        &[
            "border_bits",
            "inset_frac",
            "marker_size_rel",
            "min_border_score",
            "dedup_by_id",
        ],
    )
}

fn validate_charuco_params_dict(dict: &Bound<'_, PyDict>, path: &str) -> PyResult<()> {
    validate_dict_keys(
        dict,
        path,
        &[
            "px_per_square",
            "chessboard",
            "charuco",
            "graph",
            "scan",
            "max_hamming",
            "min_marker_inliers",
        ],
    )?;
    if let Some(chessboard) = get_optional_dict(dict, "chessboard")? {
        validate_chessboard_params_dict(&chessboard, &format!("{path}.chessboard"))?;
    }
    if let Some(graph) = get_optional_dict(dict, "graph")? {
        validate_grid_graph_dict(&graph, &format!("{path}.graph"))?;
    }
    if let Some(scan) = get_optional_dict(dict, "scan")? {
        validate_scan_decode_dict(&scan, &format!("{path}.scan"))?;
    }
    Ok(())
}

fn validate_circle_score_dict(dict: &Bound<'_, PyDict>, path: &str) -> PyResult<()> {
    validate_dict_keys(
        dict,
        path,
        &[
            "patch_size",
            "diameter_frac",
            "ring_thickness_frac",
            "ring_radius_mul",
            "min_contrast",
            "samples",
            "center_search_px",
        ],
    )
}

fn validate_circle_match_dict(dict: &Bound<'_, PyDict>, path: &str) -> PyResult<()> {
    validate_dict_keys(
        dict,
        path,
        &[
            "max_candidates_per_polarity",
            "max_distance_cells",
            "min_offset_inliers",
        ],
    )
}

fn validate_marker_board_layout_dict(dict: &Bound<'_, PyDict>, path: &str) -> PyResult<()> {
    validate_dict_keys(dict, path, &["rows", "cols", "cell_size", "circles"])
}

fn validate_marker_board_params_dict(dict: &Bound<'_, PyDict>, path: &str) -> PyResult<()> {
    validate_dict_keys(
        dict,
        path,
        &[
            "layout",
            "chessboard",
            "grid_graph",
            "circle_score",
            "match_params",
            "roi_cells",
        ],
    )?;
    if let Some(layout) = get_optional_dict(dict, "layout")? {
        validate_marker_board_layout_dict(&layout, &format!("{path}.layout"))?;
    }
    if let Some(chessboard) = get_optional_dict(dict, "chessboard")? {
        validate_chessboard_params_dict(&chessboard, &format!("{path}.chessboard"))?;
    }
    if let Some(grid_graph) = get_optional_dict(dict, "grid_graph")? {
        validate_grid_graph_dict(&grid_graph, &format!("{path}.grid_graph"))?;
    }
    if let Some(circle_score) = get_optional_dict(dict, "circle_score")? {
        validate_circle_score_dict(&circle_score, &format!("{path}.circle_score"))?;
    }
    if let Some(match_params) = get_optional_dict(dict, "match_params")? {
        validate_circle_match_dict(&match_params, &format!("{path}.match_params"))?;
    }
    Ok(())
}

const NUMPY_SCALAR_TYPES: &[&str] = &[
    "bool_", "bool8", "int8", "int16", "int32", "int64", "uint8", "uint16", "uint32", "uint64",
    "float16", "float32", "float64", "float128",
];

fn is_numpy_scalar(obj: &Bound<'_, PyAny>) -> bool {
    let ty = obj.get_type();
    let Ok(module) = ty.getattr("__module__") else {
        return false;
    };
    let Ok(module_name) = module.extract::<&str>() else {
        return false;
    };
    if module_name != "numpy" {
        return false;
    }
    let Ok(name) = ty.name() else {
        return false;
    };
    NUMPY_SCALAR_TYPES.iter().any(|known| *known == name)
}

fn py_to_json(obj: &Bound<'_, PyAny>, path: &str) -> PyResult<Value> {
    if obj.is_none() {
        return Ok(Value::Null);
    }

    if obj.is_instance_of::<PyBool>() {
        return Ok(Value::Bool(obj.extract::<bool>()?));
    }

    if let Ok(dict) = obj.downcast::<PyDict>() {
        let mut out = Map::with_capacity(dict.len());
        for (key, value) in dict.iter() {
            let key_str: String = key.extract().map_err(|_| {
                value_error(format!(
                    "{path}: dictionary keys must be strings for JSON conversion"
                ))
            })?;
            let child_path = format!("{path}.{key_str}");
            let value_json = py_to_json(&value, &child_path)?;
            out.insert(key_str, value_json);
        }
        return Ok(Value::Object(out));
    }

    if let Ok(list) = obj.downcast::<PyList>() {
        let mut out = Vec::with_capacity(list.len());
        for (idx, item) in list.iter().enumerate() {
            let child_path = format!("{path}[{idx}]");
            out.push(py_to_json(&item, &child_path)?);
        }
        return Ok(Value::Array(out));
    }

    if let Ok(tuple) = obj.downcast::<PyTuple>() {
        let mut out = Vec::with_capacity(tuple.len());
        for (idx, item) in tuple.iter().enumerate() {
            let child_path = format!("{path}[{idx}]");
            out.push(py_to_json(&item, &child_path)?);
        }
        return Ok(Value::Array(out));
    }

    if obj.is_instance_of::<PyString>() {
        let text: String = obj.extract()?;
        return Ok(Value::String(text));
    }

    if is_numpy_scalar(obj) {
        if let Ok(item) = obj.call_method0("item") {
            return py_to_json(&item, path);
        }
    }

    if let Ok(value) = obj.extract::<i64>() {
        return Ok(Value::Number(value.into()));
    }

    if let Ok(value) = obj.extract::<u64>() {
        return Ok(Value::Number(value.into()));
    }

    if let Ok(value) = obj.extract::<f64>() {
        let number = Number::from_f64(value).ok_or_else(|| {
            value_error(format!("{path}: non-finite float is not JSON compatible"))
        })?;
        return Ok(Value::Number(number));
    }

    Err(value_error(format!(
        "{path}: unsupported type for JSON conversion"
    )))
}

fn json_to_py(py: Python<'_>, value: &Value) -> PyResult<PyObject> {
    match value {
        Value::Null => Ok(py.None()),
        Value::Bool(v) => Ok(v.into_py(py)),
        Value::Number(v) => {
            if let Some(i) = v.as_i64() {
                Ok(i.into_py(py))
            } else if let Some(u) = v.as_u64() {
                Ok(u.into_py(py))
            } else if let Some(f) = v.as_f64() {
                Ok(f.into_py(py))
            } else {
                Ok(py.None())
            }
        }
        Value::String(s) => Ok(s.into_py(py)),
        Value::Array(values) => {
            let mut out = Vec::with_capacity(values.len());
            for item in values {
                out.push(json_to_py(py, item)?);
            }
            Ok(PyList::new_bound(py, out).into_py(py))
        }
        Value::Object(map) => {
            let dict = PyDict::new_bound(py);
            for (key, item) in map.iter() {
                let value = json_to_py(py, item)?;
                dict.set_item(key, value)?;
            }
            Ok(dict.into_py(py))
        }
    }
}

fn json_number_f32(value: f32) -> Value {
    Number::from_f64(f64::from(value))
        .map(Value::Number)
        .unwrap_or(Value::Null)
}

fn json_number_u64(value: u64) -> Value {
    Value::Number(Number::from(value))
}

fn chess_params_to_json(params: &ChessParams) -> Value {
    let mut map = Map::new();
    map.insert("use_radius10".to_string(), Value::Bool(params.use_radius10));
    map.insert(
        "descriptor_use_radius10".to_string(),
        params
            .descriptor_use_radius10
            .map(Value::Bool)
            .unwrap_or(Value::Null),
    );
    map.insert(
        "threshold_rel".to_string(),
        json_number_f32(params.threshold_rel),
    );
    map.insert(
        "threshold_abs".to_string(),
        params
            .threshold_abs
            .map(json_number_f32)
            .unwrap_or(Value::Null),
    );
    map.insert(
        "nms_radius".to_string(),
        json_number_u64(params.nms_radius as u64),
    );
    map.insert(
        "min_cluster_size".to_string(),
        json_number_u64(params.min_cluster_size as u64),
    );
    Value::Object(map)
}

fn pyramid_params_to_json(params: &PyramidParams) -> Value {
    let mut map = Map::new();
    map.insert(
        "num_levels".to_string(),
        json_number_u64(params.num_levels as u64),
    );
    map.insert(
        "min_size".to_string(),
        json_number_u64(params.min_size as u64),
    );
    Value::Object(map)
}

fn coarse_to_fine_to_json(params: &CoarseToFineParams) -> Value {
    let mut map = Map::new();
    map.insert(
        "pyramid".to_string(),
        pyramid_params_to_json(&params.pyramid),
    );
    map.insert(
        "refinement_radius".to_string(),
        json_number_u64(params.refinement_radius as u64),
    );
    map.insert(
        "merge_radius".to_string(),
        json_number_f32(params.merge_radius),
    );
    Value::Object(map)
}

fn chess_config_to_json(cfg: &ChessConfig) -> Value {
    let mut map = Map::new();
    map.insert("params".to_string(), chess_params_to_json(&cfg.params));
    map.insert(
        "multiscale".to_string(),
        coarse_to_fine_to_json(&cfg.multiscale),
    );
    Value::Object(map)
}

fn parse_required<T: DeserializeOwned>(obj: &Bound<'_, PyAny>, name: &str) -> PyResult<T> {
    if obj.is_none() {
        return Err(value_error(format!("{name} is required")));
    }
    let value = py_to_json(obj, name)?;
    serde_json::from_value(value).map_err(|err| value_error(format!("{name}: {err}")))
}

fn chess_params_from_obj(
    obj: &Bound<'_, PyAny>,
    path: &str,
    base: ChessParams,
) -> PyResult<ChessParams> {
    if obj.is_none() {
        return Ok(base);
    }
    if let Ok(params) = obj.extract::<PyRef<PyChessCornerParams>>() {
        return Ok(params.inner.clone());
    }
    if let Ok(dict) = obj.downcast::<PyDict>() {
        validate_chess_params_dict(dict, path)?;
    }
    let value = py_to_json(obj, path)?;
    let overrides: ChessParamsOverrides =
        serde_json::from_value(value).map_err(|err| value_error(format!("{path}: {err}")))?;
    let mut params = base;
    overrides.apply(&mut params);
    Ok(params)
}

fn pyramid_params_from_obj(
    obj: &Bound<'_, PyAny>,
    path: &str,
    base: PyramidParams,
) -> PyResult<PyramidParams> {
    if obj.is_none() {
        return Ok(base);
    }
    if let Ok(params) = obj.extract::<PyRef<PyPyramidParams>>() {
        return Ok(params.inner.clone());
    }
    if let Ok(dict) = obj.downcast::<PyDict>() {
        validate_pyramid_dict(dict, path)?;
    }
    let value = py_to_json(obj, path)?;
    let overrides: PyramidOverrides =
        serde_json::from_value(value).map_err(|err| value_error(format!("{path}: {err}")))?;
    let mut params = base;
    overrides.apply(&mut params);
    Ok(params)
}

fn coarse_to_fine_params_from_obj(
    obj: &Bound<'_, PyAny>,
    path: &str,
    base: CoarseToFineParams,
) -> PyResult<CoarseToFineParams> {
    if obj.is_none() {
        return Ok(base);
    }
    if let Ok(params) = obj.extract::<PyRef<PyCoarseToFineParams>>() {
        return Ok(params.inner.clone());
    }
    if let Ok(dict) = obj.downcast::<PyDict>() {
        validate_coarse_to_fine_dict(dict, path)?;
    }
    let value = py_to_json(obj, path)?;
    let overrides: CoarseToFineOverrides =
        serde_json::from_value(value).map_err(|err| value_error(format!("{path}: {err}")))?;
    let mut params = base;
    overrides.apply(&mut params);
    Ok(params)
}

fn orientation_clustering_params_from_obj(
    obj: &Bound<'_, PyAny>,
    path: &str,
    base: core::OrientationClusteringParams,
) -> PyResult<core::OrientationClusteringParams> {
    if obj.is_none() {
        return Ok(base);
    }
    if let Ok(params) = obj.extract::<PyRef<PyOrientationClusteringParams>>() {
        return Ok(params.inner.clone());
    }
    if let Ok(dict) = obj.downcast::<PyDict>() {
        validate_orientation_clustering_dict(dict, path)?;
    }
    let value = py_to_json(obj, path)?;
    let overrides: OrientationClusteringParamsOverrides =
        serde_json::from_value(value).map_err(|err| value_error(format!("{path}: {err}")))?;
    let mut params = base;
    overrides.apply(&mut params);
    Ok(params)
}

fn chessboard_params_from_obj(
    obj: &Bound<'_, PyAny>,
    path: &str,
    base: chessboard::ChessboardParams,
) -> PyResult<chessboard::ChessboardParams> {
    if obj.is_none() {
        return Ok(base);
    }
    if let Ok(params) = obj.extract::<PyRef<PyChessboardParams>>() {
        return Ok(params.inner.clone());
    }
    if let Ok(dict) = obj.downcast::<PyDict>() {
        validate_chessboard_params_dict(dict, path)?;
    }
    let value = py_to_json(obj, path)?;
    let overrides: ChessboardParamsOverrides =
        serde_json::from_value(value).map_err(|err| value_error(format!("{path}: {err}")))?;
    let mut params = base;
    overrides.apply(&mut params);
    Ok(params)
}

fn grid_graph_params_from_obj(
    obj: &Bound<'_, PyAny>,
    path: &str,
    base: chessboard::GridGraphParams,
) -> PyResult<chessboard::GridGraphParams> {
    if obj.is_none() {
        return Ok(base);
    }
    if let Ok(params) = obj.extract::<PyRef<PyGridGraphParams>>() {
        return Ok(params.inner.clone());
    }
    if let Ok(dict) = obj.downcast::<PyDict>() {
        validate_grid_graph_dict(dict, path)?;
    }
    let value = py_to_json(obj, path)?;
    let overrides: GridGraphParamsOverrides =
        serde_json::from_value(value).map_err(|err| value_error(format!("{path}: {err}")))?;
    let mut params = base;
    overrides.apply(&mut params);
    Ok(params)
}

fn circle_score_params_from_obj(
    obj: &Bound<'_, PyAny>,
    path: &str,
    base: marker::CircleScoreParams,
) -> PyResult<marker::CircleScoreParams> {
    if obj.is_none() {
        return Ok(base);
    }
    if let Ok(params) = obj.extract::<PyRef<PyCircleScoreParams>>() {
        return Ok(params.inner);
    }
    if let Ok(dict) = obj.downcast::<PyDict>() {
        validate_circle_score_dict(dict, path)?;
    }
    let value = py_to_json(obj, path)?;
    let overrides: CircleScoreParamsOverrides =
        serde_json::from_value(value).map_err(|err| value_error(format!("{path}: {err}")))?;
    let mut params = base;
    overrides.apply(&mut params);
    Ok(params)
}

fn circle_match_params_from_obj(
    obj: &Bound<'_, PyAny>,
    path: &str,
    base: marker::CircleMatchParams,
) -> PyResult<marker::CircleMatchParams> {
    if obj.is_none() {
        return Ok(base);
    }
    if let Ok(params) = obj.extract::<PyRef<PyCircleMatchParams>>() {
        return Ok(params.inner.clone());
    }
    if let Ok(dict) = obj.downcast::<PyDict>() {
        validate_circle_match_dict(dict, path)?;
    }
    let value = py_to_json(obj, path)?;
    let overrides: CircleMatchParamsOverrides =
        serde_json::from_value(value).map_err(|err| value_error(format!("{path}: {err}")))?;
    let mut params = base;
    overrides.apply(&mut params);
    Ok(params)
}

fn marker_board_layout_from_obj(
    obj: &Bound<'_, PyAny>,
    path: &str,
) -> PyResult<marker::MarkerBoardLayout> {
    let dict = obj
        .downcast::<PyDict>()
        .map_err(|_| value_error(format!("{path}: expected dict")))?;
    validate_marker_board_layout_dict(dict, path)?;
    let value = py_to_json(obj, path)?;
    let overrides: MarkerBoardLayoutOverrides =
        serde_json::from_value(value).map_err(|err| value_error(format!("{path}: {err}")))?;
    let mut layout = marker::MarkerBoardLayout::default();
    overrides.apply(&mut layout);
    Ok(layout)
}

fn chessboard_overrides_from_obj(
    obj: &Bound<'_, PyAny>,
    path: &str,
) -> PyResult<ChessboardParamsOverrides> {
    if let Ok(params) = obj.extract::<PyRef<PyChessboardParams>>() {
        return Ok(ChessboardParamsOverrides::from_params(&params.inner));
    }
    if let Ok(dict) = obj.downcast::<PyDict>() {
        validate_chessboard_params_dict(dict, path)?;
    }
    let value = py_to_json(obj, path)?;
    serde_json::from_value(value).map_err(|err| value_error(format!("{path}: {err}")))
}

fn grid_graph_overrides_from_obj(
    obj: &Bound<'_, PyAny>,
    path: &str,
) -> PyResult<GridGraphParamsOverrides> {
    if let Ok(params) = obj.extract::<PyRef<PyGridGraphParams>>() {
        return Ok(GridGraphParamsOverrides::from_params(&params.inner));
    }
    if let Ok(dict) = obj.downcast::<PyDict>() {
        validate_grid_graph_dict(dict, path)?;
    }
    let value = py_to_json(obj, path)?;
    serde_json::from_value(value).map_err(|err| value_error(format!("{path}: {err}")))
}

fn scan_decode_overrides_from_obj(
    obj: &Bound<'_, PyAny>,
    path: &str,
) -> PyResult<ScanDecodeConfigOverrides> {
    if let Ok(params) = obj.extract::<PyRef<PyScanDecodeConfig>>() {
        return Ok(ScanDecodeConfigOverrides::from_params(&params.inner));
    }
    if let Ok(dict) = obj.downcast::<PyDict>() {
        validate_scan_decode_dict(dict, path)?;
    }
    let value = py_to_json(obj, path)?;
    serde_json::from_value(value).map_err(|err| value_error(format!("{path}: {err}")))
}

fn chess_cfg_from_py(obj: Option<&Bound<'_, PyAny>>) -> PyResult<ChessConfig> {
    let mut cfg = detect::default_chess_config();
    if let Some(obj) = obj {
        if !obj.is_none() {
            if let Ok(py_cfg) = obj.extract::<PyRef<PyChessConfig>>() {
                return Ok(py_cfg.inner.clone());
            }
            if let Ok(dict) = obj.downcast::<PyDict>() {
                validate_chess_cfg_dict(dict, "chess_cfg")?;
            }
            let value = py_to_json(obj, "chess_cfg")?;
            let overrides: ChessConfigOverrides = serde_json::from_value(value)
                .map_err(|err| value_error(format!("chess_cfg: {err}")))?;
            overrides.apply(&mut cfg);
        }
    }
    Ok(cfg)
}

fn chessboard_params_from_py(
    obj: Option<&Bound<'_, PyAny>>,
) -> PyResult<chessboard::ChessboardParams> {
    let mut params = chessboard::ChessboardParams::default();
    let Some(obj) = obj else {
        return Ok(params);
    };
    if obj.is_none() {
        return Ok(params);
    }
    if let Ok(py_params) = obj.extract::<PyRef<PyChessboardParams>>() {
        return Ok(py_params.inner.clone());
    }
    if let Ok(dict) = obj.downcast::<PyDict>() {
        validate_chessboard_params_dict(dict, "params")?;
    }
    let value = py_to_json(obj, "params")?;
    let overrides: ChessboardParamsOverrides =
        serde_json::from_value(value).map_err(|err| value_error(format!("params: {err}")))?;
    overrides.apply(&mut params);
    Ok(params)
}

fn marker_board_params_from_py(
    obj: Option<&Bound<'_, PyAny>>,
) -> PyResult<marker::MarkerBoardParams> {
    let Some(obj) = obj else {
        return Ok(marker::MarkerBoardParams::default());
    };
    if obj.is_none() {
        return Ok(marker::MarkerBoardParams::default());
    }
    if let Ok(py_params) = obj.extract::<PyRef<PyMarkerBoardParams>>() {
        return Ok(py_params.inner.clone());
    }
    if let Ok(dict) = obj.downcast::<PyDict>() {
        validate_marker_board_params_dict(dict, "params")?;
    }
    let value = py_to_json(obj, "params")?;
    let mut overrides: MarkerBoardParamsOverrides =
        serde_json::from_value(value).map_err(|err| value_error(format!("params: {err}")))?;
    let mut params = marker::MarkerBoardParams::default();
    if let Some(layout_override) = overrides.layout.take() {
        let mut layout = marker::MarkerBoardLayout::default();
        layout_override.apply(&mut layout);
        params = marker::MarkerBoardParams::new(layout);
    }
    overrides.apply(&mut params);
    Ok(params)
}

fn charuco_params_from_py(
    obj: Option<&Bound<'_, PyAny>>,
    board: &charuco::CharucoBoardSpec,
) -> PyResult<charuco::CharucoDetectorParams> {
    let mut params = charuco::CharucoDetectorParams::for_board(board);
    let Some(obj) = obj else {
        return Ok(params);
    };
    if obj.is_none() {
        return Ok(params);
    }
    if let Ok(py_params) = obj.extract::<PyRef<PyCharucoDetectorParams>>() {
        let overrides = py_params.overrides.clone();
        overrides.apply(&mut params);
        return Ok(params);
    }
    if let Ok(dict) = obj.downcast::<PyDict>() {
        validate_charuco_params_dict(dict, "params")?;
    }
    let value = py_to_json(obj, "params")?;
    let overrides: CharucoDetectorParamsOverrides =
        serde_json::from_value(value).map_err(|err| value_error(format!("params: {err}")))?;
    overrides.apply(&mut params);
    Ok(params)
}

fn gray_image_from_py(image: &Bound<'_, PyAny>) -> PyResult<::image::GrayImage> {
    let array = image
        .downcast::<PyArrayDyn<u8>>()
        .map_err(|_| value_error("image must be a numpy.ndarray with dtype=uint8"))?;
    if array.ndim() != 2 {
        return Err(value_error("image must be a 2D array"));
    }
    let readonly = array.readonly();
    let view = readonly.as_array();
    let shape = view.shape();
    let height = *shape
        .first()
        .ok_or_else(|| value_error("image has no height"))?;
    let width = *shape
        .get(1)
        .ok_or_else(|| value_error("image has no width"))?;
    let height = u32::try_from(height).map_err(|_| value_error("image height is too large"))?;
    let width = u32::try_from(width).map_err(|_| value_error("image width is too large"))?;
    let pixels = view.to_owned().into_raw_vec();
    detect::gray_image_from_slice(width, height, &pixels)
        .map_err(|err| value_error(err.to_string()))
}

/// Detect a ChArUco board in a grayscale image.
///
/// Args:
///   image: 2D numpy.ndarray[uint8] (H, W) grayscale image.
///   board: Charuco board specification (dict).
///   chess_cfg: None | dict overrides | ChessConfig.
///   params: None | dict overrides | CharucoDetectorParams.
///
/// Returns:
///   dict with detection data, or raises RuntimeError on detection errors.
#[pyfunction]
#[pyo3(signature = (image, *, board, chess_cfg=None, params=None))]
fn detect_charuco(
    py: Python<'_>,
    image: &Bound<'_, PyAny>,
    board: &Bound<'_, PyAny>,
    chess_cfg: Option<&Bound<'_, PyAny>>,
    params: Option<&Bound<'_, PyAny>>,
) -> PyResult<PyObject> {
    let img = gray_image_from_py(image)?;
    let board = parse_required::<charuco::CharucoBoardSpec>(board, "board")?;
    let chess_cfg = chess_cfg_from_py(chess_cfg)?;
    let params = charuco_params_from_py(params, &board)?;

    let result = py.allow_threads(move || detect::detect_charuco(&img, &chess_cfg, board, params));
    let result = result.map_err(|err| PyRuntimeError::new_err(err.to_string()))?;
    let json =
        serde_json::to_value(result).map_err(|err| PyRuntimeError::new_err(err.to_string()))?;
    json_to_py(py, &json)
}

/// Detect a chessboard in a grayscale image.
///
/// Args:
///   image: 2D numpy.ndarray[uint8] (H, W) grayscale image.
///   chess_cfg: None | dict overrides | ChessConfig.
///   params: None | dict overrides | ChessboardParams.
///
/// Returns:
///   dict with detection data, or None if no board is found.
#[pyfunction]
#[pyo3(signature = (image, *, chess_cfg=None, params=None))]
fn detect_chessboard(
    py: Python<'_>,
    image: &Bound<'_, PyAny>,
    chess_cfg: Option<&Bound<'_, PyAny>>,
    params: Option<&Bound<'_, PyAny>>,
) -> PyResult<Option<PyObject>> {
    let img = gray_image_from_py(image)?;
    let chess_cfg = chess_cfg_from_py(chess_cfg)?;
    let params = chessboard_params_from_py(params)?;

    let result = py.allow_threads(move || detect::detect_chessboard(&img, &chess_cfg, params));
    match result {
        Some(res) => {
            let json = serde_json::to_value(res)
                .map_err(|err| PyRuntimeError::new_err(err.to_string()))?;
            Ok(Some(json_to_py(py, &json)?))
        }
        None => Ok(None),
    }
}

/// Detect a marker-board target in a grayscale image.
///
/// Args:
///   image: 2D numpy.ndarray[uint8] (H, W) grayscale image.
///   chess_cfg: None | dict overrides | ChessConfig.
///   params: None | dict overrides | MarkerBoardParams.
///
/// Returns:
///   dict with detection data, or None if no board is found.
#[pyfunction]
#[pyo3(signature = (image, *, chess_cfg=None, params=None))]
fn detect_marker_board(
    py: Python<'_>,
    image: &Bound<'_, PyAny>,
    chess_cfg: Option<&Bound<'_, PyAny>>,
    params: Option<&Bound<'_, PyAny>>,
) -> PyResult<Option<PyObject>> {
    let img = gray_image_from_py(image)?;
    let chess_cfg = chess_cfg_from_py(chess_cfg)?;
    let params = marker_board_params_from_py(params)?;

    let result = py.allow_threads(move || detect::detect_marker_board(&img, &chess_cfg, params));
    match result {
        Some(res) => {
            let json = serde_json::to_value(res)
                .map_err(|err| PyRuntimeError::new_err(err.to_string()))?;
            Ok(Some(json_to_py(py, &json)?))
        }
        None => Ok(None),
    }
}

#[pymodule]
fn calib_targets(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyChessConfig>()?;
    m.add_class::<PyChessCornerParams>()?;
    m.add_class::<PyCoarseToFineParams>()?;
    m.add_class::<PyPyramidParams>()?;
    m.add_class::<PyOrientationClusteringParams>()?;
    m.add_class::<PyGridGraphParams>()?;
    m.add_class::<PyChessboardParams>()?;
    m.add_class::<PyScanDecodeConfig>()?;
    m.add_class::<PyCharucoDetectorParams>()?;
    m.add_class::<PyCircleScoreParams>()?;
    m.add_class::<PyCircleMatchParams>()?;
    m.add_class::<PyMarkerBoardParams>()?;
    m.add_function(wrap_pyfunction_bound!(detect_charuco, m)?)?;
    m.add_function(wrap_pyfunction_bound!(detect_chessboard, m)?)?;
    m.add_function(wrap_pyfunction_bound!(detect_marker_board, m)?)?;
    Ok(())
}
