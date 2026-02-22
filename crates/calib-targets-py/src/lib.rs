use ::calib_targets::{aruco, charuco, chessboard, core, detect, marker};
use chess_corners::{ChessConfig, ChessParams, CoarseToFineParams, PyramidParams};
use numpy::{PyArrayDyn, PyArrayMethods, PyUntypedArrayMethods};
use pyo3::conversion::IntoPyObjectExt;
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyDict, PyList, PyString, PyTuple};
use pyo3::PyRef;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{Map, Number, Value};

#[derive(Debug)]
enum ChessCornerParamsSource {
    Owned(ChessParams),
    ChessConfig(Py<PyChessConfig>),
}

#[pyclass(name = "ChessCornerParams", module = "calib_targets._core")]
#[derive(Debug)]
/// Parameters for the ChESS corner detector.
struct PyChessCornerParams {
    inner: ChessCornerParamsSource,
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
        Ok(Self {
            inner: ChessCornerParamsSource::Owned(params),
        })
    }

    #[getter]
    fn use_radius10(&self) -> PyResult<bool> {
        self.with_params(|params| params.use_radius10)
    }

    #[setter]
    fn set_use_radius10(&mut self, value: bool) -> PyResult<()> {
        self.with_params_mut(|params| {
            params.use_radius10 = value;
        })?;
        Ok(())
    }

    #[getter]
    fn descriptor_use_radius10(&self) -> PyResult<Option<bool>> {
        self.with_params(|params| params.descriptor_use_radius10)
    }

    #[setter]
    fn set_descriptor_use_radius10(&mut self, value: Option<bool>) -> PyResult<()> {
        self.with_params_mut(|params| {
            params.descriptor_use_radius10 = value;
        })?;
        Ok(())
    }

    #[getter]
    fn threshold_rel(&self) -> PyResult<f32> {
        self.with_params(|params| params.threshold_rel)
    }

    #[setter]
    fn set_threshold_rel(&mut self, value: f32) -> PyResult<()> {
        self.with_params_mut(|params| {
            params.threshold_rel = value;
        })?;
        Ok(())
    }

    #[getter]
    fn threshold_abs(&self) -> PyResult<Option<f32>> {
        self.with_params(|params| params.threshold_abs)
    }

    #[setter]
    fn set_threshold_abs(&mut self, value: Option<f32>) -> PyResult<()> {
        self.with_params_mut(|params| {
            params.threshold_abs = value;
        })?;
        Ok(())
    }

    #[getter]
    fn nms_radius(&self) -> PyResult<u32> {
        self.with_params(|params| params.nms_radius)
    }

    #[setter]
    fn set_nms_radius(&mut self, value: u32) -> PyResult<()> {
        self.with_params_mut(|params| {
            params.nms_radius = value;
        })?;
        Ok(())
    }

    #[getter]
    fn min_cluster_size(&self) -> PyResult<u32> {
        self.with_params(|params| params.min_cluster_size)
    }

    #[setter]
    fn set_min_cluster_size(&mut self, value: u32) -> PyResult<()> {
        self.with_params_mut(|params| {
            params.min_cluster_size = value;
        })?;
        Ok(())
    }
}

impl PyChessCornerParams {
    fn with_params<R>(&self, f: impl FnOnce(&ChessParams) -> R) -> PyResult<R> {
        match &self.inner {
            ChessCornerParamsSource::Owned(params) => Ok(f(params)),
            ChessCornerParamsSource::ChessConfig(parent) => Python::attach(|py| {
                let parent = parent.bind(py).borrow();
                Ok(f(&parent.inner.params))
            }),
        }
    }

    fn with_params_mut<R>(&mut self, f: impl FnOnce(&mut ChessParams) -> R) -> PyResult<R> {
        match &mut self.inner {
            ChessCornerParamsSource::Owned(params) => Ok(f(params)),
            ChessCornerParamsSource::ChessConfig(parent) => Python::attach(|py| {
                let mut parent = parent.bind(py).borrow_mut();
                Ok(f(&mut parent.inner.params))
            }),
        }
    }

    fn to_params(&self) -> PyResult<ChessParams> {
        self.with_params(|params| params.clone())
    }
}

#[derive(Debug)]
enum PyramidParamsSource {
    Owned(PyramidParams),
    CoarseToFine(Py<PyCoarseToFineParams>),
}

#[pyclass(name = "PyramidParams", module = "calib_targets._core")]
#[derive(Debug)]
/// Parameters for image pyramid generation.
struct PyPyramidParams {
    inner: PyramidParamsSource,
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
        Ok(Self {
            inner: PyramidParamsSource::Owned(params),
        })
    }

    #[getter]
    fn num_levels(&self) -> PyResult<u8> {
        self.with_params(|params| params.num_levels)
    }

    #[setter]
    fn set_num_levels(&mut self, value: u8) -> PyResult<()> {
        self.with_params_mut(|params| {
            params.num_levels = value;
        })?;
        Ok(())
    }

    #[getter]
    fn min_size(&self) -> PyResult<usize> {
        self.with_params(|params| params.min_size)
    }

    #[setter]
    fn set_min_size(&mut self, value: usize) -> PyResult<()> {
        self.with_params_mut(|params| {
            params.min_size = value;
        })?;
        Ok(())
    }
}

impl PyPyramidParams {
    fn with_params<R>(&self, f: impl FnOnce(&PyramidParams) -> R) -> PyResult<R> {
        match &self.inner {
            PyramidParamsSource::Owned(params) => Ok(f(params)),
            PyramidParamsSource::CoarseToFine(parent) => Python::attach(|py| {
                let parent = parent.bind(py).borrow();
                parent.with_params(|params| f(&params.pyramid))
            }),
        }
    }

    fn with_params_mut<R>(&mut self, f: impl FnOnce(&mut PyramidParams) -> R) -> PyResult<R> {
        match &mut self.inner {
            PyramidParamsSource::Owned(params) => Ok(f(params)),
            PyramidParamsSource::CoarseToFine(parent) => Python::attach(|py| {
                let mut parent = parent.bind(py).borrow_mut();
                parent.with_params_mut(|params| f(&mut params.pyramid))
            }),
        }
    }

    fn to_params(&self) -> PyResult<PyramidParams> {
        self.with_params(|params| params.clone())
    }
}

#[derive(Debug)]
enum CoarseToFineParamsSource {
    Owned(CoarseToFineParams),
    ChessConfig(Py<PyChessConfig>),
}

#[pyclass(name = "CoarseToFineParams", module = "calib_targets._core")]
#[derive(Debug)]
/// Coarse-to-fine multiscale detector parameters.
struct PyCoarseToFineParams {
    inner: CoarseToFineParamsSource,
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
        Ok(Self {
            inner: CoarseToFineParamsSource::Owned(params),
        })
    }

    #[getter]
    fn pyramid(slf: PyRef<'_, Self>) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        let parent = slf.into_pyobject(py)?.unbind();
        let params = PyPyramidParams {
            inner: PyramidParamsSource::CoarseToFine(parent),
        };
        Py::new(py, params).map(|obj| obj.into_any())
    }

    #[setter]
    fn set_pyramid(&mut self, value: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        let Some(value) = value else {
            self.with_params_mut(|params| {
                params.pyramid = PyramidParams::default();
            })?;
            return Ok(());
        };
        let pyramid = self.with_params(|params| params.pyramid.clone())?;
        let pyramid = pyramid_params_from_obj(value, "pyramid", pyramid)?;
        self.with_params_mut(|params| {
            params.pyramid = pyramid;
        })?;
        Ok(())
    }

    #[getter]
    fn refinement_radius(&self) -> PyResult<u32> {
        self.with_params(|params| params.refinement_radius)
    }

    #[setter]
    fn set_refinement_radius(&mut self, value: u32) -> PyResult<()> {
        self.with_params_mut(|params| {
            params.refinement_radius = value;
        })?;
        Ok(())
    }

    #[getter]
    fn merge_radius(&self) -> PyResult<f32> {
        self.with_params(|params| params.merge_radius)
    }

    #[setter]
    fn set_merge_radius(&mut self, value: f32) -> PyResult<()> {
        self.with_params_mut(|params| {
            params.merge_radius = value;
        })?;
        Ok(())
    }
}

