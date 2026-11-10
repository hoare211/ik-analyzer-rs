//! 分词上下文（双缓冲 + 路径表 + 结果队列）。
//!
//! # 对齐基准
//!
//! 对应 analysis-ik `core/src/main/java/org/wltea/analyzer/core/AnalyzeContext.java`
//! （`master@6d2d70f`）。与 u6（`AnalyzeContext.java`，2012-05-17）的差异：
//! ① **双缓冲边界 `cursor+1`**（B:131,135,232 vs A:126,133,223）——u6 每页
//! 边界重复输出 1 个重叠 token（A 级实测，调研报告 §9.1），此为**缺陷修复**；
//! ② issue#921 三字段（`lastUselessCharNum`/`maxConsumedEndPosition`/
//! `lastSkippedCount`）服务 Lucene `finalOffset`/`positionIncrement`，本库
//! 无 Lucene 契约，**不吸收**（决策 9）；③ Rust `char` 模型：`segment_buff`
//! 为 `Vec<char>`，无 UTF-16 代理对中间态（offset 口径为码点，决策 8）。

use crate::cfg::Config;
use crate::core::char_source::CharSource;
use crate::core::character_util;
use crate::core::lexeme::Lexeme;
use crate::core::lexeme_path::LexemePath;
use crate::core::quick_sort_set::QuickSortSet;
use crate::dic::Dictionary;
use std::collections::VecDeque;
use std::sync::Arc;

use bitflags::bitflags;

/// 缓冲区最大容量。短输入按 min(len, BUFF_SIZE) 分配。
pub(crate) const BUFF_SIZE: usize = 4096;
const BUFF_EXHAUST_CRITICAL: usize = 100;

bitflags! {
    /// 子分词器缓冲区锁，每位对应一个子分词器。
    /// 替代原来的 `HashSet<String>`，零堆分配。
    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
    pub(crate) struct BufferLock: u8 {
        const CJK      = 0b0000_0001;
        const QUAN     = 0b0000_0010;
        const LETTER   = 0b0000_0100;
    }
}

/// 分词器上下文状态
pub(crate) struct AnalyzeContext {
    // 字符窜读取缓冲
    segment_buff: Vec<char>,
    // 字符类型数组
    char_types: Vec<i32>,

    // 记录Reader内已分析的字串总长度
    buff_offset: usize,
    // 当前缓冲区位置指针
    cursor: usize,
    // 最近一次读入的,可处理的字串长度
    pub(crate) available: usize,

    // 子分词器锁（位掩码，零分配）
    buff_locker: BufferLock,

    // 原始分词结果集合
    org_lexemes: QuickSortSet,
    // LexemePath 位置索引表。键空间 ∈ [0, available) ⊆ [0, BUFF_SIZE)，
    // 每页清空重建，故用 `Vec<Option<LexemePath>>` 替代 `HashMap`，消掉哈希开销
    // 并提升 cache 局部性（评审 P1-12）。
    path_map: Vec<Option<LexemePath>>,
    // 最终分词结果集
    results: VecDeque<Lexeme>,

    // 分词器配置项。改为 `Arc<Config>`，避免每条文本深拷贝 `Config`
    // （含 `DictPlan::Custom(Vec<DictSource>)`，评审 P1-11）。
    cfg: Arc<Config>,
    dictionary: Arc<Dictionary>,

    /// 【新增】标记整个输入流是否已处理完毕
    is_finished: bool,
}

impl AnalyzeContext {
    /// 创建上下文。`buffer_size` 为缓冲区大小（短输入按 min(len, BUFF_SIZE)）。
    pub fn new(cfg: Arc<Config>, dictionary: Arc<Dictionary>, buffer_size: usize) -> Self {
        let buf_size = buffer_size.clamp(1, BUFF_SIZE);
        Self {
            cfg,
            dictionary,
            segment_buff: vec!['\0'; buf_size],
            char_types: vec![0; buf_size],
            buff_locker: BufferLock::empty(),
            org_lexemes: QuickSortSet::new(),
            path_map: vec![None; buf_size],
            results: VecDeque::new(),
            buff_offset: 0,
            cursor: 0,
            available: 0,
            is_finished: false,
        }
    }

    // 提供一个访问 dictionary 的方法
    pub fn dictionary(&self) -> &Arc<Dictionary> {
        &self.dictionary
    }

