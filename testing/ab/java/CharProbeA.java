package org.wltea.analyzer.core;

import java.io.BufferedReader;
import java.io.InputStreamReader;

/**
 * 预处理阶段探针（u6 侧）：
 * 从 stdin 读入十进制 code unit，逐字符输出 identifyCharType 与 regularize 结果。
 * 放在 org.wltea.analyzer.core 包内以访问 package-private 的 CharacterUtil。
 */
public class CharProbeA {
    public static void main(String[] args) throws Exception {
        BufferedReader in = new BufferedReader(
                new InputStreamReader(System.in, "UTF-8"));
        String line;
        StringBuilder out = new StringBuilder();
        while ((line = in.readLine()) != null) {
            line = line.trim();
            if (line.isEmpty()) continue;
            int cp = Integer.parseInt(line);
            char c = (char) cp;
            int type = CharacterUtil.identifyCharType(c);
            char reg = CharacterUtil.regularize(c);
            // 真实流水线顺序：initCursor/moveCursor 先 regularize 再 identifyCharType
            int effType = CharacterUtil.identifyCharType(reg);
            out.append(cp).append('\t')
               .append(type).append('\t')
               .append((int) reg).append('\t')
               .append(typeName(type)).append('\t')
               .append(effType).append('\t')
               .append(typeName(effType)).append('\n');
        }
        System.out.print(out);
    }

    static String typeName(int t) {
        switch (t) {
            case CharacterUtil.CHAR_USELESS: return "USELESS";
            case CharacterUtil.CHAR_ARABIC: return "ARABIC";
            case CharacterUtil.CHAR_ENGLISH: return "ENGLISH";
            case CharacterUtil.CHAR_CHINESE: return "CHINESE";
            case CharacterUtil.CHAR_OTHER_CJK: return "OTHER_CJK";
            default: return "T" + t;
        }
    }
}