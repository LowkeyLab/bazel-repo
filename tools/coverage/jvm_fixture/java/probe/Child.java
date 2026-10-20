package probe;

public final class Child {
  public static void main(String[] args) {
    boolean take = args.length == 1;
    if (take) {
      System.out.println("CHILD_ONLY_71");
    } else {
      throw new AssertionError("Unexpected child branch");
    }
  }
}