    pub fn is_finished(&self) -> bool {
        self.is_finished
    }
    pub fn finish(&mut self) {
        self.is_finished = true;
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }
    pub fn segment_buff(&self) -> &[char] {
        &self.segment_buff
    }
    pub fn current_char(&self) -> char {
        self.segment_buff[self.cursor]
    }
    pub fn current_char_type(&self) -> i32 {
        self.char_types[self.cursor]
    }
    pub fn buffer_offset(&self) -> usize {
        self.buff_offset
    }
    pub fn org_lexemes(&self) -> &QuickSortSet {
        &self.org_lexemes
    }
    pub fn org_lexemes_mut(&mut self) -> &mut QuickSortSet {
        &mut self.org_lexemes
    }

    pub fn fill_buffer(&mut self, source: &mut dyn CharSource) -> bool {
        // 如果已标记结束，则不再填充
        if self.is_finished {
            return false;
        }

        // 1. 移位旧数据
        // 关键：保留 cursor+1..（不含 cursor），因为 cursor 处的字符已在当前页
        // 通过 output_to_result 输出。若保留 cursor，下一页 index 0 会重复处理它，
        // 导致跨页词的下半部分被重复输出。
        let offset = self.available.saturating_sub(self.cursor + 1);
        if offset > 0 {
            self.segment_buff.copy_within(self.cursor + 1.., 0);
        }
        let mut read_count = offset;

        // 2. 从 CharSource 填充剩余空间
        let buf_cap = self.segment_buff.len();
        if read_count < buf_cap {
            let filled = source.fill(&mut self.segment_buff[read_count..buf_cap]);
            read_count += filled;
        }

        // 3. 更新上下文状态
        self.available = read_count;
        self.cursor = 0;

        // 4. 如果 source 已 EOF 且没填满，标记结束
        //    显式加括号，避免依赖 `&&` 优先级高于 `||` 的可读性陷阱（评审 P3-2）。
        if read_count == 0 || (read_count < buf_cap && read_count == offset) {
            // source 返回 0 表示 EOF
            self.finish();
        }

        // 5. 返回是否成功填充了新字符
        read_count > offset
    }

    pub fn init_cursor(&mut self) {
        if self.available > 0 {
            self.cursor = 0;
            self.segment_buff[self.cursor] = character_util::regularize(
                self.segment_buff[self.cursor],
                self.cfg.enable_lowercase(),
            );
            self.char_types[self.cursor] = character_util::identify_char_type(
                self.segment_buff[self.cursor],
                self.cfg.astral_mode(),
            );
        }
    }

    pub fn move_cursor(&mut self) -> bool {
        if self.cursor < self.available.saturating_sub(1) {
            self.cursor += 1;
            self.segment_buff[self.cursor] = character_util::regularize(
                self.segment_buff[self.cursor],
                self.cfg.enable_lowercase(),
            );
            self.char_types[self.cursor] = character_util::identify_char_type(
                self.segment_buff[self.cursor],
                self.cfg.astral_mode(),
            );
            true
        } else {
            false
        }
    }

    pub fn is_buffer_consumed(&self) -> bool {
        self.cursor == self.available.saturating_sub(1)
    }

    pub fn lock_buffer(&mut self, lock: BufferLock) {
        self.buff_locker |= lock;
    }
    pub fn unlock_buffer(&mut self, lock: BufferLock) {
        self.buff_locker &= !lock;
    }
    pub fn is_buffer_locked(&self) -> bool {
        !self.buff_locker.is_empty()
    }

    /// 判断 segmentBuff 是否需要读取新数据
    /// 注意：只有 available == BUFF_SIZE 时才可能需要换页。
    /// 短输入（buffer < BUFF_SIZE）的 available 永远 < BUFF_SIZE，天然不触发换页。
    pub fn need_refill_buffer(&self) -> bool {
        self.available == BUFF_SIZE
            && self.cursor < self.available.saturating_sub(1)
            && self.cursor > self.available.saturating_sub(BUFF_EXHAUST_CRITICAL)
            && !self.is_buffer_locked()
    }

