#!/usr/bin/env python3
"""构建字符系统考察字符表（UTF-16 code unit 序列）。

u6 与 analysis-ik 都在 Java `char`（UTF-16 code unit）上操作，
因此这里枚举的是「code unit」而非「code point」——这正是两者的实际工作单元。
"""
import sys

RANGES = [
    ("ASCII",                    0x0000, 0x007F, 1),
    ("Latin-1 Supplement",       0x0080, 0x00FF, 1),
    ("Latin Extended-A/B",       0x0100, 0x024F, 3),
    ("IPA Extensions",           0x0250, 0x02AF, 1),
    ("Spacing Modifiers",        0x02B0, 0x02FF, 1),
    ("Combining Diacritical",    0x0300, 0x036F, 1),   # 组合字符与变音符号
    ("Greek & Coptic",           0x0370, 0x03FF, 2),
    ("Cyrillic",                 0x0400, 0x04FF, 2),   # 同形异义字来源
    ("Cyrillic Supp",            0x0500, 0x052F, 1),
    ("Armenian",                 0x0530, 0x058F, 2),
    ("Hebrew",                   0x0590, 0x05FF, 1),   # Bidi
    ("Arabic",                   0x0600, 0x06FF, 1),   # Bidi
    ("Syriac",                   0x0700, 0x074F, 2),
    ("Arabic Supp",              0x0750, 0x077F, 2),
    ("Arabic Ext-A",             0x08A0, 0x08FF, 2),
    ("Devanagari",               0x0900, 0x097F, 3),
    ("Indic/SEA scripts",        0x0980, 0x0FFF, 7),
    ("Hangul Jamo",              0x1100, 0x11FF, 3),
    ("Ethiopic",                 0x1200, 0x137F, 5),
    ("Punctuation Ext",          0x2E00, 0x2E7F, 1),
    ("CJK Radicals/Kangxi",      0x2E80, 0x2FDF, 5),
    ("Ideographic Description",  0x2FF0, 0x2FFF, 1),
    ("CJK Sym & Punct",          0x3000, 0x303F, 1),   # 全角空格、中日韩标点
    ("Hiragana",                 0x3040, 0x309F, 1),
    ("Katakana",                 0x30A0, 0x30FF, 1),
    ("Bopomofo",                 0x3100, 0x312F, 3),
    ("Hangul Compat Jamo",       0x3130, 0x318F, 3),
    ("Kanbun",                   0x3190, 0x319F, 1),
    ("Enclosed CJK",             0x3200, 0x32FF, 3),
    ("CJK Compatibility",        0x3300, 0x33FF, 2),   # ㌀ ㎝ ½
    ("CJK Ext A (sample)",       0x3400, 0x4DBF, 17),
    ("Yijing Hexagrams",         0x4DC0, 0x4DFF, 3),
    ("CJK Unified (sample)",     0x4E00, 0x9FFF, 11),
    ("Yi Syllables",             0xA000, 0xA48F, 9),
    ("Hangul Syllables(sample)", 0xAC00, 0xD7AF, 41),
    ("Private Use Area",         0xE000, 0xF8FF, 41),
    ("CJK Compat Ideographs",    0xF900, 0xFAFF, 3),
    ("Arabic Pres Forms-A",      0xFB00, 0xFB4F, 1),   # 连字 ﬀ ﬁ ﬂ + Bidi
    ("Arabic Pres Forms-B",      0xFE70, 0xFEFF, 3),
    ("Variation Selectors",      0xFE00, 0xFE0F, 1),   # 变体选择符
    ("Vertical Forms",           0xFE10, 0xFE1F, 1),
    ("Combining Half Marks",     0xFE20, 0xFE2F, 1),
    ("CJK Compat Forms",         0xFE30, 0xFE4F, 1),
    ("Small Form Variants",      0xFE50, 0xFE6F, 1),
    ("Halfwidth/Fullwidth Forms",0xFF00, 0xFFEF, 1),   # 全角半角（全量 240 个）
    ("Specials",                 0xFFF9, 0xFFFD, 1),
    ("Latin Ext Additional",     0x1E00, 0x1EFF, 7),
    ("Greek Ext",                0x1F00, 0x1FFF, 7),
    ("Letterlike Symbols",       0x2100, 0x214F, 1),   # TM (TM) (R)
    ("Number Forms",             0x2150, 0x218F, 1),   # 1/3 VIII
    ("Arrows",                   0x2190, 0x21FF, 3),
    ("Math Operators",           0x2200, 0x22FF, 5),
    ("Misc Technical",           0x2300, 0x23FF, 5),
    ("Control Pictures",         0x2400, 0x243F, 1),
    ("Enclosed Alphanumerics",   0x2460, 0x24FF, 1),   # circled-1 paren-1
    ("Box Drawing",              0x2500, 0x257F, 5),
    ("Block Elements",           0x2580, 0x259F, 3),
    ("Geometric Shapes",         0x25A0, 0x25FF, 3),
    ("Misc Symbols",             0x2600, 0x26FF, 2),
    ("Dingbats",                 0x2700, 0x27BF, 2),
    ("Misc Math Symbols-A",      0x27C0, 0x27EF, 2),
    ("Supp Arrows-A",            0x27F0, 0x27FF, 2),
    ("Braille",                  0x2800, 0x28FF, 7),
    ("Supp Arrows-B/Math",       0x2900, 0x2AFF, 13),
    ("Supp Symbols & Arrows",    0x2B00, 0x2BFF, 3),
    ("Latin Ext-C/D/E",          0x2C60, 0x2C7F, 1),
    ("Tags",                     0xE0000, 0xE007F, 1),  # 语言标签（零宽族）
]

