"""Enumerate the cast-derived edges of a config's API graph.
usage: python3 cast_edges.py REPO CONFIG OUT.json"""
import sys, json, os
from pathlib import Path
sys.path.insert(0, "/opt/venv/share/wavefront/src")
os.environ.setdefault("PYTHONHASHSEED", "0")
from wavefront.layout import set_repo_root, set_config_path, Layout
import compose.deps_dag as dd
repo, cfg, out = Path(sys.argv[1]), Path(sys.argv[2]).resolve(), sys.argv[3]
set_repo_root(repo); set_config_path(cfg)
layout = Layout.discover(cfg)
rec = {}
orig = dd._build_edges
def wrapped(types, syms, amap, cast_scope=None):
    r = orig(types, syms, amap, cast_scope)
    if cast_scope is not None and cast_scope[0]:
        kt, ks = cast_scope
        res = lambda ref: dd._resolve_ctype(ref, amap, types)
        var, casts, chased, params = [], [], [], []
        for key, n in types.items():
            for ref in n.cast_field_refs:
                t = res(ref)
                if t in types and t != key and key in kt and t in kt:
                    var.append({"owner": key, "variant": t})
            for x in n.cast_to:
                t = amap.get(x)
                if t in types and t != key and key in kt and t in kt:
                    casts.append({"from": key, "to": t, "kept": t in n.dep_types,
                                  "deg_from": n.cast_degree(amap, kt), "deg_to": types[t].cast_degree(amap, kt)})
            for ref in n.chased_bases:
                t = res(ref)
                if t in types: chased.append({"derived": key, "base": t})
        for key, n in syms.items():
            if key not in ks: continue
            for ref in n.cast_sig_refs:
                t = res(ref)
                if t in kt: params.append({"fn": key, "type": t})
        rec.update(closure_types=len(kt), closure_syms=len(ks), variants=var, casts=casts, chased=chased, param_reads=params)
    return r
dd._build_edges = wrapped
from wavefront import manifests as M, scope as S
dag = dd.compose((M.entries(layout, cfg, "types", stage="wrap"), M.entries(layout, cfg, "symbols", stage="wrap")),
                 S.build(layout, cfg, stage="wrap"), codeql_dir=layout.codeql, api_headers_only=True)
rec["stats"] = dag["stats"]
json.dump(rec, open(out, "w"), indent=1, default=list)
print(rec["stats"], len(rec["variants"]), len(rec["casts"]), len(rec["param_reads"]))
