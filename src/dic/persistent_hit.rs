//! # 对齐说明
//!
//! 无 Java 对应物：Java `Hit` 自身可变且持 `DictSegment` 引用，无需持久化
//! 中间态；本库把「待续接的 Trie 节点」扁平化为 `u32` 索引快照
//! （`PersistentHit`），供子分词器跨字符续接匹配（`CJKSegmenter.tmp_hits`）。

///src/dic/persistent_hit.rs
///
/// 持久化命中：记录待后续字符继续匹配的 Trie 节点。
/// 扁平化 Trie 后存 u32 索引，无需 unsafe impl Send/Sync。

#[derive(Clone, Copy)]
pub struct PersistentHit {
    begin: i32,
    node_idx: u32,
}

impl PersistentHit {
    pub(crate) fn new(begin: i32, node_idx: u32) -> Self {
        Self { begin, node_idx }
    }

    pub fn begin(&self) -> i32 {
        self.begin
    }
    pub fn node_idx(&self) -> u32 {
        self.node_idx
    }
}