PINPOINT = {
    # 零宽字符
    "ZWSP U+200B": 0x200B, "ZWNJ U+200C": 0x200C, "ZWJ U+200D": 0x200D,
    "LRM U+200E": 0x200E, "RLM U+200F": 0x200F,
    "WORD JOINER U+2060": 0x2060, "BOM/ZWNBSP U+FEFF": 0xFEFF,
    "SOFT HYPHEN U+00AD": 0x00AD, "MONGOLIAN VOWEL SEP U+180E": 0x180E,
    "INHIBIT SWAP U+206A": 0x206A, "ACTIVATE SYM SWAP U+206B": 0x206B,
    "INHIBIT SYM SWAP U+206C": 0x206C, "ACTIVATE ARABIC U+206D": 0x206D,
    "NOMINAL DIGIT SHAPES U+206E": 0x206E, "NOMINAL BASE U+206F": 0x206F,
    # 空白家族
    "SPACE U+0020": 0x0020, "NBSP U+00A0": 0x00A0, "OGHAM U+1680": 0x1680,
    "EN QUAD U+2000": 0x2000, "EM QUAD U+2001": 0x2001,
    "EN SPACE U+2002": 0x2002, "EM SPACE U+2003": 0x2003,
    "THREE-PER-EM U+2004": 0x2004, "FOUR-PER-EM U+2005": 0x2005,
    "SIX-PER-EM U+2006": 0x2006, "FIGURE U+2007": 0x2007,
    "PUNCTUATION U+2008": 0x2008, "THIN U+2009": 0x2009,
    "HAIR U+200A": 0x200A, "NARROW NBSP U+202F": 0x202F,
    "MMSP U+205F": 0x205F, "IDEOGRAPHIC U+3000": 0x3000,
    "LINE SEP U+2028": 0x2028, "PARA SEP U+2029": 0x2029,
    # 全角/半角关键点（regularize 的 65280<x<65375 双端开区间）
    "FW NUL-ish U+FF00": 0xFF00, "FW ! U+FF01": 0xFF01,
    "FW Z U+FF3A": 0xFF3A, "FW z U+FF5A": 0xFF5A,
    "U+FF5D": 0xFF5D, "U+FF5E": 0xFF5E, "U+FF5F": 0xFF5F, "U+FF60": 0xFF60,
    "HW halfwidth-ideographic-full-stop U+FF61": 0xFF61,
    "HW halfwidth-katakana-wo U+FF66": 0xFF66,
    "HW halfwidth-katakana-small-ke U+FF9F": 0xFF9F,
    "FW cent U+FFE0": 0xFFE0, "FW pound U+FFE1": 0xFFE1,
    "FW not U+FFE2": 0xFFE2, "FW macron U+FFE3": 0xFFE3,
    "FW broken-bar U+FFE4": 0xFFE4, "FW yen U+FFE5": 0xFFE5,
    "FW won U+FFE6": 0xFFE6,
    "HW form U+FFE8": 0xFFE8, "HW form U+FFEE": 0xFFEE,
    # 标点与符号
    "EM DASH U+2014": 0x2014, "EN DASH U+2013": 0x2013,
    "MINUS U+2212": 0x2212, "FW hyphen U+FF0D": 0xFF0D,
    "CJK full stop U+3002": 0x3002, "CJK comma U+3001": 0x3001,
    "FW comma U+FF0C": 0xFF0C, "FW exclam U+FF01": 0xFF01,
    "ellipsis U+2026": 0x2026, "two-dot U+2025": 0x2025,
    "LSQ U+2018": 0x2018, "RSQ U+2019": 0x2019,
    "LDQ U+201C": 0x201C, "RDQ U+201D": 0x201D,
    "FW lparen U+FF08": 0xFF08,
    # URL / 邮箱 / 版本号连接符（Letter_Connector / Num_Connector）
    "hash #": 0x0023, "amp &": 0x0026, "plus +": 0x002B, "hyphen -": 0x002D,
    "period .": 0x002E, "at @": 0x0040, "underscore _": 0x005F,
    "slash /": 0x002F, "colon :": 0x003A, "question ?": 0x003F,
    "equals =": 0x003D, "tilde ~": 0x007E, "percent %": 0x0025,
    "comma ,": 0x002C, "semicolon ;": 0x003B, "pipe |": 0x007C,
    "FW solidus U+FF0F": 0xFF0F, "FW colon U+FF1A": 0xFF1A,
    "FW hash U+FF03": 0xFF03, "FW amp U+FF06": 0xFF06,
    "FW plus U+FF0B": 0xFF0B, "FW hyphen U+FF0D": 0xFF0D,
    "FW period U+FF0E": 0xFF0E, "FW at U+FF20": 0xFF20,
    "FW low-line U+FF3F": 0xFF3F,
    # 数字
    "digit 0": 0x0030, "digit 9": 0x0039,
    "FW 0 U+FF10": 0xFF10, "FW 9 U+FF19": 0xFF19,
    "Arabic-Indic 0 U+0660": 0x0660, "Arabic-Indic 9 U+0669": 0x0669,
    "Ext Arabic-Indic 0 U+06F0": 0x06F0, "Ext Arabic-Indic 9 U+06F9": 0x06F9,
    "Devanagari 0 U+0966": 0x0966, "Thai 0 U+0E50": 0x0E50,
    "superscript 1 U+00B9": 0x00B9, "superscript 2 U+00B2": 0x00B2,
    "superscript 0 U+2070": 0x2070, "superscript 9 U+2079": 0x2079,
    "subscript 0 U+2080": 0x2080, "subscript 9 U+2089": 0x2089,
    "circled 1 U+2460": 0x2460, "paren 1 U+3200": 0x3200,
    "Roman VIII U+2167": 0x2167, "vulgar 1/3 U+2153": 0x2153,
    # 组合字符与变音符号
    "e-acute precomposed U+00E9": 0x00E9, "plain e": 0x0065,
    "COMBINING ACUTE U+0301": 0x0301, "COMBINING GRAVE U+0300": 0x0300,
    "COMBINING TILDE U+0303": 0x0303, "COMBINING RING ABOVE U+030A": 0x030A,
    "COMBINING CEDILLA U+0327": 0x0327,
    "COMBINING DIAERESIS U+0308": 0x0308,
    "COMBINING ENCLOSING CIRCLE U+20DD": 0x20DD,
    "COMBINING KATAKANA-HIRAGANA VOICED U+3099": 0x3099,
    "COMBINING KATAKANA-HIRAGANA SEMI-VOICED U+309A": 0x309A,
    "KATAKANA-HIRAGANA VOICED MARK U+309B": 0x309B,
    "Latin small long s with dot U+1E9B": 0x1E9B,
    # 同形异义字（homoglyph）
    "CYRILLIC a U+0430": 0x0430, "CYRILLIC ie U+0435": 0x0435,
    "CYRILLIC o U+043E": 0x043E, "CYRILLIC er U+0440": 0x0440,
    "CYRILLIC es U+0441": 0x0441, "CYRILLIC ha U+0445": 0x0445,
    "GREEK omicron U+03BF": 0x03BF, "GREEK rho U+03C1": 0x03C1,
    "GREEK alpha U+03B1": 0x03B1, "GREEK Alpha U+0391": 0x0391,
    "LATIN o": 0x006F, "LATIN e": 0x0065, "LATIN a": 0x0061,
    "LATIN c": 0x0063, "LATIN x": 0x0078, "LATIN p": 0x0070,
    # CJK 关键
    "CJK zhong U+4E2D": 0x4E2D, "CJK guo U+56FD": 0x56FD,
    "CJK last U+9FA5": 0x9FA5, "ExtA first U+3400": 0x3400,
    "ExtA last U+4DBF": 0x4DBF, "Compat first U+F900": 0xF900,
    "Hiragana a U+3042": 0x3042, "Katakana a U+30A2": 0x30A2,
    "HW katakana a U+FF71": 0xFF71,
    "Hangul ga U+AC00": 0xAC00,
    "Bopomofo b U+3105": 0x3105,
    "FW open-bracket U+FF3B": 0xFF3B,
    "Interlinear annotation U+FFF9": 0xFFF9,
}

