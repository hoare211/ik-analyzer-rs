//! 英文字符及阿拉伯数字子分词器。
//!
//! # 对齐基准
//!
//! 对应 analysis-ik `core/src/main/java/org/wltea/analyzer/core/LetterSegmenter.java`
//! （`master@6d2d70f`）。与 u6（`LetterSegmenter.java`）**语义等价**：B 仅把
//! 3 处「嵌套 if + `isBufferConsumed()`」合并为单行（B:158/201/250 vs
//! A:158-166/206-214/257-265，正好差 6 行），无语义变化（调研报告 §2.2）。

use crate::core::analyze_context::{AnalyzeContext, BufferLock};
use crate::core::character_util;
use crate::core::isegmenter::ISegmenter;
use crate::core::lexeme::Lexeme;

// 连接符号
const LETTER_CONNECTOR: &[char] = &['#', '&', '+', '-', '.', '@', '_'];
// 数字符号
const NUM_CONNECTOR: &[char] = &[',', '.'];

/// 英文字符及阿拉伯数字子分词器
pub(crate) struct LetterSegmenter {
    // 词元的开始位置
    start: Option<usize>,
    // 记录词元结束位置
    end: Option<usize>,

    // 字母起始位置
    english_start: Option<usize>,
    // 字母结束位置
    english_end: Option<usize>,

    // 阿拉伯数字起始位置
    arabic_start: Option<usize>,
    // 阿拉伯数字结束位置
    arabic_end: Option<usize>,
}

impl LetterSegmenter {
    const LOCK: BufferLock = BufferLock::LETTER;

    pub fn new() -> Self {
        // 注：`LETTER_CONNECTOR` / `NUM_CONNECTOR` 常量字面量本身已按码点升序，
        // 故 `is_letter_connector` / `is_num_connector` 可直接 `binary_search`。
        Self {
            start: None,
            end: None,
            english_start: None,
            english_end: None,
            arabic_start: None,
            arabic_end: None,
        }
    }

    /// 处理数字字母混合输出
    fn process_mix_letter(&mut self, context: &mut AnalyzeContext) -> bool {
        if self.start.is_none() {
            // 当前分词器尚未开始处理
            let current_type = context.current_char_type();
            if current_type == character_util::CHAR_ARABIC
                || current_type == character_util::CHAR_ENGLISH
            {
                self.start = Some(context.cursor());
                self.end = self.start;
            }
        } else {
            // 当前分词器正在处理
            let current_type = context.current_char_type();
            // 数字/字母 或 字母连接符：均只推进 end（两分支行为相同，合并以消除 clippy
            // `if_same_blocks`，亦更清晰）。
            if current_type == character_util::CHAR_ARABIC
                || current_type == character_util::CHAR_ENGLISH
                || (current_type == character_util::CHAR_USELESS
                    && self.is_letter_connector(context.current_char()))
            {
                self.end = Some(context.cursor());
            } else {
                // 遇到非 Letter 字符，输出词元
                if let (Some(start), Some(end)) = (self.start, self.end) {
                    let lexeme = Lexeme::new(
                        context.buffer_offset() as i32,
                        start as i32,
                        (end - start + 1) as i32,
                        Lexeme::TYPE_LETTER,
                    );
                    context.add_lexeme(lexeme);
                }
                self.start = None;
                self.end = None;
            }
        }

        // 判断缓冲区是否已读完
        if context.is_buffer_consumed() {
            if let (Some(start), Some(end)) = (self.start, self.end) {
                let lexeme = Lexeme::new(
                    context.buffer_offset() as i32,
                    start as i32,
                    (end - start + 1) as i32,
                    Lexeme::TYPE_LETTER,
                );
                context.add_lexeme(lexeme);
            }
            self.start = None;
            self.end = None;
        }

        // 判断是否需要锁定缓冲区
        self.start.is_some()
    }

