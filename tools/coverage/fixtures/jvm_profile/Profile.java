package coverage.fixture;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import org.jacoco.core.analysis.Analyzer;
import org.jacoco.core.analysis.CoverageBuilder;
import org.jacoco.core.data.ExecutionData;
import org.jacoco.core.data.ExecutionDataStore;
import org.jacoco.core.data.ExecutionDataWriter;
import org.jacoco.core.data.SessionInfo;

/** Write standard profile fixtures for collector contract checks. */
public final class Profile {
  public static void main(String[] args) throws IOException {
    String kind = args.length > 1 ? args[1] : "unrelated";
    try (var stream = Files.newOutputStream(Path.of(args[0]))) {
      var writer = new ExecutionDataWriter(stream);
      if (!kind.equals("header")) {
        writer.visitSessionInfo(new SessionInfo("collector-regression", 0, 0));
      }
      if (kind.equals("unrelated")) {
        writer.visitClassExecution(new ExecutionData(42L, "unrelated/Other", new boolean[] {true}));
      } else if (kind.equals("marker")) {
        var coverage = new CoverageBuilder();
        var analyzer = new Analyzer(new ExecutionDataStore(), coverage);
        var original = Profile.class.getResourceAsStream("Marker.class.uninstrumented");
        try (var marker =
            original == null ? Profile.class.getResourceAsStream("Marker.class") : original) {
          analyzer.analyzeClass(marker, "Marker.class");
        }
        for (var marker : coverage.getClasses()) {
          writer.visitClassExecution(
              new ExecutionData(marker.getId(), marker.getName(), new boolean[] {true}));
        }
      }
    }
  }

  private Profile() {}
}
