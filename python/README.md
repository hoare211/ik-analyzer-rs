# ik-analyzer-py

基于 [IK Analyzer](https://code.google.com/p/ik-analyzer/) 算法血统的**独立 Rust 中文分词库**的
Python 绑定：零 JVM 依赖、线程安全，与 Rust crate
[`ik-analyzer-rs`](https://crates.io/crates/ik-analyzer-rs) 同源。

它不是 IK Analyzer 2012_u6 的逐字移植：实现以 analysis-ik 为基准，默认输出是独立的产品决策
（可优于原版）；只有显式调用 `u6_parity()` 时才切换到 u6 行为兼容口径。

## 安装

```bash
pip install ik-analyzer-py
```

要求 Python 3.8+。轮子为 `abi3` 构建，同一轮子覆盖 3.8 及以上版本。

## 快速开始

```python
import ik_analyzer as ik

tok = ik.Tokenizer()                       # 默认 smart=True
print(tok.segment("欢迎使用本分词器"))
# ['欢迎', '使用', '本', '分词器']

# 细粒度模式
cfg = ik.Config()
cfg.use_smart(False)
print(ik.Tokenizer(cfg).segment("张三说的确实在理"))
```

## 常用配置

```python
import ik_analyzer as ik

cfg = ik.Config()
cfg.use_smart(True)                        # 歧义消解（默认）/ False 为细粒度
cfg.enable_lowercase(True)                 # 大小写不敏感
cfg.astral_mode(ik.AstralMode.Ideographs)  # 补充平面：Drop / Ideographs / Tokens
cfg.add_ext_dict("my_words.dic")           # 追加自定义词典
cfg.set_stop_words("stopwords.dic")        # 替换停用词表
```

## 可选的 u6 行为兼容

需要与 IK Analyzer 2012_u6 完全一致的下游，可显式切换到冻结的 parity 口径：

```python
tok = ik.Tokenizer.u6_parity(use_smart=True)
```

`u6_parity` 钉住 u6 分发词典 + 无停用词 + 大小写不敏感 + 补充平面字符丢弃，
可差分验证。**默认不启用**：默认配置是独立的产品决策，可优于原版，与 u6 的差异
全部可配置或已披露，详见 [Rust 仓库 README](https://github.com/hoare211/ik-analyzer-rs/blob/main/README.md)。

## 批量处理

`segment_batch(texts)` 在批量处理时释放 GIL，适合多线程/多进程并发的服务场景。

## 链接

- 源码与文档：<https://github.com/hoare211/ik-analyzer-rs>
- 完整使用手册：<https://github.com/hoare211/ik-analyzer-rs/blob/main/docs/使用手册.md>
- Rust API 文档：<https://docs.rs/ik-analyzer-rs>
- 问题反馈：<https://github.com/hoare211/ik-analyzer-rs/issues>

## License

Apache-2.0。算法与词典派生自 IK Analyzer 2012_u6（Apache-2.0），
第三方归属声明见仓库根目录的 `NOTICE`。
