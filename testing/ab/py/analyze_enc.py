#!/usr/bin/env python3
"""编码阶段分析：偏移单位、代理对跨页、refill 边界重复。

用法: python3 analyze_enc.py [probe_file] [--verbose]
  probe_file 默认 probe_split.txt（125 位置代理对跨页扫描）。
  假定 .a.txt / .b.txt 已由 cmp.sh 生成，且与 probe_file 对应。

摘要统计：
  拆分代理对  —— CNCHAR token 的文本是孤立代理（UTF-16 高/低代理区单字符）
  重叠 token —— 两个 token 的 [begin,end) 区间相交
  last_end   —— 最后一个 token 的 end，应等于输入的 UTF-16 code unit 总数
"""
import re, sys

args = [a for a in sys.argv[1:] if not a.startswith('--')]
VERBOSE = '--verbose' in sys.argv
PROBE = args[0] if args else 'probe_split.txt'


def load(p):
    d = {}
    for line in open(p, encoding='utf-8'):
        line = line.rstrip('\n')
        if not line.startswith('#'):
            continue
        i, _, r = line.partition('|')
        d[int(i[1:])] = r.split()
    return d


def load_src():
    s = {}
    for line in open(PROBE, encoding='utf-8'):
        s[len(s)] = line.rstrip('\n')
    return s


def esc(t):
    return t.encode('unicode_escape').decode('ascii')


def parse(toks):
    spans = []
    for t in toks:
        m = re.match(r'(.*)\[(\w+)\]\((\d+)-(\d+)\)$', t)
        if m:
            spans.append((int(m.group(3)), int(m.group(4)), m.group(2), m.group(1)))
    return spans


def analyze(spans, total_units, src_text):
    """返回 (拆分代理对数, 重叠token对数, 未覆盖unit数)

    拆分代理对的判定不能用 token 文本：孤立的高位代理经 stdout 以 UTF-8
    编码时会变成 '?'（无法编码），所以必须回查**输入原文**在该偏移上的字符。
    类型也不能限定 CNCHAR——B 的 outputSingleCharLexeme 用的是 TYPE_CNWORD
    （SurrogatePairSegmenter.java:88），所以只按「长度 1 + 原字符为辅助平面」判定。

    覆盖率指标：不要用「最后一个 token 的 end」判断完整性——max_word 输出
    顺序非按位置排序（嵌套词元在后）。正确指标是所有 unit 是否被覆盖。
    """
    split = 0
    for s in spans:
        if (s[1] - s[0]) != 1:
            continue
        if s[0] < len(src_text):
            cp = ord(src_text[s[0]])
            if cp > 0xFFFF:          # 该 unit 是某个辅助平面字符的一半
                split += 1
    dup = 0
    for i in range(len(spans)):
        for j in range(i + 1, len(spans)):
            if spans[i][0] < spans[j][1] and spans[j][0] < spans[i][1]:
                dup += 1
    cov = set()
    for s in spans:
        cov.update(range(s[0], s[1]))
    holes = total_units - len(cov)
    return split, dup, holes


src = load_src()
tot = {'A': [0, 0, 0], 'B': [0, 0, 0]}
for M in ('maxword', 'smart'):
    a, b = load('.a.txt'), load('.b.txt')
    print(f"===== {M} =====")
    for name, d in (('A', a), ('B', b)):
        n_split = n_dup = n_holes = 0
        for k in sorted(d):
            spans = parse(d[k])
            s, dup, holes = analyze(spans, len(src[k].encode('utf-16-le')) // 2, src[k])
            n_split += s
            n_dup += dup
            n_holes += holes
            tot[name][0] += s
            tot[name][1] += dup
            tot[name][2] += holes
        print(f"  {name}: tokens={sum(len(v) for v in d.values())}"
              f"  拆分代理对={n_split}  重叠token={n_dup}  未覆盖unit={n_holes}")
    # 单调性抽查
    mono_bad = 0
    for k in sorted(a):
        for toks in (a[k], b.get(k, [])):
            begins = [s[0] for s in parse(toks)]
            if any(begins[i] > begins[i + 1] for i in range(len(begins) - 1)):
                mono_bad += 1
    print(f"  偏移非单调行数（A+B 合计）= {mono_bad}")
    if VERBOSE:
        for k in sorted(a)[:3]:
            print(f"  行#{k} A: {' '.join(esc(t) for t in a[k][:6])}")
            print(f"        B: {' '.join(esc(t) for t in b.get(k, [])[:6])}")

print()
print("汇总（全部行 × 2 口径）:")
print(f"  拆分代理对 : A={tot['A'][0]}  B={tot['B'][0]}   （预期 A=0；B=4 —— 高位代理恰落在 unit 4095 时被拆）")
print(f"  重叠 token : A={tot['A'][1]}  B={tot['B'][1]}   （预期 每口径 A=11，B=0）")
print(f"  未覆盖 unit: A={tot['A'][2]}  B={tot['B'][2]}   （A 丢弃 astral 故大量空洞；B 仅拆分处的低位代理）")
