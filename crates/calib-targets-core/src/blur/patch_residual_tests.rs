use super::*;
use crate::blur::defocus::ThinLensDefocusModel;
use nalgebra::{Matrix3, Vector3};

fn checker_pattern() -> BlurPattern {
    let rect = |x0: f64, y0: f64| {
        vec![
            Point2::new(x0, y0),
            Point2::new(x0 + 1.0, y0),
            Point2::new(x0 + 1.0, y0 + 1.0),
            Point2::new(x0, y0 + 1.0),
        ]
    };
    let mut p = BlurPattern::new().with_known_polarity(true);
    for b in -3..3 {
        for a in -3..3 {
            p = p.with_known_cell(a, b);
            if (a + b).rem_euclid(2) == 0 {
                p = p.with_polygon(rect(f64::from(a), f64::from(b)), 1.0);
            }
        }
    }
    p
}

#[derive(Clone, Copy)]
struct PatchRenderSpec {
    ac: [f64; 2],
    illu: [f64; 2],
    gamma: f64,
    bleed: f64,
    noise_amp: f64,
}

fn render_pattern_patch(
    width: usize,
    height: usize,
    pattern: &BlurPattern,
    hom: &Homography<f64>,
    kernel: BlurKernel,
    opts: &PhotometricFitOptions,
    spec: PatchRenderSpec,
) -> Vec<u8> {
    let polys: Vec<BoardPolygon> = pattern
        .polygons
        .iter()
        .map(|(v, w)| BoardPolygon {
            vertices: v.clone(),
            weight: *w,
        })
        .collect();
    let frame = Frame::new(hom).unwrap();
    let sigma = [
        f64::from(kernel.xx),
        f64::from(kernel.xy),
        f64::from(kernel.yy),
    ];
    let box_model = box_model_for(sigma);
    let model = Model {
        polygons: &polys,
        c0: frame.c0,
        scale: frame.scale,
        box_model,
        fixed: [false; NP],
        pixel_bounds: [0.0, 0.0, width as f64 - 1.0, height as f64 - 1.0],
        radiometric: opts.radiometric,
        fit_ink_bleed: opts.fit_ink_bleed,
        blur_gradient: None,
    };
    let mut theta = Vector::zeros();
    theta[I_SIGMA] = sigma[0];
    theta[I_SIGMA + 1] = sigma[1];
    theta[I_SIGMA + 2] = sigma[2];
    for k in 0..8 {
        theta[I_H + k] = frame.h[k];
    }
    theta[I_BLEED] = spec.bleed;
    let (pp, linv) = model.pass_polygons(&theta).unwrap();
    let mut out = vec![0u8; width * height];
    for y in 0..height {
        for x in 0..width {
            let px = Pixel {
                x: x as i32,
                y: y as i32,
                value: 0.0,
            };
            let (b_raw, _, _) = model.sample_pixel(&pp, None, px, linv, false);
            let (b, _, _) = bleed_eval(b_raw, spec.bleed, opts.fit_ink_bleed);
            let dx = (x as f64 - frame.c0[0]) / frame.scale;
            let dy = (y as f64 - frame.c0[1]) / frame.scale;
            let lin = (1.0 + spec.illu[0] * dx + spec.illu[1] * dy)
                * (spec.ac[1] + spec.ac[0] * (1.0 - b));
            let (obs, _, _) = opts.radiometric.forward(lin, spec.gamma);
            let hash = (((x * 73856093) ^ (y * 19349663)) % 97) as f64 / 48.0 - 1.0;
            out[y * width + x] = (obs + spec.noise_amp * hash).round().clamp(1.0, 254.0) as u8;
        }
    }
    out
}

