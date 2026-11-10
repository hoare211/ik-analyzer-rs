#!/usr/bin/env python3
"""预处理阶段 A/B 差分分析（含流水线顺序：regularize -> identifyCharType）。"""
from collections import defaultdict

def load(p):
    d = {}
    for line in open(p, encoding='utf-8'):
        line = line.rstrip('\n')
        if not line:
            continue
        p_ = line.split('\t')
        if len(p_) < 6:
            continue
        d[int(p_[0])] = dict(raw_type=int(p_[1]), reg=int(p_[2]), raw_type_name=p_[3],
                             eff_type=int(p_[4]), eff_type_name=p_[5])
    return d

def load_meta():
    m = {}
    for line in open('units_meta.tsv', encoding='utf-8'):
        u, _, name = line.rstrip('\n').partition('\t')
        m[int(u)] = name
    return m

a = load('chars_A.txt')
bl = load('chars_B_low.txt')
bn = load('chars_B_nolow.txt')
meta = load_meta()

def ch(x):
    return repr(chr(x)) if 0x20 <= x < 0x7F else f'<U+{x:04X}>'

raw_diff, eff_diff, reg_low, reg_nolow = [], [], [], []
for u in sorted(a):
    if a[u]['raw_type'] != bl[u]['raw_type']:
        raw_diff.append((u, a[u]['raw_type'], bl[u]['raw_type']))
    if a[u]['eff_type'] != bl[u]['eff_type']:
        eff_diff.append((u, a[u]['eff_type'], bl[u]['eff_type']))
    if a[u]['reg'] != bl[u]['reg']:
        reg_low.append((u, a[u]['reg'], bl[u]['reg']))
    if a[u]['reg'] != bn[u]['reg']:
        reg_nolow.append((u, a[u]['reg'], bn[u]['reg']))

print(f"总 code unit = {len(a)}")
print(f"[原始] identifyCharType 差异      = {len(raw_diff)}")
print(f"[流水线] regularize+identify 差异 = {len(eff_diff)}")
print(f"regularize 差异 (B lowercase=true)  = {len(reg_low)}")
print(f"regularize 差异 (B lowercase=false) = {len(reg_nolow)}")
print()

print("===== 流水线顺序（effective type）差异明细 =====")
for u, ta, tb in eff_diff[:6]:
    print(f"  U+{u:04X} {ch(u):<10} {meta.get(u,''):<44} A={ta} B={tb}")
if len(eff_diff) > 6:
    print(f"  ...（其余 {len(eff_diff)-6} 个同类：A=USELESS(0) / B=CHAR_SURROGATE(22)）")
print()

