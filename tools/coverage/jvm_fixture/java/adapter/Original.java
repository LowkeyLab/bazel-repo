package adapter;

import java.io.File;
import java.lang.reflect.Method;
import java.util.Collection;
import java.util.zip.ZipFile;

/** Temporary probe for the standard JaCoCo analysis API. */
public final class Original {
  public static void main(String[] args) throws Exception {
    Class<?> loaderType = Class.forName("org.jacoco.core.tools.ExecFileLoader");
    Object loader = loaderType.getConstructor().newInstance();
    if (!args[0].equals("empty")) {
      loaderType.getMethod("load", File.class).invoke(loader, new File(args[0]));
    }
    Class<?> storeType = Class.forName("org.jacoco.core.data.ExecutionDataStore");
    Object store = loaderType.getMethod("getExecutionDataStore").invoke(loader);
    Class<?> builderType = Class.forName("org.jacoco.core.analysis.CoverageBuilder");
    Object builder = builderType.getConstructor().newInstance();
    Class<?> analyzerType = Class.forName("org.jacoco.core.analysis.Analyzer");
    Object analyzer =
        analyzerType
            .getConstructor(storeType, Class.forName("org.jacoco.core.analysis.ICoverageVisitor"))
            .newInstance(store, builder);
    Method analyze = analyzerType.getMethod("analyzeClass", byte[].class, String.class);
    for (int i = 1; i < args.length; i++) {
      try (ZipFile jar = new ZipFile(args[i])) {
        var entries = jar.entries();
        while (entries.hasMoreElements()) {
          var entry = entries.nextElement();
          if (entry.getName().endsWith(".class.uninstrumented")) {
            try (var input = jar.getInputStream(entry)) {
              analyze.invoke(analyzer, input.readAllBytes(), entry.getName());
            }
          }
        }
      }
    }
    Class<?> classType = Class.forName("org.jacoco.core.analysis.IClassCoverage");
    Class<?> nodeType = Class.forName("org.jacoco.core.analysis.ISourceNode");
    Class<?> coverageType = Class.forName("org.jacoco.core.analysis.ICoverageNode");
    Class<?> lineType = Class.forName("org.jacoco.core.analysis.ILine");
    Class<?> counterType = Class.forName("org.jacoco.core.analysis.ICounter");
    for (Object subject : (Collection<?>) builderType.getMethod("getClasses").invoke(builder)) {
      String name = (String) coverageType.getMethod("getName").invoke(subject);
      String source = (String) classType.getMethod("getSourceFileName").invoke(subject);
      System.out.println("CLASS\t" + name + "\t" + source);
      int first = (int) nodeType.getMethod("getFirstLine").invoke(subject);
      int last = (int) nodeType.getMethod("getLastLine").invoke(subject);
      for (int line = first; line <= last && line > 0; line++) {
        Object record = nodeType.getMethod("getLine", int.class).invoke(subject, line);
        Object instructions = lineType.getMethod("getInstructionCounter").invoke(record);
        int total = (int) counterType.getMethod("getTotalCount").invoke(instructions);
        int hits = (int) counterType.getMethod("getCoveredCount").invoke(instructions);
        if (total > 0) {
          Object branches = lineType.getMethod("getBranchCounter").invoke(record);
          int branchTotal = (int) counterType.getMethod("getTotalCount").invoke(branches);
          int branchHits = (int) counterType.getMethod("getCoveredCount").invoke(branches);
          System.out.println(
              "LINE\t"
                  + name
                  + "\t"
                  + line
                  + "\t"
                  + hits
                  + "\t"
                  + total
                  + "\t"
                  + branchHits
                  + "\t"
                  + branchTotal);
        }
      }
    }
  }
}
