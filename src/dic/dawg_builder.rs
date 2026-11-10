//! # 对齐说明
//!
//! 对应 analysis-ik `dic/.../Dictionary.java` 的**写侧**（`DictSegment.fillSegment`
//! 构建 Trie）——本库以 DAWG（后缀共享去重）替代 Java 的逐节点 Trie，
//! 使 27.6 万词的主词典仅 7.4 万节点 / 3.09MB。与 u6/B 的差异是**表示层**：
//! 匹配语义（整段成词 / 前缀）经 9133 行语料对拍与两侧一致。

//! 运行期 DAWG 构建：把词表编译成与旧 build.rs 预编译产物同构的 DAWG 字节。
//!
//! 来源：`build.rs` 的 `Builder → DAG（后缀共享去重）→ serialize` 逻辑库内移植
//! （方案 C'，见 DAWG 构建方案）。与 build.rs 的差异仅一处：
//! `BNode.children` 用有序 `Vec<(char, u32)>` 替代 `HashMap<char, usize>`，
//! 控制构建期峰值内存（每节点 24B + 元素，而非每节点 ≥48B 的 HashMap）。
//!
//! 字节布局（little-endian，与 compact_trie.rs 的读取端严格配套）：
//!   [magic: b"IKDG"][version: u32][node_count: u32][edge_count: u32]
//!   [root_idx: u32][word_count: u32]   // word_count = 去重后的真实词数（≠ 终止节点数）
//!   [nodes: node_count × (is_word: u8, child_count: u32, child_start: u32)]
//!   [edges: edge_count × (char: u32, target: u32)]   // 每个节点的 children 按 char 升序

const MAGIC: &[u8; 4] = b"IKDG";
const VERSION: u32 = 2;

/// 后缀共享去重的记忆化键：`(is_word, 子表序列)`。
type MemoKey = (bool, Vec<(char, u32)>);
/// conv 的记忆化表（评审 P2-3：抽出类型别名降低复杂度）。
type ConvMemo = std::collections::HashMap<MemoKey, u32>;

// ---------------- 构建期 Trie（构建过程的中间结构，finish 后整体释放）----------------

struct BNode {
    is_word: bool,
    /// 有序子表：(char, builder 节点索引)。二分插入保持按 char 升序。
    children: Vec<(char, usize)>,
}

pub(crate) struct DawgBuilder {
    nodes: Vec<BNode>,
    /// 去重后的真实词数（同词重复 insert 只计一次；等价于旧 build.rs 的 seen HashSet）。
    word_count: u32,
}

impl DawgBuilder {
    pub(crate) fn new() -> Self {
        DawgBuilder {
            nodes: vec![BNode {
                is_word: false,
                children: Vec::new(),
            }],
            word_count: 0,
        }
    }

    /// 插入一个词。调用方已完成大小写策略处理（to_word_chars），本层不做任何归一化。
    pub(crate) fn insert(&mut self, word: &[char]) {
        if word.is_empty() {
            return;
        }
        let mut cur = 0usize;
        for &c in word {
            cur = self.get_or_create_child(cur, c);
        }
        let node = &mut self.nodes[cur];
        if !node.is_word {
            node.is_word = true;
            self.word_count += 1;
        }
    }

    /// 二分查找子节点，不存在则有序插入新节点，返回子节点索引。
    fn get_or_create_child(&mut self, parent: usize, c: char) -> usize {
        match self.nodes[parent]
            .children
            .binary_search_by_key(&c, |&(ch, _)| ch)
        {
            Ok(pos) => self.nodes[parent].children[pos].1,
            Err(pos) => {
                let new_idx = self.nodes.len();
                self.nodes.push(BNode {
                    is_word: false,
                    children: Vec::new(),
                });
                self.nodes[parent].children.insert(pos, (c, new_idx));
                new_idx
            }
        }
    }

    /// 去重后的真实词数。
    pub(crate) fn word_count(&self) -> u32 {
        self.word_count
    }

