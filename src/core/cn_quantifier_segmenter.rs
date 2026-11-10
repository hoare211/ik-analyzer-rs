//! 中文数量词子分词器。
//!
//! # 对齐基准
//!
//! 对应 analysis-ik `core/src/main/java/org/wltea/analyzer/core/CN_QuantifierSegmenter.java`
//! （`master@6d2d70f`）。与 u6（`CN_QuantifierSegmenter.java`）的关系：
//! **保持 u6 语义**——B 在 `processCount` 内追加的 `shouldMatchSingleChar`
//! 门控（B:173-202）会改变 `两个`/`两千万` 等**最高频数词短语**的切分
//! （u6 在数词累积中即扫，A 级实测级联效应，调研报告 §9.3），且是行为变更
//! 而非缺陷修复，**明确不吸收**（A 类已对齐项）。

use crate::core::analyze_context::{AnalyzeContext, BufferLock};
use crate::core::character_util;
use crate::core::isegmenter::ISegmenter;
use crate::core::lexeme::Lexeme;
use crate::dic::PersistentHit;

// --- 静态初始化 ---
//
// 中文数词字符表。刻意用**排序后的字面量**而非 `LazyLock<Vec<char>>`：
// 这 33 个码点是编译期常量，排序结果同样确定，`LazyLock` 只会给每字符的热路径
// 增加一次 lazy 状态原子读 + 一次堆 Vec 指针间接寻址（评审 P2-3）。
// 修改本表时必须保持码点升序，否则 `binary_search` 会静默失配。
//
// 原始未排序串保留在 `CHN_NUM`，仅供测试比对两者是否一致。
#[cfg(test)]
const CHN_NUM: &str = "一二两三四五六七八九十零壹贰叁肆伍陆柒捌玖拾百千万亿拾佰仟萬億兆卅廿";

/// `CHN_NUM` 去重后按码点升序的字符表（33 个唯一字符，`CHN_NUM` 原始串 34 字，
/// 其中"拾"重复出现一次）。升序是 `binary_search` 正确性的前提。
static CHN_NUMBER_CHARS: &[char] = &[
    '一', '七', '万', '三', '两', '九', '二', '五', '亿', '仟', '伍', '佰', '億', '兆', '八', '六',
    '十', '千', '卅', '叁', '四', '壹', '廿', '拾', '捌', '柒', '玖', '百', '肆', '萬', '贰', '陆',
    '零',
];

/// 判断字符是否为中文数词字符（升序数组二分查找，无运行时初始化）。
#[inline]
fn is_chn_number_char(c: char) -> bool {
    CHN_NUMBER_CHARS.binary_search(&c).is_ok()
}

/// 中文数量词子分词器
#[derive(Default)]
pub(crate) struct CnQuantifierSegmenter {
    // 数词的开始位置
    n_start: Option<usize>,
    // 数词的结束位置
    n_end: Option<usize>,
    // 【修改】待处理的量词 hit 队列，现在存储 PersistentHit
    count_hits: Vec<PersistentHit>,
    // 复用缓冲：避免每字符分配
    lexemes_buf: Vec<Lexeme>,
    next_count_hits_buf: Vec<PersistentHit>,
}

impl CnQuantifierSegmenter {
    const LOCK: BufferLock = BufferLock::QUAN;

    pub fn new() -> Self {
        Self::default()
    }

    /// 处理数词 (此部分逻辑与借用检查不冲突，保持不变)
    fn process_c_number(&mut self, context: &mut AnalyzeContext) {
        if self.n_start.is_none() {
            if context.current_char_type() == character_util::CHAR_CHINESE
                && is_chn_number_char(context.current_char())
            {
                self.n_start = Some(context.cursor());
                self.n_end = self.n_start;
            }
        } else {
            if context.current_char_type() == character_util::CHAR_CHINESE
                && is_chn_number_char(context.current_char())
            {
                self.n_end = Some(context.cursor());
            } else {
                self.output_num_lexeme(context);
                self.n_start = None;
                self.n_end = None;
            }
        }

        if context.is_buffer_consumed() && self.n_start.is_some() {
            self.output_num_lexeme(context);
            self.n_start = None;
            self.n_end = None;
        }
    }

    /// 判断是否需要扫描量词 (保持不变)
    fn need_count_scan(&self, context: &AnalyzeContext) -> bool {
        if self.n_start.is_some() || !self.count_hits.is_empty() {
            return true;
        } else {
            if let Some(l) = context.org_lexemes().peek_last() {
                if (l.lexeme_type() == Lexeme::TYPE_CNUM || l.lexeme_type() == Lexeme::TYPE_ARABIC)
                    && (l.begin() + l.length()) as usize == context.cursor()
                {
                    return true;
                }
            }
        }
        false
    }

