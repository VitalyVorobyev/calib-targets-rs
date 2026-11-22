# References

Every paper cited in the book, the API docs or the internal docs has one entry
here. Other pages link to the entry (for example
`[Bennett & Lasenby 2014](references.md#bennett2014)`) instead of repeating the
full citation. Each entry gives the reference, a stable link and one sentence
on what it is relevant for in this project.

## Corner detection and grid finding

<a id="bennett2014"></a>
**Bennett & Lasenby 2014.** S. Bennett, J. Lasenby. *ChESS — Quick and Robust
Detection of Chess-board Features.* Computer Vision and Image Understanding 118,
197–210, 2014.
[doi:10.1016/j.cviu.2013.10.008](https://doi.org/10.1016/j.cviu.2013.10.008) ·
[arXiv:1301.5491](https://arxiv.org/abs/1301.5491).
The ChESS X-junction detector whose corners and axis estimates are the input
of every detector in this workspace.

<a id="shu2010"></a>
**Shu, Brunton & Fiala 2010.** C. Shu, A. Brunton, M. Fiala. *A topological
approach to finding grids in calibration patterns.* Machine Vision and
Applications 21(6), 949–957, 2010 (published online October 2009).
[doi:10.1007/s00138-009-0202-2](https://doi.org/10.1007/s00138-009-0202-2).
The Delaunay-based grid finder that the topological grid builder follows, with
an axis-alignment cell test in place of the paper's image-colour test.

## Targets

<a id="stelldinger2024"></a>
**Stelldinger, Schönherr & Biermann 2024.** P. Stelldinger, N. Schönherr,
J. Biermann. *PuzzleBoard: A New Camera Calibration Pattern with Position
Encoding.* German Conference on Pattern Recognition (GCPR), 2024.
[arXiv:2409.20127](https://arxiv.org/abs/2409.20127) ·
[doi:10.1007/978-3-031-85187-2_21](https://doi.org/10.1007/978-3-031-85187-2_21).
The PuzzleBoard pattern and its edge-dot code, on which the PuzzleBoard target
and decoder are based.

<a id="stelldinger2024rings"></a>
**Stelldinger 2024 (rings).** P. Stelldinger. *On de Bruijn Rings and Families
of Almost Perfect Maps.* arXiv:2405.03309, 2024 (revised December 2024).
[arXiv:2405.03309](https://arxiv.org/abs/2405.03309) ·
[doi:10.1016/j.jcta.2025.106030](https://doi.org/10.1016/j.jcta.2025.106030).
The companion paper with a closed-form construction of the codes used by
PuzzleBoard; the code-maps page explains why this workspace imports the
reference maps instead.

<a id="zach2025"></a>
**Zach & Stelldinger 2025.** J. Zach, P. Stelldinger. *PuzzlePoles: Cylindrical
Fiducial Markers Based on the PuzzleBoard Pattern.* arXiv:2511.19448, 2025.
[arXiv:2511.19448](https://arxiv.org/abs/2511.19448).
The PuzzlePole target (the PuzzleBoard pattern wrapped round a cylinder) and its
construction rules.

## Blur and focus measurement from calibration targets

These papers inform the [per-corner blur](blur.md) measurement and its use
for tilted-sensor (Scheimpflug) calibration.

<a id="kumar2014"></a>
**Kumar & Ahuja 2014.** A. Kumar, N. Ahuja. *Non-frontal Camera Calibration
using Focal Stack Imagery.* International Conference on Pattern Recognition
(ICPR), 2014, 202–207.
[doi:10.1109/ICPR.2014.44](https://doi.org/10.1109/ICPR.2014.44) ·
[PDF](https://migrate2wp.web.illinois.edu/html-files-to-import/publications/Kumar_FocalStackCalibration_ICPR_2014.pdf).
Calibrates sensor tilt by adding a blur term, 1 − NCC between observed corner
patches and sharp patches blurred by a thin-lens Gaussian predicted from the
calibration parameters.

<a id="baba2006"></a>
**Baba, Mukunoki & Asada 2006.** M. Baba, M. Mukunoki, N. Asada. *A unified
camera calibration using geometry and blur of feature points.* ICPR 2006,
vol. 1, 816–819.
[doi:10.1109/ICPR.2006.191](https://doi.org/10.1109/ICPR.2006.191).
Jointly estimates blur width and feature position from blurred checkerboard
edges with a thin-lens model.

<a id="joshi2008"></a>
**Joshi, Szeliski & Kriegman 2008.** N. Joshi, R. Szeliski, D. J. Kriegman.
*PSF Estimation using Sharp Edge Prediction.* CVPR 2008.
[doi:10.1109/CVPR.2008.4587834](https://doi.org/10.1109/CVPR.2008.4587834).
Predicts the sharp edge from the blurred one and solves for a sub-pixel,
spatially varying PSF.

<a id="kee2011"></a>
**Kee, Paris, Chen & Wang 2011.** E. Kee, S. Paris, S. Chen, J. Wang.
*Modeling and Removing Spatially-Varying Optical Blur.* IEEE International
Conference on Computational Photography (ICCP), 2011.
[doi:10.1109/ICCPHOT.2011.5753120](https://doi.org/10.1109/ICCPHOT.2011.5753120).
Shows that lens optics produce asymmetric, spatially varying blur and proposes
models to predict it.

<a id="delbracio2012"></a>
**Delbracio, Musé, Almansa & Morel 2012.** M. Delbracio, P. Musé, A. Almansa,
J.-M. Morel. *The Non-parametric Sub-pixel Local Point Spread Function
Estimation Is a Well Posed Problem.* International Journal of Computer Vision
96(2), 175–194, 2012.
[doi:10.1007/s11263-011-0460-0](https://doi.org/10.1007/s11263-011-0460-0).
Analyses non-parametric sub-pixel local PSF estimation from a known target.

<a id="mosleh2015"></a>
**Mosleh et al. 2015.** A. Mosleh, P. Green, E. Onzon, I. Begin,
J. M. P. Langlois. *Camera Intrinsic Blur Kernel Estimation: A Reliable
Framework.* CVPR 2015, 4961–4968.
[doi:10.1109/CVPR.2015.7299130](https://doi.org/10.1109/CVPR.2015.7299130) ·
[CVF open access](https://openaccess.thecvf.com/content_cvpr_2015/html/Mosleh_Camera_Intrinsic_Blur_2015_CVPR_paper.html).
Non-blind measurement of intrinsic lens blur from a known target image.

<a id="reuter2012"></a>
**Reuter, Seidel & Ihrke 2012.** A. Reuter, H.-P. Seidel, I. Ihrke. *BlurTags:
spatially varying PSF estimation with out-of-focus patterns.* International
Conference on Computer Graphics, Visualization and Computer Vision (WSCG),
2012, 239–248.
[Max Planck record](https://pure.mpg.de/pubman/item/item_1858636_4).
Estimates a spatially varying PSF from out-of-focus calibration patterns.

<a id="shi2026"></a>
**Shi 2026.** Z. Shi. *Generic Camera Calibration using Blurry Images.*
arXiv:2603.05159, 2026.
[arXiv:2603.05159](https://arxiv.org/abs/2603.05159).
Estimates feature geometry and a spatially varying blur kernel per image block,
and states the shift ambiguity between kernel and pattern.

<a id="he2025"></a>
**He et al. 2025.** J. He, E. Hu, S. Chen, T. Qiu, H. Feng, Z. Xu, Y. Chen.
*CircleFlow: Flow-Guided Camera Blur Estimation using a Circle Grid Target.*
arXiv:2512.00796, 2025.
[arXiv:2512.00796](https://arxiv.org/abs/2512.00796).
Argues that checkerboard edges sample the blur in only two directions, and
estimates the PSF from a circle grid target instead.

<a id="owen1956"></a>
**Owen 1956.** D. B. Owen. *Tables for Computing Bivariate Normal
Probabilities.* The Annals of Mathematical Statistics 27(4), 1075–1090, 1956.
[doi:10.1214/aoms/1177728074](https://doi.org/10.1214/aoms/1177728074).
Introduces the T function; the per-corner blur fit integrates a Gaussian over
each printed polygon exactly with it.

<a id="neweywest1987"></a>
**Newey & West 1987.** W. K. Newey, K. D. West. *A Simple, Positive
Semi-Definite, Heteroskedasticity and Autocorrelation Consistent Covariance
Matrix.* Econometrica 55(3), 703–708, 1987.
[doi:10.2307/1913610](https://doi.org/10.2307/1913610).
The Bartlett-kernel sandwich covariance the blur fit's standard errors use.

<a id="andrews1991"></a>
**Andrews 1991.** D. W. K. Andrews. *Heteroskedasticity and Autocorrelation
Consistent Covariance Matrix Estimation.* Econometrica 59(3), 817–858, 1991.
[doi:10.2307/2938229](https://doi.org/10.2307/2938229).
Automatic bandwidth selection for that sandwich, from the residuals' own
autocorrelation.

<a id="iso12233"></a>
**ISO 12233.** ISO 12233, *Photography — Electronic still picture imaging —
Resolution and spatial frequency responses* (current edition at
[iso.org](https://www.iso.org/)).
The standard edge-based (slanted-edge) spatial frequency response measurement.

<a id="steger2017"></a>
**Steger 2017.** C. Steger. *A Comprehensive and Versatile Camera Model for
Cameras with Tilt Lenses.* International Journal of Computer Vision 123(2),
121–159, 2017.
[doi:10.1007/s11263-016-0964-8](https://doi.org/10.1007/s11263-016-0964-8).
The geometric camera model for tilt lenses, including perspective and
telecentric cases, with a calibration algorithm.
