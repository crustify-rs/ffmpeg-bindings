# Incomplete finding: `av_opt_eval_flags`' `val`

Record submitted by options wave-2 translation batch 0; corrected and
resubmitted with `query symbols --name av_opt_eval_flags --file
libavutil/opt.c --update av_opt_eval_flags.json` (the JSON here is the exact
submission; `ownership-store.json` changes in this record's `val` note only).

`val` (position 2) was described as "parsed during the call only (sscanf
num/den for rationals, av_expr_parse_and_eval otherwise)", the note shared
by all seven evaluators. For `AV_OPT_TYPE_FLAGS` neither half holds:
`set_string_number` cuts the string into tokens (an optional `+` / `-`, then
at most 255 bytes up to the next `+` / `-`, copied into `buf[256]`;
`opt.c:451-458`) and evaluates each one on its own, as a named constant or
an expression. That a token can start after the 255-byte cut, inside what
the whole string reads as one number, is what let `0*` + 253 zeros +
`ld(1e300)` past the wrapper's whole-string check (UBSan
`float-cast-overflow` in `eval.c`; evidence:
`ub_safe/eval-flags-token-split-hides-evaluator-call`). The note now records
the token rule, the hazard and the wrapper's per-token check
(`opt::flags_refusal`).

The other 13 records of the workset were checked against the C bodies and
the wrappers and are accurate: `av_opt_find` / `av_opt_find2` (FAKE_OBJ
reads only the class, children through `child_next` or
`child_class_iterate`, `target_obj` NULL with FAKE_OBJ), `av_opt_next`,
`av_opt_is_set_to_default` (a foreign option reads out of bounds; CONST
returns 1 first), `av_opt_query_ranges` / `_default` (an owned list into the
slot, NULL on the default path's failure), `av_opt_freep_ranges` (dropper)
and the six other evaluators (the output written once, on success only,
which holds: `write_number` range-checks before it stores).
