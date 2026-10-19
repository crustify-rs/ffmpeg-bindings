"""Project a config's API DAG, restricted to its API closure, onto subsystems.
usage: python3 api_subgraph.py CONFIG OUT.json"""
import sys, json, collections
from pathlib import Path
sys.path.insert(0, "/opt/venv/share/wavefront/src")
from wavefront.layout import set_repo_root, set_config_path, Layout
import compose.deps_dag as dd
from wavefront import manifests as M, scope as S
cfg = Path(sys.argv[1]).resolve()
set_repo_root(Path("/target")); set_config_path(cfg)
layout = Layout.discover(cfg)
cl = {}
orig = dd._api_closure
def wrap(types, syms, sj):
    r = orig(types, syms, sj); cl["t"], cl["s"] = r; return r
dd._api_closure = wrap
g = dd.compose((M.entries(layout, cfg, "types", stage="wrap"), M.entries(layout, cfg, "symbols", stage="wrap")),
               S.build(layout, cfg, stage="wrap"), codeql_dir=layout.codeql, api_headers_only=True)
keep = {(k[0], k[1] or None) for k in cl["t"]} | {(k[0], k[1]) for k in cl["s"]}
keepn = {k[0] for k in keep}
ss = json.load(open("crustify/artifacts/subsystems.json"))
own = {}
for lu in ss["link_units"]:
    for s in lu["subsystems"]:
        for f in s.get("implementation_files", []) + s.get("api_headers", []):
            own.setdefault(f, f'{lu["name"]}/{s["name"]}')
E = collections.Counter(); W = collections.defaultdict(list)
for layer in g["layers"]:
    for n in layer:
        if (n["id"], n.get("defined_in")) not in keep and (n["id"], n.get("defined_in") or "") not in keep: continue
        a = own.get(n.get("defined_in"))
        if not a: continue
        for part in ("deps", "fallback"):
            for side in ("types", "syms"):
                for d in (n.get(part) or {}).get(side, []):
                    b = own.get(d["defined_in"])
                    if b and b != a:
                        E[(a, b)] += 1
                        if len(W[(a, b)]) < 8: W[(a, b)].append(f'{n["id"]} -> {d["name"]}' + (" (fallback)" if part == "fallback" else ""))
json.dump({"closure": [len(cl["t"]), len(cl["s"])], "stats": g["stats"],
           "edges": {f"{a}->{b}": n for (a, b), n in sorted(E.items())},
           "examples": {f"{a}->{b}": W[(a, b)] for (a, b) in sorted(E)}}, open(sys.argv[2], "w"), indent=1)
print("closure", len(cl["t"]), len(cl["s"]), g["stats"])
for (a, b), n in sorted(E.items()): print(f"{a} -> {b}: {n}  e.g. {W[(a,b)][:4]}")
