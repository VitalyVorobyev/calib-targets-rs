# Evidence-driven detector debugging

When investigating a detector failure on a real image, every conclusion must be
tied to a **measurable number** from the diagnose JSON or a **verifiable spatial
fact** about the corners. Plausible-sounding narratives without per-corner /
per-stage evidence are not acceptable substitutes for data, and "shipping fixes
against bench" without an independent geometry check has produced false
positives in this codebase.

## Required investigation pattern for any detector failure

1. **Atomic-stage breakdown first.** Before proposing any fix, dump the
   `DebugFrame` (`bench diagnose --dump-frame …`) and walk every stage: per
   stage, list inputs, outputs, and the specific decision that dropped each
   corner. Counts per stage (Strong / Clustered / NoCluster / Labeled / Raw),
   per-corner `max_d`, per-rejection-reason buckets in `extension` / `rescue`,
   and per-corner spatial coordinates relative to the labelled bbox — these are
   the currency of analysis.

2. **Don't trust `bench check` `pos=` for new labels.** The `pos=` counter only
   verifies positions of corners present in the baseline — not the correctness
   of new `(i, j)` assignments. A change can ship `extra=22 pos=0` while
   introducing diagonal edges and wrong `(i, j)` labels. Always inspect the
   rendered overlay visually for crossing edges or non-cardinal connections,
   *and* run the geometry check below before claiming a fix is precision-safe.

3. **Distinguish hypothesis from data.** A hypothesis becomes a fact only after
   explicit verification — e.g., "marker corners bias the cluster centres" must
   be confirmed by stratifying corners by parity and showing the asymmetry in
   `max_d`, *or* by recomputing centres from labelled axes only and showing the
   parity-B failures resolve.

4. **Geometry check is mandatory before returning a detection.** Every labelled
   corner must satisfy a per-edge length + axis-slot-swap parity check against
   its cardinal labelled neighbours, plus a global / local homography residual
   gate. Detect first, *then* verify by an independent geometric predicate the
   BFS / rescue path didn't already enforce. False detections are unrecoverable
   for downstream calibration; missing corners are not.
