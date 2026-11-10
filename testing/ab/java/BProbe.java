import org.wltea.analyzer.cfg.Configuration;
import org.wltea.analyzer.dic.Dictionary;
import org.wltea.analyzer.core.IKSegmenter;
import org.wltea.analyzer.core.Lexeme;

import java.io.StringReader;
import java.nio.file.Path;
import java.nio.file.Paths;

/** B(analysis-ik) 侧探针：从 stdin 读行，逐行输出 lexeme 文本/类型/起止偏移。 */
public class BProbe {

    static class ProbeConfig extends Configuration {
        private final Path confDir;

        ProbeConfig(Path confDir, boolean useSmart, boolean enableLowercase) {
            this.confDir = confDir;
            this.useSmart = useSmart;
            this.enableLowercase = enableLowercase;
            this.enableRemoteDict = false;
        }

        @Override
        public Path getConfDir() {
            return confDir;
        }

        @Override
        public Path getConfigInPluginDir() {
            return confDir;
        }

        @Override
        public Path getPath(String first, String... more) {
            return Paths.get(first, more);
        }
    }

    public static void main(String[] args) throws Exception {
        boolean useSmart = "smart".equals(args[0]);
        boolean lower = !"nolower".equals(args[1]);
        Path confDir = Paths.get(args[2]);

        Dictionary.initial(new ProbeConfig(confDir, useSmart, lower));

        java.io.BufferedReader in = new java.io.BufferedReader(
                new java.io.InputStreamReader(System.in, "UTF-8"));
        String line;
        int n = 0;
        StringBuilder out = new StringBuilder();
        while ((line = in.readLine()) != null) {
            out.append("#").append(n++).append("|");
            IKSegmenter seg = new IKSegmenter(new StringReader(line), new ProbeConfig(confDir, useSmart, lower));
            Lexeme l;
            boolean first = true;
            while ((l = seg.next()) != null) {
                if (!first) out.append(" ");
                first = false;
                out.append(l.getLexemeText())
                   .append("[").append(l.getLexemeTypeString()).append("]")
                   .append("(").append(l.getBeginPosition()).append("-")
                   .append(l.getEndPosition()).append(")");
            }
            out.append("\n");
        }
        System.out.print(out);
        System.out.flush();
    }
}