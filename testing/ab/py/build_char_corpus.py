#!/usr/bin/env python3
"""构建字符系统考察语料（按 15 个类别组织）。每行一个用例。"""
import io
from collections import Counter

CASES = [
    # ---------- 1. 大小写 ----------
    ("case", "ABC"), ("case", "abc"), ("case", "AbC"),
    ("case", "ABC公司"), ("case", "abc公司"),
    ("case", "A股"), ("case", "a股"),
    ("case", "Hold住"), ("case", "hold住"),
    ("case", "K线"), ("case", "k线"),
    ("case", "QQ飞车"), ("case", "ｑｑ飞车"),
    ("case", "U盘"), ("case", "ｕ盘"),
    ("case", "H股"), ("case", "AB股"), ("case", "B股"),
    ("case", "iPhone"), ("case", "IPHONE"), ("case", "iphone"),
    # ---------- 2. Unicode 规范化 ----------
    ("norm", "café"), ("norm", "cafe\u0301"),
    ("norm", "ﬁnance"), ("norm", "finance"),
    ("norm", "①②③"), ("norm", "(1)(2)(3)"),
    ("norm", "㍿ 株式会社"), ("norm", "株式会社"),
    ("norm", "½ カップ"), ("norm", "1/2 カップ"),
    ("norm", "² 平方"), ("norm", "2 平方"),
    ("norm", "ﬀ ﬁ ﬂ"), ("norm", "ff fi fl"),
    ("norm", "Ⅻ 章"), ("norm", "XII 章"),
    ("norm", "Å"), ("norm", "A\u030a"),
    ("norm", "㐀"), ("norm", "豈"),
    ("norm", "㎝"), ("norm", "cm"),
    ("norm", "㎡"), ("norm", "m2"),
    # ---------- 3. emoji / 代理对 ----------
    ("emoji", "你好😀世界"), ("emoji", "😀"), ("emoji", "中文😀中文"),
    ("emoji", "a😀b"), ("emoji", "ab😀cd"), ("emoji", "1😀2"),
    ("emoji", "👍🏽test"), ("emoji", "🇨🇳flag"),
    ("emoji", "𠮷野家"), ("emoji", "曌"), ("emoji", "甲😀乙丙"),
    ("emoji", "😀a😀"), ("emoji", "test.😀x"),
    ("emoji", "❤️love"), ("emoji", "❤love"),
    ("emoji", "™️brand"), ("emoji", "™brand"),
    ("emoji", "🧑‍🚀astronaut"), ("emoji", "👨‍👩‍👧family"),
    ("emoji", "🏳️‍🌈flag"),
    ("emoji", "#️⃣hash"), ("emoji", "1️⃣one"),
    ("emoji", "👍👍👍"), ("emoji", "🚀🎉🆚"), ("emoji", "😀😀😀"),
    ("emoji", "𪚥𪚥𪚥"),
    # ---------- 4. 全角半角 ----------
    ("fw", "ＡＢＣ"), ("fw", "ａｂｃ"), ("fw", "１２３"),
    ("fw", "（）"), ("fw", "，。！？"),
    ("fw", "＃＆＋－．＠＿"), ("fw", "／：？＝～％"),
    ("fw", "　"), ("fw", "中　文"), ("fw", "中 文"), ("fw", "　中文　"),
    ("fw", "１，２３４"), ("fw", "ｱｲｳ"),
    ("fw", "¥£€"), ("fw", "￥"), ("fw", "￡￣￤"),
    ("fw", "￰￱"), ("fw", "￼"),
    ("fw", "ＡＢＣ公司"), ("fw", "１２３４"),
    # ---------- 5. 组合字符与变音符号 ----------
    ("comb", "café"), ("comb", "cafe\u0301"),
    ("comb", "résumé"), ("comb", "re\u0301sume\u0301"),
    ("comb", "naïve"), ("comb", "nai\u0308ve"),
    ("comb", "中文 café"),
    ("comb", "e\u0301e\u0301e\u0301"),
    ("comb", "a\u0300a\u0301a\u0302a\u0303"),
    ("comb", "か\u3099"), ("comb", "が"),
    ("comb", "か\u309A"), ("comb", "ガ"),
    ("comb", "x\u20DD"), ("comb", "⒭"), ("comb", "(r)"),
    ("comb", "é"), ("comb", "e"),
    # ---------- 6. 字素簇 ----------
    ("grapheme", "👍🏽"), ("grapheme", "👨‍👩‍👧‍👦"),
    ("grapheme", "é"), ("grapheme", "e\u0301"),
    ("grapheme", "🇨🇳"), ("grapheme", "🇺🇸"),
    ("grapheme", "🏳️‍🌈"),
    ("grapheme", "1️⃣"), ("grapheme", "👩🏿‍🦱"),
    ("grapheme", "👨‍👩‍👧"), ("grapheme", "🧑‍🚀"),
    # ---------- 7. 零宽字符 ----------
    ("zw", "中\u200b文"), ("zw", "中\u200c文"), ("zw", "中\u200d文"),
    ("zw", "中\u2060文"), ("zw", "中\ufeff文"), ("zw", "中\u00ad文"),
    ("zw", "a\u200bb"), ("zw", "a\u200bb"),
    ("zw", "中\u200e文"), ("zw", "中\u200f文"), ("zw", "中\u206a文"),
    ("zw", "\u200b"), ("zw", "\u200d\u200d"),
    # ---------- 8. 变体选择符 ----------
    ("vs", "❤️"), ("vs", "❤"),
    ("vs", "™️"), ("vs", "™"),
    ("vs", "☀️"), ("vs", "☀"),
    ("vs", "▶️"), ("vs", "▶"),
    ("vs", "⬛️"), ("vs", "⬛"),
    ("vs", "中\ufe0f文"), ("vs", "中\ufe0e文"),
    ("vs", "\U0001F19A️"), ("vs", "\U0001F19A"),
    # ---------- 9. 标点与空白 ----------
    ("punct", "中文，英文。测试！"),
    ("punct", "Hello, world. Test!"),
    ("punct", "中文;英文:测试?"),
    ("punct", "（中文）"), ("punct", "【中文】"),
    ("punct", "“引用”"), ("punct", "‘引用’"),
    ("punct", "中—文"), ("punct", "中–文"), ("punct", "中…文"),
    ("punct", "a,b.c"), ("punct", "1,000.00"), ("punct", "1.5.2"),
    ("punct", "a b  c   d"), ("punct", "a\tb"), ("punct", "a\nb"),
    ("punct", "中 文"), ("punct", "中 文"), ("punct", "中　文"),
    ("punct", "中 文"), ("punct", "中文　测试"),
    ("punct", "！＃＄％"), ("punct", "、。〆"),
    # ---------- 10. 中英数混合 ----------
    ("mix", "HelloWorld中文"), ("mix", "中文HelloWorld"),
    ("mix", "IKAnalyzer2012是一个轻量级的中文分词工具包"),
    ("mix", "研究生命起源2024"), ("mix", "test123abc"), ("mix", "abc123def"),
    ("mix", "中1文"), ("mix", "中a文"), ("mix", "3.14是圆周率"),
    ("mix", "100万元"), ("mix", "iPhone 15 Pro Max"),
    ("mix", "COVID-19疫情"), ("mix", "A4纸"), ("mix", "5G网络"),
    ("mix", "C++编程"), ("mix", "C#语言"),
    ("mix", "2的10次方"), ("mix", "第3章"),
    # ---------- 11. 同形异义字 ----------
    ("homo", "аpple"), ("homo", "apple"), ("homo", "аpple vs apple"),
    ("homo", "еmail"), ("homo", "email"),
    ("homo", "раssword"), ("homo", "password"),
    ("homo", "οrange"), ("homo", "orange"),
    ("homo", "Gοοgle"), ("homo", "Google"),
    ("homo", "АБС"), ("homo", "中а文中"),
    ("homo", "аdministrator"), ("homo", "administrator"),
    # ---------- 12. Bidi ----------
    ("bidi", "مرحبا بالعالم"), ("bidi", "שלום עולם"),
    ("bidi", "عربي 123"), ("bidi", "עברית 456"),
    ("bidi", "مُحَمَّد"), ("bidi", "العربية"),
    ("bidi", "כ״ג יזכור"),
    ("bidi", "中文عربي混合"), ("bidi", "עברית中文混合"),
    ("bidi", "ߊߋߌ"), ("bidi", "٠١٢٣٤"), ("bidi", "۰۱۲۳۴"),
    # ---------- 13. URL / 邮箱 / 版本号 ----------
    ("url", "https://www.example.com/path?query=1"),
    ("url", "http://example.com"), ("url", "www.example.com"),
    ("url", "example.com"), ("url", "linliangyi2005@gmail.com"),
    ("url", "user.name@example.co.uk"), ("url", "user+tag@example.com"),
    ("url", "a@b.c"), ("url", "windos2000"),
    ("url", "linliangyi2005@gmail.com和windos2000"),
    ("url", "v1.2.3"), ("url", "1.2.3.4"), ("url", "v2.0.0-rc1"),
    ("url", "version 1.10.6"), ("url", "elasticsearch-9.4.0"),
    ("url", "C:\\Users\\test"), ("url", "/usr/local/bin"),
    ("url", "file:///home/user"), ("url", "a.b.c.d"),
    ("url", "1.2.3-beta.4"), ("url", "test_123"), ("url", "user#123"),
    ("url", "a&b+c"), ("url", "100%"), ("url", "https://中文.com"),
    # ---------- 14. 特殊 / 控制 / 未分配 ----------
    ("misc", "中文\u0000测试"), ("misc", "中文\u0007测试"),
    ("misc", "中文\u001b测试"),
    ("misc", "中文\ud800测试"), ("misc", "中文\udc00测试"),
    ("misc", "中文\ufffd测试"), ("misc", "中文\ufffe测试"),
    ("misc", "中文\uffff测试"),
    ("misc", ""), ("misc", "中文测试"),
    ("misc", "\U000E0001"), ("misc", "中文\U0001F1E6测试"),
    ("misc", "\U0001F3F4\U000E0067\U000E0062\U000E0065\U000E006E\U000E0067\U000E007F"),
    ("misc", "中文\U000E0100测试"),
]

def main():
    # errors='surrogatepass'：允许把孤立代理（WTF-8/CESU-8 形态）写入文件，
    # 以便观察 Java 端 UTF-8 解码器如何处理畸形输入。
    with io.open('char_corpus.txt', 'w', encoding='utf-8', errors='surrogatepass') as f:
        for cat, text in CASES:
            safe = text.replace('\n', '\\n').replace('\t', '\\t')
            f.write(safe + '\n')
    with io.open('char_corpus_cat.tsv', 'w', encoding='utf-8') as f:
        for i, (cat, _) in enumerate(CASES):
            f.write(f"{i}\t{cat}\n")
    print(f"用例数 = {len(CASES)}")
    print("类别分布:", dict(Counter(c for c, _ in CASES)))

if __name__ == '__main__':
    main()
