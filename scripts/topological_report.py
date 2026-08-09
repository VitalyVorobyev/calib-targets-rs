"""Generate the local, dependency-free topological campaign report."""

from __future__ import annotations

import json
import os
from pathlib import Path
from typing import Any


def read_json(path: Path) -> dict[str, Any] | None:
    if not path.is_file():
        return None
    return json.loads(path.read_text(encoding="utf-8"))


def write_report(root: Path, config: dict[str, Any], config_path: Path, out_path: Path) -> None:
    variants = []
    for variant in config["variants"]:
        overlay_root = root / config["output"]["overlays"] / variant["id"]
        run_root = root / config["output"]["runs"] / variant["id"]
        manifest = read_json(overlay_root / "manifest.json")
        if manifest:
            for image in manifest.get("images", []):
                directory = Path(image["output_dir"])
                if not directory.is_absolute():
                    directory = root / directory
                image["overlays"] = [
                    os.path.relpath(directory / stage, out_path.parent)
                    for stage in image.get("stages", [])
                ]
        variants.append(
            {
                **variant,
                "manifest": manifest,
                "timing": read_json(run_root / "timing.json"),
                "quality": read_json(run_root / "quality.json"),
            }
        )

    by_id = {variant["id"]: variant for variant in variants}
    recovery_budget = []
    enabled = by_id.get("ring", {}).get("timing")
    disabled = by_id.get("ring-without-geometry-recovery", {}).get("timing")
    if enabled and disabled:
        disabled_images = {image["image"]: image for image in disabled.get("images", [])}
        for image in enabled.get("images", []):
            baseline = disabled_images.get(image["image"])
            if not baseline:
                continue
            enabled_p50 = image["summary"]["full_total"]["p50_ms"]
            disabled_p50 = baseline["summary"]["full_total"]["p50_ms"]
            budget_ms = max(0.05, 0.05 * disabled_p50)
            recovery_budget.append(
                {
                    "image": image["image"],
                    "enabled_p50_ms": enabled_p50,
                    "disabled_p50_ms": disabled_p50,
                    "delta_ms": enabled_p50 - disabled_p50,
                    "budget_ms": budget_ms,
                    "passed": enabled_p50 - disabled_p50 <= budget_ms,
                }
            )

    payload = {
        "schema": 1,
        "config": os.path.relpath(config_path, out_path.parent),
        "images": config["images"],
        "variants": variants,
        "determinism": read_json(root / config["output"]["determinism"]),
        "geometry_recovery_budget": recovery_budget,
    }
    embedded = json.dumps(payload, separators=(",", ":")).replace("</", "<\\/")
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(_document(embedded), encoding="utf-8")
    print(f"wrote {out_path.relative_to(root)}")


