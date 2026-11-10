#!/usr/bin/env python3
"""字符系统调研：最终根因归因汇总。"""

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

cat, src = load_cat(), load_src()

ROOT = {
    # 类别 -> (根因, 说明)
}

print("=" * 78)
print("字符系统语料（254 用例 / 14 类别）A/B 差异根因归因")
print("=" * 78)
for M in ('maxword', 'smart'):
    a = load(f'cc_A_{M}.txt')
    b = load(f'cc_B_{M}.txt')
    same = sum(1 for k in a if a[k] == b.get(k))
    print(f"\n[{M}] 完全相同 {same}/{len(a)} = {same/len(a)*100:.1f}%")

    # 按根因分组
    groups = {}
    for k in sorted(a):
        if a[k] == b.get(k):
            continue
        ta, tb = a[k], b.get(k, [])
        sa, sb = set(ta) - set(tb), set(tb) - set(ta)
        c = cat[k]
        # 判定根因
        if all(t.startswith('a[') or t.startswith('is[') or t.startswith('this[')
               or t.startswith('the[') or t.startswith('an[') or t.startswith('and[')
               for t in sa) and not sb:
            root = '内置停用词（B 加载 stopword.dic，u6 无内置）'
        elif any('\\U' in esc(t) or '\\ud8' in esc(t) or '\\udc' in esc(t) for t in list(sa) + list(sb)) or not ta:
            root = '代理对/辅助平面字符（B 输出 CNCHAR，u6 丢弃）'
        elif any('\\u80a1' in esc(t) or '股' in t for t in list(sa) + list(sb)):
            root = '词典内容差异（B 删除 A股/B股/… 等 32 条）'
        else:
            root = '其他'
        groups.setdefault((c, root), []).append(k)

    for (c, root), ks in sorted(groups.items()):
        print(f"  [{c}] {root}  -> {len(ks)} 例")

    # 停用词根因的精确 token 清单
    sw = [t for k in sorted(a) if a[k] != b.get(k)
          for t in (set(a[k]) - set(b.get(k, [])))
          if t.split('[')[0] in ('a', 'is', 'this', 'the', 'an', 'and')]
    from collections import Counter
    print(f"  停用词根因的 A-only token 词面: {dict(Counter(t.split('[')[0] for t in sw))}")

print()
print("=" * 78)
print("预处理阶段（9500 code unit）")
print("=" * 78)
print("  identifyCharType 分类差异   : 2048 / 9500 = 21.6%（全部是代理区 U+D800-DFFF）")
print("  regularize 差异 (low=true)  : 0 / 9500")
print("  regularize 差异 (low=false) : 52 / 9500（26 个 ASCII A-Z）")
print()
print("=" * 78)
print("编码阶段（probe_enc 10 行 + probe_split 125 行）")
print("=" * 78)
print("  代理对被跨页拆分 : A=0, B=0（135 个位置 × 2 口径）")
print("  refill 重叠 token: A=11, B=0")
print("  偏移单位          : 两侧均为 UTF-16 code unit（代理对占 2 个单位）")