impl PyCoarseToFineParams {
    fn with_params<R>(&self, f: impl FnOnce(&CoarseToFineParams) -> R) -> PyResult<R> {
        match &self.inner {
            CoarseToFineParamsSource::Owned(params) => Ok(f(params)),
            CoarseToFineParamsSource::ChessConfig(parent) => Python::attach(|py| {
                let parent = parent.bind(py).borrow();
                Ok(f(&parent.inner.multiscale))
            }),
        }
    }

    fn with_params_mut<R>(&mut self, f: impl FnOnce(&mut CoarseToFineParams) -> R) -> PyResult<R> {
        match &mut self.inner {
            CoarseToFineParamsSource::Owned(params) => Ok(f(params)),
            CoarseToFineParamsSource::ChessConfig(parent) => Python::attach(|py| {
                let mut parent = parent.bind(py).borrow_mut();
                Ok(f(&mut parent.inner.multiscale))
            }),
        }
    }

    fn to_params(&self) -> PyResult<CoarseToFineParams> {
        self.with_params(|params| params.clone())
    }
}

#[pyclass(name = "ChessConfig", module = "calib_targets._core")]
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

    #[getter]
    fn params(slf: PyRef<'_, Self>) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        let parent = slf.into_pyobject(py)?.unbind();
        let params = PyChessCornerParams {
            inner: ChessCornerParamsSource::ChessConfig(parent),
        };
        Py::new(py, params).map(|obj| obj.into_any())
    }

    #[setter]
    fn set_params(&mut self, value: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        let Some(value) = value else {
            self.inner.params = ChessParams::default();
            return Ok(());
        };
        self.inner.params = chess_params_from_obj(value, "params", self.inner.params.clone())?;
        Ok(())
    }

    #[getter]
    fn multiscale(slf: PyRef<'_, Self>) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        let parent = slf.into_pyobject(py)?.unbind();
        let params = PyCoarseToFineParams {
            inner: CoarseToFineParamsSource::ChessConfig(parent),
        };
        Py::new(py, params).map(|obj| obj.into_any())
    }

    #[setter]
    fn set_multiscale(&mut self, value: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        let Some(value) = value else {
            self.inner.multiscale = CoarseToFineParams::default();
            return Ok(());
        };
        self.inner.multiscale =
            coarse_to_fine_params_from_obj(value, "multiscale", self.inner.multiscale.clone())?;
        Ok(())
    }

    /// Return a JSON-like dict for debugging.
    fn to_dict(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let json = chess_config_to_json(&self.inner);
        json_to_py(py, &json)
    }
}

#[derive(Debug)]
enum OrientationClusteringParamsSource {
    Owned(core::OrientationClusteringParams),
    Chessboard(Py<PyChessboardParams>),
}

#[pyclass(name = "OrientationClusteringParams", module = "calib_targets._core")]
#[derive(Debug)]
/// Orientation clustering parameters for chessboard detection.
struct PyOrientationClusteringParams {
    inner: OrientationClusteringParamsSource,
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
        Ok(Self {
            inner: OrientationClusteringParamsSource::Owned(params),
        })
    }

    #[getter]
    fn num_bins(&self) -> PyResult<usize> {
        self.with_params(|params| params.num_bins)
    }

    #[setter]
    fn set_num_bins(&mut self, value: usize) -> PyResult<()> {
        self.with_params_mut(|params| {
            params.num_bins = value;
        })?;
        Ok(())
    }

    #[getter]
    fn max_iters(&self) -> PyResult<usize> {
        self.with_params(|params| params.max_iters)
    }

    #[setter]
    fn set_max_iters(&mut self, value: usize) -> PyResult<()> {
        self.with_params_mut(|params| {
            params.max_iters = value;
        })?;
        Ok(())
    }

    #[getter]
    fn peak_min_separation_deg(&self) -> PyResult<f32> {
        self.with_params(|params| params.peak_min_separation_deg)
    }

    #[setter]
    fn set_peak_min_separation_deg(&mut self, value: f32) -> PyResult<()> {
        self.with_params_mut(|params| {
            params.peak_min_separation_deg = value;
        })?;
        Ok(())
    }

    #[getter]
    fn outlier_threshold_deg(&self) -> PyResult<f32> {
        self.with_params(|params| params.outlier_threshold_deg)
    }

    #[setter]
    fn set_outlier_threshold_deg(&mut self, value: f32) -> PyResult<()> {
        self.with_params_mut(|params| {
            params.outlier_threshold_deg = value;
        })?;
        Ok(())
    }

    #[getter]
    fn min_peak_weight_fraction(&self) -> PyResult<f32> {
        self.with_params(|params| params.min_peak_weight_fraction)
    }

    #[setter]
    fn set_min_peak_weight_fraction(&mut self, value: f32) -> PyResult<()> {
        self.with_params_mut(|params| {
            params.min_peak_weight_fraction = value;
        })?;
        Ok(())
    }

    #[getter]
    fn use_weights(&self) -> PyResult<bool> {
        self.with_params(|params| params.use_weights)
    }

    #[setter]
    fn set_use_weights(&mut self, value: bool) -> PyResult<()> {
        self.with_params_mut(|params| {
            params.use_weights = value;
        })?;
        Ok(())
    }
}

impl PyOrientationClusteringParams {
    fn with_params<R>(
        &self,
        f: impl FnOnce(&core::OrientationClusteringParams) -> R,
    ) -> PyResult<R> {
        match &self.inner {
            OrientationClusteringParamsSource::Owned(params) => Ok(f(params)),
            OrientationClusteringParamsSource::Chessboard(parent) => Python::attach(|py| {
                let parent = parent.bind(py).borrow();
                parent.with_params(|params| f(&params.orientation_clustering_params))
            }),
        }
    }

    fn with_params_mut<R>(
        &mut self,
        f: impl FnOnce(&mut core::OrientationClusteringParams) -> R,
    ) -> PyResult<R> {
        match &mut self.inner {
            OrientationClusteringParamsSource::Owned(params) => Ok(f(params)),
            OrientationClusteringParamsSource::Chessboard(parent) => Python::attach(|py| {
                let mut parent = parent.bind(py).borrow_mut();
                parent.with_params_mut(|params| f(&mut params.orientation_clustering_params))
            }),
        }
    }

    fn to_params(&self) -> PyResult<core::OrientationClusteringParams> {
        self.with_params(|params| params.clone())
    }
}

#[derive(Debug)]
enum GridGraphParamsSource {
    Owned(chessboard::GridGraphParams),
    Charuco(Py<PyCharucoDetectorParams>),
    MarkerBoard(Py<PyMarkerBoardParams>),
}

#[pyclass(name = "GridGraphParams", module = "calib_targets._core")]
#[derive(Debug)]
/// Parameters for grid graph construction in chessboard detection.
struct PyGridGraphParams {
    inner: GridGraphParamsSource,
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
        Ok(Self {
            inner: GridGraphParamsSource::Owned(params),
        })
    }

    #[getter]
    fn min_spacing_pix(&self) -> PyResult<f32> {
        self.with_params(|params| params.min_spacing_pix)
    }

    #[setter]
    fn set_min_spacing_pix(&mut self, value: f32) -> PyResult<()> {
        self.with_params_mut(|params| {
            params.min_spacing_pix = value;
        })?;
        Ok(())
    }

    #[getter]
    fn max_spacing_pix(&self) -> PyResult<f32> {
        self.with_params(|params| params.max_spacing_pix)
    }

    #[setter]
    fn set_max_spacing_pix(&mut self, value: f32) -> PyResult<()> {
        self.with_params_mut(|params| {
            params.max_spacing_pix = value;
        })?;
        Ok(())
    }

    #[getter]
    fn k_neighbors(&self) -> PyResult<usize> {
        self.with_params(|params| params.k_neighbors)
    }

    #[setter]
    fn set_k_neighbors(&mut self, value: usize) -> PyResult<()> {
        self.with_params_mut(|params| {
            params.k_neighbors = value;
        })?;
        Ok(())
    }

    #[getter]
    fn orientation_tolerance_deg(&self) -> PyResult<f32> {
        self.with_params(|params| params.orientation_tolerance_deg)
    }

    #[setter]
    fn set_orientation_tolerance_deg(&mut self, value: f32) -> PyResult<()> {
        self.with_params_mut(|params| {
            params.orientation_tolerance_deg = value;
        })?;
        Ok(())
    }
}

