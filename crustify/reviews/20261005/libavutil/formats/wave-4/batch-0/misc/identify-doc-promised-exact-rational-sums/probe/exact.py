# Exact-rational model of av_csp_primaries_id_from_desc against C (ctypes over the plain libavutil):
# one BT.709 coordinate at a random large denominator placed one part around 1/1000 away.
# usage: python3 -I probe/exact.py [path/to/libavutil.so]
import ctypes, random
from fractions import Fraction as F
import sys
lib = ctypes.CDLL(sys.argv[1] if len(sys.argv) > 1 else 'crustify/builds/plain/libavutil/libavutil.so')
class Desc(ctypes.Structure):
    _fields_ = [('f', ctypes.c_int * 16)]
lib.av_csp_primaries_id_from_desc.restype = ctypes.c_uint
lib.av_csp_primaries_id_from_desc.argtypes = [ctypes.POINTER(Desc)]
lib.av_csp_primaries_desc_from_id.restype = ctypes.POINTER(Desc)
lib.av_csp_primaries_desc_from_id.argtypes = [ctypes.c_uint]
NB = 23
table = {}
for p in range(NB):
    d = lib.av_csp_primaries_desc_from_id(p)
    if d: table[p] = list(d.contents.f)
def exact_id(f):
    for p in sorted(table):
        t = table[p]
        s = sum(abs(F(f[2*i], f[2*i+1]) - F(t[2*i], t[2*i+1])) for i in range(8))
        if s < F(1, 1000): return p
    return 2
def c_id(f):
    d = Desc((ctypes.c_int*16)(*f))
    return lib.av_csp_primaries_id_from_desc(ctypes.byref(d))
random.seed(1)
found = 0
for trial in range(200000):
    base = table[1][:]  # BT709
    i = random.randrange(8)
    den = random.randrange(100001, 2**31 - 1)
    ref = F(base[2*i], base[2*i+1])
    # exact |x-ref| just below or at 1/1000
    target = ref + F(1,1000) * random.choice([1,-1])
    num = (target.numerator * den) // target.denominator + random.choice([-1, 0, 1])
    if abs(num) > 2**31 - 1: continue
    f = base[:]
    f[2*i], f[2*i+1] = num, den
    e, c = exact_id(f), c_id(f)
    if e != c:
        found += 1
        if found <= 5:
            print('mismatch', f, 'exact', e, 'C', c, 'exact sum', float(abs(F(num, den) - ref)))
print('mismatches', found)
