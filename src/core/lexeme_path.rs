//! 词元路径（`LexemePath`）与交叉路径裁决比较。
//!
//! # 对齐基准
//!
//! 对应 analysis-ik `core/src/main/java/org/wltea/analyzer/core/LexemePath.java`
//! （`master@6d2d70f`）。`compareTo` 六条裁决规则两侧逐字相同
//! （payloadLength ↓ → size ↑ → pathLength ↓ → pathEnd ↓ → XWeight ↓ →
//! PWeight ↓）。`getXWeight()` 的差异（B:158-176 vs A:158-166）：
//! **采纳饱和语义**（`long` + 溢出即 `Integer.MAX_VALUE`，u6 `int` 连乘可
//! 静默溢出——纯稳健性修复，u6 语料不可达）；**不采纳「单词长度截断 10」**
//! （会改变裁决排序，属行为变更而非缺陷修复，且 XWeight 仅是第 5 条
//! tie-breaker，需前四条全平才生效）。

use crate::core::lexeme::Lexeme;
#[cfg(any(test, feature = "judge-fast"))]
use crate::core::lexeme::LexemeRef;
use crate::core::quick_sort_set::QuickSortSet;
use std::cmp::Ordering;
use std::fmt;

/// Lexeme 链（路径）
#[derive(Clone)]
pub(crate) struct LexemePath {
    // 使用组合代替继承
    lexemes: QuickSortSet,

    // 起始位置
    path_begin: i32,
    // 结束位置
    path_end: i32,
    // 词元链的有效字符长度
    payload_length: i32,
}

impl LexemePath {
    pub fn new() -> Self {
        Self {
            lexemes: QuickSortSet::new(),
            path_begin: -1,
            path_end: -1,
            payload_length: 0,
        }
    }

    // --- 核心方法 (严格按照 Java 源码实现) ---
    pub fn poll_first(&mut self) -> Option<Lexeme> {
        self.lexemes.poll_first().map(Lexeme::from_ref)
    }

    /// 向 LexemePath 追加相交的 Lexeme
    ///
    /// **坐标纪律**：本结构全部字段（`path_begin`/`path_end`/`payload_length`）与
    /// `check_cross` 的比较均在**页内坐标**（`begin` / `begin + length`）上进行，
    /// 与 u6 `LexemePath.addCrossLexeme`（`getBegin() + getLength()`）逐字对应。
    /// **不得**改用 `Lexeme::end_position()`（= `offset + begin + length`，全局
    /// 坐标）——第 1 页 `offset == 0` 时两者恰好相等，Bug 会完全隐匿；第 2 页起
    /// 页内 begin 与全局 end 混比将导致路径/裁决全面错乱（2026-10-12 长文差分
    /// 发现：>4096 字符输入的跨页词元被成片丢弃，见设计决策）。
    pub fn add_cross_lexeme(&mut self, lexeme: Lexeme) -> bool {
        let r = lexeme.to_ref();
        let lexeme_end = lexeme.begin() + lexeme.length();
        if self.is_empty() {
            self.lexemes.add_lexeme(r);
            self.path_begin = lexeme.begin();
            self.path_end = lexeme_end;
            self.payload_length += lexeme.length(); // 初始时累加
            true
        } else if self.check_cross(&lexeme) {
            self.lexemes.add_lexeme(r);
            if lexeme_end > self.path_end {
                self.path_end = lexeme_end;
            }
            // 【关键修正】: payload_length 被重置为路径跨度
            self.payload_length = self.path_end - self.path_begin;
            true
        } else {
            false
        }
    }

    /// 移除尾部的 Lexeme
    #[cfg(any(test, feature = "judge-verify", not(feature = "judge-fast")))]
    pub fn remove_tail(&mut self) -> Option<Lexeme> {
        let tail = self.lexemes.poll_last();
        if let Some(ref tail_lexeme) = tail {
            if self.is_empty() {
                self.path_begin = -1;
                self.path_end = -1;
                self.payload_length = 0;
            } else {
                self.payload_length -= tail_lexeme.length();
                if let Some(new_tail) = self.lexemes.peek_last() {
                    // 页内坐标（见 `add_cross_lexeme` 的坐标纪律）：不用 end_position()
                    self.path_end = new_tail.begin() + new_tail.length();
                }
            }
        }
        tail.map(Lexeme::from_ref)
    }

    // --- 辅助方法 (保持不变) ---
    pub fn check_cross(&self, lexeme: &Lexeme) -> bool {
        // 页内坐标（见 `add_cross_lexeme` 的坐标纪律）：end 一律用 begin + length，
        // 不用全局的 end_position()
        let lexeme_end = lexeme.begin() + lexeme.length();
        (lexeme.begin() >= self.path_begin && lexeme.begin() < self.path_end)
            || (self.path_begin >= lexeme.begin() && self.path_begin < lexeme_end)
    }
    pub fn is_empty(&self) -> bool {
        self.lexemes.is_empty()
    }