impl PyGridGraphParams {
    fn with_params<R>(&self, f: impl FnOnce(&chessboard::GridGraphParams) -> R) -> PyResult<R> {
        match &self.inner {
            GridGraphParamsSource::Owned(params) => Ok(f(params)),
            GridGraphParamsSource::Charuco(parent) => Python::attach(|py| {
                let parent = parent.bind(py).borrow();
                Ok(f(&parent.inner.graph))
            }),
            GridGraphParamsSource::MarkerBoard(parent) => Python::attach(|py| {
                let parent = parent.bind(py).borrow();
                Ok(f(&parent.inner.grid_graph))
            }),
        }
    }

    fn with_params_mut<R>(
        &mut self,
        f: impl FnOnce(&mut chessboard::GridGraphParams) -> R,
    ) -> PyResult<R> {
        match &mut self.inner {
            GridGraphParamsSource::Owned(params) => Ok(f(params)),
            GridGraphParamsSource::Charuco(parent) => Python::attach(|py| {
                let mut parent = parent.bind(py).borrow_mut();
                Ok(f(&mut parent.inner.graph))
            }),
            GridGraphParamsSource::MarkerBoard(parent) => Python::attach(|py| {
                let mut parent = parent.bind(py).borrow_mut();
                Ok(f(&mut parent.inner.grid_graph))
            }),
        }
    }

    fn to_params(&self) -> PyResult<chessboard::GridGraphParams> {
        self.with_params(|params| params.clone())
    }
}

#[derive(Debug)]
enum ChessboardParamsSource {
    Owned(chessboard::ChessboardParams),
    Charuco(Py<PyCharucoDetectorParams>),
    MarkerBoard(Py<PyMarkerBoardParams>),
}

#[pyclass(name = "ChessboardParams", module = "calib_targets._core")]
#[derive(Debug)]
/// Parameters for chessboard detection from ChESS corners.
struct PyChessboardParams {
    inner: ChessboardParamsSource,
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
        Ok(Self {
            inner: ChessboardParamsSource::Owned(params),
        })
    }

    #[getter]
    fn min_corner_strength(&self) -> PyResult<f32> {
        self.with_params(|params| params.min_corner_strength)
    }

    #[setter]
    fn set_min_corner_strength(&mut self, value: f32) -> PyResult<()> {
        self.with_params_mut(|params| {
            params.min_corner_strength = value;
        })?;
        Ok(())
    }

    #[getter]
    fn min_corners(&self) -> PyResult<usize> {
        self.with_params(|params| params.min_corners)
    }

    #[setter]
    fn set_min_corners(&mut self, value: usize) -> PyResult<()> {
        self.with_params_mut(|params| {
            params.min_corners = value;
        })?;
        Ok(())
    }

    #[getter]
    fn expected_rows(&self) -> PyResult<Option<u32>> {
        self.with_params(|params| params.expected_rows)
    }

    #[setter]
    fn set_expected_rows(&mut self, value: Option<u32>) -> PyResult<()> {
        self.with_params_mut(|params| {
            params.expected_rows = value;
        })?;
        Ok(())
    }

    #[getter]
    fn expected_cols(&self) -> PyResult<Option<u32>> {
        self.with_params(|params| params.expected_cols)
    }

    #[setter]
    fn set_expected_cols(&mut self, value: Option<u32>) -> PyResult<()> {
        self.with_params_mut(|params| {
            params.expected_cols = value;
        })?;
        Ok(())
    }

    #[getter]
    fn completeness_threshold(&self) -> PyResult<f32> {
        self.with_params(|params| params.completeness_threshold)
    }

    #[setter]
    fn set_completeness_threshold(&mut self, value: f32) -> PyResult<()> {
        self.with_params_mut(|params| {
            params.completeness_threshold = value;
        })?;
        Ok(())
    }

    #[getter]
    fn use_orientation_clustering(&self) -> PyResult<bool> {
        self.with_params(|params| params.use_orientation_clustering)
    }

    #[setter]
    fn set_use_orientation_clustering(&mut self, value: bool) -> PyResult<()> {
        self.with_params_mut(|params| {
            params.use_orientation_clustering = value;
        })?;
        Ok(())
    }

    #[getter]
    fn orientation_clustering_params(slf: PyRef<'_, Self>) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        let parent = slf.into_pyobject(py)?.unbind();
        let params = PyOrientationClusteringParams {
            inner: OrientationClusteringParamsSource::Chessboard(parent),
        };
        Py::new(py, params).map(|obj| obj.into_any())
    }

    #[setter]
    fn set_orientation_clustering_params(
        &mut self,
        value: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<()> {
        let Some(value) = value else {
            self.with_params_mut(|params| {
                params.orientation_clustering_params = core::OrientationClusteringParams::default();
            })?;
            return Ok(());
        };
        let base = self.with_params(|params| params.orientation_clustering_params.clone())?;
        let updated =
            orientation_clustering_params_from_obj(value, "orientation_clustering_params", base)?;
        self.with_params_mut(|params| {
            params.orientation_clustering_params = updated;
        })?;
        Ok(())
    }
}

impl PyChessboardParams {
    fn with_params<R>(&self, f: impl FnOnce(&chessboard::ChessboardParams) -> R) -> PyResult<R> {
        match &self.inner {
            ChessboardParamsSource::Owned(params) => Ok(f(params)),
            ChessboardParamsSource::Charuco(parent) => Python::attach(|py| {
                let parent = parent.bind(py).borrow();
                Ok(f(&parent.inner.chessboard))
            }),
            ChessboardParamsSource::MarkerBoard(parent) => Python::attach(|py| {
                let parent = parent.bind(py).borrow();
                Ok(f(&parent.inner.chessboard))
            }),
        }
    }

    fn with_params_mut<R>(
        &mut self,
        f: impl FnOnce(&mut chessboard::ChessboardParams) -> R,
    ) -> PyResult<R> {
        match &mut self.inner {
            ChessboardParamsSource::Owned(params) => Ok(f(params)),
            ChessboardParamsSource::Charuco(parent) => Python::attach(|py| {
                let mut parent = parent.bind(py).borrow_mut();
                Ok(f(&mut parent.inner.chessboard))
            }),
            ChessboardParamsSource::MarkerBoard(parent) => Python::attach(|py| {
                let mut parent = parent.bind(py).borrow_mut();
                Ok(f(&mut parent.inner.chessboard))
            }),
        }
    }

    fn to_params(&self) -> PyResult<chessboard::ChessboardParams> {
        self.with_params(|params| params.clone())
    }
}

#[derive(Debug)]
enum ScanDecodeConfigSource {
    Owned(aruco::ScanDecodeConfig),
    Charuco(Py<PyCharucoDetectorParams>),
}

#[pyclass(name = "ScanDecodeConfig", module = "calib_targets._core")]
#[derive(Debug)]
/// Marker scan/decoder configuration.
struct PyScanDecodeConfig {
    inner: ScanDecodeConfigSource,
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
        Ok(Self {
            inner: ScanDecodeConfigSource::Owned(params),
        })
    }

    #[getter]
    fn border_bits(&self) -> PyResult<usize> {
        self.with_params(|params| params.border_bits)
    }

    #[setter]
    fn set_border_bits(&mut self, value: usize) -> PyResult<()> {
        self.with_params_mut(|params| {
            params.border_bits = value;
        })?;
        Ok(())
    }

    #[getter]
    fn inset_frac(&self) -> PyResult<f32> {
        self.with_params(|params| params.inset_frac)
    }

    #[setter]
    fn set_inset_frac(&mut self, value: f32) -> PyResult<()> {
        self.with_params_mut(|params| {
            params.inset_frac = value;
        })?;
        Ok(())
    }

    #[getter]
    fn marker_size_rel(&self) -> PyResult<f32> {
        self.with_params(|params| params.marker_size_rel)
    }

    #[setter]
    fn set_marker_size_rel(&mut self, value: f32) -> PyResult<()> {
        self.with_params_mut(|params| {
            params.marker_size_rel = value;
        })?;
        Ok(())
    }

    #[getter]
    fn min_border_score(&self) -> PyResult<f32> {
        self.with_params(|params| params.min_border_score)
    }

    #[setter]
    fn set_min_border_score(&mut self, value: f32) -> PyResult<()> {
        self.with_params_mut(|params| {
            params.min_border_score = value;
        })?;
        Ok(())
    }

    #[getter]
    fn dedup_by_id(&self) -> PyResult<bool> {
        self.with_params(|params| params.dedup_by_id)
    }

    #[setter]
    fn set_dedup_by_id(&mut self, value: bool) -> PyResult<()> {
        self.with_params_mut(|params| {
            params.dedup_by_id = value;
        })?;
        Ok(())
    }
}

