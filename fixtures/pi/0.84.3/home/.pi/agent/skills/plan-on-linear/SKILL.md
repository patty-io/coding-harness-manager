---
name: plan-on-linear
description: Use when you have a spec or feature idea and want to capture it as a structured plan issue in Linear (team PAT, "Plan" label) instead of writing a markdown plan file. Linear-bound analog of writing-plans.
---

> **Related skills:** `/skill:writing-plans` writes the same kind of plan to a markdown file. This skill writes it to a Linear issue instead. Did you `/skill:brainstorming` first? Ready to implement? Use `/skill:executing-plans` or `/skill:subagent-driven-development`.

# Plan on Linear

## Overview

Turn a spec / feature / idea into a **Linear issue** on team **PAT (Patty)** that carries a full implementation plan in its description. This is the Linear-bound counterpart to `writing-plans`: instead of saving `docs/plans/YYYY-MM-DD-*.md`, you create an issue a stranger could pick up with zero codebase context.

**Announce at start:** "I'm using the plan-on-linear skill to create a Linear plan issue."

**Prerequisite:** You need Linear authenticated (`/linear-auth` or `linear_configure_auth`). If `linear_whoami` fails, stop and auth first.

## Issue Properties (defaults)

| Property | Value | Notes |
|---|---|---|
| **Team** | `b3c4cebb-2ee9-4f59-a475-2401eca57626` (PAT / Patty) | Hardcoded — this is Patrick's planning team |
| **Label** | `Plan` | Resolve by name at runtime (see step 3). Fallback: omit + warn |
| **Title** | descriptive goal sentence | Not a slug. A stranger must understand the goal (see step 1) |
| **Description** | structured plan in Markdown | Goal / Approach / Tasks / Acceptance / Files (see step 2) |
| **Priority** | `3` (Medium) | Override to `2` (High) or `4` (Low) only if warranted |
| **State** | `Todo` (`f18b7e0a-ed33-4689-b45a-ea5dbea39468`) | Use `Backlog` if it's a future idea |
| **Assignee** | self (current user) | A plan you make is a plan you own |

### PAT state IDs (for `linear_update_issue`)

| State | ID |
|---|---|
| Backlog | `b632c0d4-d4fd-4304-afa4-08b50e4d943a` |
| Todo | `f18b7e0a-ed33-4689-b45a-ea5dbea39468` |
| In Progress | `84db6520-0384-48b9-b1f7-e92fb3f12cd3` |
| In Review | `c2ead3e4-4a28-4111-9fa0-8a68dc04689f` |
| Code Complete | `4b86562f-6605-4f72-8fc6-517c42c7b576` |
| Done | `c25c2053-e60f-45de-84cf-61636bf5139e` |
| Canceled | `c927f100-215e-47c6-b0b6-5cbac3e801db` |
| Deferred | `85f2cc38-e745-4e59-a522-6026b7f99fef` |

## Workflow

### Step 1: Craft a descriptive title

A good plan title is a **goal a stranger can understand in one read**.

- ❌ Bad: `fix lkit thing`, `refactor auth`, `ISSUE-42`, `plan-v2`
- ✅ Good: `Backfill lkit_ public IDs across ProcessedDocument and re-sync to OpenSearch`

Rules: imperative or noun-phrase, no ticket codes, no internal slang, ≤ ~90 chars. If you can't write a clear title, you don't understand the task yet — ask.

### Step 2: Build the description

Structured Markdown. Every section below, in this order:

```markdown
**Goal:** One sentence — what this builds or fixes and why it matters.

**Approach:** 2-3 sentences on the chosen approach and the key tradeoff considered.

**Tasks:**
- [ ] Task 1 — exact files involved (e.g. `src/foo.py`)
- [ ] Task 2 — `tests/test_foo.py`
- [ ] Task 3

**Acceptance criteria:**
- [ ] Concrete, testable condition 1
- [ ] Concrete, testable condition 2

**Files / context:**
- Modify: `path/to/file.py:123-145`
- Create: `path/to/new_module.py`
- Reference: link any related issue, doc, or ADR
```

Bite-sized tasks (DRY / YAGNI / TDD / frequent commits), just like `writing-plans`. Exact paths, real code references — not "add validation".

### Step 3: Resolve the "Plan" label

Call `linear_list_labels`. Find the label whose `name === "Plan"`.

- **Found** → use its `id` in `labelIds`.
- **Not found, or the call errors** → create the issue **without** the label, then tell the user plainly: *"`Plan` label not attached (Linear label endpoint unavailable or label missing). Add it manually: <issue_url>"*. Do **not** block the plan on the label. There is no `create_label` tool, so the label must exist in Linear already.

> **Note:** the Linear MCP's label/detail resolvers (`list_labels`, `workspace_metadata`, `get_issue`, `get_team`) are currently flaky and may return HTTP 400 even when the label exists. The fallback above keeps the skill working regardless.

### Step 4: Create the issue

```
linear_create_issue:
  teamId:    b3c4cebb-2ee9-4f59-a475-2401eca57626
  title:     <descriptive title from step 1>
  description: <markdown body from step 2>
  priority:  3            # Medium; 2=High, 4=Low
  labelIds:  [<Plan id>]  # from step 3, or [] if unresolved
  assigneeId: <your user id from linear_whoami>
```

Report the returned **identifier** (e.g. `PAT-543`) and **URL** to the user.

### Step 5: Execution handoff

After creating the issue, offer execution choice (same as `writing-plans`):

1. **Subagent-Driven (this session)** — dispatch a fresh subagent per task from the checklist, review between tasks. REQUIRED SUB-SKILL: `/skill:subagent-driven-development`.
2. **Parallel Session (separate)** — open a new session on the issue and use `/skill:executing-plans`.

If the user just wanted the issue captured (not executed), say so and stop.

## Remember

- One issue per plan. Don't cram multiple unrelated goals into one title.
- The description IS the plan — make it self-contained; assume zero codebase context.
- Descriptive title always; exact file paths always; real acceptance criteria always.
- DRY, YAGNI, TDD, frequent commits — the tasks checkbox list should reflect this.
- If the Plan label can't attach, say so explicitly. Never silently drop a requirement.
