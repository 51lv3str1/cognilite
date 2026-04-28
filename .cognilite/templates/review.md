Run an architectural review of the project in the working directory.

<load_neuron>Hippocampus</load_neuron>

Follow the Hippocampus protocol:
1. Map the structure (use the injected `<project_map>` if present, or `<tool>tree</tool>`).
2. Read manifests and the top 3-5 files by LOC.
3. Emit one `<finding>` per concrete problem, with `severity`, `file:line`, `category`.
4. Close with an executive summary: top-3 debts, top-3 features, and what NOT to touch.

Be critical but actionable. Every finding cites file and line — no empty citations.