def _document(data: str) -> str:
    return f"""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>Topological Grid · Performance Review</title>
<style>
:root {{ color-scheme:dark; --bg:#060a14; --card:#0b1426; --edge:#1b2740; --track:#0e1726;
  --ink:#d8e3f2; --muted:#8294b0; --blue:#5b9dff; --amber:#f2a93d; --teal:#33c6e3;
  --green:#3ddc84; --rose:#ff6b6b; --violet:#bb86fc; }}
* {{ box-sizing:border-box }}
body {{ margin:0; color:var(--ink); font:14px/1.5 ui-sans-serif,system-ui,-apple-system,"Segoe UI",sans-serif;
  background:radial-gradient(1000px 650px at 12% -10%,rgba(91,157,255,.13),transparent 60%),var(--bg) }}
.wrap {{ max-width:1220px; margin:auto; padding:30px 24px 80px }}
h1 {{ margin:0; font-size:30px; letter-spacing:-.025em }}
h2 {{ margin:38px 0 13px; color:var(--muted); font-size:12px; text-transform:uppercase; letter-spacing:.1em }}
h3 {{ margin:0 0 6px; font-size:17px }}
.sub,.muted {{ color:var(--muted) }} .sub {{ max-width:80ch; margin:7px 0 18px }}
.chips,.tabs {{ display:flex; flex-wrap:wrap; gap:8px; margin:14px 0 }}
.chip,.tab {{ border:1px solid var(--edge); border-radius:999px; padding:6px 11px; background:var(--card); color:var(--muted) }}
.tab {{ cursor:pointer }} .tab.active {{ border-color:var(--blue); color:var(--ink); background:rgba(91,157,255,.14) }}
.grid {{ display:grid; grid-template-columns:repeat(auto-fit,minmax(330px,1fr)); gap:14px }}
.card {{ background:var(--card); border:1px solid var(--edge); border-radius:14px; padding:18px; overflow:hidden }}
.callout {{ border-left:3px solid var(--amber) }}
table {{ width:100%; border-collapse:collapse; font-size:12.5px }}
th,td {{ padding:8px 7px; border-bottom:1px solid var(--edge); text-align:left }}
th {{ color:var(--muted); font-size:10.5px; text-transform:uppercase; letter-spacing:.05em }}
.num {{ text-align:right; font-variant-numeric:tabular-nums }}
.pass {{ color:var(--green) }} .fail {{ color:var(--rose) }}
.barrow {{ display:grid; grid-template-columns:145px 1fr 82px; gap:9px; align-items:center; margin:5px 0 }}
.label {{ color:var(--muted); text-align:right; white-space:nowrap; overflow:hidden; text-overflow:ellipsis }}
.track {{ height:15px; background:var(--track); border-radius:5px; overflow:hidden }}
.fill {{ height:100%; min-width:1px; background:var(--blue) }}
.value {{ text-align:right; font-variant-numeric:tabular-nums }}
.config {{ display:grid; grid-template-columns:repeat(auto-fit,minmax(250px,1fr)); gap:7px 18px; margin-top:13px }}
.config div {{ display:flex; justify-content:space-between; gap:12px; border-bottom:1px solid var(--edge); padding:4px 0 }}
.config code {{ color:var(--ink); text-align:right; overflow-wrap:anywhere }}
.gallery {{ display:grid; grid-template-columns:repeat(auto-fit,minmax(260px,1fr)); gap:12px }}
figure {{ margin:0; background:var(--card); border:1px solid var(--edge); border-radius:12px; overflow:hidden }}
figure img {{ display:block; width:100%; height:auto; background:var(--track) }}
figcaption {{ padding:8px 10px; color:var(--muted); font-size:11px; overflow-wrap:anywhere }}
code {{ font-family:ui-monospace,SFMono-Regular,Menlo,monospace }}
@media(max-width:650px) {{ .barrow {{ grid-template-columns:105px 1fr 70px }} .wrap {{ padding:20px 12px 60px }} }}
</style>
</head>
<body><main class="wrap">
<h1>Topological Grid · Performance Review</h1>
<p class="sub">One local report for exact Rust traces, quality checkpoints and production detector timing. All data and UI code are embedded; overlay images are local relative links.</p>
<div id="chips" class="chips"></div>
<section class="card callout">
  <h3>GeminiChess1 lower-left junction</h3>
  <div class="muted">RingFit finds raw corner <code>124</code> at the correct pixel position, but its local axes miss global orientation clustering. The generic grid therefore remains 52/53. The conservative geometry-only chessboard pass uses an existing L-shaped neighbourhood and restores it as <code>(0,6)</code>; the three former upper-left “truth” sites are board edges, not X-junctions.</div>
</section>
<h2>Variant comparison</h2><div id="comparison" class="card"></div>
<h2>Geometry recovery budget</h2><div id="budget" class="card"></div>
<h2>Variant details</h2><div id="tabs" class="tabs"></div>
<div id="effective"></div>
<div id="quality"></div>
<div id="timing"></div>
<h2>Blog overlays</h2><div id="gallery" class="gallery"></div>
</main>
<script id="campaign-data" type="application/json">{data}</script>
<script>
const D=JSON.parse(document.getElementById('campaign-data').textContent);
const fmt=(v,d=3)=>v==null?'—':Number(v).toFixed(d);
const pct=v=>v==null?'—':(100*Number(v)).toFixed(1)+'%';
const esc=s=>String(s).replace(/[&<>"']/g,c=>({{'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}}[c]));
const primary=D.variants.find(v=>v.primary)||D.variants[0];
const meta=primary?.timing?.metadata;
const chips=document.getElementById('chips');
[['config',D.config],['revision',meta?.git_sha?.slice(0,12)],['dirty digest',meta?.dirty_state_sha256?.slice(0,12)],['CPU',meta?.cpu],['rustc',meta?.rustc],['profile',meta?.profile],['repeats',meta?.repeats],['deterministic',D.determinism?.passed]].forEach(([k,v])=>{{if(v!=null)chips.insertAdjacentHTML('beforeend',`<span class="chip"><b>${{esc(k)}}:</b> ${{esc(v)}}</span>`)}});

function imageMap(variant) {{ return new Map((variant.timing?.images||[]).map(x=>[x.image,x])); }}
function qualityMap(variant) {{ return new Map((variant.quality?.images||[]).map(x=>[x.image,x])); }}
function renderComparison() {{
  let html='<table><thead><tr><th>Variant</th><th>Image</th><th class="num">labels</th><th class="num">recall</th><th class="num">wrong / false</th><th class="num">full p50</th><th class="num">full p95</th></tr></thead><tbody>';
  for(const v of D.variants) {{ const qm=qualityMap(v); for(const t of v.timing?.images||[]) {{ const q=qm.get(t.image)?.final_public; html+=`<tr><td>${{esc(v.id)}}</td><td>${{esc(t.image)}}</td><td class="num">${{t.labelled_count}}</td><td class="num">${{q?pct(q.recall):'unscored'}}</td><td class="num ${{q&&(q.wrong_labels||q.false_labelled_features)?'fail':'pass'}}">${{q?`${{q.wrong_labels}} / ${{q.false_labelled_features}}`:'—'}}</td><td class="num">${{fmt(t.summary.full_total.p50_ms)}} ms</td><td class="num">${{fmt(t.summary.full_total.p95_ms)}} ms</td></tr>`; }} }}
  document.getElementById('comparison').innerHTML=html+'</tbody></table>';
}}
function renderBudget() {{
  if(!D.geometry_recovery_budget.length) {{ document.getElementById('budget').innerHTML='<span class="muted">Run both Ring variants to calculate the ablation budget.</span>'; return; }}
  let html='<table><thead><tr><th>Image</th><th class="num">enabled p50</th><th class="num">disabled p50</th><th class="num">delta</th><th class="num">budget</th><th>gate</th></tr></thead><tbody>';
  for(const x of D.geometry_recovery_budget) html+=`<tr><td>${{esc(x.image)}}</td><td class="num">${{fmt(x.enabled_p50_ms)}} ms</td><td class="num">${{fmt(x.disabled_p50_ms)}} ms</td><td class="num">${{fmt(x.delta_ms)}} ms</td><td class="num">${{fmt(x.budget_ms)}} ms</td><td class="${{x.passed?'pass':'fail'}}">${{x.passed?'pass':'fail'}}</td></tr>`;
  document.getElementById('budget').innerHTML=html+'</tbody></table>';
}}

const stages=['corner_detection','input_adaptation','axis_filter','triangulation','edge_classification','triangle_merge','topological_filter','geometry_filter','cell_size_filter','walk','component_merge','validation','projective_fit','assembly','clustering','recovery','geometry_only_recovery','final_geometry_gate','output_assembly','chessboard_postprocessing','grid_total','full_total'];
const observationSpan={{corner_detection:'detect_corners',input_adaptation:'topological_inputs',axis_filter:'usable_mask',triangulation:'delaunay_triangulate',edge_classification:'classify_all_edges',triangle_merge:'merge_triangle_pairs',topological_filter:'topological_quad_filter',geometry_filter:'geometry_quad_filter',cell_size_filter:'cell_size_quad_filter',walk:'label_components',component_merge:'topological_component_merge',validation:'topological_validation',projective_fit:'topological_projective_fit',assembly:'topological_assembly',clustering:'topological_clustered_augs',recovery:'recover_topological_components',geometry_only_recovery:'chessboard_geometry_only_recovery',final_geometry_gate:'chessboard_final_geometry_gate',output_assembly:'build_topological_detections',grid_total:'detect_all_topological'}};
function renderVariant(v) {{
  const params=v.manifest?.params||v.timing?.metadata||{{}};
  let effective='<section class="card"><h3>Effective configuration</h3><div class="config">';
  for(const key of Object.keys(params).sort()) effective+=`<div><span class="muted">${{esc(key)}}</span><code>${{esc(params[key])}}</code></div>`;
  effective+='</div></section>';
  document.getElementById('effective').innerHTML=effective;
  const qrows=v.quality?.images||[]; let quality='<h2>Quality checkpoints</h2>';
  if(!qrows.length) quality+='<div class="card muted">No ground truth for this selection — timing and overlays remain available.</div>';
  else {{ quality+='<div class="grid">'; for(const row of qrows) {{ quality+=`<div class="card"><h3>${{esc(row.image)}}</h3><table><thead><tr><th>checkpoint</th><th class="num">correct</th><th class="num">recall</th><th class="num">wrong</th><th class="num">false</th><th class="num">holes</th><th class="num">components</th></tr></thead><tbody>`; for(const [name,key] of [['generic','generic_projective_grid'],['recovered','after_chessboard_recovery'],['public','final_public']]) {{ const x=row[key]; quality+=`<tr><td>${{name}}</td><td class="num">${{x.correct_labels}}/${{x.visible_ground_truth}}</td><td class="num">${{pct(x.recall)}}</td><td class="num">${{x.wrong_labels}}</td><td class="num">${{x.false_labelled_features}}</td><td class="num">${{x.holes_within_component_bboxes}}</td><td class="num">${{x.components}}</td></tr>`; }} const a=row.stage_attribution; const fits=(row.generic_fit_residuals||[]).map(x=>`n=${{x.count}} mean=${{fmt(x.mean_px)}} px max=${{fmt(x.max_px)}} px`).join(' · ')||'unavailable'; quality+=`</tbody></table><p class="muted">generic fit: ${{esc(fits)}}<br>axis additions: ${{esc(a.axis_aware_recovery_additions.join(', ')||'none')}} · geometry additions: ${{esc(a.geometry_only_recovery_additions.join(', ')||'none')}} · final drops: ${{esc(a.final_drops_or_refusal.join(', ')||'none')}}</p></div>`; }} quality+='</div>'; }}
  document.getElementById('quality').innerHTML=quality;
  let timing='<h2>Per-stage timing</h2><div class="grid">'; for(const img of v.timing?.images||[]) {{ timing+=`<div class="card"><h3>${{esc(img.image)}}</h3><p class="muted">${{img.width}}×${{img.height}} · ${{img.raw_corners}} raw · ${{img.labelled_count}} labelled</p><table><thead><tr><th>stage</th><th class="num">p50</th><th class="num">p95</th><th class="num">mean</th><th class="num">max</th><th class="num">seen</th></tr></thead><tbody>`; for(const stage of stages) {{ const stat=img.summary[stage]; if(!stat)continue; const span=observationSpan[stage]; const seen=span?img.stage_observations?.[span]:'wall'; timing+=`<tr><td>${{esc(stage)}}</td><td class="num">${{fmt(stat.p50_ms)}}</td><td class="num">${{fmt(stat.p95_ms)}}</td><td class="num">${{fmt(stat.mean_ms)}}</td><td class="num">${{fmt(stat.max_ms)}}</td><td class="num">${{seen??0}}</td></tr>`; }} timing+='</tbody></table></div>'; }} timing+='</div>'; document.getElementById('timing').innerHTML=timing;
  let gallery=''; for(const img of v.manifest?.images||[]) for(let i=0;i<(img.overlays||[]).length;i++) gallery+=`<figure><a href="${{esc(img.overlays[i])}}"><img loading="lazy" src="${{esc(img.overlays[i])}}" alt="${{esc(img.image)}} stage ${{i}}"></a><figcaption>${{esc(img.image)}} · ${{esc(img.stages[i])}}</figcaption></figure>`; document.getElementById('gallery').innerHTML=gallery||'<div class="muted">No overlays generated.</div>';
}}

function select(id) {{ document.querySelectorAll('.tab').forEach(x=>x.classList.toggle('active',x.dataset.id===id)); renderVariant(D.variants.find(v=>v.id===id)); }}
for(const v of D.variants) {{ const b=document.createElement('button'); b.className='tab'; b.dataset.id=v.id; b.textContent=v.id+(v.primary?' · primary':''); b.onclick=()=>select(v.id); document.getElementById('tabs').appendChild(b); }}
renderComparison(); renderBudget(); select(primary.id);
</script></body></html>"""
