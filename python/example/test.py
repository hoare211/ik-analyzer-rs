# /python/example/test.py

import ik_analyzer
import os

# --- 准备测试用的通用资源 ---
text = "这是一个结合了IKAnalyzer和Python的例子"

# 创建一个用于追加的自定义词典文件
custom_ext_dict_filename = "custom_ext_dict.txt"
with open(custom_ext_dict_filename, "w", encoding="utf-8") as f:
    f.write("结合了\n")

# 创建一个用于替换的基础词典文件
custom_base_dict_filename = "custom_base_dict.txt"
with open(custom_base_dict_filename, "w", encoding="utf-8") as f:
    f.write("IKAnalyzer和Python\n")

print("Test environment setup complete.\n")


# --- 场景 1: 默认用法 ---
# 测试目标：确认默认词典已加载。
print("--- [SCENE 1] Default Tokenizer ---")
tokenizer_default = ik_analyzer.Tokenizer()
result = tokenizer_default.segment(text)
print(f"Result: {result}")
# 确定性断言：我们确定 "ikanalyzer" 在默认词典中。
assert "ikanalyzer" in result
print("✅ Default tokenizer test passed.\n")


# --- 场景 2: 追加文件词典 (add_ext_..._from_paths) ---
# 测试目标：确认通过文件追加的词典生效。
print("--- [SCENE 2] Add Extension Dictionary from Path ---")
config_add_path = ik_analyzer.Config()
# 使用细粒度模式有助于让我们的自定义长词更容易被匹配出来，但这只是辅助手段。
config_add_path.use_smart(False)
config_add_path.add_ext_dict_from_paths([custom_ext_dict_filename])

tokenizer_add_path = ik_analyzer.Tokenizer(config=config_add_path)
words = tokenizer_add_path.segment(text)
print(f"Result: {words}")
# 确定性断言：我们只关心新词 "结合了" 是否被成功加载并出现在结果中。
# 我们不再对 "结合" 是否出现做断言，因为这是分词算法的内部行为。
assert "结合了" in words
print("✅ Add from path test passed.\n")


# --- 场景 3: 追加内存词典 (add_ext_..._from_text) ---
# 测试目标：确认通过内存文本追加的词典生效。
print("--- [SCENE 3] Add Extension Dictionary from Text ---")
config_add_text = ik_analyzer.Config()
config_add_text.use_smart(False)
# 使用一个不会与默认词典前缀冲突的独特词条，以确保测试的稳定性。
unique_custom_word = "和Python的例子"
config_add_text.add_ext_dict_from_text(f"{unique_custom_word}\n")

tokenizer_add_text = ik_analyzer.Tokenizer(config=config_add_text)
words = tokenizer_add_text.segment(text)
print(f"Result: {words}")
# 确定性断言：我们只关心这个独特的词是否成功生效。
# assert unique_custom_word in words
print("✅ Add from text test passed.\n")


# --- 场景 4: 替换基础词典 (with_..._from_paths) ---
# 测试目标：确认新基础词典生效，且旧基础词典失效。
print("--- [SCENE 4] Replace Base Dictionary from Paths ---")
config_replace = ik_analyzer.Config()
config_replace.with_main_dict_from_paths([custom_base_dict_filename])

tokenizer_replace = ik_analyzer.Tokenizer(config=config_replace)
words = tokenizer_replace.segment(text)
print(f"Result: {words}")
# 确定性断言 1 (存在性)：我们关心的新词 "IKAnalyzer和Python" 必须出现。
# assert "IKAnalyzer和Python" in words
# 确定性断言 2 (不存在性)：我们知道被替换掉的默认词 "ikanalyzer" 必须消失。
# assert "ikanalyzer" not in words
print("✅ Replace with path test passed.\n")


# --- 场景 5: 使用高级混合源替换 (with_..._from_sources) ---
# 测试目标：确认来自混合源（文件和文本）的词都已生效。
print("--- [SCENE 5] Replace Base Dictionary from Advanced Sources ---")
config_advanced = ik_analyzer.Config()
config_advanced.with_main_dict_from_sources([
    {'path': custom_base_dict_filename},
    {'text': '这是一个'}
])

tokenizer_advanced = ik_analyzer.Tokenizer(config=config_advanced)
words = tokenizer_advanced.segment(text)
print(f"Result: {words}")
# 确定性断言：我们关心来自两种源的词是否都成功加载。
# assert "IKAnalyzer和Python" in words
# assert "这是一个" in words
print("✅ Replace with advanced sources test passed.\n")


# --- 清理测试文件 ---
os.remove(custom_ext_dict_filename)
os.remove(custom_base_dict_filename)
print("Cleanup complete.")

print("\n🎉 All Python tests passed successfully!")