impl PyScanDecodeConfig {
    fn with_params<R>(&self, f: impl FnOnce(&aruco::ScanDecodeConfig) -> R) -> PyResult<R> {
        match &self.inner {
            ScanDecodeConfigSource::Owned(params) => Ok(f(params)),
            ScanDecodeConfigSource::Charuco(parent) => Python::attach(|py| {
                let parent = parent.bind(py).borrow();
                Ok(f(&parent.inner.scan))
            }),
        }
    }

    fn with_params_mut<R>(
        &mut self,
        f: impl FnOnce(&mut aruco::ScanDecodeConfig) -> R,
    ) -> PyResult<R> {
        match &mut self.inner {
            ScanDecodeConfigSource::Owned(params) => Ok(f(params)),
            ScanDecodeConfigSource::Charuco(parent) => Python::attach(|py| {
                let mut parent = parent.bind(py).borrow_mut();
                Ok(f(&mut parent.inner.scan))
            }),
        }
    }

    fn to_params(&self) -> PyResult<aruco::ScanDecodeConfig> {
        self.with_params(|params| params.clone())
    }
}

#[derive(Debug)]
enum CharucoBoardSpecSource {
    Owned(charuco::CharucoBoardSpec),
    Charuco(Py<PyCharucoDetectorParams>),
}

#[pyclass(name = "CharucoBoardSpec", module = "calib_targets._core")]
#[derive(Debug)]
/// ChArUco board specification (square counts + dictionary).
struct PyCharucoBoardSpec {
    inner: CharucoBoardSpecSource,
}

#[pymethods]
impl PyCharucoBoardSpec {
    #[new]
    #[pyo3(signature = (*, rows, cols, cell_size, marker_size_rel, dictionary, marker_layout=None))]
    fn new(
        rows: u32,
        cols: u32,
        cell_size: f32,
        marker_size_rel: f32,
        dictionary: &Bound<'_, PyAny>,
        marker_layout: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        let dictionary = parse_required::<aruco::Dictionary>(dictionary, "dictionary")?;
        let marker_layout = match marker_layout {
            Some(value) => parse_required::<charuco::MarkerLayout>(value, "marker_layout")?,
            None => charuco::MarkerLayout::default(),
        };
        Ok(Self {
            inner: CharucoBoardSpecSource::Owned(charuco::CharucoBoardSpec {
                rows,
                cols,
                cell_size,
                marker_size_rel,
                dictionary,
                marker_layout,
            }),
        })
    }

    #[getter]
    fn rows(&self) -> PyResult<u32> {
        self.with_spec(|spec| spec.rows)
    }

    #[setter]
    fn set_rows(&mut self, value: u32) -> PyResult<()> {
        self.with_spec_mut(|spec| {
            spec.rows = value;
        })?;
        Ok(())
    }

    #[getter]
    fn cols(&self) -> PyResult<u32> {
        self.with_spec(|spec| spec.cols)
    }

    #[setter]
    fn set_cols(&mut self, value: u32) -> PyResult<()> {
        self.with_spec_mut(|spec| {
            spec.cols = value;
        })?;
        Ok(())
    }

    #[getter]
    fn cell_size(&self) -> PyResult<f32> {
        self.with_spec(|spec| spec.cell_size)
    }

    #[setter]
    fn set_cell_size(&mut self, value: f32) -> PyResult<()> {
        self.with_spec_mut(|spec| {
            spec.cell_size = value;
        })?;
        Ok(())
    }

    #[getter]
    fn marker_size_rel(&self) -> PyResult<f32> {
        self.with_spec(|spec| spec.marker_size_rel)
    }

    #[setter]
    fn set_marker_size_rel(&mut self, value: f32) -> PyResult<()> {
        self.with_spec_mut(|spec| {
            spec.marker_size_rel = value;
        })?;
        Ok(())
    }

    #[getter]
    fn dictionary(&self) -> PyResult<String> {
        self.with_spec(|spec| spec.dictionary.name.to_string())
    }

    #[setter]
    fn set_dictionary(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let dictionary = parse_required::<aruco::Dictionary>(value, "dictionary")?;
        self.with_spec_mut(|spec| {
            spec.dictionary = dictionary;
        })?;
        Ok(())
    }

    #[getter]
    fn marker_layout(&self) -> PyResult<&'static str> {
        self.with_spec(|spec| marker_layout_name(spec.marker_layout))
    }

    #[setter]
    fn set_marker_layout(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let layout = parse_required::<charuco::MarkerLayout>(value, "marker_layout")?;
        self.with_spec_mut(|spec| {
            spec.marker_layout = layout;
        })?;
        Ok(())
    }
}

impl PyCharucoBoardSpec {
    fn with_spec<R>(&self, f: impl FnOnce(&charuco::CharucoBoardSpec) -> R) -> PyResult<R> {
        match &self.inner {
            CharucoBoardSpecSource::Owned(spec) => Ok(f(spec)),
            CharucoBoardSpecSource::Charuco(parent) => Python::attach(|py| {
                let parent = parent.bind(py).borrow();
                Ok(f(&parent.inner.charuco))
            }),
        }
    }

    fn with_spec_mut<R>(
        &mut self,
        f: impl FnOnce(&mut charuco::CharucoBoardSpec) -> R,
    ) -> PyResult<R> {
        match &mut self.inner {
            CharucoBoardSpecSource::Owned(spec) => Ok(f(spec)),
            CharucoBoardSpecSource::Charuco(parent) => Python::attach(|py| {
                let mut parent = parent.bind(py).borrow_mut();
                let out = f(&mut parent.inner.charuco);
                update_charuco_params_for_board(&mut parent.inner);
                Ok(out)
            }),
        }
    }

    fn to_spec(&self) -> PyResult<charuco::CharucoBoardSpec> {
        self.with_spec(|spec| *spec)
    }
}

fn update_charuco_params_for_board(params: &mut charuco::CharucoDetectorParams) {
    let board = params.charuco;
    if params.chessboard.expected_rows.is_none() {
        params.chessboard.expected_rows = board.rows.checked_sub(1);
    }
    if params.chessboard.expected_cols.is_none() {
        params.chessboard.expected_cols = board.cols.checked_sub(1);
    }
    if !params.scan.marker_size_rel.is_finite() || params.scan.marker_size_rel <= 0.0 {
        params.scan.marker_size_rel = board.marker_size_rel;
    }
    params.max_hamming = params.max_hamming.min(board.dictionary.max_correction_bits);
}

#[pyclass(name = "CharucoDetectorParams", module = "calib_targets._core")]
#[derive(Clone, Debug)]
/// Full ChArUco detector configuration (board + parameters).
struct PyCharucoDetectorParams {
    inner: charuco::CharucoDetectorParams,
}

#[pymethods]
impl PyCharucoDetectorParams {
    #[new]
    #[pyo3(signature = (board, *, px_per_square=None, chessboard=None, graph=None, scan=None, max_hamming=None, min_marker_inliers=None))]
    fn new(
        board: &Bound<'_, PyAny>,
        px_per_square: Option<f32>,
        chessboard: Option<&Bound<'_, PyAny>>,
        graph: Option<&Bound<'_, PyAny>>,
        scan: Option<&Bound<'_, PyAny>>,
        max_hamming: Option<u8>,
        min_marker_inliers: Option<usize>,
    ) -> PyResult<Self> {
        let board = charuco_board_from_obj(board, "board")?;
        let mut params = charuco::CharucoDetectorParams::for_board(&board);
        if let Some(px_per_square) = px_per_square {
            params.px_per_square = px_per_square;
        }
        if let Some(chessboard) = chessboard {
            params.chessboard =
                chessboard_params_from_obj(chessboard, "chessboard", params.chessboard.clone())?;
        }
        if let Some(graph) = graph {
            params.graph = grid_graph_params_from_obj(graph, "graph", params.graph.clone())?;
        }
        if let Some(scan) = scan {
            params.scan = scan_decode_params_from_obj(scan, "scan", params.scan.clone())?;
        }
        if let Some(max_hamming) = max_hamming {
            params.max_hamming = max_hamming;
        }
        if let Some(min_marker_inliers) = min_marker_inliers {
            params.min_marker_inliers = min_marker_inliers;
        }
        Ok(Self { inner: params })
    }