ASTRAL_RANGES = [
    ("Emoji: Misc Symbols & Pictographs", 0x1F300, 0x1F5FF, 1),
    ("Emoji: Emoticons",                 0x1F600, 0x1F64F, 1),
    ("Emoji: Transport & Map",           0x1F680, 0x1F6FF, 1),
    ("Emoji: Supplemental Symbols",      0x1F900, 0x1F9FF, 1),
    ("Emoji: Symbols&Pictographs Ext-A", 0x1FA70, 0x1FAFF, 1),
    ("Emoji: Regional Indicators",       0x1F1E6, 0x1F1FF, 1),
    ("Emoji: Skin tone modifiers",       0x1F3FB, 0x1F3FF, 1),
    ("Emoji: Misc Symbols & Arrows",     0x2B00, 0x2B55, 1),
    ("Emoji: Dingbats (emoji subset)",   0x2700, 0x27BF, 1),
    ("Emoji: Letterlike (emoji subset)", 0x2100, 0x214F, 1),
    ("Emoji: Supplemental Symbols-B",    0x1F004, 0x1F0CF, 1),
    ("CJK Ext B (sample)",               0x20000, 0x2A6DF, 137),
    ("CJK Ext C/D (sample)",             0x2A700, 0x2B81F, 197),
    ("CJK Ext E..I (sample)",            0x2B820, 0x323AF, 419),
    ("Tags (astral part)",               0xE0000, 0xE01EF, 1),
    ("Variation Selectors Supplement",   0xE0100, 0xE01EF, 1),
    ("Mahjong/Domino/Cards",             0x1F000, 0x1F0FF, 1),
    ("Alchemical/Shapes",                0x1F700, 0x1F77F, 1),
]

