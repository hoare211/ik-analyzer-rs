import java.io.*;
public class DecodeProbe {
    public static void main(String[] a) throws Exception {
        byte[] b = {(byte)0xE4,(byte)0xB8,(byte)0xAD,(byte)0xED,(byte)0xA0,(byte)0x80,(byte)0xE6,(byte)0xB5};
        Reader r = new InputStreamReader(new ByteArrayInputStream(b), "UTF-8");
        int c, n = 0;
        StringBuilder sb = new StringBuilder();
        while ((c = r.read()) != -1) { n++; sb.append(String.format("U+%04X ", c)); }
        System.out.println("读出 code unit 数 = " + n + " : " + sb);
    }
}