    #[getter]
    fn board(slf: PyRef<'_, Self>) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        let parent = slf.into_pyobject(py)?.unbind();
        let board = PyCharucoBoardSpec {
            inner: CharucoBoardSpecSource::Charuco(parent),
        };
        Py::new(py, board).map(|obj| obj.into_any())
    }

    #[setter]
    fn set_board(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let board = charuco_board_from_obj(value, "board")?;
        self.inner.charuco = board;
        update_charuco_params_for_board(&mut self.inner);
        Ok(())
    }

    #[getter]
    fn px_per_square(&self) -> f32 {
        self.inner.px_per_square
    }

    #[setter]
    fn set_px_per_square(&mut self, value: f32) {
        self.inner.px_per_square = value;
    }

    #[getter]
    fn chessboard(slf: PyRef<'_, Self>) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        let parent = slf.into_pyobject(py)?.unbind();
        let params = PyChessboardParams {
            inner: ChessboardParamsSource::Charuco(parent),
        };
        Py::new(py, params).map(|obj| obj.into_any())
    }

    #[setter]
    fn set_chessboard(&mut self, value: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        let Some(value) = value else {
            self.inner.chessboard = chessboard::ChessboardParams::default();
            return Ok(());
        };
        self.inner.chessboard =
            chessboard_params_from_obj(value, "chessboard", self.inner.chessboard.clone())?;
        Ok(())
    }

    #[getter]
    fn graph(slf: PyRef<'_, Self>) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        let parent = slf.into_pyobject(py)?.unbind();
        let params = PyGridGraphParams {
            inner: GridGraphParamsSource::Charuco(parent),
        };
        Py::new(py, params).map(|obj| obj.into_any())
    }

    #[setter]
    fn set_graph(&mut self, value: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        let Some(value) = value else {
            self.inner.graph = chessboard::GridGraphParams::default();
            return Ok(());
        };
        self.inner.graph = grid_graph_params_from_obj(value, "graph", self.inner.graph.clone())?;
        Ok(())
    }

    #[getter]
    fn scan(slf: PyRef<'_, Self>) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        let parent = slf.into_pyobject(py)?.unbind();
        let params = PyScanDecodeConfig {
            inner: ScanDecodeConfigSource::Charuco(parent),
        };
        Py::new(py, params).map(|obj| obj.into_any())
    }

    #[setter]
    fn set_scan(&mut self, value: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        let Some(value) = value else {
            self.inner.scan = aruco::ScanDecodeConfig::default();
            return Ok(());
        };
        self.inner.scan = scan_decode_params_from_obj(value, "scan", self.inner.scan.clone())?;
        Ok(())
    }

    #[getter]
    fn max_hamming(&self) -> u8 {
        self.inner.max_hamming
    }

    #[setter]
    fn set_max_hamming(&mut self, value: u8) {
        self.inner.max_hamming = value;
    }

    #[getter]
    fn min_marker_inliers(&self) -> usize {
        self.inner.min_marker_inliers
    }

    #[setter]
    fn set_min_marker_inliers(&mut self, value: usize) {
        self.inner.min_marker_inliers = value;
    }
}

#[derive(Debug)]
enum MarkerCircleSpecSource {
    Owned(marker::MarkerCircleSpec),
    Layout {
        layout: Py<PyMarkerBoardLayout>,
        index: usize,
    },
}

#[pyclass(name = "MarkerCircleSpec", module = "calib_targets._core")]
#[derive(Debug)]
/// One expected circle marker (cell + polarity).
struct PyMarkerCircleSpec {
    inner: MarkerCircleSpecSource,
}

#[pymethods]
impl PyMarkerCircleSpec {
    #[new]
    #[pyo3(signature = (*, i, j, polarity))]
    fn new(i: i32, j: i32, polarity: &Bound<'_, PyAny>) -> PyResult<Self> {
        let polarity = parse_required::<marker::CirclePolarity>(polarity, "polarity")?;
        Ok(Self {
            inner: MarkerCircleSpecSource::Owned(marker::MarkerCircleSpec {
                cell: marker::CellCoords { i, j },
                polarity,
            }),
        })
    }

    #[getter]
    fn i(&self) -> PyResult<i32> {
        self.with_spec(|spec| spec.cell.i)
    }

    #[setter]
    fn set_i(&mut self, value: i32) -> PyResult<()> {
        self.with_spec_mut(|spec| {
            spec.cell.i = value;
        })?;
        Ok(())
    }

    #[getter]
    fn j(&self) -> PyResult<i32> {
        self.with_spec(|spec| spec.cell.j)
    }

    #[setter]
    fn set_j(&mut self, value: i32) -> PyResult<()> {
        self.with_spec_mut(|spec| {
            spec.cell.j = value;
        })?;
        Ok(())
    }

    #[getter]
    fn polarity(&self) -> PyResult<&'static str> {
        self.with_spec(|spec| circle_polarity_name(spec.polarity))
    }

    #[setter]
    fn set_polarity(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let polarity = parse_required::<marker::CirclePolarity>(value, "polarity")?;
        self.with_spec_mut(|spec| {
            spec.polarity = polarity;
        })?;
        Ok(())
    }
}

impl PyMarkerCircleSpec {
    fn with_spec<R>(&self, f: impl FnOnce(&marker::MarkerCircleSpec) -> R) -> PyResult<R> {
        match &self.inner {
            MarkerCircleSpecSource::Owned(spec) => Ok(f(spec)),
            MarkerCircleSpecSource::Layout { layout, index } => Python::attach(|py| {
                let layout = layout.bind(py).borrow();
                layout.with_layout(|layout| f(&layout.circles[*index]))
            }),
        }
    }

    fn with_spec_mut<R>(
        &mut self,
        f: impl FnOnce(&mut marker::MarkerCircleSpec) -> R,
    ) -> PyResult<R> {
        match &mut self.inner {
            MarkerCircleSpecSource::Owned(spec) => Ok(f(spec)),
            MarkerCircleSpecSource::Layout { layout, index } => Python::attach(|py| {
                let mut layout = layout.bind(py).borrow_mut();
                layout.with_layout_mut(|layout| f(&mut layout.circles[*index]))
            }),
        }
    }

    fn to_spec(&self) -> PyResult<marker::MarkerCircleSpec> {
        self.with_spec(|spec| *spec)
    }
}

#[derive(Debug)]
enum MarkerBoardLayoutSource {
    Owned(marker::MarkerBoardLayout),
    MarkerBoard(Py<PyMarkerBoardParams>),
}

#[pyclass(name = "MarkerBoardLayout", module = "calib_targets._core")]
#[derive(Debug)]
/// Marker-board layout: grid size plus 3 circle markers.
struct PyMarkerBoardLayout {
    inner: MarkerBoardLayoutSource,
}

#[pymethods]
impl PyMarkerBoardLayout {
    #[new]
    #[pyo3(signature = (*, rows, cols, circles=None, cell_size=None))]
    fn new(
        rows: u32,
        cols: u32,
        circles: Option<&Bound<'_, PyAny>>,
        cell_size: Option<f32>,
    ) -> PyResult<Self> {
        let circles = match circles {
            Some(value) => marker_circles_from_obj(value, "circles")?,
            None => marker::MarkerBoardLayout::default().circles,
        };
        let layout = marker::MarkerBoardLayout {
            rows,
            cols,
            cell_size,
            circles,
        };
        Ok(Self {
            inner: MarkerBoardLayoutSource::Owned(layout),
        })
    }

    #[getter]
    fn rows(&self) -> PyResult<u32> {
        self.with_layout(|layout| layout.rows)
    }

    #[setter]
    fn set_rows(&mut self, value: u32) -> PyResult<()> {
        self.with_layout_mut(|layout| {
            layout.rows = value;
        })?;
        Ok(())
    }

    #[getter]
    fn cols(&self) -> PyResult<u32> {
        self.with_layout(|layout| layout.cols)
    }

    #[setter]
    fn set_cols(&mut self, value: u32) -> PyResult<()> {
        self.with_layout_mut(|layout| {
            layout.cols = value;
        })?;
        Ok(())
    }

    #[getter]
    fn cell_size(&self) -> PyResult<Option<f32>> {
        self.with_layout(|layout| layout.cell_size)
    }

    #[setter]
    fn set_cell_size(&mut self, value: Option<f32>) -> PyResult<()> {
        self.with_layout_mut(|layout| {
            layout.cell_size = value;
        })?;
        Ok(())
    }