    /// 累计相对于 reader 起始位置的位移
    /// 新页从 cursor+1 开始（fill_buffer 不保留 cursor 字符），故累加 cursor+1
    pub fn mark_buffer_offset(&mut self) {
        self.buff_offset += self.cursor + 1;
    }
    /// 方案C：内部以 LexemeRef（Copy）存入 org_lexemes，消除 Option<String> 字段拷贝
    pub fn add_lexeme(&mut self, lexeme: Lexeme) {
        self.org_lexemes.add_lexeme(lexeme.to_ref());
    }
    pub fn add_lexeme_path(&mut self, path: LexemePath) {
        // `path_begin` 是页内坐标（`LexemePath::new()` 初值为 -1）。所有构造路径
        // 在非空时都会写入它，故此处恒 ∈ [0, available) ⊆ [0, path_map.len())。
        // 显式断言把这条隐式不变量固化 —— 否则一旦将来新增构造点或改动
        // `remove_tail`，`-1 as usize == usize::MAX` 会直接变成索引越界 panic
        // （评审 P1-2）。
        let begin = path.path_begin();
        debug_assert!(
            begin >= 0 && (begin as usize) < self.path_map.len(),
            "LexemePath::path_begin()={} 越界（path_map 长度 {}）",
            begin,
            self.path_map.len()
        );
        self.path_map[begin as usize] = Some(path);
    }

    pub fn output_to_result(&mut self) {
        let mut index = 0;
        // 闭区间输出到 cursor（含）。cursor 处字符只在当前页输出，
        // 因为 fill_buffer 换页时不再保留 cursor（从 cursor+1 开始搬移）。
        while index <= self.cursor {
            // 跳过无用字符
            if self.char_types[index] == character_util::CHAR_USELESS {
                index += 1;
                continue;
            }

            // 尝试从 path_map 中移除并获取以当前 index 开头的路径
            if let Some(mut path) = self.path_map[index].take() {
                // 如果找到路径，则迭代处理该路径中的所有词元
                // 这是关键的修正部分，我们不再一次性跳跃 index
                if let Some(mut l) = path.poll_first() {
                    loop {
                        // 先记录结束位置（push 后 l 的所有权即转移，避免后续 clone）
                        //
                        // 【坐标系·评审 P0-1】这里必须用**页内**坐标 `begin + length`，
                        // 不能用全局的 `end_position()`（= `offset + begin + length`）。
                        // `index` 是页内下标：本循环的条件是页内的 `self.cursor`，
                        // 它还要下标页内的 `path_map` / `char_types`；而 `l.begin()`
                        // 的定义也是"词元的相对起始位置"（页内）。
                        //
                        // 旧代码误用 `end_position()`：第 1 页 `buff_offset == 0` 时
                        // 全局 ≡ 页内，故完全隐匿；第 2 页起 `index` 一步跳到
                        // `offset + local_end`，远超 `self.cursor`，`while` 立即退出，
                        // 该页 `index` 之后的字符被**静默丢弃**（实测恒丢末尾 2 字）。
                        let end = (l.begin() + l.length()) as usize;
                        // 直接移动词元所有权，消除一次无谓 clone
                        self.results.push_back(l);
                        // 将 index 更新到当前词元的结束位置（页内）
                        index = end;

                        // 检查路径中是否还有下一个词元
                        if let Some(next_l) = path.poll_first() {
                            // 如果有，则处理两个词元之间的间隙（单字输出）。
                            // `index` 与 `next_l.begin()` 同为页内坐标（评审 P0-1）。
                            for i in index..(next_l.begin() as usize) {
                                self.output_single_cjk(i);
                            }
                            l = next_l;
                        } else {
                            // 如果没有下一个词元了，就退出内部循环
                            break;
                        }
                    }
                } else {
                    // 如果路径为空（理论上不应发生，但作为保护），则当作单字处理
                    self.output_single_cjk(index);
                    index += 1;
                }
            } else {
                // 如果在 path_map 中找不到以 index 开头的路径，
                // 则将当前字符作为单字输出
                self.output_single_cjk(index);
                index += 1;
            }
        }
        // 清空 path_map 中可能剩余的所有路径（Vec 方案：整体置 None，保留容量）
        self.path_map.fill(None);
    }

