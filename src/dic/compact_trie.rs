//! # 对齐说明
//!
//! 对应 analysis-ik `dic/.../DictSegment.java` 的**读侧**（`match` /
//! `continueMatch`）——Rust 以 `Arc<[u8]>` + `u32` 索引二分查找实现，零 unsafe。
//! analysis-ik 相对 u6 的 `DictSegment` 5 处并发/性能修正（`ConcurrentHashMap`、
//! 无条件 `synchronized` 等，调研报告 §2.3）在本库天然规避：DAWG 构建后
//! **不可变**，无并发写路径。

//! 运行期构建的 DAWG 的零拷贝读回（方案 C'）。
//!
//! 二进制布局（little-endian，见 dawg_builder.rs 的 serialize）：
//!   [magic: b"IKDG"][version: u32][node_count: u32][edge_count: u32][root_idx: u32][word_count: u32]
//!   [nodes: node_count × (is_word: u8, child_count: u32, child_start: u32)]
//!   [edges: edge_count × (char: u32, target: u32)]   // 每个节点的 children 按 char 升序
//!
//! 字节由 `DawgBuilder` 在运行期从文本词表构建，本模块直接在这些字节上做二分查找匹配。
//! 格式不再跨构建存续（无冻结工件），版本号仅作内存一致性断言，维持 v2 不变。

use crate::dic::hit::Hit;
use std::sync::Arc;

pub(crate) struct CompactTrie {
    data: Arc<[u8]>,
    node_base: usize,
    edge_base: usize,
    root: u32,
    word_count: usize,
}

impl CompactTrie {
    /// 从 DAWG 字节零拷贝构建读回结构。
    ///
    /// `data` 由 `DawgBuilder` 在运行期产出，正常路径下字节布局合法。
    /// 作为内部 `pub(crate)` API，不再对畸形/截断字节做防御性 `assert`——
    /// 该结构不对外暴露，外部无从传入恶意字节（评审 P0-4）。
    pub(crate) fn from_bytes(data: Arc<[u8]>) -> Self {
        assert_eq!(&data[0..4], b"IKDG", "unexpected dictionary magic");
        let version = u32::from_le_bytes(data[4..8].try_into().unwrap());
        assert_eq!(version, 2, "unsupported dictionary version (expected 2)");
        let node_count = u32::from_le_bytes(data[8..12].try_into().unwrap()) as usize;
        let root = u32::from_le_bytes(data[16..20].try_into().unwrap());
        let word_count = u32::from_le_bytes(data[20..24].try_into().unwrap()) as usize;
        let node_base = 24;
        let edge_base = node_base + node_count * 9;
        Self {
            data,
            node_base,
            edge_base,
            root,
            word_count,
        }
    }

    #[inline]
    fn node_is_word(&self, idx: u32) -> bool {
        self.data[self.node_base + idx as usize * 9] != 0
    }

    #[inline]
    fn node_child_count(&self, idx: u32) -> u32 {
        let o = self.node_base + idx as usize * 9 + 1;
        u32::from_le_bytes([
            self.data[o],
            self.data[o + 1],
            self.data[o + 2],
            self.data[o + 3],
        ])
    }

    #[inline]
    fn node_child_start(&self, idx: u32) -> u32 {
        let o = self.node_base + idx as usize * 9 + 5;
        u32::from_le_bytes([
            self.data[o],
            self.data[o + 1],
            self.data[o + 2],
            self.data[o + 3],
        ])
    }

    #[inline]
    fn edge_char_u32(&self, e: u32) -> u32 {
        let o = self.edge_base + e as usize * 8;
        u32::from_le_bytes([
            self.data[o],
            self.data[o + 1],
            self.data[o + 2],
            self.data[o + 3],
        ])
    }

    #[inline]
    fn edge_target(&self, e: u32) -> u32 {
        let o = self.edge_base + e as usize * 8 + 4;
        u32::from_le_bytes([
            self.data[o],
            self.data[o + 1],
            self.data[o + 2],
            self.data[o + 3],
        ])
    }

