"""Recompute the subsystem decomposition from the plain build's objects and the CodeQL T1/T2
tables, into crustify/tmp/planner-run-1/subsystems.v2.json (and the subsystem graph that
order.py reads). The setup's decomposition script, with planner run 1's header placement:
see MOVE and SHARE. Run from the repository root, after mkdir -p crustify/tmp/planner-run-1."""
import csv, json, os, re, sys, collections, subprocess
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from inventory import LIBS, API, EXCL, tus, private_headers
T1, T2 = "crustify/wavefront/codeql/t1", "crustify/wavefront/codeql/t2"
csv.field_size_limit(1 << 30)
rows = lambda p: csv.DictReader(open(p))

# ---- subsystem rules -------------------------------------------------------
AVUTIL = {
 "core": ("mem avstring avsscanf bprint cpu error file file_open integer intmath log log2_tab "
          "mathematics random_seed rational reverse time timestamp utils version slicethread "
          "xga_font_data x86/cpu x86/cpuid attributes avassert avutil bswap common intfloat "
          "intreadwrite macros aarch64/cpu attributes_internal cpu_internal dynarray emms ffmath "
          "getenv_utf8 int128 internal libm loongarch/cpu mem_internal ppc/cpu qsort sfc64 thread "
          "time_internal timer x86/asm x86/bswap x86/intmath x86/intreadwrite x86/timer softfloat "
          "softfloat_ieee754 softfloat_tables"),
 "formats": ("pixfmt pixdesc samplefmt channel_layout imgutils csp pixelutils float2half half2float "
             "colorspace imgutils_internal x86/imgutils x86/imgutils_init x86/pixelutils"),
 "options": "dict opt eval parseutils",
 "buffer-frame": "buffer frame side_data refstruct buffer_internal",
 "hwcontext": "hwcontext hwcontext_stub hwcontext_internal",
 "metadata": ("ambient_viewing_environment detection_bbox display dovi_meta downmix_info "
              "encryption_info film_grain_params hdr_dynamic_metadata hdr_dynamic_vivid_metadata iamf "
              "mastering_display_metadata motion_vector raw_color_params replaygain spherical stereo3d "
              "tdrdi timecode timecode_internal video_enc_params video_hint"),
 "crypto": ("adler32 aes aes_ctr base64 blowfish camellia cast5 crc des hash hmac lzo md5 murmur3 rc4 "
            "ripemd sha sha512 tea twofish uuid xtea x86/aes x86/aes_init x86/crc aes_internal crc_internal"),
 "containers": "audio_fifo container_fifo executor fifo threadmessage tree",
 "dsp": ("tx tx_double tx_float tx_int32 lfg lls fixed_dsp float_dsp float_scalarproduct x86/fixed_dsp "
         "x86/fixed_dsp_init x86/float_dsp x86/float_dsp_init x86/lls x86/lls_init x86/tx_float "
         "x86/tx_float_init tx_priv"),
}
# Other libraries: API-implementing TUs by header group; the rest is internal.
API_GROUPS = {
 "libswresample": {"swresample": None}, "libswscale": {"swscale": None},
 "libavcodec": {"core": "avcodec codec codec_desc codec_id codec_par packet defs version version_major",
                "bsf": "bsf", "utils": "ac3_parser adts_parser avdct dirac dv_profile exif smpte_436m vorbis_parser"},
 "libavformat": {"core": "avformat version version_major", "avio": "avio"},
 "libavfilter": {"core": "avfilter buffersink buffersrc version version_major"},
 "libavdevice": {"core": "avdevice version version_major"},
}
INTERNAL = {"libavcodec": "codecs", "libavformat": "formats", "libavfilter": "filters", "libavdevice": "devices"}
NATIVE = {
 ("libavutil","crypto"): ["RustCrypto crates (aes, sha1, sha2, md-5, des, blowfish, camellia, cast5, twofish, ripemd, hmac, rc4)", "crc32fast / crc", "adler2", "base64", "uuid", "murmur3"],
 ("libavutil","containers"): ["std::collections::VecDeque", "std::collections::BTreeMap", "std::sync::mpsc / crossbeam-channel"],
 ("libavutil","dsp"): ["rustfft", "rand"],
 ("libavutil","core"): ["std (alloc, fmt, time, error)"],
 ("libswresample","swresample"): ["rubato"],
 ("libswscale","swscale"): ["fast_image_resize"],
}

