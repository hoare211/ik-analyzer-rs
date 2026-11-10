## 变更说明

<!-- 这个 PR 做了什么、为什么需要 -->

## 关联 Issue

<!-- Closes #123 -->

## 变更类型

- [ ] 缺陷修复（不改变既有正确行为）
- [ ] 新功能
- [ ] 行为变更（会改变分词结果，需说明是否可配置回退）
- [ ] 重构 / 性能优化（不改变输出）
- [ ] 文档 / 测试

## 验证

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test --workspace --all-targets`
- [ ] `cargo test --workspace --no-default-features --lib`
- [ ] Python 绑定改动已跑 `cd python && maturin develop --release && python example/test.py`
- [ ] 行为变更已更新 `testing/parity` golden 并在下方列出 diff 摘要

## 兼容性

<!-- 是否影响公共 API / 默认输出 / MSRV；若无影响请写「无」 -->
