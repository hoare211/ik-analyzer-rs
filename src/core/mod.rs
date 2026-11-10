// 内部实现模块。全部收敛为私有，对外只暴露下面精选重导出的类型，
// 与 `dic/mod.rs` 的写法保持一致（评审 P4-5）。
mod analyze_context;
mod char_source;
mod character_util;
mod cjk_segmenter;
mod cn_quantifier_segmenter;
mod ik_arbitrator;
mod ik_segmenter;
mod isegmenter;
mod letter_segmenter;
mod lexeme;
mod lexeme_path;
mod quick_sort_set;

// 稳定公共 API 的重导出点（`lib.rs` 再统一 `pub use core::…`）。
pub use char_source::{CharSource, ReadCharSource, StrCharSource};
pub use ik_segmenter::{IKSegmenter, SpanSegmenter};
pub use lexeme::{Lexeme, Span, char_to_byte_map};