#[test]
fn analytic_varpro_and_radiometric_jacobians_match_finite_differences() {
    let pat = checker_pattern();
    // Perspective-warped homography centered at (50.3, 50.6).
    let hom_true = Homography::new(Matrix3::new(
        26.0, 2.2, 50.3, -1.8, 25.0, 50.6, 0.004, -0.003, 1.0,
    ));
    let kernel_true = BlurKernel::new(3.1, -0.35, 2.6);

    // Case 0: pure linear Golub-Pereyra VarPro with significant noise (6.0 gray)
    // proving the second-order `-Φ w` residual-curvature term is exact and needed.
    let hom_eval = Homography::new(Matrix3::new(
        26.2, 2.0, 50.45, -1.6, 24.8, 50.42, 0.0045, -0.0028, 1.0,
    ));
    let opts_pure = PhotometricFitOptions::default();
    let px_pure = render_pattern_patch(
        100,
        100,
        &pat,
        &hom_true,
        kernel_true,
        &opts_pure,
        PatchRenderSpec {
            ac: [140.0, 42.0],
            illu: [0.06, -0.04],
            gamma: 1.0,
            bleed: 0.0,
            noise_amp: 6.0,
        },
    );
    let view_pure = GrayImageView {
        width: 100,
        height: 100,
        data: &px_pure,
    };
    let patch_pure = PatchResidual::extract(&view_pure, &pat, &hom_eval, 1.2, &opts_pure).unwrap();
    let input_pure = PatchEvalInput::new(hom_eval, BlurKernel::new(2.9, -0.2, 2.8))
        .with_illumination_gradient([0.05, -0.03]);
    let ev_pure = patch_pure.evaluate(&input_pure).unwrap();
    let nh_pure = patch_pure
        .normalize_homography(&input_pure.homography)
        .unwrap();
    let inv_norm_pure = patch_pure.norm_mat.try_inverse().unwrap();
    let denorm_pure = |h_til: &[f64; 8]| -> Homography<f64> {
        Homography::new(
            inv_norm_pure
                * Matrix3::new(
                    h_til[0], h_til[1], h_til[2], h_til[3], h_til[4], h_til[5], h_til[6], h_til[7],
                    1.0,
                ),
        )
    };
    for k in [0usize, 2] {
        let step = 1e-5;
        let mut hp = nh_pure;
        hp[k] += step;
        let mut hm = nh_pure;
        hm[k] -= step;
        let ep = patch_pure
            .evaluate(
                &PatchEvalInput::new(denorm_pure(&hp), input_pure.kernel)
                    .with_illumination_gradient(input_pure.illumination_gradient),
            )
            .unwrap();
        let em = patch_pure
            .evaluate(
                &PatchEvalInput::new(denorm_pure(&hm), input_pure.kernel)
                    .with_illumination_gradient(input_pure.illumination_gradient),
            )
            .unwrap();
        let col_max = ev_pure
            .d_res_d_h
            .iter()
            .map(|r| r[k].abs())
            .fold(0.0f64, f64::max);
        let mut max_rel = 0.0f64;
        for ((a, &p), &m) in ev_pure
            .d_res_d_h
            .iter()
            .map(|r| r[k])
            .zip(&ep.residuals)
            .zip(&em.residuals)
        {
            let fd = (p - m) / (2.0 * step);
            let scale = a.abs().max(fd.abs()).max(0.05 * col_max).max(1e-2);
            max_rel = max_rel.max((a - fd).abs() / scale);
        }
        assert!(max_rel < 1e-4, "pure varpro h[{k}] max_rel={max_rel:.3e}");
    }
    // Verify full Golub-Pereyra VarPro (-u + Phi(v - w)) beats Kaufman (-u + Phi v)
    // by > 20x on a noisy patch where residuals have significant energy.
    {
        let step = 2e-6;
        let mut hp0 = nh_pure;
        hp0[0] += step;
        let mut hm0 = nh_pure;
        hm0[0] -= step;
        let ep = patch_pure
            .evaluate(
                &PatchEvalInput::new(denorm_pure(&hp0), input_pure.kernel)
                    .with_illumination_gradient(input_pure.illumination_gradient),
            )
            .unwrap();
        let em = patch_pure
            .evaluate(
                &PatchEvalInput::new(denorm_pure(&hm0), input_pure.kernel)
                    .with_illumination_gradient(input_pure.illumination_gradient),
            )
            .unwrap();
        let ev_fix = patch_pure
            .evaluate(&input_pure.with_fixed_ac([ev_pure.contrast, ev_pure.black_level]))
            .unwrap();
        let (mut g00, mut g01, mut g11, mut pt_u0, mut pt_u1) = (0.0, 0.0, 0.0, 0.0, 0.0);
        let (a, c) = (ev_pure.contrast, ev_pure.black_level);
        let [alpha, beta] = input_pure.illumination_gradient;
        let inv_a_s_sq = |px: &PatchPixel| {
            let dx = (f64::from(px.x) - patch_pure.c0[0]) / patch_pure.scale;
            let dy = (f64::from(px.y) - patch_pure.c0[1]) / patch_pure.scale;
            1.0 + alpha * dx + beta * dy
        };
        let mut phis = Vec::with_capacity(patch_pure.pixels.len());
        for (i, px) in patch_pure.pixels.iter().enumerate() {
            let s = inv_a_s_sq(px);
            let pred_lin = px.value - ev_pure.residuals[i];
            let p1 = s;
            let p0 = (pred_lin - c * s) / a;
            let u = -ev_fix.d_res_d_h[i][0];
            g00 += p0 * p0;
            g01 += p0 * p1;
            g11 += p1 * p1;
            pt_u0 += p0 * u;
            pt_u1 += p1 * u;
            phis.push((p0, p1, u));
        }
        let inv_g = invert_2x2_sym(g00, g01, g11).unwrap();
        let v0 = inv_g[0][0] * pt_u0 + inv_g[0][1] * pt_u1;
        let v1 = inv_g[1][0] * pt_u0 + inv_g[1][1] * pt_u1;
        let mut err_gp = 0.0f64;
        let mut err_kaufman = 0.0f64;
        for (i, (p0, p1, u)) in phis.into_iter().enumerate() {
            let fd = (ep.residuals[i] - em.residuals[i]) / (2.0 * step);
            let gp = ev_pure.d_res_d_h[i][0];
            let kaufman = -u + p0 * v0 + p1 * v1;
            err_gp = err_gp.max((gp - fd).abs());
            err_kaufman = err_kaufman.max((kaufman - fd).abs());
        }
        assert!(
            err_kaufman > 20.0 * err_gp,
            "expected Kaufman ({err_kaufman:.4e}) >> Golub-Pereyra ({err_gp:.4e})"
        );
    }

    // Case 1: Linear + Golub-Pereyra VarPro + ink bleed + non-zero noise.
    let opts_lin = PhotometricFitOptions::default()
        .with_radiometric(RadiometricModel::Linear)
        .with_fit_ink_bleed(true);
    let pixels_lin = render_pattern_patch(
        100,
        100,
        &pat,
        &hom_true,
        kernel_true,
        &opts_lin,
        PatchRenderSpec {
            ac: [140.0, 42.0],
            illu: [0.06, -0.04],
            gamma: 1.0,
            bleed: 0.12,
            noise_amp: 3.0,
        },
    );
    let view_lin = GrayImageView {
        width: 100,
        height: 100,
        data: &pixels_lin,
    };
    let patch_lin = PatchResidual::extract(&view_lin, &pat, &hom_eval, 1.2, &opts_lin).unwrap();
    let input_lin = PatchEvalInput::new(hom_eval, BlurKernel::new(2.9, -0.2, 2.8))
        .with_illumination_gradient([0.05, -0.03])
        .with_bleed_kappa(0.10);
    let eval_lin = patch_lin.evaluate(&input_lin).expect("varpro eval");

    let norm_h = patch_lin
        .normalize_homography(&input_lin.homography)
        .unwrap();
    let denorm_h = |h_til: &[f64; 8]| -> Homography<f64> {
        let inv_norm = patch_lin.norm_mat.try_inverse().unwrap();
        let h_m = Matrix3::new(
            h_til[0], h_til[1], h_til[2], h_til[3], h_til[4], h_til[5], h_til[6], h_til[7], 1.0,
        );
        Homography::new(inv_norm * h_m)
    };
    let check_vec = |name: &str, analytic: &[f64], plus: &[f64], minus: &[f64], step: f64| {
        let col_max = analytic.iter().map(|v| v.abs()).fold(0.0f64, f64::max);
        let mut max_rel = 0.0f64;
        for (&a, (&p, &m)) in analytic.iter().zip(plus.iter().zip(minus)) {
            let fd = (p - m) / (2.0 * step);
            let scale = a.abs().max(fd.abs()).max(0.05 * col_max).max(1e-2);
            max_rel = max_rel.max((a - fd).abs() / scale);
        }
        assert!(max_rel < 2e-4, "{name}: max_rel={max_rel:.3e}");
    };

    // 1a) d_res_d_h
    let hh = 1e-5;
    for k in 0..8 {
        let mut hp = norm_h;
        hp[k] += hh;
        let mut hm = norm_h;
        hm[k] -= hh;
        let mut ip = input_lin;
        ip.homography = denorm_h(&hp);
        let mut im = input_lin;
        im.homography = denorm_h(&hm);
        let ep = patch_lin.evaluate(&ip).unwrap();
        let em = patch_lin.evaluate(&im).unwrap();
        let col: Vec<f64> = eval_lin.d_res_d_h.iter().map(|r| r[k]).collect();
        check_vec(
            &format!("varpro d_res_d_h[{k}]"),
            &col,
            &ep.residuals,
            &em.residuals,
            hh,
        );
    }

    // 1b) d_res_d_corner (translation T(dx, dy) * H under perspective!)
    let hc = 1e-4;
    for axis in 0..2 {
        let shift = |delta: f64| {
            let mut t = Matrix3::identity();
            t[(axis, 2)] = delta;
            Homography::new(t * input_lin.homography.h)
        };
        let mut ip = input_lin;
        ip.homography = shift(hc);
        let mut im = input_lin;
        im.homography = shift(-hc);
        let ep = patch_lin.evaluate(&ip).unwrap();
        let em = patch_lin.evaluate(&im).unwrap();
        let col: Vec<f64> = eval_lin.d_res_d_corner.iter().map(|r| r[axis]).collect();
        check_vec(
            &format!("varpro d_res_d_corner[{axis}]"),
            &col,
            &ep.residuals,
            &em.residuals,
            hc,
        );
    }

    // 1c) d_res_d_sigma (`BlurKernel` stores f32 entries; `1/128` is an exact power of 2 in f32
    // and keeps `gauss_polygon::CUT = 5.0` tail steps negligible on the L2 normal equations and all
    // significant pixels).
    let hs = (1.0 / 128.0) as f32;
    for si in 0..3 {
        let tweak = |s: BlurKernel, d: f32| match si {
            0 => BlurKernel::new(s.xx + d, s.xy, s.yy),
            1 => BlurKernel::new(s.xx, s.xy + d, s.yy),
            _ => BlurKernel::new(s.xx, s.xy, s.yy + d),
        };
        let mut ip = input_lin;
        ip.kernel = tweak(input_lin.kernel, hs);
        let mut im = input_lin;
        im.kernel = tweak(input_lin.kernel, -hs);
        let ep = patch_lin.evaluate(&ip).unwrap();
        let em = patch_lin.evaluate(&im).unwrap();
        let col: Vec<f64> = eval_lin.d_res_d_sigma.iter().map(|r| r[si]).collect();
        let col_max = col.iter().map(|v| v.abs()).fold(0.0f64, f64::max);
        let mut diff_sq = 0.0f64;
        let mut ref_sq = 0.0f64;
        let mut max_rel_sig = 0.0f64;
        let mut sig_count = 0usize;
        let mut sig_pass = 0usize;
        for ((&a, &p), &m) in col.iter().zip(&ep.residuals).zip(&em.residuals) {
            let fd = (p - m) / (2.0 * f64::from(hs));
            let d = a - fd;
            diff_sq += d * d;
            ref_sq += a * a;
            if a.abs() >= 0.05 * col_max {
                let rel = d.abs() / a.abs().max(fd.abs());
                sig_count += 1;
                if rel < 5e-4 {
                    sig_pass += 1;
                }
                max_rel_sig = max_rel_sig.max(rel);
            }
        }
        let l2_rel = diff_sq.sqrt() / ref_sq.sqrt();
        assert!(
            l2_rel < 2e-4,
            "varpro d_res_d_sigma[{si}]: l2_rel={l2_rel:.3e}"
        );
        assert!(
            sig_pass * 100 >= sig_count * 98,
            "varpro d_res_d_sigma[{si}]: only {sig_pass}/{sig_count} < 5e-4"
        );
        assert!(
            max_rel_sig < 5e-3,
            "varpro d_res_d_sigma[{si}]: max_rel_sig={max_rel_sig:.3e}"
        );
    }

    // 1d) d_res_d_bleed
    let hb = 1e-6;
    let mut ip = input_lin;
    ip.bleed_kappa += hb;
    let mut im = input_lin;
    im.bleed_kappa -= hb;
    check_vec(
        "varpro d_res_d_bleed",
        eval_lin.d_res_d_bleed.as_ref().unwrap(),
        &patch_lin.evaluate(&ip).unwrap().residuals,
        &patch_lin.evaluate(&im).unwrap().residuals,
        hb,
    );

    // Case 2: FreeGamma + fixed_ac
    let opts_g = PhotometricFitOptions::default()
        .with_radiometric(RadiometricModel::FreeGamma { initial_gamma: 2.2 })
        .with_fit_ink_bleed(true);
    let pixels_g = render_pattern_patch(
        100,
        100,
        &pat,
        &hom_true,
        kernel_true,
        &opts_g,
        PatchRenderSpec {
            ac: [140.0, 42.0],
            illu: [0.06, -0.04],
            gamma: 2.1,
            bleed: 0.12,
            noise_amp: 0.0,
        },
    );
    let view_g = GrayImageView {
        width: 100,
        height: 100,
        data: &pixels_g,
    };
    let patch_g = PatchResidual::extract(&view_g, &pat, &hom_eval, 1.2, &opts_g).unwrap();
    let input_g = PatchEvalInput::new(hom_eval, BlurKernel::new(2.9, -0.2, 2.8))
        .with_gamma(2.05)
        .with_bleed_kappa(0.10)
        .with_fixed_ac([138.0, 43.0]);
    let eval_g = patch_g.evaluate(&input_g).expect("gamma eval");
    let hg = 1e-6;
    let mut ipg = input_g;
    ipg.gamma += hg;
    let mut img = input_g;
    img.gamma -= hg;
    check_vec(
        "gamma d_res_d_gamma",
        eval_g.d_res_d_gamma.as_ref().unwrap(),
        &patch_g.evaluate(&ipg).unwrap().residuals,
        &patch_g.evaluate(&img).unwrap().residuals,
        hg,
    );
}