    /// 在 parent 的子节点中二分查找指定 char（子节点按 char 升序）。
    /// 比较按 `u32` 进行，避免每步 `char::from_u32` 的边界检查（评审 P1-13）。
    #[inline]
    fn find_child(&self, parent: u32, ch: char) -> Option<u32> {
        let count = self.node_child_count(parent);
        if count == 0 {
            return None;
        }
        let start = self.node_child_start(parent);
        let key = ch as u32;
        let mut lo: u32 = 0;
        let mut hi: u32 = count;
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            let e = start + mid;
            let c = self.edge_char_u32(e);
            if c == key {
                return Some(self.edge_target(e));
            } else if c < key {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        None
    }

    /// 从根节点开始匹配文本（**整段成词**语义）。
    ///
    /// # 语义（勿轻易改动）
    ///
    /// `match` 只在**整段文本全部推进成功**且末节点成词时置位；中途失配立即返回
    /// 一个**未置位**的 `Hit`（`match=false`、`prefix=false`、`end=0`）。
    /// `prefix` 同理只看**末节点**是否有子节点。
    ///
    /// 这一看似"不够精确"的整段语义是**刻意**的，也是与 IK 原版 2012_u6 对齐后的
    /// 既定行为（已由 9133 行语料 × 4 口径对 u6 三方差分、**0 分歧**验证，
    /// 见缺陷分析记录）。它同时是
    /// [`crate::Dictionary::is_stop_word`] 正确工作的前提 —— 后者的调用点传入的是
    /// **词元长度**（`length > 1`，见 `analyze_context::get_next_lexeme`）：
    ///
    /// ```text
    /// 停用词表 {"中"}，主词典 {"中国"}
    /// 查询 "中国"（作为词元做停用词判定）：
    ///   '中' 推进成功 → '国' 无子节点 → 中途失配 → 整段返回未置位 → 不是停用词 ✅
    /// 若改成"路径上任一位置成词即 match"（IK 另一种 `matchText` 写法会有的行为）：
    ///   '中' 成词 → match=true → "中国" 被当停用词**误删** ❌
    /// ```
    ///
    /// 即：改成逐位置语义会让任何"前缀命中停用词"的长词元被静默丢弃。
    /// 分词器自身的词典推进用的是 `length == 1`（`match_in_main_dict(buff, cursor, 1)`
    /// 与 `continue_match`），两条路径在 `length == 1` 下行为完全一致。
    pub(crate) fn match_text(&self, text_slice: &[char], begin_offset: i32) -> Hit {
        let mut hit = Hit::new();
        hit.set_begin(begin_offset);

        let mut current_idx: u32 = self.root;
        for &key_char in text_slice.iter() {
            match self.find_child(current_idx, key_char) {
                Some(next_idx) => current_idx = next_idx,
                // 中途失配：返回未置位的 hit（整段语义，见上方文档）
                None => return hit,
            }
        }

        let final_node = current_idx;
        hit.set_end(begin_offset + text_slice.len() as i32);
        if self.node_is_word(final_node) {
            hit.set_match();
        }
        if self.node_child_count(final_node) > 0 {
            hit.set_prefix();
            hit.set_matched_node(final_node);
        }
        hit
    }

    /// 从指定节点继续匹配单个字符（语义同 `Trie::continue_match`）。
    pub(crate) fn continue_match(
        &self,
        node_idx: u32,
        key_char: char,
        current_index: i32,
        hit: &mut Hit,
    ) {
        hit.set_end(current_index);

        match self.find_child(node_idx, key_char) {
            Some(next_idx) => {
                if self.node_is_word(next_idx) {
                    hit.set_match();
                }
                if self.node_child_count(next_idx) > 0 {
                    hit.set_prefix();
                    hit.set_matched_node(next_idx);
                }
            }
            None => {
                hit.set_unmatch();
            }
        }
    }

    /// 返回 DAWG 中的真实词数（构建期写入头部的去重词数）。
    /// DAWG 后缀共享使多个词共享同一终止节点，因此不能用「终止节点数」近似。
    pub(crate) fn count_words(&self) -> usize {
        self.word_count
    }
}
