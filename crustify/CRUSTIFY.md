
Fill in as many answers as you want before starting Crustify. It will ask only
for campaign decisions that remain unresolved.

# Mandatory questions

## Campaign

1. **Which repository and revision should this campaign use?** (`id: source`)
   - Answer: `https://github.com/FFmpeg/FFmpeg`, `1019f8f036`: the `c/` submodule. The
     revision the campaign has used from the start, first as the checkout at `/target`

2. **Should this campaign port the C implementation to Rust, or create safe
   Rust wrappers?** (`id: campaign`)
   - Answer: `wrap`

3. **What should this campaign target: a named subset of modules, or a named
   subset of functions and types, or the whole target repo? You can define them
   now or we can brainstorm them during the live session. You can also let the
   orchestrator decide.** (`id: scope`)
   - Answer: the whole public API of the target, excluding deprecated items

4. **Which model and billing should do the translation work?**
   (`id: translate-agent`)
   - Answer: `openrouter/anthropic/claude-opus-5.5, api`
   - Format: `<provider/model, api | subscription>`, e.g.
     `openrouter/anthropic/claude-opus-5, api`. The provider is the first
     segment and selects both the billing rate table and the backend CLI.
     Supported providers:
     - `anthropic/<model>` — Anthropic direct, driven by Claude Code;
     - `openai/<model>` — OpenAI direct, driven by Codex;
     - `openrouter/<vendor>/<model>` — OpenRouter, which resells every vendor
       behind one key, so the vendor segment picks the CLI: `anthropic/*` is
       driven by Claude Code and everything else by Codex. OpenRouter
       supports `api` billing only.

5. **Do you want agentic review after translated work lands? If so, which
   model and billing should perform each review?** (`id: review-agent`)
   - Answer: `openrouter/anthropic/claude-opus-5.5, api`, after each wave

6. **Should I run fully autonomously end to end?** (`id: autonomy`)
   - Answer: `no`

# Optional questions

Unanswered optional questions use their defaults.

## Campaign execution

7. **Which model and billing should plan the campaign, and how many planner runs
   should it make? Each run emits what the plan lacks and corrects what the runs
   before it committed.** (`id: plan-agent`)
   - Answer: the translation model and billing, 2 runs. The subsystems already
     planned were planned by the orchestrator, before planners existed.
   - By default: the translation model and billing, 2 runs.

8. **Should the campaign use the default batching and parallelism settings, or
   customize them?** (`id: workload`)
   - Answer: `max-types: 2, max-syms: 25, max-loc: 500`, at least 20 fields per type
     batch; parallelism up to 16 agents.

9. **What batch caps should review agents use? We recommend the same caps as
   translation by default.** (`id: review-workload`)
   - Answer: `as many as the reviewed wave requires`

10. **Which objectives should review agents review for? By default, every one the
    reviewer offers.** (`id: review-objectives`)
    - Answer: `all`
    - Without `fix`, reviewers report and reproduce what they find but patch nothing.

11. **Which sanitizers should the campaign build and test under? By default, every one
    the toolchain supports.** (`id: sanitizers`)
    - Answer: `asan-ubsan, tsan, bsan`, the variants `crustify/artifacts/build.json`
      records; MSan is not built.
    - Miri needs no build and is always on.

12. **How should the target be configured: which of its build features should be
    enabled or disabled?** (`id: target-config`)
    - Answer: the configuration `crustify/artifacts/build.json` records: shared
      libraries only, autodetection off, pthreads and zlib on, docs off, and the FATE
      samples in `crustify/fate-suite`.
    - By default: the target's default configuration, with deprecated features disabled.

## Autonomy (if question 6 is answered `no`)

Answer these approval-gate questions only if question 6 is answered `no`.

13. **Should I wait for your approval before starting the setup phase?** (`id: gates.setup`)
    - Answer: `no`
14. **Should I wait for your approval before starting the translation phase?**
    (`id: gates.translation`)
    - Answer: `yes`
15. **Should I wait for your approval between modules?** (`id: gates.module`)
    - Answer: `no`
16. **Should I wait for your approval before starting review passes?** (`id: gates.review`)
    - Answer: `no`

# Benchmark recording questions

17. **Where and in what format should the dashboard be recorded?** (`id: dashboard`)
    - Answer: `<repo-checkout>/crustify/artifacts/dashboard.md`, `standard` template

# Additional instructions

Unless otherwise stated, preserve the format of the dashboard exactly.

