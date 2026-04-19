# Board-level ChArUco marker detection (concept)

Setup

Let the chessboard graph give you a set of observed marker-cell candidates

\mathcal{C} = \{c_1,\dots,c_N\}

For each observed cell c_i, you rectify a patch I_i.

The board is known. For each board marker position p, you know its true marker code m_p.
A global board hypothesis H is a mapping from observed cells c_i to board positions p_i(H), including board rotation and origin/indexing choice.

So under H, cell c_i is supposed to contain marker m_{p_i(H)}.

Per-cell score

For each observed patch I_i and each possible expected marker m, define a match score

s_i(m) = \max_{r \in \{0,1,2,3\}} s(I_i, T(m,r))

where:

* T(m,r) is the ideal template for marker m at rotation r
* s(\cdot,\cdot) is a similarity score

Good practical choices for s:

* normalized cross-correlation on normalized patches
* negative SSD after normalization
* bit-likelihood score if you decode soft bits
* later: CNN log-likelihood

If orientation is already approximately known from the graph, do not maximize over all 4 rotations blindly; only test the small plausible set.

Global board score

For a board hypothesis H,

S(H) = \sum_{i \in \mathcal{V}(H)} w_i \, s_i\!\big(m_{p_i(H)}\big)

where:

* \mathcal{V}(H) = observed cells that map to valid board marker positions under H
* w_i = confidence weight for cell i

Examples for w_i:

* 1 by default
* lower if rectification is poor
* lower if patch contrast is weak
* lower if patch is near image boundary

Then choose

\[
H^\* = \arg\max_H S(H)
\]

Since the hypothesis space is tiny, brute force is fine.

Robustification

To avoid one bad patch dominating, use clipped scores:

S(H) = \sum_i w_i \, \phi\!\left(s_i(m_{p_i(H)})\right)

with

\phi(s) = \min(\max(s, a), b)

or simply threshold bad matches away.

You can also ignore cells whose best score is below a minimum confidence.

Optional probabilistic form

If per-cell decoder returns probabilities,

P_i(m) = P(\text{marker}=m \mid I_i)

then use log-likelihood:

S(H) = \sum_i w_i \log P_i\!\big(m_{p_i(H)}\big)

This is cleaner than raw correlation and is the natural next step if you later add a CNN.

After the best hypothesis

Once H^\* is chosen, every cell has an expected marker. Then do a constrained refinement:

For each cell c_i,

\[
\hat r_i = \arg\max_r s(I_i, T(m_{p_i(H^\*)}, r))
\]

and optionally refine tiny local warp / shift parameters \theta_i:

\[
\hat\theta_i = \arg\max_\theta s(I_i, W(T(m_{p_i(H^\*)}, \hat r_i), \theta))
\]

This often rescues marginal cases.

Structure

Implementation-wise:

1. From graph
    * extract candidate marker cells
    * rectify each patch
    * compute confidence w_i
2. Per-cell matcher
    * compare each patch against valid marker templates
    * store score table
        M[i,m] = s_i(m)
3. Hypothesis generator
    * enumerate board rotations
    * enumerate possible origin/index assignments if needed
4. Board scorer
    * for each H, map each observed cell i to expected marker m_{p_i(H)}
    * sum weighted scores from M[i,m]
5. Select best
    * take top hypothesis
    * check margin to second best
6. Constrained re-decode
    * verify/refine each marker using only expected identity under best board hypothesis

The key simplification

The whole method reduces to this:

* precompute a matrix of per-cell vs per-marker scores
* each board hypothesis just selects one entry per cell and sums them