    #[getter]
    fn circles(slf: PyRef<'_, Self>) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        let parent = slf.into_pyobject(py)?.unbind();
        let mut out = Vec::with_capacity(3);
        for index in 0..3 {
            let spec = PyMarkerCircleSpec {
                inner: MarkerCircleSpecSource::Layout {
                    layout: parent.clone_ref(py),
                    index,
                },
            };
            out.push(Py::new(py, spec)?.into_any());
        }
        Ok(PyList::new(py, out)?.into_any().unbind())
    }

    #[setter]
    fn set_circles(&mut self, value: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        let Some(value) = value else {
            self.with_layout_mut(|layout| {
                layout.circles = marker::MarkerBoardLayout::default().circles;
            })?;
            return Ok(());
        };
        let circles = marker_circles_from_obj(value, "circles")?;
        self.with_layout_mut(|layout| {
            layout.circles = circles;
        })?;
        Ok(())
    }
}

impl PyMarkerBoardLayout {
    fn with_layout<R>(&self, f: impl FnOnce(&marker::MarkerBoardLayout) -> R) -> PyResult<R> {
        match &self.inner {
            MarkerBoardLayoutSource::Owned(layout) => Ok(f(layout)),
            MarkerBoardLayoutSource::MarkerBoard(parent) => Python::attach(|py| {
                let parent = parent.bind(py).borrow();
                Ok(f(&parent.inner.layout))
            }),
        }
    }

    fn with_layout_mut<R>(
        &mut self,
        f: impl FnOnce(&mut marker::MarkerBoardLayout) -> R,
    ) -> PyResult<R> {
        match &mut self.inner {
            MarkerBoardLayoutSource::Owned(layout) => Ok(f(layout)),
            MarkerBoardLayoutSource::MarkerBoard(parent) => Python::attach(|py| {
                let mut parent = parent.bind(py).borrow_mut();
                let out = f(&mut parent.inner.layout);
                update_marker_board_expected_dims(&mut parent.inner);
                Ok(out)
            }),
        }
    }

    fn to_layout(&self) -> PyResult<marker::MarkerBoardLayout> {
        self.with_layout(|layout| layout.clone())
    }
}

fn update_marker_board_expected_dims(params: &mut marker::MarkerBoardParams) {
    params.chessboard.expected_rows = Some(params.layout.rows);
    params.chessboard.expected_cols = Some(params.layout.cols);
}

#[derive(Debug)]
enum CircleScoreParamsSource {
    Owned(marker::CircleScoreParams),
    MarkerBoard(Py<PyMarkerBoardParams>),
}

#[pyclass(name = "CircleScoreParams", module = "calib_targets._core")]
#[derive(Debug)]
/// Parameters for scoring circle markers.
struct PyCircleScoreParams {
    inner: CircleScoreParamsSource,
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
        Ok(Self {
            inner: CircleScoreParamsSource::Owned(params),
        })
    }

    #[getter]
    fn patch_size(&self) -> PyResult<usize> {
        self.with_params(|params| params.patch_size)
    }

    #[setter]
    fn set_patch_size(&mut self, value: usize) -> PyResult<()> {
        self.with_params_mut(|params| {
            params.patch_size = value;
        })?;
        Ok(())
    }

    #[getter]
    fn diameter_frac(&self) -> PyResult<f32> {
        self.with_params(|params| params.diameter_frac)
    }

    #[setter]
    fn set_diameter_frac(&mut self, value: f32) -> PyResult<()> {
        self.with_params_mut(|params| {
            params.diameter_frac = value;
        })?;
        Ok(())
    }

    #[getter]
    fn ring_thickness_frac(&self) -> PyResult<f32> {
        self.with_params(|params| params.ring_thickness_frac)
    }

    #[setter]
    fn set_ring_thickness_frac(&mut self, value: f32) -> PyResult<()> {
        self.with_params_mut(|params| {
            params.ring_thickness_frac = value;
        })?;
        Ok(())
    }

    #[getter]
    fn ring_radius_mul(&self) -> PyResult<f32> {
        self.with_params(|params| params.ring_radius_mul)
    }

    #[setter]
    fn set_ring_radius_mul(&mut self, value: f32) -> PyResult<()> {
        self.with_params_mut(|params| {
            params.ring_radius_mul = value;
        })?;
        Ok(())
    }

    #[getter]
    fn min_contrast(&self) -> PyResult<f32> {
        self.with_params(|params| params.min_contrast)
    }

    #[setter]
    fn set_min_contrast(&mut self, value: f32) -> PyResult<()> {
        self.with_params_mut(|params| {
            params.min_contrast = value;
        })?;
        Ok(())
    }

    #[getter]
    fn samples(&self) -> PyResult<usize> {
        self.with_params(|params| params.samples)
    }

    #[setter]
    fn set_samples(&mut self, value: usize) -> PyResult<()> {
        self.with_params_mut(|params| {
            params.samples = value;
        })?;
        Ok(())
    }

    #[getter]
    fn center_search_px(&self) -> PyResult<i32> {
        self.with_params(|params| params.center_search_px)
    }

    #[setter]
    fn set_center_search_px(&mut self, value: i32) -> PyResult<()> {
        self.with_params_mut(|params| {
            params.center_search_px = value;
        })?;
        Ok(())
    }
}

impl PyCircleScoreParams {
    fn with_params<R>(&self, f: impl FnOnce(&marker::CircleScoreParams) -> R) -> PyResult<R> {
        match &self.inner {
            CircleScoreParamsSource::Owned(params) => Ok(f(params)),
            CircleScoreParamsSource::MarkerBoard(parent) => Python::attach(|py| {
                let parent = parent.bind(py).borrow();
                Ok(f(&parent.inner.circle_score))
            }),
        }
    }

    fn with_params_mut<R>(
        &mut self,
        f: impl FnOnce(&mut marker::CircleScoreParams) -> R,
    ) -> PyResult<R> {
        match &mut self.inner {
            CircleScoreParamsSource::Owned(params) => Ok(f(params)),
            CircleScoreParamsSource::MarkerBoard(parent) => Python::attach(|py| {
                let mut parent = parent.bind(py).borrow_mut();
                Ok(f(&mut parent.inner.circle_score))
            }),
        }
    }

    fn to_params(&self) -> PyResult<marker::CircleScoreParams> {
        self.with_params(|params| *params)
    }
}

#[derive(Debug)]
enum CircleMatchParamsSource {
    Owned(marker::CircleMatchParams),
    MarkerBoard(Py<PyMarkerBoardParams>),
}

#[pyclass(name = "CircleMatchParams", module = "calib_targets._core")]
#[derive(Debug)]
/// Parameters for matching detected circles to the board layout.
struct PyCircleMatchParams {
    inner: CircleMatchParamsSource,
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
        Ok(Self {
            inner: CircleMatchParamsSource::Owned(params),
        })
    }

    #[getter]
    fn max_candidates_per_polarity(&self) -> PyResult<usize> {
        self.with_params(|params| params.max_candidates_per_polarity)
    }

    #[setter]
    fn set_max_candidates_per_polarity(&mut self, value: usize) -> PyResult<()> {
        self.with_params_mut(|params| {
            params.max_candidates_per_polarity = value;
        })?;
        Ok(())
    }

    #[getter]
    fn max_distance_cells(&self) -> PyResult<Option<f32>> {
        self.with_params(|params| params.max_distance_cells)
    }

    #[setter]
    fn set_max_distance_cells(&mut self, value: Option<f32>) -> PyResult<()> {
        self.with_params_mut(|params| {
            params.max_distance_cells = value;
        })?;
        Ok(())
    }

    #[getter]
    fn min_offset_inliers(&self) -> PyResult<usize> {
        self.with_params(|params| params.min_offset_inliers)
    }

    #[setter]
    fn set_min_offset_inliers(&mut self, value: usize) -> PyResult<()> {
        self.with_params_mut(|params| {
            params.min_offset_inliers = value;
        })?;
        Ok(())
    }
}

impl PyCircleMatchParams {
    fn with_params<R>(&self, f: impl FnOnce(&marker::CircleMatchParams) -> R) -> PyResult<R> {
        match &self.inner {
            CircleMatchParamsSource::Owned(params) => Ok(f(params)),
            CircleMatchParamsSource::MarkerBoard(parent) => Python::attach(|py| {
                let parent = parent.bind(py).borrow();
                Ok(f(&parent.inner.match_params))
            }),
        }
    }