    /// 添加数词词元到结果集 (保持不变)
    fn output_num_lexeme(&self, context: &mut AnalyzeContext) {
        if let (Some(start), Some(end)) = (self.n_start, self.n_end) {
            let lexeme = Lexeme::new(
                context.buffer_offset() as i32,
                start as i32,
                (end - start + 1) as i32,
                Lexeme::TYPE_CNUM,
            );
            context.add_lexeme(lexeme);
        }
    }
}

impl ISegmenter for CnQuantifierSegmenter {
    fn analyze(&mut self, context: &mut AnalyzeContext) {
        // 第一部分：处理数词
        self.process_c_number(context);

        // 第二部分：处理量词 (应用新的“计算-修改”模式)
        if self.need_count_scan(context) {
            if context.current_char_type() == character_util::CHAR_CHINESE {
                // --- 准备阶段 ---
                let dict = context.dictionary();
                let cursor = context.cursor();
                let segment_buff = context.segment_buff();
                let buffer_offset = context.buffer_offset();

                // --- 计算阶段 ---
                // 复用 self 上的缓冲
                self.lexemes_buf.clear();
                self.next_count_hits_buf.clear();

                // 1. 处理已有的 count_hits（遍历而非 take）
                for i in 0..self.count_hits.len() {
                    let p_hit = self.count_hits[i];
                    let current_hit =
                        dict.match_with_hit_in_quantifier(segment_buff, cursor, p_hit);
                    if current_hit.is_match() {
                        self.lexemes_buf.push(Lexeme::new(
                            buffer_offset as i32,
                            current_hit.begin(),
                            cursor as i32 - current_hit.begin() + 1,
                            Lexeme::TYPE_COUNT, // 量词类型
                        ));
                    }
                    if !current_hit.is_unmatch() && current_hit.is_prefix() {
                        if let Some(node_idx) = current_hit.get_matched_node() {
                            self.next_count_hits_buf
                                .push(PersistentHit::new(current_hit.begin(), node_idx));
                        }
                    }
                }
                self.count_hits.clear();

                // 2. 处理当前字符
                let single_char_hit = dict.match_in_quantifier_dict(segment_buff, cursor, 1);
                if single_char_hit.is_match() {
                    self.lexemes_buf.push(Lexeme::new(
                        buffer_offset as i32,
                        cursor as i32,
                        1,
                        Lexeme::TYPE_COUNT,
                    ));
                }
                if single_char_hit.is_prefix() {
                    if let Some(node_idx) = single_char_hit.get_matched_node() {
                        self.next_count_hits_buf
                            .push(PersistentHit::new(single_char_hit.begin(), node_idx));
                    }
                }

                // --- 修改阶段 ---
                std::mem::swap(&mut self.count_hits, &mut self.next_count_hits_buf);
                for lexeme in self.lexemes_buf.drain(..) {
                    context.add_lexeme(lexeme);
                }
            } else {
                // 如果需要扫描，但当前不是中文字符，则清空状态
                self.count_hits.clear();
            }
        }

        // --- 第三部分：清理和锁定缓冲区 ---
        if context.is_buffer_consumed() {
            self.count_hits.clear();
        }

        if self.n_start.is_none() && self.count_hits.is_empty() {
            context.unlock_buffer(Self::LOCK);
        } else {
            context.lock_buffer(Self::LOCK);
        }
    }

    fn reset(&mut self) {
        self.n_start = None;
        self.n_end = None;
        self.count_hits.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::{CHN_NUM, CHN_NUMBER_CHARS, is_chn_number_char};

    /// `CHN_NUMBER_CHARS` 必须严格升序 —— 这是 `binary_search` 正确性的前提。
    /// 一旦有人插入未排序的字面量，`binary_search` 会**静默失配**（不 panic，
    /// 只是悄悄漏判），故必须有此测试兜住。
    #[test]
    fn test_chn_number_chars_is_sorted_and_deduped() {
        assert!(
            CHN_NUMBER_CHARS.windows(2).all(|w| w[0] < w[1]),
            "CHN_NUMBER_CHARS 必须严格升序且无重复，否则 binary_search 会静默失配"
        );
    }

    /// 字面量表必须与 `CHN_NUM` 原始串完全一致（去重后同集合）。
    #[test]
    fn test_chn_number_chars_matches_source_string() {
        let mut expected: Vec<char> = CHN_NUM.chars().collect();
        expected.sort_unstable();
        expected.dedup();
        assert_eq!(
            CHN_NUMBER_CHARS,
            expected.as_slice(),
            "CHN_NUMBER_CHARS 与 CHN_NUM 不一致（改表时两处必须同步）"
        );
    }

    /// 逐字符核对判定结果，并确认非数词被拒。
    #[test]
    fn test_is_chn_number_char() {
        for c in CHN_NUM.chars() {
            assert!(is_chn_number_char(c), "'{c}' 应被判为中文数词字符");
        }
        for c in ['a', '中', '文', '个', '条', '，'] {
            assert!(!is_chn_number_char(c), "'{c}' 不应被判为中文数词字符");
        }
    }
}
