// 导入我们库的公共 API
use ik_analyzer::{Config, TokenizerFactory};
use std::env;
use std::path::PathBuf;

fn main() {
    // 初始化一个简单的日志记录器，以便能看到 Dictionary 加载报告。
    // `env_logger` 现为 optional feature（评审 P1-7），需 `--features env_logger` 启用；
    // 未启用时退化为无后端，`log` 调用仍可编译但无输出。
    #[cfg(feature = "env_logger")]
    env_logger::init();

    // --- 解析命令行参数 ---
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!(
            "Usage: cargo run -- \"<text>\" [--no-smart] [--lowercase true|false] [--custom-dict path/to/dict.txt]"
        );
        eprintln!(
            "\n默认即 v1.0.0 / IK 2012_u6 语义：smart 模式 + 大小写不敏感（ASCII 归一）+ 全角→半角。"
        );
        eprintln!("\nExamples:");
        eprintln!("  cargo run -- \"这是一个中文分词的例子\"");
        eprintln!("  cargo run -- \"研究生命起源\" --no-smart");
        eprintln!(
            "  cargo run -- \"HelloWorld 中文\" --lowercase false   # 保留原文大小写（严格大小写敏感）"
        );
        eprintln!("  cargo run -- \"测试自定义词\" --custom-dict custom/long_word_dict.dic");
        return;
    }

    let text_to_segment = &args[1];
    let use_smart = !args.contains(&"--no-smart".to_string());
    let enable_lowercase = match args.iter().position(|r| r == "--lowercase") {
        Some(idx) => match args.get(idx + 1).map(String::as_str) {
            Some("true") => true,
            Some("false") => false,
            _ => {
                eprintln!("Error: --lowercase requires `true` or `false`.");
                return;
            }
        },
        None => true, // v1.0.0 / 2012_u6 默认：小写化（大小写不敏感）
    };

    // --- 创建 TokenizerFactory ---
    // 这是核心的修改：我们先根据所有参数构建好 Config，然后只创建一次 Factory。
    let factory = {
        let mut config_builder = Config::builder()
            .use_smart(use_smart)
            .enable_lowercase(enable_lowercase);

        // 检查是否有自定义词典参数
        if let Some(dict_path_index) = args.iter().position(|r| r == "--custom-dict") {
            if let Some(path) = args.get(dict_path_index + 1) {
                println!(
                    "\n--- Running with Custom Dictionary (smart={}, lowercase={}) ---",
                    use_smart, enable_lowercase
                );
                config_builder = config_builder.add_ext_dict_paths(vec![PathBuf::from(path)]);
            } else {
                eprintln!("Error: --custom-dict flag requires a path.");
                return;
            }
        } else {
            println!(
                "\n--- Running in Default Mode (smart={}, lowercase={}) ---",
                use_smart, enable_lowercase
            );
        }

        // 使用最终的配置构建工厂
        // 词典加载的报告会在这里被打印出来
        TokenizerFactory::new(config_builder.build())
    };

    // --- 使用工厂进行分词 ---
    // 无论上面是哪种配置，我们都使用同一个 `factory` 实例
    let segmenter = factory.segmenter(text_to_segment);
    let results: Vec<String> = segmenter.map(|l| l.text().to_string()).collect();

    println!("\nInput: '{}'", text_to_segment);
    println!("Result: {}", results.join("/"));

    // 演示工厂的复用性
    println!("\n--- Reusing the factory for another text ---");
    let another_text = "第二次调用不会再加载词典";
    let results2: Vec<String> = factory
        .segmenter(another_text)
        .map(|l| l.text().to_string())
        .collect();
    println!("Input: '{}'", another_text);
    println!("Result: {}", results2.join("/"));
}