    /// Builder → DAG（后缀共享去重）→ serialize，返回 dawg 字节。
    pub(crate) fn finish(self) -> Vec<u8> {
        let (dag, root_idx) = build_dag(&self.nodes);
        serialize(&dag, root_idx, self.word_count)
    }
}

// ---------------- DAWG 转换（后缀共享）----------------

#[derive(Clone)]
struct DagNode {
    is_word: bool,
    // 子节点（按 char 升序），存 (char, dag_index)
    children: Vec<(char, u32)>,
}

fn build_dag(bnodes: &[BNode]) -> (Vec<DagNode>, u32) {
    let mut dag: Vec<DagNode> = Vec::new();
    // 用「实际内容」做去重键，避免哈希碰撞导致错误共享节点（旧 build.rs 已踩过的坑）。
    let mut memo: ConvMemo = std::collections::HashMap::new();

    fn conv(bnodes: &[BNode], idx: usize, memo: &mut ConvMemo, dag: &mut Vec<DagNode>) -> u32 {
        // 子表在构建期已按 char 升序（二分插入保证），此处直接按序转换；
        // 同构子树的子表序列一致 → memo 键一致 → 后缀共享正确。
        let mut child_keys: Vec<(char, u32)> = Vec::with_capacity(bnodes[idx].children.len());
        for &(c, child_bidx) in &bnodes[idx].children {
            let dag_child = conv(bnodes, child_bidx, memo, dag);
            child_keys.push((c, dag_child));
        }
        let key = (bnodes[idx].is_word, child_keys);
        if let Some(&existing) = memo.get(&key) {
            return existing;
        }
        let new_idx = dag.len() as u32;
        dag.push(DagNode {
            is_word: key.0,
            children: key.1.clone(),
        });
        memo.insert(key, new_idx);
        new_idx
    }

    // 注意：conv 是后序遍历，根节点是最后被 push 的，其索引 ≠ 0，
    // 因此必须返回根的真实索引，序列化时写入头部（旧 build.rs 已踩过的坑）。
    let root_idx = conv(bnodes, 0, &mut memo, &mut dag);
    (dag, root_idx)
}

// ---------------- 序列化（扁平、零拷贝可读取）----------------

