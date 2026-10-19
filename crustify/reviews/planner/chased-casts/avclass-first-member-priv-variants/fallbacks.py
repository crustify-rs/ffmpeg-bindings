"""List the cut cycle edges (fallback) of a config's API DAG. usage: fallbacks.py CONFIG"""
import sys
from pathlib import Path
sys.path.insert(0, "/opt/venv/share/wavefront/src")
from wavefront.layout import set_repo_root, set_config_path, Layout
from wavefront import dag as D
cfg = Path(sys.argv[1]).resolve(); set_repo_root(Path("/target")); set_config_path(cfg)
g = D.build(Layout.discover(cfg), cfg, stage="wrap", api_headers_only=True)
print(g["stats"])
for layer in g["layers"]:
    for n in layer:
        for side in ("types", "syms"):
            for d in (n.get("fallback") or {}).get(side, []):
                print(f'  {n["id"]} ({n["defined_in"]}) -> {d["name"]} ({d["defined_in"]})')