    /// 从另一个路径拷贝内容，**复用自身容量**（容量足够时不重新 malloc），纯 memcpy。
    /// 与 `other.clone()` 计算成本相同，但省去每次新建 `Vec<LexemeRef>` 的堆分配。
    /// 零分配优化（永久生效）：`judge` 的 `best` 原地重建用。
    #[cfg(any(test, feature = "judge-verify", not(feature = "judge-fast")))]
    pub(crate) fn clone_from_path(&mut self, other: &LexemePath) {
        self.lexemes.clone_from(&other.lexemes);
        self.path_begin = other.path_begin;
        self.path_end = other.path_end;
        self.payload_length = other.payload_length;
    }
    pub fn path_begin(&self) -> i32 {
        self.path_begin
    }
    pub fn path_end(&self) -> i32 {
        self.path_end
    }
    pub fn payload_length(&self) -> i32 {
        self.payload_length
    }
    pub fn path_length(&self) -> i32 {
        self.path_end - self.path_begin
    }
    pub fn size(&self) -> usize {
        self.lexemes.size()
    }
    pub fn lexemes(&self) -> &QuickSortSet {
        &self.lexemes
    }

    // --- 权重计算 ---

    pub fn x_weight(&self) -> i32 {
        self.lexemes
            .iter()
            .map(|l| l.length())
            .try_fold(1i32, |acc, len| acc.checked_mul(len))
            .unwrap_or(i32::MAX)
    }

    pub fn p_weight(&self) -> i32 {
        // 用 i64 累加后饱和到 i32::MAX，与 x_weight 的处理一致，
        // 避免大 n 下 i32 求和溢出与 judge_fast 的 i64 累加产生分歧（评审 P0-7）。
        let sum: i64 = self
            .lexemes
            .iter()
            .enumerate()
            .map(|(i, l)| (i as i64 + 1) * l.length() as i64)
            .sum();
        sum.min(i32::MAX as i64) as i32
    }

    /// 向 LexemePath 追加不相交的 Lexeme
    /// 方案H：`forward_path` 按 `lexemes` 切片顺序（已按 begin 排序）追加不相交词元，
    /// 故直接 `push_lexeme`（O(1)），省去 `add_lexeme` 的二分查找 + 旋转搬移。
    pub fn add_not_cross_lexeme(&mut self, lexeme: Lexeme) -> bool {
        let r = lexeme.to_ref();
        // 页内坐标（见 `add_cross_lexeme` 的坐标纪律）：end 一律用 begin + length
        let lexeme_end = lexeme.begin() + lexeme.length();
        if self.is_empty() {
            self.lexemes.push_lexeme(r);
            self.path_begin = lexeme.begin();
            self.path_end = lexeme_end;
            self.payload_length += lexeme.length();
            true
        } else if self.check_cross(&lexeme) {
            false
        } else {
            self.lexemes.push_lexeme(r);
            self.payload_length += lexeme.length();
            if let Some(head) = self.lexemes.peek_first() {
                self.path_begin = head.begin();
            }
            if let Some(tail) = self.lexemes.peek_last() {
                self.path_end = tail.begin() + tail.length();
            }
            true
        }
    }

    /// 由一组"已按 begin 升序、互不交叉"的词元索引，一次性重建路径（终局重建用）。
    /// 调用方保证 `indices` 形如某条合法链（前缀 ⊕ 后缀），故 `add_not_cross_lexeme`
    /// 会按序全部接受，得到的 path_begin/path_end/payload 与参考实现逐位一致。
    #[cfg(any(test, feature = "judge-fast"))]
    pub(crate) fn from_chain_indices(lexemes: &[LexemeRef], indices: &[usize]) -> LexemePath {
        let mut path = LexemePath::new();
        for &idx in indices {
            path.add_not_cross_lexeme(Lexeme::from_ref(lexemes[idx]));
        }
        path
    }
}

impl PartialEq for LexemePath {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}
impl Eq for LexemePath {}
impl PartialOrd for LexemePath {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for LexemePath {
    fn cmp(&self, other: &Self) -> Ordering {
        // 规则 1：比较有效文本长度 (越长越好)
        other
            .payload_length()
            .cmp(&self.payload_length())
            // 规则 2：比较词元个数 (越少越好)
            .then_with(|| self.size().cmp(&other.size()))
            // 规则 3：比较路径跨度 (越大越好)
            .then_with(|| other.path_length().cmp(&self.path_length()))
            // 规则 4：比较结束位置 (越靠后越好)
            .then_with(|| other.path_end().cmp(&self.path_end()))
            // 规则 5：比较词长积 (越大越好)
            .then_with(|| other.x_weight().cmp(&self.x_weight()))
            // 规则 6：比较词元位置权重 (越大越好)
            .then_with(|| other.p_weight().cmp(&self.p_weight()))
    }
}

impl fmt::Debug for LexemePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // 分词过程中 `Lexeme::from_ref(*l).text()` 恒为空串，差分调试输出全是 `[, , ]`，
        // 毫无信息量。改打印 `(begin, length)`，使 judge 差分断言可读（评审 P2-5）。
        let sigs: Vec<String> = self
            .lexemes
            .iter()
            .map(|l| format!("({}, {})", l.begin(), l.length()))
            .collect();
        write!(f, "[{}]", sigs.join(", "))
    }
}
