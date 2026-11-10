//! 词典匹配命中结果（`Hit`）。
//!
//! # 对齐基准
//!
//! 对应 analysis-ik `dic/src/main/java/org/wltea/analyzer/dic/Hit.java`
//! （`master@6d2d70f`）——与 u6 同名文件仅 CRLF→LF 行尾差异，剥离 `\r` 后
//! `diff` 为空，语义零差异（调研报告 §2.1/§2.2 勘误）。Rust 侧以 bitflags
//! `u8` + `u32` 哨兵实现。

///src/dic/hit.rs
use bitflags::bitflags;

bitflags! {
    /// 表示词典匹配命中的状态。
    /// 方案D：底层改用 u8（原 u32），紧凑布局、消除多余填充。
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct HitState: u8 {
        /// 未匹配
        const UNMATCH = 0b00000000;
        /// 完全匹配
        const MATCH   = 0b00000001;
        /// 前缀匹配
        const PREFIX  = 0b00000010;
    }
}

/// 表示一次词典匹配的命中结果。
/// 扁平化 Trie 后，matched_node 存 u32 索引而非引用，无需生命周期。
/// 方案D：matched_node 用 `u32::MAX` 哨兵表示"无"，替代 `Option<u32>`（消除 niche 填充 + 判空分支）。
#[derive(Clone)]
pub struct Hit {
    hit_state: HitState,
    matched_node: u32,
    begin: i32,
    end: i32,
}

impl Default for Hit {
    fn default() -> Self {
        Self {
            hit_state: HitState::default(),
            matched_node: u32::MAX, // 哨兵：表示"无匹配节点"
            begin: 0,
            end: 0,
        }
    }
}

impl Hit {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_match(&self) -> bool {
        self.hit_state.contains(HitState::MATCH)
    }

    pub fn set_match(&mut self) {
        self.hit_state |= HitState::MATCH;
    }

    pub fn is_prefix(&self) -> bool {
        self.hit_state.contains(HitState::PREFIX)
    }

    pub fn set_prefix(&mut self) {
        self.hit_state |= HitState::PREFIX;
    }

    pub fn is_unmatch(&self) -> bool {
        self.hit_state == HitState::UNMATCH
    }

    pub fn set_unmatch(&mut self) {
        self.hit_state = HitState::UNMATCH;
    }

    /// 返回匹配到的 Trie 节点索引；未命中时返回 None（内部用 `u32::MAX` 哨兵表示）。
    pub fn get_matched_node(&self) -> Option<u32> {
        if self.matched_node == u32::MAX {
            None
        } else {
            Some(self.matched_node)
        }
    }
    pub fn set_matched_node(&mut self, node_idx: u32) {
        self.matched_node = node_idx;
    }

    pub fn begin(&self) -> i32 {
        self.begin
    }
    pub fn set_begin(&mut self, begin: i32) {
        self.begin = begin;
    }
    pub fn end(&self) -> i32 {
        self.end
    }
    pub fn set_end(&mut self, end: i32) {
        self.end = end;
    }
}
