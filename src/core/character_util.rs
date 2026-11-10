//! 字符集识别和规格化工具模块。
//!
//! # 对齐基准
//!
//! 实现基准为 **analysis-ik** `master@6d2d70f`
//! （`core/src/main/java/org/wltea/analyzer/core/CharacterUtil.java`）；
//! 与 u6（`CharacterUtil.java`，2012-05-17）的差异逐条列于各函数注释。
//! 本文件新增的 Rust 侧能力：astral 分类（[`CHAR_ASTRAL`]）与
//! [`crate::cfg::AstralMode`] 驱动的补充平面处理。

use crate::cfg::AstralMode;

// 常量定义，匹配 Java 版本
pub(crate) const CHAR_USELESS: i32 = 0;
pub(crate) const CHAR_ARABIC: i32 = 0x00000001;
pub(crate) const CHAR_ENGLISH: i32 = 0x00000002;
pub(crate) const CHAR_CHINESE: i32 = 0x00000004;
pub(crate) const CHAR_OTHER_CJK: i32 = 0x00000008;
/// 补充平面字符（emoji、符号等，仅 [`AstralMode::Tokens`] 下产生）。
///
/// 对应 analysis-ik 的 `CHAR_SURROGATE`（`CharacterUtil.java:44`，因 Java
/// `char` 是 UTF-16 码元，代理区即补充平面）；Rust `char` 为标量值，
/// 无「代理对」中间态，故按码点直接分类。
///
/// 刻意不复用 2 的幂位掩码风格中的空位约定：全库对 `char_types` 只做 `==`
/// 比较（analysis-ik 的 `0x16` 同样不是 2 的幂，功能无碍）。
pub(crate) const CHAR_ASTRAL: i32 = 0x00000010;

/// 识别字符类型。
///
/// # 对齐说明
///
/// - 分类模型与 u6 / analysis-ik 相同：`identifyCharType`（u6
///   `CharacterUtil.java:50-82`；analysis-ik 同文件 `:79-83` 仅多代理区分支）。
/// - **差异 1（Rust 标量值）**：u6/B 以 UTF-16 码元工作，补充平面字符落入
///   `CHAR_USELESS`；本函数按码点处理，CJK 扩展 B–I 由 [`AstralMode`] 决定
///   识别为 `CHAR_CHINESE` 还是丢弃。
/// - **差异 2（新增 astral 分类）**：[`AstralMode::Tokens`] 下其余补充平面
///   字符分类为 [`CHAR_ASTRAL`]（机制对齐 analysis-ik 的
///   `SurrogatePairSegmenter` 目标，但实现不照搬其缺陷）。
///
/// # Arguments
/// * `input` - 待识别的字符
/// * `astral` - 补充平面字符处理模式（来自 `Config::astral_mode`）
///
/// # Returns
/// 返回定义的字符类型常量。
pub(crate) fn identify_char_type(input: char, astral: AstralMode) -> i32 {
    match input {
        '0'..='9' => CHAR_ARABIC,
        'a'..='z' | 'A'..='Z' => CHAR_ENGLISH,
        _ => {
            let cp = input as u32;
            match cp {
                // --- CJK 主区和扩展A区 (与原版一致) ---
                0x4E00..=0x9FFF => CHAR_CHINESE, // CJK Unified Ideographs (中日韩统一表意文字)
                0x3400..=0x4DBF => CHAR_CHINESE, // CJK Unified Ideographs Extension A (扩展A)
                0xF900..=0xFAFF => CHAR_CHINESE, // CJK Compatibility Ideographs (兼容区)

                // --- CJK 统一表意文字扩展 B–I（Unicode 15.1）---
                // u6/analysis-ik 因 Java `char`（UTF-16 码元）把补充平面拆为代理对、
                // 落入 USELESS 丢弃；本 crate 用 Rust `char`（Unicode 标量值）直接处理。
                // `Drop` = u6 严格口径（parity 预设钉此值）；其余模式识别为汉字。
                // 区间：B(20000-2A6DF) C(2A700-2B73F) D(2B740-2B81F) E(2B820-2CEAF)
                //       F(2CEB0-2EBEF) I(2EBF0-2EE5D) G(30000-3134F) H(31350-323AF)
                0x20000..=0x2A6DF
                | 0x2A700..=0x2B73F
                | 0x2B740..=0x2B81F
                | 0x2B820..=0x2CEAF
                | 0x2CEB0..=0x2EBEF
                | 0x2EBF0..=0x2EE5D
                | 0x30000..=0x3134F
                | 0x31350..=0x323AF => match astral {
                    AstralMode::Drop => CHAR_USELESS,
                    _ => CHAR_CHINESE,
                },

                // --- 其余补充平面字符（emoji、符号、变体标签等）---
                // `Tokens` 模式下输出为单字 CNCHAR 词元；其余模式丢弃（= u6 行为）。
                0x10000..=0x10FFFF => match astral {
                    AstralMode::Tokens => CHAR_ASTRAL,
                    _ => CHAR_USELESS,
                },

                // --- 其他 CJK 字符 (日韩文、全角字符等) ---
                0xFF00..=0xFFEF => CHAR_OTHER_CJK, // Half-width and Full-width Forms (半角及全角)
                0xAC00..=0xD7AF => CHAR_OTHER_CJK, // Hangul Syllables (朝鲜文音节)
                0x1100..=0x11FF => CHAR_OTHER_CJK, // Hangul Jamo (朝鲜文)
                0x3130..=0x318F => CHAR_OTHER_CJK, // Hangul Compatibility Jamo (朝鲜文兼容)
                0x3040..=0x309F => CHAR_OTHER_CJK, // Hiragana (日文平假名)
                0x30A0..=0x30FF => CHAR_OTHER_CJK, // Katakana (日文片假名)
                0x31F0..=0x31FF => CHAR_OTHER_CJK, // Katakana Phonetic Extensions (日文片假名语音扩展)

                _ => CHAR_USELESS,
            }
        }
    }
}

