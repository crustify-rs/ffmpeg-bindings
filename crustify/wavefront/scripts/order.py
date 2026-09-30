"""Bottom-up subsystem order: link units by the linker graph; inside each SCC the member
that the most other members depend on goes first, ties broken by incoming edge weight
from the tied members."""
import json, collections
g = json.load(open("crustify/tmp/subsystem-graph.json"))
E = collections.Counter()
for k, n in g["edges"].items():
    a, b = k.split("->")
    if not b.startswith("<oot>"): E[(a, b)] += n
LU = ["libavutil","libswresample","libswscale","libavcodec","libavformat","libavfilter","libavdevice"]
nodes = ["/".join(x) for x in g["order"]]

def sccs(ns):
    idx, low, st, on, out, c = {}, {}, [], set(), [], [0]
    def visit(v):
        idx[v] = low[v] = c[0]; c[0] += 1; st.append(v); on.add(v)
        for w in ns:
            if (v, w) in E:
                if w not in idx: visit(w); low[v] = min(low[v], low[w])
                elif w in on: low[v] = min(low[v], idx[w])
        if low[v] == idx[v]:
            comp = []
            while True:
                w = st.pop(); on.discard(w); comp.append(w)
                if w == v: break
            out.append(comp)
    for v in ns:
        if v not in idx: visit(v)
    return out  # Tarjan emits sinks (producers) first

def order(ns, cuts):
    res = []
    for comp in sccs(ns):
        if len(comp) == 1: res += comp; continue
        rest = list(comp)
        dependents = {v: sum(1 for u in rest if u != v and (u, v) in E) for v in rest}
        best = max(dependents.values())
        tied = [v for v in rest if dependents[v] == best]
        pick = max(tied, key=lambda v: (sum(E[(u, v)] for u in tied if u != v), v))
        cuts += [(pick, u, E[(pick, u)]) for u in rest if u != pick and (pick, u) in E]
        res += [pick] + order([v for v in rest if v != pick], cuts)
    return res

cuts, final = [], []
for l in LU:
    ns = [n for n in nodes if n.startswith(l + "/")]
    back = [(a, b) for (a, b) in E if a.startswith(l + "/") and b.split("/")[0] in LU[LU.index(l)+1:]]
    # Header-level (inline/macro) inclusions against the link order; the linker graph
    # fixes the link-unit order, so these are cuts implied by it.
    cuts += [(a, b, E[(a, b)]) for (a, b) in back]
    final += order(ns, cuts)
json.dump({"order": final, "cuts": cuts}, open("crustify/tmp/subsystem-order.json", "w"), indent=1)
for n in final: print(n)
print("\ncycle cuts (consumer scheduled before producer):")
for a, b, n in cuts: print(f"  {a} -> {b}: {n} edges")