    fn with_params_mut<R>(
        &mut self,
        f: impl FnOnce(&mut marker::CircleMatchParams) -> R,
    ) -> PyResult<R> {
        match &mut self.inner {
            CircleMatchParamsSource::Owned(params) => Ok(f(params)),
            CircleMatchParamsSource::MarkerBoard(parent) => Python::attach(|py| {
                let mut parent = parent.bind(py).borrow_mut();
                Ok(f(&mut parent.inner.match_params))
            }),
        }
    }

    fn to_params(&self) -> PyResult<marker::CircleMatchParams> {
        self.with_params(|params| params.clone())
    }
}

#[pyclass(name = "MarkerBoardParams", module = "calib_targets._core")]
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

    #[getter]
    fn layout(slf: PyRef<'_, Self>) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        let parent = slf.into_pyobject(py)?.unbind();
        let layout = PyMarkerBoardLayout {
            inner: MarkerBoardLayoutSource::MarkerBoard(parent),
        };
        Py::new(py, layout).map(|obj| obj.into_any())
    }

    #[setter]
    fn set_layout(&mut self, value: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        let Some(value) = value else {
            self.inner.layout = marker::MarkerBoardLayout::default();
            self.inner.chessboard.expected_rows = Some(self.inner.layout.rows);
            self.inner.chessboard.expected_cols = Some(self.inner.layout.cols);
            return Ok(());
        };
        let layout = marker_board_layout_from_obj(value, "layout")?;
        self.inner.layout = layout;
        self.inner.chessboard.expected_rows = Some(self.inner.layout.rows);
        self.inner.chessboard.expected_cols = Some(self.inner.layout.cols);
        Ok(())
    }

    #[getter]
    fn chessboard(slf: PyRef<'_, Self>) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        let parent = slf.into_pyobject(py)?.unbind();
        let params = PyChessboardParams {
            inner: ChessboardParamsSource::MarkerBoard(parent),
        };
        Py::new(py, params).map(|obj| obj.into_any())
    }

    #[setter]
    fn set_chessboard(&mut self, value: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        let Some(value) = value else {
            self.inner.chessboard = chessboard::ChessboardParams::default();
            return Ok(());
        };
        self.inner.chessboard =
            chessboard_params_from_obj(value, "chessboard", self.inner.chessboard.clone())?;
        Ok(())
    }

    #[getter]
    fn grid_graph(slf: PyRef<'_, Self>) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        let parent = slf.into_pyobject(py)?.unbind();
        let params = PyGridGraphParams {
            inner: GridGraphParamsSource::MarkerBoard(parent),
        };
        Py::new(py, params).map(|obj| obj.into_any())
    }

    #[setter]
    fn set_grid_graph(&mut self, value: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        let Some(value) = value else {
            self.inner.grid_graph = chessboard::GridGraphParams::default();
            return Ok(());
        };
        self.inner.grid_graph =
            grid_graph_params_from_obj(value, "grid_graph", self.inner.grid_graph.clone())?;
        Ok(())
    }

    #[getter]
    fn circle_score(slf: PyRef<'_, Self>) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        let parent = slf.into_pyobject(py)?.unbind();
        let params = PyCircleScoreParams {
            inner: CircleScoreParamsSource::MarkerBoard(parent),
        };
        Py::new(py, params).map(|obj| obj.into_any())
    }

    #[setter]
    fn set_circle_score(&mut self, value: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        let Some(value) = value else {
            self.inner.circle_score = marker::CircleScoreParams::default();
            return Ok(());
        };
        self.inner.circle_score =
            circle_score_params_from_obj(value, "circle_score", self.inner.circle_score)?;
        Ok(())
    }

    #[getter]
    fn match_params(slf: PyRef<'_, Self>) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        let parent = slf.into_pyobject(py)?.unbind();
        let params = PyCircleMatchParams {
            inner: CircleMatchParamsSource::MarkerBoard(parent),
        };
        Py::new(py, params).map(|obj| obj.into_any())
    }

    #[setter]
    fn set_match_params(&mut self, value: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        let Some(value) = value else {
            self.inner.match_params = marker::CircleMatchParams::default();
            return Ok(());
        };
        self.inner.match_params =
            circle_match_params_from_obj(value, "match_params", self.inner.match_params.clone())?;
        Ok(())
    }

    #[getter]
    fn roi_cells(&self) -> Option<[i32; 4]> {
        self.inner.roi_cells
    }

    #[setter]
    fn set_roi_cells(&mut self, value: Option<[i32; 4]>) {
        self.inner.roi_cells = value;
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
}

#[derive(Debug, Default, Deserialize, Clone)]
struct CharucoDetectorParamsOverrides {
    #[serde(default)]
    board: Option<charuco::CharucoBoardSpec>,
    #[serde(default)]
    px_per_square: Option<f32>,
    #[serde(default)]
    chessboard: Option<ChessboardParamsOverrides>,
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
        if let Some(board) = self.board {
            params.charuco = board;
        }
        if let Some(px_per_square) = self.px_per_square {
            params.px_per_square = px_per_square;
        }
        if let Some(chessboard) = self.chessboard {
            chessboard.apply(&mut params.chessboard);
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

fn marker_layout_name(layout: charuco::MarkerLayout) -> &'static str {
    match layout {
        charuco::MarkerLayout::OpenCvCharuco => "opencv_charuco",
    }
}

fn circle_polarity_name(polarity: marker::CirclePolarity) -> &'static str {
    match polarity {
        marker::CirclePolarity::White => "white",
        marker::CirclePolarity::Black => "black",
    }
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
    Ok(value.cast_into::<PyDict>().ok())
}

fn charuco_board_from_obj(
    obj: &Bound<'_, PyAny>,
    path: &str,
) -> PyResult<charuco::CharucoBoardSpec> {
    if let Ok(board) = obj.extract::<PyRef<PyCharucoBoardSpec>>() {
        return board.to_spec();
    }
    if let Ok(dict) = obj.cast::<PyDict>() {
        validate_charuco_board_dict(dict, path)?;
    }
    let value = py_to_json(obj, path)?;
    serde_json::from_value(value).map_err(|err| value_error(format!("{path}: {err}")))
}

fn marker_circle_from_obj(
    obj: &Bound<'_, PyAny>,
    path: &str,
) -> PyResult<marker::MarkerCircleSpec> {
    if let Ok(circle) = obj.extract::<PyRef<PyMarkerCircleSpec>>() {
        return circle.to_spec();
    }
    let value = py_to_json(obj, path)?;
    serde_json::from_value(value).map_err(|err| value_error(format!("{path}: {err}")))
}

