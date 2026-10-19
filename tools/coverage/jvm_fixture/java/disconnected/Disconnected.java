package disconnected;

public final class Disconnected {
  static {
    if (System.nanoTime() != 0) throw new AssertionError("Baseline executed initializer");
  }

  public static int untouched(boolean take) {
    if (take) return 41;
    return 43;
  }
}
