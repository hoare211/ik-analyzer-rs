#!/usr/bin/env python3
"""字符系统语料 A/B 差异按类别归因。"""
import sys
from collections import defaultdict

def load(p):
    d = {}
    for line in open(p, encoding='utf-8'):
        line = line.rstrip('\n')
        if not line.startswith('#'):
            continue
        i, _, r = line.partition('|')
        d[int(i[1:])] = r.split()
    return d

def load_cat():
    c = {}
    for line in open('char_corpus_cat.tsv', encoding='utf-8'):
        i, _, k = line.rstrip('\n').partition('\t')
        c[int(i)] = k
    return c

def load_src():
    s = {}
    for line in open('char_corpus.txt', encoding='utf-8', errors='surrogatepass'):
        s[len(s)] = line.rstrip('\n')
    return s

def esc(t):
    return t.encode('unicode_escape').decode('ascii')

M = sys.argv[1] if len(sys.argv) > 1 else 'maxword'
want = sys.argv[2] if len(sys.argv) > 2 else None
a = load(f'cc_A_{M}.txt')
b = load(f'cc_B_{M}.txt')
cat = load_cat()
src = load_src()

by_cat = defaultdict(list)
for k in sorted(a):
    if a[k] == b.get(k):
        continue
    by_cat[cat[k]].append(k)

for c, ks in by_cat.items():
    if want and c != want:
        continue
    print(f"########## 类别 = {c}  ({len(ks)} 个用例) ##########")
    for k in ks:
        ta, tb = a[k], b.get(k, [])
        sa = set(ta) - set(tb)
        sb = set(tb) - set(ta)
        print(f"  [{k}] {esc(src[k])}")
        if sa or sb:
            print(f"      A: {' '.join(esc(t) for t in ta)}")
            print(f"      B: {' '.join(esc(t) for t in tb)}")
            print(f"      A-only: {[esc(x) for x in sa]}")
            print(f"      B-only: {[esc(x) for x in sb]}")
        else:
            print(f"      (仅顺序不同) A: {' '.join(esc(t) for t in ta)}")
            print(f"                    B: {' '.join(esc(t) for t in tb)}")
    print()