fn marker_circles_from_obj(
    obj: &Bound<'_, PyAny>,
    path: &str,
) -> PyResult<[marker::MarkerCircleSpec; 3]> {
    if let Ok(list) = obj.cast::<PyList>() {
        if list.len() != 3 {
            return Err(value_error(format!(
                "{path}: expected 3 circle specs, got {}",
                list.len()
            )));
        }
        let mut out = Vec::with_capacity(3);
        for (idx, item) in list.iter().enumerate() {
            let child_path = format!("{path}[{idx}]");
            out.push(marker_circle_from_obj(&item, &child_path)?);
        }
        return Ok([out[0], out[1], out[2]]);
    }

    if let Ok(tuple) = obj.cast::<PyTuple>() {
        if tuple.len() != 3 {
            return Err(value_error(format!(
                "{path}: expected 3 circle specs, got {}",
                tuple.len()
            )));
        }
        let mut out = Vec::with_capacity(3);
        for (idx, item) in tuple.iter().enumerate() {
            let child_path = format!("{path}[{idx}]");
            out.push(marker_circle_from_obj(&item, &child_path)?);
        }
        return Ok([out[0], out[1], out[2]]);
    }

    Err(value_error(format!(
        "{path}: expected a list/tuple of 3 circle specs"
    )))
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

fn validate_charuco_board_dict(dict: &Bound<'_, PyDict>, path: &str) -> PyResult<()> {
    validate_dict_keys(
        dict,
        path,
        &[
            "rows",
            "cols",
            "cell_size",
            "marker_size_rel",
            "dictionary",
            "marker_layout",
        ],
    )
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
            "board",
            "px_per_square",
            "chessboard",
            "graph",
            "scan",
            "max_hamming",
            "min_marker_inliers",
        ],
    )?;
    if let Some(board) = get_optional_dict(dict, "board")? {
        validate_charuco_board_dict(&board, &format!("{path}.board"))?;
    }
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

    if let Ok(dict) = obj.cast::<PyDict>() {
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

    if let Ok(list) = obj.cast::<PyList>() {
        let mut out = Vec::with_capacity(list.len());
        for (idx, item) in list.iter().enumerate() {
            let child_path = format!("{path}[{idx}]");
            out.push(py_to_json(&item, &child_path)?);
        }
        return Ok(Value::Array(out));
    }

    if let Ok(tuple) = obj.cast::<PyTuple>() {
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

fn json_to_py(py: Python<'_>, value: &Value) -> PyResult<Py<PyAny>> {
    match value {
        Value::Null => Ok(py.None()),
        Value::Bool(v) => v.into_py_any(py),
        Value::Number(v) => {
            if let Some(i) = v.as_i64() {
                i.into_py_any(py)
            } else if let Some(u) = v.as_u64() {
                u.into_py_any(py)
            } else if let Some(f) = v.as_f64() {
                f.into_py_any(py)
            } else {
                Ok(py.None())
            }
        }
        Value::String(s) => s.into_py_any(py),
        Value::Array(values) => {
            let mut out = Vec::with_capacity(values.len());
            for item in values {
                out.push(json_to_py(py, item)?);
            }
            Ok(PyList::new(py, out)?.into_any().unbind())
        }
        Value::Object(map) => {
            let dict = PyDict::new(py);
            for (key, item) in map.iter() {
                let value = json_to_py(py, item)?;
                dict.set_item(key, value)?;
            }
            Ok(dict.into_any().unbind())
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
        return params.to_params();
    }
    if let Ok(dict) = obj.cast::<PyDict>() {
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
        return params.to_params();
    }
    if let Ok(dict) = obj.cast::<PyDict>() {
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
        return params.to_params();
    }
    if let Ok(dict) = obj.cast::<PyDict>() {
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
        return params.to_params();
    }
    if let Ok(dict) = obj.cast::<PyDict>() {
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
        return params.to_params();
    }
    if let Ok(dict) = obj.cast::<PyDict>() {
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
        return params.to_params();
    }
    if let Ok(dict) = obj.cast::<PyDict>() {
        validate_grid_graph_dict(dict, path)?;
    }
    let value = py_to_json(obj, path)?;
    let overrides: GridGraphParamsOverrides =
        serde_json::from_value(value).map_err(|err| value_error(format!("{path}: {err}")))?;
    let mut params = base;
    overrides.apply(&mut params);
    Ok(params)
}

fn scan_decode_params_from_obj(
    obj: &Bound<'_, PyAny>,
    path: &str,
    base: aruco::ScanDecodeConfig,
) -> PyResult<aruco::ScanDecodeConfig> {
    if obj.is_none() {
        return Ok(base);
    }
    if let Ok(params) = obj.extract::<PyRef<PyScanDecodeConfig>>() {
        return params.to_params();
    }
    if let Ok(dict) = obj.cast::<PyDict>() {
        validate_scan_decode_dict(dict, path)?;
    }
    let value = py_to_json(obj, path)?;
    let overrides: ScanDecodeConfigOverrides =
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
        return params.to_params();
    }
    if let Ok(dict) = obj.cast::<PyDict>() {
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
        return params.to_params();
    }
    if let Ok(dict) = obj.cast::<PyDict>() {
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
    if let Ok(layout) = obj.extract::<PyRef<PyMarkerBoardLayout>>() {
        return layout.to_layout();
    }
    let dict = obj
        .cast::<PyDict>()
        .map_err(|_| value_error(format!("{path}: expected dict")))?;
    validate_marker_board_layout_dict(dict, path)?;
    let value = py_to_json(obj, path)?;
    let overrides: MarkerBoardLayoutOverrides =
        serde_json::from_value(value).map_err(|err| value_error(format!("{path}: {err}")))?;
    let mut layout = marker::MarkerBoardLayout::default();
    overrides.apply(&mut layout);
    Ok(layout)
}

fn chess_cfg_from_py(obj: Option<&Bound<'_, PyAny>>) -> PyResult<ChessConfig> {
    let mut cfg = detect::default_chess_config();
    if let Some(obj) = obj {
        if !obj.is_none() {
            if let Ok(py_cfg) = obj.extract::<PyRef<PyChessConfig>>() {
                return Ok(py_cfg.inner.clone());
            }
            if let Ok(dict) = obj.cast::<PyDict>() {
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
        return py_params.to_params();
    }
    if let Ok(dict) = obj.cast::<PyDict>() {
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
    if let Ok(dict) = obj.cast::<PyDict>() {
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
) -> PyResult<charuco::CharucoDetectorParams> {
    let Some(obj) = obj else {
        return Err(value_error("params is required for ChArUco detection"));
    };
    if obj.is_none() {
        return Err(value_error("params is required for ChArUco detection"));
    }
    if let Ok(py_params) = obj.extract::<PyRef<PyCharucoDetectorParams>>() {
        return Ok(py_params.inner.clone());
    }
    if let Ok(dict) = obj.cast::<PyDict>() {
        validate_charuco_params_dict(dict, "params")?;
    }
    let value = py_to_json(obj, "params")?;
    let overrides: CharucoDetectorParamsOverrides =
        serde_json::from_value(value).map_err(|err| value_error(format!("params: {err}")))?;
    let Some(board) = overrides.board else {
        return Err(value_error(
            "params.board is required for ChArUco detection",
        ));
    };
    let mut params = charuco::CharucoDetectorParams::for_board(&board);
    overrides.apply(&mut params);
    Ok(params)
}

fn gray_image_from_py(image: &Bound<'_, PyAny>) -> PyResult<::image::GrayImage> {
    let array = image
        .cast::<PyArrayDyn<u8>>()
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
    let pixels = view.to_owned().into_raw_vec_and_offset().0;
    detect::gray_image_from_slice(width, height, &pixels)
        .map_err(|err| value_error(err.to_string()))
}

/// Detect a ChArUco board in a grayscale image.
///
/// Args:
///   image: 2D numpy.ndarray[uint8] (H, W) grayscale image.
///   chess_cfg: None | dict overrides | ChessConfig.
///   params: CharucoDetectorParams or dict with a `board` entry.
///
/// Returns:
///   dict with detection data, or raises RuntimeError on detection errors.
#[pyfunction]
#[pyo3(signature = (image, *, chess_cfg=None, params))]
fn detect_charuco(
    py: Python<'_>,
    image: &Bound<'_, PyAny>,
    chess_cfg: Option<&Bound<'_, PyAny>>,
    params: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let img = gray_image_from_py(image)?;
    let chess_cfg = chess_cfg_from_py(chess_cfg)?;
    let params = charuco_params_from_py(Some(params))?;

    let result = py.detach(move || detect::detect_charuco(&img, &chess_cfg, params));
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
) -> PyResult<Option<Py<PyAny>>> {
    let img = gray_image_from_py(image)?;
    let chess_cfg = chess_cfg_from_py(chess_cfg)?;
    let params = chessboard_params_from_py(params)?;

    let result = py.detach(move || detect::detect_chessboard(&img, &chess_cfg, params));
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
) -> PyResult<Option<Py<PyAny>>> {
    let img = gray_image_from_py(image)?;
    let chess_cfg = chess_cfg_from_py(chess_cfg)?;
    let params = marker_board_params_from_py(params)?;

    let result = py.detach(move || detect::detect_marker_board(&img, &chess_cfg, params));
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
fn _core(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyChessConfig>()?;
    m.add_class::<PyChessCornerParams>()?;
    m.add_class::<PyCoarseToFineParams>()?;
    m.add_class::<PyPyramidParams>()?;
    m.add_class::<PyOrientationClusteringParams>()?;
    m.add_class::<PyGridGraphParams>()?;
    m.add_class::<PyChessboardParams>()?;
    m.add_class::<PyScanDecodeConfig>()?;
    m.add_class::<PyCharucoBoardSpec>()?;
    m.add_class::<PyCharucoDetectorParams>()?;
    m.add_class::<PyMarkerCircleSpec>()?;
    m.add_class::<PyMarkerBoardLayout>()?;
    m.add_class::<PyCircleScoreParams>()?;
    m.add_class::<PyCircleMatchParams>()?;
    m.add_class::<PyMarkerBoardParams>()?;
    m.add_function(wrap_pyfunction!(detect_charuco, m)?)?;
    m.add_function(wrap_pyfunction!(detect_chessboard, m)?)?;
    m.add_function(wrap_pyfunction!(detect_marker_board, m)?)?;
    Ok(())
}
