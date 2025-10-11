---
Update (SNAPSHOT v0.7)

Evidence
- When discovery fails (and --skip-discovery is not set), `load_or_default_project` returns an in-memory ProjectConfig targeting the managed Alpine image. No castra.toml is written.

Refinement
- Ensure error paths distinguish between explicit `--config` missing (still an error) and discovery fallback (zero-config path). Maintain warning-summary behavior for synthesized configs.

Acceptance criteria (amended)
- Discovery failure without override yields a usable default project with managed_image; explicit --config pointing to a missing file continues to error with exit 66. [DONE]


---