ASTRAL_PINPOINT = {
    "ji U+20BB7": 0x20BB7,
    "zhe U+2A6A5": 0x2A6A5,
    "first ExtB U+20000": 0x20000,
    "grinning U+1F600": 0x1F600,
    "heavy-black-heart U+2764 (BMP)": 0x2764,
    "trade-mark U+2122 (BMP)": 0x2122,
    "copyright U+00A9 (BMP)": 0x00A9,
    "waving-white-flag U+1F3F3": 0x1F3F3,
    "adult U+1F9D1": 0x1F9D1,
    "thumbs-up U+1F44D": 0x1F44D,
    "regional C U+1F1E8": 0x1F1E8,
    "rocket U+1F680": 0x1F680,
    "party-popper U+1F389": 0x1F389,
    "vs-button U+1F19A": 0x1F19A,
    "prohibited U+1F232": 0x1F232,
    "negative-squared-latin-a U+1F130": 0x1F130,
    "high-voltage U+26A1": 0x26A1,
    "black-scissors U+2702": 0x2702,
    "sun U+2600": 0x2600,
    "play-button U+25B6": 0x25B6,
    "right-arrow U+27A1": 0x27A1,
    "black-large-square U+2B1B": 0x2B1B,
    "joker U+1F0A0": 0x1F0A0,
    "black-flag U+1F3F4": 0x1F3F4,
}

SURROGATE_RANGE = (0xD800, 0xDFFF)


def main():
    units = []
    meta = {}
    for name, lo, hi, step in RANGES:
        for u in range(lo, hi + 1, step):
            units.append(u)
            meta.setdefault(u, name)
    for name, cp in PINPOINT.items():
        units.append(cp)
        meta[cp] = name
    for name, lo, hi, step in ASTRAL_RANGES:
        for cp in range(lo, hi + 1, step):
            if cp > 0xFFFF:
                c = cp - 0x10000
                hi_ = 0xD800 + (c >> 10)
                lo_ = 0xDC00 + (c & 0x3FF)
                units.extend([hi_, lo_])
                meta[hi_] = name + " [high surrogate]"
                meta[lo_] = name + " [low surrogate]"
    for name, cp in ASTRAL_PINPOINT.items():
        if cp > 0xFFFF:
            c = cp - 0x10000
            hi_ = 0xD800 + (c >> 10)
            lo_ = 0xDC00 + (c & 0x3FF)
            units.extend([hi_, lo_])
            meta[hi_] = name + " [high surrogate]"
            meta[lo_] = name + " [low surrogate]"
        else:
            units.append(cp)
            meta[cp] = name
    # 全部代理区 code unit
    for u in range(SURROGATE_RANGE[0], SURROGATE_RANGE[1] + 1):
        units.append(u)
        meta[u] = "Surrogate area"

    # 去重（保序）
    seen = set()
    uniq = []
    for u in units:
        if u in seen:
            continue
        seen.add(u)
        uniq.append(u)

    with open("units.txt", "w", encoding="utf-8") as f:
        for u in uniq:
            f.write(f"{u}\n")
    with open("units_meta.tsv", "w", encoding="utf-8") as f:
        for u in uniq:
            f.write(f"{u}\t{meta.get(u, '?')}\n")
    print(f"code units = {len(uniq)}")


if __name__ == "__main__":
    main()
