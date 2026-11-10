//! 分词器专用的 Lexeme 有序集合（`QuickSortSet`）。
//!
//! # 对齐基准
//!
//! 对应 analysis-ik `core/src/main/java/org/wltea/analyzer/core/QuickSortSet.java`
//! （`master@6d2d70f`）——该文件与 u6 同名文件**逐字节相同**（含 CRLF），
//! 无 A/B 差异（调研报告 §2.1）。Rust 侧实现差异：Java 双向链表 →
//! `Vec` 二分插入。

use crate::core::lexeme::LexemeRef;

/// IK分词器专用的Lexeme有序集合。
///
/// 原始 Java 版本基于双向链表；Rust 移植版最初用 `BTreeSet`。
/// 现改为 `Vec` + 二分插入，因为一条路径通常只有 2~5 个词元，
/// `Vec` 的 cache 局部性 + 线性扫描比 BTreeSet 的节点遍历更快。
/// 方案C：内部存储 `LexemeRef`（Copy），消除 `Option<String>` 字段拷贝。
#[derive(Debug, Clone)]
pub(crate) struct QuickSortSet {
    lexemes: Vec<LexemeRef>,
}

impl QuickSortSet {
    /// 单条路径词元数量通常仅 2~5 个，预分配容量避免频繁堆扩容。
    const INITIAL_CAPACITY: usize = 4;

    pub fn new() -> Self {
        Self {
            lexemes: Vec::with_capacity(Self::INITIAL_CAPACITY),
        }
    }

    /// 向集合添加词元（保持有序、去重）。
    /// 返回是否为新插入。
    pub fn add_lexeme(&mut self, lexeme: LexemeRef) -> bool {
        match self.lexemes.binary_search(&lexeme) {
            Ok(_) => false,
            Err(idx) => {
                // 原地搬移：在 idx 处腾出空间后写入，避免 Vec::insert 的额外分支
                self.lexemes.push(lexeme);
                self.lexemes[idx..].rotate_right(1);
                true
            }
        }
    }

    /// 追加词元（调用方保证按 `begin` 升序追加，绝大多数情况下为末尾追加）。
    ///
    /// 用于 `forward_path`：传入的 `lexemes` 切片已按 `begin` 排序，每次只追加
    /// 「不相交」词元，故新词元恒为当前最大（push 到末尾即保持有序），无需
    /// `rotate_right` 的 O(n) 元素搬移。仍用 `binary_search` 去重（O(log n)），
    /// 与 `add_lexeme` 的语义完全一致；仅在极个别非末尾插入时回退到有序插入。
    pub fn push_lexeme(&mut self, lexeme: LexemeRef) {
        match self.lexemes.binary_search(&lexeme) {
            Ok(_) => {} // 重复，跳过（与 add_lexeme 一致）
            Err(idx) if idx == self.lexemes.len() => self.lexemes.push(lexeme),
            Err(idx) => {
                // 罕见：非末尾插入，保持有序
                self.lexemes.push(lexeme);
                self.lexemes[idx..].rotate_right(1);
            }
        }
    }

    pub fn peek_first(&self) -> Option<&LexemeRef> {
        self.lexemes.first()
    }

    pub fn poll_first(&mut self) -> Option<LexemeRef> {
        if self.lexemes.is_empty() {
            None
        } else {
            Some(self.lexemes.remove(0))
        }
    }

    pub fn peek_last(&self) -> Option<&LexemeRef> {
        self.lexemes.last()
    }

    #[cfg(any(test, feature = "judge-verify", not(feature = "judge-fast")))]
    pub fn poll_last(&mut self) -> Option<LexemeRef> {
        self.lexemes.pop()
    }

    /// 一次性取走所有词元（O(1)），原集合留下空 Vec 并复用其容量。
    /// 替代逐个 `poll_first`/`remove(0)` 的 O(n²) 搬移，用于 `org_lexemes` 整体消费场景。
    pub fn drain_all(&mut self) -> Vec<LexemeRef> {
        std::mem::take(&mut self.lexemes)
    }

    pub fn size(&self) -> usize {
        self.lexemes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lexemes.is_empty()
    }

    /// 从另一个集合拷贝内容，**复用自身容量**（容量足够时不重新 malloc）。
    /// 等价于 `self = other.clone()` 但省去每次新建 `Vec` 的堆分配。
    /// 零分配优化（永久生效）：`LexemePath::clone_from_path` 复用。
    #[cfg(any(test, feature = "judge-verify", not(feature = "judge-fast")))]
    pub fn clone_from(&mut self, other: &QuickSortSet) {
        self.lexemes.clone_from(&other.lexemes);
    }

    pub fn iter(&self) -> std::slice::Iter<'_, LexemeRef> {
        self.lexemes.iter()
    }

    /// 返回底层词元切片的只读视图，避免为遍历/引用而整体 clone。
    pub fn as_slice(&self) -> &[LexemeRef] {
        &self.lexemes
    }
}

// --- 单元测试 ---
#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::lexeme::LexemeRef;

    #[test]
    fn test_add_and_order() {
        let mut set = QuickSortSet::new();
        let l1 = LexemeRef::new(0, 5, 3, 0); // begin: 5, len: 3
        let l2 = LexemeRef::new(0, 5, 4, 0); // begin: 5, len: 4 (更长，所以更小)
        let l3 = LexemeRef::new(0, 6, 3, 0); // begin: 6 (更大)

        assert!(set.add_lexeme(l1));
        assert!(set.add_lexeme(l3));
        assert!(set.add_lexeme(l2));

        // 测试重复添加
        assert!(!set.add_lexeme(l2));

        assert_eq!(set.size(), 3);

        // 验证顺序
        assert_eq!(set.peek_first(), Some(&l2));
        assert_eq!(set.peek_last(), Some(&l3));
    }

    #[test]
    fn test_poll() {
        let mut set = QuickSortSet::new();
        let l1 = LexemeRef::new(0, 5, 3, 0);
        let l2 = LexemeRef::new(0, 5, 4, 0);
        let l3 = LexemeRef::new(0, 6, 3, 0);

        set.add_lexeme(l1);
        set.add_lexeme(l2);
        set.add_lexeme(l3);

        assert_eq!(set.poll_first(), Some(l2));
        assert_eq!(set.size(), 2);

        assert_eq!(set.poll_last(), Some(l3));
        assert_eq!(set.size(), 1);
    }

    #[test]
    fn test_iter() {
        let mut set = QuickSortSet::new();
        let l1 = LexemeRef::new(0, 5, 3, 0);
        let l2 = LexemeRef::new(0, 5, 4, 0);
        let l3 = LexemeRef::new(0, 6, 3, 0);

        set.add_lexeme(l1);
        set.add_lexeme(l2);
        set.add_lexeme(l3);

        let mut iter = set.iter();
        assert_eq!(iter.next(), Some(&l2));
        assert_eq!(iter.next(), Some(&l1));
        assert_eq!(iter.next(), Some(&l3));
        assert_eq!(iter.next(), None);
    }
}
