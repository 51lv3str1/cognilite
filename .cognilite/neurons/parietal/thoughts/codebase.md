# Codebase ingestion protocol

When the user says "read the project", "understand the codebase", "become an expert", or similar — follow this exact protocol. Do not improvise the order. Do not loop in thinking deciding what to read next.

**The goal is not to read everything. The goal is to answer questions.**

---

## Step 1 — Identify the project type (1 command)

```
read_file Cargo.toml
```
or `package.json`, `pyproject.toml`, `go.mod`, etc. This tells you: language, name, main dependencies. Done in one read.

## Step 2 — Map the file surface (1 command)

```
glob_files *.rs
```
or `*.py`, `*.go`, etc. You now have the full file list. **Do not read any file yet.**

## Step 3 — Read the entry point (1 read)

```
read_file src/main.rs
```
or `src/lib.rs`, `main.py`, `cmd/main.go`. The entry point tells you: what modules exist, how they connect, what the program does at startup. This is the most important file. Read it fully.

## Step 4 — Read the central state type (1 read)

Every non-trivial program has one struct/class that owns most of the state. Find it:

```
grep_files "^pub struct\|^struct\|^class " src/
```

Read the file containing the main state type — just the struct definition and its `impl` method names, not the full implementations. This tells you what data the program manages.

## Step 5 — Summarize before continuing

After steps 1–4, **stop and write a summary**:
- What does this project do?
- What are the main types / modules?
- What is the data flow at a high level?

This forces you to consolidate what you know before reading more. If you can't answer these questions, re-read the entry point — don't read more files.

---

## Reading more (only when needed)

After step 5, read additional files **only to answer a specific question**. Never "to be thorough."

```
grep_files "fn send_message" src/     # find where something lives
read_file src/app.rs 450 520          # read only that region
```

**Stop reading when you can answer the user's question.** You don't need to read every file.

---

## Anti-loop rules

These are the most common thinking traps — recognize them and break out immediately:

**"Should I read file A or file B first?"**
→ Read the smaller one. Or grep both for the key term. Pick and act, don't deliberate.

**"I'm not sure if I have enough context yet."**
→ Write what you know. Gaps will appear when you try to explain. Read to fill gaps, not preemptively.

**"I already read this, but let me re-check."**
→ Trust your previous read. Only re-read if you have a specific contradiction to resolve.

**"This is a large codebase, I need to read more."**
→ No. Do step 5 first. Most questions can be answered from the entry point + main type alone.

**"I need to understand the full flow before I can answer."**
→ Answer with what you have, flag what you're uncertain about. Then read to resolve the uncertainty.

**The rule: when you notice yourself re-reading the same question in your thinking, stop thinking and run a command instead.**

---

## Architecture patterns to recognize

Once you've done steps 1–5, classify the architecture. This shapes how you reason about changes:

**Event loop**: one central loop that polls state and dispatches events (e.g. TUI apps, game loops). Changes propagate by mutating shared state, not by calling functions directly.

**Actor / channel**: components communicate via message queues. Side effects are isolated. To trace a behavior, follow the channel, not the call stack.

**Request/response**: stateless handlers, each request is independent. State lives in a database or external service.

**Plugin / neuron system**: behavior is composed from loaded modules. Adding a feature means adding a module, not touching core code.

When you recognize the pattern, say so explicitly — it tells the user you actually understand the structure.
