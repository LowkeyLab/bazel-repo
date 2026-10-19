package adapter;

import java.io.ByteArrayInputStream;
import java.io.File;
import java.io.InputStream;
import java.lang.reflect.Constructor;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Arrays;

/** Temporary metadata-only entry point. The input jars contain original class bytes. */
public final class Baseline {
  public static void main(String[] args) throws Exception {
    Class<?> runner = Class.forName("com.google.testing.coverage.JacocoCoverageRunner");
    Constructor<?> constructor =
        runner.getConstructor(InputStream.class, String.class, File[].class);
    byte[] execution = args[1].equals("empty") ? new byte[0] : Files.readAllBytes(Path.of(args[1]));
    File[] metadata = Arrays.stream(args).skip(2).map(File::new).toArray(File[]::new);
    for (File jar : metadata) {
      Object analyzer =
          constructor.newInstance(new ByteArrayInputStream(execution), args[0], new File[] {jar});
      runner.getMethod("create").invoke(analyzer);
    }
  }
}