    /// 处理纯英文字母输出
    fn process_english_letter(&mut self, context: &mut AnalyzeContext) -> bool {
        if self.english_start.is_none() {
            if context.current_char_type() == character_util::CHAR_ENGLISH {
                self.english_start = Some(context.cursor());
                self.english_end = self.english_start;
            }
        } else {
            if context.current_char_type() == character_util::CHAR_ENGLISH {
                self.english_end = Some(context.cursor());
            } else {
                if let (Some(start), Some(end)) = (self.english_start, self.english_end) {
                    let lexeme = Lexeme::new(
                        context.buffer_offset() as i32,
                        start as i32,
                        (end - start + 1) as i32,
                        Lexeme::TYPE_ENGLISH,
                    );
                    context.add_lexeme(lexeme);
                }
                self.english_start = None;
                self.english_end = None;
            }
        }

        if context.is_buffer_consumed() {
            if let (Some(start), Some(end)) = (self.english_start, self.english_end) {
                let lexeme = Lexeme::new(
                    context.buffer_offset() as i32,
                    start as i32,
                    (end - start + 1) as i32,
                    Lexeme::TYPE_ENGLISH,
                );
                context.add_lexeme(lexeme);
            }
            self.english_start = None;
            self.english_end = None;
        }

        self.english_start.is_some()
    }

    /// 处理阿拉伯数字输出
    fn process_arabic_letter(&mut self, context: &mut AnalyzeContext) -> bool {
        if self.arabic_start.is_none() {
            if context.current_char_type() == character_util::CHAR_ARABIC {
                self.arabic_start = Some(context.cursor());
                self.arabic_end = self.arabic_start;
            }
        } else {
            if context.current_char_type() == character_util::CHAR_ARABIC {
                self.arabic_end = Some(context.cursor());
            } else if context.current_char_type() == character_util::CHAR_USELESS
                && self.is_num_connector(context.current_char())
            {
                // 不输出，但不标记结束
            } else {
                if let (Some(start), Some(end)) = (self.arabic_start, self.arabic_end) {
                    let lexeme = Lexeme::new(
                        context.buffer_offset() as i32,
                        start as i32,
                        (end - start + 1) as i32,
                        Lexeme::TYPE_ARABIC,
                    );
                    context.add_lexeme(lexeme);
                }
                self.arabic_start = None;
                self.arabic_end = None;
            }
        }

        if context.is_buffer_consumed() {
            if let (Some(start), Some(end)) = (self.arabic_start, self.arabic_end) {
                let lexeme = Lexeme::new(
                    context.buffer_offset() as i32,
                    start as i32,
                    (end - start + 1) as i32,
                    Lexeme::TYPE_ARABIC,
                );
                context.add_lexeme(lexeme);
            }
            self.arabic_start = None;
            self.arabic_end = None;
        }

        self.arabic_start.is_some()
    }

    /// 判断是否是字母连接符号
    fn is_letter_connector(&self, input: char) -> bool {
        LETTER_CONNECTOR.binary_search(&input).is_ok()
    }

    /// 判断是否是数字连接符号
    fn is_num_connector(&self, input: char) -> bool {
        NUM_CONNECTOR.binary_search(&input).is_ok()
    }
}

impl ISegmenter for LetterSegmenter {
    fn analyze(&mut self, context: &mut AnalyzeContext) {
        let mut buffer_lock_flag = false;
        buffer_lock_flag |= self.process_english_letter(context);
        buffer_lock_flag |= self.process_arabic_letter(context);
        buffer_lock_flag |= self.process_mix_letter(context);

        if buffer_lock_flag {
            context.lock_buffer(Self::LOCK);
        } else {
            context.unlock_buffer(Self::LOCK);
        }
    }

    fn reset(&mut self) {
        self.start = None;
        self.end = None;
        self.english_start = None;
        self.english_end = None;
        self.arabic_start = None;
        self.arabic_end = None;
    }
}
