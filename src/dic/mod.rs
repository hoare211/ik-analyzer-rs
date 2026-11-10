// 内部模块（不再对外整体暴露，避免把 CompactTrie/DawgBuilder 等纯内部结构
// 摊到公共 API 与 docs.rs，后续重命名不再是 breaking change）。
mod compact_trie;
mod dawg_builder;
mod dictionary;
mod hit;
mod persistent_hit;

// 仅导出稳定公共类型。
// `Dictionary` 是公开入口；其 `match_*` 方法返回 `Hit`，故 `Hit` 必须公开。
// `PersistentHit` 仅在 crate 内部由子分词器使用，收敛为 `pub(crate)`。
pub use dictionary::Dictionary;
pub use hit::Hit;
pub(crate) use persistent_hit::PersistentHit;