/// 归一化字符（`regularize`）。
///
/// # 对齐说明
///
/// 结构与 u6 `CharacterUtil.java:89-101` / analysis-ik `:94-103` **逐字同构
/// 的 else-if 链**：全角空格 → 半角；全角字符 → 半角；ASCII 大写 → 小写
/// （受 `lowercase` 开关控制，开关即 analysis-ik 5.0 的 `enableLowercase`）。
///
/// **关键语义**：命中前两个分支后**不再进入**小写分支，故全角大写 `Ａ`
/// 只降宽度为 `A`，不叠加小写。历史上的「顺序式」实现（先全角→半角、
/// 再条件小写）会把 `Ａ` 折成 `a`——那是 u6 与 analysis-ik **都没有**的
/// 第三种行为（缺陷分析记录；
/// 设计决策 3）。
pub(crate) fn regularize(input: char, lowercase: bool) -> char {
    if input == '\u{3000}' {
        // 全角空格 (U+3000) → 半角空格 (U+0020)
        ' '
    } else if ('\u{FF01}'..='\u{FF5E}').contains(&input) {
        // 全角字符（除空格外）→ 半角：码点减去 65248（u6: `65280 < x < 65375`，
        // 双端开区间，精确等于 U+FF01–U+FF5E）
        char::from_u32(input as u32 - 65248).unwrap_or(input)
    } else if lowercase && input.is_ascii_uppercase() {
        // ASCII 大写 → 小写（与原版 2012_u6 的 `+32` 语义一致，仅 ASCII）。
        // `lowercase` 来自 `enable_lowercase` 开关（默认 true = 原版默认行为；
        // false 保留原文大小写，严格 A 语义。吸收自 analysis-ik 5.0 的 enableLowercase）。
        input.to_ascii_lowercase()
    } else {
        input
    }
}

