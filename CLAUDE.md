# Claude instructions for this project

## Maintaining README.md

When adding a new API endpoint:

1. Add handler to the appropriate route file under `crates/api/src/routes/`.
2. Register the route in `crates/api/src/routes/mod.rs`.
3. Add a `#[utoipa::path]` annotation (minimal style — see `feedback_utoipa_style.md` in memory).
4. **Update `README.md` → "API endpoints (v1)" section**: add a row to the correct table with Method, Path, Auth (`—` or `Bearer`), and a one-line description. Keep tables sorted by path within each group.

Same rule applies when renaming or deleting endpoints — update or remove the corresponding row.


## Planning Workflow (MANDATORY)

You must follow a strict two-phase workflow for every non-trivial task.

### PHASE 1 — PLANNING
- Analyze the request carefully.
- Break the task into clear, logical steps.
- Identify assumptions, edge cases, and risks.
- Propose a structured plan.
- DO NOT write final code yet.
- Ask for user approval before proceeding.

### PHASE 2 — EXECUTION
- Only start after explicit approval.
- Implement step-by-step according to the plan.
- If anything changes, pause and ask.

### RULES
- Never skip the planning phase unless user says "skip planning".
- Never mix planning and implementation.
- If you violate this, stop and restart from planning.