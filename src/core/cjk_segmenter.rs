use crate::core::analyze_context::{AnalyzeContext, BufferLock};
use crate::core::character_util;
use crate::core::isegmenter::ISegmenter;
use crate::core::lexeme::Lexeme;
use crate::dic::PersistentHit;

/// 中文/日文/韩文子分词器。
///
/// # 对齐基准
///
/// 对应 analysis-ik `core/src/main/java/org/wltea/analyzer/core/CJKSegmenter.java`
/// （`master@6d2d70f`）——该文件与 u6 同名文件**逐字相同**（两侧 `:54` 门控一致），
/// 故本文件无 A/B 差异。

#[derive(Default)]
pub(crate) struct CJKSegmenter {
    tmp_hits: Vec<PersistentHit>,
    // 复用缓冲：避免每字符分配
    lexemes_buf: Vec<Lexeme>,
    next_hits_buf: Vec<PersistentHit>,
}

impl CJKSegmenter {
    pub fn new() -> Self {
        Self::default()
    }
    const LOCK: BufferLock = BufferLock::CJK;
}

impl ISegmenter for CJKSegmenter {
    fn analyze(&mut self, context: &mut AnalyzeContext) {
        // 门控：**非 USELESS 即推进词典匹配**——与 u6 `CJKSegmenter.java:54` /
        // analysis-ik 同位置逐字一致。英文/数字/OTHER_CJK 同样参与，
        // `hold住`/`A股` 类混合词条因此可达。
        //
        // 历史实现误用 `!= CHAR_CHINESE` 作 early-return 条件（移植期引入，
        // v1.0.0 即存在），导致 9 个内置词条（`A股`/`B股`/`AB股`/`H股`/`K线`/
        // `U盘`/`QQ宠物`/`QQ飞车`/`Hold住`）与**全部**用户混合词条端到端失效，
        // 且词典层测试是通的（测试盲区）。修复见
        // 缺陷分析记录、
        // 设计决策 2。
        if context.current_char_type() == character_util::CHAR_USELESS {
            self.tmp_hits.clear();
            context.unlock_buffer(Self::LOCK);
            return;
        }

        // --- 准备阶段 ---
        let dict = context.dictionary();
        let cursor = context.cursor();
        let segment_buff = context.segment_buff();
        let buffer_offset = context.buffer_offset();

        // --- 计算阶段 ---
        // 复用 self 上的缓冲，避免每字符堆分配
        self.lexemes_buf.clear();
        self.next_hits_buf.clear();

        // 1. 处理已有的 tmp_hits（遍历而非 take，避免重新分配）
        for i in 0..self.tmp_hits.len() {
            let p_hit = self.tmp_hits[i];
            let current_hit = dict.match_with_hit(segment_buff, cursor, p_hit);

            if current_hit.is_match() {
                self.lexemes_buf.push(Lexeme::new(
                    buffer_offset as i32,
                    current_hit.begin(),
                    cursor as i32 - current_hit.begin() + 1,
                    Lexeme::TYPE_CNWORD,
                ));
            }

            if !current_hit.is_unmatch() && current_hit.is_prefix() {
                if let Some(node_idx) = current_hit.get_matched_node() {
                    self.next_hits_buf
                        .push(PersistentHit::new(current_hit.begin(), node_idx));
                }
            }
        }
        self.tmp_hits.clear();

        // 2. 处理当前字符
        let single_char_hit = dict.match_in_main_dict(segment_buff, cursor, 1);
        if single_char_hit.is_match() {
            self.lexemes_buf.push(Lexeme::new(
                buffer_offset as i32,
                cursor as i32,
                1,
                Lexeme::TYPE_CNWORD,
            ));
        }

        if single_char_hit.is_prefix() {
            if let Some(node_idx) = single_char_hit.get_matched_node() {
                self.next_hits_buf
                    .push(PersistentHit::new(single_char_hit.begin(), node_idx));
            }
        }
        // --- 计算阶段结束 ---

        // --- 修改阶段 ---
        // swap 而非 move，保留两个 Vec 的容量供下次复用
        std::mem::swap(&mut self.tmp_hits, &mut self.next_hits_buf);

        // 将词元添加到 context（drain 避免泄漏容量）
        for lexeme in self.lexemes_buf.drain(..) {
            context.add_lexeme(lexeme);
        }

        // --- 清理和锁定逻辑 ---
        if context.is_buffer_consumed() {
            self.tmp_hits.clear();
        }

        if self.tmp_hits.is_empty() {
            context.unlock_buffer(Self::LOCK);
        } else {
            context.lock_buffer(Self::LOCK);
        }
    }

    fn reset(&mut self) {
        self.tmp_hits.clear();
    }
}
