package probe;

import java.nio.file.Files;
import java.nio.file.Path;

public final class FixtureTest {
  public static void main(String[] args) throws Exception {
    if (Executed.select(true) != 17) throw new AssertionError();
    if (kprobe.ExecutedKotlinKt.selectKotlin(true) != 22) throw new AssertionError();
    if (mismatch.DirectoryMismatch.value() != 31) throw new AssertionError();
    if (kodd.KotlinMismatchKt.mismatchValue() != 59) throw new AssertionError();
    if (shared.TwinKt.firstTwin() != 61) throw new AssertionError();
    if (shared.SecondTwin.secondTwin() != 67) throw new AssertionError();
    Path output = Path.of(System.getenv("TEST_UNDECLARED_OUTPUTS_DIR"));
    ProcessBuilder child = new ProcessBuilder(args[0], "take");
    child.environment().put("JAVA_COVERAGE_FILE", output.resolve("child.dat").toString());
    child.redirectErrorStream(true);
    child.redirectOutput(output.resolve("child.stdout").toFile());
    Process process = child.start();
    if (process.waitFor() != 0) throw new AssertionError("Child failed");
    if (!Files.readString(output.resolve("child.stdout")).contains("CHILD_ONLY_71")) {
      throw new AssertionError("Child did not complete");
    }
    Files.writeString(output.resolve("child.completed"), "exit=0\n");
    System.out.println("FUNCTIONAL_TEST_PASS");
  }
}