CATS = {
    '零宽字符': [0x200B, 0x200C, 0x200D, 0x200E, 0x200F, 0x2060, 0xFEFF, 0x00AD, 0x180E, 0x206A, 0x206E],
    '变体选择符': [0xFE00, 0xFE0E, 0xFE0F],
    '组合字符/变音符号': [0x0300, 0x0301, 0x0303, 0x030A, 0x0327, 0x0308, 0x20DD, 0x3099, 0x309A, 0x309B, 0x00E9],
    '同形异义字': [0x0430, 0x0435, 0x043E, 0x0440, 0x0441, 0x0445, 0x03BF, 0x03C1, 0x03B1, 0x0391],
    'Bidi': [0x05D0, 0x0627, 0x0644, 0x0660, 0x0669, 0x06F0, 0x06F9, 0xFB00, 0xFB06],
    '空白家族': [0x0020, 0x00A0, 0x1680, 0x2000, 0x2002, 0x2003, 0x200A, 0x202F, 0x205F, 0x3000],
    'CJK/全角标点': [0x3001, 0x3002, 0xFF0C, 0xFF01, 0x3008, 0x3009, 0xFF08, 0xFF09],
    '半角片假名/全角符号': [0xFF61, 0xFF66, 0xFF9F, 0xFFE0, 0xFFE1, 0xFFE5, 0xFFE6, 0xFFE8, 0xFFEE],
    '数字': [0x30, 0x39, 0xFF10, 0xFF19, 0x0660, 0x06F0, 0x0966, 0x0E50, 0x2070, 0x2080, 0x2460, 0x2153, 0x00B2],
    'CJK/假名/谚文/注音': [0x4E2D, 0x56FD, 0x3400, 0x4DBF, 0xF900, 0x3042, 0x30A2, 0xAC00, 0x3105, 0xFF71, 0x3300, 0x33FF],
    'URL/邮箱/版本号连接符': [0x23, 0x26, 0x2B, 0x2D, 0x2E, 0x40, 0x5F, 0x2F, 0x3A, 0x3F, 0x3D, 0x7E, 0x25,
                           0x2C, 0x3B, 0x7C, 0xFF0F, 0xFF1A, 0xFF03, 0xFF06, 0xFF0B, 0xFF0D, 0xFF0E, 0xFF20, 0xFF3F],
    'ASCII 字母': [0x41, 0x5A, 0x61, 0x7A, 0xFF21, 0xFF3A, 0xFF41, 0xFF5A],
}
print("===== 关键类别：effective type 与 regularize 结果 =====")
for cat, cps in CATS.items():
    print(f"  --- {cat} ---")
    for cp in cps:
        if cp not in a:
            continue
        A = a[cp]; L = bl[cp]; N = bn[cp]
        same = (A['eff_type'] == L['eff_type'] == N['eff_type']) and (A['reg'] == L['reg'] == N['reg'])
        mark = '' if same else '   <== 差异'
        print(f"    U+{cp:04X} {ch(cp):<9} reg=U+{A['reg']:04X}  eff: A={A['eff_type_name']:<10}"
              f" B(low)={L['eff_type_name']:<10} B(nolow)={N['eff_type_name']:<10}{mark}")
print()

hi = [u for u, _, _ in reg_nolow if 0xFF21 <= u <= 0xFF3A]
lo = [u for u, _, _ in reg_nolow if 0x41 <= u <= 0x5A]
other = [u for u, _, _ in reg_nolow if not (0x41 <= u <= 0x5A or 0xFF21 <= u <= 0xFF3A)]
print("===== regularize 差异全量（B lowercase=false 时）=====")
print(f"  ASCII A-Z (U+0041-005A): {len(lo)} 个")
print(f"  全角 A-Z (U+FF21-FF3A):  {len(hi)} 个")
print(f"  其他: {len(other)} 个 {other}")
print()
print("===== 全角->半角改写边界（u6 regularize）=====")
fw = [(u, a[u]['reg']) for u in sorted(a) if 0xFF00 <= u <= 0xFFEF]
changed = [u for u, r in fw if u != r]
unchanged = [u for u, r in fw if u == r]
print(f"  枚举 U+FF00-FFEF 共 {len(fw)} 个；改写 {len(changed)}；未改写 {len(unchanged)}")
print(f"  改写区间 = U+{min(changed):04X} .. U+{max(changed):04X}  (即 65280 < x < 65375，双端开区间)")
print(f"  未改写码位 = {[f'U+{u:04X}' for u in unchanged]}")
print()
print("===== 代理区 effective type =====")
sh = [u for u in sorted(a) if 0xD800 <= u <= 0xDBFF]
sl = [u for u in sorted(a) if 0xDC00 <= u <= 0xDFFF]
print(f"  高位代理 {len(sh)} 个: A={a[sh[0]]['eff_type_name'] if sh else '-'}, B(low)={bl[sh[0]]['eff_type_name'] if sh else '-'}")
print(f"  低位代理 {len(sl)} 个: A={a[sl[0]]['eff_type_name'] if sl else '-'}, B(low)={bl[sl[0]]['eff_type_name'] if sl else '-'}")
