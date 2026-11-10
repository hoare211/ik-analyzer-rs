# 贡献指南

感谢你考虑为 ik-analyzer-rs 贡献代码。本文说明开发环境、测试要求与提交规范。

## 开发环境

- Rust **1.86+**（`edition = "2024"`；MSRV 由 `rust-version` 与 CI 的 `msrv` job 校验）
- Python 3.8+ 与 [maturin](https://www.maturin.rs/)（仅在改动 Python 绑定时需要）
- JDK 21（仅在复算 `testing/ab/` 的 Java A/B 对拍时需要）

## 构建与测试

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
cargo test --workspace --no-default-features --lib   # judge_reference 回落路径
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps
cargo package --list -p ik-analyzer-rs               # 确认 dict/ 与 testing/parity/ 在包内
```

Python 绑定：

```bash
cd python
maturin develop --release
python example/test.py
```

## 代码约定

- 保持**零 `unsafe`**。
- 热路径避免逐词元堆分配；新增 API 请说明是否需要对应的零拷贝路径（`segment_spans`）。
- 公共 API 必须写 rustdoc，并在适用时标注 `# Errors` / `# Panics`。
- 内部实现保持 `pub(crate)`，只经 `src/lib.rs` 的精选 `pub use` 暴露。
- 中文注释可以，但术语与标识符保持英文。

## 行为变更的额外要求

本项目的**默认输出是独立的产品决策，不要求与 u6 一致**；但 `Config::u6_parity`
是一条对外承诺的显式契约：显式启用该预设后，同有效配置必须同输出。
若改动会改变分词结果：

1. 在 PR 中说明属于**缺陷修复**还是**可配置改进**；
2. 缺陷修复需附最小复现与 u6 侧对照输出；
3. 若影响长文或换页路径，运行 `testing/parity/regen_goldens.sh` 更新 golden，并在 PR 中单独列出 golden diff；
4. 在 `CHANGELOG.md` 对应小节记录。

## 提交与 PR

- 提交信息建议使用 Conventional Commits：`feat:` / `fix:` / `docs:` / `test:` / `refactor:` / `chore:`。
- 一个 PR 只做一件事；格式化与重构请与行为变更分开。
- PR 描述使用仓库模板。
- CI 必须全绿（fmt / clippy / test / msrv / doc / package / versions-consistent）。

## 不要提交的内容

- `target/`、`python/*.so`、虚拟环境与编辑器配置；
- 本地绝对路径；
- 与改动无关的词典或语料格式化。

## 许可

贡献即表示你同意以 [Apache-2.0](LICENSE) 许可发布你的贡献。