    /// 对 CJK 字符进行单字输出
    fn output_single_cjk(&mut self, index: usize) {
        let char_type = self.char_types[index];
        if char_type == character_util::CHAR_CHINESE {
            let lexeme = Lexeme::new(
                self.buff_offset as i32,
                index as i32,
                1,
                Lexeme::TYPE_CNCHAR,
            );
            self.results.push_back(lexeme);
        } else if char_type == character_util::CHAR_OTHER_CJK {
            let lexeme = Lexeme::new(
                self.buff_offset as i32,
                index as i32,
                1,
                Lexeme::TYPE_OTHER_CJK,
            );
            self.results.push_back(lexeme);
        } else if char_type == character_util::CHAR_ASTRAL {
            // 补充平面字符（emoji 等，仅 `AstralMode::Tokens` 下产生）单字输出。
            // 类型取 CNCHAR，与 analysis-ik `SurrogatePairSegmenter` 一致；
            // 长度按**码点**计 1（offset 口径为码点而非 Java 的 UTF-16 码元，
            // 见设计决策 4/8）。
            let lexeme = Lexeme::new(
                self.buff_offset as i32,
                index as i32,
                1,
                Lexeme::TYPE_CNCHAR,
            );
            self.results.push_back(lexeme);
        }
    }

    /// 弹出下一个**已定稿**的词元：已完成数量词合并、且已滤除停用词，
    /// 但**尚未填充文本**（`text == None`）。两条输出路径（`Lexeme` / `Span`）共用此逻辑。
    fn pop_finalized_lexeme(&mut self) -> Option<Lexeme> {
        while let Some(mut result) = self.results.pop_front() {
            self.compound(&mut result);
            let is_stop_word = self.dictionary.is_stop_word(
                &self.segment_buff,
                result.begin() as usize,
                result.length() as usize,
            );

            if !is_stop_word {
                return Some(result);
            }
        }
        None
    }

    /// 获取下一个词元，同时处理合并和停用词。
    ///
    /// `with_text = true` 时惰性生成文本（**每个输出词元一次 `String` 分配**，
    /// 实测占分词热路径全部堆分配的 94%，见 `examples/alloc_count.rs`）；
    /// `with_text = false` 时不触碰文本，供零拷贝的 `Span` 路径使用。
    pub fn get_next_lexeme(&mut self, with_text: bool) -> Option<Lexeme> {
        let mut result = self.pop_finalized_lexeme()?;
        if with_text {
            let text = String::from_iter(
                &self.segment_buff
                    [result.begin() as usize..result.begin() as usize + result.length() as usize],
            );
            result.set_text(text);
        }
        Some(result)
    }

    /// 组合词元（数量词合并）
    fn compound(&mut self, result: &mut Lexeme) {
        if !self.cfg.use_smart || self.results.is_empty() {
            return;
        }

        // 第一轮合并：阿拉伯数字 + 中文数词/中文量词
        // 注意：此处必须与 Java 原版一致地使用「两个独立的 if」而非 if/else if。
        // 因为第一轮把 ARABIC 合并成 CN_NUM 后，result 的类型会变成 CN_NUM，
        // 从而立即满足第二轮的条件，形成 “42 + 三 + 首 => 42三首(CN_QUAN)” 的连续合并。
        if result.lexeme_type() == Lexeme::TYPE_ARABIC {
            if let Some(next_lexeme) = self.results.front() {
                let mut append_ok = false;
                if next_lexeme.lexeme_type() == Lexeme::TYPE_CNUM {
                    // 合并英文数词 + 中文数词
                    append_ok = result.append(next_lexeme, Lexeme::TYPE_CNUM);
                } else if next_lexeme.lexeme_type() == Lexeme::TYPE_COUNT {
                    // 合并英文数词 + 中文量词
                    append_ok = result.append(next_lexeme, Lexeme::TYPE_CQUAN);
                }
                if append_ok {
                    self.results.pop_front();
                }
            }
        }

        // 第二轮合并：中文数词 + 中文量词
        if result.lexeme_type() == Lexeme::TYPE_CNUM && !self.results.is_empty() {
            if let Some(next_lexeme) = self.results.front() {
                let mut append_ok = false;
                if next_lexeme.lexeme_type() == Lexeme::TYPE_COUNT {
                    append_ok = result.append(next_lexeme, Lexeme::TYPE_CQUAN);
                }
                if append_ok {
                    self.results.pop_front();
                }
            }
        }
    }
    pub fn results_is_empty(&self) -> bool {
        self.results.is_empty()
    }
}