def stem(p):  # "libavutil/x86/cpu.c" -> "x86/cpu"
    return os.path.splitext(p.split("/", 1)[1])[0]

TU = tus()
PRIV = private_headers(set(TU))
api_tu = collections.defaultdict(set)   # TU -> API headers it implements
for r in rows(f"{T1}/functions.csv"):
    d = r["def_file"]
    if d in TU:
        for h in r["decl_files"].split("|"):
            if h in API and h != d: api_tu[d].add(h)

home = {}  # file -> (lib, subsystem)
for l in LIBS:
    files = [f for f in TU if TU[f] == l] + [h for h in PRIV if h.startswith(l + "/")] \
          + [h for h in API if h.startswith(l + "/")]
    if l == "libavutil":
        rule = {s: n for n, ss in AVUTIL.items() for s in ss.split()}
        for f in files:
            if stem(f) not in rule: sys.exit(f"unassigned {f}")
            home[f] = (l, rule[stem(f)])
        continue
    groups = API_GROUPS[l]
    single = len(groups) == 1 and next(iter(groups.values())) is None
    hdr_grp = {} if single else {f"{l}/{h}.h": g for g, hs in groups.items() for h in hs.split()}
    for f in files:
        if single: home[f] = (l, next(iter(groups)))
        elif f in hdr_grp: home[f] = (l, hdr_grp[f])
        elif f in api_tu:
            gs = {hdr_grp[h] for h in api_tu[f]}
            assert len(gs) == 1, (f, gs); home[f] = (l, gs.pop())
        elif l == "libavcodec" and (f.startswith("libavcodec/bsf/") or f in ("libavcodec/bsf_internal.h",)):
            home[f] = (l, "bsf")
        else: home[f] = (l, INTERNAL[l])
    if not single:  # private header with a same-stem TU follows it
        for f in files:
            if f.endswith(".h") and f not in API:
                for e in (".c", ".asm"):
                    t = f[:-2] + e
                    if t in home: home[f] = home[t]

# ---- planner run 1: header placement -------------------------------------------
# A private header defining a public type goes with the subsystem that publishes it:
# moved when only that subsystem includes it, shared otherwise. A shared header's
# entities are owned by the sharer scheduled first.
MOVE = {"libavcodec/vorbis_parser_internal.h": ("libavcodec", "utils")}
SHARE = {"libavcodec/internal.h": ("libavcodec", "core"), "libavformat/internal.h": ("libavformat", "core"),
         "libavfilter/filters.h": ("libavfilter", "core"), "libavfilter/formats.h": ("libavfilter", "core")}
home.update(MOVE)
entity_home = dict(home); entity_home.update(SHARE)

# ---- entities --------------------------------------------------------------
def first(s): return s.split("|")[0] if s else ""
types = collections.defaultdict(list)
for r in rows(f"{T1}/types.csv"): types[r["name"]].append(r)
INC = collections.defaultdict(set)
for r in rows(f"{T1}/includes.csv"): INC[r["source_file"]].add(r["included_file"])
_reach = {}
def reach(f):
    """Files visible from f through its include closure, f included."""
    if f not in _reach:
        seen, st = {f}, [f]
        while st:
            for g in INC.get(st.pop(), ()):
                if g not in seen: seen.add(g); st.append(g)
        _reach[f] = seen
    return _reach[f]
