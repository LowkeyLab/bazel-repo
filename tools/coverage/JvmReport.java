package coverage;

import java.io.File;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.HashSet;
import java.util.List;
import java.util.Set;
import java.util.jar.JarFile;
import org.jacoco.core.analysis.Analyzer;
import org.jacoco.core.analysis.CoverageBuilder;
import org.jacoco.core.tools.ExecFileLoader;
import org.jacoco.report.xml.XMLFormatter;

/** Analyze configured class files with the standard JaCoCo coverage model. */
public final class JvmReport {
  public static void main(String[] args) throws IOException {
    Path output = Path.of(args[0]);
    ExecFileLoader execution = new ExecFileLoader();
    List<File> jars = new ArrayList<>();
    for (int i = 1; i < args.length; i += 2) {
      if (args[i].equals("--classfiles")) {
        jars.add(new File(args[i + 1]));
      } else if (args[i].equals("--exec")) {
        File profile = new File(args[i + 1]);
        if (profile.length() == 0) {
          throw new IOException("Empty or missing required JaCoCo profile: " + profile);
        }
        execution.load(profile);
      } else {
        throw new IOException("Unknown JaCoCo report option: " + args[i]);
      }
    }
    if (jars.isEmpty()) {
      throw new IOException("Required JaCoCo class inputs are absent.");
    }
    CoverageBuilder coverage = new CoverageBuilder();
    Analyzer analyzer = new Analyzer(execution.getExecutionDataStore(), coverage);
    for (File input : jars) {
      try (JarFile jar = new JarFile(input)) {
        // Bazel preserves original classes beside offline-instrumented classes.
        // Analyze each original class once, without changing its bytecode or name.
        Set<String> originals = new HashSet<>();
        var entries = jar.entries();
        while (entries.hasMoreElements()) {
          var entry = entries.nextElement();
          if (entry.getName().endsWith(".class.uninstrumented")) {
            originals.add(entry.getName().replaceFirst("\\.uninstrumented$", ""));
            try (var stream = jar.getInputStream(entry)) {
              analyzer.analyzeClass(stream, input + "!" + entry.getName());
            }
          }
        }
        entries = jar.entries();
        while (entries.hasMoreElements()) {
          var entry = entries.nextElement();
          if (entry.getName().endsWith(".class") && !originals.contains(entry.getName())) {
            try (var stream = jar.getInputStream(entry)) {
              analyzer.analyzeClass(stream, input + "!" + entry.getName());
            }
          }
        }
      }
    }
    if (!coverage.getNoMatchClasses().isEmpty()) {
      throw new IOException("JaCoCo execution data does not match configured class files.");
    }
    try (var stream = Files.newOutputStream(output)) {
      var report = new XMLFormatter().createVisitor(stream);
      report.visitInfo(
          execution.getSessionInfoStore().getInfos(),
          execution.getExecutionDataStore().getContents());
      report.visitBundle(coverage.getBundle("configured JVM classes"), null);
      report.visitEnd();
    }
  }

  private JvmReport() {}
}