// --- 单元测试 ---
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_regularize() {
        // lowercase=true（默认，对齐 2012_u6 原版默认行为）：
        // **半角** ASCII 大写→小写；**全角**大写只降宽度、不叠加小写（else-if 语义）。
        assert_eq!(regularize('C', true), 'c');
        assert_eq!(regularize('d', true), 'd');
        assert_eq!(regularize('Z', true), 'z');
        assert_eq!(regularize('Ａ', true), 'A'); // 全角→半角，大小写保留（u6 语义）
        assert_eq!(regularize('Ｂ', true), 'B');
        // 全角→半角不受开关影响
        assert_eq!(regularize('１', true), '1');
        assert_eq!(regularize('，', true), ',');
        assert_eq!(regularize('\u{3000}', true), ' '); // 全角空格
        assert_eq!(regularize('３', true), '3');

        // lowercase=false（严格 A 语义）：保留原文大小写
        assert_eq!(regularize('Ａ', false), 'A'); // 全角→半角，大小写保留
        assert_eq!(regularize('C', false), 'C');
        assert_eq!(regularize('d', false), 'd');
        assert_eq!(regularize('Z', false), 'Z');
        // 全角→半角不受开关影响
        assert_eq!(regularize('１', false), '1');
        assert_eq!(regularize('，', false), ',');
    }

    #[test]
    fn test_identify_char_type() {
        // BMP 分类与 u6/analysis-ik 一致
        assert_eq!(identify_char_type('1', AstralMode::Ideographs), CHAR_ARABIC);
        assert_eq!(
            identify_char_type('a', AstralMode::Ideographs),
            CHAR_ENGLISH
        );
        assert_eq!(
            identify_char_type('Z', AstralMode::Ideographs),
            CHAR_ENGLISH
        );
        assert_eq!(
            identify_char_type('中', AstralMode::Ideographs),
            CHAR_CHINESE
        );
        assert_eq!(
            identify_char_type('国', AstralMode::Ideographs),
            CHAR_CHINESE
        );
        assert_eq!(
            identify_char_type('あ', AstralMode::Ideographs),
            CHAR_OTHER_CJK
        ); // 日文平假名
        assert_eq!(
            identify_char_type('가', AstralMode::Ideographs),
            CHAR_OTHER_CJK
        ); // 韩文
        // '，'（U+FF0C）在全角区段内，直接分类为 OTHER_CJK——与 u6 一致
        // （真实 pipeline 先 regularize 为 ',' 再 identify，那才是 USELESS）
        assert_eq!(
            identify_char_type('，', AstralMode::Ideographs),
            CHAR_OTHER_CJK
        );
        assert_eq!(
            identify_char_type(' ', AstralMode::Ideographs),
            CHAR_USELESS
        );

        // 全角逗号经 regularize 归一为半角逗号（两种模式均然），归一后为 USELESS
        assert_eq!(regularize('，', true), ',');
        assert_eq!(regularize('，', false), ',');
        assert_eq!(
            identify_char_type(regularize('，', true), AstralMode::Ideographs),
            CHAR_USELESS
        );

        // 补充平面：CJK 扩展 B–I 汉字（U+2A6A5 𪚥）
        assert_eq!(
            identify_char_type('\u{2A6A5}', AstralMode::Ideographs),
            CHAR_CHINESE,
            "Ideographs（默认）应把扩展区汉字识别为汉字"
        );
        assert_eq!(
            identify_char_type('\u{2A6A5}', AstralMode::Tokens),
            CHAR_CHINESE,
            "Tokens 同样识别扩展区汉字"
        );
        assert_eq!(
            identify_char_type('\u{2A6A5}', AstralMode::Drop),
            CHAR_USELESS,
            "Drop（u6 严格口径）应丢弃全部非 BMP 字符"
        );

        // 补充平面：emoji（U+1F600 😀）
        assert_eq!(
            identify_char_type('\u{1F600}', AstralMode::Ideographs),
            CHAR_USELESS,
            "默认模式丢弃 emoji（= u6 行为）"
        );
        assert_eq!(
            identify_char_type('\u{1F600}', AstralMode::Drop),
            CHAR_USELESS,
            "Drop 模式丢弃 emoji"
        );
        assert_eq!(
            identify_char_type('\u{1F600}', AstralMode::Tokens),
            CHAR_ASTRAL,
            "Tokens 模式把 emoji 分类为 CHAR_ASTRAL（可成为单字词元）"
        );
    }
}