def resolve(cands, consumer):
    """Pick the definition a name refers to from consumer's point of view."""
    if len(cands) <= 1: return cands[0] if cands else ""
    if consumer in cands: return consumer
    vis = [c for c in cands if c in reach(consumer)] if consumer else []
    if vis: return vis[0]
    lib = consumer.split("/")[0]
    same = [c for c in cands if c.split("/")[0] == lib]
    return (same or cands)[0]
def type_file(name, kind, def_file, consumer=""):
    if def_file: return def_file
    cands = []
    for r in types.get(name, ()):
        if r["kind"] == kind or not kind:
            f = r["def_file"] or first(r["decl_files"])
            if f and f not in cands: cands.append(f)
    return resolve(cands, consumer)
fdecl, gdecl = collections.defaultdict(list), collections.defaultdict(list)
for r in rows(f"{T1}/functions.csv"): fdecl[r["name"]].append(r["def_file"] or first(r["decl_files"]))
for r in rows(f"{T1}/globals.csv"): gdecl[r["name"]].append(r["def_file"] or first(r["decl_files"]))
fdef = lambda n, c: resolve(fdecl.get(n, []), c)
gdef = lambda n, c: resolve(gdecl.get(n, []), c)

UNION_RE = re.compile(r"^(?:typedef\s+)?union\b(?:\s+(\w+))?[^;{]*\{", re.M)
def unions_in(f):
    try: txt = open(f, errors="replace").read()
    except OSError: return set()
    out = set()
    for m in UNION_RE.finditer(txt):
        name = m.group(1)
        if not name:  # typedef union { ... } name;
            end = txt.find("}", m.end()); tail = re.match(r"\}\s*(\w+)", txt[end:])
            name = tail.group(1) if tail else f"<anon@{m.start()}>"
        out.add(name)
    return out
UNIONS = {}
def is_union(name, f):
    if f not in UNIONS: UNIONS[f] = unions_in(f)
    return name in UNIONS[f]

def type_kind(name, kind, f):
    """-> counter key, or None for kinds the counters do not track."""
    if kind == "enum": return "enums"
    recs = [r for r in types.get(name, ()) if (r["def_file"] or first(r["decl_files"])) == f] or types.get(name, [])
    uk = {r["unaliased_kind"] for r in recs}
    if "callback" in uk: return "callbacks"
    if "primitive" in uk: return None
    if "enum_anonymous" in uk: return "enums"
    return "unions" if is_union(name, f) else "structs"

OOT = [(re.compile(p), lib) for p, lib in [
    (r"zlib\.h|zconf\.h", "libz"), (r"/math\.h|bits/math|bits/mathcalls|/fenv\.h|/complex\.h|/tgmath\.h", "libm"),
    (r"^/usr/include/(linux|asm|asm-generic|drm|sound)/|/x86_64-linux-gnu/asm/", "linux-uapi (headers only)"),
    (r"^/usr/lib/gcc/", "gcc builtins"), (r"^/usr/include/", "libc")]]
def owner(f):
    if f in entity_home: return entity_home[f]
    if not f or f.split("/")[0] in LIBS or not f.startswith("/"): return None
    for rx, lib in OOT:
        if rx.search(f): return ("<oot>", lib)
    return ("<oot>", "libc")

# ---- edges: (consumer entity) -> (producer entity) --------------------------
edges = set()   # (consumer_owner, consumer_entity, producer_owner, producer_entity, producer_kind)
def add(cf, cent, pname, pkind, pf):
    co, po = owner(cf), owner(pf)
    if not co or not po or co == po or co[0] == "<oot>": return
    edges.add((co, cent, po, (pname, pf), pkind))
for r in rows(f"{T2}/function_calls.csv"):
    add(r["caller_file"], ("f", r["caller_name"]), r["callee_name"], "functions", r["callee_def_file"] or fdef(r["callee_name"], r["caller_file"]))