fn rot_board(rx: f64, ry: f64, rz: f64) -> Matrix3<f64> {
    let (sx, cx) = rx.sin_cos();
    let (sy, cy) = ry.sin_cos();
    let (sz, cz) = rz.sin_cos();
    let mx = Matrix3::new(1.0, 0.0, 0.0, 0.0, cx, -sx, 0.0, sx, cx);
    let my = Matrix3::new(cy, 0.0, sy, 0.0, 1.0, 0.0, -sy, 0.0, cy);
    let mz = Matrix3::new(cz, -sz, 0.0, sz, cz, 0.0, 0.0, 0.0, 1.0);
    mz * my * mx
}

fn tilted_sensor_board_homography(
    defocus: &ThinLensDefocusModel,
    pose: &[f64; 6],
) -> (Matrix3<f64>, [f64; 3], f64) {
    let rb = rot_board(pose[0], pose[1], pose[2]);
    let tb = Vector3::new(pose[3], pose[4], pose[5]);
    let v0 = defocus.axial_image_distance_mm().unwrap();
    let (e1, e2, ns) = defocus.sensor_basis();
    let rs = Matrix3::from_columns(&[
        Vector3::new(e1[0], e1[1], e1[2]),
        Vector3::new(e2[0], e2[1], e2[2]),
        Vector3::new(ns[0], ns[1], ns[2]),
    ]);
    let p = defocus.pixel_pitch_mm;
    let g1 = e1[2];
    let g2 = e2[2];
    let nz = ns[2];
    let ks = Matrix3::new(
        v0 * nz / p,
        0.0,
        defocus.principal_point_px[0] - v0 * g1 / p,
        0.0,
        v0 * nz / p,
        defocus.principal_point_px[1] - v0 * g2 / p,
        0.0,
        0.0,
        1.0,
    );
    let rt = Matrix3::from_columns(&[rb.column(0).into_owned(), rb.column(1).into_owned(), tb]);
    let h_board = ks * rs.transpose() * rt;
    let plane_n = rb.column(2).into_owned();
    let plane_d = plane_n.dot(&tb);
    (h_board, [plane_n.x, plane_n.y, plane_n.z], plane_d)
}

