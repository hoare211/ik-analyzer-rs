//! 子分词器接口。
//!
//! # 对齐基准
//!
//! 对应 analysis-ik `core/src/main/java/org/wltea/analyzer/core/ISegmenter.java`
//! （`master@6d2d70f`）——与 u6 同名文件逐字节相同（调研报告 §2.1）。
//! Rust 侧以 trait 化（`Send` 约束）替代 Java 接口。

use crate::core::analyze_context::AnalyzeContext;

/// 分词器接口定义
pub(crate) trait ISegmenter: Send {
    /// 对分词上下文进行分析
    fn analyze(&mut self, context: &mut AnalyzeContext);

    /// 重置分词器状态
    fn reset(&mut self);
}
