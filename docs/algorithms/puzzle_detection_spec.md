Approach: Robust PuzzleBoard Matching by Soft Edge Decoding and Global Graph Inference

Goal

Estimate the correct global PuzzleBoard assignment from a partially observed, noisy corner graph under low resolution, blur, missing corners, and local graph errors.

The approach must prefer:

* global consistency over local hard decisions
* soft evidence over binary decoding
* rejection of ambiguous matches over confident wrong matches

⸻

Core idea

PuzzleBoard information is carried by the bits associated with edges between neighboring corners.
Therefore, matching should not be based on isolated corners or on a raw raster correlation of the whole observation.

Instead, the observation is treated as a sparse graph with uncertain edge bits, and board matching is formulated as a global hypothesis scoring problem.

The method consists of three conceptual stages:

1. soft edge decoding
    infer probabilistic bit evidence for observed graph edges
2. local hypothesis generation
    use small reliable neighborhoods to propose plausible board positions and rotations
3. global board inference
    score each candidate hypothesis against the full observed graph and choose the most consistent one

⸻

Representation

The observed target is represented as:

* a set of detected corners
* a local graph connecting neighboring corners
* a classification of edges into horizontal and vertical directions
* for each edge, a soft estimate of the encoded bit

The known PuzzleBoard target is represented as:

* a fixed lattice
* known horizontal and vertical edge-bit maps
* the four valid board rotations
* the local code structure that makes neighborhoods identifiable

⸻

Soft edge decoding

Each observed graph edge is assigned a probability of bit value 0 or 1, rather than a hard binary decision.

This step should use local photometric evidence around the edge and produce:

* a bit likelihood
* a confidence value

The purpose is to preserve uncertainty.
A weak or blurred edge should contribute weak evidence, not a potentially destructive hard error.

⸻

Local hypothesis generation

Small local neighborhoods in the observed graph are used as anchors.

Each anchor provides a partial code signature derived from its horizontal and vertical edges.
That signature is compared against the known PuzzleBoard structure to generate a small set of plausible:

* board rotations
* board offsets / indexings

The purpose of this stage is not to solve the problem completely, but to reduce the search to a manageable set of globally plausible hypotheses.

⸻

Global board inference

Each candidate hypothesis is evaluated against all observed edges.

For a given hypothesis, each observed edge implies an expected PuzzleBoard bit.
The observed soft edge evidence is then compared to that expected bit.

The score of a hypothesis is the sum of all weighted edge contributions:

S(H) = \sum_{e} w_e \log p_e(b_e(H))

where:

* H is a global board hypothesis
* e is an observed edge
* b_e(H) is the expected bit under that hypothesis
* p_e(\cdot) is the observed soft bit likelihood
* w_e is the edge confidence

This gives a board-level likelihood-like score.

The best solution is the hypothesis with the strongest global support across the observed graph, not the one that happens to explain one local patch well.

⸻

Robustness principles

The method should explicitly tolerate:

* missing corners
* missing edges
* uncertain edge orientation labels
* incorrect local graph links
* local photometric failures
* partial visibility of the board

A small number of incorrect edges must not dominate the result.
Weak or unreliable observations should be down-weighted or ignored.

The preferred failure mode is:

* ambiguous / reject

rather than:

* confidently wrong assignment

⸻

Role of repeated structure

If the PuzzleBoard contains repeated code structure, that repetition is treated as a source of redundancy and error correction.

Repeated edge information should strengthen consistent hypotheses and reduce sensitivity to individual decoding errors.

The repeated structure is therefore not just a property of the board design; it is part of the inference signal.

⸻

Final interpretation

Once the best global hypothesis is selected, it defines:

* the board rotation
* the board indexing / offset
* the board coordinates of the observed corners
* the expected bit values on observed edges

This turns the matching problem into a globally constrained assignment problem, where local uncertainties are resolved through consistency with the whole board.

⸻

Summary

This approach replaces brittle hard matching with a probabilistic, board-level interpretation of the observed corner graph.

Its key principles are:

* decode edge bits softly
* generate hypotheses from local neighborhoods
* select the solution by global graph consistency

In short:

PuzzleBoard matching should be treated as inference on a sparse noisy code graph, not as local hard decoding and not as raw image correlation.