#[test]
fn sparse_photometric_scheimpflug_ba_recovers_pose_focus_and_tilt() {
    let true_tau = [4.0_f64.to_radians(), -2.0_f64.to_radians()];
    let true_z0 = 290.0;
    let base_model = ThinLensDefocusModel::new(24.0, 2.0, true_z0, 0.005, [160.0, 120.0])
        .with_sensor_tilt_rad(true_tau);
    let true_pose = [
        25.0_f64.to_radians(),
        -18.0_f64.to_radians(),
        8.0_f64.to_radians(),
        0.0,
        0.0,
        272.0,
    ];
    let square_mm = 2.0;
    let (h_board_true, plane_n_true, plane_d_true) =
        tilted_sensor_board_homography(&base_model, &true_pose);

    // 3×3 well-separated patches on a 320×240 canvas rendered cleanly across
    // ±26 px per patch with exact Scheimpflug defocus covariance + spatial
    // scatter gradient.
    let pat_even = checker_pattern();
    let grid_coords: [(i32, i32); 9] = [
        (-2, -2),
        (0, -2),
        (2, -2),
        (-2, 0),
        (0, 0),
        (2, 0),
        (-2, 2),
        (0, 2),
        (2, 2),
    ];
    let mut img = vec![128u8; 320 * 240];
    let mut corner_homs_true = Vec::with_capacity(9);
    for &(gu, gv) in &grid_coords {
        let x_mm = f64::from(gu) * square_mm;
        let y_mm = f64::from(gv) * square_mm;
        let local_to_mm = Matrix3::new(square_mm, 0.0, x_mm, 0.0, square_mm, y_mm, 0.0, 0.0, 1.0);
        let hk = Homography::new(h_board_true * local_to_mm);
        let center = hk.apply(Point2::new(0.0, 0.0));
        let (pred, grad) = base_model
            .predict_plane_blur_gradient(center.x, center.y, plane_n_true, plane_d_true)
            .unwrap();
        let polys: Vec<BoardPolygon> = pat_even
            .polygons
            .iter()
            .map(|(v, w)| BoardPolygon {
                vertices: v.clone(),
                weight: *w,
            })
            .collect();
        let frame = Frame::new(&hk).unwrap();
        let sigma = [
            f64::from(pred.kernel.xx),
            f64::from(pred.kernel.xy),
            f64::from(pred.kernel.yy),
        ];
        let box_model = box_model_for(sigma);
        let sv = box_model.shift_var();
        let l = cholesky2([sigma[0] - sv, sigma[1], sigma[2] - sv]).unwrap();
        let dls = Model::dl_dsigma(l);
        let model = Model {
            polygons: &polys,
            c0: frame.c0,
            scale: frame.scale,
            box_model,
            fixed: [false; NP],
            pixel_bounds: [0.0, 0.0, 319.0, 239.0],
            radiometric: RadiometricModel::Linear,
            fit_ink_bleed: false,
            blur_gradient: Some(grad),
        };
        let mut theta = Vector::zeros();
        theta[I_SIGMA] = sigma[0];
        theta[I_SIGMA + 1] = sigma[1];
        theta[I_SIGMA + 2] = sigma[2];
        for k in 0..8 {
            theta[I_H + k] = frame.h[k];
        }
        let (pp, linv) = model.pass_polygons(&theta).unwrap();
        let sc = super::super::model::ScatterPass::new(grad, l, linv, dls);
        for dy in -26..=26 {
            for dx in -26..=26 {
                let x = center.x.round() as i32 + dx;
                let y = center.y.round() as i32 + dy;
                if (0..320).contains(&x) && (0..240).contains(&y) {
                    let px = Pixel { x, y, value: 0.0 };
                    let (b, _, _) = model.sample_pixel(&pp, Some(&sc), px, linv, false);
                    let v = 40.0 + 165.0 * (1.0 - b);
                    img[y as usize * 320 + x as usize] = v.round().clamp(1.0, 254.0) as u8;
                }
            }
        }
        corner_homs_true.push((center, hk));
    }

    let view = GrayImageView {
        width: 320,
        height: 240,
        data: &img,
    };
    let opts = PhotometricFitOptions::default();

    // Start from perturbed BA state: shifted pose + wrong Z0 (-15mm) + zero sensor tilt.
    let mut state = [
        true_pose[0] + 1.0_f64.to_radians(),
        true_pose[1] - 1.0_f64.to_radians(),
        true_pose[2] + 1.0_f64.to_radians(),
        true_pose[3] + 0.05,
        true_pose[4] - 0.05,
        true_pose[5] - 2.0,
        true_z0 - 15.0,
        0.0,
        0.0,
    ];

    let state_to_model = |s: &[f64; 9]| {
        ThinLensDefocusModel::new(24.0, 2.0, s[6], 0.005, [160.0, 120.0])
            .with_sensor_tilt_rad([s[7], s[8]])
    };
    let init_model = state_to_model(&state);
    let (h_init, _, _) = tilted_sensor_board_homography(
        &init_model,
        &[state[0], state[1], state[2], state[3], state[4], state[5]],
    );
    let patches: Vec<PatchResidual> = grid_coords
        .iter()
        .map(|&(gu, gv)| {
            let x_mm = f64::from(gu) * square_mm;
            let y_mm = f64::from(gv) * square_mm;
            let l2m = Matrix3::new(square_mm, 0.0, x_mm, 0.0, square_mm, y_mm, 0.0, 0.0, 1.0);
            let h0 = Homography::new(h_init * l2m);
            PatchResidual::extract(&view, &pat_even, &h0, 0.42, &opts).expect("extract patch")
        })
        .collect();

    // Joint damped Gauss-Newton / LM over all 9 patches and 9 state parameters.
    let mut lambda = 1e-3;
    for _ in 0..40 {
        let cur_model = state_to_model(&state);
        let (h_cur, pn, pd) = tilted_sensor_board_homography(
            &cur_model,
            &[state[0], state[1], state[2], state[3], state[4], state[5]],
        );
        let mut cost = 0.0;
        let mut h_mat = nalgebra::SMatrix::<f64, 9, 9>::zeros();
        let mut grad_vec = nalgebra::SVector::<f64, 9>::zeros();

        for (patch, &(gu, gv)) in patches.iter().zip(&grid_coords) {
            let x_mm = f64::from(gu) * square_mm;
            let y_mm = f64::from(gv) * square_mm;
            let l2m = Matrix3::new(square_mm, 0.0, x_mm, 0.0, square_mm, y_mm, 0.0, 0.0, 1.0);
            let hk = Homography::new(h_cur * l2m);
            let nom = patch.nominal_corner();
            let (pred, bgrad) = cur_model
                .predict_plane_blur_gradient(nom.x, nom.y, pn, pd)
                .unwrap();
            let input = PatchEvalInput::new(hk, pred.kernel).with_blur_gradient(bgrad);
            let eval = patch.evaluate(&input).expect("eval patch");
            cost += eval.cost;

            let mut dh_dp = [[0.0f64; 9]; 8];
            let mut dsig_dp = [[0.0f64; 9]; 3];
            for col in 0..9 {
                let eps = if col == 6 {
                    1e-2
                } else if (3..6).contains(&col) {
                    1e-4
                } else {
                    1e-5
                };
                let mut sp = state;
                let mut sm = state;
                sp[col] += eps;
                sm[col] -= eps;
                let eval_state_k = |st: &[f64; 9]| {
                    let m = state_to_model(st);
                    let (hg, pns, pds) = tilted_sensor_board_homography(
                        &m,
                        &[st[0], st[1], st[2], st[3], st[4], st[5]],
                    );
                    let hloc = Homography::new(hg * l2m);
                    let hnorm = patch.normalize_homography(&hloc).unwrap();
                    let (pr, _) = m
                        .predict_plane_blur_gradient(nom.x, nom.y, pns, pds)
                        .unwrap();
                    let sig = [
                        f64::from(pr.kernel.xx),
                        f64::from(pr.kernel.xy),
                        f64::from(pr.kernel.yy),
                    ];
                    (hnorm, sig)
                };
                let (hp, sigp) = eval_state_k(&sp);
                let (hm, sigm) = eval_state_k(&sm);
                for r in 0..8 {
                    dh_dp[r][col] = (hp[r] - hm[r]) / (2.0 * eps);
                }
                for r in 0..3 {
                    dsig_dp[r][col] = (sigp[r] - sigm[r]) / (2.0 * eps);
                }
            }

            for i in 0..eval.residuals.len() {
                let mut row = nalgebra::SVector::<f64, 9>::zeros();
                let jh = &eval.d_res_d_h[i];
                let js = &eval.d_res_d_sigma[i];
                for col in 0..9 {
                    let mut v = 0.0;
                    for r in 0..8 {
                        v += jh[r] * dh_dp[r][col];
                    }
                    for r in 0..3 {
                        v += js[r] * dsig_dp[r][col];
                    }
                    row[col] = v;
                }
                h_mat += row * row.transpose();
                grad_vec += row * eval.residuals[i];
            }
        }

        let mut damped = h_mat;
        for d in 0..9 {
            damped[(d, d)] += lambda * (h_mat[(d, d)].max(1e-4));
        }
        let Some(mut delta) = damped.lu().solve(&(-grad_vec)) else {
            lambda *= 10.0;
            continue;
        };
        let max_ang = delta[0]
            .abs()
            .max(delta[1].abs())
            .max(delta[2].abs())
            .max(delta[7].abs())
            .max(delta[8].abs());
        let mut step_scale = 1.0f64;
        if max_ang > 0.03 {
            step_scale = step_scale.min(0.03 / max_ang);
        }
        if delta[6].abs() > 8.0 {
            step_scale = step_scale.min(8.0 / delta[6].abs());
        }
        delta *= step_scale;

        let cand = std::array::from_fn(|i| state[i] + delta[i]);
        let cand_model = state_to_model(&cand);
        if cand_model.axial_image_distance_mm().is_some() {
            let (hc, pnc, pdc) = tilted_sensor_board_homography(
                &cand_model,
                &[cand[0], cand[1], cand[2], cand[3], cand[4], cand[5]],
            );
            let mut cand_cost = 0.0;
            let mut ok = true;
            for (patch, &(gu, gv)) in patches.iter().zip(&grid_coords) {
                let x_mm = f64::from(gu) * square_mm;
                let y_mm = f64::from(gv) * square_mm;
                let l2m = Matrix3::new(square_mm, 0.0, x_mm, 0.0, square_mm, y_mm, 0.0, 0.0, 1.0);
                let hk = Homography::new(hc * l2m);
                let nom = patch.nominal_corner();
                let Some((pred, bgrad)) =
                    cand_model.predict_plane_blur_gradient(nom.x, nom.y, pnc, pdc)
                else {
                    ok = false;
                    break;
                };
                let input = PatchEvalInput::new(hk, pred.kernel).with_blur_gradient(bgrad);
                if let Some(ev) = patch.evaluate(&input) {
                    cand_cost += ev.cost;
                } else {
                    ok = false;
                    break;
                }
            }
            if ok && cand_cost < cost {
                state = cand;
                lambda = (lambda * 0.3).max(1e-9);
                if delta.norm() < 1e-9 {
                    break;
                }
                continue;
            }
        }
        lambda *= 5.0;
    }

    let est_model = state_to_model(&state);
    let (h_est, _, _) = tilted_sensor_board_homography(
        &est_model,
        &[state[0], state[1], state[2], state[3], state[4], state[5]],
    );
    let mut max_corner_err = 0.0f64;
    for ((true_center, _), &(gu, gv)) in corner_homs_true.iter().zip(&grid_coords) {
        let x_mm = f64::from(gu) * square_mm;
        let y_mm = f64::from(gv) * square_mm;
        let est_center = Homography::new(h_est).apply(Point2::new(x_mm, y_mm));
        max_corner_err = max_corner_err.max((est_center - true_center).norm());
    }
    let z0_rel_err = (state[6] - true_z0).abs() / true_z0;
    let tau_x_err_deg = (state[7] - true_tau[0]).to_degrees().abs();
    let tau_y_err_deg = (state[8] - true_tau[1]).to_degrees().abs();
    assert!(max_corner_err < 0.05, "max_corner_err={max_corner_err:.4}");
    assert!(
        z0_rel_err < 0.02,
        "z0_rel_err={z0_rel_err:.4} (est={})",
        state[6]
    );
    assert!(tau_x_err_deg < 0.15, "tau_x_err_deg={tau_x_err_deg:.4}");
    assert!(tau_y_err_deg < 0.15, "tau_y_err_deg={tau_y_err_deg:.4}");
}
