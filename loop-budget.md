# Loop Budget — loco-laravel migration

> Primary loop: **Migration Refactor Loop** (pr-babysitter pattern, L2 manual trigger)

## Daily limits

| Loop | Max runs/day | Max tokens/day | Max sub-agent spawns/run |
|------|--------------|----------------|--------------------------|
| Migration slice (maker + checker) | 3 slices | 1M | 2 (1 implementer + 1 verifier) |

Whole-migration expectation: ~12 slices ≈ 2.5–3.5M tokens total at ~250k per
action run (registry estimate for pr-babysitter action runs).

## On budget exceed

1. Stop starting new slices; finish the one in flight if a merge is imminent
2. Append a `budget` event line to `loop-run-log.md`
3. Flag in `STATE.md` High Priority: `[BUDGET] day cap reached — human decision needed`
4. Never self-raise the cap. If High Priority work remains and ROI justifies
   it, follow `skills/budget-negotiator` (draft a bump request for a human to
   approve in this file — max +20% or +50k, once per day)

## Kill switch

- Flag in `STATE.md`: `LOOP PAUSED — loop-pause-all` (agents must check this
  line before starting any slice)
- Resume only after a human removes the flag

## Estimate spend

```bash
npx @cobusgreyling/loop-cost --pattern pr-babysitter --level L2
```