for r in rows(f"{T2}/function_addresses.csv"):
    add(r["access_file"], ("f", r["enclosing_name"]), r["target_name"], "functions", r["target_def_file"] or fdef(r["target_name"], r["access_file"]))
for r in rows(f"{T2}/global_accesses.csv"):
    add(r["access_file"], ("f", r["enclosing_name"]), r["global_name"], "global_variables", r["global_def_file"] or gdef(r["global_name"], r["access_file"]))
for r in rows(f"{T2}/macro_expansions.csv"):
    if r["macro_name"]:
        add(r["invocation_file"], ("f", r["enclosing_name"]), r["macro_name"], "macros", r["macro_def_file"])
def tuse(cf, cent, tn, tk, tf):
    if not tn: return
    f = type_file(tn, tk, tf, cf); k = type_kind(tn, tk, f)
    if k: add(cf, cent, tn, k, f)
for t in ("signature_type_uses", "local_type_uses"):
    for r in rows(f"{T2}/{t}.csv"):
        tuse(r["function_def_file"] or fdef(r["function_name"], ""), ("f", r["function_name"]), r["type_name"], r["type_kind"], r["type_def_file"])
for r in rows(f"{T2}/field_type_uses.csv"):
    tuse(r["struct_def_file"], ("t", r["struct_name"]), r["type_name"], r["type_kind"], r["type_def_file"])
for r in rows(f"{T2}/global_type_uses.csv"):
    tuse(r["global_def_file"], ("g", r["global_name"]), r["type_name"], r["type_kind"], r["type_def_file"])
for r in rows(f"{T2}/callback_signature_type_uses.csv"):
    tuse(r["callback_def_file"], ("c", r["callback_name"]), r["type_name"], r["type_kind"], r["type_def_file"])
for r in rows(f"{T2}/field_accesses.csv"):
    f = r["struct_def_file"]; k = type_kind(r["struct_name"], "struct", f)
    if k: add(r["access_file"], ("f", r["enclosing_name"]), r["struct_name"], k, f)

KINDS = ["structs", "functions", "global_variables", "enums", "unions", "callbacks", "macros"]
out_edges = collections.Counter(); consumed = collections.defaultdict(lambda: collections.defaultdict(set))
for co, ce, po, pe, pk in edges:
    out_edges[(co, po)] += 1; consumed[(co, po)][pk].add(pe)

# ---- counters ----------------------------------------------------------------
LINE_C = re.compile(r"//[^\n]*|/\*.*?\*/", re.S)
def loc(f):
    txt = open(f, errors="replace").read()
    txt = re.sub(r";[^\n]*", "", txt) if f.endswith(".asm") else LINE_C.sub("", txt)
    return sum(1 for ln in txt.splitlines() if ln.strip())
defs = collections.defaultdict(lambda: collections.defaultdict(set))  # file -> kind -> names
for r in rows(f"{T1}/functions.csv"):
    if r["def_file"]: defs[r["def_file"]]["functions"].add(r["name"])
for r in rows(f"{T1}/globals.csv"):
    if r["def_file"]: defs[r["def_file"]]["global_variables"].add(r["name"])
for r in rows(f"{T1}/macros.csv"):
    if r["name"] and r["def_file"]: defs[r["def_file"]]["macros"].add(r["name"])
for n, rs in types.items():
    for r in rs:
        f = r["def_file"] if r["kind"] in ("struct", "enum") else (first(r["decl_files"]) if r["unaliased_kind"] in ("callback", "struct_anonymous", "enum_anonymous") else "")
        if not f: continue
        k = type_kind(n, r["kind"] if r["kind"] == "enum" else "", f) if r["kind"] != "enum" else "enums"
        if k: defs[f][k].add(n)
pub_decl = collections.defaultdict(lambda: collections.defaultdict(set))  # api header -> kind -> names
for r in rows(f"{T1}/functions.csv"):
    for h in r["decl_files"].split("|") + [r["def_file"]]:
        if h in API: pub_decl[h]["functions"].add(r["name"])