fn serialize(dag: &[DagNode], root_idx: u32, word_count: u32) -> Vec<u8> {
    let mut edges: Vec<(char, u32)> = Vec::new();
    let mut nodes: Vec<(bool, u32, u32)> = Vec::with_capacity(dag.len());
    for n in dag {
        let child_start = edges.len() as u32;
        let child_count = n.children.len() as u32;
        for &(c, target) in &n.children {
            edges.push((c, target));
        }
        nodes.push((n.is_word, child_count, child_start));
    }

    let mut buf = Vec::new();
    buf.extend_from_slice(MAGIC);
    buf.extend_from_slice(&VERSION.to_le_bytes());
    buf.extend_from_slice(&(nodes.len() as u32).to_le_bytes());
    buf.extend_from_slice(&(edges.len() as u32).to_le_bytes());
    buf.extend_from_slice(&root_idx.to_le_bytes());
    buf.extend_from_slice(&word_count.to_le_bytes());
    for (is_word, child_count, child_start) in &nodes {
        buf.push(*is_word as u8);
        buf.extend_from_slice(&child_count.to_le_bytes());
        buf.extend_from_slice(&child_start.to_le_bytes());
    }
    for (c, target) in &edges {
        buf.extend_from_slice(&(*c as u32).to_le_bytes());
        buf.extend_from_slice(&target.to_le_bytes());
    }
    buf
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dic::compact_trie::CompactTrie;
    use std::sync::Arc;

    /// 用 CompactTrie 读回 DawgBuilder 产物做往返验证。
    fn roundtrip(words: &[&str]) -> (CompactTrie, u32) {
        let mut b = DawgBuilder::new();
        for w in words {
            b.insert(&w.chars().collect::<Vec<_>>());
        }
        let count = b.word_count();
        let bytes: Arc<[u8]> = Arc::from(b.finish());
        (CompactTrie::from_bytes(bytes), count)
    }

    fn is_match(ct: &CompactTrie, w: &str) -> bool {
        ct.match_text(&w.chars().collect::<Vec<_>>(), 0).is_match()
    }

    #[test]
    fn test_roundtrip_words_queryable() {
        let (ct, count) = roundtrip(&["中华人民共和国", "中国", "中", "中国人民", "AB", "ab", "a"]);
        // 全部成员词可查
        for w in ["中华人民共和国", "中国", "中", "中国人民", "AB", "ab", "a"] {
            assert!(is_match(&ct, w), "词 {w} 应可查到");
        }
        // 非成员词不命中
        for w in ["美", "中美", "ABC", "b", "中中人"] {
            assert!(!is_match(&ct, w), "非词 {w} 不应命中");
        }
        // 去重词数
        assert_eq!(count, 7);
        assert_eq!(ct.count_words(), 7);
    }

    #[test]
    fn test_duplicate_insert_counted_once() {
        let (ct, count) = roundtrip(&["测试", "测试", "测试", "中国", "中国"]);
        assert_eq!(count, 2, "重复词只计一次");
        assert_eq!(ct.count_words(), 2);
        assert!(is_match(&ct, "测试"));
    }

    #[test]
    fn test_prefix_semantics() {
        let (ct, _) = roundtrip(&["中国", "中国人民"]);
        // "中国" 是词，且是 "中国人民" 的前缀 → match + prefix
        let hit = ct.match_text(&"中国".chars().collect::<Vec<_>>(), 0);
        assert!(hit.is_match());
        assert!(hit.is_prefix(), "'中国' 是更长词的前缀，应置 prefix");
        // "中国人" 不是词，但是前缀
        let hit2 = ct.match_text(&"中国人".chars().collect::<Vec<_>>(), 0);
        assert!(!hit2.is_match());
        assert!(hit2.is_prefix());
        // "中美" 既非词也非前缀
        let hit3 = ct.match_text(&"中美".chars().collect::<Vec<_>>(), 0);
        assert!(!hit3.is_match());
        assert!(!hit3.is_prefix());
    }

    /// 后缀共享正确性：共享后缀的词必须互不干扰。
    #[test]
    fn test_suffix_sharing_no_interference() {
        let (ct, count) = roundtrip(&["人民", "公民", "居民", "中华人民"]);
        assert_eq!(count, 4);
        for w in ["人民", "公民", "居民", "中华人民"] {
            assert!(is_match(&ct, w), "词 {w} 应可查到");
        }
        assert!(!is_match(&ct, "华人"));
    }

    /// 空词不插入、不计数。
    #[test]
    fn test_empty_word_ignored() {
        let mut b = DawgBuilder::new();
        b.insert(&[]);
        b.insert(&"中国".chars().collect::<Vec<_>>());
        assert_eq!(b.word_count, 1);
    }

    /// 规模锚点（来自旧 build.rs 预编译产物 dict_compiled/main.dawg 的实测值，
    /// 该工件已随方案 C' 删除，锚点作为回归常量保留）：同样的默认主词典
    /// 应得到同样的 DAG 规模（275,717 词 / 74,043 节点 / 302,892 边——见方案 §6 注）。
    #[test]
    fn test_default_main_dict_scale_matches_precompiled() {
        let mut b = DawgBuilder::new();
        for text in [
            include_str!("../../dict/main2012.dic"),
            include_str!("../../dict/ext_dict/ext.dic"),
        ] {
            for line in text.lines() {
                let w = line.trim();
                if !w.is_empty() {
                    b.insert(&w.chars().collect::<Vec<_>>());
                }
            }
        }
        assert_eq!(b.word_count, 275_717, "去重词数应与预编译 dawg 一致");
        let bytes = b.finish();
        // 头部：node_count/edge_count 应与 dict_compiled/main.dawg 的锚点一致
        let node_count = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
        let edge_count = u32::from_le_bytes(bytes[12..16].try_into().unwrap());
        assert_eq!(
            node_count, 74_043,
            "DAG 节点数应与预编译 dawg 一致（后缀共享等价）"
        );
        assert_eq!(edge_count, 302_892, "边数应与预编译 dawg 一致");
    }
}
