# Architecture design

When the user asks you to design or evaluate an architecture — for a new feature, a new system, or a refactor — follow this protocol.

---

## Before designing anything, answer these three questions

**1. What problem does this solve?**
One sentence. If you can't write it in one sentence, ask the user to clarify.

**2. What already exists that could solve it?**
Grep the codebase for related types, functions, or patterns. The best architecture reuses what's there.

**3. Where does new data live?**
Every feature needs state somewhere. Identify the existing type that should own it before proposing a new one.

---

## Design output format

When proposing an architecture, always include:

1. **Data model** — what structs/types change or get added, and where they live
2. **Entry point** — what triggers the new behavior (user action, event, API call)
3. **Data flow** — how data moves from trigger → processing → output, step by step
4. **Integration points** — exactly which existing functions/methods get called or modified
5. **What stays the same** — explicitly state what you're NOT changing

Skip diagrams unless the user asks. Words are faster and more precise.

---

## Evaluating a proposed design

When reviewing an architecture (yours or the user's), check:

- **Ownership**: is it clear which module/type owns each piece of state? Shared mutable state is a bug waiting to happen.
- **Data flow direction**: does data flow one way (input → transform → output) or do components call each other in circles? Cycles create debugging nightmares.
- **Blast radius**: if this component fails, what else breaks? A good architecture contains failures.
- **Reversibility**: how hard is it to undo this decision in 6 months? Prefer reversible designs.

Flag problems directly — "this will cause a deadlock when X and Y both hold the lock" is more useful than "this might have concurrency issues."

---

## Common architecture mistakes to call out

**God struct**: one struct that knows everything and does everything. Fix: split by responsibility.

**Premature abstraction**: an interface with one implementation. Fix: write the concrete type first; extract the trait when you have two implementations.

**Hidden coupling**: module A imports module B imports module A. Fix: introduce a shared type or event that both modules depend on.

**State duplication**: the same data stored in two places that can get out of sync. Fix: one source of truth, derived values computed on read.

**Wrong layer**: business logic in the UI handler, or rendering logic in the data model. Fix: identify the layer boundary and move the code.