for r in rows(f"{T1}/globals.csv"):
    for h in r["decl_files"].split("|"):
        if h in API: pub_decl[h]["global_variables"].add(r["name"])
for n, rs in types.items():
    for r in rs:
        if r["kind"] == "struct" and r["def_file"] not in API:
            for h in r["decl_files"].split("|"):
                if h in API: pub_decl[h]["opaque_structs"].add(n)

def impl_counters(files, sub):
    c = {"files": len(files), "loc": sum(loc(f) for f in files)}
    for k in KINDS: c[k] = sum(len(defs[f][k]) for f in files if entity_home[f] == sub)
    return c
seen_opaque = set()
def api_counters(hdrs):
    c = {"files": len(hdrs), "loc": sum(loc(f) for f in hdrs)}
    for k in ["structs", "opaque_structs", "functions", "global_variables", "enums", "unions", "callbacks", "macros"]:
        if k in ("functions", "global_variables"): names = set().union(*[pub_decl[h][k] for h in hdrs]) if hdrs else set()
        elif k == "opaque_structs":
            names = (set().union(*[pub_decl[h][k] for h in hdrs]) if hdrs else set()) - seen_opaque
            seen_opaque.update(names)  # a shared forward declaration counts once
        else: names = set().union(*[defs[h][k] for h in hdrs]) if hdrs else set()
        c[k] = len(names)
    return c

# ---- assemble ------------------------------------------------------------------
subs = collections.OrderedDict()
for f, (l, s) in sorted(home.items()):
    subs.setdefault((l, s), {"impl": [], "api": []})["api" if f in API else "impl"].append(f)
for f, (l, s) in sorted(SHARE.items()):
    subs[(l, s)]["impl"].append(f)
order = [(l, s) for l in LIBS for (ll, s) in subs if ll == l]
json.dump({"order": order, "edges": {f"{a[0]}/{a[1]}->{b[0]}/{b[1]}": n for (a, b), n in out_edges.items()}},
          open("crustify/tmp/planner-run-1/subsystem-graph.v2.json", "w"), indent=1)
rev = subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()
doc = {"version": rev, "link_units": []}
for l in LIBS:
    lu = {"name": l, "kind": "library", "linkage": "shared", "subsystems": []}
    for (ll, s) in order:
        if ll != l: continue
        sd = subs[(l, s)]
        in_tree, oot = [], []
        for (co, po), n in sorted(out_edges.items()):
            if co != (l, s): continue
            cnt = {k: len(consumed[(co, po)][k]) for k in KINDS}
            if po[0] == "<oot>": oot.append({"library": po[1], "counters": cnt})
            else: in_tree.append({"link_unit": po[0], "subsystem": po[1], "outgoing_edges": n,
                                  "incoming_edges": out_edges.get((po, co), 0), "counters": cnt})
        lu["subsystems"].append({
            "name": s, "objective": {"objective": "wrap", "rust_native_equivalent": NATIVE.get((l, s), [])},
            "implementation_files": sorted(sd["impl"]), "api_headers": sorted(sd["api"]),
            "counters": {"implementation": impl_counters(sorted(sd["impl"]), (l, s)), "api": api_counters(sorted(sd["api"]))},
            "imported_deps": {"in_tree": in_tree, "out_of_tree": oot}})
    doc["link_units"].append(lu)
for exe in ("ffmpeg", "ffprobe"):
    doc["link_units"].append({"name": exe, "kind": "executable", "linkage": None, "subsystems": []})
json.dump(doc, open("crustify/tmp/planner-run-1/subsystems.v2.json", "w"), indent=2); open("crustify/tmp/planner-run-1/subsystems.v2.json", "a").write("\n")
print(len(order), "subsystems;", len(edges), "cross-subsystem edges")
