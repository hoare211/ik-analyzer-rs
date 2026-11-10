import org.wltea.analyzer.core.IKSegmenter;
import org.wltea.analyzer.core.Lexeme;

import java.io.StringReader;
import java.util.List;

/** A(u6) 侧探针：从 stdin 读行，逐行输出 lexeme 文本/类型/起止偏移。 */
public class AProbe {
    public static void main(String[] args) throws Exception {
        boolean useSmart = "smart".equals(args[0]);
        java.io.BufferedReader in = new java.io.BufferedReader(
                new java.io.InputStreamReader(System.in, "UTF-8"));
        String line;
        int n = 0;
        StringBuilder out = new StringBuilder();
        while ((line = in.readLine()) != null) {
            out.append("#").append(n++).append("|");
            IKSegmenter seg = new IKSegmenter(new StringReader(line), useSmart);
            Lexeme l;
            boolean first = true;
            while ((l = seg.next()) != null) {
                if (!first) out.append(" ");
                first = false;
                out.append(l.getLexemeText())
                   .append("[").append(typeName(l.getLexemeType())).append("]")
                   .append("(").append(l.getBeginPosition()).append("-")
                   .append(l.getEndPosition()).append(")");
            }
            out.append("\n");
        }
        System.out.print(out);
        System.out.flush();
    }

    static String typeName(int t) {
        switch (t) {
            case Lexeme.TYPE_UNKNOWN: return "UNKNOWN";
            case Lexeme.TYPE_ENGLISH: return "ENGLISH";
            case Lexeme.TYPE_ARABIC: return "ARABIC";
            case Lexeme.TYPE_LETTER: return "LETTER";
            case Lexeme.TYPE_CNWORD: return "CNWORD";
            case Lexeme.TYPE_CNCHAR: return "CNCHAR";
            case Lexeme.TYPE_OTHER_CJK: return "OTHER_CJK";
            case Lexeme.TYPE_COUNT: return "COUNT";
            case Lexeme.TYPE_CNUM: return "CNUM";
            case Lexeme.TYPE_CQUAN: return "CQUAN";
            default: return "T" + t;
        }
    }
}